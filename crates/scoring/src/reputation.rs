//! Level C — node reputation. See `docs/02`, §C.
//!
//! Two scores that must never be combined (CLAUDE.md #4, `docs/01` D5): the author
//! score `C_a` gates the proposal rate limit; the evaluator score `E_u` weights the
//! review vote. They live on separate, unlinkable pseudonyms.

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

// ------------------------ C.2 evaluator score (D33) ------------------------

/// Parameters of the odds-scale evaluator weight (`docs/01` D33, T50):
/// `w_u = exp(γ · S_u · k_u / (k_u + k₀))`. At `γ ≈ 35` a reviewer reliably 0.02 better
/// than the crowd weighs about double; `k₀ ≈ 100` scored items is the shrinkage that
/// stops luck from buying weight (with 16 scored items one standard error of luck,
/// 0.025, is worth ×2.4 without shrinkage and ×1.13 with it). Provisional (T25).
#[derive(Clone, Copy, Debug)]
pub struct EvaluatorParams {
    pub gamma: f64,
    pub k0: f64,
}

impl Default for EvaluatorParams {
    fn default() -> Self {
        EvaluatorParams {
            gamma: 35.0,
            k0: 100.0,
        }
    }
}

/// The per-item difference score `d = (baseline − o)² − (p − o)²` (D33, paper Prop. 14):
/// the reviewer's Brier improvement over the baseline on one scored item. Strictly proper
/// — the baseline term does not depend on the report, and the Brier score is strictly
/// proper — and exactly 0 for a report equal to the baseline.
pub fn difference_score(p: f64, baseline: f64, o: f64) -> f64 {
    (baseline - o).powi(2) - (p - o).powi(2)
}

/// Leave-one-out crowd baselines `p̄_{−u,j} = Σ_{v≠u} w_v p_vj / Σ_{v≠u} w_v` (D33: the
/// D23 crowd baseline minus the reviewer being scored). `predictions[u][j]`, `weights[u]`.
/// A reviewer whose other panelists carry no weight has nothing to be compared with: the
/// baseline is their own forecast, so every score of theirs is 0.
pub fn loo_baseline(predictions: &[Vec<f64>], weights: &[f64]) -> Vec<Vec<f64>> {
    let m = predictions.first().map_or(0, |row| row.len());
    (0..predictions.len())
        .map(|u| {
            let others: f64 = weights
                .iter()
                .enumerate()
                .filter(|&(v, _)| v != u)
                .map(|(_, &w)| w)
                .sum();
            (0..m)
                .map(|j| {
                    if others > 0.0 {
                        predictions
                            .iter()
                            .zip(weights)
                            .enumerate()
                            .filter(|&(v, _)| v != u)
                            .map(|(_, (p, &w))| w * p[j])
                            .sum::<f64>()
                            / others
                    } else {
                        predictions[u][j]
                    }
                })
                .collect()
        })
        .collect()
}

/// Per-reviewer, per-item difference scores against the leave-one-out baseline (D33):
/// `scores[u][j] = (p̄_{−u,j} − o_j)² − (p_uj − o_j)²`.
pub fn loo_scores(predictions: &[Vec<f64>], weights: &[f64], outcomes: &[f64]) -> Vec<Vec<f64>> {
    let baseline = loo_baseline(predictions, weights);
    predictions
        .iter()
        .zip(&baseline)
        .map(|(p, b)| {
            p.iter()
                .zip(b)
                .zip(outcomes)
                .map(|((&p, &b), &o)| difference_score(p, b, o))
                .collect()
        })
        .collect()
}

/// `S_u`: the mean of a reviewer's per-item scores — the symmetric long-window mean of
/// D34 — 0 with nothing scored.
pub fn mean_score(scores: &[f64]) -> f64 {
    if scores.is_empty() {
        0.0
    } else {
        scores.iter().sum::<f64>() / scores.len() as f64
    }
}

/// The evaluator's review weight on the odds scale, shrunk toward 1 by the number of
/// scored items: `exp(γ · S_u · k_u / (k_u + k₀))` (D33). 1 for a crowd-level reviewer
/// and for one with nothing scored; unbounded above, so the cap `3 × median` can bind.
pub fn odds_weight(s_u: f64, k_u: usize, params: &EvaluatorParams) -> f64 {
    let k = k_u as f64;
    exp(params.gamma * s_u * k / (k + params.k0))
}

/// Brier Skill Score of predictions `p` against outcomes `o`, normalized by a
/// `baseline` predictor. Zero means "no better than the baseline"; positive means
/// right when the baseline is wrong. *Retired as the evaluator score* (D33, T50): the
/// ratio of two sums is not proper — the optimal report moves toward the outcome the
/// crowd favours (paper Prop. 12). Kept for the sim-reproduction oracle (REPUTATION-002).
pub fn brier_skill_score(p: &[f64], o: &[f64], baseline: &[f64]) -> f64 {
    let mut num = 0.0;
    let mut den = 0.0;
    for i in 0..o.len() {
        num += (p[i] - o[i]).powi(2);
        den += (baseline[i] - o[i]).powi(2);
    }
    // A zero-variance baseline (e.g. every outcome identical, so the base rate equals
    // every outcome) has no error to improve on: skill is undefined. Report it as a
    // finite, neutral 0.0 rather than dividing by zero into NaN/−∞.
    if den == 0.0 {
        return 0.0;
    }
    1.0 - num / den
}

/// Constant base-rate baseline (mean outcome), as used in `sim/bridging_irt_dif.py`.
/// Kept for the sim-reproduction test; the evaluator score uses [`crowd_baseline`].
pub fn base_rate_baseline(o: &[f64]) -> Vec<f64> {
    let mean = o.iter().sum::<f64>() / o.len() as f64;
    vec![mean; o.len()]
}

/// Crowd baseline (`docs/01` D23, docs/08 G-09): per-item weight-adjusted mean of the
/// panel's declared predictions, `p̄_j = Σ_u w_u p_uj / Σ_u w_u`, the reviewer included.
/// The evaluator score reads its leave-one-out form ([`loo_baseline`], D33): the whole
/// panel's mean is kept as the crowd forecast of an item. `predictions[u][j]`.
pub fn crowd_baseline(predictions: &[Vec<f64>], weights: &[f64]) -> Vec<f64> {
    let m = predictions.first().map_or(0, |row| row.len());
    let total: f64 = weights.iter().sum();
    (0..m)
        .map(|j| {
            if total > 0.0 {
                predictions
                    .iter()
                    .zip(weights)
                    .map(|(p, &w)| w * p[j])
                    .sum::<f64>()
                    / total
            } else {
                0.0
            }
        })
        .collect()
}

// -------------------- C.4 temporal asymmetry & cap --------------------

/// Asymmetric update: rises slowly, falls fast, so a long-con of hoarded reputation
/// does not pay (`docs/02`, §C.4). `up` ≪ `down`.
pub fn asymmetric_ema(prev: f64, new: f64, up: f64, down: f64) -> f64 {
    let rate = if new >= prev { up } else { down };
    prev + rate * (new - prev)
}

/// Hard per-node weight cap `3 × median(weights)` (`docs/02`, §C.4), recomputed each
/// epoch over the weights that count. On the odds scale of [`odds_weight`] it binds
/// (D33; it never did on `E_u ∈ (0,1)`, docs/08 G-12).
pub fn weight_cap(weights: &[f64]) -> f64 {
    3.0 * median(weights)
}

/// Applies the vote weight `w_u = min(w_max, w)`.
pub fn capped_weight(w: f64, w_max: f64) -> f64 {
    w.min(w_max)
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
