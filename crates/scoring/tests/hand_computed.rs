//! Hand-computed values for the Level C and anti-collusion arithmetic (T41). The other
//! suites check ranges and orderings (a cartel is discounted, reputation falls fast);
//! cargo-mutants showed those pass with a wrong decay, an unweighted baseline, a wrong
//! even-length median or a wrong mean in Pearson. These pin the formulas themselves.

use scoring::collusion::{correlation_matrix, discount_weights};
use scoring::reputation::{author_score, difference_scores, loo_baseline, weight_cap, AuthorPrior};

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

/// `p̄_{−u,j} = Σ_{v≠u} w_v p_vj / Σ_{v≠u} w_v`: the reviewer's own forecast is left out
/// and a weight-2 peer counts twice (D33).
#[test]
fn loo_baseline_is_the_weighted_mean_of_the_others() {
    let preds = vec![vec![0.9, 0.2], vec![0.3, 0.8], vec![0.5, 0.5]];
    let w = [2.0, 1.0, 1.0];
    let for_first = loo_baseline(&preds, &w, 0);
    assert!((for_first[0] - (0.3 + 0.5) / 2.0).abs() < EPS);
    assert!((for_first[1] - (0.8 + 0.5) / 2.0).abs() < EPS);
    let for_second = loo_baseline(&preds, &w, 1);
    assert!((for_second[0] - (2.0 * 0.9 + 0.5) / 3.0).abs() < EPS);
    assert!((for_second[1] - (2.0 * 0.2 + 0.5) / 3.0).abs() < EPS);
}

/// No weight among the others (all on probation): their plain mean is the crowd, never
/// 0/0; with no other panelist there is no crowd and nothing is scored.
#[test]
fn loo_baseline_without_weight_among_the_others_is_their_plain_mean() {
    let preds = vec![vec![0.9, 0.2], vec![0.3, 0.8], vec![0.5, 0.5]];
    let got = loo_baseline(&preds, &[1.0, 0.0, 0.0], 0);
    assert!((got[0] - 0.4).abs() < EPS && (got[1] - 0.65).abs() < EPS);
    assert!(loo_baseline(&preds[..1], &[1.0], 0).is_empty());
    assert!(difference_scores(&preds[..1], &[1.0], &[1.0, 0.0])[0].is_empty());
}

/// `S_uj = (p̄_{−u,j} − o_j)² − (p_uj − o_j)²`: forecasting 0.9 against a crowd at 0.4
/// on an item that passes scores 0.36 − 0.01 = 0.35; on one that fails, 0.16 − 0.81.
#[test]
fn difference_score_by_hand() {
    let preds = vec![vec![0.9, 0.9], vec![0.3, 0.3], vec![0.5, 0.5]];
    let s = difference_scores(&preds, &[1.0, 1.0, 1.0], &[1.0, 0.0]);
    assert!((s[0][0] - 0.35).abs() < EPS, "{}", s[0][0]);
    assert!((s[0][1] - (0.16 - 0.81)).abs() < EPS, "{}", s[0][1]);
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
