//! Level B — IRT and classical item statistics. See `docs/02`, §B.1–B.2.

use crate::glm::{fit_logistic, LogisticFit};

pub const A_MIN: f64 = 0.6;
pub const B_ABS_MAX: f64 = 2.5;
pub const R_PBIS_MIN: f64 = 0.20;

/// Ability θ from a set of DIF-free anchor items: standardized total score
/// (`docs/02`, §B.4; matches `th` in `sim/bridging_irt_dif.py`).
pub fn theta_from_anchors(anchors: &[Vec<f64>]) -> Vec<f64> {
    let totals: Vec<f64> = anchors.iter().map(|row| row.iter().sum()).collect();
    standardize(&totals)
}

/// `(t − mean) / sd_pop`. With no spread (all totals equal) there is no ability signal
/// to scale, so every θ is 0 rather than NaN (IRT-001, T36); an empty input is empty.
pub(crate) fn standardize(values: &[f64]) -> Vec<f64> {
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let var = values.iter().map(|t| (t - mean).powi(2)).sum::<f64>() / n;
    let sd = var.sqrt();
    if sd.is_nan() || sd == 0.0 {
        return vec![0.0; values.len()];
    }
    values.iter().map(|t| (t - mean) / sd).collect()
}

/// Point-biserial: correlation between a binary item and the total score.
/// Negative usually means a wrong answer key (`docs/02`, §B.2). If either side has no
/// variance (everyone answered alike, or θ is flat) the correlation is undefined; it is
/// reported as 0, which fails the `R_PBIS_MIN` screen rather than propagating NaN (T36).
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
    let den = vi.sqrt() * vt.sqrt();
    if den.is_nan() || den == 0.0 {
        return 0.0;
    }
    cov / den
}

/// A 2PL item fit. `a` and `b` are meaningful only when `status` is `Converged`: under
/// separation the slope diverges (docs/08 OPT-001, T34).
#[derive(Clone, Copy, Debug)]
pub struct Fit2pl {
    pub a: f64,
    pub b: f64,
    pub status: LogisticFit,
}

/// 2PL fit for one item given fixed θ: `logit P = a(θ − b)`, via logistic
/// regression on `[1, θ]` with `a = slope`, `b = −intercept / slope`.
pub fn fit_2pl_item(theta: &[f64], responses: &[f64]) -> Fit2pl {
    let x: Vec<Vec<f64>> = theta.iter().map(|&t| vec![1.0, t]).collect();
    let fit = fit_logistic(&x, responses, 200);
    let w = fit.weights;
    Fit2pl {
        a: w[1],
        b: -w[0] / w[1],
        status: fit.status,
    }
}
