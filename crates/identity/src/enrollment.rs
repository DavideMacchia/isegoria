//! Enrollment pipeline (M1, M2). See `docs/03`, §M1–M2.
//!
//! Every source converges on one canonical anchor (the codice fiscale in Italy),
//! which an oblivious PRF turns into a uniqueness label: a second enrollment yields
//! the same label and is rejected as a duplicate, regardless of the source. The real
//! backend is a single-server VOPRF ([`VoprfOracle`], RFC 9497); the spec's target is
//! the *threshold* OPRF split across the committee — see [`VoprfOracle`] for exactly
//! what is real today and what that leaves to future work.

use crate::hash::tagged;
use rand_core::OsRng;
use std::collections::HashSet;
use voprf::{Ristretto255, VoprfClient, VoprfServer};

/// Canonical per-person anchor. In Italy both CIE and SPID bind to the codice fiscale.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Anchor(pub String);

/// An enrollment source. Each real verifier (CIE via NFC, SPID IdP, e-passport)
/// is an adapter that extracts the same canonical anchor.
pub trait IdentityDocument {
    fn canonical_anchor(&self) -> Anchor;
}

fn normalize_cf(cf: &str) -> Anchor {
    Anchor(cf.trim().to_uppercase())
}

pub struct Cie {
    pub codice_fiscale: String,
}
impl IdentityDocument for Cie {
    fn canonical_anchor(&self) -> Anchor {
        normalize_cf(&self.codice_fiscale)
    }
}

pub struct Spid {
    pub codice_fiscale: String,
}
impl IdentityDocument for Spid {
    fn canonical_anchor(&self) -> Anchor {
        normalize_cf(&self.codice_fiscale)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Label(pub [u8; 32]);

/// Oblivious PRF over the anchor (`docs/03`, §M1). The anchor space is small and
/// brute-forceable, so the label is computed obliviously: the server never sees the
/// anchor in the clear. The spec's target is a *threshold* OPRF, so that no single
/// issuer can evaluate it alone; [`VoprfOracle`] is the real single-server step, and
/// [`ReferenceOracle`] is a bare keyed hash kept only for cheap tests.
pub trait UniquenessOracle {
    fn label(&self, anchor: &Anchor) -> Label;
}

/// Test-only oracle: a bare keyed hash, with no oblivious protocol at all. It exists
/// only to exercise the registry cheaply; it is NOT secure and NOT a stand-in for the
/// real thing. The real backend is [`VoprfOracle`]; prefer it everywhere.
pub struct ReferenceOracle {
    key: [u8; 32],
}
impl ReferenceOracle {
    pub fn new(key: [u8; 32]) -> Self {
        ReferenceOracle { key }
    }
}
impl UniquenessOracle for ReferenceOracle {
    fn label(&self, anchor: &Anchor) -> Label {
        Label(tagged(
            "isegoria/uniqueness/v1",
            &[&self.key, anchor.0.as_bytes()],
        ))
    }
}

/// Info string binding the derived key to this application (RFC 9497 DeriveKeyPair).
const VOPRF_INFO: &[u8] = b"isegoria/uniqueness/v1";

/// Real uniqueness-label backend: a single-server **VOPRF** (RFC 9497, verifiable
/// mode, Ristretto255-SHA512). `label` runs the full oblivious round-trip in-process
/// — the client blinds the anchor, the server evaluates under its committed key and
/// proves it, the client verifies the proof and unblinds — so the returned label is
/// the genuine PRF output `F(k, anchor)`, independent of the random blind.
///
/// **What is real here.** The oblivious protocol (the server never sees the anchor
/// in the clear) and verifiability (the client checks, against the server's public
/// key, that the committed key was used). This is a real cryptographic primitive,
/// not a placeholder.
///
/// **What is still modeled (`docs/03` §M1, future work).** The key lives with a
/// *single* server. The spec calls for a **threshold** key split t-of-n across the
/// issuing committee, so that no single party can evaluate the OPRF alone. Until
/// then, a lone key-holder can still brute-force the small, enumerable codice-fiscale
/// space by evaluating `F(k, ·)` on candidates. Single-server VOPRF does not close
/// that gap — it does not regress on the keyed hash and it does not overclaim it.
pub struct VoprfOracle {
    server: VoprfServer<Ristretto255>,
}

impl VoprfOracle {
    /// Derives the server key deterministically from `seed` (RFC 9497 DeriveKeyPair):
    /// the same seed yields the same key, hence the same labels across processes —
    /// which is what makes the dedup registry and its tests reproducible.
    pub fn new(seed: [u8; 32]) -> Self {
        let server = VoprfServer::<Ristretto255>::new_from_seed(&seed, VOPRF_INFO)
            .expect("32-byte seed derives a valid Ristretto255 VOPRF key");
        VoprfOracle { server }
    }
}

impl UniquenessOracle for VoprfOracle {
    fn label(&self, anchor: &Anchor) -> Label {
        let input = anchor.0.as_bytes();
        let mut rng = OsRng;
        // 1. Client blinds the anchor; only the blinded element goes to the server.
        let blind = VoprfClient::<Ristretto255>::blind(input, &mut rng)
            .expect("anchor is a non-empty, bounded byte string");
        // 2. Server evaluates under its committed key and returns a proof of it.
        let eval = self.server.blind_evaluate(&mut rng, &blind.message);
        // 3. Client verifies the proof against the public key and unblinds to F(k, anchor).
        let output = blind
            .state
            .finalize(
                input,
                &eval.message,
                &eval.proof,
                self.server.get_public_key(),
            )
            .expect("proof verifies: client and server share this server's key");
        // Reduce the 64-byte PRF output to a 32-byte, domain-separated label.
        Label(tagged("isegoria/uniqueness/voprf/v1", &[output.as_slice()]))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct DuplicateEnrollment;

/// Set of uniqueness labels already enrolled. Enforces P1: one person → one ID.
#[derive(Default)]
pub struct EnrollmentRegistry {
    used: HashSet<Label>,
}

impl EnrollmentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enrolls a document via the oracle. Returns the fresh label, or a duplicate
    /// error if this person (any source) already enrolled.
    pub fn enroll(
        &mut self,
        doc: &dyn IdentityDocument,
        oracle: &dyn UniquenessOracle,
    ) -> Result<Label, DuplicateEnrollment> {
        let label = oracle.label(&doc.canonical_anchor());
        if self.used.contains(&label) {
            Err(DuplicateEnrollment)
        } else {
            self.used.insert(label.clone());
            Ok(label)
        }
    }

    pub fn is_enrolled(&self, label: &Label) -> bool {
        self.used.contains(label)
    }
}
