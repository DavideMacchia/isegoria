//! Level B acceptance tests, run on the same data as the sims (exported by
//! `sim/export_fixtures.py`). They cover point-biserial + logistic DIF against the
//! oracle, and the latent-class mixture detector (`sim/latent_dif_and_capacity.py`).

use scoring::dif::mixture_dif;
#[cfg(feature = "calibration")]
use scoring::dif::{logistic_dif, mantel_haenszel, EtsClass, BETA2_MAX};
use scoring::irt::{fit_2pl_item, theta_from_anchors};
#[cfg(feature = "calibration")]
use scoring::irt::{point_biserial, R_PBIS_MIN};
#[cfg(feature = "calibration")]
use scoring::validation::purify_theta;
use std::fs;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn read_matrix(name: &str) -> Vec<Vec<f64>> {
    let path = fixtures_dir().join(name);
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {:?}: {e}", path));
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
    read_matrix(name).into_iter().map(|row| row[0]).collect()
}

#[cfg(feature = "calibration")]
fn read_csv_skip_header(name: &str) -> Vec<Vec<f64>> {
    let path = fixtures_dir().join(name);
    let text = fs::read_to_string(&path).unwrap();
    text.lines()
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            l.split(',')
                .map(|c| c.trim().parse::<f64>().unwrap())
                .collect()
        })
        .collect()
}

fn column(m: &[Vec<f64>], j: usize) -> Vec<f64> {
    m.iter().map(|row| row[j]).collect()
}

fn pearson_abs(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len() as f64;
    let ma = a.iter().sum::<f64>() / n;
    let mb = b.iter().sum::<f64>() / n;
    let (mut cov, mut va, mut vb) = (0.0, 0.0, 0.0);
    for i in 0..a.len() {
        cov += (a[i] - ma) * (b[i] - mb);
        va += (a[i] - ma).powi(2);
        vb += (b[i] - mb).powi(2);
    }
    (cov / (va.sqrt() * vb.sqrt())).abs()
}

#[cfg(feature = "calibration")]
#[test]
fn point_biserial_and_logistic_dif_reproduce_oracle() {
    let xa = read_matrix("levelb_XA.csv");
    let x = read_matrix("levelb_X.csv");
    let grp = read_vector("levelb_grp.csv");
    let expected = read_csv_skip_header("levelb_expected.csv"); // idx,p,r_pbis,beta2
    let theta = theta_from_anchors(&xa);

    for row in &expected {
        let j = row[0] as usize;
        let (rp_exp, b2_exp) = (row[2], row[3]);
        let item = column(&x, j);

        let rp = point_biserial(&item, &theta);
        assert!(
            (rp - rp_exp).abs() < 0.02,
            "r_pbis[{j}]={rp:.4} exp={rp_exp:.4}"
        );

        let b2 = logistic_dif(&item, &theta, &grp).beta2;
        assert!(
            (b2 - b2_exp).abs() < 0.02,
            "beta2[{j}]={b2:.4} exp={b2_exp:.4}"
        );
    }
}

#[cfg(feature = "calibration")]
#[test]
fn verdicts_match_the_oracle() {
    let xa = read_matrix("levelb_XA.csv");
    let x = read_matrix("levelb_X.csv");
    let grp = read_vector("levelb_grp.csv");
    let theta = theta_from_anchors(&xa);

    // 04 ESM (idx 3): strong DIF.
    assert!(logistic_dif(&column(&x, 3), &theta, &grp).beta2.abs() > BETA2_MAX);
    // 05 capital of Italy (idx 4): no discrimination.
    assert!(point_biserial(&column(&x, 4), &theta) < R_PBIS_MIN);
    // 06 wrong key (idx 5): negative point-biserial.
    assert!(point_biserial(&column(&x, 5), &theta) < 0.0);
    // A clean item (idx 0) passes both.
    assert!(point_biserial(&column(&x, 0), &theta) >= R_PBIS_MIN);
    assert!(logistic_dif(&column(&x, 0), &theta, &grp).beta2.abs() <= BETA2_MAX);
}

#[test]
fn irt_2pl_discrimination_ranks_items() {
    let xa = read_matrix("levelb_XA.csv");
    let x = read_matrix("levelb_X.csv");
    let theta = theta_from_anchors(&xa);
    let a_flat = fit_2pl_item(&theta, &column(&x, 4)).a; // capital of Italy: low discrimination
    let a_sharp = fit_2pl_item(&theta, &column(&x, 0)).a; // number of deputies: high discrimination
    assert!(a_sharp > a_flat, "a_sharp={a_sharp:.3} a_flat={a_flat:.3}");
    assert!(
        a_flat < 0.6,
        "flat item discrimination should be below A_MIN, got {a_flat:.3}"
    );
}

