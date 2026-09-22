//! Review aggregation (`docs/05` [4]/[5], `docs/02` §C.2): the composed decision that
//! weights reviewer judgments by the evaluator score `E_u` (probation-gated) and
//! discounts coordinated blocks √k. These tests are the real proving ground — diverse
//! scenarios, edge cases, and property-based invariants — for the two rules this piece
//! must never break: quality is not a majority vote (#2), and the two reputation scores
//! are never merged (#4).

use proptest::prelude::*;
use protocol::aggregate::{
    aggregate_pass_probability, resolve_band, review_weights, DECISION_THRESHOLD,
};
use protocol::honeypot::reviewer_skills;
use protocol::probation::{effective_review_weight, N_PROBATION};
use scoring::collusion::{discount_weights, ALPHA};
use scoring::reputation::evaluator_score;

const CORR: f64 = 0.9;

/// One-hot judgment vector of length `len` — distinct positions are near-uncorrelated
/// (`ρ = -1/(len-1)`), so honest reviewers stay singletons; identical vectors give
/// `ρ = 1` and cluster (used to model a cartel).
fn onehot(pos: usize, len: usize) -> Vec<f64> {
    let mut v = vec![0.0; len];
    v[pos] = 1.0;
    v
}

/// Build a panel of `h` independent honest established reviewers plus a `k`-strong
/// perfectly-correlated cartel. Honest vote `p_honest`, cartel vote `p_cartel`. All
/// established with unit E_u and unit cap. Returns the aggregate pass-probability.
fn honest_vs_cartel(h: usize, k: usize, p_honest: f64, p_cartel: f64) -> Option<f64> {
    let n = h + k;
    let len = h + 1; // positions: 0 = cartel, 1..=h = honest
    let is_founder = vec![false; n];
    let established = vec![N_PROBATION; n];
    let e_u = vec![1.0; n];

    let mut vectors = Vec::with_capacity(n);
    let mut probs = Vec::with_capacity(n);
    for u in 0..h {
        vectors.push(onehot(u + 1, len));
        probs.push(p_honest);
    }
    for _ in 0..k {
        vectors.push(onehot(0, len)); // every cartel member: the same vector
        probs.push(p_cartel);
    }

    let w = review_weights(&is_founder, &established, &e_u, 1.0, &vectors, CORR);
    aggregate_pass_probability(&probs, &w)
}

// ------------------------------- (a) scenarios -------------------------------

#[test]
fn cartel_influence_grows_like_sqrt_k_but_never_flips_the_honest_verdict() {
    // 40 honest reviewers judge a mediocre item as failing (p = 0.2). A cartel of
    // growing size votes a confident pass (p = 1.0). Its pull rises with √k but the
    // discounted weight never overturns the honest signal.
    let h = 40;
    let mut last = 0.0;
    for &k in &[4usize, 25, 100, 400] {
        let p = honest_vs_cartel(h, k, 0.2, 1.0).unwrap();
        assert!(p > last, "influence should grow with k (k={k}, p={p})");
        assert!(
            !resolve_band(p, DECISION_THRESHOLD),
            "cartel of {k} must not flip the verdict (p={p})"
        );
        last = p;
    }
}

#[test]
fn without_the_discount_the_same_cartel_would_win() {
    // Contrast: with raw (un-discounted) weights, a 400-strong cartel swamps 40 honest
    // reviewers and forces a pass — which is exactly what the discount prevents above.
    let (h, k) = (40usize, 400usize);
    let mut probs = vec![0.2; h];
    probs.extend(vec![1.0; k]);
    let raw_weights = vec![1.0; h + k]; // no clustering / no discount
    let p = aggregate_pass_probability(&probs, &raw_weights).unwrap();
    assert!(
        resolve_band(p, DECISION_THRESHOLD),
        "un-discounted, the cartel wins (p={p})"
    );
}

