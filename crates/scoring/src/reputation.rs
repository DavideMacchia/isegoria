//! Level C — node reputation. See `docs/02`, §C.
//!
//! Two scores that must never be combined (CLAUDE.md #4, `docs/01` D5): the author
//! score `C_a` gates the proposal rate limit; the evaluator score `E_u` weights the
//! review vote. They live on separate, unlinkable pseudonyms.

use crate::glm::sigmoid;

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
        let w = (-age / prior.decay_months).exp();
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

/// Brier Skill Score of predictions `p` against outcomes `o`, normalized by a
/// `baseline` predictor. Zero means "no better than the baseline"; positive means
/// right when the baseline is wrong.
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
/// panel's declared predictions, `p̄_j = Σ_u w_u p_uj / Σ_u w_u`. This is the reference
/// the evaluator score is normalized against, so a reviewer who just predicts the crowd
/// scores `BSS ≈ 0` — not the hindsight outcome base rate. `predictions[u][j]`.
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

/// Squashes a Brier Skill Score into `E_u ∈ (0,1)` via `σ(γ·BSS)`.
pub fn evaluator_score(bss: f64, gamma: f64) -> f64 {
    sigmoid(gamma * bss)
}

// -------------------- C.4 temporal asymmetry & cap --------------------

/// Asymmetric update: rises slowly, falls fast, so a long-con of hoarded reputation
/// does not pay (`docs/02`, §C.4). `up` ≪ `down`.
pub fn asymmetric_ema(prev: f64, new: f64, up: f64, down: f64) -> f64 {
    let rate = if new >= prev { up } else { down };
    prev + rate * (new - prev)
}

/// Hard per-node weight cap `3 × median(weights)` (`docs/02`, §C.4).
pub fn weight_cap(weights: &[f64]) -> f64 {
    3.0 * median(weights)
}

/// Applies the vote weight `w_u = min(w_max, E_u)`.
pub fn capped_weight(e_u: f64, w_max: f64) -> f64 {
    e_u.min(w_max)
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
