//! The Sybil-resistant admission boundary (`docs/08` PROTO-007/G-04, INV-9, T6): every
//! entry point keys identity on the **proven** nullifier ([`NullifierProof::id`]), never
//! on the unproven `nym::derive_nym`; a proof is bound to its action context (AT-ID-05).

use identity::credential::IssuerPublic;
use identity::nullifier::{verify, NullifierProof};
use identity::nym::{Nym, Role};
use identity::ratelimit::within_quota;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unproven {
    BadProof,
    WrongRole,
}

/// Verifies a role nullifier proof for one action; `context` binds it (INV-9, AT-ID-05).
pub fn admit(
    proof: &NullifierProof,
    issuer: &IssuerPublic,
    expected_role: Role,
    context: &[u8],
) -> Result<Nym, Unproven> {
    if proof.role() != expected_role {
        return Err(Unproven::WrongRole);
    }
    if !verify(proof, issuer, context) {
        return Err(Unproven::BadProof);
    }
    Ok(proof.id())
}

#[derive(Debug, PartialEq, Eq)]
pub struct DuplicateNullifier;

/// Role nullifiers that already acted in one context, keyed on the verified id (INV-9).
#[derive(Default)]
pub struct NullifierSet {
    seen: HashSet<Nym>,
}

impl NullifierSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records an admitted id, rejecting a repeat under the same role in this context.
    pub fn spend(&mut self, id: Nym) -> Result<(), DuplicateNullifier> {
        if self.seen.insert(id) {
            Ok(())
        } else {
            Err(DuplicateNullifier)
        }
    }

    pub fn contains(&self, id: &Nym) -> bool {
        self.seen.contains(id)
    }

    pub fn len(&self) -> usize {
        self.seen.len()
    }

    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct OverQuota;

/// Per-credential proposal quota for one epoch (`docs/08` ID-008): counts by proposer
/// nullifier id (INV-9), set by the caller from `C_a` (`scoring::reputation::proposal_rate`).
#[derive(Default)]
pub struct QuotaLedger {
    used: HashMap<Nym, u32>,
}

impl QuotaLedger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn charge(&mut self, proposer: Nym, quota: u32) -> Result<(), OverQuota> {
        let used = self.used.entry(proposer).or_insert(0);
        if !within_quota(*used, quota) {
            return Err(OverQuota);
        }
        *used += 1;
        Ok(())
    }

    pub fn used(&self, proposer: &Nym) -> u32 {
        self.used.get(proposer).copied().unwrap_or(0)
    }
}