#[test]
fn probationers_never_move_the_outcome() {
    // A verdict set by two established reviewers (p = 0.2) is unmoved by any number of
    // probationers voting a confident pass, because their weight is 0.
    let baseline = {
        let is_founder = vec![false, false];
        let established = vec![N_PROBATION, N_PROBATION];
        let e_u = vec![1.0, 1.0];
        let vectors = vec![onehot(1, 3), onehot(2, 3)];
        let w = review_weights(&is_founder, &established, &e_u, 1.0, &vectors, CORR);
        aggregate_pass_probability(&[0.2, 0.2], &w).unwrap()
    };

    for extra in [1usize, 10, 500] {
        let n = 2 + extra;
        let mut is_founder = vec![false; n];
        let mut established = vec![0usize; n]; // probationers: 0 outcomes
        established[0] = N_PROBATION;
        established[1] = N_PROBATION;
        let e_u = vec![1.0; n];
        let mut vectors = vec![onehot(1, n + 1), onehot(2, n + 1)];
        let mut probs = vec![0.2, 0.2];
        for j in 0..extra {
            vectors.push(onehot(3 + j, n + 1));
            probs.push(1.0);
        }
        is_founder.truncate(n);
        established.truncate(n);
        let w = review_weights(&is_founder, &established, &e_u, 1.0, &vectors, CORR);
        let p = aggregate_pass_probability(&probs, &w).unwrap();
        assert!(
            (p - baseline).abs() < 1e-12,
            "{extra} probationers shifted the outcome ({p} vs {baseline})"
        );
    }
}

#[test]
fn a_few_high_skill_reviewers_outweigh_a_low_skill_majority() {
    // NOT a head-count: 3 high-E_u reviewers voting pass beat 50 low-E_u reviewers
    // voting fail. w_max is generous so E_u drives the weight.
    let n = 53;
    let is_founder = vec![false; n];
    let established = vec![N_PROBATION; n];
    let mut e_u = vec![0.1; n]; // the low-skill majority
    for slot in e_u.iter_mut().take(3) {
        *slot = 5.0; // the three experts
    }
    let vectors: Vec<Vec<f64>> = (0..n).map(|u| onehot(u, n)).collect();
    let mut probs = vec![0.9; 3];
    probs.extend(vec![0.1; n - 3]);

    let w = review_weights(&is_founder, &established, &e_u, 5.0, &vectors, CORR);
    let p = aggregate_pass_probability(&probs, &w).unwrap();
    assert!(
        resolve_band(p, DECISION_THRESHOLD),
        "the experts should carry the verdict (p={p})"
    );
}

#[test]
fn founders_seed_the_bootstrap_then_yield_to_evaluator_score() {
    // At bootstrap a founder carries weight 1 while a not-yet-established node carries
    // 0; once past N_PROBATION the weight becomes the capped E_u.
    assert_eq!(effective_review_weight(true, 0, 0.0, 3.0), 1.0);
    assert_eq!(effective_review_weight(false, 0, 9.9, 3.0), 0.0);
    assert_eq!(effective_review_weight(false, N_PROBATION, 2.0, 3.0), 2.0);
    // the cap bites:
    assert_eq!(effective_review_weight(false, N_PROBATION, 9.9, 3.0), 3.0);
}

#[test]
fn honeypot_skill_drives_the_weights_and_the_verdict() {
    // Golden items give ground truth. A reviewer who predicts them well earns a high
    // E_u; a coin-flipper earns ~0. Feeding those E_u into the aggregation lets the
    // skilled reviewer's vote decide.
    let outcomes = vec![1.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0];
    let sharp: Vec<f64> = outcomes
        .iter()
        .map(|&o| if o > 0.5 { 0.95 } else { 0.05 })
        .collect();
    let coinflip = vec![0.5; outcomes.len()];

    // Score both against the crowd baseline (the panel is sharp + coinflip).
    let skills = reviewer_skills(&[sharp, coinflip], &[1.0, 1.0], &outcomes);
    let e_sharp = evaluator_score(skills[0], 1.0);
    let e_flip = evaluator_score(skills[1], 1.0);
    assert!(e_sharp > e_flip, "a sharp predictor must earn more E_u");

    // Two established reviewers disagree on a new item; the sharp one votes pass.
    let is_founder = vec![false, false];
    let established = vec![N_PROBATION, N_PROBATION];
    let e_u = vec![e_sharp, e_flip];
    let vectors = vec![onehot(0, 2), onehot(1, 2)];
    let w = review_weights(&is_founder, &established, &e_u, 5.0, &vectors, CORR);
    let p = aggregate_pass_probability(&[0.9, 0.2], &w).unwrap();
    assert!(
        resolve_band(p, DECISION_THRESHOLD),
        "the higher-skill reviewer should carry it (p={p})"
    );
}

