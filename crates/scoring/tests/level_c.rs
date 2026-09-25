//! Level C acceptance tests. The evaluator BSS is checked against the VALUTATORI
//! oracle of `sim/bridging_irt_dif.py`; the author score against the docs examples.

use scoring::reputation::{
    asymmetric_ema, author_score, base_rate_baseline, brier_skill_score, capped_weight,
    crowd_baseline, dasgupta_ghosh, loo_scores, mean_score, odds_weight, proposal_rate, weight_cap,
    AuthorPrior, EvaluatorParams,
};
use std::fs;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn read_matrix(name: &str) -> Vec<Vec<f64>> {
    let text = fs::read_to_string(fixtures_dir().join(name)).unwrap();
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            l.split(',')
                .map(|c| c.trim().parse::<f64>().unwrap())
                .collect()
        })
        .collect()
}

fn read_vector(name: &str) -> Vec<f64> {
    read_matrix(name).into_iter().map(|r| r[0]).collect()
}

fn read_bss() -> Vec<f64> {
    let text = fs::read_to_string(fixtures_dir().join("levelc_bss.csv")).unwrap();
    text.lines()
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.split(',').nth(1).unwrap().trim().parse::<f64>().unwrap())
        .collect()
}

#[test]
fn evaluator_bss_reproduces_the_oracle() {
    // Profile order: base_rate, follows_peers, follows_bridging, expert, partisan.
    let o = read_vector("levelc_o.csv");
    let p = read_matrix("levelc_p.csv");
    let expected = read_bss();
    let baseline = base_rate_baseline(&o);

    for (k, &exp) in expected.iter().enumerate() {
        let bss = brier_skill_score(&p[k], &o, &baseline);
        assert!(
            (bss - exp).abs() < 1e-6,
            "profile {k}: bss={bss:.4} exp={exp:.4}"
        );
    }
}

#[test]
fn following_the_crowd_does_not_pay() {
    // Under the leave-one-out difference score (docs/01 D33): the expert who tracks the
    // outcomes beats the crowd; the crowd-followers do not.
    let o = read_vector("levelc_o.csv");
    let p = read_matrix("levelc_p.csv");
    let scores = loo_scores(&p, &vec![1.0; p.len()], &o);
    let s = |k: usize| mean_score(&scores[k]);

    // Profiles: 0 base-rate, 1 follows-peers, 2 follows-bridging, 3 expert, 4 partisan.
    assert!(s(3) > 0.0, "the expert should beat the crowd: {}", s(3));
    assert!(s(3) > s(1), "expert beats peer-following");
    assert!(s(3) > s(2), "expert beats bridging-following");
    // The retired ratio-form skill score told the same story on this fixture; what it
    // could not do is stay proper (paper Prop. 12, `evaluator_score.rs`).
    let baseline = crowd_baseline(&p, &vec![1.0; p.len()]);
    assert!(brier_skill_score(&p[3], &o, &baseline) > 0.0);
}

#[test]
fn at_rep_02_a_consensus_follower_scores_zero() {
    // docs/08 AT-REP-02 / D33: a reviewer whose forecast is the other panelists' mean
    // earns exactly 0 on every item, so their odds weight is exactly 1 — not a reward.
    let o = read_vector("levelc_o.csv");
    let mut p = read_matrix("levelc_p.csv");
    let n = p.len();
    let w = vec![1.0; n];
    // Replace the partisan profile by a copier of the other four.
    let others: Vec<f64> = (0..o.len())
        .map(|j| (0..n - 1).map(|u| p[u][j]).sum::<f64>() / (n - 1) as f64)
        .collect();
    p[n - 1] = others;
    let copier = &loo_scores(&p, &w, &o)[n - 1];
    assert!(copier.iter().all(|&d| d == 0.0), "copier scores {copier:?}");
    let s = mean_score(copier);
    assert_eq!(s, 0.0);
    assert_eq!(odds_weight(s, 400, &EvaluatorParams::default()), 1.0);
}

