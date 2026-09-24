//! Level A — bridging. See `docs/02-scoring-engine.md`, §A.
//!
//! Matrix factorization `r̂_uj = μ + b_u + b_j + ⟨f_u, f_j⟩` with asymmetric
//! regularization (`λ_b ≫ λ_f`). The bridge score is the item intercept `b_j`.
//! This module covers the `d = 1` case. Reference prototype: `sim/bridging_irt_dif.py`.

use crate::optim::{lbfgs, Convergence};
use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

#[derive(Clone, Copy, Debug)]
pub struct Obs {
    pub u: usize,
    pub j: usize,
    pub r: f64,
}

#[derive(Clone, Debug)]
pub struct Ratings {
    pub n: usize,
    pub m: usize,
    pub obs: Vec<Obs>,
    /// Per-reviewer weight `w_u`, length `n`; uniform (1.0) by default. The fit
    /// minimizes `Σ w_u (r_uj − r̂_uj)²` (docs/08 BRIDGE-007, G-03): `w_u` is the
    /// reviewer's `discount(cap(E_u))` (probation = 0) from the *previous* epoch.
    pub weights: Vec<f64>,
}

impl Ratings {
    /// Builds observations in canonical `(u, j)` order (see [`Ratings::canonical`]),
    /// with uniform reviewer weights (call [`Ratings::with_weights`] to set them).
    pub fn from_dense(r: &[Vec<f64>], mask: &[Vec<bool>]) -> Self {
        let n = r.len();
        let m = if n > 0 { r[0].len() } else { 0 };
        let mut obs = Vec::new();
        for u in 0..n {
            for j in 0..m {
                if mask[u][j] {
                    obs.push(Obs { u, j, r: r[u][j] });
                }
            }
        }
        Ratings {
            n,
            m,
            obs,
            weights: vec![1.0; n],
        }
    }

    /// Sets the per-reviewer weights `w_u` (docs/08 BRIDGE-007). One weight per reviewer:
    /// a vector of another length is refused by [`Ratings::validate`] at the fit
    /// (`RatingsError::WeightCount`), not asserted here (T62).
    pub fn with_weights(mut self, weights: Vec<f64>) -> Self {
        self.weights = weights;
        self
    }

    /// Checks the input the fit relies on (T62, `docs/12` §2.3): one finite, non-negative
    /// weight per reviewer; every observation inside `[0, n) × [0, m)` with a finite
    /// rating; no `(u, j)` pair observed twice (it would count twice in the objective).
    /// [`fit`] and [`bridge_scores`] run it first, so malformed input is an error, never
    /// a panic inside the objective. The first problem found is reported, in this order.
    pub fn validate(&self) -> Result<(), RatingsError> {
        if self.weights.len() != self.n {
            return Err(RatingsError::WeightCount {
                expected: self.n,
                found: self.weights.len(),
            });
        }
        if let Some(u) = self.weights.iter().position(|w| !w.is_finite() || *w < 0.0) {
            return Err(RatingsError::BadWeight { u });
        }
        for o in &self.obs {
            if o.u >= self.n || o.j >= self.m {
                return Err(RatingsError::IndexOutOfRange {
                    u: o.u,
                    j: o.j,
                    n: self.n,
                    m: self.m,
                });
            }
            if !o.r.is_finite() {
                return Err(RatingsError::NonFiniteRating { u: o.u, j: o.j });
            }
        }
        let mut pairs: Vec<(usize, usize)> = self.obs.iter().map(|o| (o.u, o.j)).collect();
        pairs.sort_unstable();
        if let Some(w) = pairs.windows(2).find(|w| w[0] == w[1]) {
            return Err(RatingsError::DuplicateObservation {
                u: w[0].0,
                j: w[0].1,
            });
        }
        Ok(())
    }

    /// A copy with `obs` in canonical order (sorted by `(u, j)`, then rating bits), so
    /// the fit is invariant to input order (docs/08 INV-13, REPRO-002). Weights are
    /// indexed by reviewer, so they are unaffected by the observation order.
    pub fn canonical(&self) -> Ratings {
        let mut obs = self.obs.clone();
        obs.sort_by(|a, b| (a.u, a.j, a.r.to_bits()).cmp(&(b.u, b.j, b.r.to_bits())));
        Ratings {
            n: self.n,
            m: self.m,
            obs,
            weights: self.weights.clone(),
        }
    }
}

