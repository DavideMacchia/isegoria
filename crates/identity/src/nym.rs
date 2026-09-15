//! Role nullifiers (M3). See `docs/03`, §M3.
//!
//! `nym = H(secret, context)`: always equal for the same role (no second identity
//! per role → no whitewashing), not reversible to the secret, and different across
//! roles (the three role pseudonyms are unlinkable).

use crate::hash::tagged;

/// The three actions a person can take, each under a separate pseudonym.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    Propose,
    Judge,
    Respond,
}

impl Role {
    pub(crate) fn tag(self) -> &'static str {
        match self {
            Role::Propose => "propose",
            Role::Judge => "judge",
            Role::Respond => "respond",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Nym(pub [u8; 32]);

/// Deterministic pseudonym for one role. In production this is a Semaphore-style
/// nullifier proven in zero knowledge; the derivation contract is the same.
pub fn derive_nym(secret: &[u8; 32], role: Role) -> Nym {
    Nym(tagged("isegoria/nym/v1", &[secret, role.tag().as_bytes()]))
}