#[test]
fn the_odds_weight_is_positive_and_monotone_in_the_skill() {
    let p = EvaluatorParams::default();
    let low = odds_weight(-0.05, 400, &p);
    let mid = odds_weight(0.0, 400, &p);
    let high = odds_weight(0.05, 400, &p);
    assert!(low > 0.0 && low < mid && mid < high);
    assert!((mid - 1.0).abs() < 1e-12);
    // Nothing scored yet: weight 1 whatever the skill (D36: shrinkage after probation).
    assert_eq!(odds_weight(0.5, 0, &p), 1.0);
}

#[test]
fn author_score_shrinks_small_samples() {
    // docs/02 §C.1: 2/2 accepted → 4/7 ≈ 0.571; 180/200 → 182/205 ≈ 0.888.
    let prior = AuthorPrior::default();

    let a = author_score(&[1.0, 1.0], &[0.0, 0.0], &prior);
    assert!((a - 4.0 / 7.0).abs() < 1e-9, "A = {a:.4}");

    let mut q = vec![1.0; 180];
    q.extend(vec![0.0; 20]);
    let ages = vec![0.0; 200];
    let b = author_score(&q, &ages, &prior);
    assert!((b - 182.0 / 205.0).abs() < 1e-9, "B = {b:.4}");

    assert!(
        a < b,
        "the prolific reliable author must rank above the lucky one"
    );
}

#[test]
fn author_score_decays_with_age() {
    let prior = AuthorPrior::default();
    let fresh = author_score(&[1.0], &[0.0], &prior);
    let old = author_score(&[1.0], &[60.0], &prior);
    assert!(
        old < fresh,
        "old success should count less: old={old:.4} fresh={fresh:.4}"
    );
}

#[test]
fn proposal_rate_scales_with_author_score() {
    assert!((proposal_rate(0.0, 0.5, 5.0) - 0.5).abs() < 1e-9);
    assert!((proposal_rate(1.0, 0.5, 5.0) - 5.0).abs() < 1e-9);
    assert!(proposal_rate(0.5, 0.5, 5.0) > proposal_rate(0.2, 0.5, 5.0));
}

#[test]
fn reputation_rises_slowly_and_falls_fast() {
    let up = asymmetric_ema(0.5, 0.9, 0.1, 0.8);
    let down = asymmetric_ema(0.5, 0.1, 0.1, 0.8);
    assert!((up - 0.54).abs() < 1e-9, "slow rise: {up}");
    assert!((down - 0.18).abs() < 1e-9, "fast fall: {down}");
}

#[test]
fn weight_cap_limits_a_single_node() {
    let weights = vec![0.2, 0.4, 0.5, 0.6, 0.9];
    let cap = weight_cap(&weights); // 3 * median(0.5) = 1.5
    assert!((cap - 1.5).abs() < 1e-9);
    assert!((capped_weight(2.0, cap) - 1.5).abs() < 1e-9);
    assert!((capped_weight(0.7, cap) - 0.7).abs() < 1e-9);
}

#[test]
fn peer_prediction_rewards_informative_agreement() {
    // Agreement on the shared item, disagreement on the separate items → +1.
    assert!((dasgupta_ghosh(true, true, true, false) - 1.0).abs() < 1e-9);
    // Agreement everywhere → 0: the shared agreement is only baseline agreement.
    assert!(dasgupta_ghosh(true, true, true, true).abs() < 1e-9);
}

/// REPUTATION-003 / AT-REP-03: a zero-variance baseline (every outcome identical, so
/// the base rate equals every outcome) leaves `brier_skill_score` dividing by zero.
/// The result MUST be finite and defined — reported as a neutral 0.0, not NaN/−∞.
#[test]
fn brier_skill_score_is_finite_when_the_baseline_is_perfect() {
    let o = vec![1.0, 1.0, 1.0, 1.0];
    let baseline = base_rate_baseline(&o); // = [1,1,1,1], zero error
                                           // A predictor that also nails it, and one that is wrong: both must stay finite.
    for p in [vec![1.0; 4], vec![0.2, 0.9, 0.5, 0.7]] {
        let bss = brier_skill_score(&p, &o, &baseline);
        assert!(bss.is_finite(), "BSS must be finite, got {bss}");
        assert_eq!(
            bss, 0.0,
            "no skill is measurable against a perfect baseline"
        );
    }
}
