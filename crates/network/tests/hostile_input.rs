//! No panic on hostile input (T44): the network decoders fed arbitrary bytes, shard sets,
//! layouts, signatures and indices. The `fuzz/` targets explore the same entry points
//! for longer on a nightly toolchain; these properties keep them covered on every push.

use ed25519_dalek::Signature;
use network::anchoring::{Anchor, AnchorState, OtsAnchor, Receipt};
use network::consortium::{
    Checkpoint, CheckpointClient, CheckpointReject, CheckpointUpdate, Consortium, Member,
};
use network::erasure::{encode, reconstruct, reconstruct_verified, RecoverError};
use network::log::TransparencyLog;
use network::merkle::{leaf_hash, merkle_proof, merkle_root, verify_proof, MerkleProof};
use proptest::prelude::*;

const SMALL: &[u8] = include_bytes!("fixtures/ots/pending-two-calendars.ots");
const LARGE: &[u8] = include_bytes!("fixtures/ots/bitcoin-attested.ots");
const OTS_MAGIC: &[u8] = b"\x00OpenTimestamps\x00\x00Proof\x00\xbf\x89\xe2\xe8\x84\xe8\x92\x94";

/// An anchor that has confirmed one block, so Bitcoin attestations can meet a height.
fn anchor() -> OtsAnchor {
    let mut anchor = OtsAnchor::new("https://calendar.example");
    let pending = anchor.submit([0u8; 32]);
    anchor.upgrade(&pending);
    anchor
}

/// The digest a proof claims, used as the receipt root so parsing gets past it.
fn claimed_root(proof: &[u8]) -> [u8; 32] {
    proof
        .get(OTS_MAGIC.len() + 2..OTS_MAGIC.len() + 34)
        .and_then(|d| d.try_into().ok())
        .unwrap_or([0u8; 32])
}

fn verify(proof: Vec<u8>) -> AnchorState {
    anchor().verify(&Receipt {
        root: claimed_root(&proof),
        proof,
    })
}

/// Edits to a genuine proof: overwrite, insert or delete bytes at arbitrary offsets.
#[derive(Clone, Debug)]
enum Edit {
    Set(prop::sample::Index, u8),
    Insert(prop::sample::Index, Vec<u8>),
    Delete(prop::sample::Index, usize),
}

fn edit() -> impl Strategy<Value = Edit> {
    prop_oneof![
        (any::<prop::sample::Index>(), any::<u8>()).prop_map(|(i, b)| Edit::Set(i, b)),
        (
            any::<prop::sample::Index>(),
            prop::collection::vec(any::<u8>(), 1..16)
        )
            .prop_map(|(i, v)| Edit::Insert(i, v)),
        (any::<prop::sample::Index>(), 1usize..64).prop_map(|(i, n)| Edit::Delete(i, n)),
    ]
}

fn apply(mut bytes: Vec<u8>, edits: &[Edit]) -> Vec<u8> {
    for e in edits {
        if bytes.is_empty() {
            break;
        }
        match e {
            Edit::Set(i, b) => {
                let at = i.index(bytes.len());
                bytes[at] = *b;
            }
            Edit::Insert(i, v) => {
                let at = i.index(bytes.len());
                bytes.splice(at..at, v.iter().copied());
            }
            Edit::Delete(i, n) => {
                let at = i.index(bytes.len());
                let end = (at + n).min(bytes.len());
                bytes.drain(at..end);
            }
        }
    }
    bytes
}

