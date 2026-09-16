//! The Semaphore-style ZK nullifier (`docs/03` §M3): a per-role pseudonym proven to
//! derive from a valid credential, deterministic per (person, role), unlinkable across
//! roles and to the person.

use identity::credential::{Credential, Issuer};
use identity::enrollment::Label;
use identity::nullifier::{prove, verify};
use identity::nym::Role;

fn issued() -> (Issuer, identity::credential::AnonymousCredential) {
    let issuer = Issuer::new([1u8; 32]);
    let holder = Credential::from_secret([9u8; 32]);
    let (request, pending) = holder.request_issuance(&Label([7u8; 32]), &issuer.public());
    let credential = pending.finalize(issuer.issue(&request).unwrap());
    (issuer, credential)
}

#[test]
fn a_valid_nullifier_proof_verifies() {
    let (issuer, cred) = issued();
    let proof = prove(&cred, &issuer.public(), Role::Judge);
    assert!(verify(&proof, &issuer.public()));
}

#[test]
fn same_person_and_role_give_the_same_nullifier() {
    let (issuer, cred) = issued();
    // Two independent proofs (fresh randomness) both verify and share the nullifier —
    // so repeated actions by one person in one role collide and are detectable.
    let p1 = prove(&cred, &issuer.public(), Role::Judge);
    let p2 = prove(&cred, &issuer.public(), Role::Judge);
    assert!(verify(&p1, &issuer.public()));
    assert!(verify(&p2, &issuer.public()));
    assert_eq!(p1.nullifier(), p2.nullifier());
}

#[test]
fn different_roles_give_different_nullifiers() {
    let (issuer, cred) = issued();
    let judge = prove(&cred, &issuer.public(), Role::Judge);
    let propose = prove(&cred, &issuer.public(), Role::Propose);
    assert_ne!(judge.nullifier(), propose.nullifier());
}

#[test]
fn different_people_get_different_nullifiers() {
    let issuer = Issuer::new([1u8; 32]);
    let mk = |secret: [u8; 32]| {
        let holder = Credential::from_secret(secret);
        let (req, pending) = holder.request_issuance(&Label([7u8; 32]), &issuer.public());
        pending.finalize(issuer.issue(&req).unwrap())
    };
    let a = prove(&mk([9u8; 32]), &issuer.public(), Role::Judge);
    let b = prove(&mk([10u8; 32]), &issuer.public(), Role::Judge);
    assert_ne!(a.nullifier(), b.nullifier());
}

#[test]
fn a_proof_does_not_verify_under_a_different_issuer() {
    let (issuer, cred) = issued();
    let other = Issuer::new([2u8; 32]);
    let proof = prove(&cred, &issuer.public(), Role::Judge);
    assert!(verify(&proof, &issuer.public()));
    assert!(!verify(&proof, &other.public()));
}
