//! The evaluator score after D33: the leave-one-out difference score is strictly proper
//! (AT-REP-05), a crowd copier scores exactly 0 (AT-REP-02), the odds-scale weight lets
//! the `3 × median` cap bind (AT-REP-04); the ratio-form skill score is not (Prop. 12).

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use scoring::reputation::{
    brier_skill_score, capped_weight, difference_score, loo_baseline, loo_scores, mean_score,
    odds_weight, weight_cap, EvaluatorParams,
};

fn expected_difference_score(p: &[f64], q: &[f64], b: &[f64]) -> f64 {
    let m = p.len();
    let mut total = 0.0;
    for outcome in 0..(1usize << m) {
        let mut prob = 1.0;
        let mut score = 0.0;
        for j in 0..m {
            let o = ((outcome >> j) & 1) as f64;
            prob *= if o > 0.5 { q[j] } else { 1.0 - q[j] };
            score += difference_score(p[j], b[j], o);
        }
        total += prob * score;
    }
    total
}

/// AT-REP-05: for random beliefs and baselines, the exact expected score is maximized by
/// the true belief, and by exactly the squared-error gap over any other report.
#[test]
fn at_rep_05_the_difference_score_is_strictly_proper() {
    let mut rng = ChaCha8Rng::seed_from_u64(33);
    for m in 1..=4 {
        for _ in 0..50 {
            let q: Vec<f64> = (0..m).map(|_| rng.gen_range(0.1..0.9)).collect();
            let b: Vec<f64> = (0..m).map(|_| rng.gen_range(0.2..0.8)).collect();
            let truthful = expected_difference_score(&q, &q, &b);
            for _ in 0..20 {
                let p: Vec<f64> = (0..m).map(|_| rng.gen_range(0.0..=1.0)).collect();
                let gap: f64 = p.iter().zip(&q).map(|(p, q)| (p - q).powi(2)).sum();
                let other = expected_difference_score(&p, &q, &b);
                assert!(
                    (truthful - other - gap).abs() < 1e-12,
                    "m = {m}: truthful {truthful:.6}, report {other:.6}, gap {gap:.6}"
                );
            }
        }
    }
}

/// Paper Prop. 12: under the ratio-form skill score, a reviewer who believes 0.30 against
/// a crowd at 0.65 is best off reporting about 0.60 — toward the crowd, not the truth.
#[test]
fn the_retired_skill_score_paid_the_dissenter_to_move_toward_the_crowd() {
    let (q, b) = (0.30, 0.65);
    let expected_bss = |p: f64| {
        q * brier_skill_score(&[p], &[1.0], &[b])
            + (1.0 - q) * brier_skill_score(&[p], &[0.0], &[b])
    };
    let (mut best_p, mut best) = (0.0, f64::NEG_INFINITY);
    for i in 1..100 {
        let p = i as f64 / 100.0;
        let e = expected_bss(p);
        if e > best {
            (best_p, best) = (p, e);
        }
    }
    assert!((best_p - 0.60).abs() < 0.011, "optimal report {best_p}");
    assert!(expected_bss(0.60) > expected_bss(q));
    // The difference score, on the same belief and crowd, is maximized by the truth.
    let truthful = expected_difference_score(&[q], &[q], &[b]);
    assert!(truthful > expected_difference_score(&[0.60], &[q], &[b]));
}

/// The leave-one-out baseline weighs the other reviewers, falling back to 0 when none
/// carry weight.
#[test]
fn the_baseline_is_the_weighted_mean_of_the_others() {
    let preds = vec![vec![0.9, 0.2], vec![0.3, 0.8], vec![0.5, 0.5]];
    let base = loo_baseline(&preds, &[2.0, 1.0, 1.0]);
    assert!((base[0][0] - 0.4).abs() < 1e-12 && (base[0][1] - 0.65).abs() < 1e-12);
    assert!((base[1][0] - 2.3 / 3.0).abs() < 1e-12);
    let alone = loo_scores(&[vec![0.9, 0.2]], &[1.0], &[1.0, 0.0]);
    assert_eq!(alone, vec![vec![0.0, 0.0]]);
    let weightless = loo_scores(&preds, &[1.0, 0.0, 0.0], &[1.0, 0.0]);
    assert_eq!(weightless[0], vec![0.0, 0.0]);
    assert!(weightless[1][0] != 0.0);
}

/// AT-REP-04: on the odds scale, an outlier's weight is capped at `3 × median` of the crowd.
#[test]
fn at_rep_04_the_cap_binds_on_an_outlier() {
    let p = EvaluatorParams::default();
    let mut weights: Vec<f64> = vec![odds_weight(0.0, 400, &p); 9];
    weights.push(odds_weight(0.1, 400, &p));
    assert!((weights[9] - (35.0f64 * 0.1 * 0.8).exp()).abs() < 1e-9);
    assert!(weights[9] > 16.0);
    let cap = weight_cap(&weights); // 3 × median(1) = 3
    assert!((cap - 3.0).abs() < 1e-12);
    assert!((capped_weight(weights[9], cap) - 3.0).abs() < 1e-12);
    assert!((capped_weight(weights[0], cap) - 1.0).abs() < 1e-12);
}

/// D33's arithmetic: reliably scoring above the reference raises the odds weight;
/// shrinkage tempers luck at low sample counts.
#[test]
fn shrinkage_stops_luck_from_buying_weight() {
    let p = EvaluatorParams::default();
    let reliable = odds_weight(0.02, 100_000, &p);
    assert!((reliable - 2.0).abs() < 0.02, "{reliable}");
    let no_shrinkage = EvaluatorParams { k0: 0.0, ..p };
    assert!((odds_weight(0.025, 16, &no_shrinkage) - 2.40).abs() < 0.01);
    assert!((odds_weight(0.025, 16, &p) - 1.13).abs() < 0.01);
    assert_eq!(mean_score(&[]), 0.0);
}
