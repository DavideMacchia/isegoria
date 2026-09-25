//! Level C — node reputation. See `docs/02`, §C.
//!
//! Two scores that must never be combined (CLAUDE.md #4, `docs/01` D5): the author
//! score `C_a` gates the proposal rate limit; the evaluator score `S_u` — the
//! leave-one-out difference score of D33, a strictly proper rule — sets the review vote
//! weight on the odds scale. They live on separate, unlinkable pseudonyms.

use crate::fmath::exp;

// -------------------------- C.1 author score --------------------------

#[derive(Clone, Copy, Debug)]
pub struct AuthorPrior {
    pub alpha0: f64,
    pub beta0: f64,
    /// time constant of `w = exp(−Δt / T)`, in months
    pub decay_months: f64,
}

impl Default for AuthorPrior {
    fn default() -> Self {
        AuthorPrior {
            alpha0: 2.0,
            beta0: 3.0,
            decay_months: 18.0,
        }
    }
}

/// Posterior-mean author score with shrinkage and time decay. `qualities[j]` is the
/// final quality `q_j ∈ [0,1]` of item `j`, `ages_months[j]` its age.
pub fn author_score(qualities: &[f64], ages_months: &[f64], prior: &AuthorPrior) -> f64 {
    let mut sum_wq = 0.0;
    let mut sum_w = 0.0;
    for (q, age) in qualities.iter().zip(ages_months.iter()) {
        let w = exp(-age / prior.decay_months);
        sum_wq += w * q;
        sum_w += w;
    }
    (prior.alpha0 + sum_wq) / (prior.alpha0 + prior.beta0 + sum_w)
}

/// Maps the author score to a proposal rate: `q_min + (q_max − q_min)·C_a`.
pub fn proposal_rate(c_a: f64, q_min: f64, q_max: f64) -> f64 {
    q_min + (q_max - q_min) * c_a
}

// ------------------------ C.2 evaluator score ------------------------

/// Odds-scale gain of the evaluator weight (`docs/01` D33): at `γ = 35` a reviewer
/// reliably 0.02 better than the crowd weighs double. Provisional until T24/T25.
pub const GAMMA: f64 = 35.0;
/// Shrinkage constant `k_0` of the evaluator weight (D33): with `k_u` scored outcomes the
/// score counts `k_u / (k_u + k_0)`. The per-item noise (≈ 0.1) against skill
/// differences of 0.01–0.02 puts it near 100 (paper §7.2). Provisional until T24/T25.
pub const K_SHRINK: f64 = 100.0;

/// Leave-one-out crowd forecast of panelist `u` on every item: the weight-adjusted mean
/// of the *other* panelists' forecasts, `p̄_{−u,j} = Σ_{v≠u} w_v p_vj / Σ_{v≠u} w_v` — D23's
/// crowd baseline minus the reviewer being scored (D33). The weights are the review
/// weights, so a probationer's 0 keeps it out of the crowd others are scored against,
/// not out of the scoring. When no other panelist carries weight, the crowd is the plain
/// mean of the others; with no other panelist there is no crowd, and the result is empty.
/// `predictions[u][j]`.
pub fn loo_baseline(predictions: &[Vec<f64>], weights: &[f64], u: usize) -> Vec<f64> {
    let m = predictions.first().map_or(0, Vec::len);
    let others: Vec<usize> = (0..predictions.len()).filter(|&v| v != u).collect();
    if others.is_empty() {
        return Vec::new();
    }
    let total: f64 = others.iter().map(|&v| weights[v]).sum();
    (0..m)
        .map(|j| {
            if total > 0.0 {
                others
                    .iter()
                    .map(|&v| weights[v] * predictions[v][j])
                    .sum::<f64>()
                    / total
            } else {
                others.iter().map(|&v| predictions[v][j]).sum::<f64>() / others.len() as f64
            }
        })
        .collect()
}

