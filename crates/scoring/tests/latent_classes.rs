//! The latent-class DIF detector beyond the two-class, uniform case (T40): the number of
//! classes and uniform vs non-uniform DIF are chosen by BIC, each candidate from several
//! seeded starts. Synthetic data with known parameters: N = 3000, K = 8, θ known.

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use scoring::dif::mixture_dif;
use std::fs;
use std::path::PathBuf;

fn normal(r: &mut ChaCha8Rng) -> f64 {
    let u1: f64 = 1.0 - r.gen::<f64>();
    let u2: f64 = r.gen();
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

/// Respondents drawn into classes by `shares`; item `j` answers a 2PL with the class's
/// `(a(j, c), b(j, c))`.
fn generate(
    seed: u64,
    shares: &[f64],
    a: impl Fn(usize, usize) -> f64,
    b: impl Fn(usize, usize) -> f64,
) -> (Vec<f64>, Vec<Vec<f64>>) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let (mut theta, mut x) = (Vec::new(), Vec::new());
    for _ in 0..3000 {
        let t = normal(&mut rng);
        let u: f64 = rng.gen();
        let (mut c, mut acc) = (0, shares[0]);
        while u > acc && c + 1 < shares.len() {
            c += 1;
            acc += shares[c];
        }
        x.push(
            (0..8)
                .map(|j| {
                    let p = 1.0 / (1.0 + (-a(j, c) * (t - b(j, c))).exp());
                    (rng.gen::<f64>() < p) as i32 as f64
                })
                .collect(),
        );
        theta.push(t);
    }
    (theta, x)
}

fn difficulty(j: usize) -> f64 {
    -1.2 + 2.4 * j as f64 / 7.0
}

/// No DIF at all: the BIC keeps one class, so there is no gap to flag (a false-positive
/// check on one dataset; the rate is T24's).
#[test]
fn clean_data_selects_a_single_class() {
    let (theta, x) = generate(10, &[1.0], |_, _| 1.2, |j, _| difficulty(j));
    let res = mixture_dif(&theta, &x, 8, 0);
    assert_eq!(res.classes, 1, "{:?}", res.candidates);
    assert!(res.dif.iter().all(|&d| d == 0.0));
    assert_eq!(res.bic_gain, 0.0);
}

/// Non-uniform DIF: the same difficulty in both classes, but items 0–2 discriminate at
/// 0.5 in one class and 2.5 in the other. The BIC prefers per-class discrimination and
/// `a_gap` locates those items; their difficulty gap stays small. (The verdict still
/// reads only the difficulty gap, as `docs/02` §B.3 specifies — see `docs/08` DIF-004.)
#[test]
fn a_discrimination_shift_is_found_as_non_uniform_dif() {
    let (theta, x) = generate(
        21,
        &[0.5, 0.5],
        |j, c| match (j < 3, c) {
            (true, 0) => 0.5,
            (true, _) => 2.5,
            _ => 1.2,
        },
        |j, _| difficulty(j),
    );
    let res = mixture_dif(&theta, &x, 8, 0);
    assert_eq!(res.classes, 2, "{:?}", res.candidates);
    assert!(res.non_uniform, "{:?}", res.candidates);
    for j in 0..3 {
        assert!(res.a_gap[j] > 1.0, "item {j}: a_gap = {:.2}", res.a_gap[j]);
    }
    for j in 3..8 {
        assert!(res.a_gap[j] < 0.7, "item {j}: a_gap = {:.2}", res.a_gap[j]);
    }
}

/// Three classes shifting items 0–3 by −1, 0, +1: whatever number of classes the BIC
/// settles on, the shifted items get a large difficulty gap and the others a small one.
#[test]
fn a_three_way_shift_is_detected_on_the_shifted_items() {
    let (theta, x) = generate(
        30,
        &[1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0],
        |_, _| 1.2,
        |j, c| difficulty(j) + if j < 4 { c as f64 - 1.0 } else { 0.0 },
    );
    let res = mixture_dif(&theta, &x, 8, 0);
    assert!(res.classes >= 2, "{:?}", res.candidates);
    for j in 0..4 {
        assert!(res.dif[j] > 1.2, "item {j}: dif = {:.2}", res.dif[j]);
    }
    for j in 4..8 {
        assert!(res.dif[j] < 0.3, "item {j}: dif = {:.2}", res.dif[j]);
    }
}

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

