//! Cryptographic properties of the real uniqueness-label backend (`docs/03` §M1): the
//! single-server VOPRF behind `UniquenessOracle` — determinism independent of the blind,
//! key separation, obliviousness, verifiability. Dedup is covered in `properties.rs`.

use identity::enrollment::{Anchor, UniquenessOracle, VoprfOracle};
use rand_core::OsRng;
use voprf::{Ristretto255, VoprfClient, VoprfServer};

const INFO: &[u8] = b"isegoria/uniqueness/v1";

fn anchor(s: &str) -> Anchor {
    Anchor(s.to_string())
}

#[test]
fn label_is_deterministic_despite_a_fresh_random_blind() {
    // Each call blinds with a fresh random scalar; the finalized output F(k, anchor)
    // is blind-independent.
    let oracle = VoprfOracle::new([7u8; 32]);
    let a = anchor("RSSMRA80A01H501U");
    assert_eq!(oracle.label(&a), oracle.label(&a));
}

#[test]
fn same_seed_rebuilds_the_same_oracle() {
    // DeriveKeyPair is deterministic: reloading the seed in another process
    // recomputes identical labels.
    let a = anchor("VRDLGI85M02F205Z");
    assert_eq!(
        VoprfOracle::new([42u8; 32]).label(&a),
        VoprfOracle::new([42u8; 32]).label(&a),
    );
}

#[test]
fn different_keys_give_different_labels() {
    // Two committees (distinct keys) must not collide on the same person, or one
    // could pre-seed the other's label set.
    let a = anchor("RSSMRA80A01H501U");
    assert_ne!(
        VoprfOracle::new([1u8; 32]).label(&a),
        VoprfOracle::new([2u8; 32]).label(&a),
    );
}

#[test]
fn distinct_anchors_give_distinct_labels() {
    let oracle = VoprfOracle::new([7u8; 32]);
    assert_ne!(oracle.label(&anchor("AAA")), oracle.label(&anchor("BBB")));
}

#[test]
fn an_anchor_beyond_the_rfc_9497_limit_still_gets_a_stable_label() {
    // Longer than the RFC 9497 cap: hashed first, under a tag of its own, so it stays
    // distinct from every other anchor's; an anchor at the limit is unaffected.
    let oracle = VoprfOracle::new([7u8; 32]);
    let long = anchor(&"A".repeat(70_000));
    let longer = anchor(&"A".repeat(70_001));
    let at_limit = anchor(&"A".repeat(usize::from(u16::MAX)));
    assert_eq!(oracle.label(&long), oracle.label(&long));
    assert_ne!(oracle.label(&long), oracle.label(&longer));
    assert_eq!(oracle.label(&at_limit), oracle.label(&at_limit));
    assert_ne!(oracle.label(&at_limit), oracle.label(&long));
}

#[test]
fn the_blinded_message_hides_the_anchor() {
    // Obliviousness: what leaves the client is a blinded group element, not the
    // codice fiscale.
    let input = b"RSSMRA80A01H501U";
    let mut rng = OsRng;
    let blind = VoprfClient::<Ristretto255>::blind(input, &mut rng).unwrap();
    let on_the_wire = blind.message.serialize();
    assert_ne!(on_the_wire.as_slice(), input.as_slice());
}

#[test]
fn a_wrong_key_fails_verification() {
    // Verifiability: the client checks the server's proof against the committed
    // public key; a mismatched key is rejected.
    let input = b"RSSMRA80A01H501U";
    let mut rng = OsRng;

    let server = VoprfServer::<Ristretto255>::new_from_seed(&[7u8; 32], INFO).unwrap();
    let blind = VoprfClient::<Ristretto255>::blind(input, &mut rng).unwrap();
    let eval = server.blind_evaluate(&mut rng, &blind.message);

    // Honest path verifies.
    assert!(blind
        .state
        .finalize(input, &eval.message, &eval.proof, server.get_public_key())
        .is_ok());

    // Same proof checked against a different key: verification fails.
    let impostor = VoprfServer::<Ristretto255>::new_from_seed(&[8u8; 32], INFO).unwrap();
    assert!(blind
        .state
        .finalize(input, &eval.message, &eval.proof, impostor.get_public_key())
        .is_err());
}
