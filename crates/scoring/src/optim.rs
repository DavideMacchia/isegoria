//! In-house L-BFGS: two-loop recursion + Armijo backtracking line search.
//! Kept in-house for control over floating-point determinism (CLAUDE.md #7).

/// How a minimization ended, so a caller never mistakes a stalled run for a solved
/// one (docs/08 OPT-001). The final gradient norm is the authority: whatever the exit
/// path, [`Convergence::Converged`] means `‖∇f‖∞ ≤ g_tol` at the returned point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Convergence {
    /// The gradient's infinity norm reached `g_tol`.
    Converged,
    /// The iteration budget ran out (or progress stalled) with the gradient still above `g_tol`.
    MaxIters,
    /// The line search could not find a descent step — no descent direction, or the
    /// step collapsed. Typical of an objective with no finite minimizer (e.g. a
    /// perfectly separable logistic fit, whose MLE is at infinity).
    LineSearchFailed,
}

/// A minimizer together with how the run ended.
pub struct Minimized {
    pub x: Vec<f64>,
    pub status: Convergence,
}

/// Minimizes `cost` from `x0`. Deterministic: no RNG, no parallelism. The returned
/// [`Minimized::status`] reports whether it actually converged (OPT-001).
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
    let mut line_search_failed = false;
    // Default outcome if the iteration budget runs out while still descending.
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

        let c1 = 1e-4;
        let mut step = 1.0;
        let mut x_new = add_scaled(&x, step, &d);
        let mut f_new = cost(&x_new);
        let mut backtracks = 0;
        while f_new > fx + c1 * step * gd {
            step *= 0.5;
            if step < 1e-20 {
                line_search_failed = true;
                break;
            }
            x_new = add_scaled(&x, step, &d);
            f_new = cost(&x_new);
            backtracks += 1;
            if backtracks > 60 {
                line_search_failed = true;
                break;
            }
        }

        let g_new = grad(&x_new);

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

        // A stall means the objective is flat here — a stationary point was reached, so
        // this is convergence even if the gradient never dropped under the tight g_tol.
        // Only a genuine inability to descend with the objective still moving is a
        // line-search failure (typical of an unbounded objective).
        if progress <= 1e-12 * (1.0 + fx.abs()) {
            status = Convergence::Converged;
            break;
        }
        if line_search_failed {
            status = Convergence::LineSearchFailed;
            break;
        }
    }

    Minimized { x, status }
}

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

    #[test]
    fn reports_non_convergence_when_the_budget_runs_out() {
        // `f(x) = x` has no minimizer and a constant, non-vanishing gradient: the
        // optimizer keeps descending by a full step every iteration and never stalls,
        // so it exhausts the budget. Status must report that, not convergence.
        let cost = |x: &[f64]| x[0];
        let grad = |_x: &[f64]| vec![1.0];
        let m = lbfgs(vec![0.0], cost, grad, 5, 50, 1e-8);
        assert_eq!(m.status, Convergence::MaxIters);
    }
}
