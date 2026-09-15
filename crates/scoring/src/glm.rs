//! Logistic regression by maximum likelihood, shared by IRT item fits and DIF.

use crate::optim::lbfgs;

#[inline]
pub fn sigmoid(z: f64) -> f64 {
    if z >= 0.0 {
        1.0 / (1.0 + (-z).exp())
    } else {
        let e = z.exp();
        e / (1.0 + e)
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

/// Fits weights `w` for `logit P(y=1) = x·w`. Each row of `x` includes its own
/// intercept column when needed. No regularization (matches the prototype).
pub fn fit_logistic(x: &[Vec<f64>], y: &[f64], max_iters: usize) -> Vec<f64> {
    let p = if x.is_empty() { 0 } else { x[0].len() };

    let cost = |w: &[f64]| -> f64 {
        let mut s = 0.0;
        for (row, &yi) in x.iter().zip(y.iter()) {
            let z = dot(row, w);
            s += softplus(z) - yi * z;
        }
        s
    };
    let grad = |w: &[f64]| -> Vec<f64> {
        let mut g = vec![0.0; p];
        for (row, &yi) in x.iter().zip(y.iter()) {
            let r = sigmoid(dot(row, w)) - yi;
            for k in 0..p {
                g[k] += r * row[k];
            }
        }
        g
    };

    lbfgs(vec![0.0; p], cost, grad, 10, max_iters, 1e-8)
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    let mut s = 0.0;
    for i in 0..a.len() {
        s += a[i] * b[i];
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sigmoid_is_stable_and_symmetric() {
        assert!((sigmoid(0.0) - 0.5).abs() < 1e-12);
        assert!(sigmoid(40.0) > 0.999 && sigmoid(-40.0) < 0.001);
        assert!((sigmoid(2.0) + sigmoid(-2.0) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn recovers_known_logistic_coefficients() {
        // Data generated from logit p = -0.5 + 1.5 x; fit should recover it closely.
        let (w0, w1) = (-0.5, 1.5);
        let mut x = Vec::new();
        let mut y = Vec::new();
        for i in 0..2000 {
            let xi = -3.0 + 6.0 * (i as f64) / 1999.0;
            let p = sigmoid(w0 + w1 * xi);
            // deterministic split around the probability
            let frac = (i as f64 * 0.61803398875).fract();
            x.push(vec![1.0, xi]);
            y.push(if frac < p { 1.0 } else { 0.0 });
        }
        let w = fit_logistic(&x, &y, 500);
        assert!((w[0] - w0).abs() < 0.2, "intercept = {:.3}", w[0]);
        assert!((w[1] - w1).abs() < 0.2, "slope = {:.3}", w[1]);
    }
}
