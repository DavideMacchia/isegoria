//! The production latent re-check reads the quantity `docs/02` §B.3 specifies and acts
//! only on a trustworthy fit (T35, `docs/08` DIF-006).
//!
//! - `dif` is the b-gap `|b⁺ − b⁻| = 2|δ|`, not the half-gap the code used to threshold;
//! - a fit that did not converge, or whose BIC does not favour two classes, flags nothing;
//! - the threshold is the provisional 1.0 on the b-gap: the literature 0.5 would retire
//!   every clean item of the one-biased-item fixture.

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

fn fit(dif: Vec<f64>, bic: f64, status: Convergence) -> MixtureDif {
    MixtureDif {
        pi: 0.5,
        dif,
        class_posterior: Vec::new(),
        lr: 0.0,
        bic,
        status,
    }
}

#[test]
fn a_trustworthy_fit_flags_exactly_the_items_above_the_threshold() {
    let t = MIXTURE_DIF_MAX;
    let res = fit(
        vec![0.0, t - 1e-9, t, t + 1e-9, 2.0 * t],
        10.0,
        Convergence::Converged,
    );
    assert_eq!(latent_flags(&res), vec![false, false, false, true, true]);
}

#[test]
fn a_fit_that_did_not_converge_flags_nothing() {
    let dif = vec![0.2, 3.0, 3.0];
    for status in [Convergence::MaxIters, Convergence::LineSearchFailed] {
        assert_eq!(
            latent_flags(&fit(dif.clone(), 50.0, status)),
            vec![false; 3],
            "{status:?}"
        );
    }
    // Contrast: the same gaps on a converged fit are flagged.
    assert_eq!(
        latent_flags(&fit(dif, 50.0, Convergence::Converged)),
        vec![false, true, true]
    );
}

#[test]
fn a_fit_whose_bic_does_not_favour_two_classes_flags_nothing() {
    for bic in [0.0, -1.2, -500.0] {
        assert_eq!(
            latent_flags(&fit(vec![3.0, 3.0], bic, Convergence::Converged)),
            vec![false, false],
            "bic = {bic}"
        );
    }
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
    assert_eq!(
        revalidate_pool_latent(&theta, &x, 0),
        vec![true, true, true, false, false, false, false, false]
    );
}

/// Why the threshold is provisional. On the one-biased-item fixture the mixture still
/// prefers two classes (BIC > 0) and every item's gap sits in 0.5–1.0: the literature
/// cut-off would retire all eight, seven of them clean. The stated 1.0 retires none —
/// the lone biased item stays invisible (INV-8), which is the documented limit.
#[test]
fn the_literature_threshold_would_retire_every_item_of_the_single_bias_fixture() {
    let theta = read_vector("mixture_single_theta.csv");
    let x = read_matrix("mixture_single_X.csv");
    let res = mixture_dif(&theta, &x, 8, 0);
    assert!(res.bic > 0.0, "bic = {:.1}", res.bic);
    assert!(
        res.dif.iter().all(|&d| d > 0.5),
        "at 0.5 every item is flagged: {:?}",
        res.dif
    );
    assert_eq!(latent_flags(&res), vec![false; 8]);
}
