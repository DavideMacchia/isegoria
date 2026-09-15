//! Storage-layer guarantees (`docs/04`): content addressing, tamper-evident log,
//! Merkle inclusion, consortium threshold checkpoints, anchoring, erasure recovery.

use network::anchoring::{Anchor, ReferenceAnchor};
use network::cid::cid;
use network::consortium::{Checkpoint, Consortium, Member};
use network::erasure::{encode, reconstruct};
use network::log::TransparencyLog;
use network::merkle::{leaf_hash, merkle_proof, merkle_root, verify_proof};

#[test]
fn cid_binds_to_content() {
    assert_eq!(cid(b"question A"), cid(b"question A"));
    assert_ne!(cid(b"question A"), cid(b"question A."));
}

#[test]
fn append_only_log_is_tamper_evident() {
    let mut log = TransparencyLog::new();
    log.append(cid(b"item 1"));
    log.append(cid(b"item 2"));
    log.append(cid(b"item 3"));
    assert!(log.verify());
    assert_eq!(log.len(), 3);

    // Each entry links to the previous one.
    let entries = log.entries();
    assert_eq!(entries[1].prev, entries[0].hash);
    assert_eq!(entries[2].prev, entries[1].hash);
}

#[test]
fn log_head_changes_with_every_append() {
    let mut log = TransparencyLog::new();
    let h0 = log.head();
    log.append(cid(b"x"));
    let h1 = log.head();
    log.append(cid(b"y"));
    let h2 = log.head();
    assert_ne!(h0, h1);
    assert_ne!(h1, h2);
}

#[test]
fn merkle_inclusion_proof_verifies() {
    let leaves: Vec<[u8; 32]> = (0..7u8).map(|i| leaf_hash(&[i])).collect();
    let root = merkle_root(&leaves);
    for i in 0..leaves.len() {
        let proof = merkle_proof(&leaves, i);
        assert!(verify_proof(leaves[i], &proof, root), "leaf {i}");
    }
    // A wrong leaf does not verify.
    let bad = merkle_proof(&leaves, 3);
    assert!(!verify_proof(leaf_hash(&[99]), &bad, root));
}

#[test]
fn changing_a_leaf_changes_the_root() {
    let a: Vec<[u8; 32]> = (0..4u8).map(|i| leaf_hash(&[i])).collect();
    let mut b = a.clone();
    b[2] = leaf_hash(&[42]);
    assert_ne!(merkle_root(&a), merkle_root(&b));
}

fn consortium(n: usize, threshold: usize) -> (Vec<Member>, Consortium) {
    let members: Vec<Member> = (0..n).map(|i| Member::from_seed([i as u8; 32])).collect();
    let pubs = members.iter().map(|m| m.public()).collect();
    (members, Consortium::new(pubs, threshold))
}

#[test]
fn checkpoint_needs_a_threshold_of_signers() {
    let (members, con) = consortium(5, 3);
    let cp = Checkpoint {
        height: 10,
        head: [7u8; 32],
    };

    let three: Vec<_> = (0..3).map(|i| (i, members[i].sign(&cp))).collect();
    assert!(con.verify(&cp, &three), "3 of 5 should pass");

    let two: Vec<_> = (0..2).map(|i| (i, members[i].sign(&cp))).collect();
    assert!(!con.verify(&cp, &two), "2 of 5 should fail");
}

#[test]
fn duplicate_and_wrong_signatures_do_not_count() {
    let (members, con) = consortium(5, 3);
    let cp = Checkpoint {
        height: 1,
        head: [1u8; 32],
    };
    // The same member three times is still one signer.
    let dup: Vec<_> = (0..3).map(|_| (0usize, members[0].sign(&cp))).collect();
    assert!(!con.verify(&cp, &dup));

    // A signature over a different checkpoint is invalid here.
    let other = Checkpoint {
        height: 2,
        head: [1u8; 32],
    };
    let mixed = vec![
        (0, members[0].sign(&cp)),
        (1, members[1].sign(&cp)),
        (2, members[2].sign(&other)),
    ];
    assert!(!con.verify(&cp, &mixed), "only 2 valid → below threshold");
}

#[test]
fn anchoring_round_trip() {
    let mut anchor = ReferenceAnchor::new();
    let root = [9u8; 32];
    let receipt = anchor.submit(root);
    assert!(anchor.verify(&receipt));

    let never = network::anchoring::Receipt {
        root: [0u8; 32],
        proof: vec![],
    };
    assert!(!anchor.verify(&never));
}

#[test]
fn erasure_reconstructs_from_any_k_of_n() {
    // docs/04: erasure (10,30) — 10 data shards, 30 total, recover from any 10.
    let data: Vec<u8> = (0..1000u32).map(|i| (i % 251) as u8).collect();
    let enc = encode(&data, 10, 20);
    assert_eq!(enc.shards.len(), 30);

    // Drop 20 shards (indices 5..25); keep 10.
    let mut received: Vec<Option<Vec<u8>>> = enc.shards.iter().cloned().map(Some).collect();
    for s in received.iter_mut().take(25).skip(5) {
        *s = None;
    }

    let recovered = reconstruct(received, 10, 20, enc.orig_len).expect("recover");
    assert_eq!(recovered, data);
}

#[test]
fn erasure_fails_below_k_survivors() {
    let data = vec![1u8; 100];
    let enc = encode(&data, 4, 2); // 6 shards, need any 4
    let mut received: Vec<Option<Vec<u8>>> = enc.shards.iter().cloned().map(Some).collect();
    // Only 3 survive → unrecoverable.
    for s in received.iter_mut().take(3) {
        *s = None;
    }
    assert!(reconstruct(received, 4, 2, enc.orig_len).is_none());
}
