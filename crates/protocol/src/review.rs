//! [4] Review (`docs/05`): reviewers are assigned RANDOMLY and stratified on the
//! latent position f_u, so the panel mirrors every position of the axis and no one
//! picks what to review (anti-brigading). Judgments are committed then revealed, so
//! no one can copy others or ride an information cascade.

use crate::admission::{admit, DuplicateNullifier, NullifierSet, Unproven};
use identity::credential::IssuerPublic;
use identity::nullifier::NullifierProof;
use identity::nym::{Nym, Role};
use network::cid::Cid;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use sha2::{Digest, Sha256};

/// A candidate reviewer: role pseudonym and latent position from Level A.
#[derive(Clone, Copy, Debug)]
pub struct Reviewer {
    pub nym: Nym,
    pub f_u: f64,
}

/// Picks `k` reviewers stratified across f_u: sort by position, split into `k`
/// strata, draw one per stratum. Deterministic per `(item_seed)`.
pub fn assign_reviewers(reviewers: &[Reviewer], k: usize, item_seed: u64) -> Vec<Reviewer> {
    let n = reviewers.len();
    if n == 0 || k == 0 {
        return Vec::new();
    }
    let k = k.min(n);
    let mut sorted: Vec<Reviewer> = reviewers.to_vec();
    // `total_cmp`: a NaN position sorts last instead of panicking (docs/08 IQ-2).
    sorted.sort_by(|a, b| a.f_u.total_cmp(&b.f_u));

    let mut rng = ChaCha8Rng::seed_from_u64(item_seed);
    let mut chosen = Vec::with_capacity(k);
    for s in 0..k {
        let lo = s * n / k;
        let hi = ((s + 1) * n / k).max(lo + 1);
        let stratum = &sorted[lo..hi.min(n)];
        chosen.push(*stratum.choose(&mut rng).unwrap());
    }
    chosen
}

/// A hiding commitment to a judgment probability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Commit(pub [u8; 32]);

/// commit = H(prob, nonce). The reviewer declares a probability that the item
/// passes Level B (`docs/02` C.2), not a yes/no.
pub fn commit(prob: f64, nonce: &[u8; 32]) -> Commit {
    let mut h = Sha256::new();
    h.update(b"isegoria/commit/v1");
    h.update(prob.to_le_bytes());
    h.update(nonce);
    Commit(h.finalize().into())
}

/// Checks a revealed (prob, nonce) against its commitment.
pub fn reveal(commitment: Commit, prob: f64, nonce: &[u8; 32]) -> bool {
    commit(prob, nonce) == commitment
}

/// The action context a review proof is bound to: this item and epoch (AT-ID-05). A
/// reviewer proves against exactly this, so a proof made for one item cannot be replayed
/// onto another.
pub fn review_context(item: Cid, epoch: u64) -> Vec<u8> {
    let mut ctx = Vec::with_capacity(40);
    ctx.extend_from_slice(&item.0);
    ctx.extend_from_slice(&epoch.to_le_bytes());
    ctx
}

/// The identity-gated review entry point (`docs/08` §9.1, INV-9, T6): the reviewer
/// presents a `NullifierProof(Judge)` bound to this item and epoch, so it cannot be
/// replayed onto another item (AT-ID-05). The proven id is recorded in `panel`, rejecting
/// a second judgment by the same role-nullifier on this item. Returns the reviewer's
/// proven, non-rotatable id — the id the panel and reputation key on, never
/// `nym::derive_nym` (a bare `Nym` has no proof and is refused, AT-PRO-01).
pub fn submit_review(
    proof: &NullifierProof,
    issuer: &IssuerPublic,
    item: Cid,
    epoch: u64,
    panel: &mut NullifierSet,
) -> Result<Nym, ReviewRejected> {
    let id = admit(proof, issuer, Role::Judge, &review_context(item, epoch))?;
    panel.spend(id)?;
    Ok(id)
}

/// Why a submitted review was refused at the identity-gated entry point.
#[derive(Debug, PartialEq, Eq)]
pub enum ReviewRejected {
    /// No valid `Judge` nullifier proof for this item and epoch.
    Unproven(Unproven),
    /// This role-nullifier already reviewed this item.
    Duplicate,
}

impl From<Unproven> for ReviewRejected {
    fn from(u: Unproven) -> Self {
        ReviewRejected::Unproven(u)
    }
}

impl From<DuplicateNullifier> for ReviewRejected {
    fn from(_: DuplicateNullifier) -> Self {
        ReviewRejected::Duplicate
    }
}