// ----------------------------- (b) edge cases -----------------------------

#[test]
fn empty_panel_is_undecided() {
    let w = review_weights(&[], &[], &[], 1.0, &[], CORR);
    assert!(w.is_empty());
    assert_eq!(aggregate_pass_probability(&[], &w), None);
}

#[test]
fn all_probation_is_undecided_not_a_default_pass() {
    let n = 5;
    let is_founder = vec![false; n];
    let established = vec![0usize; n]; // nobody has a track record
    let e_u = vec![1.0; n];
    let vectors: Vec<Vec<f64>> = (0..n).map(|u| onehot(u, n)).collect();
    let w = review_weights(&is_founder, &established, &e_u, 1.0, &vectors, CORR);
    assert!(w.iter().all(|&x| x == 0.0));
    assert_eq!(aggregate_pass_probability(&vec![1.0; n], &w), None);
}

#[test]
fn a_single_reviewer_decides_alone() {
    let w = review_weights(
        &[false],
        &[N_PROBATION],
        &[1.0],
        1.0,
        &[vec![1.0, 0.0]],
        CORR,
    );
    assert_eq!(aggregate_pass_probability(&[0.9], &w), Some(0.9));
    assert_eq!(aggregate_pass_probability(&[0.1], &w), Some(0.1));
}

#[test]
fn a_tie_at_the_threshold_resolves_in_favour() {
    // Documented tie-break: exactly at the threshold advances.
    assert!(resolve_band(0.5, DECISION_THRESHOLD));
    let w = vec![1.0, 1.0];
    let p = aggregate_pass_probability(&[0.5, 0.5], &w).unwrap();
    assert!(resolve_band(p, DECISION_THRESHOLD));
}

#[test]
fn constant_judgment_vectors_do_not_cluster_spuriously() {
    // A constant vector has undefined correlation (treated as 0), so reviewers who
    // happen to vote identically across the shared items are NOT fused into a cartel:
    // each keeps its own unit weight.
    let n = 4;
    let is_founder = vec![false; n];
    let established = vec![N_PROBATION; n];
    let e_u = vec![1.0; n];
    let vectors = vec![vec![3.0; 5]; n]; // all identical AND constant
    let w = review_weights(&is_founder, &established, &e_u, 1.0, &vectors, CORR);
    assert!(
        w.iter().all(|&x| (x - 1.0).abs() < 1e-12),
        "no spurious discount: {w:?}"
    );
}

#[test]
fn a_tiny_cap_binds_established_weights() {
    let w = review_weights(
        &[false],
        &[N_PROBATION],
        &[9.9],
        0.25,
        &[vec![1.0, 0.0]],
        CORR,
    );
    assert!((w[0] - 0.25).abs() < 1e-12, "cap should bind: {w:?}");
}

// -------------------------- (c) property-based --------------------------

fn probs_and_weights() -> impl Strategy<Value = (Vec<f64>, Vec<f64>)> {
    (1usize..12).prop_flat_map(|n| {
        (
            prop::collection::vec(0.0f64..=1.0, n),
            prop::collection::vec(0.0f64..=10.0, n),
        )
    })
}

