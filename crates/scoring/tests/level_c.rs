//! Level C acceptance tests. The evaluator score — the leave-one-out difference score
//! and its odds weight (`docs/02` §C.2, `docs/01` D33, T50) — is checked against the
//! evaluators oracle of `sim/bridging_irt_dif.py` (`levelc_scores.csv`); the author
//! score against the docs examples.

use scoring::reputation::{
    author_score, cap_weights, capped_weight, dasgupta_ghosh, difference_scores, loo_baseline,
    mean_score, odds_weight, proposal_rate, weight_cap, AuthorPrior, EvaluatorHistory, CUSUM_H,
    CUSUM_K, GAMMA, K_SHRINK,
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

/// `(loo_score, weight)` per profile from `levelc_scores.csv`.
fn read_scores() -> Vec<(f64, f64)> {
    let text = fs::read_to_string(fixtures_dir().join("levelc_scores.csv")).unwrap();
    text.lines()
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let mut cols = l.split(',').skip(1);
            let mut next = || cols.next().unwrap().trim().parse::<f64>().unwrap();
            (next(), next())
        })
        .collect()
}

/// The five sim profiles scored against one another with uniform weights: each one's
/// crowd is the mean forecast of the other four.
fn profile_scores() -> Vec<f64> {
    let o = read_vector("levelc_o.csv");
    let p = read_matrix("levelc_p.csv");
    difference_scores(&p, &vec![1.0; p.len()], &o)
        .iter()
        .map(|per_item| mean_score(per_item))
        .collect()
}

#[test]
fn evaluator_scores_reproduce_the_oracle() {
    // Profile order: base_rate, follows_peers, follows_bridging, expert, partisan. The
    // oracle weights each profile over its k = 10 scored items.
    let o = read_vector("levelc_o.csv");
    let expected = read_scores();
    assert_eq!(expected.len(), 5);
    for (k, (s, &(exp_s, exp_w))) in profile_scores().iter().zip(&expected).enumerate() {
        assert!(
            (s - exp_s).abs() < 1e-6,
            "profile {k}: score {s:.6} vs oracle {exp_s:.6}"
        );
        let w = odds_weight(*s, o.len(), GAMMA, K_SHRINK);
        assert!(
            (w - exp_w).abs() < 1e-6 * exp_w.max(1.0),
            "profile {k}: weight {w:.6} vs oracle {exp_w:.6}"
        );
    }
}

#[test]
fn following_the_crowd_does_not_pay() {
    // Profiles: 0 base-rate, 1 follows-peers, 2 follows-bridging, 3 expert, 4 partisan.
    // The expert, who tracks the outcomes, beats the crowd; the crowd-followers and the
    // partisan score below it.
    let s = profile_scores();
    assert!(s[3] > 0.0, "the expert should beat the crowd: {}", s[3]);
    assert!(s[3] > s[1], "expert beats peer-following");
    assert!(s[3] > s[2], "expert beats bridging-following");
    assert!(s[1] < 0.0, "following the peer average loses: {}", s[1]);
    assert!(s[4] < 0.0, "the partisan loses: {}", s[4]);
}

/// AT-REP-02 (D6, D23, D33): a reviewer whose forecast is the other panelists' weighted
/// mean scores exactly 0 on every item, for every outcome — not only in expectation
/// (paper Prop. 14 (ii)) — so following the crowd earns nothing, and its weight is 1.
#[test]
fn at_rep_02_a_consensus_follower_scores_exactly_zero() {
    let p = read_matrix("levelc_p.csv");
    let weights = [1.0, 2.0, 0.5, 1.0, 1.0];
    let n = p.len();
    // The copier joins the panel with the others' weighted mean as its forecast.
    let mut panel = p.clone();
    panel.push(loo_baseline(&p, &weights, n));
    let mut w = weights.to_vec();
    w.push(1.0);
    for o in [
        read_vector("levelc_o.csv"),
        vec![1.0; 10],
        vec![0.0; 10],
        (0..10).map(|j| (j % 2) as f64).collect(),
    ] {
        let scores = difference_scores(&panel, &w, &o);
        assert!(
            scores[n].iter().all(|&s| s == 0.0),
            "copier's scores {:?}",
            scores[n]
        );
    }
    assert_eq!(odds_weight(0.0, 500, GAMMA, K_SHRINK), 1.0);
}

/// AT-REP-03: the difference score has no denominator, so the case that made the ratio
/// score undefined — a crowd that is right about every outcome — is just a score.
#[test]
fn at_rep_03_scores_are_finite_when_the_crowd_is_perfect() {
    let o = vec![1.0, 1.0, 1.0, 1.0];
    let perfect = vec![1.0; 4];
    let hedged = vec![0.2, 0.9, 0.5, 0.7];
    let scores = difference_scores(&[perfect, hedged], &[1.0, 1.0], &o);
    assert!(scores.iter().flatten().all(|s| s.is_finite()));
    // The perfect forecaster beats the hedger by the hedger's squared errors …
    assert!((mean_score(&scores[0]) - (0.64 + 0.01 + 0.25 + 0.09) / 4.0).abs() < 1e-12);
    // … and the hedger loses exactly that.
    assert!((mean_score(&scores[0]) + mean_score(&scores[1])).abs() < 1e-12);
}

