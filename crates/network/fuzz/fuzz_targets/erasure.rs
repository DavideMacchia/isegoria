//! Erasure decoding of untrusted shards and layout (T44): no panic on any shard set or
//! claimed layout, and — for a genuine encoding — recovery exactly when at least
//! `data_shards` authentic shards survive, with the original bytes.
#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use network::erasure::{encode, reconstruct, reconstruct_verified, RecoverError};

#[derive(Debug, Arbitrary)]
enum Input {
    /// A genuine encoding, then losses and corruptions.
    Genuine {
        data: Vec<u8>,
        data_shards: u8,
        parity_shards: u8,
        lost: Vec<u8>,
        corrupt: Vec<(u8, u16, u8)>,
    },
    /// Everything attacker-chosen: shards, manifest and layout.
    Hostile {
        shards: Vec<Option<Vec<u8>>>,
        manifest: Vec<[u8; 32]>,
        data_shards: usize,
        parity_shards: usize,
        orig_len: usize,
    },
}

fuzz_target!(|input: Input| match input {
    Input::Genuine {
        data,
        data_shards,
        parity_shards,
        lost,
        corrupt,
    } => {
        let k = usize::from(data_shards % 16) + 1;
        let m = usize::from(parity_shards % 16) + 1;
        let enc = encode(&data, k, m);
        let mut shards: Vec<Option<Vec<u8>>> = enc.shards.iter().cloned().map(Some).collect();
        for i in lost {
            shards[usize::from(i) % (k + m)] = None;
        }
        for (i, at, mask) in corrupt {
            if let Some(shard) = shards[usize::from(i) % (k + m)].as_mut() {
                let at = usize::from(at) % shard.len();
                shard[at] ^= mask;
            }
        }
        // Compare with the original: two flips of one byte cancel out.
        let tampered: Vec<bool> = shards
            .iter()
            .zip(&enc.shards)
            .map(|(s, orig)| s.as_ref().is_some_and(|s| s != orig))
            .collect();
        let authentic = shards
            .iter()
            .zip(&tampered)
            .filter(|(s, t)| s.is_some() && !**t)
            .count();
        let got = reconstruct_verified(shards.clone(), &enc.manifest, k, m, enc.orig_len);
        if authentic >= k {
            assert_eq!(got.as_deref(), Ok(data.as_slice()));
        } else {
            assert_eq!(
                got,
                Err(RecoverError::TooFewAuthenticShards { authentic, need: k })
            );
        }
        if !tampered.contains(&true) {
            let plain = reconstruct(shards, k, m, enc.orig_len);
            assert_eq!(plain.is_some(), authentic >= k);
            if let Some(bytes) = plain {
                assert_eq!(bytes, data);
            }
        }
    }
    Input::Hostile {
        shards,
        manifest,
        data_shards,
        parity_shards,
        orig_len,
    } => {
        let _ = reconstruct_verified(
            shards.clone(),
            &manifest,
            data_shards,
            parity_shards,
            orig_len,
        );
        let _ = reconstruct(shards, data_shards, parity_shards, orig_len);
    }
});
