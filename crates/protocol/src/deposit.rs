//! [2] Deposit (`docs/05`): the draft is content-addressed and its hash recorded
//! on the append-only log. The bond is in reputation, never money (invariant #3).

use crate::admission::{admit, Unproven};
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
/// mandatory: a draft without one is not depositable.
///
/// This is the pure record step. A real proposal MUST go through
/// [`deposit_with_identity`], which proves the proposer's identity first (INV-9).
pub fn deposit(log: &mut TransparencyLog, draft: &Draft) -> Result<Cid, NoPrimarySource> {
    if draft.primary_source.is_empty() {
        return Err(NoPrimarySource);
    }
    let id = draft.content_id();
    log.append(id);
    Ok(id)
}

/// The identity-gated proposal entry point (`docs/08` §9.1, INV-9, T6): the proposer
/// presents a `NullifierProof(Propose)` **bound to this exact draft** (context = its
/// content id, so the proof cannot be replayed onto another draft, AT-ID-05). On success
/// the draft is recorded and the proposer's proven, non-rotatable id is returned — the id
/// a reputation/rate-limit layer keys on, never `nym::derive_nym`.
pub fn deposit_with_identity(
    log: &mut TransparencyLog,
    draft: &Draft,
    proof: &NullifierProof,
    issuer: &IssuerPublic,
) -> Result<(Cid, Nym), DepositRejected> {
    if draft.primary_source.is_empty() {
        return Err(DepositRejected::NoPrimarySource);
    }
    let id = draft.content_id();
    let proposer = admit(proof, issuer, Role::Propose, &id.0)?;
    log.append(id);
    Ok((id, proposer))
}

#[derive(Debug, PartialEq, Eq)]
pub struct NoPrimarySource;

/// Why a proposal was refused at the identity-gated entry point.
#[derive(Debug, PartialEq, Eq)]
pub enum DepositRejected {
    NoPrimarySource,
    /// The proposer did not present a valid `Propose` nullifier proof for this draft.
    Unproven(Unproven),
}

impl From<Unproven> for DepositRejected {
    fn from(u: Unproven) -> Self {
        DepositRejected::Unproven(u)
    }
}
