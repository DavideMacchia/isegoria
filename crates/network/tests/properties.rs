//! Property-based tests over arbitrary inputs (`proptest`): the invariants that must
//! hold for every Merkle tree, every erasure code, and every append-only log.

use network::cid::cid;
use network::erasure::{encode, reconstruct};
use network::log::TransparencyLog;
use network::merkle::{leaf_hash, merkle_proof, merkle_root, verify_proof};
use proptest::prelude::*;

fn leaf_set() -> impl Strategy<Value = Vec<Vec<u8>>> {
    prop::collection::vec(prop::collection::vec(any::<u8>(), 0..8), 1..40)
}

proptest! {
    /// Every leaf has a valid inclusion proof against the root.
    #[test]
    fn merkle_inclusion_always_verifies(leaves in leaf_set()) {
        let hs: Vec<[u8; 32]> = leaves.iter().map(|d| leaf_hash(d)).collect();
        let root = merkle_root(&hs);
        for (i, &h) in hs.iter().enumerate() {
            prop_assert!(verify_proof(h, &merkle_proof(&hs, i), root), "leaf {i}");
        }
    }

    /// Changing any single leaf changes the root.
    #[test]
    fn changing_a_leaf_changes_the_root(
        leaves in leaf_set(),
        idx in any::<prop::sample::Index>(),
        replacement in prop::collection::vec(any::<u8>(), 0..8),
    ) {
        let i = idx.index(leaves.len());
        prop_assume!(leaf_hash(&leaves[i]) != leaf_hash(&replacement));
        let hs: Vec<[u8; 32]> = leaves.iter().map(|d| leaf_hash(d)).collect();
        let mut hs2 = hs.clone();
        hs2[i] = leaf_hash(&replacement);
        prop_assert_ne!(merkle_root(&hs), merkle_root(&hs2));
    }

    /// Reed–Solomon recovers the original bytes from ANY `data_shards` survivors.
    #[test]
    fn erasure_recovers_from_any_k(
        (k, m, erase, data) in (1usize..=8, 1usize..=8).prop_flat_map(|(k, m)| {
            let n = k + m;
            (
                Just(k),
                Just(m),
                proptest::sample::subsequence((0..n).collect::<Vec<usize>>(), m..=m),
                prop::collection::vec(any::<u8>(), 1..300),
            )
        })
    ) {
        let enc = encode(&data, k, m);
        let mut shards: Vec<Option<Vec<u8>>> = enc.shards.iter().cloned().map(Some).collect();
        for e in erase {
            shards[e] = None;
        }
        let recovered = reconstruct(shards, k, m, enc.orig_len);
        prop_assert_eq!(recovered.as_deref(), Some(data.as_slice()));
    }

    /// Any sequence of appends verifies, and the head advances on every append.
    #[test]
    fn log_verifies_and_head_advances(
        payloads in prop::collection::vec(prop::collection::vec(any::<u8>(), 0..8), 0..30)
    ) {
        let mut log = TransparencyLog::new();
        let mut prev = log.head();
        for p in &payloads {
            log.append(cid(p));
            let h = log.head();
            prop_assert_ne!(h, prev);
            prev = h;
        }
        prop_assert!(log.verify());
        prop_assert_eq!(log.len(), payloads.len());
    }
}