proptest! {
    /// The aggregate is always a probability in [0,1], or None when no weight applies.
    #[test]
    fn aggregate_is_a_probability_or_none((probs, weights) in probs_and_weights()) {
        match aggregate_pass_probability(&probs, &weights) {
            Some(p) => prop_assert!((0.0..=1.0).contains(&p), "p={p} out of range"),
            None => prop_assert!(weights.iter().sum::<f64>() <= 0.0),
        }
    }

    /// Raising one reviewer's probability (with positive weight) never lowers the aggregate.
    #[test]
    fn monotone_in_each_probability(
        (probs, weights) in probs_and_weights(),
        idx in any::<prop::sample::Index>(),
        bump in 0.0f64..=1.0,
    ) {
        let i = idx.index(probs.len());
        let before = aggregate_pass_probability(&probs, &weights);
        let mut raised = probs.clone();
        raised[i] = (raised[i] + bump).min(1.0);
        let after = aggregate_pass_probability(&raised, &weights);
        if let (Some(b), Some(a)) = (before, after) {
            prop_assert!(a >= b - 1e-9, "monotonicity: {a} < {b}");
        }
    }

    /// Appending zero-weight (probation) reviewers never changes the outcome.
    #[test]
    fn zero_weight_reviewers_are_irrelevant(
        (probs, weights) in probs_and_weights(),
        extra in prop::collection::vec(0.0f64..=1.0, 0..6),
    ) {
        let base = aggregate_pass_probability(&probs, &weights);
        let mut p2 = probs.clone();
        let mut w2 = weights.clone();
        for e in extra {
            p2.push(e);
            w2.push(0.0);
        }
        prop_assert_eq!(aggregate_pass_probability(&p2, &w2), base);
    }

    /// A coordinated block of k unit-weight clones is discounted to √k total (≤ k for
    /// k ≥ 1): coordination is strictly non-expansive.
    #[test]
    fn a_cartel_of_k_counts_like_sqrt_k(k in 1usize..500) {
        let weights = vec![1.0; k];
        let one_cluster = vec![0usize; k];
        let discounted = discount_weights(&weights, &one_cluster, ALPHA);
        let total: f64 = discounted.iter().sum();
        prop_assert!((total - (k as f64).sqrt()).abs() < 1e-6, "k={k} total={total}");
        prop_assert!(total <= k as f64 + 1e-9);
    }

    /// The composed aggregation is deterministic.
    #[test]
    fn aggregation_is_deterministic((probs, weights) in probs_and_weights()) {
        prop_assert_eq!(
            aggregate_pass_probability(&probs, &weights),
            aggregate_pass_probability(&probs, &weights)
        );
    }
}

// ------------------- (d) documented limitations (docs/08 PROTO-012) -------------------
//
// These tests pin what the band resolution *is*, so that nobody mistakes it for the
// bridging property. They are not guarantees; when the D26 mechanism (more reviewers,
// clean re-decision of the bridging score) replaces this tie-break, they should fail
// and be deleted.

#[test]
fn documents_limitation_a_polarized_band_item_is_resolved_by_the_larger_camp() {
    // 120 established, independent, unit-E_u reviewers from the majority camp say 0.9;
    // 80 from the minority camp say 0.3. There is no cartel (every vector distinct),
    // so no discount applies, and the weighted mean is 0.66 ≥ 0.5: the larger camp
    // decides. Bridging would explain this pattern by the axis; the tie-break cannot.
    let (maj, min_) = (120usize, 80usize);
    let n = maj + min_;
    let vectors: Vec<Vec<f64>> = (0..n).map(|u| onehot(u, n)).collect();
    let probs: Vec<f64> = (0..n).map(|u| if u < maj { 0.9 } else { 0.3 }).collect();
    let w = review_weights(
        &vec![false; n],
        &vec![N_PROBATION; n],
        &vec![1.0; n],
        1.0,
        &vectors,
        CORR,
    );
    let p = aggregate_pass_probability(&probs, &w).unwrap();
    assert!((p - 0.66).abs() < 1e-9, "p = {p}");
    assert!(
        resolve_band(p, DECISION_THRESHOLD),
        "the larger camp alone resolves the band in favour (weighted majority)"
    );
}

#[test]
fn documents_limitation_sqrt_k_holds_the_line_only_up_to_a_bounded_cartel() {
    // With √k, 40 honest reviewers at 0.2 outweigh an exact-copy cartel at 1.0 only
    // while (8 + √k)/(40 + √k) < 0.5, i.e. √k < 24, k < 576. At k = 576 the weighted
    // mean reaches exactly 0.5 and the verdict flips. "Never flips" in the scenario
    // tests above is a statement about k ≤ 400 — and about exact copies: a cartel that
    // jitters its votes (σ ≈ 0.05) is not clustered at all (docs/08 COLLUSION-002) and
    // flips the same panel with k ≈ 65.
    let p_575 = honest_vs_cartel(40, 575, 0.2, 1.0).unwrap();
    let p_576 = honest_vs_cartel(40, 576, 0.2, 1.0).unwrap();
    assert!(!resolve_band(p_575, DECISION_THRESHOLD), "p = {p_575}");
    assert!(resolve_band(p_576, DECISION_THRESHOLD), "p = {p_576}");
}