/// Why a [`Ratings`] cannot be fitted (T62): the engine refuses malformed input with an
/// error instead of panicking inside the objective.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RatingsError {
    /// An observation names a reviewer or item outside `[0, n) × [0, m)`.
    IndexOutOfRange {
        u: usize,
        j: usize,
        n: usize,
        m: usize,
    },
    /// `weights` does not hold one weight per reviewer.
    WeightCount { expected: usize, found: usize },
    /// A rating that is not a finite number.
    NonFiniteRating { u: usize, j: usize },
    /// A weight that is not a finite, non-negative number.
    BadWeight { u: usize },
    /// The same `(u, j)` pair observed twice.
    DuplicateObservation { u: usize, j: usize },
    /// A requested item index outside `[0, m)`.
    ItemOutOfRange { j: usize, m: usize },
}

impl std::fmt::Display for RatingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RatingsError::IndexOutOfRange { u, j, n, m } => {
                write!(
                    f,
                    "observation ({u}, {j}) outside {n} reviewers × {m} items"
                )
            }
            RatingsError::WeightCount { expected, found } => {
                write!(f, "{found} weights for {expected} reviewers")
            }
            RatingsError::NonFiniteRating { u, j } => write!(f, "rating ({u}, {j}) is not finite"),
            RatingsError::BadWeight { u } => {
                write!(
                    f,
                    "weight of reviewer {u} is not a finite, non-negative number"
                )
            }
            RatingsError::DuplicateObservation { u, j } => {
                write!(f, "observation ({u}, {j}) appears twice")
            }
            RatingsError::ItemOutOfRange { j, m } => write!(f, "item {j} outside {m} items"),
        }
    }
}

impl std::error::Error for RatingsError {}

#[derive(Clone, Copy, Debug)]
pub struct BridgingParams {
    pub lam_b: f64,
    pub lam_f: f64,
    pub m_hist: usize,
    pub max_iters: usize,
    pub g_tol: f64,
    pub seed: u64,
    /// Independent starts of the non-convex fit (T48); the lowest objective wins. Start
    /// `k` is seeded `seed + k`, so `n_starts = 1` is the single seeded fit.
    pub n_starts: usize,
}

impl Default for BridgingParams {
    fn default() -> Self {
        BridgingParams {
            lam_b: 0.15,
            lam_f: 0.03,
            m_hist: 10,
            max_iters: 4000,
            g_tol: 1e-7,
            seed: 0,
            n_starts: DEFAULT_STARTS,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Fit {
    pub mu: f64,
    pub b_u: Vec<f64>,
    pub b_j: Vec<f64>,
    pub f_u: Vec<f64>,
    pub f_j: Vec<f64>,
    /// Convergence of the L-BFGS fit (docs/08 OPT-001).
    pub status: Convergence,
}

// Parameter vector layout: [ μ | b_u(n) | b_j(m) | f_u(n) | f_j(m) ].
struct Layout {
    n: usize,
    m: usize,
}
impl Layout {
    #[inline]
    fn mu(&self, x: &[f64]) -> f64 {
        x[0]
    }
    #[inline]
    fn bu<'a>(&self, x: &'a [f64]) -> &'a [f64] {
        &x[1..1 + self.n]
    }
    #[inline]
    fn bj<'a>(&self, x: &'a [f64]) -> &'a [f64] {
        &x[1 + self.n..1 + self.n + self.m]
    }
    #[inline]
    fn fu<'a>(&self, x: &'a [f64]) -> &'a [f64] {
        &x[1 + self.n + self.m..1 + 2 * self.n + self.m]
    }
    #[inline]
    fn fj<'a>(&self, x: &'a [f64]) -> &'a [f64] {
        &x[1 + 2 * self.n + self.m..]
    }
}

/// Default number of starts (T48): see `docs/08` BRIDGE-001 for the measurements.
pub const DEFAULT_STARTS: usize = 8;

/// The bridging fit. The objective is non-convex and has distinct local minima on real
/// data, which a single seeded start reaches depending on the seed — sometimes on
/// either side of `τ` (`docs/08` BRIDGE-001, T48). So the fit runs `n_starts` seeded
/// starts and keeps the lowest objective (the earliest start on a tie): deterministic,
/// and the verdict no longer hinges on one start's basin. Malformed input is refused with
/// a [`RatingsError`] before anything is computed (T62).
pub fn fit(data: &Ratings, p: &BridgingParams) -> Result<Fit, RatingsError> {
    data.validate()?;
    Ok(fit_validated(data, p))
}

