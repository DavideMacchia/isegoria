//! The Sybil-resistant admission boundary (docs/08 PROTO-007 / G-04 / INV-9, T6).
//!
//! - **AT-PRO-01:** an action carries a verified role nullifier proof or it is refused;
//!   a `Nym` alone cannot act, and a proof from anyone but the issuing committee, or for
//!   the wrong role, is rejected.
//! - **AT-ID-05:** a proof is bound to its action context, so it cannot be lifted from
//!   one action onto another.
//! - INV-9 rate-limit keying: one judgment per role-nullifier per item.

use identity::credential::{AnonymousCredential, Credential, Issuer};
use identity::enrollment::Label;
use identity::nullifier::{prove, NullifierProof};
use identity::nym::Role;
use network::cid::cid;
use protocol::admission::{admit, NullifierSet, QuotaLedger, Unproven};
use protocol::deposit::{deposit_context, deposit_with_identity, DepositRejected, Draft};
use protocol::review::{review_context, submit_review, ReviewRejected};

const EPOCH: u64 = 7;

fn issued(secret: [u8; 32]) -> (Issuer, AnonymousCredential) {
    let issuer = Issuer::new([1u8; 32]);
    let holder = Credential::from_secret(secret);
    let (req, pending) = holder.request_issuance(&Label([7u8; 32]), &issuer.public());
    let cred = pending.finalize(issuer.issue(&req).unwrap());
    (issuer, cred)
}

/// A `Judge` proof bound to `item` at `EPOCH`, the way a real reviewer client builds it.
fn judge_proof(
    issuer: &Issuer,
    cred: &AnonymousCredential,
    item: network::cid::Cid,
) -> NullifierProof {
    prove(
        cred,
        &issuer.public(),
        Role::Judge,
        &review_context(item, EPOCH),
    )
}

// ------------------------------------ AT-PRO-01 ------------------------------------

#[test]
fn a_valid_role_proof_is_admitted() {
    let (issuer, cred) = issued([9u8; 32]);
    let item = cid(b"item-A");
    let mut panel = NullifierSet::new();
    let proof = judge_proof(&issuer, &cred, item);
    assert!(submit_review(&proof, &issuer.public(), item, EPOCH, &mut panel).is_ok());
}

#[test]
fn a_self_issued_credential_cannot_act() {
    // The Sybil attempt: an attacker runs its OWN committee, issues itself a credential,
    // and proves against its own key. Presented to the real committee's verifier the proof
    // fails, so a self-minted identity carries no weight (AT-PRO-01).
    let real = Issuer::new([1u8; 32]);
    let attacker = Issuer::new([2u8; 32]);
    let holder = Credential::from_secret([9u8; 32]);
    let (req, pending) = holder.request_issuance(&Label([7u8; 32]), &attacker.public());
    let sybil_cred = pending.finalize(attacker.issue(&req).unwrap());

    let item = cid(b"item-A");
    let proof = prove(
        &sybil_cred,
        &attacker.public(),
        Role::Judge,
        &review_context(item, EPOCH),
    );
    let mut panel = NullifierSet::new();
    assert_eq!(
        submit_review(&proof, &real.public(), item, EPOCH, &mut panel),
        Err(ReviewRejected::Unproven(Unproven::BadProof))
    );
}

#[test]
fn a_proof_for_the_wrong_role_is_rejected() {
    let (issuer, cred) = issued([9u8; 32]);
    let item = cid(b"item-A");
    // A Propose proof cannot stand in for a Judge action.
    let propose = prove(
        &cred,
        &issuer.public(),
        Role::Propose,
        &review_context(item, EPOCH),
    );
    let mut panel = NullifierSet::new();
    assert_eq!(
        submit_review(&propose, &issuer.public(), item, EPOCH, &mut panel),
        Err(ReviewRejected::Unproven(Unproven::WrongRole))
    );
}

