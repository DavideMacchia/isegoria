//! AT-REP-05 (`docs/01` D33, paper Prop. 14, `docs/08` REPUTATION-008): the leave-one-out
//! difference score is strictly proper. For random beliefs and crowd forecasts, the exact
//! expected score — enumerated over all `2^m` outcomes — is maximized by reporting the
//! true belief, and equals `c − Σ_j (p_j − q_j)² / m` for a `c` that does not depend on
//! the report. The retired ratio-form skill score was not (paper Prop. 12): with one
//! scored item its optimal report is `logit q + 2 logit b`, on the crowd's side.

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use scoring::reputation::{difference_scores, mean_score};

/// Exact expectation of the reviewer's mean difference score when it reports `report`,
/// believes `belief` and the other panelists (one peer, unit weights) forecast `crowd`.
fn expected_score(report: &[f64], belief: &[f64], crowd: &[f64]) -> f64 {
    let m = belief.len();
    let panel = [report.to_vec(), crowd.to_vec()];
    let mut expectation = 0.0;
    for bits in 0..(1u32 << m) {
        let outcomes: Vec<f64> = (0..m).map(|j| ((bits >> j) & 1) as f64).collect();
        let prob: f64 = belief
            .iter()
            .zip(&outcomes)
            .map(|(q, o)| if *o > 0.5 { *q } else { 1.0 - q })
            .product();
        let scores = difference_scores(&panel, &[1.0, 1.0], &outcomes);
        expectation += prob * mean_score(&scores[0]);
    }
    expectation
}

fn uniform(rng: &mut ChaCha8Rng, lo: f64, hi: f64, m: usize) -> Vec<f64> {
    (0..m).map(|_| lo + (hi - lo) * rng.gen::<f64>()).collect()
}

#[test]
fn at_rep_05_the_true_belief_maximizes_the_expected_score() {
    let mut rng = ChaCha8Rng::seed_from_u64(33);
    for m in 1..=4 {
        for _ in 0..50 {
            let belief = uniform(&mut rng, 0.1, 0.9, m);
            let crowd = uniform(&mut rng, 0.2, 0.8, m);
            let truthful = expected_score(&belief, &belief, &crowd);
            for _ in 0..20 {
                let report = uniform(&mut rng, 0.0, 1.0, m);
                let distance: f64 = report
                    .iter()
                    .zip(&belief)
                    .map(|(p, q)| (p - q).powi(2))
                    .sum::<f64>()
                    / m as f64;
                let loss = truthful - expected_score(&report, &belief, &crowd);
                // The closed form: the expected loss of a report is its mean squared
                // distance from the belief, so any other report is strictly worse.
                assert!(
                    (loss - distance).abs() < 1e-12,
                    "m={m}: loss {loss} vs {distance}"
                );
                assert!(
                    loss > 0.0,
                    "m={m}: a report other than the belief was not worse"
                );
            }
        }
    }
}

/// Why the rule changed (paper Prop. 12): on one item with the crowd at `b`, the ratio
/// score `1 − (p − o)² / (b − o)²` in expectation is maximized at
/// `logit p* = logit q + 2 logit b`. A reviewer who believes 0.30 while the crowd says
/// 0.65 was best off reporting 0.60 — crossing to the crowd's side. The difference score
/// puts the optimum back at 0.30.
#[test]
fn the_retired_ratio_score_paid_a_dissenter_to_move_toward_the_crowd() {
    let (q, b) = (0.30f64, 0.65f64);
    let logit = |x: f64| (x / (1.0 - x)).ln();
    let expit = |z: f64| 1.0 / (1.0 + (-z).exp());
    let expected_ratio = |p: f64| {
        1.0 - q * (1.0 - p).powi(2) / (1.0 - b).powi(2) - (1.0 - q) * p.powi(2) / b.powi(2)
    };
    let grid: Vec<f64> = (1..1000).map(|i| i as f64 / 1000.0).collect();
    let best = |f: &dyn Fn(f64) -> f64| {
        grid.iter()
            .copied()
            .max_by(|x, y| f(*x).total_cmp(&f(*y)))
            .unwrap()
    };
    let ratio_best = best(&expected_ratio);
    let closed_form = expit(logit(q) + 2.0 * logit(b));
    assert!((closed_form - 0.5965).abs() < 5e-4, "{closed_form}");
    assert!(
        (ratio_best - closed_form).abs() < 1e-3,
        "{ratio_best} vs {closed_form}"
    );

    let difference_best = best(&|p: f64| expected_score(&[p], &[q], &[b]));
    assert!((difference_best - q).abs() < 1e-9, "{difference_best}");
}
