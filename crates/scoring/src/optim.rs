//! In-house L-BFGS: two-loop recursion + Armijo backtracking line search.
//! Kept in-house for control over floating-point determinism (CLAUDE.md #7).

/// How a minimization ended (docs/08 OPT-001).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Convergence {
    /// Gradient norm reached `g_tol`, or progress stalled at a stationary point.
    Converged,
    /// Iteration budget ran out while still descending.
    MaxIters,
    /// No descent step found (e.g. an objective with no finite minimizer).
    LineSearchFailed,
}

pub struct Minimized {
    pub x: Vec<f64>,
    pub status: Convergence,
}

/// Minimizes `cost` from `x0`. Deterministic: no RNG, no parallelism.
pub fn lbfgs<C, G>(
    x0: Vec<f64>,
    cost: C,
    grad: G,
    m_hist: usize,
    max_iters: usize,
    g_tol: f64,
) -> Minimized
where
    C: Fn(&[f64]) -> f64,
    G: Fn(&[f64]) -> Vec<f64>,
{
    let n = x0.len();
    let mut x = x0;
    let mut g = grad(&x);
    let mut fx = cost(&x);
    let mut status = Convergence::MaxIters;

    let mut s_hist: Vec<Vec<f64>> = Vec::with_capacity(m_hist);
    let mut y_hist: Vec<Vec<f64>> = Vec::with_capacity(m_hist);
    let mut rho_hist: Vec<f64> = Vec::with_capacity(m_hist);

    for _ in 0..max_iters {
        if inf_norm(&g) <= g_tol {
            status = Convergence::Converged;
            break;
        }

        let mut q = g.clone();
        let k = s_hist.len();
        let mut alpha = vec![0.0; k];
        for i in (0..k).rev() {
            let a = rho_hist[i] * dot(&s_hist[i], &q);
            alpha[i] = a;
            axpy(&mut q, -a, &y_hist[i]);
        }
        let gamma = if k > 0 {
            let sy = dot(&s_hist[k - 1], &y_hist[k - 1]);
            let yy = dot(&y_hist[k - 1], &y_hist[k - 1]);
            if yy > 0.0 {
                sy / yy
            } else {
                1.0
            }
        } else {
            1.0
        };
        for qi in q.iter_mut() {
            *qi *= gamma;
        }
        for i in 0..k {
            let beta = rho_hist[i] * dot(&y_hist[i], &q);
            axpy(&mut q, alpha[i] - beta, &s_hist[i]);
        }
        let mut d = q;
        for di in d.iter_mut() {
            *di = -*di;
        }

        // Fall back to steepest descent if the direction is not a descent one.
        let mut gd = dot(&g, &d);
        if gd >= 0.0 {
            d = g.iter().map(|v| -v).collect();
            gd = dot(&g, &d);
            if gd >= 0.0 {
                status = Convergence::LineSearchFailed;
                break;
            }
        }

        // Strong-Wolfe line search (T48). Armijo-only backtracking accepted tiny steps in
        // narrow valleys, where the stall test below then declared a premature
        // "convergence" far from any minimum.
        let accepted = wolfe_search(&x, fx, gd, &d, &cost, &grad);
        let Some(Trial {
            x: x_new,
            f: f_new,
            g: g_new,
        }) = accepted.filter(|t| t.x != x)
        else {
            // No point along `d` lowers the cost (or the step rounds back to `x`). The
            // start point is kept, and this is not reported as convergence (T41).
            status = Convergence::LineSearchFailed;
            break;
        };

        let s: Vec<f64> = (0..n).map(|i| x_new[i] - x[i]).collect();
        let y: Vec<f64> = (0..n).map(|i| g_new[i] - g[i]).collect();
        let sy = dot(&s, &y);
        if sy > 1e-12 {
            // Curvature condition: keep the Hessian approximation positive definite.
            if s_hist.len() == m_hist {
                s_hist.remove(0);
                y_hist.remove(0);
                rho_hist.remove(0);
            }
            rho_hist.push(1.0 / sy);
            s_hist.push(s);
            y_hist.push(y);
        }

        let progress = (fx - f_new).abs();
        x = x_new;
        g = g_new;
        fx = f_new;

        // A stall is a stationary point: converged even if ‖g‖ never reached g_tol.
        if progress <= 1e-12 * (1.0 + fx.abs()) {
            status = Convergence::Converged;
            break;
        }
    }

    Minimized { x, status }
}

