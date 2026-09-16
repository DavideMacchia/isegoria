//! Threshold BBS+ issuance (`docs/03` §M2): a `t`-of-`n` committee blind-signs the
//! credential via MPC, and the aggregate is an ordinary BBS+ signature the holder
//! unblinds and verifies — exactly as with the single issuer, but no `t-1` members and
//! no single party can produce it. The holder side is unchanged.

use identity::credential::{Credential, ThresholdIssuer};
use identity::enrollment::Label;

fn label(byte: u8) -> Label {
    Label([byte; 32])
}

#[test]
fn threshold_issued_credential_verifies() {
    let issuer = ThresholdIssuer::new([1u8; 32], 4, 3);
    let holder = Credential::from_secret([9u8; 32]);

    let (request, pending) = holder.request_issuance(&label(7), &issuer.public());
    let blind = issuer
        .issue(&request)
        .expect("committee signs a valid request");
    let credential = pending.finalize(blind);

    assert!(credential.verify(&issuer.public()));
}

#[test]
fn a_smaller_quorum_also_works() {
    // t-of-n with a 2-of-3 committee.
    let issuer = ThresholdIssuer::new([5u8; 32], 3, 2);
    let holder = Credential::from_secret([42u8; 32]);

    let (request, pending) = holder.request_issuance(&label(3), &issuer.public());
    let blind = issuer.issue(&request).unwrap();
    assert!(pending.finalize(blind).verify(&issuer.public()));
}

#[test]
fn a_credential_does_not_verify_under_a_different_committee() {
    let issuer = ThresholdIssuer::new([1u8; 32], 4, 3);
    let other = ThresholdIssuer::new([2u8; 32], 4, 3);
    let holder = Credential::from_secret([9u8; 32]);

    let (request, pending) = holder.request_issuance(&label(7), &issuer.public());
    let credential = pending.finalize(issuer.issue(&request).unwrap());

    assert!(credential.verify(&issuer.public()));
    assert!(!credential.verify(&other.public()));
}