/// [`fit`] on input [`Ratings::validate`] has accepted.
fn fit_validated(data: &Ratings, p: &BridgingParams) -> Fit {
    // Canonicalize: the init mean and cost/grad sums are order-dependent (INV-13).
    let data = data.canonical();
    let mut best: Option<(f64, Fit)> = None;
    for k in 0..p.n_starts.max(1) {
        let x0 = random_init(&data, p.seed.wrapping_add(k as u64));
        let f = fit_with_init(&data, p, x0);
        let obj = objective(&data, p, &pack(&f));
        if best.as_ref().is_none_or(|(b, _)| obj < *b) {
            best = Some((obj, f));
        }
    }
    let mut f = best.expect("at least one start").1;
    canonical_sign(&mut f);
    f
}

/// `f` is identified only up to sign (the objective is unchanged by `(f_u, f_j) →
/// (−f_u, −f_j)`), so the start that wins can return either twin. Downstream draws order
/// reviewers by `f_u`, so the sign is fixed: the largest `|f_j|` (earliest on a tie) is
/// made non-negative. Exact negation: `b_j` and the objective are untouched (T48).
fn canonical_sign(f: &mut Fit) {
    let lead = f.f_j.iter().copied().fold(
        0.0_f64,
        |best, v| if v.abs() > best.abs() { v } else { best },
    );
    if lead < 0.0 {
        for v in f.f_u.iter_mut().chain(f.f_j.iter_mut()) {
            *v = -*v;
        }
    }
}

// RNG consumption order is part of the reproducibility contract.
fn random_init(data: &Ratings, seed: u64) -> Vec<f64> {
    let (n, m) = (data.n, data.m);
    // Weighted like the objective, so a zero-weight (probation) reviewer's ratings do not
    // move the start point either: on this non-convex objective a different start can
    // land in a different minimum, and "weight 0" must mean "absent" (T42). With uniform
    // weights this is bit-identical to the plain mean.
    let (sum_wr, sum_w) = data.obs.iter().fold((0.0, 0.0), |(swr, sw), o| {
        let w = data.weights[o.u];
        (swr + w * o.r, sw + w)
    });
    let mean_r = if sum_w > 0.0 { sum_wr / sum_w } else { 0.0 };
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut x0 = vec![0.0_f64; 1 + 2 * n + 2 * m];
    x0[0] = mean_r;
    for i in 0..n {
        x0[1 + i] = normal(&mut rng) * 0.1;
    }
    for j in 0..m {
        x0[1 + n + j] = normal(&mut rng) * 0.1;
    }
    for i in 0..n {
        x0[1 + n + m + i] = normal(&mut rng) * 0.3;
    }
    for j in 0..m {
        x0[1 + 2 * n + m + j] = normal(&mut rng) * 0.3;
    }
    x0
}

fn pack(f: &Fit) -> Vec<f64> {
    let mut x = Vec::with_capacity(1 + 2 * f.b_u.len() + 2 * f.b_j.len());
    x.push(f.mu);
    x.extend_from_slice(&f.b_u);
    x.extend_from_slice(&f.b_j);
    x.extend_from_slice(&f.f_u);
    x.extend_from_slice(&f.f_j);
    x
}

// The weighted objective `Σ w_u (r − r̂)² + λ_b‖b‖² + λ_f‖f‖²` (docs/08 BRIDGE-007).
fn objective(data: &Ratings, p: &BridgingParams, x: &[f64]) -> f64 {
    let lay = Layout {
        n: data.n,
        m: data.m,
    };
    let w = &data.weights;
    let mu = lay.mu(x);
    let (bu, bj, fu, fj) = (lay.bu(x), lay.bj(x), lay.fu(x), lay.fj(x));
    let mut se = 0.0;
    for o in &data.obs {
        let pred = mu + bu[o.u] + bj[o.j] + fu[o.u] * fj[o.j];
        let e = pred - o.r;
        se += w[o.u] * e * e;
    }
    let reg_b: f64 = bu.iter().map(|v| v * v).sum::<f64>() + bj.iter().map(|v| v * v).sum::<f64>();
    let reg_f: f64 = fu.iter().map(|v| v * v).sum::<f64>() + fj.iter().map(|v| v * v).sum::<f64>();
    se + p.lam_b * reg_b + p.lam_f * reg_f
}

