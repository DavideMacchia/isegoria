//! Level B acceptance tests, run on the same data as the sims (exported by
//! `sim/export_fixtures.py`). They cover point-biserial + logistic DIF against the
//! oracle, and the latent-class mixture detector (`sim/latent_dif_and_capacity.py`).

use scoring::dif::{logistic_dif, mantel_haenszel, mixture_dif, EtsClass, BETA2_MAX};
use scoring::irt::{fit_2pl_item, point_biserial, theta_from_anchors, R_PBIS_MIN};
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
    let (a_flat, _) = fit_2pl_item(&theta, &column(&x, 4)); // capital of Italy: low discrimination
    let (a_sharp, _) = fit_2pl_item(&theta, &column(&x, 0)); // number of deputies: high discrimination
    assert!(a_sharp > a_flat, "a_sharp={a_sharp:.3} a_flat={a_flat:.3}");
    assert!(
        a_flat < 0.6,
        "flat item discrimination should be below A_MIN, got {a_flat:.3}"
    );
}

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
    assert!(res.iterations <= 10);

    // Fixed point: re-running DIF with the final θ reproduces the same flagged set.
    for (j, &flag) in res.flagged.iter().enumerate() {
        let refit = logistic_dif(&column(&x, j), &res.theta, &grp).beta2.abs() > BETA2_MAX;
        assert_eq!(refit, flag, "item {j} not at a fixed point");
    }
}

#[test]
fn mixture_detects_bias_in_a_batch() {
    // 3 of 8 items biased on a never-observed axis (edu). The detector should
    // recover high |δ| on the biased items, ~0 on the clean ones, BIC>0, and
    // reconstruct the hidden axis (docs/02, §B.3).
    let theta = read_vector("mixture_batch_theta.csv");
    let x = read_matrix("mixture_batch_X.csv");
    let edu = read_vector("mixture_batch_edu.csv");
    let res = mixture_dif(&theta, &x, 8, 0);

    let biased_mean = res.delta[..3].iter().sum::<f64>() / 3.0;
    let clean_mean = res.delta[3..].iter().sum::<f64>() / 5.0;
    assert!(biased_mean > 0.6, "biased |δ| mean = {biased_mean:.3}");
    assert!(clean_mean < 0.4, "clean |δ| mean = {clean_mean:.3}");
    assert!(
        res.bic > 0.0,
        "BIC = {:.1} should favor two classes",
        res.bic
    );

    let axis = pearson_abs(&res.class_posterior, &edu);
    assert!(axis > 0.4, "hidden axis recovery |corr| = {axis:.3}");
}

#[test]
fn mixture_misses_a_single_biased_item() {
    // Invariant #8 / docs/02 §B.3: a lone biased item is unidentifiable — the
    // reason validation must run in batches.
    let theta = read_vector("mixture_single_theta.csv");
    let x = read_matrix("mixture_single_X.csv");
    let edu = read_vector("mixture_single_edu.csv");
    let res = mixture_dif(&theta, &x, 8, 0);

    let axis = pearson_abs(&res.class_posterior, &edu);
    assert!(
        axis < 0.35,
        "single biased item should stay invisible, axis = {axis:.3}"
    );
}

/// A NaN in the caller-supplied θ used to panic `mantel_haenszel` via
/// `partial_cmp().unwrap()`. Bad data must degrade the stratification, not crash.
#[test]
fn mantel_haenszel_tolerates_a_nan_theta() {
    let item = vec![1.0, 0.0, 1.0, 0.0, 1.0, 0.0];
    let theta = vec![0.5, f64::NAN, -0.3, 1.2, f64::NAN, -1.0];
    let group = vec![0.0, 1.0, 0.0, 1.0, 0.0, 1.0];
    let r = mantel_haenszel(&item, &theta, &group, 3); // must not panic
                                                       // The classification is still one of the ETS classes.
    assert!(matches!(r.class, EtsClass::A | EtsClass::B | EtsClass::C));
}

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
