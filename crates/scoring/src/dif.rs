//! Level B — Differential Item Functioning. See `docs/02`, §B.3.
//!
//! Variant 1: logistic regression on a group axis (`|β₂| > 0.40` → reject), plus
//! Mantel–Haenszel with ETS A/B/C classification.
//! Variant 2: latent-class mixture IRT, the anonymity-compatible detector
//! (`DIF = max|b_g − b_h| > 0.5` → reject), run per batch, never per single item.
//! Reference prototype: `sim/latent_dif_and_capacity.py`.

use crate::glm::{fit_logistic, sigmoid};
use crate::optim::{lbfgs, numerical_gradient};
use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

pub const BETA2_MAX: f64 = 0.40;
pub const MIXTURE_DIF_MAX: f64 = 0.5;
pub const MH_DELTA_B: f64 = 1.0;
pub const MH_DELTA_C: f64 = 1.5;

#[derive(Clone, Copy, Debug)]
pub struct DifCoefs {
    pub beta0: f64,
    pub beta1: f64,
    pub beta2: f64,
    pub beta3: f64,
}

/// Variant 1: `logit P = β₀ + β₁θ + β₂g + β₃(θ·g)`. `β₂` is uniform DIF,
/// `β₃` is non-uniform DIF.
pub fn logistic_dif(item: &[f64], theta: &[f64], group: &[f64]) -> DifCoefs {
    let x: Vec<Vec<f64>> = (0..item.len())
        .map(|i| vec![1.0, theta[i], group[i], theta[i] * group[i]])
        .collect();
    let w = fit_logistic(&x, item, 400);
    DifCoefs {
        beta0: w[0],
        beta1: w[1],
        beta2: w[2],
        beta3: w[3],
    }
}

/// ETS DIF severity classes (`docs/02`, §B.3): A negligible, B moderate
/// (accepted with monitoring), C large (rejected).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EtsClass {
    A,
    B,
    C,
}

#[derive(Clone, Copy, Debug)]
pub struct MhResult {
    /// common odds ratio α_MH
    pub alpha: f64,
    /// `Δ_MH = −2.35 · ln(α_MH)`
    pub delta: f64,
    pub class: EtsClass,
}

