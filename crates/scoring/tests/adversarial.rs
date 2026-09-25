//! Adversarial scenarios (`docs/06`): compose several mechanisms to show the system
//! resists the attacks it is designed against.

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use scoring::collusion::{cluster_by_correlation, correlation_matrix, discount_weights, ALPHA};
use scoring::reputation::{
    cap_weights, odds_weight, EvaluatorHistory, CUSUM_H, CUSUM_K, GAMMA, K_SHRINK,
};

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

fn normal(rng: &mut ChaCha8Rng) -> f64 {
    let u1: f64 = 1.0 - rng.gen::<f64>();
    let u2: f64 = rng.gen();
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

#[test]
fn a_long_con_is_unprofitable() {
    // Hoarding reputation to spend it later does not pay (docs/02 C.4, D33/D34): 300
    // honest items build a weight, then the con — forecasts flipped often enough to cost
    // about 0.15 per item — is caught by the CUSUM within about a hundred items, and the
    // alarm wipes the record: back to probation, weight 0, nothing to spend.
    let mut rng = ChaCha8Rng::seed_from_u64(5);
    let mut history = EvaluatorHistory::new();
    for _ in 0..300 {
        let honest = 0.01 + 0.10 * normal(&mut rng);
        assert!(
            !history.record(honest, CUSUM_K, CUSUM_H),
            "a false alarm in the honest phase"
        );
    }
    let before = odds_weight(history.score(), history.scored(), GAMMA, K_SHRINK);
    assert!(
        history.scored() == 300 && before > 1.0,
        "weight built honestly: {before}"
    );

    let mut caught_after = None;
    for t in 0..300 {
        let con = -0.15 + 0.20 * normal(&mut rng);
        if history.record(con, CUSUM_K, CUSUM_H) {
            caught_after = Some(t + 1);
            break;
        }
    }
    let caught_after = caught_after.expect("the con was never caught");
    assert!(
        caught_after <= 100,
        "caught only after {caught_after} items"
    );
    assert_eq!(history.scored(), 0, "the record is wiped");
    assert_eq!(history.alarms(), 1);

    // And even a maxed-out actor is capped at 3× the median of the counted weights, so no
    // single node dominates the vote.
    let mut weights = vec![1.0; 9];
    weights.push(odds_weight(0.1, 1000, GAMMA, K_SHRINK)); // exp(35 · 0.1 · 0.91) ≈ 24
    let capped = cap_weights(&weights);
    assert_eq!(capped[9], 3.0, "a huge weight is capped");
    assert!(
        capped[..9].iter().all(|&w| w == 1.0),
        "normal weights are untouched"
    );
}
