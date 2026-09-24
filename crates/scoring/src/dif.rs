//! Level B — Differential Item Functioning. See `docs/02`, §B.3.
//!
//! Variant 1: logistic regression on a group axis (`|β₂| > 0.40` → reject), plus
//! Mantel–Haenszel with ETS A/B/C classification. Calibration-only: it needs a
//! per-respondent `group` (`docs/01` D20), so it is gated behind the `calibration` feature.
//! Variant 2: latent-class mixture IRT, the anonymity-compatible detector
//! (`DIF = max|b_g − b_h| > MIXTURE_DIF_MAX` → reject), run per batch, never per single
//! item — the only variant on the production path. The number of classes and uniform vs
//! non-uniform DIF are chosen by BIC, from seeded multi-starts (T40). Reference
//! prototype: `sim/latent_dif_and_capacity.py`.

#[cfg(feature = "calibration")]
use crate::glm::{fit_logistic, LogisticFit};
use crate::optim::{lbfgs, Convergence};
use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use std::cell::RefCell;

/// Variant-1 rejection threshold; calibration-only (see `mantel_haenszel`).
#[cfg(feature = "calibration")]
pub const BETA2_MAX: f64 = 0.40;
/// Variant-2 rejection threshold on `DIF_j = |b_j⁺ − b_j⁻|` (the b-gap). Provisional:
/// `docs/02` §B.3 cites 0.5 from the literature, but on this estimator 0.5 flags every
/// item of the one-biased-item fixture; 1.0 is the value the code has applied since the
/// start (it thresholded the half-gap `|δ|` at 0.5), now stated on the specified
/// quantity until T24/T25 calibrate it (`docs/08` DIF-006).
pub const MIXTURE_DIF_MAX: f64 = 1.0;
#[cfg(feature = "calibration")]
pub const MH_DELTA_B: f64 = 1.0;
#[cfg(feature = "calibration")]
pub const MH_DELTA_C: f64 = 1.5;

#[cfg(feature = "calibration")]
#[derive(Clone, Copy, Debug)]
pub struct DifCoefs {
    pub beta0: f64,
    pub beta1: f64,
    pub beta2: f64,
    pub beta3: f64,
    /// Logistic fit status; `Separated` means `beta2` is undetermined (docs/08 AT-DIF-06).
    pub status: LogisticFit,
}

/// Variant 1: `logit P = β₀ + β₁θ + β₂g + β₃(θ·g)`. `β₂` uniform DIF, `β₃` non-uniform.
/// Calibration-only: needs a per-respondent `group` (`docs/01` D20).
#[cfg(feature = "calibration")]
pub fn logistic_dif(item: &[f64], theta: &[f64], group: &[f64]) -> DifCoefs {
    let x: Vec<Vec<f64>> = (0..item.len())
        .map(|i| vec![1.0, theta[i], group[i], theta[i] * group[i]])
        .collect();
    let fit = fit_logistic(&x, item, 400);
    let w = &fit.weights;
    DifCoefs {
        beta0: w[0],
        beta1: w[1],
        beta2: w[2],
        beta3: w[3],
        status: fit.status,
    }
}

/// ETS DIF severity classes (`docs/02`, §B.3): A negligible, B moderate
/// (accepted with monitoring), C large (rejected).
#[cfg(feature = "calibration")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EtsClass {
    A,
    B,
    C,
}

#[cfg(feature = "calibration")]
#[derive(Clone, Copy, Debug)]
pub struct MhResult {
    /// common odds ratio α_MH
    pub alpha: f64,
    /// `Δ_MH = −2.35 · ln(α_MH)`
    pub delta: f64,
    pub class: EtsClass,
}

