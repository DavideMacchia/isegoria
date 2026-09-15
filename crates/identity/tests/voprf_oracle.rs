//! Cryptographic properties of the real uniqueness-label backend (`docs/03` §M1):
//! the single-server VOPRF (RFC 9497, Ristretto255-SHA512) behind
//! `UniquenessOracle`. Dedup / cross-source equality is covered by the pipeline
//! tests in `properties.rs`; here we pin the protocol-level guarantees that make the
//! label trustworthy — determinism independent of the blind, key separation,
//! obliviousness, and verifiability.

use identity::enrollment::{Anchor, UniquenessOracle, VoprfOracle};
use rand_core::OsRng;
use voprf::{Ristretto255, VoprfClient, VoprfServer};

const INFO: &[u8] = b"isegoria/uniqueness/v1";

fn anchor(s: &str) -> Anchor {
    Anchor(s.to_string())
}

#[test]
fn label_is_deterministic_despite_a_fresh_random_blind() {
    // Each call blinds with a fresh random scalar, yet the finalized PRF output
    // F(k, anchor) is blind-independent — so the label is stable. Without this the
    // dedup registry could never recognise a repeat enrollment.
    let oracle = VoprfOracle::new([7u8; 32]);
    let a = anchor("RSSMRA80A01H501U");
    assert_eq!(oracle.label(&a), oracle.label(&a));
}

#[test]
fn same_seed_rebuilds_the_same_oracle() {
    // DeriveKeyPair is deterministic: an operator that reloads its seed in another
    // process recomputes identical labels. This is what lets dedup work across the
    // network rather than within a single instance.
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
fn the_blinded_message_hides_the_anchor() {
    // Obliviousness: what leaves the client is a blinded group element, not the
    // codice fiscale. The server evaluating the OPRF never sees the anchor in the
    // clear. (This is the property the plain keyed-hash reference never had.)
    let input = b"RSSMRA80A01H501U";
    let mut rng = OsRng;
    let blind = VoprfClient::<Ristretto255>::blind(input, &mut rng).unwrap();
    let on_the_wire = blind.message.serialize();
    assert_ne!(on_the_wire.as_slice(), input.as_slice());
}

#[test]
fn a_wrong_key_fails_verification() {
    // Verifiability: the client checks the server's proof against the committed
    // public key. An evaluation that does not match that key is rejected — a server
    // cannot silently answer under a different key.
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
