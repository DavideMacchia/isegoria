//! Enrollment pipeline (M1, M2). See `docs/03`, §M1–M2.
//!
//! Every source converges on one canonical anchor (the codice fiscale in Italy);
//! a threshold OPRF turns it into a uniqueness label that the user cannot compute
//! alone and no single issuer can read. A second enrollment yields the same label
//! and is rejected as a duplicate — regardless of which source it came from.

use crate::hash::tagged;
use std::collections::HashSet;

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

/// Threshold OPRF over the anchor (`docs/03`, §M1). The anchor space is small and
/// brute-forceable, so the label must be computable only with the committee's help:
/// no issuer learns the anchor or the label, and the user cannot derive it alone.
pub trait UniquenessOracle {
    fn label(&self, anchor: &Anchor) -> Label;
}

/// Non-production reference oracle: a keyed hash. It stands in for the threshold
/// OPRF so the registry can be tested; it is NOT secure (a single holder of the key
/// can compute and invert usage). Production replaces it with a real threshold OPRF.
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
