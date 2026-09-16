//! Composed gate decision (`docs/05` [4]/[5], `docs/02` §C.2): the path
//! `bridging_gate → (uncertainty band) → aggregate → resolve_band`, exercised with
//! adversarial panels. Reviewer skill is grounded in the real Level-C oracle profiles
//! (`levelc_p`/`levelc_o` → BSS → `E_u`); coordination is modelled with controlled
//! judgment vectors. Shows the band is resolved by weighted, anti-collusion-discounted
//! review — never a head-count, and robust to cartels and probationers.

use protocol::aggregate::{
    aggregate_pass_probability, resolve_band, review_weights, DECISION_THRESHOLD,
};
use protocol::gate::{bridging_gate, GateOutcome};
use protocol::probation::N_PROBATION;
use scoring::reputation::{base_rate_baseline, brier_skill_score, evaluator_score};
use std::fs;
use std::path::PathBuf;

// Gate band, matching the end-to-end epoch.
const TAU: f64 = 0.08;
const EPS: f64 = 0.008;
const APPEAL_THRESHOLD: f64 = 0.5;
const GAMMA: f64 = 1.0; // BSS → E_u squashing (documented test choice)
const CORR: f64 = 0.9;

// Level-C profile row order (see scoring/tests/level_c.rs).
const FOLLOWS_PEERS: usize = 1;
const EXPERT: usize = 3;
const PARTISAN: usize = 4;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../scoring/tests/fixtures")
}

fn read_matrix(name: &str) -> Vec<Vec<f64>> {
    fs::read_to_string(fixtures_dir().join(name))
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.split(',').map(|c| c.trim().parse().unwrap()).collect())
        .collect()
}

/// Real `E_u` for each Level-C profile, from its Brier Skill Score over the oracle
/// outcomes. Order: base_rate, follows_peers, follows_bridging, expert, partisan.
fn profile_e_u() -> Vec<f64> {
    let p = read_matrix("levelc_p.csv");
    let o: Vec<f64> = read_matrix("levelc_o.csv")
        .into_iter()
        .map(|r| r[0])
        .collect();
    let baseline = base_rate_baseline(&o);
    p.iter()
        .map(|pred| evaluator_score(brier_skill_score(pred, &o, &baseline), GAMMA))
        .collect()
}

fn onehot(pos: usize, len: usize) -> Vec<f64> {
    let mut v = vec![0.0; len];
    v[pos] = 1.0;
    v
}

#[test]
fn a_band_item_reaches_the_aggregation_but_a_clear_one_does_not() {
    // Only the uncertainty band routes to the aggregation; a clear pass or a clear
    // (polarized) reject is decided by bridging alone.
    assert_eq!(
        bridging_gate(TAU, 0.0, TAU, EPS, APPEAL_THRESHOLD),
        GateOutcome::SupplementaryReview
    );
    assert_eq!(
        bridging_gate(TAU + 2.0 * EPS, 0.0, TAU, EPS, APPEAL_THRESHOLD),
        GateOutcome::Pass
    );
    assert_eq!(
        bridging_gate(TAU - 2.0 * EPS, 0.9, TAU, EPS, APPEAL_THRESHOLD),
        GateOutcome::AppealEligible
    );
}

#[test]
fn the_higher_skill_reviewer_carries_the_band() {
    // A band item with the expert (high E_u) voting pass and the partisan (low E_u)
    // voting reject resolves the expert's way — decided by skill, not a 1-1 tie.
    let e = profile_e_u();
    assert!(e[EXPERT] > e[PARTISAN]);

    let e_u = vec![e[EXPERT], e[PARTISAN]];
    let vectors = vec![onehot(0, 2), onehot(1, 2)];
    let w = review_weights(
        &[false, false],
        &[N_PROBATION; 2],
        &e_u,
        5.0,
        &vectors,
        CORR,
    );
    let p = aggregate_pass_probability(&[0.9, 0.1], &w).unwrap();
    assert!(
        resolve_band(p, DECISION_THRESHOLD),
        "expert should carry it (p={p})"
    );
}

