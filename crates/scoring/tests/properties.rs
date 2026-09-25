//! Property tests for the scoring engine (T42, scoring half): invariants that must hold
//! for *any* input, not only the fixture datasets. Bridging runs on small random rating
//! matrices, so each case fits in milliseconds.
//!
//! Not stated as a property: "raising every rating of an item never lowers its `b_j`".
//! The bridging objective can have several local minima of different depth, and a
//! seeded single start may land in either; monotonicity then fails for reasons that are
//! about the start point, not the model (`docs/10` T40).

use proptest::prelude::*;
use scoring::bridging::{bridge_scores, fit, side_balanced, BridgingParams, Obs, Ratings};
use scoring::collusion::{correlation_matrix, discount_weights, sublinear_group_weight};
use scoring::irt::{point_biserial, theta_from_anchors};
use scoring::reputation::{
    author_score, difference_scores, loo_baseline, odds_weight, AuthorPrior, EvaluatorHistory,
};

// ------------------------------ generators ------------------------------

/// A ratings set: `n` reviewers × `m` items, ratings in [0, 1], ~80% observed, and
/// per-reviewer weights in [0.2, 2].
fn ratings() -> impl Strategy<Value = Ratings> {
    (3usize..10, 2usize..6).prop_flat_map(|(n, m)| {
        (
            prop::collection::vec(prop::collection::vec(0.0f64..=1.0, m), n),
            prop::collection::vec(prop::collection::vec(prop::bool::weighted(0.8), m), n),
            prop::collection::vec(0.2f64..2.0, n),
        )
            .prop_map(|(r, mask, w)| Ratings::from_dense(&r, &mask).with_weights(w))
    })
}

fn bits(v: &[f64]) -> Vec<u64> {
    v.iter().map(|x| x.to_bits()).collect()
}

fn same_fit(a: &scoring::bridging::Fit, b: &scoring::bridging::Fit) -> bool {
    a.mu.to_bits() == b.mu.to_bits()
        && bits(&a.b_j) == bits(&b.b_j)
        && bits(&a.f_j) == bits(&b.f_j)
        && bits(&a.b_u) == bits(&b.b_u)
        && bits(&a.f_u) == bits(&b.f_u)
        && a.status == b.status
}

// ------------------------------- bridging -------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Invariant #7: the same input gives the same output, bit for bit.
    #[test]
    fn bridging_is_deterministic(data in ratings()) {
        let p = BridgingParams::default();
        prop_assert!(same_fit(&fit(&data, &p).unwrap(), &fit(&data, &p).unwrap()));
    }

    /// INV-13: the order observations arrive in cannot change the result.
    #[test]
    fn bridging_is_invariant_to_observation_order(data in ratings(), key in any::<u64>()) {
        let mut shuffled = data.clone();
        // A deterministic permutation driven by `key`.
        shuffled.obs.sort_by_key(|o: &Obs| {
            (o.u as u64 ^ key).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (o.j as u64)
        });
        let p = BridgingParams::default();
        prop_assert!(same_fit(&fit(&data, &p).unwrap(), &fit(&shuffled, &p).unwrap()));
    }

    /// Weight 0 (probation) means absent: zeroing a reviewer's weight gives exactly the
    /// fit obtained by deleting their ratings — including the start point (T42).
    #[test]
    fn a_zero_weight_reviewer_is_the_same_as_an_absent_one(
        data in ratings(),
        who in any::<prop::sample::Index>(),
    ) {
        let u = who.index(data.n);
        let mut w = data.weights.clone();
        w[u] = 0.0;
        let zeroed = data.clone().with_weights(w.clone());
        let mut absent = data.clone().with_weights(w);
        absent.obs.retain(|o| o.u != u);
        let p = BridgingParams::default();
        prop_assert!(same_fit(&fit(&zeroed, &p).unwrap(), &fit(&absent, &p).unwrap()));
    }

    /// Finite ratings and weights give finite scores.
    #[test]
    fn bridging_outputs_are_finite(data in ratings()) {
        let f = fit(&data, &BridgingParams::default()).unwrap();
        prop_assert!(f.mu.is_finite());
        for v in f.b_j.iter().chain(&f.f_j).chain(&f.b_u).chain(&f.f_u) {
            prop_assert!(v.is_finite());
        }
    }

    /// `f` is returned in a canonical sign (T48): its largest-magnitude item loading is
    /// non-negative, so draws that order reviewers by `f_u` do not flip with the start.
    #[test]
    fn the_latent_axis_has_a_canonical_sign(data in ratings()) {
        let f = fit(&data, &BridgingParams::default()).unwrap();
        let lead = f.f_j.iter().copied().fold(0.0_f64, |b, v| if v.abs() > b.abs() { v } else { b });
        prop_assert!(lead >= 0.0, "leading f_j = {lead}");
    }

    /// The robust score is pessimistic: never above the full-data fit's side-balanced
    /// score, which travels with it.
    #[test]
    fn the_bootstrap_minimum_never_exceeds_the_full_fit(data in ratings()) {
        let p = BridgingParams::default();
        let full = side_balanced(&fit(&data, &p).unwrap());
        let bridge = bridge_scores(&data, &p, 5, 0.85).unwrap();
        prop_assert_eq!(&bridge.full, &full);
        for (j, (b, f)) in bridge.robust.iter().zip(&full.score).enumerate() {
            prop_assert!(b <= f, "item {j}: bootstrap {b} > full {f}");
        }
    }

    /// The side-balanced score is symmetric in the sign of `f` (D32): negating the axis
    /// swaps the two sides and leaves the score and the gap bit for bit (T49).
    #[test]
    fn the_side_balanced_score_is_symmetric_in_the_sign_of_f(data in ratings()) {
        let f = fit(&data, &BridgingParams::default()).unwrap();
        let mut flipped = f.clone();
        for v in flipped.f_u.iter_mut().chain(flipped.f_j.iter_mut()) {
            *v = -*v;
        }
        let (a, b) = (side_balanced(&f), side_balanced(&flipped));
        prop_assert_eq!(bits(&a.score), bits(&b.score));
        prop_assert_eq!(bits(&a.gap), bits(&b.gap));
        prop_assert_eq!(bits(&a.side_a), bits(&b.side_b));
        prop_assert_eq!(bits(&a.side_b), bits(&b.side_a));
    }
}