/// The odds weight matches the decision's examples (D33): fully shrunk, 0.02 better than
/// the crowd weighs double; with 16 scored items one standard error of luck (0.025)
/// buys ×2.4 without shrinkage and ×1.13 with it.
#[test]
fn odds_weight_matches_the_decision_examples() {
    assert_eq!(GAMMA, 35.0);
    assert_eq!(K_SHRINK, 100.0);
    let doubled = odds_weight(0.02, 1_000_000_000, GAMMA, K_SHRINK);
    assert!((doubled - 0.7f64.exp()).abs() < 1e-6, "{doubled}");
    assert!((doubled - 2.01).abs() < 0.01);
    let lucky_unshrunk = odds_weight(0.025, 16, GAMMA, 0.0);
    let lucky = odds_weight(0.025, 16, GAMMA, K_SHRINK);
    assert!((lucky_unshrunk - 2.4).abs() < 0.01, "{lucky_unshrunk}");
    assert!((lucky - 1.13).abs() < 0.005, "{lucky}");
    // Exactly 1 at the crowd's level, or before anything is scored; monotone in the
    // score and, for a positive score, in the count.
    assert_eq!(odds_weight(0.0, 50, GAMMA, K_SHRINK), 1.0);
    assert_eq!(odds_weight(0.3, 0, GAMMA, K_SHRINK), 1.0);
    assert!(odds_weight(-0.02, 50, GAMMA, K_SHRINK) < 1.0);
    assert!(odds_weight(0.02, 50, GAMMA, K_SHRINK) < odds_weight(0.02, 100, GAMMA, K_SHRINK));
    assert!(odds_weight(0.01, 50, GAMMA, K_SHRINK) < odds_weight(0.02, 50, GAMMA, K_SHRINK));
}

/// AT-REP-04 (REPUTATION-005, G-12): on the odds scale the `3 × median` cap binds.
/// Nine reviewers at the crowd's level and one reliably 0.05 better over 400 outcomes
/// (exp(1.4) ≈ 4.06): the outlier is capped at 3, the others untouched, and a
/// probationer's 0 does not pull the cap down.
#[test]
fn at_rep_04_the_cap_binds_on_an_outlier() {
    let mut w = vec![1.0; 9];
    w.push(odds_weight(0.05, 400, GAMMA, K_SHRINK));
    assert!(w[9] > 4.0, "{}", w[9]);
    let capped = cap_weights(&w);
    assert_eq!(capped[9], 3.0);
    assert!(capped[..9].iter().all(|&x| x == 1.0));

    let mut with_probation = vec![0.0; 5];
    with_probation.extend(&w);
    let capped = cap_weights(&with_probation);
    assert_eq!(&capped[..5], &[0.0; 5]);
    assert_eq!(capped[14], 3.0);
    assert_eq!(cap_weights(&[0.0, 0.0]), vec![0.0, 0.0]);
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

/// The history by hand (D34): the first score only seeds the mean; the CUSUM then adds
/// each drop below the mean of the previous items, less the allowance, and never goes
/// below 0; a drop past the threshold is an alarm that restarts the record.
#[test]
fn evaluator_history_by_hand() {
    assert_eq!(CUSUM_K, 0.03);
    assert_eq!(CUSUM_H, 1.5);
    let mut h = EvaluatorHistory::new();
    assert_eq!((h.score(), h.scored(), h.cusum()), (0.0, 0, 0.0));
    assert!(!h.record(0.2, CUSUM_K, CUSUM_H));
    assert_eq!(h.cusum(), 0.0, "no reference for the first item");
    assert!(!h.record(0.0, CUSUM_K, CUSUM_H));
    // reference 0.2: s = max(0, 0 + (0.2 − 0.0) − 0.03) = 0.17; the mean is now 0.1
    assert!((h.cusum() - 0.17).abs() < 1e-12, "{}", h.cusum());
    assert!((h.score() - 0.1).abs() < 1e-12);
    assert!(!h.record(0.5, CUSUM_K, CUSUM_H));
    // a rise: s = max(0, 0.17 + (0.1 − 0.5) − 0.03) = 0
    assert_eq!(h.cusum(), 0.0);
    assert_eq!(h.scored(), 3);
    // a collapse: reference 0.7/3, s = 0 + (0.2333 + 2.0) − 0.03 > 1.5 → alarm, restart
    assert!(h.record(-2.0, CUSUM_K, CUSUM_H));
    assert_eq!(
        (h.score(), h.scored(), h.cusum(), h.alarms()),
        (0.0, 0, 0.0, 1)
    );
    // the record rebuilds from nothing, the alarm stays counted
    assert!(!h.record(0.3, CUSUM_K, CUSUM_H));
    assert_eq!((h.scored(), h.alarms()), (1, 1));
    assert!((h.score() - 0.3).abs() < 1e-12);
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