proptest! {
    // Cheap cases (microseconds each): many of them, so a regression is caught on a push.
    #![proptest_config(ProptestConfig::with_cases(4096))]

    /// AT-NET-07: arbitrary bytes, with or without a valid header, never panic `verify`.
    #[test]
    fn ots_verify_survives_arbitrary_bytes(
        with_header in any::<bool>(),
        tail in prop::collection::vec(any::<u8>(), 0..512),
    ) {
        let mut proof = Vec::new();
        if with_header {
            proof.extend_from_slice(OTS_MAGIC);
            proof.extend_from_slice(&[0x01, 0x08]);
            proof.extend_from_slice(&[0u8; 32]);
        }
        proof.extend_from_slice(&tail);
        let _ = verify(proof);
    }

    /// AT-NET-07: edits to real proofs — the parser's deepest paths — never panic it.
    #[test]
    fn ots_verify_survives_edited_genuine_proofs(
        large in any::<bool>(),
        edits in prop::collection::vec(edit(), 1..6),
    ) {
        let genuine = if large { LARGE } else { SMALL };
        let _ = verify(apply(genuine.to_vec(), &edits));
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Any shard set and claimed layout is answered, never a panic; a genuine encoding
    /// with losses and corruptions recovers exactly when `k` authentic shards survive.
    #[test]
    fn erasure_survives_hostile_shards_and_layouts(
        shards in prop::collection::vec(
            prop::option::of(prop::collection::vec(any::<u8>(), 0..8)), 0..12),
        manifest in prop::collection::vec(any::<[u8; 32]>(), 0..12),
        data_shards in prop_oneof![0usize..8, Just(usize::MAX), any::<usize>()],
        parity_shards in prop_oneof![0usize..8, Just(usize::MAX), any::<usize>()],
        orig_len in prop_oneof![0usize..64, Just(usize::MAX), any::<usize>()],
    ) {
        let _ = reconstruct_verified(
            shards.clone(), &manifest, data_shards, parity_shards, orig_len);
        let _ = reconstruct(shards, data_shards, parity_shards, orig_len);
    }

    #[test]
    fn erasure_recovers_iff_enough_authentic_shards(
        data in prop::collection::vec(any::<u8>(), 0..64),
        k in 1usize..6,
        m in 1usize..6,
        lost in prop::collection::vec(any::<prop::sample::Index>(), 0..6),
        corrupt in prop::collection::vec(
            (any::<prop::sample::Index>(), any::<prop::sample::Index>(), 1u8..), 0..4),
    ) {
        let enc = encode(&data, k, m);
        let mut shards: Vec<Option<Vec<u8>>> = enc.shards.iter().cloned().map(Some).collect();
        for i in lost {
            shards[i.index(k + m)] = None;
        }
        for (i, at, mask) in corrupt {
            if let Some(shard) = shards[i.index(k + m)].as_mut() {
                let at = at.index(shard.len());
                shard[at] ^= mask;
            }
        }
        let authentic = shards
            .iter()
            .zip(&enc.shards)
            .filter(|(s, orig)| s.as_ref() == Some(*orig))
            .count();
        let got = reconstruct_verified(shards, &enc.manifest, k, m, enc.orig_len);
        if authentic >= k {
            prop_assert_eq!(got.as_deref(), Ok(data.as_slice()));
        } else {
            prop_assert_eq!(got, Err(RecoverError::TooFewAuthenticShards { authentic, need: k }));
        }
    }

    /// Arbitrary signature bytes and indices are never accepted and never panic, however
    /// the checkpoint is bound.
    #[test]
    fn forged_checkpoint_signatures_are_rejected(
        height in any::<u64>(),
        head in any::<[u8; 32]>(),
        sigs in prop::collection::vec(
            (prop_oneof![0usize..8, any::<usize>()], prop::collection::vec(any::<u8>(), 64)),
            0..8),
        with_log in any::<bool>(),
    ) {
        let members: Vec<Member> = (0..5u8).map(|i| Member::from_seed([i; 32])).collect();
        let consortium = || Consortium::new(members.iter().map(Member::public).collect(), 3);
        let net_id = [7u8; 32];
        let cp = Checkpoint::new(net_id, consortium().member_set_hash(), height, head);
        let sigs: Vec<(usize, Signature)> = sigs
            .iter()
            .map(|(i, b)| (*i, Signature::from_bytes(&b[..].try_into().unwrap())))
            .collect();
        let mut client = CheckpointClient::new(net_id, consortium());
        let log = TransparencyLog::new();
        let update = if with_log {
            client.ingest_with_log(&cp, &sigs, &log)
        } else {
            client.ingest(&cp, &sigs)
        };
        prop_assert_eq!(
            update,
            CheckpointUpdate::Rejected(CheckpointReject::InsufficientSignatures)
        );
        prop_assert!(client.trusted().is_none());
    }

    /// A proof exists exactly for an in-range leaf, and an arbitrary proof never panics
    /// verification.
    #[test]
    fn merkle_proofs_for_any_index(
        n in 0usize..40,
        index in prop_oneof![0usize..48, any::<usize>()],
        siblings in prop::collection::vec((any::<bool>(), any::<[u8; 32]>()), 0..8),
    ) {
        let leaves: Vec<[u8; 32]> = (0..n).map(|i| leaf_hash(&i.to_le_bytes())).collect();
        let root = merkle_root(&leaves);
        match merkle_proof(&leaves, index) {
            Some(proof) => prop_assert!(verify_proof(leaves[index], &proof, root)),
            None => prop_assert!(index >= n),
        }
        let _ = verify_proof([0u8; 32], &MerkleProof { siblings }, root);
    }
}