#[test]
fn a_cartel_cannot_flip_a_band_item_the_honest_panel_rejects() {
    // 20 independent expert-calibrated reviewers judge a band item as failing (p=0.2).
    // A 200-strong cartel (peer-followers, correlated) pushes a confident pass (p=0.9).
    let e = profile_e_u();
    let (h, k) = (20usize, 200usize);
    let len = h + 1;

    let mut is_founder = vec![false; h + k];
    let established = vec![N_PROBATION; h + k];
    let mut e_u = Vec::with_capacity(h + k);
    let mut vectors = Vec::with_capacity(h + k);
    let mut probs = Vec::with_capacity(h + k);
    for u in 0..h {
        e_u.push(e[EXPERT]);
        vectors.push(onehot(u + 1, len)); // independent
        probs.push(0.2);
    }
    for _ in 0..k {
        e_u.push(e[FOLLOWS_PEERS]);
        vectors.push(onehot(0, len)); // identical → one cluster
        probs.push(0.9);
    }
    is_founder.truncate(h + k);

    // Discounted: the cartel is √k-shrunk and the honest verdict (reject) holds.
    let w = review_weights(&is_founder, &established, &e_u, 5.0, &vectors, CORR);
    let discounted = aggregate_pass_probability(&probs, &w).unwrap();
    assert!(
        !resolve_band(discounted, DECISION_THRESHOLD),
        "discounted cartel must not flip the band (p={discounted})"
    );

    // Contrast: with raw (un-discounted) weights the same cartel would force a pass.
    let raw: Vec<f64> = e_u.clone(); // established, unit-cap not binding at these E_u
    let raw_p = aggregate_pass_probability(&probs, &raw).unwrap();
    assert!(
        resolve_band(raw_p, DECISION_THRESHOLD),
        "un-discounted, the cartel wins (p={raw_p})"
    );
}

#[test]
fn probationers_do_not_resolve_the_band() {
    // Two established experts reject a band item; any number of probationers voting a
    // confident pass cannot move it, because their weight is zero.
    let e = profile_e_u();
    let base = {
        let w = review_weights(
            &[false, false],
            &[N_PROBATION; 2],
            &[e[EXPERT], e[EXPERT]],
            5.0,
            &[onehot(0, 2), onehot(1, 2)],
            CORR,
        );
        aggregate_pass_probability(&[0.2, 0.2], &w).unwrap()
    };
    assert!(!resolve_band(base, DECISION_THRESHOLD));

    for extra in [1usize, 50, 500] {
        let n = 2 + extra;
        let len = n + 1;
        let mut is_founder = vec![false; n];
        let mut established = vec![0usize; n];
        established[0] = N_PROBATION;
        established[1] = N_PROBATION;
        let e_u = vec![e[EXPERT]; n];
        let mut vectors = vec![onehot(0, len), onehot(1, len)];
        let mut probs = vec![0.2, 0.2];
        for j in 0..extra {
            vectors.push(onehot(2 + j, len));
            probs.push(0.9);
        }
        is_founder.truncate(n);
        established.truncate(n);
        let w = review_weights(&is_founder, &established, &e_u, 5.0, &vectors, CORR);
        let p = aggregate_pass_probability(&probs, &w).unwrap();
        assert!(
            (p - base).abs() < 1e-12,
            "{extra} probationers shifted the band ({p} vs {base})"
        );
    }
}

#[test]
fn an_all_probation_panel_leaves_the_band_undecided() {
    // No eligible weight → the aggregate is None; the band must NOT advance by default.
    let n = 6;
    let len = n + 1;
    let e_u = vec![profile_e_u()[EXPERT]; n];
    let vectors: Vec<Vec<f64>> = (0..n).map(|u| onehot(u, len)).collect();
    let w = review_weights(&vec![false; n], &vec![0usize; n], &e_u, 5.0, &vectors, CORR);
    let resolved = aggregate_pass_probability(&vec![0.9; n], &w)
        .map(|p| resolve_band(p, DECISION_THRESHOLD))
        .unwrap_or(false);
    assert!(!resolved, "an undecided band must not advance by default");
}
