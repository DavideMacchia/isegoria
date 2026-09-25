//! Live outcomes with randomized exploration (`docs/01` D35, T52; paper Prop. "Exploration
//! restores properness"). When the gate decides which outcomes are observed, scoring only
//! the observed items pays a reviewer to report on the gate's side; observing a random
//! fraction `ε` of the rejections and weighting each observed score by `1/π_j` gives a
//! score whose expectation is the full-information score, which truthful reports maximize
//! (AT-REP-06).

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use scoring::reputation::{difference_score, inverse_probability_mean, loo_scores};

/// The gate passes an item on a report of at least `tau`; a rejection is explored, and
/// its outcome observed, with probability `epsilon`.
fn inclusion(report: f64, tau: f64, epsilon: f64) -> f64 {
    if report >= tau {
        1.0
    } else {
        epsilon
    }
}

/// Exact expectation over the outcome (belief `q`) and the observation draw of the
/// reviewer's per-item score: weighted by `1/π` when `weighted`, the bare observed score
/// otherwise (0 when unobserved).
fn expected_score(p: f64, q: f64, b: f64, tau: f64, epsilon: f64, weighted: bool) -> f64 {
    let pi = inclusion(p, tau, epsilon);
    let weight = if weighted { 1.0 / pi } else { 1.0 };
    [0.0, 1.0]
        .iter()
        .map(|&o| {
            let prob = if o > 0.5 { q } else { 1.0 - q };
            // Observed with probability π (score × weight), unobserved otherwise (0).
            prob * pi * weight * difference_score(p, b, o)
        })
        .sum()
}

/// AT-REP-06, the exact half: under a gate that observes the outcome only on the
/// reports it passes (and on `ε` of the rest), the inverse-probability-weighted score has
/// exactly the full-information expectation `(b − q)² − (p − q)²` whatever the report, so
/// the true belief is its unique optimum; the bare observed score is scaled by `π(p)` and
/// pays a reviewer who believes 0.40 while the crowd says 0.70 to report 0.50 — the
/// gate's side — whether the exploration rate is 5% or zero.
#[test]
fn at_rep_06_the_weighted_score_is_proper_under_a_report_dependent_gate() {
    let (tau, eps) = (0.5, 0.05);
    let mut rng = ChaCha8Rng::seed_from_u64(35);
    for _ in 0..2000 {
        let q: f64 = rng.gen_range(0.05..0.95);
        let b: f64 = rng.gen_range(0.05..0.95);
        let p: f64 = rng.gen_range(0.0..=1.0);
        let full = (b - q).powi(2) - (p - q).powi(2);
        let weighted = expected_score(p, q, b, tau, eps, true);
        assert!(
            (weighted - full).abs() < 1e-12,
            "report {p:.3}, belief {q:.3}, crowd {b:.3}: weighted {weighted:.6} vs full {full:.6}"
        );
        assert!(
            expected_score(q, q, b, tau, eps, true) - weighted >= -1e-12,
            "a report other than the belief scored higher"
        );
    }
    // The paper's dissenter, under the gate: honest 0.40 against a crowd at 0.70.
    let (q, b) = (0.40, 0.70);
    let truthful = expected_score(q, q, b, tau, eps, false);
    let on_the_gate_s_side = expected_score(0.50, q, b, tau, eps, false);
    println!(
        "bare observed score: truthful {truthful:.4}, reporting 0.50 {on_the_gate_s_side:.4}; \
         without exploration: truthful {:.4}, reporting 0.50 {:.4}",
        expected_score(q, q, b, tau, 0.0, false),
        expected_score(0.50, q, b, tau, 0.0, false)
    );
    assert!(
        on_the_gate_s_side > truthful,
        "the bare observed score should pay the reviewer to cross to the gate's side"
    );
    assert!(expected_score(0.50, q, b, tau, 0.0, false) > expected_score(q, q, b, tau, 0.0, false));
    let truthful_w = expected_score(q, q, b, tau, eps, true);
    let shaded_w = expected_score(0.50, q, b, tau, eps, true);
    assert!(
        truthful_w > shaded_w,
        "weighted: truthful {truthful_w:.4} should beat shading to 0.50 ({shaded_w:.4})"
    );
    assert!((truthful_w - 0.09).abs() < 1e-12 && (shaded_w - 0.08).abs() < 1e-12);
}

