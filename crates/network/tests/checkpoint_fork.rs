//! A higher checkpoint must extend the trusted one (T38, NET-006 residual). With only
//! checkpoints, a threshold-signed checkpoint at a greater height on a *different*
//! history is indistinguishable from an extension and `ingest` accepts it. A client that
//! holds the log uses `ingest_with_log`, which checks both heads against its log
//! (`log::verify_extends`, T14) and reports the divergence as `Forked`.

use network::cid::cid;
use network::consortium::{
    Checkpoint, CheckpointClient, CheckpointReject, CheckpointUpdate, Consortium, Member,
};
use network::log::TransparencyLog;

const NET: [u8; 32] = [0xAA; 32];

fn committee() -> (Vec<Member>, Consortium) {
    let members: Vec<Member> = (0u8..4).map(|i| Member::from_seed([i + 1; 32])).collect();
    let consortium = Consortium::new(members.iter().map(|m| m.public()).collect(), 3);
    (members, consortium)
}

/// The log's current checkpoint, co-signed by a threshold of `members`.
fn signed(
    members: &[Member],
    msh: [u8; 32],
    log: &TransparencyLog,
) -> (Checkpoint, Vec<(usize, ed25519_dalek::Signature)>) {
    let cp = log.checkpoint(NET, msh);
    let sigs = members
        .iter()
        .enumerate()
        .take(3)
        .map(|(i, m)| (i, m.sign(&cp)))
        .collect();
    (cp, sigs)
}

fn log_of(tags: &[&str]) -> TransparencyLog {
    let mut log = TransparencyLog::new();
    for t in tags {
        log.append(cid(t.as_bytes()));
    }
    log
}

/// Honest history: a, b, c then d, e. The fork shares a, b and rewrites from the third
/// entry (c' instead of c), then keeps going to the same height.
fn histories() -> (TransparencyLog, TransparencyLog, TransparencyLog) {
    let prefix = log_of(&["a", "b", "c"]);
    let honest = log_of(&["a", "b", "c", "d", "e"]);
    let fork = log_of(&["a", "b", "c'", "d", "e"]);
    (prefix, honest, fork)
}

/// A client that trusts the height-3 checkpoint of the honest history.
fn trusting_prefix() -> (Vec<Member>, [u8; 32], CheckpointClient) {
    let (members, consortium) = committee();
    let msh = consortium.member_set_hash();
    let mut client = CheckpointClient::new(NET, consortium);
    let (prefix, _, _) = histories();
    let (cp3, s3) = signed(&members, msh, &prefix);
    assert_eq!(client.ingest(&cp3, &s3), CheckpointUpdate::Accepted);
    (members, msh, client)
}

#[test]
fn an_honest_extension_is_accepted() {
    let (members, msh, mut client) = trusting_prefix();
    let (_, honest, _) = histories();
    let (cp5, s5) = signed(&members, msh, &honest);
    assert_eq!(
        client.ingest_with_log(&cp5, &s5, &honest),
        CheckpointUpdate::Accepted
    );
    assert_eq!(client.trusted(), Some(&cp5));
}

#[test]
fn a_threshold_signed_higher_fork_is_reported_not_accepted() {
    let (members, msh, mut client) = trusting_prefix();
    let (prefix, honest, fork) = histories();
    let (cp_fork, s_fork) = signed(&members, msh, &fork);
    let (cp3, _) = signed(&members, msh, &prefix);

    assert_eq!(
        client.ingest_with_log(&cp_fork, &s_fork, &honest),
        CheckpointUpdate::Forked {
            trusted: cp3,
            conflicting: cp_fork
        }
    );
    assert_eq!(
        client.trusted(),
        Some(&cp3),
        "trust does not move to the fork"
    );

    // Contrast: the checkpoint-only path cannot tell and accepts the fork.
    let (members, msh, mut blind) = trusting_prefix();
    let (cp_fork, s_fork) = signed(&members, msh, &fork);
    assert_eq!(blind.ingest(&cp_fork, &s_fork), CheckpointUpdate::Accepted);
}

#[test]
fn a_log_that_has_not_caught_up_defers_rather_than_decides() {
    let (members, msh, mut client) = trusting_prefix();
    let (prefix, honest, fork) = histories();
    let (cp3, _) = signed(&members, msh, &prefix);

    // The local log is still at height 3: neither an extension nor a fork can be shown yet.
    for other in [&honest, &fork] {
        let (cp5, s5) = signed(&members, msh, other);
        assert_eq!(
            client.ingest_with_log(&cp5, &s5, &prefix),
            CheckpointUpdate::Rejected(CheckpointReject::LogBehind)
        );
        assert_eq!(client.trusted(), Some(&cp3));
    }

    // Once synced, the honest checkpoint is accepted.
    let (cp5, s5) = signed(&members, msh, &honest);
    assert_eq!(
        client.ingest_with_log(&cp5, &s5, &honest),
        CheckpointUpdate::Accepted
    );
}

/// A local log shorter than the *trusted* checkpoint has not caught up; nothing shows it
/// left the trusted history. Found by the T43 model: after trusting a checkpoint without
/// the log (or on first use), a log that was an honest prefix of it was reported as
/// `LocalLogDiverged` — the local copy at fault — instead of `LogBehind`.
#[test]
fn a_local_log_behind_the_trusted_checkpoint_is_behind_not_diverged() {
    let (members, msh, mut client) = trusting_prefix();
    let (_, honest, _) = histories();
    let (cp5, s5) = signed(&members, msh, &honest);

    // Two entries, or none, against a trusted height of 3.
    for short in [log_of(&["a", "b"]), TransparencyLog::new()] {
        assert_eq!(
            client.ingest_with_log(&cp5, &s5, &short),
            CheckpointUpdate::Rejected(CheckpointReject::LogBehind)
        );
    }

    // Once synced, the honest checkpoint is accepted.
    assert_eq!(
        client.ingest_with_log(&cp5, &s5, &honest),
        CheckpointUpdate::Accepted
    );
}

#[test]
fn a_local_log_that_left_the_trusted_history_is_its_own_fault() {
    let (members, msh, mut client) = trusting_prefix();
    let (_, _, fork) = histories();
    // The client's own copy is the rewritten one; the incoming checkpoint is the same
    // rewritten history, so it "extends" the log — but the log no longer extends what
    // the client trusts.
    let (cp_fork, s_fork) = signed(&members, msh, &fork);
    assert_eq!(
        client.ingest_with_log(&cp_fork, &s_fork, &fork),
        CheckpointUpdate::Rejected(CheckpointReject::LocalLogDiverged)
    );
}

#[test]
fn the_other_section_9_4_rules_still_apply_with_a_log() {
    let (members, msh, mut client) = trusting_prefix();
    let (prefix, honest, _) = histories();

    // Replay of the trusted height: stale.
    let (cp3, s3) = signed(&members, msh, &prefix);
    assert_eq!(
        client.ingest_with_log(&cp3, &s3, &honest),
        CheckpointUpdate::Stale
    );

    // Below threshold: rejected before any log check.
    let (cp5, s5) = signed(&members, msh, &honest);
    assert_eq!(
        client.ingest_with_log(&cp5, &s5[..2], &honest),
        CheckpointUpdate::Rejected(CheckpointReject::InsufficientSignatures)
    );
    assert_eq!(client.trusted(), Some(&cp3));
}
