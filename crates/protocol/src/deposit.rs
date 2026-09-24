//! [2] Deposit (`docs/05`): the draft is content-addressed and its hash recorded
//! on the append-only log. The bond is in reputation, never money (invariant #3).

use crate::admission::{admit, OverQuota, QuotaLedger, Unproven};
use identity::credential::IssuerPublic;
use identity::nullifier::NullifierProof;
use identity::nym::{Nym, Role};
use network::cid::{cid, Cid};
use network::log::TransparencyLog;

/// A draft to deposit: the item text and its mandatory primary source, in the
/// structured form the identity layer normalizes for anonymity.
pub struct Draft {
    pub item: Vec<u8>,
    pub primary_source: Vec<u8>,
}

impl Draft {
    pub fn content_id(&self) -> Cid {
        // Length-prefix each field before hashing. Plain concatenation makes the
        // fields ambiguous — `("ab", "c")` and `("a", "bc")` both hash `"abc"` and
        // collide (PROTO-011); the length prefixes make the boundary unambiguous.
        let mut buf = Vec::with_capacity(16 + self.item.len() + self.primary_source.len());
        buf.extend_from_slice(&(self.item.len() as u64).to_le_bytes());
        buf.extend_from_slice(&self.item);
        buf.extend_from_slice(&(self.primary_source.len() as u64).to_le_bytes());
        buf.extend_from_slice(&self.primary_source);
        cid(&buf)
    }
}

/// Records the draft on the log and returns its content id. The primary source is
/// mandatory: a draft without one is not depositable. A content id already on the log
/// is refused (`docs/08` §9.1 row 1, T64): a draft is deposited once.
///
/// This is the pure record step. A real proposal MUST go through
/// [`deposit_with_identity`], which proves the proposer's identity first (INV-9).
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

/// The action context a `Propose` proof is bound to: this draft's content id and the
/// epoch (AT-ID-05, T64), as `review::review_context` does for a review. A proof made
/// for one draft cannot be replayed onto another, and one made in epoch `e` does not
/// verify in `e + 1`.
pub fn deposit_context(item: Cid, epoch: u64) -> Vec<u8> {
    let mut ctx = Vec::with_capacity(40);
    ctx.extend_from_slice(&item.0);
    ctx.extend_from_slice(&epoch.to_le_bytes());
    ctx
}

/// The identity-gated, rate-limited proposal entry point (`docs/08` §9.1, INV-9/ID-008,
/// T6/T11/T64): the proposer presents a `NullifierProof(Propose)` **bound to this exact
/// draft and epoch** ([`deposit_context`], so the proof cannot be replayed onto another
/// draft or into a later epoch, AT-ID-05), and the proposal is charged against a
/// per-credential epoch `quota` keyed on the proven id (over quota → rejected). `quota`
/// is set by the caller from the author score `C_a`
/// (`scoring::reputation::proposal_rate`): the cost of proposing is reputation and a rate
/// limit, never money (invariant #3).
///
/// A content id already on the log is refused **before** the identity check and the
/// quota charge (T64): whoever replays a proposal seen in transit appends nothing and
/// costs its author nothing. On success the draft is recorded and the proposer's proven,
/// non-rotatable id is returned.
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

/// Why a proposal was refused at the deposit entry points.
#[derive(Debug, PartialEq, Eq)]
pub enum DepositRejected {
    NoPrimarySource,
    /// The draft's content id is already on the log (`docs/08` §9.1 row 1, T64): a replay
    /// of an earlier deposit, refused before the identity check and the quota charge.
    DuplicateCid,
    /// The proposer did not present a valid `Propose` nullifier proof for this draft and
    /// epoch.
    Unproven(Unproven),
    /// The proposer is over its per-credential proposal quota for the epoch (ID-008).
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
