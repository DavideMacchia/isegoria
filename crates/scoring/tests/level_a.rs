//! Level A acceptance tests, run on the same dataset as `sim/bridging_irt_dif.py`
//! (exported by `sim/export_fixtures.py`). Oracle: μ=0.7552, |corr(axis)|=0.9898,
//! b_j=[0.108, 0.081, -0.117, 0.092, 0.104, 0.080, 0.084, -0.155, -0.262, -0.020].

use scoring::bridging::{bridge_scores, fit, BridgingParams, Obs, Ratings};
use scoring::Convergence;
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

fn read_expected_bj() -> Vec<f64> {
    let path = fixtures_dir().join("expected_levelA.csv");
    let text = fs::read_to_string(&path).unwrap();
    text.lines()
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.split(',').nth(4).unwrap().trim().parse::<f64>().unwrap())
        .collect()
}

fn load_ratings() -> Ratings {
    let r = read_matrix("R.csv");
    let mask_num = read_matrix("mask.csv");
    let mask: Vec<Vec<bool>> = mask_num
        .iter()
        .map(|row| row.iter().map(|&v| v != 0.0).collect())
        .collect();
    Ratings::from_dense(&r, &mask)
}

fn pearson_abs(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len() as f64;
    let ma = a.iter().sum::<f64>() / n;
    let mb = b.iter().sum::<f64>() / n;
    let mut cov = 0.0;
    let mut va = 0.0;
    let mut vb = 0.0;
    for i in 0..a.len() {
        let da = a[i] - ma;
        let db = b[i] - mb;
        cov += da * db;
        va += da * da;
        vb += db * db;
    }
    (cov / (va.sqrt() * vb.sqrt())).abs()
}

const TAU: f64 = 0.08;

#[test]
fn fit_reproduces_oracle_on_identical_dataset() {
    let data = load_ratings();
    let expected = read_expected_bj();
    let true_f = read_vector("true_f.csv");

    let f = fit(&data, &BridgingParams::default()).unwrap();
    assert_eq!(f.status, Convergence::Converged);

    assert!((f.mu - 0.7552).abs() < 0.002, "mu = {:.4}", f.mu);

    let corr = pearson_abs(&true_f, &f.f_u);
    assert!(corr > 0.98, "axis recovery |corr| = {:.4}", corr);

    // Half the gate's uncertainty band ε = 0.008: a fit error the tolerance hid could
    // flip an item across τ (T41). Rust and SciPy agree to ~0.002 today.
    for (j, bj_exp) in expected.iter().enumerate() {
        assert!(
            (f.b_j[j] - bj_exp).abs() < 0.004,
            "b_j[{j}] = {:.4}, expected {:.4}",
            f.b_j[j],
            bj_exp
        );
    }
}

#[test]
fn asymmetric_regularization_separates_bridging_from_majority() {
    let data = load_ratings();
    let f = fit(&data, &BridgingParams::default()).unwrap();

    // Polarized items (large |f_j|) are rejected even when heavily voted.
    for &j in &[2usize, 7, 8, 9] {
        assert!(f.b_j[j] < TAU, "polarized item {j}: B_j = {:.4}", f.b_j[j]);
        assert!(
            f.f_j[j].abs() > 0.4,
            "item {j}: |f_j| = {:.3}",
            f.f_j[j].abs()
        );
    }
    // Cross-cutting quality items pass.
    for &j in &[0usize, 3, 4] {
        assert!(f.b_j[j] >= TAU, "neutral item {j}: B_j = {:.4}", f.b_j[j]);
        assert!(
            f.f_j[j].abs() < 0.4,
            "item {j}: |f_j| = {:.3}",
            f.f_j[j].abs()
        );
    }
}

#[test]
fn bootstrap_min_is_pessimistic() {
    let data = load_ratings();
    let p = BridgingParams::default();
    let full = fit(&data, &p).unwrap();
    let bridge = bridge_scores(&data, &p, 10, 0.85).unwrap();

    for (j, &b) in bridge.iter().enumerate() {
        assert!(
            b <= full.b_j[j] + 1e-6,
            "bridge[{j}]={:.4} full={:.4}",
            b,
            full.b_j[j]
        );
    }
    // "≤ full" holds trivially if the minimum is never updated (it starts at the full
    // fit): the subsamples must actually pull some scores down (T41).
    let lowered = (0..bridge.len())
        .filter(|&j| bridge[j] < full.b_j[j] - 1e-4)
        .count();
    assert!(lowered >= bridge.len() / 2, "only {lowered} scores lowered");
    for &j in &[2usize, 7, 8, 9] {
        assert!(bridge[j] < TAU, "bridge[{j}] = {:.4}", bridge[j]);
    }
    for &j in &[0usize, 4] {
        assert!(bridge[j] >= TAU, "bridge[{j}] = {:.4}", bridge[j]);
    }
}

