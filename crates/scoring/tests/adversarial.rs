//! Adversarial scenarios (`docs/06`): compose several mechanisms to show the system
//! resists the attacks it is designed against.

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use scoring::collusion::{cluster_by_correlation, correlation_matrix, discount_weights, ALPHA};
use scoring::reputation::{capped_weight, weight_cap, Cusum, CusumParams};

/// A cartel of 400 identical votes cannot outweigh 120 independent honest reviewers:
/// correlation clustering finds the block and the √k discount collapses its influence.
#[test]
fn a_cartel_cannot_outweigh_an_honest_majority() {
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

/// A reviewer who hoards reputation and spends it is caught by the CUSUM change detector
/// (`docs/01` D34, `docs/02` §C.4): a sustained drop trips it and the reviewer returns to
/// probation.
#[test]
fn a_long_con_is_unprofitable() {
    let params = CusumParams::default();
    let mut c = Cusum::new();
    let mean = 0.01;
    for i in 0..500 {
        let honest = mean + if i % 2 == 0 { 0.08 } else { -0.08 };
        assert!(!c.observe(mean, honest, &params), "false alarm at {i}");
    }
    let caught = (1..=100).find(|_| c.observe(mean, mean - 0.1, &params));
    assert!(caught.is_some_and(|n| n <= 30), "caught at {caught:?}");

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