/// Mantel–Haenszel DIF (Variant 1): matches respondents on ability (θ split into
/// `n_strata` equal-frequency strata) and compares the two `group` values (−1 reference,
/// +1 focal) within each stratum. Calibration-only (`docs/01` D20).
#[cfg(feature = "calibration")]
pub fn mantel_haenszel(item: &[f64], theta: &[f64], group: &[f64], n_strata: usize) -> MhResult {
    let n = item.len();
    let mut order: Vec<usize> = (0..n).collect();
    // NaN policy (docs/08 IQ-2): sort with `total_cmp`, a total order that places
    // NaN after every number. `partial_cmp().unwrap()` panics on NaN, and a comparator
    // that treats NaN as *equal to everything* is not a total order, which Rust's sort
    // is allowed to detect and panic on (since 1.81). A NaN θ therefore lands in the
    // top stratum deterministically instead of crashing scoring.
    order.sort_by(|&a, &b| theta[a].total_cmp(&theta[b]));

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

/// Settings of the latent-class DIF detector (T40).
#[derive(Clone, Copy, Debug)]
pub struct MixtureParams {
    /// Largest number of latent classes tried; the BIC picks among `1..=max_classes`.
    pub max_classes: usize,
    /// Seeded starts per candidate model (the objective is non-convex, as in T48).
    pub n_starts: usize,
    pub seed: u64,
}

impl Default for MixtureParams {
    fn default() -> Self {
        MixtureParams {
            max_classes: 4,
            n_starts: 4,
            seed: 0,
        }
    }
}

/// A class with a smaller share than this is not a population whose difficulties can be
/// estimated (150 of 3000 respondents): its `b_jg` are unidentified, so it does not
/// define `DIF_j` (T40).
pub const MIN_CLASS_SHARE: f64 = 0.05;

const MIXTURE_MAX_ITERS: usize = 1000;

/// The latent-class DIF fit selected by BIC.
#[derive(Clone, Debug)]
pub struct MixtureDif {
    /// Number of latent classes of the selected model; 1 means no mixture was found.
    pub classes: usize,
    /// Whether the selected model lets the discrimination differ by class (non-uniform
    /// DIF) or shares `a_j` across classes (uniform DIF).
    pub non_uniform: bool,
    /// Mixing proportion of each class.
    pub pi: Vec<f64>,
    /// Per-item `DIF_j = max_{g,h} |b_jg − b_jh|` over classes with share ≥
    /// [`MIN_CLASS_SHARE`] — the quantity `docs/02` §B.3 thresholds. 0 with one class.
    pub dif: Vec<f64>,
    /// Per-item `max_{g,h} |a_jg − a_jh|` (non-uniform DIF); 0 when `a_j` is shared.
    pub a_gap: Vec<f64>,
    /// `posterior[i][g]`: probability that respondent `i` belongs to class `g`.
    pub posterior: Vec<Vec<f64>>,
    /// `BIC(one class) − BIC(selected)`: > 0 when a mixture is preferred.
    pub bic_gain: f64,
    /// Every candidate tried: `(classes, non_uniform, BIC)`.
    pub candidates: Vec<(usize, bool, f64)>,
    /// Convergence of the selected fit (docs/08 OPT-001).
    pub status: Convergence,
}

/// One candidate model: `G` classes, with a shared or a per-class discrimination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Model {
    g: usize,
    k: usize,
    per_class_a: bool,
}

// Parameter layout: [ η (G−1, class logits vs class 0) | a (K, or K·G) | b (K·G) ],
// class-major within `a` and `b`: index `g·K + j`.
impl Model {
    fn len(&self) -> usize {
        (self.g - 1) + self.n_a() + self.g * self.k
    }
    fn n_a(&self) -> usize {
        if self.per_class_a {
            self.g * self.k
        } else {
            self.k
        }
    }
    fn a_idx(&self, g: usize, j: usize) -> usize {
        (self.g - 1) + if self.per_class_a { g * self.k + j } else { j }
    }
    fn b_idx(&self, g: usize, j: usize) -> usize {
        (self.g - 1) + self.n_a() + g * self.k + j
    }
    fn free_params(&self) -> usize {
        self.len()
    }
    /// Class proportions from the logits (softmax with class 0 as reference).
    fn pi(&self, p: &[f64]) -> Vec<f64> {
        let mut eta = vec![0.0; self.g];
        eta[1..].copy_from_slice(&p[..self.g - 1]);
        let m = eta.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let e: Vec<f64> = eta.iter().map(|v| (v - m).exp()).collect();
        let s: f64 = e.iter().sum();
        e.iter().map(|v| v / s).collect()
    }
}