/// A point evaluated during the line search: position, cost and gradient.
struct Trial {
    x: Vec<f64>,
    f: f64,
    g: Vec<f64>,
}

/// One end of a line-search bracket: step length, `φ(a)`, `φ'(a)`, and the evaluated
/// point (absent for `a = 0`, the start).
struct End {
    a: f64,
    f: f64,
    dg: f64,
    trial: Option<Trial>,
}

// Sufficient decrease and curvature constants (Nocedal & Wright, quasi-Newton values).
const WOLFE_C1: f64 = 1e-4;
const WOLFE_C2: f64 = 0.9;
const MAX_EXPANSIONS: usize = 40;
const MAX_ZOOM: usize = 60;

/// Line search satisfying the strong Wolfe conditions along a descent direction `d`
/// (`gd = ∇f(x)·d < 0`), after Nocedal & Wright, Algorithms 3.5 and 3.6, with a
/// safeguarded cubic interpolation. Returns a point with sufficient decrease — one that
/// also satisfies the curvature condition whenever the bracket allows it — or `None`
/// when no step lowers the cost. Deterministic.
fn wolfe_search<C, G>(x: &[f64], fx: f64, gd: f64, d: &[f64], cost: &C, grad: &G) -> Option<Trial>
where
    C: Fn(&[f64]) -> f64,
    G: Fn(&[f64]) -> Vec<f64>,
{
    let eval = |a: f64| -> End {
        let xa = add_scaled(x, a, d);
        let f = cost(&xa);
        let g = grad(&xa);
        let dg = dot(&g, d);
        End {
            a,
            f,
            dg,
            trial: Some(Trial { x: xa, f, g }),
        }
    };
    let sufficient = |e: &End| e.f.is_finite() && e.f <= fx + WOLFE_C1 * e.a * gd;
    let curvature = |e: &End| e.dg.abs() <= -WOLFE_C2 * gd;

    let mut prev = End {
        a: 0.0,
        f: fx,
        dg: gd,
        trial: None,
    };
    let mut a = 1.0;
    for i in 0..MAX_EXPANSIONS {
        let cur = eval(a);
        if !sufficient(&cur) || (i > 0 && cur.f >= prev.f) {
            return zoom(prev, cur, &eval, &sufficient, &curvature);
        }
        if curvature(&cur) {
            return cur.trial;
        }
        if cur.dg >= 0.0 {
            return zoom(cur, prev, &eval, &sufficient, &curvature);
        }
        prev = cur;
        a *= 2.0;
    }
    prev.trial
}

/// The bracketing phase: `lo` always satisfies sufficient decrease and has the lower
/// cost; the bracket shrinks until a strong-Wolfe point is found or it collapses, in
/// which case the best sufficient-decrease point seen (`lo`) is returned.
fn zoom<E, S, K>(mut lo: End, mut hi: End, eval: &E, sufficient: &S, curvature: &K) -> Option<Trial>
where
    E: Fn(f64) -> End,
    S: Fn(&End) -> bool,
    K: Fn(&End) -> bool,
{
    for _ in 0..MAX_ZOOM {
        let width = (hi.a - lo.a).abs();
        if width <= 1e-16 * lo.a.abs().max(1.0) {
            break;
        }
        let a = interpolate(&lo, &hi);
        let cur = eval(a);
        if !sufficient(&cur) || cur.f >= lo.f {
            hi = cur;
        } else {
            if curvature(&cur) {
                return cur.trial;
            }
            if cur.dg * (hi.a - lo.a) >= 0.0 {
                hi = lo;
            }
            lo = cur;
        }
    }
    // `lo.trial` is `None` only while `lo` is still the start point: no decrease found.
    lo.trial
}

