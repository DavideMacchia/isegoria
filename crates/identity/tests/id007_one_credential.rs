//! One credential per label (docs/08 ID-007, T11): a person has exactly one uniqueness
//! label (enrollment), so the issuer signs at most one credential for it. This is the
//! structural block on whitewashing — you cannot mint a fresh identity to shed a bad
//! reputation, whatever secret you choose.

use identity::credential::{Credential, IssuanceError, IssuanceRegistry, Issuer};
use identity::enrollment::Label;

#[test]
fn at_id_02_a_second_credential_for_a_label_is_refused() {
    let issuer = Issuer::new([1u8; 32]);
    let label = Label([7u8; 32]);
    let mut registry = IssuanceRegistry::new();

    let (req1, _) = Credential::from_secret([9u8; 32]).request_issuance(&label, &issuer.public());
    assert!(issuer.issue_once(&mut registry, &req1).is_ok());

    // A second, well-formed request for the SAME label is refused.
    let (req2, _) = Credential::from_secret([9u8; 32]).request_issuance(&label, &issuer.public());
    assert_eq!(
        issuer.issue_once(&mut registry, &req2).unwrap_err(),
        IssuanceError::AlreadyIssued
    );
}

#[test]
fn at_id_03_whitewashing_with_a_fresh_secret_is_refused() {
    // Enroll once → one label. Requesting with secret x₁ then a NEW secret x₂ carries the
    // same label both times, so the second issuance is refused: a person cannot obtain a
    // second, reputation-free identity by picking a fresh secret.
    let issuer = Issuer::new([1u8; 32]);
    let label = Label([7u8; 32]);
    let mut registry = IssuanceRegistry::new();

    let (req_x1, _) = Credential::from_secret([1u8; 32]).request_issuance(&label, &issuer.public());
    assert!(issuer.issue_once(&mut registry, &req_x1).is_ok());

    let (req_x2, _) = Credential::from_secret([2u8; 32]).request_issuance(&label, &issuer.public());
    assert_eq!(
        issuer.issue_once(&mut registry, &req_x2).unwrap_err(),
        IssuanceError::AlreadyIssued
    );
}

#[test]
fn distinct_labels_each_receive_one_credential() {
    // Two different people (two labels) each get their credential; the registry gates per
    // label, not globally.
    let issuer = Issuer::new([1u8; 32]);
    let mut registry = IssuanceRegistry::new();

    let (a, _) =
        Credential::from_secret([9u8; 32]).request_issuance(&Label([7u8; 32]), &issuer.public());
    let (b, _) =
        Credential::from_secret([9u8; 32]).request_issuance(&Label([8u8; 32]), &issuer.public());
    assert!(issuer.issue_once(&mut registry, &a).is_ok());
    assert!(issuer.issue_once(&mut registry, &b).is_ok());
    assert!(registry.contains(&Label([7u8; 32])));
    assert!(registry.contains(&Label([8u8; 32])));
}