/// Per-respondent class log-likelihoods `ℓ_ig = Σ_j x_ij·lo − softplus(lo)` with
/// `lo = a_jg(θ_i − b_jg)`, and their log mixing weights.
fn class_loglik(model: &Model, p: &[f64], theta: &[f64], row: &[f64], i: usize) -> Vec<f64> {
    (0..model.g)
        .map(|g| {
            let mut s = 0.0;
            for (j, &x) in row.iter().enumerate().take(model.k) {
                let lo = p[model.a_idx(g, j)] * (theta[i] - p[model.b_idx(g, j)]);
                s += x * lo - softplus(lo);
            }
            s
        })
        .collect()
}

/// Negative marginal log-likelihood of the mixture: the straightforward form, kept as
/// the reference the fused [`nll_and_grad`] is tested against.
#[cfg(test)]
fn nll(model: &Model, p: &[f64], theta: &[f64], x: &[Vec<f64>]) -> f64 {
    let ln_pi: Vec<f64> = model.pi(p).iter().map(|v| v.ln()).collect();
    let mut total = 0.0;
    for (i, row) in x.iter().enumerate() {
        let ll = class_loglik(model, p, theta, row, i);
        let terms: Vec<f64> = ll.iter().zip(&ln_pi).map(|(l, lp)| l + lp).collect();
        total += logsumexp(&terms);
    }
    -total
}

/// Analytic gradient of [`nll`] (T40; pinned against central differences in the tests).
/// With `r_ig` the class posterior and `e_ijg = x_ij − σ(lo_ijg)`:
/// `∂/∂η_g = −Σ_i (r_ig − π_g)`, `∂/∂a_jg = −Σ_i r_ig e_ijg (θ_i − b_jg)`,
/// `∂/∂b_jg = Σ_i r_ig e_ijg a_jg` (summed over `g` when `a_j` is shared).
#[cfg(test)]
fn nll_grad(model: &Model, p: &[f64], theta: &[f64], x: &[Vec<f64>]) -> Vec<f64> {
    nll_and_grad(model, p, theta, x).1
}

/// [`nll`] and [`nll_grad`] in one pass over the data: per cell, one `exp` gives both
/// `softplus(lo)` and `σ(lo)`.
fn nll_and_grad(model: &Model, p: &[f64], theta: &[f64], x: &[Vec<f64>]) -> (f64, Vec<f64>) {
    let (gn, k) = (model.g, model.k);
    let pi = model.pi(p);
    let ln_pi: Vec<f64> = pi.iter().map(|v| v.ln()).collect();
    let mut grad = vec![0.0; p.len()];
    let mut total = 0.0;
    // Per respondent: class terms, and each cell's residual `x − σ(lo)`.
    let mut terms = vec![0.0; gn];
    let mut resid = vec![0.0; gn * k];
    for (i, row) in x.iter().enumerate() {
        for g in 0..gn {
            let mut s = ln_pi[g];
            for (j, &xij) in row.iter().enumerate().take(k) {
                let lo = p[model.a_idx(g, j)] * (theta[i] - p[model.b_idx(g, j)]);
                let e = (-lo.abs()).exp();
                let (softplus, sig) = if lo > 0.0 {
                    (lo + e.ln_1p(), 1.0 / (1.0 + e))
                } else {
                    (e.ln_1p(), e / (1.0 + e))
                };
                s += xij * lo - softplus;
                resid[g * k + j] = xij - sig;
            }
            terms[g] = s;
        }
        let lse = logsumexp(&terms);
        total += lse;
        for g in 0..gn {
            let r = (terms[g] - lse).exp();
            if g > 0 {
                grad[g - 1] -= r - pi[g];
            }
            for j in 0..k {
                let (ai, bi) = (model.a_idx(g, j), model.b_idx(g, j));
                let e = resid[g * k + j];
                grad[ai] -= r * e * (theta[i] - p[bi]);
                grad[bi] += r * e * p[ai];
            }
        }
    }
    (-total, grad)
}

/// A parameter vector with its NLL and gradient.
type Evaluated = (Vec<f64>, f64, Vec<f64>);

