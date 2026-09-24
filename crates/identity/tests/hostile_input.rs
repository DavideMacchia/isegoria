//! No panic on hostile input (T44): enrollment of arbitrary codice-fiscale strings
//! through every oracle, and the OPRF wire messages decoded from arbitrary bytes. The
//! `fuzz/` targets explore the same entry points for longer on a nightly toolchain;
//! these properties keep them covered on every push.

use identity::enrollment::{
    Cie, DuplicateEnrollment, EnrollmentRegistry, ReferenceOracle, Spid, UniquenessOracle,
    VoprfOracle,
};
use identity::oprf::ThresholdOprfOracle;
use proptest::prelude::*;
use rand_core::OsRng;
use std::sync::OnceLock;
use voprf::{BlindedElement, EvaluationElement, Proof, Ristretto255, VoprfClient, VoprfServer};

const INFO: &[u8] = b"isegoria/uniqueness/v1";

// Built once: elliptic-curve work dominates these tests in debug builds, and the `fuzz/`
// targets carry the volume.
type Oracles = [Box<dyn UniquenessOracle + Send + Sync>; 3];

fn oracles() -> &'static Oracles {
    static ORACLES: OnceLock<Oracles> = OnceLock::new();
    ORACLES.get_or_init(|| {
        [
            Box::new(ReferenceOracle::new([1u8; 32])),
            Box::new(VoprfOracle::new([2u8; 32])),
            Box::new(ThresholdOprfOracle::new([3u8; 32], 3, 2)),
        ]
    })
}

/// The server under test, and another one whose answers are well-formed but not its.
fn servers() -> &'static (VoprfServer<Ristretto255>, VoprfServer<Ristretto255>) {
    static SERVERS: OnceLock<(VoprfServer<Ristretto255>, VoprfServer<Ristretto255>)> =
        OnceLock::new();
    SERVERS.get_or_init(|| {
        (
            VoprfServer::new_from_seed(&[7u8; 32], INFO).unwrap(),
            VoprfServer::new_from_seed(&[8u8; 32], INFO).unwrap(),
        )
    })
}

/// Codice-fiscale strings: realistic ones, arbitrary Unicode, whitespace-only, and
/// anchors past the RFC 9497 input limit (u16::MAX bytes).
fn codice_fiscale() -> impl Strategy<Value = String> {
    prop_oneof![
        "[A-Z]{6}[0-9]{2}[A-Z][0-9]{2}[A-Z][0-9]{3}[A-Z]",
        any::<String>(),
        "[ \t\n]{0,4}",
        (65_530usize..65_540, any::<char>()).prop_map(|(n, c)| c.to_string().repeat(n)),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(16))]

    /// Any string enrolls once through every oracle, and the same person via the other
    /// source is a duplicate — never a panic.
    #[test]
    fn any_codice_fiscale_enrolls_once(cf in codice_fiscale()) {
        for oracle in oracles() {
            let mut registry = EnrollmentRegistry::new();
            let first = registry.enroll(&Cie { codice_fiscale: cf.clone() }, oracle.as_ref());
            prop_assert!(first.is_ok());
            prop_assert_eq!(
                registry.enroll(&Spid { codice_fiscale: cf.clone() }, oracle.as_ref()),
                Err(DuplicateEnrollment)
            );
        }
    }
}

/// Proof bytes that decode: two canonical scalars (below 2^252, hence below the group
/// order), so verification itself — not just decoding — runs on hostile values.
fn canonical_proof() -> impl Strategy<Value = Vec<u8>> {
    any::<[[u8; 32]; 2]>().prop_map(|scalars| {
        scalars
            .iter()
            .flat_map(|s| {
                let mut s = *s;
                s[31] &= 0x0f;
                s
            })
            .collect()
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// The server decodes whatever a client sends; the client decodes whatever a server
    /// sends back. Neither panics, and no hostile evaluation verifies.
    #[test]
    fn voprf_wire_messages_survive_arbitrary_bytes(
        blinded in prop::collection::vec(any::<u8>(), 0..40),
        element in prop::collection::vec(any::<u8>(), 0..40),
        proof in prop_oneof![prop::collection::vec(any::<u8>(), 0..72), canonical_proof()],
        genuine_element in any::<bool>(),
        anchor in prop::collection::vec(any::<u8>(), 0..32),
    ) {
        let (server, other) = servers();
        if let Ok(b) = BlindedElement::<Ristretto255>::deserialize(&blinded) {
            let _ = server.blind_evaluate(&mut OsRng, &b);
        }

        let blind = VoprfClient::<Ristretto255>::blind(&anchor, &mut OsRng).unwrap();
        // A well-formed element that is not this server's answer, or arbitrary bytes.
        let element = if genuine_element {
            other.blind_evaluate(&mut OsRng, &blind.message).message.serialize().to_vec()
        } else {
            element
        };
        if let (Ok(e), Ok(p)) = (
            EvaluationElement::<Ristretto255>::deserialize(&element),
            Proof::<Ristretto255>::deserialize(&proof),
        ) {
            let out = blind.state.finalize(&anchor, &e, &p, server.get_public_key());
            prop_assert!(out.is_err());
        }
    }
}