// Analytic gradient of [`objective`]; pinned against central differences in the tests.
fn gradient(data: &Ratings, p: &BridgingParams, x: &[f64]) -> Vec<f64> {
    let (n, m) = (data.n, data.m);
    let lay = Layout { n, m };
    let w = &data.weights;
    let mu = lay.mu(x);
    let (bu, bj, fu, fj) = (lay.bu(x), lay.bj(x), lay.fu(x), lay.fj(x));
    let mut g = vec![0.0_f64; x.len()];
    for o in &data.obs {
        let pred = mu + bu[o.u] + bj[o.j] + fu[o.u] * fj[o.j];
        let e = 2.0 * w[o.u] * (pred - o.r);
        g[0] += e;
        g[1 + o.u] += e;
        g[1 + n + o.j] += e;
        g[1 + n + m + o.u] += e * fj[o.j];
        g[1 + 2 * n + m + o.j] += e * fu[o.u];
    }
    for i in 0..n {
        g[1 + i] += 2.0 * p.lam_b * bu[i];
        g[1 + n + m + i] += 2.0 * p.lam_f * fu[i];
    }
    for j in 0..m {
        g[1 + n + j] += 2.0 * p.lam_b * bj[j];
        g[1 + 2 * n + m + j] += 2.0 * p.lam_f * fj[j];
    }
    g
}

fn fit_with_init(data: &Ratings, p: &BridgingParams, x0: Vec<f64>) -> Fit {
    let lay = Layout {
        n: data.n,
        m: data.m,
    };
    let cost = |x: &[f64]| objective(data, p, x);
    let grad = |x: &[f64]| gradient(data, p, x);

    let m = lbfgs(x0, cost, grad, p.m_hist, p.max_iters, p.g_tol);
    let x = m.x;

    Fit {
        mu: lay.mu(&x),
        b_u: lay.bu(&x).to_vec(),
        b_j: lay.bj(&x).to_vec(),
        f_u: lay.fu(&x).to_vec(),
        f_j: lay.fj(&x).to_vec(),
        status: m.status,
    }
}

/// Robust bridge score (`docs/02`, §A.4): bootstrap-min over `n_bootstrap`
/// subsamples (each observation kept with probability `keep_frac`). Malformed input is
/// refused with a [`RatingsError`] (T62).
pub fn bridge_scores(
    data: &Ratings,
    p: &BridgingParams,
    n_bootstrap: usize,
    keep_frac: f64,
) -> Result<Vec<f64>, RatingsError> {
    data.validate()?;
    // Canonicalize: the bootstrap subsampling walks `obs` in order (INV-13, REPRO-002).
    let data = data.canonical();

    // Warm-start each subsample from the full fit: the bilinear term makes the
    // objective non-convex, so independent random inits would let some subsamples
    // land in a different minimum, polluting the min with optimizer noise.
    let full = fit_validated(&data, p);
    let anchor = pack(&full);

    let mut best = full.b_j.clone();
    for s in 0..n_bootstrap {
        let mut rng = ChaCha8Rng::seed_from_u64(p.seed.wrapping_add(100 + s as u64));
        let sub_obs: Vec<Obs> = data
            .obs
            .iter()
            .copied()
            .filter(|_| rng.gen::<f64>() < keep_frac)
            .collect();
        let sub = Ratings {
            n: data.n,
            m: data.m,
            obs: sub_obs,
            weights: data.weights.clone(),
        };
        let mut x0 = anchor.clone();
        for v in x0.iter_mut() {
            *v += normal(&mut rng) * 0.02;
        }
        let fit_s = fit_with_init(&sub, p, x0);
        for (b, &bj) in best.iter_mut().zip(fit_s.b_j.iter()) {
            if bj < *b {
                *b = bj;
            }
        }
    }
    Ok(best)
}