/// The differential gap (D37, T53) is the class gap net of the batch's common class
/// shift. On the batch fixture (3 of 8 items shifted by δ = 0.9) the common shift is the
/// clean items' ≈ 0, so the differential gap agrees with the raw gap: ≈ 1.8 on the
/// shifted items, small on the rest. It is reported next to `dif`, which alone is the
/// verdict; the campaign in which it inverts is `protocol/tests/anchor_reliability.rs`.
#[test]
fn the_differential_gap_agrees_with_the_raw_gap_when_few_items_are_shifted() {
    let theta: Vec<f64> = read_matrix("mixture_batch_theta.csv")
        .into_iter()
        .map(|r| r[0])
        .collect();
    let x = read_matrix("mixture_batch_X.csv");
    let res = mixture_dif(&theta, &x, 8, 0);
    assert_eq!(res.differential_gap.len(), 8);
    for j in 0..3 {
        let (raw, diff) = (res.dif[j], res.differential_gap[j]);
        assert!(diff > 1.5, "shifted item {j}: differential gap {diff:.2}");
        assert!(
            (raw - diff).abs() < 0.4,
            "item {j}: raw {raw:.2} vs differential {diff:.2}"
        );
    }
    for j in 3..8 {
        let d = res.differential_gap[j];
        assert!(d < 0.5, "clean item {j}: differential gap {d:.2}");
    }
}

/// The multi-start makes the verdict independent of the seed (as T48 did for bridging):
/// on the batch fixture, different seeds select the same model and the same gaps.
#[test]
fn the_selected_model_does_not_depend_on_the_seed() {
    let theta: Vec<f64> = read_matrix("mixture_batch_theta.csv")
        .into_iter()
        .map(|r| r[0])
        .collect();
    let x = read_matrix("mixture_batch_X.csv");
    let base = mixture_dif(&theta, &x, 8, 0);
    for seed in [1, 7, 1234] {
        let res = mixture_dif(&theta, &x, 8, seed);
        assert_eq!(
            (res.classes, res.non_uniform),
            (base.classes, base.non_uniform)
        );
        for j in 0..8 {
            assert!(
                (res.dif[j] - base.dif[j]).abs() < 1e-3,
                "seed {seed}, item {j}: {:.4} vs {:.4}",
                res.dif[j],
                base.dif[j]
            );
        }
    }
}

/// Well-separated classes are selected as such: three equal classes shifting items 0–5
/// by −2, 0, +2 give `G = 3`, proportions near 1/3, and gaps near the true 4 on the
/// shifted items only — the gap is taken over all three classes, not a pair.
#[test]
fn three_well_separated_classes_are_selected_as_three() {
    let (theta, x) = generate(
        40,
        &[1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0],
        |_, _| 1.2,
        |j, c| difficulty(j) + if j < 6 { 2.0 * (c as f64 - 1.0) } else { 0.0 },
    );
    let res = mixture_dif(&theta, &x, 8, 0);
    assert_eq!(res.classes, 3, "{:?}", res.candidates);
    for p in &res.pi {
        assert!((0.25..0.42).contains(p), "pi = {:?}", res.pi);
    }
    for j in 0..6 {
        assert!(res.dif[j] > 3.0, "item {j}: dif = {:.2}", res.dif[j]);
    }
    for j in 6..8 {
        assert!(res.dif[j] < 0.5, "item {j}: dif = {:.2}", res.dif[j]);
    }
}

/// The staged search: a larger mixture is tried only while the best BIC improves. On
/// clean data two classes do not help, so three are never fitted.
#[test]
fn larger_mixtures_are_tried_only_while_the_bic_improves() {
    let (theta, x) = generate(10, &[1.0], |_, _| 1.2, |j, _| difficulty(j));
    let shapes: Vec<(usize, bool)> = mixture_dif(&theta, &x, 8, 0)
        .candidates
        .iter()
        .map(|c| (c.0, c.1))
        .collect();
    assert_eq!(shapes, vec![(1, false), (2, false), (2, true)]);

    let theta: Vec<f64> = read_matrix("mixture_batch_theta.csv")
        .into_iter()
        .map(|r| r[0])
        .collect();
    let x = read_matrix("mixture_batch_X.csv");
    let shapes: Vec<(usize, bool)> = mixture_dif(&theta, &x, 8, 0)
        .candidates
        .iter()
        .map(|c| (c.0, c.1))
        .collect();
    // Two classes improve on one; three do not improve on two: stop there.
    assert_eq!(
        shapes,
        vec![(1, false), (2, false), (2, true), (3, false), (3, true)]
    );
}

/// The seed is honoured: different seeds start the mixtures elsewhere (the fitted
/// posteriors differ in their low bits) and still select the same model.
#[test]
fn the_seed_changes_the_starts_not_the_verdict() {
    let theta: Vec<f64> = read_matrix("mixture_batch_theta.csv")
        .into_iter()
        .map(|r| r[0])
        .collect();
    let x = read_matrix("mixture_batch_X.csv");
    let (a, b) = (mixture_dif(&theta, &x, 8, 0), mixture_dif(&theta, &x, 8, 7));
    let bits = |r: &scoring::dif::MixtureDif| -> Vec<u64> {
        r.posterior.concat().iter().map(|v| v.to_bits()).collect()
    };
    assert_ne!(bits(&a), bits(&b), "seed 7 reproduced seed 0 bit for bit");
    assert_eq!((a.classes, a.non_uniform), (b.classes, b.non_uniform));
}