/// Per-item leave-one-out difference scores (D33; paper Prop. 14):
/// `S_uj = (p̄_{−u,j} − o_j)² − (p_uj − o_j)²` for every panelist `u` and scored item `j`,
/// with `p̄_{−u,j}` from [`loo_baseline`]. `predictions[u][j]`, `outcomes[j] ∈ {0, 1}`,
/// `weights[u]` the review weights. Strictly proper: the expected score is
/// `c_u − Σ_j (p_uj − q_j)² / m`, maximized by the true belief `q`; exactly 0 for every
/// outcome when the forecast equals the crowd's; positive exactly when the reviewer's
/// Brier score beats the crowd's; additive over items, so it accumulates across epochs.
/// Empty rows for a panel of one — no crowd to beat.
pub fn difference_scores(
    predictions: &[Vec<f64>],
    weights: &[f64],
    outcomes: &[f64],
) -> Vec<Vec<f64>> {
    (0..predictions.len())
        .map(|u| {
            let crowd = loo_baseline(predictions, weights, u);
            crowd
                .iter()
                .zip(&predictions[u])
                .zip(outcomes)
                .map(|((b, p), o)| (b - o).powi(2) - (p - o).powi(2))
                .collect()
        })
        .collect()
}

/// A reviewer's evaluator score `S_u`: the mean of its per-item scores (D33); 0 with none.
pub fn mean_score(per_item: &[f64]) -> f64 {
    if per_item.is_empty() {
        0.0
    } else {
        per_item.iter().sum::<f64>() / per_item.len() as f64
    }
}

/// Odds-scale review weight with shrinkage (D33): `w_u = exp(γ · S_u · k_u / (k_u + k_0))`,
/// `k_u` the number of scored outcomes. Unbounded above, so the `3 × median` cap can bind
/// (paper Prop. 15); exactly 1 at the crowd's level or with no scored outcome.
pub fn odds_weight(score: f64, scored: usize, gamma: f64, k_shrink: f64) -> f64 {
    let k = scored as f64;
    let shrink = if k + k_shrink > 0.0 {
        k / (k + k_shrink)
    } else {
        0.0
    };
    exp(gamma * score * shrink)
}

// -------------------- C.4 temporal asymmetry & cap --------------------

/// Asymmetric update: rises slowly, falls fast, so a long-con of hoarded reputation
/// does not pay (`docs/02`, §C.4). `up` ≪ `down`. Superseded by the CUSUM change
/// detector of D34 (T51).
pub fn asymmetric_ema(prev: f64, new: f64, up: f64, down: f64) -> f64 {
    let rate = if new >= prev { up } else { down };
    prev + rate * (new - prev)
}

/// Hard per-node weight cap `3 × median(weights)` (`docs/02`, §C.4).
pub fn weight_cap(weights: &[f64]) -> f64 {
    3.0 * median(weights)
}

/// Applies the vote weight cap: `min(w_max, w_u)`.
pub fn capped_weight(w_u: f64, w_max: f64) -> f64 {
    w_u.min(w_max)
}

/// The cap across an epoch's review weights (D33): `w_max = 3 × median` of the weights
/// that count — the positive ones; a probationer's 0 is an exclusion, not a weight, and
/// would pull the cap to 0 in a young network — and every weight becomes
/// `min(w_max, w_u)`. On the odds scale the cap can bind (paper Prop. 15). With no
/// positive weight nothing changes.
pub fn cap_weights(weights: &[f64]) -> Vec<f64> {
    let counted: Vec<f64> = weights.iter().copied().filter(|&w| w > 0.0).collect();
    if counted.is_empty() {
        return weights.to_vec();
    }
    let w_max = weight_cap(&counted);
    weights.iter().map(|&w| capped_weight(w, w_max)).collect()
}

fn median(values: &[f64]) -> f64 {
    let mut v = values.to_vec();
    // `total_cmp`: NaN weights sort last instead of panicking (docs/08 IQ-2).
    v.sort_by(|a, b| a.total_cmp(b));
    let n = v.len();
    if n == 0 {
        0.0
    } else if n % 2 == 1 {
        v[n / 2]
    } else {
        0.5 * (v[n / 2 - 1] + v[n / 2])
    }
}

// -------------------- C.3 peer prediction (no ground truth) --------------------

/// Dasgupta–Ghosh peer-prediction score for dimensions with no empirical verdict
/// (`docs/02`, §C.3): agreement with a reference reviewer on a shared item, minus
/// the baseline agreement estimated on two items judged separately.
pub fn dasgupta_ghosh(shared_p: bool, shared_q: bool, p_other: bool, q_other: bool) -> f64 {
    (shared_p == shared_q) as i32 as f64 - (p_other == q_other) as i32 as f64
}
