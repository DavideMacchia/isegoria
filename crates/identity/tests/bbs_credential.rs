//! Blind BBS+ credential issuance (`docs/03` §M2) through the public API: end-to-end
//! round-trip and issuer-key binding. Blindness and PoK soundness are asserted with
//! internal access in `credential.rs`'s unit tests.

use identity::credential::{Credential, Issuer};
use identity::enrollment::Label;

fn label(byte: u8) -> Label {
    Label([byte; 32])
}

#[test]
fn issued_credential_verifies() {
    let issuer = Issuer::new([1u8; 32]);
    let holder = Credential::from_secret([9u8; 32]);

    let (request, pending) = holder.request_issuance(&label(7), &issuer.public());
    let blind = issuer.issue(&request).expect("valid request is signed");
    let credential = pending.finalize(blind);

    assert!(credential.verify(&issuer.public()));
}

#[test]
fn a_credential_does_not_verify_under_a_different_issuer() {
    let issuer = Issuer::new([1u8; 32]);
    let other = Issuer::new([2u8; 32]);
    let holder = Credential::from_secret([9u8; 32]);

    let (request, pending) = holder.request_issuance(&label(7), &issuer.public());
    let blind = issuer.issue(&request).unwrap();
    let credential = pending.finalize(blind);

    // Right key accepts, wrong key rejects: the signature is bound to the issuer.
    assert!(credential.verify(&issuer.public()));
    assert!(!credential.verify(&other.public()));
}
