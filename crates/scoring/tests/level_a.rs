//! Level A acceptance tests, run on the same dataset as `sim/bridging_irt_dif.py`
//! (exported by `sim/export_fixtures.py`). Oracle: μ=0.7552, |corr(axis)|=0.9898,
//! b_j=[0.108, 0.081, -0.117, 0.092, 0.104, 0.080, 0.084, -0.155, -0.262, -0.020].

use scoring::bridging::{bridge_scores, fit, BridgingParams, Obs, Ratings};
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

    let f = fit(&data, &BridgingParams::default());

    assert!((f.mu - 0.7552).abs() < 0.02, "mu = {:.4}", f.mu);

    let corr = pearson_abs(&true_f, &f.f_u);
    assert!(corr > 0.98, "axis recovery |corr| = {:.4}", corr);

    for (j, bj_exp) in expected.iter().enumerate() {
        assert!(
            (f.b_j[j] - bj_exp).abs() < 0.03,
            "b_j[{j}] = {:.4}, expected {:.4}",
            f.b_j[j],
            bj_exp
        );
    }
}

#[test]
fn asymmetric_regularization_separates_bridging_from_majority() {
    let data = load_ratings();
    let f = fit(&data, &BridgingParams::default());

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
    let full = fit(&data, &p);
    let bridge = bridge_scores(&data, &p, 10, 0.85);

    for (j, &b) in bridge.iter().enumerate() {
        assert!(
            b <= full.b_j[j] + 1e-6,
            "bridge[{j}]={:.4} full={:.4}",
            b,
            full.b_j[j]
        );
    }
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
        };
        fit(&data, &BridgingParams::default()).b_j[item]
    };

    let (s0, s20, s40, s70) = (scored(0), scored(20), scored(40), scored(70));
    assert!(s0 < s20, "s0={s0:.3} s20={s20:.3}");
    assert!(s20 < s40, "s20={s20:.3} s40={s40:.3}");
    assert!(s40 < s70, "s40={s40:.3} s70={s70:.3}");
    assert!(s0 < TAU, "s0={s0:.3} should stay < τ");
    assert!(s70 > s0 + 0.2, "s0={s0:.3} s70={s70:.3}");
}
