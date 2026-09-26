//! The generators draw the populations `docs/13` §3 defines: the paper's batches, the
//! design's class shares and attackers, and the bridging designs.

use characterization::generate::{dif_batch, expected_clamped, fixture, sweep_data};
use characterization::grid::{Attack, DifDesign, Layout, SweepDesign};
use scoring::irt::kr20;

fn design(n: usize, anchors: usize, k: usize, layout: Layout) -> DifDesign {
    DifDesign {
        n,
        anchors,
        k,
        layout,
        ..DifDesign::default()
    }
}

fn share(rows: &[Vec<f64>], j: usize, keep: impl Fn(usize) -> bool) -> f64 {
    let kept: Vec<f64> = (0..rows.len())
        .filter(|&i| keep(i))
        .map(|i| rows[i][j])
        .collect();
    kept.iter().sum::<f64>() / kept.len() as f64
}

/// The anchors' KR-20 follows the paper's table for 10, 20, 30 and 60 anchors (T53).
#[test]
fn the_anchors_reach_the_paper_s_kr20() {
    for (anchors, paper) in [(10, 0.694), (20, 0.822), (30, 0.874), (60, 0.932)] {
        let mean = (0..4)
            .map(|s| kr20(&dif_batch(&design(6000, anchors, 8, Layout::Campaign(0)), s).anchors))
            .sum::<f64>()
            / 4.0;
        assert!(
            (mean - paper).abs() < 0.03,
            "{anchors} anchors: {mean:.3} vs {paper}"
        );
    }
}

/// The class share is π, a biased item is harder for class +1 by about 2δ, a clean one is not.
#[test]
fn a_biased_item_separates_the_classes_and_a_clean_one_does_not() {
    let d = DifDesign {
        delta: 0.9,
        pi: 0.3,
        ..design(6000, 20, 4, Layout::Campaign(1))
    };
    let batch = dif_batch(&d, 11);
    assert_eq!(batch.roles, "+ccc");
    let plus = batch.z.iter().filter(|&&z| z > 0.0).count() as f64 / 6000.0;
    assert!((plus - 0.3).abs() < 0.02, "share of class +1: {plus}");
    let gap = |j: usize| {
        share(&batch.x, j, |i| batch.z[i] < 0.0) - share(&batch.x, j, |i| batch.z[i] > 0.0)
    };
    assert!(gap(0) > 0.25, "biased item: {}", gap(0));
    assert!(gap(1).abs() < 0.05 && gap(2).abs() < 0.05, "clean items");
}

/// Impact shifts class +1's ability; guessing puts a floor under every item.
#[test]
fn impact_and_guessing_change_the_population_as_designed() {
    let d = DifDesign {
        impact: 1.0,
        guess: 0.2,
        ..design(6000, 20, 4, Layout::Campaign(0))
    };
    let batch = dif_batch(&d, 12);
    let total = |i: usize| batch.anchors[i].iter().sum::<f64>();
    let mean = |plus: bool| {
        let rows: Vec<f64> = (0..6000)
            .filter(|&i| (batch.z[i] > 0.0) == plus)
            .map(total)
            .collect();
        rows.iter().sum::<f64>() / rows.len() as f64
    };
    assert!(
        mean(true) > mean(false) + 2.0,
        "{} vs {}",
        mean(true),
        mean(false)
    );
    let lowest = (0..20)
        .map(|j| share(&batch.anchors, j, |_| true))
        .fold(1.0_f64, f64::min);
    assert!(lowest > 0.2, "an anchor below the guessing floor: {lowest}");
}

