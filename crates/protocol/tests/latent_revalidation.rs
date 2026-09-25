//! The production latent re-check reads the quantity `docs/02` §B.3 specifies and acts
//! only on a trustworthy fit (T35, `docs/08` DIF-006): a fit that did not converge, or
//! for which the BIC selects one class, flags nothing.

use protocol::revalidation::{latent_flags, revalidate_pool_latent};
use scoring::dif::{mixture_dif, MixtureDif, MIXTURE_DIF_MAX};
use scoring::Convergence;
use std::fs;
use std::path::PathBuf;

fn read_matrix(name: &str) -> Vec<Vec<f64>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../scoring/tests/fixtures")
        .join(name);
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.split(',').map(|c| c.trim().parse().unwrap()).collect())
        .collect()
}

fn read_vector(name: &str) -> Vec<f64> {
    read_matrix(name).into_iter().map(|r| r[0]).collect()
}

/// A selected fit with `classes` classes and the given per-item gaps.
fn fit(dif: Vec<f64>, classes: usize, status: Convergence) -> MixtureDif {
    let k = dif.len();
    MixtureDif {
        classes,
        non_uniform: false,
        pi: vec![1.0 / classes as f64; classes],
        dif,
        a_gap: vec![0.0; k],
        differential: vec![0.0; k],
        posterior: Vec::new(),
        bic_gain: if classes > 1 { 10.0 } else { 0.0 },
        candidates: Vec::new(),
        status,
    }
}

#[test]
fn a_trustworthy_fit_flags_exactly_the_items_above_the_threshold() {
    let t = MIXTURE_DIF_MAX;
    let res = fit(
        vec![0.0, t - 1e-9, t, t + 1e-9, 2.0 * t],
        2,
        Convergence::Converged,
    );
    assert_eq!(latent_flags(&res), vec![false, false, false, true, true]);
}

#[test]
fn a_fit_that_did_not_converge_flags_nothing() {
    let dif = vec![0.2, 3.0, 3.0];
    for status in [Convergence::MaxIters, Convergence::LineSearchFailed] {
        assert_eq!(
            latent_flags(&fit(dif.clone(), 2, status)),
            vec![false; 3],
            "{status:?}"
        );
    }
    // Contrast: the same gaps on a converged fit are flagged.
    assert_eq!(
        latent_flags(&fit(dif, 2, Convergence::Converged)),
        vec![false, true, true]
    );
}

#[test]
fn a_fit_for_which_the_bic_selects_one_class_flags_nothing() {
    assert_eq!(
        latent_flags(&fit(vec![3.0, 3.0], 1, Convergence::Converged)),
        vec![false, false]
    );
}

#[test]
fn the_reported_gap_is_twice_the_class_shift() {
    // The fixture plants a class shift δ = 0.9 on items 0..3: a b-gap of 1.8. The
    // estimate lands on the gap, not on δ (DIF-006).
    let theta = read_vector("mixture_batch_theta.csv");
    let x = read_matrix("mixture_batch_X.csv");
    let res = mixture_dif(&theta, &x, 8, 0);
    for (j, d) in res.dif[..3].iter().enumerate() {
        assert!((1.5..=2.6).contains(d), "biased item {j}: DIF = {d:.3}");
    }
    for (j, d) in res.dif[3..].iter().enumerate() {
        assert!(*d < 0.5, "clean item {}: DIF = {d:.3}", j + 3);
    }
    // The diagnostic (D37, T53): with 3 of 8 shifted, the batch's common shift is a
    // clean item's, so the differential gap tells the same story as the raw gap here.
    for (j, d) in res.differential.iter().enumerate() {
        if j < 3 {
            assert!(*d > 1.0, "biased item {j}: differential = {d:.3}");
        } else {
            assert!(*d < 0.5, "clean item {j}: differential = {d:.3}");
        }
    }
    assert_eq!(
        revalidate_pool_latent(&theta, &x, 0),
        vec![true, true, true, false, false, false, false, false]
    );
}

/// INV-8: the lone biased item stays invisible behind the provisional threshold.
#[test]
fn the_literature_threshold_would_retire_every_item_of_the_single_bias_fixture() {
    let theta = read_vector("mixture_single_theta.csv");
    let x = read_matrix("mixture_single_X.csv");
    let res = mixture_dif(&theta, &x, 8, 0);
    assert!(res.classes >= 2 && res.bic_gain > 0.0, "{res:?}");
    assert!(
        res.dif.iter().all(|&d| d > 0.5),
        "at 0.5 every item is flagged: {:?}",
        res.dif
    );
    assert_eq!(latent_flags(&res), vec![false; 8]);
}