/// Minimizer of the cubic through both bracket ends (values and slopes), kept at least
/// 0.1% of the bracket away from either end so the bracket keeps shrinking; bisection
/// when the cubic is unusable. (A 10% margin rejects the exact minimizer of a quadratic
/// whenever it lies near an end — the usual case on a first, overshooting step.)
fn interpolate(lo: &End, hi: &End) -> f64 {
    let (a0, a1) = (lo.a, hi.a);
    let mid = 0.5 * (a0 + a1);
    if !hi.f.is_finite() || !hi.dg.is_finite() {
        return mid;
    }
    let d1 = lo.dg + hi.dg - 3.0 * (lo.f - hi.f) / (a0 - a1);
    let disc = d1 * d1 - lo.dg * hi.dg;
    if disc < 0.0 {
        return mid;
    }
    let d2 = (a1 - a0).signum() * disc.sqrt();
    let a = a1 - (a1 - a0) * (hi.dg + d2 - d1) / (hi.dg - lo.dg + 2.0 * d2);
    let (lo_b, hi_b) = (a0.min(a1), a0.max(a1));
    let margin = 1e-3 * (hi_b - lo_b);
    if a.is_finite() && a >= lo_b + margin && a <= hi_b - margin {
        a
    } else {
        mid
    }
}

#[cfg(test)]
/// Central-difference gradient, for objectives whose analytic gradient is not
/// worth deriving (matches SciPy's numerical gradient in `sim/latent_dif_and_capacity.py`).
pub fn numerical_gradient<C>(cost: &C, x: &[f64], h: f64) -> Vec<f64>
where
    C: Fn(&[f64]) -> f64,
{
    let mut g = vec![0.0; x.len()];
    let mut xp = x.to_vec();
    for i in 0..x.len() {
        let orig = xp[i];
        xp[i] = orig + h;
        let fp = cost(&xp);
        xp[i] = orig - h;
        let fm = cost(&xp);
        xp[i] = orig;
        g[i] = (fp - fm) / (2.0 * h);
    }
    g
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    let mut s = 0.0;
    for i in 0..a.len() {
        s += a[i] * b[i];
    }
    s
}

fn axpy(y: &mut [f64], a: f64, x: &[f64]) {
    for i in 0..y.len() {
        y[i] += a * x[i];
    }
}

fn add_scaled(x: &[f64], step: f64, d: &[f64]) -> Vec<f64> {
    (0..x.len()).map(|i| x[i] + step * d[i]).collect()
}

