//! Level B — IRT and classical item statistics. See `docs/02`, §B.1–B.2.

use crate::glm::{fit_logistic, LogisticFit};

pub const A_MIN: f64 = 0.6;
pub const B_ABS_MAX: f64 = 2.5;
pub const R_PBIS_MIN: f64 = 0.20;
/// Anchor-reliability floor of the latent re-check (`docs/01` D37, T53): the standardized
/// anchor total stands in for θ only when the anchors' KR-20, on the batch's own
/// respondents, is at least this — about 40 anchors of the reference design (the paper's
/// Table: 30 anchors give 0.87, 40 give 0.90, 60 give 0.93). Provisional (T25).
pub const KR20_MIN: f64 = 0.90;

/// Ability θ from a set of DIF-free anchor items: standardized total score
/// (`docs/02`, §B.4; matches `th` in `sim/bridging_irt_dif.py`).
pub fn theta_from_anchors(anchors: &[Vec<f64>]) -> Vec<f64> {
    let totals: Vec<f64> = anchors.iter().map(|row| row.iter().sum()).collect();
    standardize(&totals)
}

/// Kuder–Richardson 20 of the anchor total: `K/(K−1) · (1 − Σ_j p_j(1−p_j) / Var(T))`,
/// with `p_j` the proportion answering anchor `j` correctly and `Var(T)` the population
/// variance of the totals (the paper's `revisions_dif.py`). `anchors` is respondents ×
/// anchors, one row per respondent (as [`theta_from_anchors`]). It is the reliability of
/// the θ proxy: below [`KR20_MIN`] the latent-class detector mistakes proxy error for a
/// latent class (`docs/08` DIF-010). With fewer than two anchors, no respondents or no
/// spread in the totals there is no reliability to measure: 0, not NaN (T36). A ragged
/// matrix is read up to its shortest row.
pub fn kr20(anchors: &[Vec<f64>]) -> f64 {
    let n = anchors.len();
    let k = anchors.iter().map(Vec::len).min().unwrap_or(0);
    if n == 0 || k < 2 {
        return 0.0;
    }
    let nf = n as f64;
    let totals: Vec<f64> = anchors.iter().map(|row| row[..k].iter().sum()).collect();
    let mean_t = totals.iter().sum::<f64>() / nf;
    let var_t = totals.iter().map(|t| (t - mean_t).powi(2)).sum::<f64>() / nf;
    if var_t.is_nan() || var_t <= 0.0 {
        return 0.0;
    }
    let item_var: f64 = (0..k)
        .map(|j| {
            let p = anchors.iter().map(|row| row[j]).sum::<f64>() / nf;
            p * (1.0 - p)
        })
        .sum();
    let kf = k as f64;
    kf / (kf - 1.0) * (1.0 - item_var / var_t)
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
