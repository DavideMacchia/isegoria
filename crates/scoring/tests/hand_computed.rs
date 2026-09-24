//! Hand-computed values for the Level C and anti-collusion arithmetic (T41). The other
//! suites check ranges and orderings (a cartel is discounted, reputation falls fast);
//! cargo-mutants showed those pass with a wrong decay, an unweighted baseline, a wrong
//! even-length median or a wrong mean in Pearson. These pin the formulas themselves.

use scoring::collusion::{correlation_matrix, discount_weights};
use scoring::reputation::{author_score, crowd_baseline, weight_cap, AuthorPrior};

const EPS: f64 = 1e-12;

// ------------------------------ Level C ------------------------------

/// `C_a = (α₀ + Σ wq) / (α₀ + β₀ + Σ w)`, `w = exp(−age / T)`: a fresh good item and an
/// old bad one, with the default prior (α₀ = 2, β₀ = 3, T = 18 months).
#[test]
fn author_score_decays_old_items_by_exp_minus_age_over_t() {
    let p = AuthorPrior::default();
    let w_old = (-1.0_f64).exp(); // age = T
    let want = (2.0 + 1.0 * 1.0 + 0.0 * w_old) / (2.0 + 3.0 + 1.0 + w_old);
    let got = author_score(&[1.0, 0.0], &[0.0, 18.0], &p);
    assert!((got - want).abs() < EPS, "{got} vs {want}");
    // With no history the score is the prior mean.
    assert!((author_score(&[], &[], &p) - 2.0 / 5.0).abs() < EPS);
}

/// `p̄_j = Σ w_u p_uj / Σ w_u`: a weight-2 reviewer counts twice.
#[test]
fn crowd_baseline_is_the_weighted_mean() {
    let preds = vec![vec![0.9, 0.2], vec![0.3, 0.8]];
    let got = crowd_baseline(&preds, &[2.0, 1.0]);
    assert!((got[0] - (2.0 * 0.9 + 0.3) / 3.0).abs() < EPS);
    assert!((got[1] - (2.0 * 0.2 + 0.8) / 3.0).abs() < EPS);
}

/// No weight at all (everyone on probation): the baseline is 0, never 0/0.
#[test]
fn crowd_baseline_without_weight_is_zero() {
    let got = crowd_baseline(&[vec![0.9, 0.2], vec![0.3, 0.8]], &[0.0, 0.0]);
    assert_eq!(got, vec![0.0, 0.0]);
}

/// `w_max = 3 × median`: the median of an even count is the mean of the middle two.
#[test]
fn weight_cap_uses_the_true_median() {
    assert!((weight_cap(&[4.0, 1.0, 3.0, 2.0]) - 3.0 * 2.5).abs() < EPS);
    assert!((weight_cap(&[5.0, 1.0, 3.0]) - 3.0 * 3.0).abs() < EPS);
    assert!((weight_cap(&[10.0, 1.0]) - 3.0 * 5.5).abs() < EPS);
    assert_eq!(weight_cap(&[]), 0.0);
}

// --------------------------- anti-collusion ---------------------------

/// Pearson on a small pair: [1,2,3,4] vs [1,3,2,5] → cov 5.5, variances 5 and 8.75.
#[test]
fn correlation_matches_a_hand_computed_pearson() {
    let c = correlation_matrix(&[vec![1.0, 2.0, 3.0, 4.0], vec![1.0, 3.0, 2.0, 5.0]]);
    let want = 5.5 / (5.0_f64 * 8.75).sqrt();
    assert!((c[0][1] - want).abs() < EPS, "{} vs {want}", c[0][1]);
    assert_eq!(c[0][1].to_bits(), c[1][0].to_bits(), "symmetric");
}

/// A constant row has undefined correlation with anything: 0, not NaN — whether the
/// other row varies or not. The diagonal stays 1 regardless.
#[test]
fn a_constant_row_correlates_zero_and_keeps_a_unit_diagonal() {
    let c = correlation_matrix(&[vec![0.5; 4], vec![1.0, 2.0, 3.0, 4.0], vec![0.5; 4]]);
    for (i, j) in [(0, 1), (1, 0), (0, 2), (1, 2)] {
        assert_eq!(c[i][j], 0.0, "c[{i}][{j}]");
    }
    for (i, row) in c.iter().enumerate() {
        assert_eq!(row[i], 1.0, "diagonal {i}");
    }
}

/// A cluster whose members all weigh 0 (probation) stays at 0 instead of 0·(0^α/0).
#[test]
fn a_zero_weight_cluster_stays_zero() {
    let got = discount_weights(&[0.0, 0.0, 4.0], &[0, 0, 1], 0.5);
    assert_eq!(&got[..2], &[0.0, 0.0]);
    // The other cluster: a single node of weight 4 keeps 4 · 4^{-1/2} = 2.
    assert!((got[2] - 2.0).abs() < EPS);
}