/// Minimizes the NLL of `model` from `p0`.
fn fit_from(
    model: &Model,
    p0: Vec<f64>,
    theta: &[f64],
    x: &[Vec<f64>],
) -> (f64, Vec<f64>, Convergence) {
    // The line search asks for the cost and the gradient at the same point: one fused
    // pass serves both (a cache of the last point; the values are the same bits).
    let cache: RefCell<Option<Evaluated>> = RefCell::new(None);
    let eval = |p: &[f64]| -> (f64, Vec<f64>) {
        if let Some((cp, f, g)) = cache.borrow().as_ref() {
            if cp.as_slice() == p {
                return (*f, g.clone());
            }
        }
        let (f, g) = nll_and_grad(model, p, theta, x);
        *cache.borrow_mut() = Some((p.to_vec(), f, g.clone()));
        (f, g)
    };
    let m = lbfgs(
        p0,
        |p| eval(p).0,
        |p| eval(p).1,
        10,
        MIXTURE_MAX_ITERS,
        1e-6,
    );
    (eval(&m.x).0, m.x, m.status)
}

/// Variant 2 (`docs/02` §B.3): a latent-class IRT mixture
/// `P(x_ij = 1 | θ_i, g) = σ(a_jg(θ_i − b_jg))` on a batch of `k` items, with the number
/// of classes (1..=4) and uniform vs non-uniform DIF chosen by BIC, each candidate from
/// several seeded starts (T40). `theta` is the anchor ability; `x` is respondents × items.
pub fn mixture_dif(theta: &[f64], x: &[Vec<f64>], k: usize, seed: u64) -> MixtureDif {
    mixture_dif_with(
        theta,
        x,
        k,
        &MixtureParams {
            seed,
            ..MixtureParams::default()
        },
    )
}