/// Mantel–Haenszel DIF: matches respondents on ability (θ split into `n_strata`
/// equal-frequency strata) and compares the two `group` values (−1 reference,
/// +1 focal) within each stratum.
pub fn mantel_haenszel(item: &[f64], theta: &[f64], group: &[f64], n_strata: usize) -> MhResult {
    let n = item.len();
    let mut order: Vec<usize> = (0..n).collect();
    // `unwrap()` here panics on a NaN θ; treat incomparable values as equal so a
    // caller's bad datum degrades the stratification instead of crashing scoring.
    order.sort_by(|&a, &b| {
        theta[a]
            .partial_cmp(&theta[b])
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut num = 0.0; // Σ A_s D_s / N_s
    let mut den = 0.0; // Σ B_s C_s / N_s
    for s in 0..n_strata {
        let lo = s * n / n_strata;
        let hi = (s + 1) * n / n_strata;
        let (mut a, mut b, mut c, mut d) = (0.0, 0.0, 0.0, 0.0); // ref✓ ref✗ foc✓ foc✗
        for &i in &order[lo..hi] {
            let correct = item[i] > 0.5;
            match (group[i] > 0.0, correct) {
                (false, true) => a += 1.0,
                (false, false) => b += 1.0,
                (true, true) => c += 1.0,
                (true, false) => d += 1.0,
            }
        }
        let ns = a + b + c + d;
        if ns > 0.0 {
            num += a * d / ns;
            den += b * c / ns;
        }
    }

    let alpha = if den > 0.0 { num / den } else { f64::INFINITY };
    let delta = -2.35 * alpha.ln();
    let class = if delta.abs() < MH_DELTA_B {
        EtsClass::A
    } else if delta.abs() < MH_DELTA_C {
        EtsClass::B
    } else {
        EtsClass::C
    };
    MhResult {
        alpha,
        delta,
        class,
    }
}

#[derive(Clone, Debug)]
pub struct MixtureDif {
    /// mixing proportion of class +1
    pub pi: f64,
    /// per-item |δ|, the latent-class difficulty shift
    pub delta: Vec<f64>,
    /// per-respondent posterior probability of class +1
    pub class_posterior: Vec<f64>,
    /// likelihood ratio of the free-δ model vs the null (δ = 0)
    pub lr: f64,
    /// `lr − K·ln(NT)`: > 0 favors the two-class model
    pub bic: f64,
}

// Marginal negative log-likelihood over two latent classes z ∈ {−1, +1}, which
// differ only by the per-item shift δ_j·z. Layout: [ logit_pi | a(K) | b(K) | δ(K) ].
fn mixture_nll(params: &[f64], theta: &[f64], x: &[Vec<f64>], k: usize, free_delta: bool) -> f64 {
    let pi = sigmoid(params[0]);
    let a = &params[1..1 + k];
    let b = &params[1 + k..1 + 2 * k];
    let zero = vec![0.0; k];
    let d: &[f64] = if free_delta {
        &params[1 + 2 * k..1 + 3 * k]
    } else {
        &zero
    };
    let ln_pi_pos = pi.ln();
    let ln_pi_neg = (1.0 - pi).ln();

    let mut total = 0.0;
    for (i, row) in x.iter().enumerate() {
        let mut ll = [0.0_f64; 2];
        for (zi, &z) in [-1.0_f64, 1.0].iter().enumerate() {
            let mut s = 0.0;
            for j in 0..k {
                let lo = a[j] * (theta[i] - b[j] - d[j] * z);
                s += row[j] * lo - softplus(lo);
            }
            ll[zi] = s;
        }
        let a_term = ll[0] + ln_pi_neg;
        let b_term = ll[1] + ln_pi_pos;
        total += logsumexp2(a_term, b_term);
    }
    -total
}

/// Variant 2: fit the two-class mixture on a batch of `k` items and compare it to
/// the null. `theta` is the ability estimated on anchors; `x` is respondents × items.
pub fn mixture_dif(theta: &[f64], x: &[Vec<f64>], k: usize, seed: u64) -> MixtureDif {
    let nt = x.len();

    // δ init must be nonzero to break the class symmetry (as in the prototype).
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut full0 = vec![0.0; 1 + 3 * k];
    for j in 0..k {
        full0[1 + j] = 1.0; // a
        full0[1 + 2 * k + j] = normal(&mut rng) * 0.3; // δ
    }
    let full = lbfgs(
        full0,
        |p| mixture_nll(p, theta, x, k, true),
        |p| numerical_gradient(&|q| mixture_nll(q, theta, x, k, true), p, 1e-5),
        10,
        3000,
        1e-6,
    );

    let mut null0 = vec![0.0; 1 + 2 * k];
    for j in 0..k {
        null0[1 + j] = 1.0;
    }
    let null = lbfgs(
        null0,
        |p| mixture_nll(p, theta, x, k, false),
        |p| numerical_gradient(&|q| mixture_nll(q, theta, x, k, false), p, 1e-5),
        10,
        3000,
        1e-6,
    );

    let full_fun = mixture_nll(&full, theta, x, k, true);
    let null_fun = mixture_nll(&null, theta, x, k, false);
    let lr = 2.0 * (null_fun - full_fun);
    let bic = lr - (k as f64) * (nt as f64).ln();

    let pi = sigmoid(full[0]);
    let a = &full[1..1 + k];
    let b = &full[1 + k..1 + 2 * k];
    let d = &full[1 + 2 * k..1 + 3 * k];
    let delta: Vec<f64> = d.iter().map(|v| v.abs()).collect();

    let ln_pi_pos = pi.ln();
    let ln_pi_neg = (1.0 - pi).ln();
    let class_posterior: Vec<f64> = (0..nt)
        .map(|i| {
            let mut ll = [0.0_f64; 2];
            for (zi, &z) in [-1.0_f64, 1.0].iter().enumerate() {
                let mut s = 0.0;
                for j in 0..k {
                    let lo = a[j] * (theta[i] - b[j] - d[j] * z);
                    s += x[i][j] * lo - softplus(lo);
                }
                ll[zi] = s;
            }
            let neg = ll[0] + ln_pi_neg;
            let pos = ll[1] + ln_pi_pos;
            let m = neg.max(pos);
            let pe = (pos - m).exp();
            let ne = (neg - m).exp();
            pe / (pe + ne)
        })
        .collect();

    MixtureDif {
        pi,
        delta,
        class_posterior,
        lr,
        bic,
    }
}

#[inline]
fn softplus(z: f64) -> f64 {
    if z > 0.0 {
        z + (-z).exp().ln_1p()
    } else {
        z.exp().ln_1p()
    }
}

#[inline]
fn logsumexp2(a: f64, b: f64) -> f64 {
    let m = a.max(b);
    m + ((a - m).exp() + (b - m).exp()).ln()
}

fn normal(rng: &mut ChaCha8Rng) -> f64 {
    let u1: f64 = 1.0 - rng.gen::<f64>();
    let u2: f64 = rng.gen::<f64>();
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}
