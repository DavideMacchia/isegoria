//! KR-20 of the anchor total (`docs/02` §B.4, `docs/01` D37, T53): the reliability the
//! production latent re-check requires of its ability proxy (`KR20_MIN`, applied by
//! `protocol::pilot::admit_anchors`). Hand-computed values, the degenerate cases (0,
//! never NaN, as `point_biserial` — T36) and the repository's own 30-anchor fixture,
//! which sits below the floor.

use scoring::irt::{kr20, KR20_MIN};
use std::fs;
use std::path::PathBuf;

fn read_matrix(name: &str) -> Vec<Vec<f64>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.split(',').map(|c| c.trim().parse().unwrap()).collect())
        .collect()
}

#[test]
fn kr20_matches_the_hand_computation() {
    // Totals 3, 2, 1, 0: mean 1.5, population variance 1.25; p = (0.75, 0.5, 0.25),
    // Σ p(1 − p) = 0.625; KR-20 = 3/2 · (1 − 0.625 / 1.25) = 0.75.
    let xa = vec![
        vec![1.0, 1.0, 1.0],
        vec![1.0, 1.0, 0.0],
        vec![1.0, 0.0, 0.0],
        vec![0.0, 0.0, 0.0],
    ];
    assert!((kr20(&xa) - 0.75).abs() < 1e-12, "{}", kr20(&xa));

    // Two anchors always answered alike: perfectly consistent, KR-20 = 1.
    let same = vec![
        vec![1.0, 1.0],
        vec![1.0, 1.0],
        vec![0.0, 0.0],
        vec![0.0, 0.0],
    ];
    assert!((kr20(&same) - 1.0).abs() < 1e-12);

    // Mostly opposite answers: the total varies less than independent anchors would
    // make it, and KR-20 is negative (totals 1, 1, 1, 1, 2, 0: variance 1/3, Σ p(1 − p)
    // = 0.5, KR-20 = 2 · (1 − 1.5) = −1). It fails the floor, as it should.
    let opposite = vec![
        vec![1.0, 0.0],
        vec![0.0, 1.0],
        vec![1.0, 0.0],
        vec![0.0, 1.0],
        vec![1.0, 1.0],
        vec![0.0, 0.0],
    ];
    assert!((kr20(&opposite) + 1.0).abs() < 1e-12, "{}", kr20(&opposite));
    assert!(kr20(&opposite) < KR20_MIN);
}

/// A perfect Guttman scale of 40 anchors (respondent `i` gets the first `⌊41·i/n⌋`
/// right) is reliable well above the floor — the anchor set the protocol tests use where
/// only the other floors are under test.
#[test]
fn a_guttman_scale_of_forty_anchors_clears_the_floor() {
    let n = 400;
    let xa: Vec<Vec<f64>> = (0..n)
        .map(|i| {
            let total = i * 41 / n;
            (0..40).map(|j| if j < total { 1.0 } else { 0.0 }).collect()
        })
        .collect();
    let r = kr20(&xa);
    assert!((0.96..0.99).contains(&r), "KR-20 = {r}");
}

#[test]
fn undefined_reliability_is_reported_as_zero_never_nan() {
    // No anchors, one anchor (K/(K−1) undefined), no spread in the totals, a non-finite
    // entry, a ragged row: 0 in every case, which fails the floor.
    assert_eq!(kr20(&[]), 0.0);
    assert_eq!(kr20(&[vec![1.0], vec![0.0], vec![1.0]]), 0.0);
    assert_eq!(kr20(&[vec![1.0, 0.0], vec![0.0, 1.0], vec![1.0, 0.0]]), 0.0);
    assert_eq!(kr20(&[vec![1.0, 1.0], vec![1.0, 1.0]]), 0.0);
    let nan = vec![vec![1.0, f64::NAN], vec![0.0, 0.0], vec![1.0, 1.0]];
    assert_eq!(kr20(&nan), 0.0);
    let ragged = vec![vec![1.0, 1.0, 1.0], vec![1.0], vec![0.0, 0.0, 0.0]];
    assert!(kr20(&ragged).is_finite());
}

/// The Level-B fixture's anchors (`levelb_XA.csv`: 1,500 respondents, 30 anchors
/// generated as 3PL with a 0.25 guessing floor by `sim/bridging_irt_dif.py`) have a
/// KR-20 of 0.816, pinned here against an independent (Python stdlib) computation. That
/// is below `KR20_MIN`: the production re-check would refuse an ability proxy from them
/// (D37), so the oracle tests of Level B reach the detector through the ungated math
/// (`revalidate_pool_latent`), not the gated entry point. The mixture fixtures carry θ
/// from 30 2PL anchors (KR-20 ≈ 0.87 in the paper's Table 14) and no anchor matrix; they
/// are regenerated with the target model (T54).
#[test]
fn the_level_b_fixture_anchors_are_below_the_production_floor() {
    let r = kr20(&read_matrix("levelb_XA.csv"));
    assert!((r - 0.815691844231).abs() < 1e-9, "KR-20 = {r}");
    assert!(r < KR20_MIN);
}