// ------------------------------- Level C -------------------------------

proptest! {
    /// `C_a` is a posterior mean with a Beta(2, 3) prior: always strictly inside (0, 1)
    /// for qualities in [0, 1], and never lowered by a better item.
    #[test]
    fn author_score_is_a_probability_and_rewards_quality(
        items in prop::collection::vec((0.0f64..=1.0, 0.0f64..120.0), 0..12),
        which in any::<prop::sample::Index>(),
    ) {
        let p = AuthorPrior::default();
        let (q, age): (Vec<f64>, Vec<f64>) = items.iter().copied().unzip();
        let c = author_score(&q, &age, &p);
        prop_assert!(c > 0.0 && c < 1.0, "C_a = {c}");
        if !q.is_empty() {
            let mut better = q.clone();
            let j = which.index(q.len());
            better[j] = 1.0;
            prop_assert!(author_score(&better, &age, &p) >= c);
        }
    }

    /// The leave-one-out crowd is a weighted mean of the *other* panelists' forecasts: it
    /// stays within their range and does not move with the reviewer's own forecast.
    #[test]
    fn loo_baseline_stays_within_the_others_predictions(
        rows in (2usize..8, 1usize..5).prop_flat_map(|(n, m)| (
            prop::collection::vec(prop::collection::vec(0.0f64..=1.0, m), n),
            prop::collection::vec(0.0f64..3.0, n),
        )),
    ) {
        let (preds, w) = rows;
        for u in 0..preds.len() {
            let crowd = loo_baseline(&preds, &w, u);
            for (j, b) in crowd.iter().enumerate() {
                let others = preds.iter().enumerate().filter(|(v, _)| *v != u).map(|(_, p)| p[j]);
                let lo = others.clone().fold(f64::INFINITY, f64::min);
                let hi = others.fold(f64::NEG_INFINITY, f64::max);
                prop_assert!(*b >= lo - 1e-12 && *b <= hi + 1e-12, "item {j}: {b} not in [{lo}, {hi}]");
            }
            let mut moved = preds.clone();
            for x in moved[u].iter_mut() {
                *x = 1.0 - *x;
            }
            prop_assert_eq!(loo_baseline(&moved, &w, u), crowd);
        }
    }

    /// The difference score is bounded in [−1, 1], a panelist who copies the others'
    /// weighted mean scores exactly 0, and the odds weight is positive and finite.
    #[test]
    fn difference_score_is_bounded_and_zero_for_the_crowd(
        rows in (2usize..7, 1usize..6).prop_flat_map(|(n, m)| (
            prop::collection::vec(prop::collection::vec(0.0f64..=1.0, m), n),
            prop::collection::vec(0.0f64..3.0, n),
            prop::collection::vec(prop::bool::ANY, m),
        )),
    ) {
        let (mut preds, mut w, o) = rows;
        let o: Vec<f64> = o.iter().map(|&b| b as i32 as f64).collect();
        let n = preds.len();
        preds.push(loo_baseline(&preds, &w, n));
        w.push(1.0);
        let scores = difference_scores(&preds, &w, &o);
        for s in scores.iter().flatten() {
            prop_assert!((-1.0 - 1e-12..=1.0 + 1e-12).contains(s), "S = {s}");
        }
        prop_assert!(scores[n].iter().all(|&s| s == 0.0), "copier: {:?}", scores[n]);
        let mean = scores[0].iter().sum::<f64>() / scores[0].len() as f64;
        let weight = odds_weight(mean, o.len(), 35.0, 100.0);
        prop_assert!(weight.is_finite() && weight > 0.0, "w = {weight}");
    }

    /// The history's mean stays within the scores recorded since its last restart, the
    /// CUSUM statistic is never negative, and an alarm restarts the record and is counted.
    #[test]
    fn evaluator_history_mean_is_within_its_scores_and_alarms_restart_the_record(
        scores in prop::collection::vec(-1.0f64..=1.0, 1..60),
        k in 0.0f64..0.1, h in 0.1f64..2.0,
    ) {
        let mut history = EvaluatorHistory::new();
        let mut since_restart: Vec<f64> = Vec::new();
        let mut alarms = 0;
        for &x in &scores {
            if history.record(x, k, h) {
                alarms += 1;
                since_restart.clear();
                prop_assert_eq!((history.scored(), history.cusum()), (0, 0.0));
            } else {
                since_restart.push(x);
            }
            prop_assert!(history.cusum() >= 0.0);
            prop_assert_eq!(history.scored(), since_restart.len());
            prop_assert_eq!(history.alarms(), alarms);
            if !since_restart.is_empty() {
                let lo = since_restart.iter().cloned().fold(f64::INFINITY, f64::min);
                let hi = since_restart.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                prop_assert!(history.score() >= lo - 1e-12 && history.score() <= hi + 1e-12);
            }
        }
    }
}

