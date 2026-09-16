//! [4]/[5] Review aggregation (`docs/05` [4]/[5], `docs/02` §C.2).
//!
//! Composes the existing bricks into the actual decision: reviewers' revealed
//! pass-probabilities are combined into a single signal that resolves the bridging
//! gate's uncertainty band — **weighted by the evaluator score `E_u`** (probation-
//! gated and capped) and **discounted for collusion** so a coordinated block of `k`
//! reviewers counts like `√k` independents.
//!
//! Two invariants are load-bearing here and are respected by construction:
//! - **#2 — quality is never a majority vote.** The output is a *weighted mean of
//!   probabilities* (weights are `E_u`, not head-counts) with the cartel discount
//!   applied; there is no `positive > negative` anywhere. It only resolves the
//!   uncertainty band the bridging model already flags.
//! - **#4 — the two reputation scores are never merged.** This module uses only the
//!   evaluator score `E_u`. It never takes or imports the author score `C_a`.

use scoring::collusion::{cluster_by_correlation, correlation_matrix, discount_weights, ALPHA};

use crate::probation::effective_review_weight;

/// Default decision threshold for [`resolve_band`]: a weighted pass-probability at or
/// above 0.5 resolves the band in favour of the item.
pub const DECISION_THRESHOLD: f64 = 0.5;

/// Per-reviewer vote weight for the aggregation.
///
/// 1. base weight = [`effective_review_weight`] (0 on probation, 1 for a bootstrap
///    founder, `min(w_max, E_u)` once established);
/// 2. cluster reviewers by the correlation of their judgment vectors; a **coordinated
///    block** (a cluster of two or more) has its combined weight shrunk sub-linearly
///    to `(Σ w)^α` (`α ≈ 0.5`, so `k` clones count like `√k`), while an **independent**
///    reviewer (a singleton cluster) keeps its full weight.
///
/// Restricting the shrink to real clusters matters: [`discount_weights`] maps *every*
/// cluster (singletons included) through `w ↦ w^α`, which would distort an honest
/// reviewer's legitimate `E_u`-based weight (inflating sub-unit weights, compressing
/// large ones). The design's rule is "independent nodes untouched, coordinated blocks
/// discounted" — so singletons are left at their base weight here.
///
/// All input slices are indexed by reviewer and must have the same length; each
/// `judgment_vectors[u]` is that reviewer's votes across a shared set of items (the
/// basis for detecting coordination).
pub fn review_weights(
    is_founder: &[bool],
    judgments_with_outcome: &[usize],
    e_u: &[f64],
    w_max: f64,
    judgment_vectors: &[Vec<f64>],
    corr_threshold: f64,
) -> Vec<f64> {
    let n = is_founder.len();
    debug_assert!(
        judgments_with_outcome.len() == n && e_u.len() == n && judgment_vectors.len() == n,
        "per-reviewer slices must share the same length"
    );

    let base: Vec<f64> = (0..n)
        .map(|u| effective_review_weight(is_founder[u], judgments_with_outcome[u], e_u[u], w_max))
        .collect();

    let corr = correlation_matrix(judgment_vectors);
    let clusters = cluster_by_correlation(&corr, corr_threshold);
    let discounted = discount_weights(&base, &clusters, ALPHA);

    // Cluster sizes: only a coordinated block (≥ 2 members) is discounted; an
    // independent reviewer keeps its full base weight.
    let max_id = clusters.iter().copied().max().map_or(0, |m| m + 1);
    let mut sizes = vec![0usize; max_id];
    for &c in &clusters {
        sizes[c] += 1;
    }
    (0..n)
        .map(|u| {
            if sizes[clusters[u]] >= 2 {
                discounted[u]
            } else {
                base[u]
            }
        })
        .collect()
}

/// Weighted mean of the revealed pass-probabilities: `Σ w_u·p_u / Σ w_u`.
///
/// This is a proper-scoring aggregate, **not** a vote count. Returns `None` when the
/// total eligible weight is zero (e.g. every reviewer is on probation) — the panel is
/// undecided and the caller must fall back (typically to supplementary review), never
/// to a default pass.
pub fn aggregate_pass_probability(revealed_probs: &[f64], weights: &[f64]) -> Option<f64> {
    debug_assert_eq!(revealed_probs.len(), weights.len());
    let total: f64 = weights.iter().sum();
    if total <= 0.0 {
        return None;
    }
    let weighted: f64 = revealed_probs.iter().zip(weights).map(|(p, w)| p * w).sum();
    Some(weighted / total)
}

/// Resolves the bridging gate's uncertainty band: the item advances iff the weighted
/// pass-probability reaches `decision_threshold`. Use [`DECISION_THRESHOLD`] for the
/// default 0.5.
pub fn resolve_band(pass_probability: f64, decision_threshold: f64) -> bool {
    pass_probability >= decision_threshold
}