/// [`mixture_dif`] with explicit settings.
pub fn mixture_dif_with(theta: &[f64], x: &[Vec<f64>], k: usize, mp: &MixtureParams) -> MixtureDif {
    let nt = x.len();
    let ln_n = (nt.max(1) as f64).ln();
    let bic = |model: &Model, nll: f64| 2.0 * nll + model.free_params() as f64 * ln_n;

    // One class: plain 2PL on fixed θ, convex; its solution seeds every mixture start.
    let one = Model {
        g: 1,
        k,
        per_class_a: false,
    };
    let mut p1 = vec![0.0; one.len()];
    for j in 0..k {
        p1[one.a_idx(0, j)] = 1.0;
    }
    let (nll1, p1, st1) = fit_from(&one, p1, theta, x);
    let bic1 = bic(&one, nll1);

    let mut candidates = vec![(1, false, bic1)];
    let mut best = (bic1, one, p1.clone(), st1);
    let mut rng = ChaCha8Rng::seed_from_u64(mp.seed);
    for g in 2..=mp.max_classes.max(1) {
        // Staged search: once adding a class no longer lowers the BIC, larger mixtures
        // (slower, and increasingly unidentified) are not tried.
        let best_before = best.0;
        for per_class_a in [false, true] {
            let model = Model { g, k, per_class_a };
            let mut chosen: Option<(f64, Vec<f64>, Convergence)> = None;
            for _ in 0..mp.n_starts.max(1) {
                let mut p0 = vec![0.0; model.len()];
                for v in p0.iter_mut().take(g - 1) {
                    *v = normal(&mut rng) * 0.3;
                }
                for c in 0..g {
                    for j in 0..k {
                        p0[model.a_idx(c, j)] = p1[one.a_idx(0, j)];
                        p0[model.b_idx(c, j)] = p1[one.b_idx(0, j)] + normal(&mut rng) * 0.3;
                    }
                }
                let fit = fit_from(&model, p0, theta, x);
                // Prefer a converged start; among equals, the lowest NLL (earliest on a tie).
                let better = match &chosen {
                    None => true,
                    Some((f, _, s)) => {
                        let (conv, prev_conv) = (
                            fit.2 == Convergence::Converged,
                            *s == Convergence::Converged,
                        );
                        (conv && !prev_conv) || (conv == prev_conv && fit.0 < *f)
                    }
                };
                if better {
                    chosen = Some(fit);
                }
            }
            let (f, p, st) = chosen.expect("at least one start");
            let b = bic(&model, f);
            candidates.push((g, per_class_a, b));
            if st == Convergence::Converged && b < best.0 {
                best = (b, model, p, st);
            }
        }
        if best.0 >= best_before {
            break;
        }
    }

    let (best_bic, model, p, status) = best;
    let pi = model.pi(&p);
    let counted: Vec<usize> = (0..model.g).filter(|&g| pi[g] >= MIN_CLASS_SHARE).collect();
    let gap = |idx: &dyn Fn(usize, usize) -> usize, j: usize| -> f64 {
        let vals: Vec<f64> = counted.iter().map(|&g| p[idx(g, j)]).collect();
        let hi = vals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let lo = vals.iter().copied().fold(f64::INFINITY, f64::min);
        if vals.len() < 2 {
            0.0
        } else {
            hi - lo
        }
    };
    let dif: Vec<f64> = (0..k).map(|j| gap(&|g, j| model.b_idx(g, j), j)).collect();
    let a_gap: Vec<f64> = (0..k).map(|j| gap(&|g, j| model.a_idx(g, j), j)).collect();

    let ln_pi: Vec<f64> = pi.iter().map(|v| v.ln()).collect();
    let posterior: Vec<Vec<f64>> = x
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let ll = class_loglik(&model, &p, theta, row, i);
            let terms: Vec<f64> = ll.iter().zip(&ln_pi).map(|(l, lp)| l + lp).collect();
            let lse = logsumexp(&terms);
            terms.iter().map(|t| (t - lse).exp()).collect()
        })
        .collect();

    MixtureDif {
        classes: model.g,
        non_uniform: model.per_class_a,
        pi,
        dif,
        a_gap,
        posterior,
        bic_gain: bic1 - best_bic,
        candidates,
        status,
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

fn logsumexp(v: &[f64]) -> f64 {
    let m = v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if m == f64::NEG_INFINITY {
        return m;
    }
    m + v.iter().map(|x| (x - m).exp()).sum::<f64>().ln()
}

fn normal(rng: &mut ChaCha8Rng) -> f64 {
    let u1: f64 = 1.0 - rng.gen::<f64>();
    let u2: f64 = rng.gen::<f64>();
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::optim::numerical_gradient;

    /// The BIC penalty counts the free parameters: `G − 1` proportions, `K` (shared) or
    /// `K·G` (per-class) discriminations, and `K·G` difficulties.
    #[test]
    fn free_parameters_are_counted_as_specified() {
        let k = 8;
        for g in 1..=4 {
            let shared = Model {
                g,
                k,
                per_class_a: false,
            };
            let per_class = Model {
                g,
                k,
                per_class_a: true,
            };
            assert_eq!(shared.free_params(), (g - 1) + k + k * g, "G={g} shared");
            assert_eq!(
                per_class.free_params(),
                (g - 1) + 2 * k * g,
                "G={g} per-class"
            );
        }
    }

    /// The analytic mixture gradient matches central differences for every model shape:
    /// 1–3 classes, shared or per-class discrimination (T40).
    #[test]
    fn mixture_gradient_matches_central_differences() {
        let mut rng = ChaCha8Rng::seed_from_u64(3);
        let (n, k) = (40, 4);
        let theta: Vec<f64> = (0..n).map(|_| normal(&mut rng)).collect();
        let x: Vec<Vec<f64>> = (0..n)
            .map(|_| {
                (0..k)
                    .map(|_| (rng.gen::<f64>() < 0.5) as i32 as f64)
                    .collect()
            })
            .collect();
        for g in 1..=3 {
            for per_class_a in [false, true] {
                let model = Model { g, k, per_class_a };
                let p: Vec<f64> = (0..model.len()).map(|_| normal(&mut rng) * 0.7).collect();
                let analytic = nll_grad(&model, &p, &theta, &x);
                let fused = nll_and_grad(&model, &p, &theta, &x).0;
                let reference = nll(&model, &p, &theta, &x);
                assert!(
                    (fused - reference).abs() <= 1e-10 * reference.abs(),
                    "g={g}: NLL {fused} vs {reference}"
                );
                let numeric = numerical_gradient(&|q: &[f64]| nll(&model, q, &theta, &x), &p, 1e-6);
                for (i, (a, b)) in analytic.iter().zip(&numeric).enumerate() {
                    assert!(
                        (a - b).abs() <= 1e-5 * (1.0 + b.abs()),
                        "g={g} per_class_a={per_class_a} param {i}: {a} vs {b}"
                    );
                }
            }
        }
    }
}