// ---------------------------- anti-collusion ----------------------------

proptest! {
    /// A correlation matrix is symmetric, has a unit diagonal and entries in [−1, 1].
    #[test]
    fn correlation_matrix_is_a_correlation_matrix(
        rows in (2usize..7, 2usize..10).prop_flat_map(|(n, m)|
            prop::collection::vec(prop::collection::vec(-1.0f64..1.0, m), n)),
    ) {
        let c = correlation_matrix(&rows);
        for (i, row) in c.iter().enumerate() {
            prop_assert_eq!(row[i], 1.0);
            for (j, &v) in row.iter().enumerate() {
                prop_assert!((-1.0 - 1e-12..=1.0 + 1e-12).contains(&v), "c[{i}][{j}] = {v}");
                prop_assert_eq!(v.to_bits(), c[j][i].to_bits());
            }
        }
    }

    /// INV-14: the cluster discount never raises any weight and never makes one negative;
    /// a cluster's total never exceeds its raw total.
    #[test]
    fn the_cluster_discount_is_never_a_boost(
        members in prop::collection::vec((0.0f64..5.0, 0usize..4), 1..20),
        alpha in 0.1f64..=1.0,
    ) {
        let (w, ids): (Vec<f64>, Vec<usize>) = members.iter().copied().unzip();
        let d = discount_weights(&w, &ids, alpha);
        for i in 0..w.len() {
            prop_assert!(d[i] >= 0.0 && d[i] <= w[i] + 1e-12, "node {i}: {} -> {}", w[i], d[i]);
        }
        prop_assert!(sublinear_group_weight(&w, alpha) <= w.iter().sum::<f64>() + 1e-12);
    }
}

// --------------------------------- IRT ---------------------------------

proptest! {
    /// θ is a standardized score: mean 0 and variance 1, or all 0 when there is no spread.
    #[test]
    fn theta_is_standardized(
        anchors in (1usize..60, 1usize..6).prop_flat_map(|(n, k)|
            prop::collection::vec(prop::collection::vec(prop::bool::ANY, k), n)),
    ) {
        let rows: Vec<Vec<f64>> = anchors
            .iter()
            .map(|r| r.iter().map(|&b| b as i32 as f64).collect())
            .collect();
        let theta = theta_from_anchors(&rows);
        let n = theta.len() as f64;
        let mean = theta.iter().sum::<f64>() / n;
        let var = theta.iter().map(|t| (t - mean).powi(2)).sum::<f64>() / n;
        prop_assert!(mean.abs() < 1e-9, "mean {mean}");
        prop_assert!(var.abs() < 1e-9 || (var - 1.0).abs() < 1e-9, "var {var}");
    }

    /// The point-biserial is a correlation: in [−1, 1], unchanged by a positive affine
    /// rescaling of the total, and sign-flipped by a negative one.
    #[test]
    fn point_biserial_is_a_scale_free_correlation(
        pairs in prop::collection::vec((prop::bool::ANY, -5.0f64..5.0), 2..40),
        a in 0.1f64..10.0,
        b in -10.0f64..10.0,
    ) {
        let item: Vec<f64> = pairs.iter().map(|p| p.0 as i32 as f64).collect();
        let total: Vec<f64> = pairs.iter().map(|p| p.1).collect();
        let r = point_biserial(&item, &total);
        prop_assert!((-1.0 - 1e-12..=1.0 + 1e-12).contains(&r), "r = {r}");
        let scaled: Vec<f64> = total.iter().map(|t| a * t + b).collect();
        let flipped: Vec<f64> = total.iter().map(|t| -a * t + b).collect();
        prop_assert!((point_biserial(&item, &scaled) - r).abs() < 1e-9);
        prop_assert!((point_biserial(&item, &flipped) + r).abs() < 1e-9);
    }
}
