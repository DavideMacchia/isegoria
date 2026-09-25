//! Level A — bridging. See `docs/02-scoring-engine.md`, §A.
//!
//! Matrix factorization `r̂_uj = μ + b_u + b_j + ⟨f_u, f_j⟩` with asymmetric
//! regularization (`λ_b ≫ λ_f`). The bridge score is the *side-balanced predicted
//! approval* (`docs/01` D32, T49): reviewers split into two sides on `f_u`, the model's
//! predictions averaged within each side, the two sides averaged with equal weight
//! ([`side_balanced`]). The item intercept `b_j` is still reported, but the gate no longer
//! reads it: it is relative to the batch and partly majoritarian (`docs/08` BRIDGE-008/009).
//! This module covers the `d = 1` case. Reference prototype: `sim/bridging_irt_dif.py`.

use crate::fmath::{cos, ln};
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
    /// Per-reviewer flag, length `n`: whether the reviewer defines the latent axis
    /// (`docs/02` §A.4, T39). A reviewer below the review floor `n_min` is absent from
    /// the core fit — it shapes neither `f_j` nor `b_j` — and is then placed on the fixed
    /// axis by projection: a position of its own (`f_u`, `b_u`) and no influence on
    /// anyone else; it enters no side of the side-balanced score. All `true` by default
    /// (call [`Ratings::with_axis`] to set it).
    pub axis: Vec<bool>,
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
            axis: vec![true; n],
        }
    }

    /// Sets which reviewers define the latent axis (`docs/02` §A.4, T39): `false` keeps
    /// the reviewer out of the core fit and places it on the fixed axis afterwards. One
    /// flag per reviewer: a vector of another length is refused by [`Ratings::validate`]
    /// at the fit (`RatingsError::AxisCount`).
    pub fn with_axis(mut self, axis: Vec<bool>) -> Self {
        self.axis = axis;
        self
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
        if self.axis.len() != self.n {
            return Err(RatingsError::AxisCount {
                expected: self.n,
                found: self.axis.len(),
            });
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
            axis: self.axis.clone(),
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
    /// `axis` does not hold one flag per reviewer (T39).
    AxisCount { expected: usize, found: usize },
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
            RatingsError::AxisCount { expected, found } => {
                write!(f, "{found} axis flags for {expected} reviewers")
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
    /// Which reviewers defined the axis (`Ratings::axis`, T39): the others were placed on
    /// it by projection and enter no side of the side-balanced score.
    pub axis: Vec<bool>,
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
    // The core fit is the axis reviewers' (T39): a reviewer off the axis is absent from
    // it — as a zero-weight reviewer is (T42) — so it shapes neither the axis nor the
    // item levels; it is placed on the fixed axis afterwards. With everyone on the axis
    // the core is the data itself.
    let off_axis: Vec<usize> = (0..data.n).filter(|&u| !data.axis[u]).collect();
    let core = if off_axis.is_empty() {
        data.clone()
    } else {
        let mut weights = data.weights.clone();
        for &u in &off_axis {
            weights[u] = 0.0;
        }
        Ratings {
            weights,
            ..data.clone()
        }
    };
    let mut best: Option<(f64, Fit)> = None;
    for k in 0..p.n_starts.max(1) {
        let x0 = random_init(&core, p.seed.wrapping_add(k as u64));
        let f = fit_with_init(&core, p, x0);
        let obj = objective(&core, p, &pack(&f));
        if best.as_ref().is_none_or(|(b, _)| obj < *b) {
            best = Some((obj, f));
        }
    }
    let mut f = best.expect("at least one start").1;
    canonical_sign(&mut f);
    for &u in &off_axis {
        let (b_u, f_u) = project(&data, p, &f, u);
        f.b_u[u] = b_u;
        f.f_u[u] = f_u;
    }
    f
}

/// The position of a reviewer off the axis (T39): its `(b_u, f_u)` on the fixed axis
/// `(μ, b_j, f_j)` of the core fit, by ridge least squares over its own ratings at unit
/// weight — the same `λ_b`, `λ_f` as the fit. A placement in the space, with no
/// influence on anyone else; without ratings it is the origin.
fn project(data: &Ratings, p: &BridgingParams, f: &Fit, u: usize) -> (f64, f64) {
    let (mut s1, mut sx, mut sxx, mut sy, mut sxy) = (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
    for o in data.obs.iter().filter(|o| o.u == u) {
        let x = f.f_j[o.j];
        let y = o.r - f.mu - f.b_j[o.j];
        s1 += 1.0;
        sx += x;
        sxx += x * x;
        sy += y;
        sxy += x * y;
    }
    let (a11, a12, a22) = (s1 + p.lam_b, sx, sxx + p.lam_f);
    let det = a11 * a22 - a12 * a12;
    if det.is_nan() || det <= 0.0 {
        return (0.0, 0.0);
    }
    ((a22 * sy - a12 * sxy) / det, (a11 * sxy - a12 * sy) / det)
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
        axis: data.axis.clone(),
        status: m.status,
    }
}

/// Which of the two sides a reviewer falls on (D32): a deterministic one-dimensional
/// 2-means on `f_u`, initialized at its minimum (side A) and maximum (side B). The
/// orientation follows the canonical sign of `f` (T48); the score is symmetric in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    A,
    B,
}

/// The side-balanced bridge score (`docs/02` §A.3, D32, T49). The reviewers who define
/// the axis (`Fit::axis`, T39) are split into two sides by [`two_means`] on `f_u`; the
/// model's predicted ratings `r̂_uj` — every such reviewer's, rated or not — are averaged
/// within each side; the score is the mean of the two side averages, so each side counts
/// once whatever its size, and the gap between them is the item's polarization
/// (`docs/05` [5b]). A reviewer off the axis carries the nominal side of the nearer
/// centre and enters no average. With one side only (every reviewer at one position)
/// both side means are the mean over all axis reviewers; with none they are `μ + b_j`,
/// the prediction for a neutral reviewer.
#[derive(Clone, Debug, PartialEq)]
pub struct SideScores {
    /// Each reviewer's side, in reviewer order.
    pub side: Vec<Side>,
    /// Mean predicted rating over side A, per item.
    pub side_a: Vec<f64>,
    /// Mean predicted rating over side B, per item.
    pub side_b: Vec<f64>,
    /// The bridge score `S_j = (side_a + side_b) / 2`.
    pub score: Vec<f64>,
    /// The polarization `|side_a − side_b|`.
    pub gap: Vec<f64>,
}

/// Deterministic 1-D 2-means on `f_u`, initialized at its extremes (D32). A tie goes to
/// side A, an empty side keeps its centre, and the loop stops when both centres are
/// unchanged (to the tolerance NumPy's `allclose` uses, as in the paper's script) or after
/// 100 rounds. Reviewer order is the only order used, so the result is reproducible.
pub fn two_means(f_u: &[f64]) -> Vec<Side> {
    if f_u.is_empty() {
        return Vec::new();
    }
    let lo = f_u.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = f_u.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let mut centres = [lo, hi];
    let mut side = vec![Side::A; f_u.len()];
    let close = |a: f64, b: f64| (a - b).abs() <= 1e-8 + 1e-5 * b.abs();
    for _ in 0..100 {
        for (s, &f) in side.iter_mut().zip(f_u) {
            *s = if (f - centres[0]).abs() <= (f - centres[1]).abs() {
                Side::A
            } else {
                Side::B
            };
        }
        let (mut sum, mut count) = ([0.0_f64; 2], [0usize; 2]);
        for (s, &f) in side.iter().zip(f_u) {
            let k = *s as usize;
            sum[k] += f;
            count[k] += 1;
        }
        let next = [
            if count[0] > 0 {
                sum[0] / count[0] as f64
            } else {
                centres[0]
            },
            if count[1] > 0 {
                sum[1] / count[1] as f64
            } else {
                centres[1]
            },
        ];
        if close(next[0], centres[0]) && close(next[1], centres[1]) {
            break;
        }
        centres = next;
    }
    side
}

/// The side-balanced score of every item of a fit (see [`SideScores`]).
pub fn side_balanced(fit: &Fit) -> SideScores {
    let (n, m) = (fit.b_u.len(), fit.b_j.len());
    let on_axis: Vec<usize> = (0..n)
        .filter(|&u| fit.axis.get(u).copied().unwrap_or(true))
        .collect();
    let side = if on_axis.len() == n {
        two_means(&fit.f_u)
    } else {
        // The sides are formed by the axis reviewers; an off-axis reviewer is labelled by
        // the nearer side centre (a tie to A) and counts in no average (T39).
        let f_axis: Vec<f64> = on_axis.iter().map(|&u| fit.f_u[u]).collect();
        let axis_sides = two_means(&f_axis);
        let (mut sum, mut count) = ([0.0_f64; 2], [0usize; 2]);
        for (s, &f) in axis_sides.iter().zip(&f_axis) {
            sum[*s as usize] += f;
            count[*s as usize] += 1;
        }
        let centre = |k: usize| {
            if count[k] > 0 {
                sum[k] / count[k] as f64
            } else {
                0.0
            }
        };
        let (ca, cb) = (centre(0), centre(1));
        let mut axis_side = axis_sides.iter();
        (0..n)
            .map(|u| {
                if fit.axis[u] {
                    *axis_side.next().expect("one side per axis reviewer")
                } else if (fit.f_u[u] - ca).abs() <= (fit.f_u[u] - cb).abs() {
                    Side::A
                } else {
                    Side::B
                }
            })
            .collect()
    };
    let n_a = on_axis.iter().filter(|&&u| side[u] == Side::A).count();
    let n_b = on_axis.len() - n_a;
    let mut out = SideScores {
        side,
        side_a: Vec::with_capacity(m),
        side_b: Vec::with_capacity(m),
        score: Vec::with_capacity(m),
        gap: Vec::with_capacity(m),
    };
    for j in 0..m {
        let (mut sum_a, mut sum_b) = (0.0_f64, 0.0_f64);
        for &u in &on_axis {
            let pred = fit.mu + fit.b_u[u] + fit.b_j[j] + fit.f_u[u] * fit.f_j[j];
            match out.side[u] {
                Side::A => sum_a += pred,
                Side::B => sum_b += pred,
            }
        }
        let (a, b) = match (n_a, n_b) {
            (0, 0) => {
                let neutral = fit.mu + fit.b_j[j];
                (neutral, neutral)
            }
            (0, _) => {
                let all = sum_b / n_b as f64;
                (all, all)
            }
            (_, 0) => {
                let all = sum_a / n_a as f64;
                (all, all)
            }
            _ => (sum_a / n_a as f64, sum_b / n_b as f64),
        };
        out.side_a.push(a);
        out.side_b.push(b);
        out.score.push((a + b) / 2.0);
        out.gap.push((a - b).abs());
    }
    out
}

/// The gate's inputs for every item (`docs/02` §A.3–A.4, D32): `robust`, the bootstrap-min
/// of the side-balanced score — the pessimistic estimate the gate compares with `τ` — and
/// `full`, the side scores of the full fit, whose `gap` is the polarization the appeal
/// rule reads.
#[derive(Clone, Debug, PartialEq)]
pub struct BridgeScores {
    pub robust: Vec<f64>,
    pub full: SideScores,
}

/// Robust bridge score (`docs/02`, §A.4): the bootstrap-min of the side-balanced score
/// over `n_bootstrap` subsamples (each observation kept with probability `keep_frac`),
/// with the full fit's side scores (see [`BridgeScores`]). Malformed input is refused
/// with a [`RatingsError`] (T62).
pub fn bridge_scores(
    data: &Ratings,
    p: &BridgingParams,
    n_bootstrap: usize,
    keep_frac: f64,
) -> Result<BridgeScores, RatingsError> {
    data.validate()?;
    // Canonicalize: the bootstrap subsampling walks `obs` in order (INV-13, REPRO-002).
    let data = data.canonical();

    // Warm-start each subsample from the full fit: the bilinear term makes the
    // objective non-convex, so independent random inits would let some subsamples
    // land in a different minimum, polluting the min with optimizer noise.
    let full = fit_validated(&data, p);
    let anchor = pack(&full);
    let full_sides = side_balanced(&full);

    let mut robust = full_sides.score.clone();
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
            axis: data.axis.clone(),
        };
        let mut x0 = anchor.clone();
        for v in x0.iter_mut() {
            *v += normal(&mut rng) * 0.02;
        }
        let fit_s = fit_with_init(&sub, p, x0);
        for (b, &sj) in robust.iter_mut().zip(side_balanced(&fit_s).score.iter()) {
            if sj < *b {
                *b = sj;
            }
        }
    }
    Ok(BridgeScores {
        robust,
        full: full_sides,
    })
}

// Standard normal via Box–Muller, to control exact RNG consumption order.
fn normal(rng: &mut ChaCha8Rng) -> f64 {
    let u1: f64 = 1.0 - rng.gen::<f64>();
    let u2: f64 = rng.gen::<f64>();
    (-2.0 * ln(u1)).sqrt() * cos(2.0 * std::f64::consts::PI * u2)
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
