//! [4] Review (`docs/05`): reviewers are assigned RANDOMLY and stratified on the
//! latent position f_u, so the panel mirrors every position of the axis and no one
//! picks what to review (anti-brigading). Judgments are committed then revealed, so
//! no one can copy others or ride an information cascade.
//!
//! A detected coordination cluster (`scoring::collusion::coordination_clusters`, D39)
//! constrains the assignment, not the weights (`docs/01` D40, T57): a panel holds at
//! most one member of each cluster ([`assign_diverse`]), the band's extra round included,
//! and nobody's weight changes — the protocol never applies the sublinear discount.

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

/// Reviewer assignment seeded from the signed checkpoint (INV-10, D29, T8), keyed on the
/// item's admitted `slot` — a byte-independent index, never the draft bytes — so an author
/// cannot regenerate the draft to select a panel (AT-BR-05), and cannot predict the beacon
/// to steer it. Sanctioned entry point; [`assign_reviewers`] takes a raw seed for testing.
pub fn assign_from_beacon(
    reviewers: &[Reviewer],
    k: usize,
    beacon: &crate::randomness::Beacon,
    slot: u64,
) -> Vec<Reviewer> {
    assign_reviewers(
        reviewers,
        k,
        beacon.seed(crate::randomness::REVIEW_ASSIGNMENT, slot),
    )
}

/// Extra reviewers of the band's second round (`docs/01` D26, T60): drawn from the
/// beacon like the first panel, stratified on `f_u`, and never from the first panel —
/// the re-decision must add evidence, not re-weigh the same. Provisional size
/// [`K_EXTRA`] (T25); `slot` is the item's admitted slot, as for the first panel.
pub fn assign_extra_from_beacon(
    reviewers: &[Reviewer],
    first_panel: &[Nym],
    k_extra: usize,
    beacon: &crate::randomness::Beacon,
    slot: u64,
) -> Vec<Reviewer> {
    let outside: Vec<Reviewer> = reviewers
        .iter()
        .filter(|r| !first_panel.contains(&r.nym))
        .copied()
        .collect();
    assign_reviewers(
        &outside,
        k_extra,
        beacon.seed(crate::randomness::EXTRA_REVIEW, slot),
    )
}

/// The provisional size of the band's extra panel (D26, T60): four more reviewers — a
/// panel of nine grows by almost half — to be calibrated with the band width (T25).
pub const K_EXTRA: usize = 4;

/// Panel assignment under D40 (T57): stratified on `f_u` as [`assign_reviewers`], with at
/// most one member of each coordination cluster on the panel. `clusters[i]` is the
/// cluster id of `reviewers[i]` (`scoring::collusion::CoordinationReport::clusters`; a
/// singleton's id is its own). `taken` are the nyms already on the panel — for the band's
/// extra round (T60) the first panel — excluded with their clusters. A stratum whose
/// every member is excluded is filled by the eligible reviewer nearest to it on the
/// axis; with nobody eligible left the panel is shorter, and the lifecycle refuses it.
/// Deterministic per `seed`.
pub fn assign_diverse(
    reviewers: &[Reviewer],
    clusters: &[usize],
    taken: &[Nym],
    k: usize,
    seed: u64,
) -> Vec<Reviewer> {
    let n = reviewers.len();
    if n == 0 || k == 0 || clusters.len() != n {
        return Vec::new();
    }
    let mut order: Vec<usize> = (0..n).collect();
    // `total_cmp`: a NaN position sorts last instead of panicking (docs/08 IQ-2).
    order.sort_by(|&a, &b| reviewers[a].f_u.total_cmp(&reviewers[b].f_u));

    let mut used_clusters: Vec<usize> = reviewers
        .iter()
        .zip(clusters)
        .filter(|(r, _)| taken.contains(&r.nym))
        .map(|(_, &c)| c)
        .collect();
    let mut chosen_idx: Vec<usize> = Vec::with_capacity(k);
    let eligible = |i: usize, used: &[usize], chosen: &[usize]| {
        !taken.contains(&reviewers[i].nym) && !used.contains(&clusters[i]) && !chosen.contains(&i)
    };

    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let k = k.min(n);
    for s in 0..k {
        let lo = s * n / k;
        let hi = ((s + 1) * n / k).max(lo + 1).min(n);
        let stratum: Vec<usize> = order[lo..hi]
            .iter()
            .copied()
            .filter(|&i| eligible(i, &used_clusters, &chosen_idx))
            .collect();
        let pick = match stratum.choose(&mut rng) {
            Some(&i) => Some(i),
            None => {
                // The nearest eligible reviewer to the stratum's centre on the axis.
                let centre = reviewers[order[(lo + hi - 1) / 2]].f_u;
                order
                    .iter()
                    .copied()
                    .filter(|&i| eligible(i, &used_clusters, &chosen_idx))
                    .min_by(|&a, &b| {
                        (reviewers[a].f_u - centre)
                            .abs()
                            .total_cmp(&(reviewers[b].f_u - centre).abs())
                    })
            }
        };
        if let Some(i) = pick {
            used_clusters.push(clusters[i]);
            chosen_idx.push(i);
        }
    }
    chosen_idx.into_iter().map(|i| reviewers[i]).collect()
}

/// [`assign_diverse`] seeded from the beacon (INV-10) for an item's first panel: the
/// sanctioned entry point under D40.
pub fn assign_diverse_from_beacon(
    reviewers: &[Reviewer],
    clusters: &[usize],
    k: usize,
    beacon: &crate::randomness::Beacon,
    slot: u64,
) -> Vec<Reviewer> {
    assign_diverse(
        reviewers,
        clusters,
        &[],
        k,
        beacon.seed(crate::randomness::REVIEW_ASSIGNMENT, slot),
    )
}

/// The band's extra panel under D40 (T57, T60): outside the first panel *and* outside
/// its members' clusters, on the extra round's beacon domain.
pub fn assign_extra_diverse_from_beacon(
    reviewers: &[Reviewer],
    clusters: &[usize],
    first_panel: &[Nym],
    k_extra: usize,
    beacon: &crate::randomness::Beacon,
    slot: u64,
) -> Vec<Reviewer> {
    assign_diverse(
        reviewers,
        clusters,
        first_panel,
        k_extra,
        beacon.seed(crate::randomness::EXTRA_REVIEW, slot),
    )
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

/// A hiding commitment to a judgment probability, bound to its committer and item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Commit(pub [u8; 32]);

/// `commit = H(prob, nonce, committer, item)`. The reviewer declares a probability that
/// the item passes Level B (`docs/02` C.2), not a yes/no. The commitment **binds the
/// committer's nullifier id and the item cid** (`docs/08` CRYPTO-007 / INV-12, T7): a
/// commitment copied from another reviewer, or lifted onto another item, cannot be opened
/// — only the same `(committer, item)` recomputes it (AT-BR-06).
pub fn commit(prob: f64, nonce: &[u8; 32], committer: Nym, item: Cid) -> Commit {
    let mut h = Sha256::new();
    h.update(b"isegoria/commit/v2");
    h.update(prob.to_le_bytes());
    h.update(nonce);
    h.update(committer.0);
    h.update(item.0);
    Commit(h.finalize().into())
}

/// Checks a revealed `(prob, nonce)` against a commitment, for the claimed committer and
/// item. Fails if the opener is not the committer or the item differs (INV-12, AT-BR-06).
pub fn reveal(commitment: Commit, prob: f64, nonce: &[u8; 32], committer: Nym, item: Cid) -> bool {
    commit(prob, nonce, committer, item) == commitment
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