#[cfg(feature = "calibration")]
#[test]
fn mantel_haenszel_classifies_dif() {
    let xa = read_matrix("levelb_XA.csv");
    let x = read_matrix("levelb_X.csv");
    let grp = read_vector("levelb_grp.csv");
    let theta = theta_from_anchors(&xa);

    // 04 ESM (idx 3): strong DIF → class C (rejected).
    let esm = mantel_haenszel(&column(&x, 3), &theta, &grp, 5);
    assert_eq!(esm.class, EtsClass::C, "ESM Δ_MH = {:.2}", esm.delta);
    // A clean item (idx 0) → class A, and its sign agrees with β₂ (~0).
    let clean = mantel_haenszel(&column(&x, 0), &theta, &grp, 5);
    assert_eq!(clean.class, EtsClass::A, "clean Δ_MH = {:.2}", clean.delta);
}

#[cfg(feature = "calibration")]
#[test]
fn purification_reaches_a_stable_flagged_set() {
    // docs/02 §B.4: iterate until the flagged set is a fixed point. Only the DIF
    // item (idx 3, ESM) is flagged; clean items are not; θ stays aligned with the
    // anchor-based estimate.
    let xa = read_matrix("levelb_XA.csv");
    let x = read_matrix("levelb_X.csv");
    let grp = read_vector("levelb_grp.csv");

    let res = purify_theta(&xa, &x, &grp, 10);
    assert!(res.flagged[3], "ESM item should be flagged");
    for j in [0usize, 1, 2, 7, 8, 9] {
        assert!(!res.flagged[j], "clean item {j} should not be flagged");
    }
    // Round 1 flags ESM (a change from "none flagged"), round 2 confirms it: the loop
    // stops at the first round that reproduces the previous set, not before (T41).
    assert_eq!(res.iterations, 2);

    // θ is the standardized total over the anchors plus the batch items left unflagged.
    let totals: Vec<f64> = (0..xa.len())
        .map(|i| {
            xa[i].iter().sum::<f64>()
                + (0..x[i].len())
                    .filter(|&j| !res.flagged[j])
                    .map(|j| x[i][j])
                    .sum::<f64>()
        })
        .collect();
    let n = totals.len() as f64;
    let mean = totals.iter().sum::<f64>() / n;
    let sd = (totals.iter().map(|t| (t - mean).powi(2)).sum::<f64>() / n).sqrt();
    for (i, t) in totals.iter().enumerate() {
        assert!((res.theta[i] - (t - mean) / sd).abs() < 1e-9, "theta[{i}]");
    }

    // Fixed point: re-running DIF with the final θ reproduces the same flagged set.
    for (j, &flag) in res.flagged.iter().enumerate() {
        let refit = logistic_dif(&column(&x, j), &res.theta, &grp).beta2.abs() > BETA2_MAX;
        assert_eq!(refit, flag, "item {j} not at a fixed point");
    }
}

#[test]
fn mixture_detects_bias_in_a_batch() {
    // 3 of 8 items biased on a never-observed axis (edu). The detector should
    // recover a large difficulty gap on the biased items, a small one on the clean
    // ones, prefer a two-class mixture by BIC, and reconstruct the hidden axis
    // (docs/02, §B.3).
    let theta = read_vector("mixture_batch_theta.csv");
    let x = read_matrix("mixture_batch_X.csv");
    let edu = read_vector("mixture_batch_edu.csv");
    let res = mixture_dif(&theta, &x, 8, 0);

    // `dif` is the b-gap 2|δ| (DIF-006); the fixture plants δ = 0.9, a gap of 1.8.
    let biased_mean = res.dif[..3].iter().sum::<f64>() / 3.0;
    let clean_mean = res.dif[3..].iter().sum::<f64>() / 5.0;
    assert!(biased_mean > 1.2, "biased DIF mean = {biased_mean:.3}");
    assert!(clean_mean < 0.8, "clean DIF mean = {clean_mean:.3}");
    assert_eq!(
        res.classes, 2,
        "BIC should select two classes: {:?}",
        res.candidates
    );
    assert!(
        !res.non_uniform,
        "the planted DIF is uniform: {:?}",
        res.candidates
    );
    assert!(res.bic_gain > 0.0);

    let axis = best_axis(&res.posterior, &edu);
    assert!(axis > 0.4, "hidden axis recovery |corr| = {axis:.3}");
}