#[test]
fn deposit_requires_a_propose_proof_bound_to_the_draft() {
    let (issuer, cred) = issued([9u8; 32]);
    let mut log = network::log::TransparencyLog::new();
    let draft = Draft {
        item: b"a question".to_vec(),
        primary_source: b"Gazzetta Ufficiale".to_vec(),
    };
    let proof = prove(
        &cred,
        &issuer.public(),
        Role::Propose,
        &deposit_context(draft.content_id(), EPOCH),
    );
    let mut quota = QuotaLedger::new();
    let (deposited, proposer) = deposit_with_identity(
        &mut log,
        &draft,
        &proof,
        &issuer.public(),
        EPOCH,
        &mut quota,
        8,
    )
    .unwrap();
    assert_eq!(deposited, draft.content_id());
    // The proposer id is the proven nullifier id, stable and non-rotatable.
    assert_eq!(proposer, proof.id());
}

// ------------------------------------ AT-ID-05 ------------------------------------

#[test]
fn a_review_proof_cannot_be_replayed_onto_another_item() {
    let (issuer, cred) = issued([9u8; 32]);
    let (item_a, item_b) = (cid(b"item-A"), cid(b"item-B"));
    let proof = judge_proof(&issuer, &cred, item_a);

    // It works for the item it was made for …
    let mut panel_a = NullifierSet::new();
    assert!(submit_review(&proof, &issuer.public(), item_a, EPOCH, &mut panel_a).is_ok());

    // … but the same proof presented for a different item does not verify (AT-ID-05).
    let mut panel_b = NullifierSet::new();
    assert_eq!(
        submit_review(&proof, &issuer.public(), item_b, EPOCH, &mut panel_b),
        Err(ReviewRejected::Unproven(Unproven::BadProof))
    );
}

#[test]
fn a_deposit_proof_cannot_be_replayed_onto_another_draft() {
    let (issuer, cred) = issued([9u8; 32]);
    let mut log = network::log::TransparencyLog::new();
    let draft_a = Draft {
        item: b"draft A".to_vec(),
        primary_source: b"src".to_vec(),
    };
    let draft_b = Draft {
        item: b"draft B".to_vec(),
        primary_source: b"src".to_vec(),
    };
    // A proof bound to draft A, presented for draft B, is refused.
    let proof = prove(
        &cred,
        &issuer.public(),
        Role::Propose,
        &deposit_context(draft_a.content_id(), EPOCH),
    );
    let mut quota = QuotaLedger::new();
    assert_eq!(
        deposit_with_identity(
            &mut log,
            &draft_b,
            &proof,
            &issuer.public(),
            EPOCH,
            &mut quota,
            8
        ),
        Err(DepositRejected::Unproven(Unproven::BadProof))
    );
}

// -------------------------------- INV-9 dedup keying --------------------------------

#[test]
fn one_person_cannot_review_an_item_twice() {
    let (issuer, cred) = issued([9u8; 32]);
    let item = cid(b"item-A");
    let mut panel = NullifierSet::new();

    // Two independent proofs (fresh randomness) by the same person-role share the
    // nullifier id, so the panel accepts the first and rejects the second.
    let p1 = judge_proof(&issuer, &cred, item);
    let p2 = judge_proof(&issuer, &cred, item);
    let id1 = submit_review(&p1, &issuer.public(), item, EPOCH, &mut panel).unwrap();
    assert_eq!(
        submit_review(&p2, &issuer.public(), item, EPOCH, &mut panel),
        Err(ReviewRejected::Duplicate)
    );
    // Distinct people are admitted on the same item.
    let (issuer2, cred2) = issued([10u8; 32]);
    let _ = issuer2; // same committee key [1u8;32]
    let other = judge_proof(&issuer, &cred2, item);
    let id2 = submit_review(&other, &issuer.public(), item, EPOCH, &mut panel).unwrap();
    assert_ne!(id1, id2);
}

// ------------------------------------ admit() unit ------------------------------------

#[test]
fn admit_returns_the_proven_id_and_binds_role_and_context() {
    let (issuer, cred) = issued([9u8; 32]);
    let ctx = b"some-action";
    let proof = prove(&cred, &issuer.public(), Role::Judge, ctx);
    assert_eq!(
        admit(&proof, &issuer.public(), Role::Judge, ctx),
        Ok(proof.id())
    );
    assert_eq!(
        admit(&proof, &issuer.public(), Role::Propose, ctx),
        Err(Unproven::WrongRole)
    );
    assert_eq!(
        admit(&proof, &issuer.public(), Role::Judge, b"other-action"),
        Err(Unproven::BadProof)
    );
}
