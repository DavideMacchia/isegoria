//! Enrollment pipeline (`docs/03` §M1–M2): every source converges on one anchor, turned
//! into a uniqueness label ([`VoprfOracle`], single-server; threshold is the target).

use crate::hash::tagged;
use rand_core::OsRng;
use sha2::{Digest, Sha512};
use std::collections::HashSet;
use std::fmt;
use voprf::{Ristretto255, VoprfClient, VoprfServer};

#[derive(Clone, PartialEq, Eq)]
pub struct Anchor(pub String);

impl fmt::Debug for Anchor {
    /// Redacted: the anchor is the codice fiscale — PII (invariant #1, PV-4).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Anchor(..)")
    }
}

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

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Label(pub [u8; 32]);

impl fmt::Debug for Label {
    /// Redacted: the uniqueness label is a per-person id (PV-4, docs/08 §8.3).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Label(..)")
    }
}

/// Oblivious PRF over the anchor (`docs/03` §M1): the server never sees it in the
/// clear. [`VoprfOracle`] is real; [`ReferenceOracle`] is test-only.
pub trait UniquenessOracle {
    fn label(&self, anchor: &Anchor) -> Label;
}

/// Test-only bare keyed hash — NOT secure; prefer [`VoprfOracle`] everywhere else.
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
/// Longest input RFC 9497 accepts: its length is encoded in two bytes.
const VOPRF_MAX_INPUT: usize = u16::MAX as usize;

/// Single-server VOPRF (RFC 9497, Ristretto255-SHA512): `label` returns `F(k, anchor)`,
/// independent of the blind. Threshold across the committee is the spec's target (`docs/03` §M1).
pub struct VoprfOracle {
    server: VoprfServer<Ristretto255>,
}

impl VoprfOracle {
    /// Derives the server key deterministically from `seed`, so labels are reproducible.
    pub fn new(seed: [u8; 32]) -> Self {
        let server = VoprfServer::<Ristretto255>::new_from_seed(&seed, VOPRF_INFO)
            .expect("32-byte seed derives a valid Ristretto255 VOPRF key");
        VoprfOracle { server }
    }
}

impl UniquenessOracle for VoprfOracle {
    fn label(&self, anchor: &Anchor) -> Label {
        let anchor = anchor.0.as_bytes();
        // Longer than the RFC 9497 cap: hash to 64 bytes first, with its own tag, so a
        // long anchor's label cannot equal a short one's.
        let digest;
        let (input, tag) = if anchor.len() <= VOPRF_MAX_INPUT {
            (anchor, "isegoria/uniqueness/voprf/v1")
        } else {
            digest = Sha512::digest(anchor);
            (digest.as_slice(), "isegoria/uniqueness/voprf/long/v1")
        };
        let mut rng = OsRng;
        // 1. Client blinds the anchor; only the blinded element goes to the server.
        let blind = VoprfClient::<Ristretto255>::blind(input, &mut rng)
            .expect("the input is at most u16::MAX bytes");
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
        Label(tagged(tag, &[output.as_slice()]))
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

    /// Enrolls via the oracle; errs with `DuplicateEnrollment` if already enrolled.
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