/// The class whose posterior tracks the hidden axis best: `max_g |corr(r_·g, axis)|`.
fn best_axis(posterior: &[Vec<f64>], axis: &[f64]) -> f64 {
    let classes = posterior.first().map_or(0, |r| r.len());
    (0..classes)
        .map(|g| {
            let col: Vec<f64> = posterior.iter().map(|r| r[g]).collect();
            pearson_abs(&col, axis)
        })
        .fold(0.0, f64::max)
}

#[test]
fn mixture_misses_a_single_biased_item() {
    // Invariant #8 / docs/02 §B.3: a lone biased item is unidentifiable — the
    // reason validation must run in batches.
    let theta = read_vector("mixture_single_theta.csv");
    let x = read_matrix("mixture_single_X.csv");
    let edu = read_vector("mixture_single_edu.csv");
    let res = mixture_dif(&theta, &x, 8, 0);

    let axis = best_axis(&res.posterior, &edu);
    assert!(
        axis < 0.35,
        "single biased item should stay invisible, axis = {axis:.3}"
    );
}

/// A NaN in the caller-supplied θ used to panic `mantel_haenszel` via
/// `partial_cmp().unwrap()`. Bad data must degrade the stratification, not crash.
#[cfg(feature = "calibration")]
#[test]
fn mantel_haenszel_tolerates_a_nan_theta() {
    let item = vec![1.0, 0.0, 1.0, 0.0, 1.0, 0.0];
    let theta = vec![0.5, f64::NAN, -0.3, 1.2, f64::NAN, -1.0];
    let group = vec![0.0, 1.0, 0.0, 1.0, 0.0, 1.0];
    let r = mantel_haenszel(&item, &theta, &group, 3); // must not panic
                                                       // The classification is still one of the ETS classes.
    assert!(matches!(r.class, EtsClass::A | EtsClass::B | EtsClass::C));
}

#[cfg(feature = "calibration")]
#[test]
fn mantel_haenszel_tolerates_nan_theta_at_sort_detection_sizes() {
    // docs/08 IQ-2 / DIF-003 guard: a comparator that treats NaN as equal to everything
    // is not a total order, which Rust's sort (≥ 1.81) may detect on longer slices and
    // panic on. `total_cmp` cannot. 200 respondents, every 13th θ is NaN.
    let n = 200;
    let item: Vec<f64> = (0..n).map(|i| ((i * 7) % 3 == 0) as u8 as f64).collect();
    let theta: Vec<f64> = (0..n)
        .map(|i| {
            if i % 13 == 0 {
                f64::NAN
            } else {
                (i as f64 / n as f64) * 4.0 - 2.0
            }
        })
        .collect();
    let group: Vec<f64> = (0..n)
        .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
        .collect();
    let r = mantel_haenszel(&item, &theta, &group, 5);
    assert!(matches!(r.class, EtsClass::A | EtsClass::B | EtsClass::C));
}

/// Respondents for a hand-computed Mantel–Haenszel table: `(θ, group, correct)`.
#[cfg(feature = "calibration")]
fn mh_input(rows: &[(f64, f64, bool)]) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let item = rows.iter().map(|r| r.2 as i32 as f64).collect();
    let theta = rows.iter().map(|r| r.0).collect();
    let group = rows.iter().map(|r| r.1).collect();
    (item, theta, group)
}

/// `a` reference-correct, `b` reference-wrong, `c` focal-correct, `d` focal-wrong, all
/// at ability `theta` (group −1 reference, +1 focal).
#[cfg(feature = "calibration")]
fn cell_rows(theta: f64, a: usize, b: usize, c: usize, d: usize) -> Vec<(f64, f64, bool)> {
    let mut v = Vec::new();
    v.extend(std::iter::repeat_n((theta, -1.0, true), a));
    v.extend(std::iter::repeat_n((theta, -1.0, false), b));
    v.extend(std::iter::repeat_n((theta, 1.0, true), c));
    v.extend(std::iter::repeat_n((theta, 1.0, false), d));
    v
}

