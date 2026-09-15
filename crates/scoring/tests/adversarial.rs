//! Adversarial scenarios (`docs/06`): compose several mechanisms to show the system
//! resists the attacks it is designed against.

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use scoring::collusion::{cluster_by_correlation, correlation_matrix, discount_weights, ALPHA};
use scoring::reputation::{asymmetric_ema, capped_weight, weight_cap};

#[test]
fn a_cartel_cannot_outweigh_an_honest_majority() {
    // 120 honest reviewers with independent behavior, and a cartel of 400 that all
    // vote identically to seize control. Behavior correlation detects the cartel as a
    // single block and the √k discount collapses its influence.
    let m = 24;
    let mut rng = ChaCha8Rng::seed_from_u64(1);
    let mut judgments: Vec<Vec<f64>> = Vec::new();
    for _ in 0..120 {
        judgments.push((0..m).map(|_| rng.gen::<f64>()).collect());
    }
    let cartel_pattern: Vec<f64> = (0..m).map(|_| rng.gen::<f64>()).collect();
    for _ in 0..400 {
        judgments.push(cartel_pattern.clone());
    }

    let corr = correlation_matrix(&judgments);
    let clusters = cluster_by_correlation(&corr, 0.99);

    // The 400 colluders collapse into one cluster; the honest reviewers do not join it.
    let cartel_id = clusters[120];
    assert!(
        clusters[120..].iter().all(|&c| c == cartel_id),
        "cartel not one cluster"
    );
    assert!(
        clusters[..120].iter().all(|&c| c != cartel_id),
        "honest swallowed by cartel"
    );

    let weights = vec![1.0; judgments.len()];
    let discounted = discount_weights(&weights, &clusters, ALPHA);
    let cartel_influence: f64 = discounted[120..].iter().sum();
    let honest_influence: f64 = discounted[..120].iter().sum();

    // 400 coordinated nodes count as √400 = 20; 120 honest singletons count as 120.
    assert!(
        (cartel_influence - 400f64.sqrt()).abs() < 1e-6,
        "cartel = {cartel_influence:.2}"
    );
    assert!(
        (honest_influence - 120.0).abs() < 1e-6,
        "honest = {honest_influence:.2}"
    );
    assert!(
        honest_influence > 5.0 * cartel_influence,
        "a 400-strong cartel ({cartel_influence:.1}) must not overturn 120 honest ({honest_influence:.1})"
    );
}

#[test]
fn a_long_con_is_unprofitable() {
    // Reputation rises slowly and falls fast, so accumulating trust to spend it later
    // does not pay (docs/02 C.4).
    let (up, down) = (0.05, 0.5);
    let mut e = 0.5;
    for _ in 0..15 {
        e = asymmetric_ema(e, 1.0, up, down);
    }
    let peak = e;
    assert!(
        peak < 0.85,
        "even 15 honest epochs should not saturate reputation: {peak:.3}"
    );

    // One betrayal erases far more than a single honest epoch ever added.
    let after_betrayal = asymmetric_ema(peak, 0.0, up, down);
    let one_step_gain = up * (1.0 - peak);
    assert!(
        (peak - after_betrayal) > 10.0 * one_step_gain,
        "the fall ({:.3}) should dwarf a single gain ({:.4})",
        peak - after_betrayal,
        one_step_gain
    );

    // And even a maxed-out actor is capped at 3× the crowd median, so no single node
    // dominates the vote.
    let crowd = [0.3, 0.4, 0.4, 0.5, 0.6];
    let w_max = weight_cap(&crowd); // 3 * median(0.4) = 1.2
    assert!(
        (capped_weight(0.9, w_max) - 0.9).abs() < 1e-9,
        "a normal weight is untouched"
    );
    assert!(
        (capped_weight(5.0, w_max) - w_max).abs() < 1e-9,
        "a huge E_u is capped"
    );
}
