//! A deposit is accepted once (`docs/08` §9.1 row 1, PROTO-007, T64).
//!
//! The same `(draft, proof)` presented again — by the author or by anyone who saw it in
//! transit — is refused as a duplicate **before** the identity check and the quota
//! charge, so a replay appends nothing and costs the author nothing. A `Propose` proof
//! is bound to the draft *and* the epoch (`deposit_context`, AT-ID-05), so it does not
//! outlive its epoch.

use identity::credential::{AnonymousCredential, Credential, Issuer};
use identity::enrollment::Label;
use identity::nullifier::{prove, NullifierProof};
use identity::nym::Role;
use network::log::TransparencyLog;
use protocol::admission::{QuotaLedger, Unproven};
use protocol::deposit::{deposit, deposit_context, deposit_with_identity, DepositRejected, Draft};

const EPOCH: u64 = 7;
const QUOTA: u32 = 2;

fn issued(secret: [u8; 32]) -> (Issuer, AnonymousCredential) {
    let issuer = Issuer::new([1u8; 32]);
    let holder = Credential::from_secret(secret);
    let (req, pending) = holder.request_issuance(&Label([7u8; 32]), &issuer.public());
    let cred = pending.finalize(issuer.issue(&req).unwrap());
    (issuer, cred)
}

fn draft(tag: &str) -> Draft {
    Draft {
        item: tag.as_bytes().to_vec(),
        primary_source: b"Gazzetta Ufficiale".to_vec(),
    }
}

/// A `Propose` proof bound to `draft` at `epoch`, the way a real author client builds it.
fn propose_proof(
    issuer: &Issuer,
    cred: &AnonymousCredential,
    draft: &Draft,
    epoch: u64,
) -> NullifierProof {
    prove(
        cred,
        &issuer.public(),
        Role::Propose,
        &deposit_context(draft.content_id(), epoch),
    )
}

fn try_deposit(
    log: &mut TransparencyLog,
    ledger: &mut QuotaLedger,
    issuer: &Issuer,
    draft: &Draft,
    proof: &NullifierProof,
    epoch: u64,
) -> Result<(network::cid::Cid, identity::nym::Nym), DepositRejected> {
    deposit_with_identity(log, draft, proof, &issuer.public(), epoch, ledger, QUOTA)
}

#[test]
fn a_replayed_deposit_is_refused_before_the_quota_is_charged() {
    let (issuer, cred) = issued([9u8; 32]);
    let mut log = TransparencyLog::new();
    let mut ledger = QuotaLedger::new();
    let d = draft("a question");
    let proof = propose_proof(&issuer, &cred, &d, EPOCH);
    let (id, proposer) = try_deposit(&mut log, &mut ledger, &issuer, &d, &proof, EPOCH).unwrap();
    assert!(log.contains(&id));

    // Whoever saw the pair in transit replays it, again and again.
    for _ in 0..5 {
        assert_eq!(
            try_deposit(&mut log, &mut ledger, &issuer, &d, &proof, EPOCH),
            Err(DepositRejected::DuplicateCid)
        );
    }
    assert_eq!(log.len(), 1, "a replay appends nothing");
    assert_eq!(ledger.used(&proposer), 1, "a replay is not charged");

    // The author's quota is intact: a second, different draft still goes through.
    let d2 = draft("another question");
    let proof2 = propose_proof(&issuer, &cred, &d2, EPOCH);
    assert!(
        try_deposit(&mut log, &mut ledger, &issuer, &d2, &proof2, EPOCH).is_ok(),
        "the replay did not drain the author's quota"
    );
    assert_eq!(log.len(), 2);
}

#[test]
fn the_same_draft_with_a_fresh_proof_is_still_a_duplicate() {
    // Not a byte replay: the author proves again (fresh randomness) for the same draft.
    // The content id is the same, so the log refuses it — one deposit per draft.
    let (issuer, cred) = issued([9u8; 32]);
    let mut log = TransparencyLog::new();
    let mut ledger = QuotaLedger::new();
    let d = draft("a question");
    let first = propose_proof(&issuer, &cred, &d, EPOCH);
    let (_, proposer) = try_deposit(&mut log, &mut ledger, &issuer, &d, &first, EPOCH).unwrap();

    let again = propose_proof(&issuer, &cred, &d, EPOCH);
    assert_eq!(
        try_deposit(&mut log, &mut ledger, &issuer, &d, &again, EPOCH),
        Err(DepositRejected::DuplicateCid)
    );
    assert_eq!(log.len(), 1);
    assert_eq!(ledger.used(&proposer), 1);
}

#[test]
fn the_plain_record_step_refuses_a_duplicate_cid() {
    let mut log = TransparencyLog::new();
    let d = draft("a question");
    let id = deposit(&mut log, &d).unwrap();
    assert_eq!(deposit(&mut log, &d), Err(DepositRejected::DuplicateCid));
    assert_eq!(log.len(), 1);
    assert!(log.contains(&id));
}

#[test]
fn a_propose_proof_does_not_outlive_its_epoch() {
    let (issuer, cred) = issued([9u8; 32]);
    let mut log = TransparencyLog::new();
    let mut ledger = QuotaLedger::new();
    let d = draft("a question");
    let proof = propose_proof(&issuer, &cred, &d, EPOCH);

    // Presented in the next epoch, the proof does not verify: nothing is appended or
    // charged.
    assert_eq!(
        try_deposit(&mut log, &mut ledger, &issuer, &d, &proof, EPOCH + 1),
        Err(DepositRejected::Unproven(Unproven::BadProof))
    );
    assert!(log.is_empty());
    assert_eq!(ledger.used(&proof.id()), 0);

    // In its own epoch it is accepted.
    assert!(try_deposit(&mut log, &mut ledger, &issuer, &d, &proof, EPOCH).is_ok());
}
