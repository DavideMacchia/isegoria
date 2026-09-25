//! Determinism (CLAUDE.md, invariant #7): the engine returns identical output,
//! bit-for-bit, for identical input across repeated runs.

use scoring::bridging::{bridge_scores, fit, BridgingParams, Ratings};
use scoring::dif::mixture_dif;
use std::fs;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn read_matrix(name: &str) -> Vec<Vec<f64>> {
    let text = fs::read_to_string(fixtures_dir().join(name)).unwrap();
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
    read_matrix(name).into_iter().map(|r| r[0]).collect()
}

fn ratings() -> Ratings {
    let r = read_matrix("R.csv");
    let mask: Vec<Vec<bool>> = read_matrix("mask.csv")
        .iter()
        .map(|row| row.iter().map(|&v| v != 0.0).collect())
        .collect();
    Ratings::from_dense(&r, &mask)
}

#[test]
fn bridging_fit_is_bit_for_bit_reproducible() {
    let data = ratings();
    let p = BridgingParams::default();
    let a = fit(&data, &p).unwrap();
    let b = fit(&data, &p).unwrap();
    assert_eq!(a.mu.to_bits(), b.mu.to_bits());
    assert_eq!(bits(&a.b_j), bits(&b.b_j));
    assert_eq!(bits(&a.f_j), bits(&b.f_j));
    assert_eq!(bits(&a.b_u), bits(&b.b_u));
    assert_eq!(bits(&a.f_u), bits(&b.f_u));
}

#[test]
fn bridge_scores_are_bit_for_bit_reproducible() {
    let data = ratings();
    let p = BridgingParams::default();
    let a = bridge_scores(&data, &p, 10, 0.85).unwrap();
    let b = bridge_scores(&data, &p, 10, 0.85).unwrap();
    assert_eq!(bits(&a.robust), bits(&b.robust));
    assert_eq!(bits(&a.full.score), bits(&b.full.score));
    assert_eq!(bits(&a.full.gap), bits(&b.full.gap));
    assert_eq!(a.full.side, b.full.side);
}

#[test]
fn mixture_dif_is_bit_for_bit_reproducible() {
    let theta = read_vector("mixture_batch_theta.csv");
    let x = read_matrix("mixture_batch_X.csv");
    let a = mixture_dif(&theta, &x, 8, 0);
    let b = mixture_dif(&theta, &x, 8, 0);
    assert_eq!(bits(&a.dif), bits(&b.dif));
    assert_eq!(a.bic_gain.to_bits(), b.bic_gain.to_bits());
    assert_eq!(bits(&a.pi), bits(&b.pi));
    assert_eq!(bits(&a.posterior.concat()), bits(&b.posterior.concat()));
}

fn bits(v: &[f64]) -> Vec<u64> {
    v.iter().map(|x| x.to_bits()).collect()
}
