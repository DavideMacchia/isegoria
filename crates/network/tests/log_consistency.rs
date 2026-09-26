//! Signed append-only log: consistency proofs and truncation detection (`docs/08`
//! NET-004/DS-1). A hash chain alone cannot catch a *consistent* suffix rewrite or a
//! truncation; both are caught against a consortium-signed prior head (AT-NET-01).

use network::cid::cid;
use network::consortium::{Checkpoint, Consortium, Member};
use network::log::{ConsistencyError, TransparencyLog};

fn log_of(payloads: &[&[u8]]) -> TransparencyLog {
    let mut log = TransparencyLog::new();
    for p in payloads {
        log.append(cid(p));
    }
    log
}

fn committee() -> (Vec<Member>, Consortium) {
    let members: Vec<Member> = (0u8..4).map(|i| Member::from_seed([i + 1; 32])).collect();
    let consortium = Consortium::new(members.iter().map(|m| m.public()).collect(), 3);
    (members, consortium)
}

/// Stamp a checkpoint bound to the committee's member set; the consistency check reads only
/// the height and the head, so a zero network id is fine here.
fn checkpoint(log: &TransparencyLog) -> Checkpoint {
    log.checkpoint([0u8; 32], committee().1.member_set_hash())
}

/// A checkpoint the consortium has co-signed (a threshold of members) — what a verifier
/// trusts as the prior head.
fn signed(cp: &Checkpoint) -> bool {
    let (members, consortium) = committee();
    let sigs: Vec<(usize, _)> = members
        .iter()
        .enumerate()
        .map(|(i, m)| (i, m.sign(cp)))
        .collect();
    consortium.verify(cp, &sigs)
}

#[test]
fn at_net_01_a_consistent_rewrite_is_detected_against_a_signed_head() {
    // Height-4 log; the consortium signs its head — this is the trusted prior checkpoint.
    let original = log_of(&[b"a", b"b", b"c", b"d"]);
    let prior = checkpoint(&original);
    assert!(signed(&prior), "the prior head is consortium-signed");

    // A consistent rewrite: entry 1 is `X` instead of `b`, every later hash recomputed, so
    // the chain verifies on its own — the NET-004 attack the hash chain cannot catch alone.
    let forged = log_of(&[b"a", b"X", b"c", b"d"]);
    assert!(forged.verify(), "the rewritten chain verifies in isolation");
    assert_eq!(
        forged.verify_extends(&prior),
        Err(ConsistencyError::ForkedHistory),
        "but it does not extend the signed prior head"
    );
}

#[test]
fn truncation_is_detected() {
    let prior = checkpoint(&log_of(&[b"a", b"b", b"c", b"d"]));
    let truncated = log_of(&[b"a", b"b"]);
    assert!(truncated.verify(), "a shorter prefix verifies on its own");
    assert_eq!(
        truncated.verify_extends(&prior),
        Err(ConsistencyError::Truncated { have: 2, need: 4 })
    );
}

#[test]
fn a_genuine_extension_passes() {
    let original = log_of(&[b"a", b"b", b"c", b"d"]);
    let prior = checkpoint(&original);
    // The same prefix plus new entries: the checkpointed head is unchanged, so it extends.
    let extended = log_of(&[b"a", b"b", b"c", b"d", b"e", b"f"]);
    assert_eq!(extended.verify_extends(&prior), Ok(()));
    // The empty checkpoint is a prefix of everything.
    assert_eq!(
        extended.verify_extends(&checkpoint(&TransparencyLog::new())),
        Ok(())
    );
}
