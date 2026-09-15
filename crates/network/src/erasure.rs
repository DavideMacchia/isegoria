//! Erasure coding (`docs/04`, §Durability). Data is split into shards spread across
//! nodes; any `data_shards` of them reconstruct it. At equal storage it beats
//! replication and, unlike replication, its durability lives on all nodes.
//! Real Reed–Solomon via `reed-solomon-erasure`.

use reed_solomon_erasure::galois_8::ReedSolomon;

#[derive(Clone, Debug)]
pub struct Encoded {
    pub shards: Vec<Vec<u8>>,
    pub data_shards: usize,
    pub parity_shards: usize,
    pub orig_len: usize,
}

/// Encodes `data` into `data_shards` systematic shards plus `parity_shards`.
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
    Encoded {
        shards,
        data_shards,
        parity_shards,
        orig_len: data.len(),
    }
}

/// Reconstructs the original bytes from surviving shards (`None` = lost). Returns
/// `None` if fewer than `data_shards` survive.
pub fn reconstruct(
    mut shards: Vec<Option<Vec<u8>>>,
    data_shards: usize,
    parity_shards: usize,
    orig_len: usize,
) -> Option<Vec<u8>> {
    let r = ReedSolomon::new(data_shards, parity_shards).ok()?;
    r.reconstruct(&mut shards).ok()?;
    let mut out = Vec::with_capacity(orig_len);
    for shard in shards.iter().take(data_shards) {
        out.extend_from_slice(shard.as_ref()?);
    }
    out.truncate(orig_len);
    Some(out)
}