/// One stratum: α_MH = (a·d)/(b·c) = 9/16, Δ = −2.35·ln α ≈ +1.35 → class B. Swapping
/// the groups inverts α and flips the sign of Δ, same class (T41).
#[cfg(feature = "calibration")]
#[test]
fn mantel_haenszel_matches_a_hand_computed_table() {
    let (item, theta, group) = mh_input(&cell_rows(0.0, 3, 4, 4, 3));
    let r = mantel_haenszel(&item, &theta, &group, 1);
    assert!((r.alpha - 9.0 / 16.0).abs() < 1e-12, "alpha = {}", r.alpha);
    assert!((r.delta - (-2.35 * (9.0_f64 / 16.0).ln())).abs() < 1e-12);
    assert!(r.delta > 1.0 && r.delta < 1.5, "delta = {}", r.delta);
    assert_eq!(r.class, EtsClass::B);

    let flipped: Vec<f64> = group.iter().map(|g| -g).collect();
    let s = mantel_haenszel(&item, &theta, &flipped, 1);
    assert!((s.alpha - 16.0 / 9.0).abs() < 1e-12);
    assert!((s.delta + r.delta).abs() < 1e-12, "Δ flips sign");
    assert_eq!(s.class, EtsClass::B);
}

/// Matching on ability matters: two strata with α = 3 each pool to α_MH = 3, while the
/// collapsed single table gives 4. Pins the stratum boundaries and per-stratum sums.
#[cfg(feature = "calibration")]
#[test]
fn mantel_haenszel_pools_within_ability_strata() {
    let mut rows = cell_rows(-1.0, 3, 1, 1, 1);
    rows.extend(cell_rows(1.0, 1, 1, 1, 3));
    let (item, theta, group) = mh_input(&rows);
    let two = mantel_haenszel(&item, &theta, &group, 2);
    assert!(
        (two.alpha - 3.0).abs() < 1e-12,
        "stratified alpha = {}",
        two.alpha
    );
    let one = mantel_haenszel(&item, &theta, &group, 1);
    assert!(
        (one.alpha - 4.0).abs() < 1e-12,
        "collapsed alpha = {}",
        one.alpha
    );
    assert_eq!(two.class, EtsClass::C);
}

/// Strata of unequal size weight each table by `1/N_s`: 6 respondents with (3,1,1,1)
/// and 7 with (1,1,1,4) give α_MH = (3/6 + 4/7)/(1/6 + 1/7) = 45/13 — not the
/// unweighted 7/2, and not the collapsed 20/4 (T41).
#[cfg(feature = "calibration")]
#[test]
fn mantel_haenszel_weights_each_stratum_by_its_size() {
    let mut rows = cell_rows(-1.0, 3, 1, 1, 1);
    rows.extend(cell_rows(1.0, 1, 1, 1, 4));
    let (item, theta, group) = mh_input(&rows);
    let r = mantel_haenszel(&item, &theta, &group, 2);
    assert!((r.alpha - 45.0 / 13.0).abs() < 1e-12, "alpha = {}", r.alpha);
}

/// More strata than respondents leaves strata empty; they contribute nothing instead of
/// a 0/0 NaN. Note what remains: every stratum holds one person, who forms no
/// discordant pair, so α is undefined → ∞ → class C. Over-stratifying a small sample
/// rejects the item (calibration-only; the caller picks `n_strata`).
#[cfg(feature = "calibration")]
#[test]
fn mantel_haenszel_skips_empty_strata() {
    let (item, theta, group) = mh_input(&cell_rows(0.0, 3, 4, 4, 3));
    let r = mantel_haenszel(&item, &theta, &group, 40);
    assert_eq!(r.alpha, f64::INFINITY);
    assert!(!r.delta.is_nan());
    assert_eq!(r.class, EtsClass::C);
    // The same respondents in one stratum: a finite estimate.
    assert!(mantel_haenszel(&item, &theta, &group, 1).alpha.is_finite());
}

/// No reference-wrong/focal-correct pair at all (here: everyone correct): the odds ratio
/// is undefined; it is reported as infinite and classed C, not NaN.
#[cfg(feature = "calibration")]
#[test]
fn mantel_haenszel_without_discordant_cells_is_infinite_and_class_c() {
    let (item, theta, group) = mh_input(&cell_rows(0.0, 5, 0, 5, 0));
    let r = mantel_haenszel(&item, &theta, &group, 1);
    assert_eq!(r.alpha, f64::INFINITY);
    assert_eq!(r.class, EtsClass::C);
}