fn normal(r: &mut ChaCha8Rng) -> f64 {
    let u1: f64 = 1.0 - r.gen::<f64>();
    let u2: f64 = r.gen();
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

/// AT-REP-06, the Monte Carlo half: a panel of nine with forecast noise from 0.05 to 0.30
/// (the paper's marked regime) on 100,000 items; the gate passes an item when the panel's
/// mean forecast reaches 0.5 and a seeded draw explores 5% of the rest. Per reviewer, the
/// inverse-probability-weighted mean of the observed leave-one-out scores is within Monte
/// Carlo error of the full-information mean over every item, while the mean of the
/// passed items alone is biased by many standard errors for the best and the worst
/// reviewer.
#[test]
fn the_weighted_mean_matches_the_full_information_score_within_monte_carlo_error() {
    const N: usize = 100_000;
    const K: usize = 9;
    const EPS: f64 = 0.05;
    let sigma: Vec<f64> = (0..K)
        .map(|u| 0.05 + 0.25 * u as f64 / (K - 1) as f64)
        .collect();
    let weights = vec![1.0; K];
    let mut r = ChaCha8Rng::seed_from_u64(52);
    let mut full: Vec<Vec<f64>> = (0..K).map(|_| Vec::with_capacity(N)).collect();
    let mut observed: Vec<Vec<(f64, f64)>> = vec![Vec::new(); K];
    let mut passed_only = vec![Vec::new(); K];
    let mut terms: Vec<Vec<f64>> = (0..K).map(|_| Vec::with_capacity(N)).collect();
    let mut explored = 0usize;
    let mut passed = 0usize;
    for _ in 0..N {
        let pi = (r.gen::<f64>() + r.gen::<f64>()) / 2.0;
        let o = f64::from(r.gen::<f64>() < pi);
        let forecasts: Vec<Vec<f64>> = sigma
            .iter()
            .map(|s| vec![(pi + s * normal(&mut r)).clamp(0.02, 0.98)])
            .collect();
        let mean = forecasts.iter().map(|f| f[0]).sum::<f64>() / K as f64;
        let scores = loo_scores(&forecasts, &weights, &[o]);
        let gate_passes = mean >= 0.5;
        let inclusion = if gate_passes { 1.0 } else { EPS };
        let is_observed = gate_passes || r.gen::<f64>() < EPS;
        passed += usize::from(gate_passes);
        explored += usize::from(is_observed && !gate_passes);
        for u in 0..K {
            let d = scores[u][0];
            full[u].push(d);
            terms[u].push(if is_observed { d / inclusion } else { 0.0 });
            if is_observed {
                observed[u].push((d, inclusion));
            }
            if gate_passes {
                passed_only[u].push(d);
            }
        }
    }
    println!(
        "{passed} of {N} items pass the gate; {explored} rejections explored ({:.2}%)",
        100.0 * explored as f64 / (N - passed) as f64
    );
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    let mut worst_bias_in_se = 0.0f64;
    for u in 0..K {
        let truth = mean(&full[u]);
        let weighted = inverse_probability_mean(&observed[u], N);
        let se = {
            let m = mean(&terms[u]);
            (terms[u].iter().map(|t| (t - m).powi(2)).sum::<f64>() / (N - 1) as f64 / N as f64)
                .sqrt()
        };
        let bare = passed_only[u].iter().sum::<f64>() / N as f64;
        let bias_in_se = (bare - truth).abs() / se;
        worst_bias_in_se = worst_bias_in_se.max(bias_in_se);
        println!(
            "σ = {:.3}: full {truth:+.5}, weighted {weighted:+.5} (se {se:.5}), passed-only \
             {bare:+.5} ({bias_in_se:.1} se off)",
            sigma[u]
        );
        assert!(
            (weighted - truth).abs() <= 4.0 * se,
            "σ = {:.3}: weighted {weighted:.5} vs full {truth:.5}, se {se:.5}",
            sigma[u]
        );
        assert_eq!(observed[u].len(), passed + explored);
    }
    assert!(
        worst_bias_in_se > 4.0,
        "the passed-only mean should be visibly biased for some reviewer ({worst_bias_in_se:.1} se)"
    );
}

/// With every outcome observed at probability one the weighted mean is the plain mean;
/// with nothing reviewed it is 0.
#[test]
fn the_inverse_probability_mean_reduces_to_the_plain_mean_when_everything_is_observed() {
    let scores = [0.1, -0.2, 0.4, 0.0];
    let observed: Vec<(f64, f64)> = scores.iter().map(|&d| (d, 1.0)).collect();
    let plain = scores.iter().sum::<f64>() / scores.len() as f64;
    assert!((inverse_probability_mean(&observed, scores.len()) - plain).abs() < 1e-15);
    assert_eq!(inverse_probability_mean(&[], 0), 0.0);
    // One explored item among twenty reviewed counts twenty times.
    assert!((inverse_probability_mean(&[(0.3, 0.05)], 20) - 0.3).abs() < 1e-15);
}
