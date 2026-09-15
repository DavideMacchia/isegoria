//! Anonymous credential and its (blind, threshold) issuance. See `docs/03`, §M2.

use crate::enrollment::Label;
use crate::nym::{derive_nym, Nym, Role};

/// The root secret a node holds. The three role nyms derive from it; it never
/// leaves the holder.
#[derive(Clone, Debug)]
pub struct Credential {
    secret: [u8; 32],
}

impl Credential {
    /// The holder picks its own secret with a CSPRNG; the issuer must not learn it
    /// (that is what blind issuance guarantees).
    pub fn from_secret(secret: [u8; 32]) -> Self {
        Credential { secret }
    }

    pub fn nym(&self, role: Role) -> Nym {
        derive_nym(&self.secret, role)
    }

    pub(crate) fn secret(&self) -> &[u8; 32] {
        &self.secret
    }
}

/// Blind, threshold issuance (`docs/03`, §M2): the committee certifies eligibility
/// for a fresh uniqueness label without learning the holder's secret or being able
/// to recognize the credential later. Production wires this to BBS+ over a t-of-n
/// committee; implementors provide a real backend.
pub trait BlindIssuer {
    type Attestation;

    /// Issue against a label already accepted as unique by the registry.
    fn issue(&self, label: &Label, credential: &Credential) -> Self::Attestation;
}

/// Non-production reference issuer for pipeline tests only: it echoes an opaque tag
/// and, crucially, is given the credential by reference but records nothing linking
/// it to the label. It provides NO unlinkability guarantee on its own.
pub struct ReferenceIssuer;

impl BlindIssuer for ReferenceIssuer {
    type Attestation = [u8; 32];

    fn issue(&self, label: &Label, credential: &Credential) -> [u8; 32] {
        crate::hash::tagged("isegoria/attestation/v1", &[&label.0, credential.secret()])
    }
}
