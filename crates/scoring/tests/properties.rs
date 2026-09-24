//! Property tests for the scoring engine (T42, scoring half): invariants that must hold
//! for *any* input, not only the fixture datasets. Bridging runs on small random rating
//! matrices, so each case fits in milliseconds.
//!
//! Not stated as a property: "raising every rating of an item never lowers its `b_j`".
//! The bridging objective can have several local minima of different depth, and a
//! seeded single start may land in either; monotonicity then fails for reasons that are
//! about the start point, not the model (`docs/10` T40).

use proptest::prelude::*;
use scoring::bridging::{bridge_scores, fit, BridgingParams, Obs, Ratings};
use scoring::collusion::{correlation_matrix, discount_weights, sublinear_group_weight};
use scoring::irt::{point_biserial, theta_from_anchors};
use scoring::reputation::{
    asymmetric_ema, author_score, brier_skill_score, crowd_baseline, evaluator_score, AuthorPrior,
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
        prop_assert!(same_fit(&fit(&data, &p), &fit(&data, &p)));
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
        prop_assert!(same_fit(&fit(&data, &p), &fit(&shuffled, &p)));
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
        prop_assert!(same_fit(&fit(&zeroed, &p), &fit(&absent, &p)));
    }

    /// Finite ratings and weights give finite scores.
    #[test]
    fn bridging_outputs_are_finite(data in ratings()) {
        let f = fit(&data, &BridgingParams::default());
        prop_assert!(f.mu.is_finite());
        for v in f.b_j.iter().chain(&f.f_j).chain(&f.b_u).chain(&f.f_u) {
            prop_assert!(v.is_finite());
        }
    }

    /// `f` is returned in a canonical sign (T48): its largest-magnitude item loading is
    /// non-negative, so draws that order reviewers by `f_u` do not flip with the start.
    #[test]
    fn the_latent_axis_has_a_canonical_sign(data in ratings()) {
        let f = fit(&data, &BridgingParams::default());
        let lead = f.f_j.iter().copied().fold(0.0_f64, |b, v| if v.abs() > b.abs() { v } else { b });
        prop_assert!(lead >= 0.0, "leading f_j = {lead}");
    }

    /// The robust score is pessimistic: never above the full-data fit.
    #[test]
    fn the_bootstrap_minimum_never_exceeds_the_full_fit(data in ratings()) {
        let p = BridgingParams::default();
        let full = fit(&data, &p);
        let robust = bridge_scores(&data, &p, 5, 0.85);
        for (j, (b, f)) in robust.iter().zip(&full.b_j).enumerate() {
            prop_assert!(b <= f, "item {j}: bootstrap {b} > full {f}");
        }
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

    /// The crowd baseline is a weighted mean: it stays within the panel's predictions.
    #[test]
    fn crowd_baseline_stays_within_the_predictions(
        rows in (1usize..8, 1usize..5).prop_flat_map(|(n, m)| (
            prop::collection::vec(prop::collection::vec(0.0f64..=1.0, m), n),
            prop::collection::vec(0.0f64..3.0, n),
        )),
    ) {
        let (preds, w) = rows;
        prop_assume!(w.iter().sum::<f64>() > 0.0);
        for (j, b) in crowd_baseline(&preds, &w).iter().enumerate() {
            let lo = preds.iter().map(|p| p[j]).fold(f64::INFINITY, f64::min);
            let hi = preds.iter().map(|p| p[j]).fold(f64::NEG_INFINITY, f64::max);
            prop_assert!(*b >= lo - 1e-12 && *b <= hi + 1e-12, "item {j}: {b} not in [{lo}, {hi}]");
        }
    }

    /// Skill is capped at 1 (a perfect predictor) and a predictor equal to the baseline
    /// scores exactly 0; the evaluator score maps any skill into [0, 1]. BSS is unbounded
    /// below — a poor prediction against a near-perfect baseline scores in the thousands
    /// below zero — so `σ(γ·BSS)` can underflow to exactly 0: weight zero, as intended.
    #[test]
    fn brier_skill_is_at_most_one_and_zero_for_the_baseline(
        cases in prop::collection::vec((0.0f64..=1.0, 0.0f64..=1.0, prop::bool::ANY), 1..20),
    ) {
        let p: Vec<f64> = cases.iter().map(|c| c.0).collect();
        let base: Vec<f64> = cases.iter().map(|c| c.1).collect();
        let o: Vec<f64> = cases.iter().map(|c| c.2 as i32 as f64).collect();
        let bss = brier_skill_score(&p, &o, &base);
        prop_assert!(bss <= 1.0 + 1e-12, "BSS = {bss}");
        prop_assert_eq!(brier_skill_score(&base, &o, &base), 0.0);
        let e = evaluator_score(bss, 3.0);
        prop_assert!((0.0..=1.0).contains(&e), "E = {e}");
    }

    /// The asymmetric update moves toward the new value without overshooting it.
    #[test]
    fn asymmetric_ema_moves_between_old_and_new(
        prev in 0.0f64..=1.0, new in 0.0f64..=1.0, up in 0.0f64..=1.0, down in 0.0f64..=1.0,
    ) {
        let next = asymmetric_ema(prev, new, up, down);
        prop_assert!(next >= prev.min(new) - 1e-12 && next <= prev.max(new) + 1e-12);
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