/// Injecting attackers answer the targets wrong; masking ones answer the biased items as if clean.
#[test]
fn the_attackers_are_the_last_rows_and_act_as_designed() {
    let inject = DifDesign {
        attack: Attack::Inject {
            fraction: 0.05,
            targets: 2,
        },
        ..design(2000, 10, 6, Layout::Campaign(0))
    };
    let batch = dif_batch(&inject, 13);
    assert_eq!(batch.roles, "cccctt");
    assert!(batch.x[1900..]
        .iter()
        .all(|row| row[4] == 0.0 && row[5] == 0.0));
    assert!(share(&batch.x, 4, |i| i < 1900) > 0.2);

    let mask = DifDesign {
        delta: 3.0,
        attack: Attack::Mask { fraction: 0.5 },
        ..design(4000, 10, 4, Layout::Campaign(1))
    };
    let batch = dif_batch(&mask, 14);
    let gap = |from: usize, to: usize| {
        share(&batch.x, 0, |i| (from..to).contains(&i) && batch.z[i] < 0.0)
            - share(&batch.x, 0, |i| (from..to).contains(&i) && batch.z[i] > 0.0)
    };
    assert!(gap(0, 2000) > 0.5, "honest rows: {}", gap(0, 2000));
    assert!(
        gap(2000, 4000).abs() < 0.06,
        "masking rows: {}",
        gap(2000, 4000)
    );
}

/// Mirror and two-axis layouts place their leaners as specified.
#[test]
fn the_layouts_place_their_leaners() {
    let mirror = dif_batch(&design(100, 5, 8, Layout::Mirror), 1);
    assert_eq!(mirror.roles, "+-+-cccc");
    assert_eq!(mirror.signs, vec![1.0, -1.0, 1.0, -1.0, 0.0, 0.0, 0.0, 0.0]);
    let two = dif_batch(&design(100, 5, 8, Layout::TwoAxes(2)), 1);
    assert_eq!(two.roles, "++22cccc");
}

/// The bridging design has its camps, its mirror pairs and its ratings per reviewer.
#[test]
fn the_bridging_design_has_its_camps_pairs_and_ratings() {
    let d = SweepDesign {
        n: 200,
        share: 0.6,
        per_reviewer: 9,
        noise: 0.07,
    };
    let data = sweep_data(&d, 5);
    assert_eq!((data.ratings.n, data.ratings.m), (200, 20));
    assert_eq!(data.ratings.obs.len(), 200 * 9);
    assert_eq!(data.true_f.iter().filter(|&&f| f > 0.0).count(), 120);
    assert!(data.lean[..10].iter().all(|&l| l == 0.0));
    assert_eq!(&data.lean[10..12], &[0.8, -0.8]);
    assert!(data.q[..10].iter().all(|&q| (0.70..0.95).contains(&q)));
    for j in 0..10 {
        let (q, t) = (data.q[j], data.truth[j]);
        assert!(
            t <= q + 0.01 && t > q - 0.06,
            "consensus item {j}: q {q}, truth {t}"
        );
    }
    assert!(data.truth[10..].iter().all(|&t| (0.45..0.65).contains(&t)));
    let fx = fixture();
    assert_eq!(
        (fx.r.len(), fx.mask.len(), fx.true_f.len()),
        (200, 200, 200)
    );
    assert_eq!(fx.true_f.iter().filter(|&&f| f < 0.0).count(), 80);
}

/// The expected clamped rating matches hand-computed values (a Monte Carlo agrees to 1e-4).
#[test]
fn the_expected_clamped_rating_matches_hand_computed_values() {
    let close = |a: f64, b: f64| (a - b).abs() < 1e-6;
    assert!(close(expected_clamped(0.95, 0.15), 0.911_865));
    assert!(close(expected_clamped(0.5, 0.15), 0.5));
    assert!(close(expected_clamped(0.02, 0.07), 0.039_058));
    assert_eq!(expected_clamped(1.3, 0.0), 1.0);
}

/// The same seed draws the same batch; another seed another one.
#[test]
fn a_batch_is_a_function_of_its_seed() {
    let d = DifDesign {
        delta: 0.5,
        ..design(300, 8, 4, Layout::Campaign(2))
    };
    let (a, b, c) = (dif_batch(&d, 3), dif_batch(&d, 3), dif_batch(&d, 4));
    assert_eq!((&a.anchors, &a.x), (&b.anchors, &b.x));
    assert_ne!(a.x, c.x);
}
