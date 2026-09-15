//! Level B — IRT and classical item statistics. See `docs/02`, §B.1–B.2.

use crate::glm::fit_logistic;

pub const A_MIN: f64 = 0.6;
pub const B_ABS_MAX: f64 = 2.5;
pub const R_PBIS_MIN: f64 = 0.20;

/// Ability θ from a set of DIF-free anchor items: standardized total score
/// (`docs/02`, §B.4; matches `th` in `sim/bridging_irt_dif.py`).
pub fn theta_from_anchors(anchors: &[Vec<f64>]) -> Vec<f64> {
    let totals: Vec<f64> = anchors.iter().map(|row| row.iter().sum()).collect();
    standardize(&totals)
}

pub(crate) fn standardize(values: &[f64]) -> Vec<f64> {
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let var = values.iter().map(|t| (t - mean).powi(2)).sum::<f64>() / n;
    let sd = var.sqrt();
    values.iter().map(|t| (t - mean) / sd).collect()
}

/// Point-biserial: correlation between a binary item and the total score.
/// Negative usually means a wrong answer key (`docs/02`, §B.2).
pub fn point_biserial(item: &[f64], total: &[f64]) -> f64 {
    let n = item.len() as f64;
    let mi = item.iter().sum::<f64>() / n;
    let mt = total.iter().sum::<f64>() / n;
    let mut cov = 0.0;
    let mut vi = 0.0;
    let mut vt = 0.0;
    for k in 0..item.len() {
        let di = item[k] - mi;
        let dt = total[k] - mt;
        cov += di * dt;
        vi += di * di;
        vt += dt * dt;
    }
    cov / (vi.sqrt() * vt.sqrt())
}

/// 2PL fit for one item given fixed θ: `logit P = a(θ − b)`, via logistic
/// regression on `[1, θ]` with `a = slope`, `b = −intercept / slope`.
pub fn fit_2pl_item(theta: &[f64], responses: &[f64]) -> (f64, f64) {
    let x: Vec<Vec<f64>> = theta.iter().map(|&t| vec![1.0, t]).collect();
    let w = fit_logistic(&x, responses, 200);
    let a = w[1];
    let b = -w[0] / w[1];
    (a, b)
}
