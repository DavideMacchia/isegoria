//! Degenerate inputs have a defined, conservative answer instead of NaN (T36, IRT-001).

use scoring::irt::{point_biserial, theta_from_anchors, R_PBIS_MIN};

#[test]
fn theta_is_zero_when_every_anchor_total_is_equal() {
    // Everyone answered the anchors alike: no spread, no ability signal to scale.
    let anchors = vec![vec![1.0, 0.0, 1.0]; 50];
    assert_eq!(theta_from_anchors(&anchors), vec![0.0; 50]);
    // Everyone perfect, everyone zero: same.
    assert_eq!(theta_from_anchors(&vec![vec![1.0; 4]; 3]), vec![0.0; 3]);
    assert_eq!(theta_from_anchors(&vec![vec![0.0; 4]; 3]), vec![0.0; 3]);
}

#[test]
fn theta_of_no_respondents_is_empty() {
    assert!(theta_from_anchors(&[]).is_empty());
}

#[test]
fn theta_is_still_standardized_when_there_is_spread() {
    let anchors: Vec<Vec<f64>> = (0..10).map(|i| vec![(i % 3) as f64, 1.0]).collect();
    let theta = theta_from_anchors(&anchors);
    let n = theta.len() as f64;
    let mean = theta.iter().sum::<f64>() / n;
    let var = theta.iter().map(|t| (t - mean).powi(2)).sum::<f64>() / n;
    assert!(mean.abs() < 1e-12, "mean = {mean}");
    assert!((var - 1.0).abs() < 1e-12, "var = {var}");
}

#[test]
fn point_biserial_without_variance_is_zero_and_fails_the_screen() {
    let theta: Vec<f64> = (0..20).map(|i| i as f64 / 10.0 - 1.0).collect();
    // Everyone got the item right / wrong: the item has no variance.
    for constant in [0.0, 1.0] {
        let r = point_biserial(&[constant; 20], &theta);
        assert_eq!(r, 0.0);
        assert!(r < R_PBIS_MIN);
    }
    // Flat θ (the constant-anchor case above): no variance on the other side.
    let item: Vec<f64> = (0..20).map(|i| (i % 2) as f64).collect();
    assert_eq!(point_biserial(&item, &[0.0; 20]), 0.0);
    // No respondents at all.
    assert_eq!(point_biserial(&[], &[]), 0.0);
}

#[test]
fn point_biserial_is_unchanged_on_ordinary_input() {
    let theta: Vec<f64> = (0..20).map(|i| i as f64).collect();
    let item: Vec<f64> = (0..20).map(|i| (i >= 10) as i32 as f64).collect();
    let r = point_biserial(&item, &theta);
    assert!(r > 0.8 && r <= 1.0, "r = {r}");
    let reversed: Vec<f64> = item.iter().map(|x| 1.0 - x).collect();
    assert!((point_biserial(&reversed, &theta) + r).abs() < 1e-12);
}

/// A hand-computed case pins the formula, not only its range: item [0,0,1,1] against
/// totals [1,2,3,4] gives cov 2, item variance 1, total variance 5 → r = 2/√5 (T41).
#[test]
fn point_biserial_matches_a_hand_computed_value() {
    let r = point_biserial(&[0.0, 0.0, 1.0, 1.0], &[1.0, 2.0, 3.0, 4.0]);
    assert!((r - 2.0 / 5.0_f64.sqrt()).abs() < 1e-12, "r = {r}");
}

/// The 2PL difficulty `b = −intercept/slope` is recovered from data generated with a
/// known `(a, b)`, not only the discrimination (T41).
#[test]
fn fit_2pl_recovers_the_item_difficulty() {
    let (a, b) = (1.5, 0.4);
    let mut theta = Vec::new();
    let mut resp = Vec::new();
    for i in 0..3000 {
        let t = -3.0 + 6.0 * i as f64 / 2999.0;
        let p = 1.0 / (1.0 + (-a * (t - b)).exp());
        // Deterministic low-discrepancy draw, as in the glm unit test.
        let u = (i as f64 * 0.618_033_988_75).fract();
        theta.push(t);
        resp.push(if u < p { 1.0 } else { 0.0 });
    }
    let fit = scoring::irt::fit_2pl_item(&theta, &resp);
    assert_eq!(fit.status, scoring::LogisticFit::Converged);
    assert!((fit.a - a).abs() < 0.15, "a = {}", fit.a);
    assert!((fit.b - b).abs() < 0.1, "b = {}", fit.b);
}
