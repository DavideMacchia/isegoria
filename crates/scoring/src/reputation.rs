//! Level C — node reputation (`docs/02` §C). The author score `C_a` (proposal rate limit)
//! and the evaluator score `E_u` (review weight) live on separate, unlinkable pseudonyms
//! and are never combined (CLAUDE.md #4, `docs/01` D5).

use crate::fmath::exp;

#[derive(Clone, Copy, Debug)]
pub struct AuthorPrior {
    pub alpha0: f64,
    pub beta0: f64,
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

/// Posterior-mean author score (shrinkage, time decay): `qualities[j] ∈ [0,1]`, `ages_months[j]`.
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

pub fn proposal_rate(c_a: f64, q_min: f64, q_max: f64) -> f64 {
    q_min + (q_max - q_min) * c_a
}

/// Odds-scale evaluator weight `w_u = exp(γ · S_u · k_u / (k_u + k₀))` (D33); provisional (T25).
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

/// Per-item Brier improvement over the baseline (D33): strictly proper, 0 at the baseline.
pub fn difference_score(p: f64, baseline: f64, o: f64) -> f64 {
    (baseline - o).powi(2) - (p - o).powi(2)
}

/// Leave-one-out crowd baseline (D33); with no other weight, a reviewer's own forecast.
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

/// `S_u`, the symmetric long-window mean of a reviewer's per-item scores (D34).
pub fn mean_score(scores: &[f64]) -> f64 {
    if scores.is_empty() {
        0.0
    } else {
        scores.iter().sum::<f64>() / scores.len() as f64
    }
}

/// Inverse-probability-weighted mean of `(score, π)` pairs (D35): proper whatever the gate decided.
pub fn inverse_probability_mean(observed: &[(f64, f64)], reviewed: usize) -> f64 {
    if reviewed == 0 {
        0.0
    } else {
        observed.iter().map(|&(d, pi)| d / pi).sum::<f64>() / reviewed as f64
    }
}

/// Odds-scale review weight (D33): 1 at crowd level or with nothing scored; see [`weight_cap`].
pub fn odds_weight(s_u: f64, k_u: usize, params: &EvaluatorParams) -> f64 {
    let k = k_u as f64;
    exp(params.gamma * s_u * k / (k + params.k0))
}

/// Brier Skill Score of `p` against `o`; not proper, kept for the sim oracle (REPUTATION-002).
pub fn brier_skill_score(p: &[f64], o: &[f64], baseline: &[f64]) -> f64 {
    let mut num = 0.0;
    let mut den = 0.0;
    for i in 0..o.len() {
        num += (p[i] - o[i]).powi(2);
        den += (baseline[i] - o[i]).powi(2);
    }
    // A zero-variance baseline has no error to improve on: 0.0, not a division by zero.
    if den == 0.0 {
        return 0.0;
    }
    1.0 - num / den
}

/// Constant base-rate baseline (mean outcome), kept for the sim-reproduction test only.
pub fn base_rate_baseline(o: &[f64]) -> Vec<f64> {
    let mean = o.iter().sum::<f64>() / o.len() as f64;
    vec![mean; o.len()]
}

/// Crowd baseline, reviewer included (`docs/01` D23); the evaluator score uses [`loo_baseline`].
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

/// One-sided CUSUM parameters (`docs/01` D34): allowance `k` and alarm threshold `h`; provisional.
#[derive(Clone, Copy, Debug)]
pub struct CusumParams {
    pub k: f64,
    pub h: f64,
}

impl Default for CusumParams {
    fn default() -> Self {
        CusumParams { k: 0.03, h: 1.5 }
    }
}

/// One-sided CUSUM (D34) for a sustained *drop* of a reviewer's per-item scores below their
/// long-run mean: a change in mean, not in variance.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Cusum {
    s: f64,
}

impl Cusum {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds one `score` against the reviewer's `reference` mean; `true` on an alarm, which resets.
    pub fn observe(&mut self, reference: f64, score: f64, params: &CusumParams) -> bool {
        self.s = (self.s + (reference - score) - params.k).max(0.0);
        if self.s > params.h {
            self.s = 0.0;
            true
        } else {
            false
        }
    }

    pub fn statistic(&self) -> f64 {
        self.s
    }
}

/// Per-node weight cap `3 × median(weights)` (`docs/02` §C.4) on the odds scale, per epoch.
pub fn weight_cap(weights: &[f64]) -> f64 {
    3.0 * median(weights)
}

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

/// Dasgupta–Ghosh peer-prediction score for dimensions without a verdict (`docs/02` §C.3):
/// agreement with a reference reviewer on a shared item minus the agreement on two
/// items judged separately.
pub fn dasgupta_ghosh(shared_p: bool, shared_q: bool, p_other: bool, q_other: bool) -> f64 {
    (shared_p == shared_q) as i32 as f64 - (p_other == q_other) as i32 as f64
}