// Standard normal via Box–Muller, to control exact RNG consumption order.
fn normal(rng: &mut ChaCha8Rng) -> f64 {
    let u1: f64 = 1.0 - rng.gen::<f64>();
    let u2: f64 = rng.gen::<f64>();
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small ratings set with every kind of term active: missing cells, non-uniform
    /// weights (one zero), and both regularizers.
    fn sample() -> (Ratings, BridgingParams) {
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        let (n, m) = (6, 5);
        let r: Vec<Vec<f64>> = (0..n)
            .map(|_| (0..m).map(|_| rng.gen::<f64>()).collect())
            .collect();
        let mask: Vec<Vec<bool>> = (0..n)
            .map(|u| (0..m).map(|j| (u + 2 * j) % 4 != 0).collect())
            .collect();
        let data = Ratings::from_dense(&r, &mask).with_weights(vec![1.0, 0.5, 2.0, 0.0, 1.5, 0.8]);
        (data, BridgingParams::default())
    }

    /// The analytic gradient must match central differences of the objective in every
    /// component: a wrong term still lets L-BFGS report `Converged` near the true
    /// minimum and shifts `b_j` by less than the oracle tolerance (T41).
    #[test]
    fn gradient_matches_central_differences() {
        let (data, p) = sample();
        let dim = 1 + 2 * data.n + 2 * data.m;
        let mut rng = ChaCha8Rng::seed_from_u64(11);
        for _ in 0..5 {
            let x: Vec<f64> = (0..dim).map(|_| normal(&mut rng)).collect();
            let g = gradient(&data, &p, &x);
            let h = 1e-6;
            for k in 0..dim {
                let (mut xp, mut xm) = (x.clone(), x.clone());
                xp[k] += h;
                xm[k] -= h;
                let num = (objective(&data, &p, &xp) - objective(&data, &p, &xm)) / (2.0 * h);
                assert!(
                    (g[k] - num).abs() <= 1e-6 * (1.0 + num.abs()),
                    "component {k}: analytic {} vs numerical {num}",
                    g[k]
                );
            }
        }
    }

    use proptest::prelude::*;

    /// Random data and a random point in the parameter space.
    fn data_and_point() -> impl Strategy<Value = (Ratings, Vec<f64>)> {
        (2usize..7, 2usize..5).prop_flat_map(|(n, m)| {
            (
                prop::collection::vec(prop::collection::vec(0.0f64..=1.0, m), n),
                prop::collection::vec(prop::collection::vec(prop::bool::weighted(0.8), m), n),
                prop::collection::vec(0.0f64..2.0, n),
                prop::collection::vec(-2.0f64..2.0, 1 + 2 * n + 2 * m),
            )
                .prop_map(|(r, mask, w, x)| (Ratings::from_dense(&r, &mask).with_weights(w), x))
        })
    }

    proptest! {
        /// `f` is identified only up to sign: `(f_u, f_j) → (−f_u, −f_j)` leaves the
        /// objective unchanged bit for bit, so `b_j` — the bridge score — cannot depend
        /// on which sign the fit picks (T42).
        #[test]
        fn the_objective_is_symmetric_in_the_sign_of_f((data, x) in data_and_point()) {
            let p = BridgingParams::default();
            let mut flipped = x.clone();
            for v in &mut flipped[1 + data.n + data.m..] {
                *v = -*v;
            }
            prop_assert_eq!(
                objective(&data, &p, &x).to_bits(),
                objective(&data, &p, &flipped).to_bits()
            );
        }

        /// The analytic gradient matches central differences on any data and point.
        #[test]
        fn the_gradient_matches_central_differences_anywhere((data, x) in data_and_point()) {
            let p = BridgingParams::default();
            let g = gradient(&data, &p, &x);
            let h = 1e-6;
            for k in 0..x.len() {
                let (mut xp, mut xm) = (x.clone(), x.clone());
                xp[k] += h;
                xm[k] -= h;
                let num = (objective(&data, &p, &xp) - objective(&data, &p, &xm)) / (2.0 * h);
                prop_assert!((g[k] - num).abs() <= 1e-6 * (1.0 + num.abs()), "component {}", k);
            }
        }
    }

    /// The objective itself: the weighted squared error plus both penalties, on a point
    /// small enough to compute by hand.
    #[test]
    fn objective_is_weighted_error_plus_penalties() {
        // One reviewer, one item, weight 2: x = [μ, b_u, b_j, f_u, f_j].
        let data = Ratings::from_dense(&[vec![1.0]], &[vec![true]]).with_weights(vec![2.0]);
        let p = BridgingParams::default();
        let x = [0.5, 0.1, 0.2, 0.3, 0.4];
        let pred = 0.5 + 0.1 + 0.2 + 0.3 * 0.4;
        let want = 2.0 * (pred - 1.0_f64).powi(2)
            + p.lam_b * (0.1_f64.powi(2) + 0.2_f64.powi(2))
            + p.lam_f * (0.3_f64.powi(2) + 0.4_f64.powi(2));
        assert!((objective(&data, &p, &x) - want).abs() < 1e-15);
    }
}