fn inf_norm(v: &[f64]) -> f64 {
    let mut m = 0.0_f64;
    for &x in v {
        m = m.max(x.abs());
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimizes_a_simple_quadratic() {
        let cost = |x: &[f64]| (x[0] - 3.0).powi(2) + 2.0 * (x[1] + 1.0).powi(2);
        let grad = |x: &[f64]| vec![2.0 * (x[0] - 3.0), 4.0 * (x[1] + 1.0)];
        let m = lbfgs(vec![0.0, 0.0], cost, grad, 5, 200, 1e-10);
        assert_eq!(m.status, Convergence::Converged);
        let x = m.x;
        assert!((x[0] - 3.0).abs() < 1e-5, "x0 = {}", x[0]);
        assert!((x[1] + 1.0).abs() < 1e-5, "x1 = {}", x[1]);
    }

    #[test]
    fn numerical_gradient_matches_analytic() {
        let cost = |x: &[f64]| (x[0] - 3.0).powi(2) + 2.0 * (x[1] + 1.0).powi(2);
        let x = [1.0, 4.0];
        let ng = numerical_gradient(&cost, &x, 1e-6);
        let analytic = [2.0 * (x[0] - 3.0), 4.0 * (x[1] + 1.0)];
        assert!((ng[0] - analytic[0]).abs() < 1e-4);
        assert!((ng[1] - analytic[1]).abs() < 1e-4);
    }

    #[test]
    fn minimizes_rosenbrock() {
        let cost = |x: &[f64]| (1.0 - x[0]).powi(2) + 100.0 * (x[1] - x[0] * x[0]).powi(2);
        let grad = |x: &[f64]| {
            vec![
                -2.0 * (1.0 - x[0]) - 400.0 * x[0] * (x[1] - x[0] * x[0]),
                200.0 * (x[1] - x[0] * x[0]),
            ]
        };
        let x = lbfgs(vec![-1.2, 1.0], cost, grad, 10, 2000, 1e-8).x;
        assert!((x[0] - 1.0).abs() < 1e-3, "x0 = {}", x[0]);
        assert!((x[1] - 1.0).abs() < 1e-3, "x1 = {}", x[1]);
    }

    use std::cell::Cell;

    // Pinned trajectories: (cost evaluations, gradient evaluations, final x bits).
    const PINNED_ROSEN: (usize, usize, &[u64]) =
        (51, 51, &[4607182418800301113, 4607182418800609318]);
    const PINNED_QUAD: (usize, usize, &[u64]) = (
        83,
        83,
        &[
            13736512305672097628,
            13731834493049554682,
            13728298090719871024,
            4493789166252270074,
            13717216977604639406,
            13711635847696458132,
            4476207571572229212,
            13695079438433771476,
        ],
    );
    const PINNED_WALL: (usize, usize, &[u64]) = (8, 8, &[4607182418800017408]);

    /// Eigenvalues of `½ Σ λ_i x_i²`, log-spaced over [1, 10⁴]: condition number 10⁴.
    fn ill_conditioned_lambdas(dim: usize) -> Vec<f64> {
        (0..dim)
            .map(|i| 10f64.powf(4.0 * i as f64 / (dim - 1) as f64))
            .collect()
    }

    /// Counts calls to `f`, so a test can bound the work an optimization took.
    fn counted<'a, T>(
        calls: &'a Cell<usize>,
        f: impl Fn(&[f64]) -> T + 'a,
    ) -> impl Fn(&[f64]) -> T + 'a {
        move |x| {
            calls.set(calls.get() + 1);
            f(x)
        }
    }

    /// L-BFGS, not steepest descent: on a condition-10⁴ quadratic the quasi-Newton
    /// updates reach the minimum within a SciPy-like budget (SciPy L-BFGS-B, m = 10: ~790
    /// gradients). Disabling the two-loop recursion, corrupting the curvature pairs or
    /// the scaling leaves the old tests green but breaks this one (T41).
    #[test]
    fn solves_an_ill_conditioned_quadratic_within_a_quasi_newton_budget() {
        let lam = ill_conditioned_lambdas(20);
        let cost = |x: &[f64]| 0.5 * x.iter().zip(&lam).map(|(v, l)| l * v * v).sum::<f64>();
        let grad = |x: &[f64]| x.iter().zip(&lam).map(|(v, l)| l * v).collect::<Vec<f64>>();
        let x0 = vec![1.0; lam.len()];
        let f0 = cost(&x0);
        let grads = Cell::new(0);
        let m = lbfgs(x0, cost, counted(&grads, grad), 10, 5000, 1e-9);
        assert_eq!(m.status, Convergence::Converged);
        assert!(cost(&m.x) <= 1e-8 * f0, "f = {:e}", cost(&m.x));
        assert!(grads.get() <= 800, "{} gradient evaluations", grads.get());
    }

    /// Rosenbrock to 1e-6 within a SciPy-like budget: ~51 gradients with the strong-Wolfe
    /// search (SciPy L-BFGS-B ~46). The former Armijo-only search needed ~670 (T48).
    #[test]
    fn solves_rosenbrock_within_a_budget() {
        let cost = |x: &[f64]| (1.0 - x[0]).powi(2) + 100.0 * (x[1] - x[0] * x[0]).powi(2);
        let grad = |x: &[f64]| {
            vec![
                -2.0 * (1.0 - x[0]) - 400.0 * x[0] * (x[1] - x[0] * x[0]),
                200.0 * (x[1] - x[0] * x[0]),
            ]
        };
        let grads = Cell::new(0);
        let m = lbfgs(vec![-1.2, 1.0], cost, counted(&grads, grad), 10, 2000, 1e-8);
        assert_eq!(m.status, Convergence::Converged);
        assert!(
            (m.x[0] - 1.0).abs() < 1e-6 && (m.x[1] - 1.0).abs() < 1e-6,
            "{:?}",
            m.x
        );
        assert!(grads.get() <= 80, "{} gradient evaluations", grads.get());
    }

    /// What a stall-reported `Converged` guarantees (T45; `docs/08` OPT-001). The run
    /// stops on a relative progress below `1e-12 · (1 + |f|)` even while `‖g‖_∞ ≫ g_tol`;
    /// for a quadratic model the last step's decrease is at least `‖g‖² / (2 λ_max)`, so
    /// at a stall `‖g‖_∞ ≤ √(2 λ_max · 1e-12 · (1 + |f|))` — about `1.4e-4 · √λ_max` near
    /// `f = 0`, not `g_tol`. Pinned on the condition-10² to 10⁶ quadratics and on
    /// Rosenbrock (λ_max ≈ 1.0e3 at the minimum) with an unattainable `g_tol`: the
    /// gradient at the reported convergence stays within that bound, and the minimizer is
    /// reached to the precision the bound allows.
    #[test]
    fn a_stall_convergence_leaves_the_gradient_within_its_bound() {
        let bound = |lam_max: f64, f: f64| (2.0 * lam_max * 1e-12 * (1.0 + f.abs())).sqrt();
        for cond in [2.0, 4.0, 6.0] {
            let dim = 20;
            let lam: Vec<f64> = (0..dim)
                .map(|i| 10f64.powf(cond * i as f64 / (dim - 1) as f64))
                .collect();
            let cost = |x: &[f64]| 0.5 * x.iter().zip(&lam).map(|(v, l)| l * v * v).sum::<f64>();
            let grad = |x: &[f64]| x.iter().zip(&lam).map(|(v, l)| l * v).collect::<Vec<f64>>();
            let m = lbfgs(vec![1.0; dim], cost, grad, 10, 20_000, 1e-300);
            assert_eq!(m.status, Convergence::Converged, "condition 1e{cond}");
            let g = inf_norm(&grad(&m.x));
            let lam_max = 10f64.powf(cond);
            assert!(
                g <= bound(lam_max, cost(&m.x)),
                "condition 1e{cond}: ‖g‖ {g:e} beyond the stall bound {:e}",
                bound(lam_max, cost(&m.x))
            );
            assert!(
                inf_norm(&m.x) <= 10.0 * bound(lam_max, 0.0),
                "condition 1e{cond}: x {:e}",
                inf_norm(&m.x)
            );
        }
        let cost = |x: &[f64]| (1.0 - x[0]).powi(2) + 100.0 * (x[1] - x[0] * x[0]).powi(2);
        let grad = |x: &[f64]| {
            vec![
                -2.0 * (1.0 - x[0]) - 400.0 * x[0] * (x[1] - x[0] * x[0]),
                200.0 * (x[1] - x[0] * x[0]),
            ]
        };
        for start in [vec![-1.2, 1.0], vec![2.0, 2.0], vec![0.0, 0.0]] {
            let m = lbfgs(start.clone(), cost, grad, 10, 20_000, 1e-300);
            assert_eq!(m.status, Convergence::Converged, "from {start:?}");
            let g = inf_norm(&grad(&m.x));
            assert!(
                g <= bound(1.0e3, cost(&m.x)),
                "from {start:?}: ‖g‖ {g:e} beyond the stall bound {:e}",
                bound(1.0e3, cost(&m.x))
            );
            assert!(
                (m.x[0] - 1.0).abs() < 1e-5 && (m.x[1] - 1.0).abs() < 1e-5,
                "from {start:?}: {:?}",
                m.x
            );
        }
    }

    /// Started at the minimizer, the gradient test stops the run before any step: no
    /// cost is evaluated beyond the initial one and `x` is returned unchanged.
    #[test]
    fn stops_immediately_at_a_stationary_point() {
        let (costs, grads) = (Cell::new(0), Cell::new(0));
        let m = lbfgs(
            vec![3.0, -1.0],
            counted(&costs, |x: &[f64]| {
                (x[0] - 3.0).powi(2) + 2.0 * (x[1] + 1.0).powi(2)
            }),
            counted(&grads, |x: &[f64]| {
                vec![2.0 * (x[0] - 3.0), 4.0 * (x[1] + 1.0)]
            }),
            5,
            100,
            1e-10,
        );
        assert_eq!(m.status, Convergence::Converged);
        assert_eq!(m.x, vec![3.0, -1.0]);
        assert_eq!((costs.get(), grads.get()), (1, 1));
    }

    /// In one dimension a single curvature pair gives the exact Hessian, so the second
    /// direction lands on the minimum: one gradient to start, one after the backtracked
    /// first step, one at the minimum. (The `γ` scaling cancels in 1-D; `golden.rs` pins
    /// it through the full fits.)
    #[test]
    fn one_curvature_pair_solves_a_one_dimensional_quadratic() {
        let grads = Cell::new(0);
        let m = lbfgs(
            vec![1.0],
            |x: &[f64]| 50.0 * x[0] * x[0],
            counted(&grads, |x: &[f64]| vec![100.0 * x[0]]),
            5,
            100,
            1e-12,
        );
        assert_eq!(m.status, Convergence::Converged);
        assert!(m.x[0].abs() < 1e-12, "x = {}", m.x[0]);
        assert_eq!(grads.get(), 3);
    }

    /// Armijo sufficient decrease, not mere non-increase: from x = 1 on x², the unit step
    /// lands on x = −1 with the *same* cost. It must be rejected and halved to the
    /// minimum; a test that accepted it would stall at −1 and call that convergence.
    #[test]
    fn armijo_rejects_a_step_that_does_not_decrease_the_cost() {
        let m = lbfgs(
            vec![1.0],
            |x: &[f64]| x[0] * x[0],
            |x: &[f64]| vec![2.0 * x[0]],
            5,
            100,
            1e-12,
        );
        assert_eq!(m.status, Convergence::Converged);
        assert!(m.x[0].abs() < 1e-12, "x = {}", m.x[0]);
    }

    /// When every trial point raises the cost, the failed search returns the start point:
    /// on `f(x) = x` with an uphill gradient, the last trial `x = 2⁻⁶¹` is worse than 0.
    #[test]
    fn a_failed_line_search_keeps_the_better_point() {
        let m = lbfgs(
            vec![0.0],
            |x: &[f64]| x[0],
            |_x: &[f64]| vec![-1.0],
            5,
            50,
            1e-8,
        );
        assert_eq!(m.status, Convergence::LineSearchFailed);
        assert_eq!(m.x, vec![0.0]);
    }

    /// Sufficient decrease is required even where the slope is flat. On
    /// `f(a) = −a + (2 + 3ε)a² − (1 + 2ε)a³` (ε = 5·10⁻⁵) the first unit step lands on a
    /// point with `f'(1) = 0` but `f(1) = ε > f(0)`: it satisfies the curvature condition
    /// and must still be rejected, or the run "converges" at a point worse than its start.
    #[test]
    fn a_flat_point_that_raises_the_cost_is_not_accepted() {
        let e = 5e-5;
        let (b, c) = (2.0 + 3.0 * e, -(1.0 + 2.0 * e));
        let cost = |x: &[f64]| -x[0] + b * x[0] * x[0] + c * x[0].powi(3);
        let grad = |x: &[f64]| vec![-1.0 + 2.0 * b * x[0] + 3.0 * c * x[0] * x[0]];
        let m = lbfgs(vec![0.0], cost, grad, 5, 100, 1e-10);
        assert_eq!(m.status, Convergence::Converged);
        assert!(
            cost(&m.x) < 0.0,
            "stopped at x = {} with f = {}",
            m.x[0],
            cost(&m.x)
        );
    }

    /// A cost that is infinite past a wall (a barrier) makes the cubic unusable, and the
    /// search bisects the bracket. `(x − 1)²` for `x < 1.5`, `∞` beyond: the first step
    /// from 0 lands at 2, the midpoint of `[0, 1]` lands exactly on the minimum.
    #[test]
    fn an_infinite_cost_past_a_wall_is_bisected_back_into_range() {
        let cost = |x: &[f64]| {
            if x[0] < 1.5 {
                (x[0] - 1.0).powi(2)
            } else {
                f64::INFINITY
            }
        };
        let grad = |x: &[f64]| vec![2.0 * (x[0] - 1.0)];
        let m = lbfgs(vec![0.0], cost, grad, 5, 100, 1e-12);
        assert_eq!(m.status, Convergence::Converged);
        assert_eq!(m.x, vec![1.0]);
    }

    /// The optimizer's trajectory is part of the reproducibility contract (invariant #7):
    /// on reference problems the exact number of evaluations and the final point, bit for
    /// bit, are pinned. A change to the line search that still converges — a different
    /// bracket orientation, interpolation margin or stopping width — moves these. After an
    /// intended change, re-derive the constants from the failure message.
    #[test]
    fn trajectories_on_reference_problems_are_pinned() {
        fn run(
            x0: Vec<f64>,
            cost: impl Fn(&[f64]) -> f64,
            grad: impl Fn(&[f64]) -> Vec<f64>,
        ) -> (usize, usize, Vec<u64>) {
            let (costs, grads) = (Cell::new(0), Cell::new(0));
            let m = lbfgs(
                x0,
                counted(&costs, cost),
                counted(&grads, grad),
                10,
                5000,
                1e-9,
            );
            (
                costs.get(),
                grads.get(),
                m.x.iter().map(|v| v.to_bits()).collect(),
            )
        }
        let rosen = run(
            vec![-1.2, 1.0],
            |x: &[f64]| (1.0 - x[0]).powi(2) + 100.0 * (x[1] - x[0] * x[0]).powi(2),
            |x: &[f64]| {
                vec![
                    -2.0 * (1.0 - x[0]) - 400.0 * x[0] * (x[1] - x[0] * x[0]),
                    200.0 * (x[1] - x[0] * x[0]),
                ]
            },
        );
        let lam = ill_conditioned_lambdas(8);
        let quad = run(
            vec![1.0; 8],
            |x: &[f64]| 0.5 * x.iter().zip(&lam).map(|(v, l)| l * v * v).sum::<f64>(),
            |x: &[f64]| x.iter().zip(&lam).map(|(v, l)| l * v).collect(),
        );
        let wall = run(
            vec![-3.0],
            |x: &[f64]| {
                if x[0] < 1.5 {
                    (x[0] - 1.0).powi(4)
                } else {
                    f64::INFINITY
                }
            },
            |x: &[f64]| vec![4.0 * (x[0] - 1.0).powi(3)],
        );
        // Exercise the bracketing branches: a slope that stays at −1 until a cliff past
        // the minimum (expansion overshoots with a rising value), and a steep wall where
        // an interpolated point is lower than the bracket end but far too steep (the
        // bracket must be re-oriented).
        let cliff = run(
            vec![0.0],
            |x: &[f64]| -x[0] + (x[0] - 6.0).exp(),
            |x: &[f64]| vec![-1.0 + (x[0] - 6.0).exp()],
        );
        let steep = run(
            vec![0.0],
            |x: &[f64]| -x[0] + 0.1 * (10.0 * (x[0] - 6.0)).exp(),
            |x: &[f64]| vec![-1.0 + (10.0 * (x[0] - 6.0)).exp()],
        );
        // A failing search: the bracket must collapse and stop, within its budget.
        let uphill = run(
            vec![1.0],
            |x: &[f64]| x[0] * x[0],
            |x: &[f64]| vec![-2.0 * x[0]],
        );
        assert_eq!(
            (cliff.0, cliff.1, &cliff.2[..]),
            (14, 14, &[4618441417868437217][..])
        );
        assert_eq!(
            (steep.0, steep.1, &steep.2[..]),
            (18, 18, &[4618441417868443096][..])
        );
        assert_eq!(
            (uphill.0, uphill.1, &uphill.2[..]),
            (18, 18, &[4607182418800017408][..])
        );
        assert_eq!((rosen.0, rosen.1, &rosen.2[..]), PINNED_ROSEN);
        assert_eq!((quad.0, quad.1, &quad.2[..]), PINNED_QUAD);
        assert_eq!((wall.0, wall.1, &wall.2[..]), PINNED_WALL);
    }

    /// A gradient that points uphill (here the sign is wrong): no step lowers the cost,
    /// so the line search fails. It used to report `Converged` — the shrinking step
    /// barely moved `f` and read as a stall, or `x + step·d` rounded back to `x` and
    /// passed Armijo with no progress (T41). The start point is not made worse.
    #[test]
    fn an_uphill_gradient_is_a_failed_line_search_not_convergence() {
        let costs = Cell::new(0);
        let m = lbfgs(
            vec![1.0],
            counted(&costs, |x: &[f64]| x[0] * x[0]),
            |x: &[f64]| vec![-2.0 * x[0]],
            5,
            50,
            1e-8,
        );
        assert_eq!(m.status, Convergence::LineSearchFailed);
        assert!(m.x[0] * m.x[0] <= 1.0, "cost rose to {}", m.x[0] * m.x[0]);
        assert!(costs.get() <= 64, "{} cost evaluations", costs.get());
    }

    #[test]
    fn reports_non_convergence_when_the_budget_runs_out() {
        // `f(x) = x`: no minimizer, constant gradient — the budget is exhausted.
        let cost = |x: &[f64]| x[0];
        let grad = |_x: &[f64]| vec![1.0];
        let m = lbfgs(vec![0.0], cost, grad, 5, 50, 1e-8);
        assert_eq!(m.status, Convergence::MaxIters);
    }
}
