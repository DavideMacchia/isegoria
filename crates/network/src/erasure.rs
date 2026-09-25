//! Erasure coding (`docs/04` §Durability, real Reed–Solomon via `reed-solomon-erasure`):
//! any `data_shards` of the shards reconstruct the data. [`reconstruct_verified`]
//! authenticates each present shard against `Encoded::manifest` first (NET-007, AT-NET-06).

use crate::hash::tagged;
use reed_solomon_erasure::galois_8::ReedSolomon;

/// Hash committing to one shard's bytes (its manifest entry).
pub fn shard_hash(shard: &[u8]) -> [u8; 32] {
    tagged("isegoria/erasure/shard/v1", &[shard])
}

#[derive(Clone, Debug)]
pub struct Encoded {
    pub shards: Vec<Vec<u8>>,
    pub data_shards: usize,
    pub parity_shards: usize,
    pub orig_len: usize,
    /// Per-shard hash, index-aligned to `shards` (`shard_hash`).
    pub manifest: Vec<[u8; 32]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoverError {
    InvalidLayout,
    TooFewAuthenticShards { authentic: usize, need: usize },
    Decode,
}

/// Encodes `data` into `data_shards` systematic shards plus `parity_shards`. Panics if
/// either is zero or their sum exceeds 256 (GF(2⁸)): an invalid layout is a configuration
/// error, not runtime input.
pub fn encode(data: &[u8], data_shards: usize, parity_shards: usize) -> Encoded {
    let r = ReedSolomon::new(data_shards, parity_shards).expect("valid shard counts");
    let shard_len = data.len().div_ceil(data_shards).max(1);
    let mut shards: Vec<Vec<u8>> = Vec::with_capacity(data_shards + parity_shards);
    for i in 0..data_shards {
        let mut s = vec![0u8; shard_len];
        let start = i * shard_len;
        let end = (start + shard_len).min(data.len());
        if start < data.len() {
            s[..end - start].copy_from_slice(&data[start..end]);
        }
        shards.push(s);
    }
    for _ in 0..parity_shards {
        shards.push(vec![0u8; shard_len]);
    }
    r.encode(&mut shards).expect("encode");
    let manifest = shards.iter().map(|s| shard_hash(s)).collect();
    Encoded {
        shards,
        data_shards,
        parity_shards,
        orig_len: data.len(),
        manifest,
    }
}

/// Reconstructs from surviving shards (`None` = lost); `None` on failure or an invalid layout.
pub fn reconstruct(
    shards: Vec<Option<Vec<u8>>>,
    data_shards: usize,
    parity_shards: usize,
    orig_len: usize,
) -> Option<Vec<u8>> {
    decode(shards, data_shards, parity_shards, orig_len).ok()
}

fn check_layout(
    shards: usize,
    data_shards: usize,
    parity_shards: usize,
) -> Result<(), RecoverError> {
    let total = data_shards
        .checked_add(parity_shards)
        .ok_or(RecoverError::InvalidLayout)?;
    if data_shards == 0 || parity_shards == 0 || total > 256 || shards != total {
        return Err(RecoverError::InvalidLayout);
    }
    Ok(())
}

/// Reed–Solomon decode of an already-authenticated (or trusted) shard set.
fn decode(
    mut shards: Vec<Option<Vec<u8>>>,
    data_shards: usize,
    parity_shards: usize,
    orig_len: usize,
) -> Result<Vec<u8>, RecoverError> {
    check_layout(shards.len(), data_shards, parity_shards)?;
    let r = ReedSolomon::new(data_shards, parity_shards).map_err(|_| RecoverError::Decode)?;
    r.reconstruct(&mut shards)
        .map_err(|_| RecoverError::Decode)?;
    let data: Vec<&Vec<u8>> = shards
        .iter()
        .take(data_shards)
        .map(|s| s.as_ref().ok_or(RecoverError::Decode))
        .collect::<Result<_, _>>()?;
    let capacity = data.iter().map(|s| s.len()).sum::<usize>();
    if orig_len > capacity {
        return Err(RecoverError::InvalidLayout);
    }
    let mut out = Vec::with_capacity(orig_len);
    for shard in data {
        out.extend_from_slice(shard);
    }
    out.truncate(orig_len);
    Ok(out)
}

/// Reconstructs, authenticating every present shard against `manifest` first (NET-007,
/// AT-NET-06): `shards` and `manifest` must be index-aligned to the `data + parity` shards.
pub fn reconstruct_verified(
    shards: Vec<Option<Vec<u8>>>,
    manifest: &[[u8; 32]],
    data_shards: usize,
    parity_shards: usize,
    orig_len: usize,
) -> Result<Vec<u8>, RecoverError> {
    check_layout(shards.len(), data_shards, parity_shards)?;
    let clean: Vec<Option<Vec<u8>>> = shards
        .into_iter()
        .enumerate()
        .map(|(i, s)| match s {
            // Keep a shard only if present AND its bytes match the committed hash.
            Some(bytes) if manifest.get(i) == Some(&shard_hash(&bytes)) => Some(bytes),
            _ => None,
        })
        .collect();
    let authentic = clean.iter().filter(|s| s.is_some()).count();
    if authentic < data_shards {
        return Err(RecoverError::TooFewAuthenticShards {
            authentic,
            need: data_shards,
        });
    }
    decode(clean, data_shards, parity_shards, orig_len)
}
