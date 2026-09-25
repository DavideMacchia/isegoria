//! [2] Deposit (`docs/05`): the draft is content-addressed and its hash recorded
//! on the append-only log. The bond is in reputation, never money (invariant #3).

use crate::admission::{admit, OverQuota, QuotaLedger, Unproven};
use identity::credential::IssuerPublic;
use identity::nullifier::NullifierProof;
use identity::nym::{Nym, Role};
use network::cid::{cid, Cid};
use network::log::TransparencyLog;

pub struct Draft {
    pub item: Vec<u8>,
    pub primary_source: Vec<u8>,
}

impl Draft {
    pub fn content_id(&self) -> Cid {
        // Length-prefixes avoid concatenation ambiguity: `("ab","c")` and `("a","bc")`
        // both hash `"abc"` and would collide (PROTO-011).
        let mut buf = Vec::with_capacity(16 + self.item.len() + self.primary_source.len());
        buf.extend_from_slice(&(self.item.len() as u64).to_le_bytes());
        buf.extend_from_slice(&self.item);
        buf.extend_from_slice(&(self.primary_source.len() as u64).to_le_bytes());
        buf.extend_from_slice(&self.primary_source);
        cid(&buf)
    }
}

/// Records the draft on the log (`docs/08` §9.1 row 1, T64): refused without a primary
/// source, or if its content id is already on the log. The pure record step.
pub fn deposit(log: &mut TransparencyLog, draft: &Draft) -> Result<Cid, DepositRejected> {
    if draft.primary_source.is_empty() {
        return Err(DepositRejected::NoPrimarySource);
    }
    let id = draft.content_id();
    if log.contains(&id) {
        return Err(DepositRejected::DuplicateCid);
    }
    log.append(id);
    Ok(id)
}

pub fn deposit_context(item: Cid, epoch: u64) -> Vec<u8> {
    let mut ctx = Vec::with_capacity(40);
    ctx.extend_from_slice(&item.0);
    ctx.extend_from_slice(&epoch.to_le_bytes());
    ctx
}

/// The identity-gated, rate-limited proposal entry point (`docs/08` §9.1, INV-9/ID-008):
/// charges the epoch `quota` against the proven `Propose` id (T64).
pub fn deposit_with_identity(
    log: &mut TransparencyLog,
    draft: &Draft,
    proof: &NullifierProof,
    issuer: &IssuerPublic,
    epoch: u64,
    quota_ledger: &mut QuotaLedger,
    quota: u32,
) -> Result<(Cid, Nym), DepositRejected> {
    if draft.primary_source.is_empty() {
        return Err(DepositRejected::NoPrimarySource);
    }
    let id = draft.content_id();
    if log.contains(&id) {
        return Err(DepositRejected::DuplicateCid);
    }
    let proposer = admit(proof, issuer, Role::Propose, &deposit_context(id, epoch))?;
    quota_ledger.charge(proposer, quota)?;
    log.append(id);
    Ok((id, proposer))
}

#[derive(Debug, PartialEq, Eq)]
pub enum DepositRejected {
    NoPrimarySource,
    DuplicateCid,
    Unproven(Unproven),
    OverQuota,
}

impl From<Unproven> for DepositRejected {
    fn from(u: Unproven) -> Self {
        DepositRejected::Unproven(u)
    }
}

impl From<OverQuota> for DepositRejected {
    fn from(_: OverQuota) -> Self {
        DepositRejected::OverQuota
    }
}
