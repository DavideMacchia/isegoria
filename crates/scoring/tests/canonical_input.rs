//! INV-13 / REPRO-002 / AT-BR-03: the bridge score is invariant to the order of the
//! input observations, because the engine canonicalizes `obs` before using it.

use scoring::bridging::{bridge_scores, fit, BridgingParams, Ratings};

/// A small, deterministic, sparse ratings matrix with a real latent axis, so bridging
/// has a genuine signal to fit.
fn sample() -> Ratings {
    let (n, m) = (24usize, 9usize);
    let mut r = vec![vec![0.0f64; m]; n];
    let mut mask = vec![vec![false; m]; n];
    for u in 0..n {
        let f_u = (u as f64 / (n as f64 - 1.0)) * 2.0 - 1.0; // reviewer position in [-1, 1]
        for j in 0..m {
            // keep ~3/4 of the cells, deterministically
            if (u * 7 + j * 5) % 4 != 0 {
                let lean = (j as f64 / (m as f64 - 1.0)) * 2.0 - 1.0; // item lean
                mask[u][j] = true;
                r[u][j] = (0.5 + 0.4 * f_u * lean).clamp(0.0, 1.0);
            }
        }
    }
    Ratings::from_dense(&r, &mask)
}

/// The same observations in a thoroughly scrambled order (rotate, then reverse).
fn permuted(base: &Ratings, shift: usize) -> Ratings {
    let mut obs = base.obs.clone();
    let len = obs.len().max(1);
    obs.rotate_left(shift % len);
    obs.reverse();
    Ratings {
        n: base.n,
        m: base.m,
        obs,
        weights: base.weights.clone(),
    }
}

#[test]
fn fit_is_bit_identical_under_input_permutation() {
    let p = BridgingParams::default();
    let base = sample();
    let a = fit(&base, &p).unwrap();
    let b = fit(&permuted(&base, 7), &p).unwrap();
    assert_eq!(a.b_j, b.b_j, "b_j must be permutation-invariant");
    assert_eq!(a.f_j, b.f_j, "f_j must be permutation-invariant");
    assert_eq!(a.mu, b.mu, "mu must be permutation-invariant");
}

#[test]
fn bridge_scores_are_bit_identical_under_input_permutation() {
    let p = BridgingParams::default();
    let base = sample();
    let s0 = bridge_scores(&base, &p, 10, 0.85).unwrap();
    let s1 = bridge_scores(&permuted(&base, 13), &p, 10, 0.85).unwrap();
    assert_eq!(
        s0, s1,
        "the robust and full side-balanced scores must be permutation-invariant"
    );
}
