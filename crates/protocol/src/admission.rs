//! The Sybil-resistant admission boundary (`docs/08` PROTO-007 / G-04 / INV-9, T6).
//!
//! Every real entry point requires a verified role nullifier proof and keys the
//! participant's identity on the **proven** nullifier ([`NullifierProof::id`]), never on
//! the unproven `nym::derive_nym`. A bare or forged `Nym` carries no proof, so it cannot
//! act ([`admit`] rejects it — AT-PRO-01); a proof is bound to its action context, so it
//! cannot be lifted onto another action (AT-ID-05).
//!
//! Scope: this is the in-process identity gate. The per-credential proposal quota (ID-008)
//! plugs into the same boundary as its cryptographic-grade form under T11/T20; it needs
//! the credential secret, which does not cross this boundary, so it is not enforced here.

use identity::credential::IssuerPublic;
use identity::nullifier::{verify, NullifierProof};
use identity::nym::{Nym, Role};
use identity::ratelimit::within_quota;
use std::collections::{HashMap, HashSet};

/// Why a presented identity proof is not admissible.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unproven {
    /// The proof does not verify for this issuer and this action context (a forged or
    /// replayed proof, or one for the wrong action).
    BadProof,
    /// The proof is for a different role than the action requires.
    WrongRole,
}

/// Verifies a role nullifier proof for one action and returns the participant's stable
/// protocol id (INV-9). `context` binds the proof to this action, so a proof made for a
/// different action does not verify here (AT-ID-05).
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

/// A second action by a role-nullifier that has already acted in this context.
#[derive(Debug, PartialEq, Eq)]
pub struct DuplicateNullifier;

/// The role-nullifiers that have already acted in one context — e.g. the review panel
/// for a single item. Keys on the verified nullifier id, so one person cannot act twice
/// under one role (INV-9 rate-limit keying); mirrors `identity::ratelimit::SlotLedger`.
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

    /// How many distinct role-nullifiers have acted in this context: the count the pilot
    /// floors read (T65) — persons, never rows.
    pub fn len(&self) -> usize {
        self.seen.len()
    }

    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }
}

/// Over the per-credential proposal quota for the epoch.
#[derive(Debug, PartialEq, Eq)]
pub struct OverQuota;

/// Per-credential proposal quota for one epoch (`docs/08` ID-008; the structural
/// in-process form — the RLN cryptographic-grade form is T20). Counts proposals per
/// **proposer nullifier id** (INV-9, never `derive_nym`), so a person's proposals are
/// bounded across their unlinkable Propose actions. The quota is set by the caller from
/// the author score `C_a` (`scoring::reputation::proposal_rate`) — the cost of proposing
/// is reputation and a rate limit, never money (invariant #3). Use one ledger per epoch.
#[derive(Default)]
pub struct QuotaLedger {
    used: HashMap<Nym, u32>,
}

impl QuotaLedger {
    pub fn new() -> Self {
        Self::default()
    }

    /// Charges one proposal to `proposer` against `quota`, rejecting once the count would
    /// exceed it. Reuses [`identity::ratelimit::within_quota`] for the bound.
    pub fn charge(&mut self, proposer: Nym, quota: u32) -> Result<(), OverQuota> {
        let used = self.used.entry(proposer).or_insert(0);
        if !within_quota(*used, quota) {
            return Err(OverQuota);
        }
        *used += 1;
        Ok(())
    }

    /// How many proposals `proposer` has made this epoch.
    pub fn used(&self, proposer: &Nym) -> u32 {
        self.used.get(proposer).copied().unwrap_or(0)
    }
}