#[test]
fn corner_case_bipartisan_corruption_cost() {
    // docs/06: with bridging, pushing a partisan item (idx 7) requires corrupting
    // the OPPOSING field. b_j must rise monotonically with the opposing-field count.
    let base = load_ratings();
    let true_f = read_vector("true_f.csv");
    let field_b: Vec<usize> = (0..base.n).filter(|&u| true_f[u] > 0.0).collect();
    let field_a: Vec<usize> = (0..base.n).filter(|&u| true_f[u] < 0.0).collect();
    let item = 7usize;
    let attackers_b: Vec<usize> = field_b.iter().copied().take(40).collect();

    let scored = |n_a: usize| -> f64 {
        let boosters: std::collections::HashSet<usize> = attackers_b
            .iter()
            .copied()
            .chain(field_a.iter().copied().take(n_a))
            .collect();
        let mut obs: Vec<Obs> = base
            .obs
            .iter()
            .copied()
            .filter(|o| !(o.j == item && boosters.contains(&o.u)))
            .collect();
        for &u in &boosters {
            obs.push(Obs { u, j: item, r: 1.0 });
        }
        let data = Ratings {
            n: base.n,
            m: base.m,
            obs,
            weights: base.weights.clone(),
        };
        fit(&data, &BridgingParams::default()).unwrap().b_j[item]
    };

    let (s0, s20, s40, s70) = (scored(0), scored(20), scored(40), scored(70));
    assert!(s0 < s20, "s0={s0:.3} s20={s20:.3}");
    assert!(s20 < s40, "s20={s20:.3} s40={s40:.3}");
    assert!(s40 < s70, "s40={s40:.3} s70={s70:.3}");
    assert!(s0 < TAU, "s0={s0:.3} should stay < τ");
    assert!(s70 > s0 + 0.2, "s0={s0:.3} s70={s70:.3}");
}

#[test]
fn an_empty_rating_matrix_has_no_reviewers_items_or_observations() {
    let data = Ratings::from_dense(&[], &[]);
    assert_eq!((data.n, data.m, data.obs.len()), (0, 0, 0));
    assert!(data.weights.is_empty());
}

/// T48: the verdict does not depend on the seed. With the default multi-start, fits from
/// disjoint seed sets agree on every `b_j` to 1e-4 and on the sign convention of `f`.
#[test]
fn bridging_scores_do_not_depend_on_the_seed() {
    let data = load_ratings();
    let base = fit(&data, &BridgingParams::default()).unwrap();
    for seed in 1..8u64 {
        let f = fit(
            &data,
            &BridgingParams {
                seed: seed * 1000,
                ..BridgingParams::default()
            },
        )
        .unwrap();
        for j in 0..data.m {
            assert!(
                (f.b_j[j] - base.b_j[j]).abs() < 1e-4,
                "seed {}: b_j[{j}] = {:.5} vs {:.5}",
                seed * 1000,
                f.b_j[j],
                base.b_j[j]
            );
            assert!(
                (f.f_j[j] - base.f_j[j]).abs() < 1e-3,
                "seed {}: f_j[{j}]",
                seed * 1000
            );
        }
    }
}

/// Why T48 needed the multi-start: on this very dataset a single start from seed 5 stops
/// in a worse local minimum (objective ~4× the best) that makes item 08 bridge
/// (`b_j` ≈ 1.08 instead of −0.26). The default fit from the same seed does not.
#[test]
fn a_single_start_can_land_in_a_worse_minimum() {
    let data = load_ratings();
    let single = fit(
        &data,
        &BridgingParams {
            seed: 5,
            n_starts: 1,
            ..BridgingParams::default()
        },
    )
    .unwrap();
    assert!(single.b_j[8] > 0.5, "b_j[8] = {:.3}", single.b_j[8]);
    let multi = fit(
        &data,
        &BridgingParams {
            seed: 5,
            ..BridgingParams::default()
        },
    )
    .unwrap();
    assert!(multi.b_j[8] < 0.0, "b_j[8] = {:.3}", multi.b_j[8]);
}

/// Everyone on probation (all weights 0): no rating counts, so the fit is the prior —
/// finite, with no NaN from a 0/0 start value.
#[test]
fn a_fit_with_every_weight_zero_is_finite() {
    let data = load_ratings();
    let n = data.n;
    let f = fit(&data.with_weights(vec![0.0; n]), &BridgingParams::default()).unwrap();
    assert!(f.mu.is_finite());
    for v in f.b_j.iter().chain(&f.f_j).chain(&f.b_u).chain(&f.f_u) {
        assert!(v.is_finite());
    }
}
