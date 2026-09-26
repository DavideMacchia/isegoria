//! Review (`docs/05` [4]): reviewers are assigned randomly, stratified on the latent
//! position `f_u`, so no one picks what to review; judgments are committed then
//! revealed, so no one can copy others (`docs/01` D39/D40, T57).

use crate::admission::{admit, DuplicateNullifier, NullifierSet, Unproven};
use identity::credential::IssuerPublic;
use identity::nullifier::NullifierProof;
use identity::nym::{Nym, Role};
use network::cid::Cid;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug)]
pub struct Reviewer {
    pub nym: Nym,
    pub f_u: f64,
}

/// Reviewer assignment seeded from the epoch's beacon (INV-10), keyed on the item's
/// admitted `slot` — never the draft bytes — so an author cannot steer the panel (AT-BR-05).
/// Sanctioned entry point; [`assign_reviewers`] takes a raw seed for testing.
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

/// Extra reviewers of the band's second round (D26): drawn from the beacon like the
/// first panel, stratified on `f_u`, never from it. Provisional size [`K_EXTRA`] (T25);
/// `slot` is the item's admitted slot, as for the first panel.
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

/// The provisional size of the band's extra panel (D26, T25).
pub const K_EXTRA: usize = 4;

/// Panel assignment under D40 (T57): stratified on `f_u`, with at most one member of
/// each coordination cluster (`clusters[i]` keys `reviewers[i]`); `taken` (with its
/// clusters) is excluded. Deterministic per `seed`; short if too few are eligible.
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

/// Picks `k` reviewers stratified across f_u, one per stratum. Deterministic per `item_seed`.
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Commit(pub [u8; 32]);

/// `commit = H(prob, nonce, committer, item)` (`docs/02` C.2). Binds the committer's
/// nullifier id and the item cid (INV-12): a commitment copied onto another reviewer
/// or item cannot be opened (AT-BR-06).
pub fn commit(prob: f64, nonce: &[u8; 32], committer: Nym, item: Cid) -> Commit {
    let mut h = Sha256::new();
    h.update(b"isegoria/commit/v2");
    h.update(prob.to_le_bytes());
    h.update(nonce);
    h.update(committer.0);
    h.update(item.0);
    Commit(h.finalize().into())
}

/// Checks a reveal against its commitment; another committer or item fails (INV-12, AT-BR-06).
pub fn reveal(commitment: Commit, prob: f64, nonce: &[u8; 32], committer: Nym, item: Cid) -> bool {
    commit(prob, nonce, committer, item) == commitment
}

pub fn review_context(item: Cid, epoch: u64) -> Vec<u8> {
    let mut ctx = Vec::with_capacity(40);
    ctx.extend_from_slice(&item.0);
    ctx.extend_from_slice(&epoch.to_le_bytes());
    ctx
}

/// The identity-gated review entry point (`docs/08` §9.1, INV-9, T6): records the
/// proven `Judge` id in `panel`, rejecting a second judgment by the same role-nullifier
/// on this item. Returns the proven, non-rotatable id the panel and reputation key on.
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

#[derive(Debug, PartialEq, Eq)]
pub enum ReviewRejected {
    Unproven(Unproven),
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
