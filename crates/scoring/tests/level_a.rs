//! Level A acceptance tests, run on the same dataset as `sim/bridging_irt_dif.py`
//! (exported by `sim/export_fixtures.py`), checked against its oracle output.

use scoring::bridging::{bridge_scores, fit, side_balanced, BridgingParams, Obs, Ratings, Side};
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

/// Column `col` of `expected_levelA.csv`: 4 = `bj_full`, 6..=9 = `side_a_full`,
/// `side_b_full`, `side_full`, `gap_full`.
fn read_expected(col: usize) -> Vec<f64> {
    let path = fixtures_dir().join("expected_levelA.csv");
    let text = fs::read_to_string(&path).unwrap();
    text.lines()
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            l.split(',')
                .nth(col)
                .unwrap()
                .trim()
                .parse::<f64>()
                .unwrap()
        })
        .collect()
}

fn read_expected_bj() -> Vec<f64> {
    read_expected(4)
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

/// The provisional threshold on the side-balanced score (`docs/02` §A.3, D32;
/// `protocol::gate::TAU`).
const TAU: f64 = 0.80;

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

    // Tolerance below half the gate's band (`protocol::gate::EPS`), so a hidden fit error
    // cannot move an item across τ.
    for (j, bj_exp) in expected.iter().enumerate() {
        assert!(
            (f.b_j[j] - bj_exp).abs() < 0.004,
            "b_j[{j}] = {:.4}, expected {:.4}",
            f.b_j[j],
            bj_exp
        );
    }

    // The oracle's side 0 is whichever side its sign-arbitrary fit put low, so the two
    // side means are compared as an unordered pair (D32).
    let sides = side_balanced(&f);
    let (exp_a, exp_b) = (read_expected(6), read_expected(7));
    let (exp_score, exp_gap) = (read_expected(8), read_expected(9));
    for j in 0..data.m {
        let (lo, hi) = (
            sides.side_a[j].min(sides.side_b[j]),
            sides.side_a[j].max(sides.side_b[j]),
        );
        let (exp_lo, exp_hi) = (exp_a[j].min(exp_b[j]), exp_a[j].max(exp_b[j]));
        assert!(
            (lo - exp_lo).abs() < 0.004,
            "side lo[{j}] = {lo:.4} vs {exp_lo:.4}"
        );
        assert!(
            (hi - exp_hi).abs() < 0.004,
            "side hi[{j}] = {hi:.4} vs {exp_hi:.4}"
        );
        assert!(
            (sides.score[j] - exp_score[j]).abs() < 0.004,
            "S_j[{j}] = {:.4}, expected {:.4}",
            sides.score[j],
            exp_score[j]
        );
        assert!(
            (sides.gap[j] - exp_gap[j]).abs() < 0.008,
            "gap[{j}] = {:.4}, expected {:.4}",
            sides.gap[j],
            exp_gap[j]
        );
    }
    let n_a = sides.side.iter().filter(|s| **s == Side::A).count();
    assert!(
        (n_a, data.n - n_a) == (80, 120) || (n_a, data.n - n_a) == (120, 80),
        "sides of {n_a} and {}",
        data.n - n_a
    );
    for (u, s) in sides.side.iter().enumerate() {
        let camp_b = true_f[u] > 0.0;
        let side_b = *s == Side::B;
        // Whichever orientation, every reviewer of a camp is on the same side.
        assert_eq!(
            camp_b == side_b,
            (true_f[0] > 0.0) == (sides.side[0] == Side::B)
        );
    }
}

#[test]
fn asymmetric_regularization_separates_bridging_from_majority() {
    let data = load_ratings();
    let f = fit(&data, &BridgingParams::default()).unwrap();
    let sides = side_balanced(&f);

    // Polarized items: one side likes them and the other does not — a wide gap and a
    // score below τ whichever side is the majority (D32) — and a large axis loading.
    for &j in &[2usize, 7, 8, 9] {
        assert!(
            sides.score[j] < TAU,
            "polarized item {j}: S_j = {:.4}",
            sides.score[j]
        );
        assert!(sides.gap[j] > 0.25, "item {j}: gap = {:.3}", sides.gap[j]);
        assert!(
            f.f_j[j].abs() > 0.4,
            "item {j}: |f_j| = {:.3}",
            f.f_j[j].abs()
        );
    }
    // Cross-cutting quality items: both sides approve, so the sides agree and the score
    // is the approval itself.
    for &j in &[0usize, 3, 4] {
        assert!(
            sides.score[j] >= TAU,
            "neutral item {j}: S_j = {:.4}",
            sides.score[j]
        );
        assert!(sides.gap[j] < 0.1, "item {j}: gap = {:.3}", sides.gap[j]);
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
    let full = side_balanced(&fit(&data, &p).unwrap());
    let bridge = bridge_scores(&data, &p, 10, 0.85).unwrap();
    assert_eq!(
        bridge.full, full,
        "the full fit's side scores travel with the robust ones"
    );

    for (j, &b) in bridge.robust.iter().enumerate() {
        assert!(
            b <= full.score[j] + 1e-6,
            "robust[{j}]={:.4} full={:.4}",
            b,
            full.score[j]
        );
    }
    // "≤ full" holds trivially if the minimum is never updated (it starts at the full
    // fit): the subsamples must actually pull some scores down.
    let lowered = (0..bridge.robust.len())
        .filter(|&j| bridge.robust[j] < full.score[j] - 1e-4)
        .count();
    assert!(
        lowered >= bridge.robust.len() / 2,
        "only {lowered} scores lowered"
    );
    for &j in &[2usize, 7, 8, 9] {
        assert!(
            bridge.robust[j] < TAU,
            "robust[{j}] = {:.4}",
            bridge.robust[j]
        );
    }
    for &j in &[0usize, 4] {
        assert!(
            bridge.robust[j] >= TAU,
            "robust[{j}] = {:.4}",
            bridge.robust[j]
        );
    }
}

/// Corner case (`docs/06`): raising a partisan item's score requires corrupting the
/// opposing field; the score rises monotonically with the opposing-field count.
#[test]
fn corner_case_bipartisan_corruption_cost() {
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
            axis: base.axis.clone(),
        };
        side_balanced(&fit(&data, &BridgingParams::default()).unwrap()).score[item]
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

/// The verdict does not depend on the seed: with the default multi-start, fits from
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
        let (s_base, s_f) = (side_balanced(&base), side_balanced(&f));
        for j in 0..data.m {
            assert!(
                (f.b_j[j] - base.b_j[j]).abs() < 1e-4,
                "seed {}: b_j[{j}] = {:.5} vs {:.5}",
                seed * 1000,
                f.b_j[j],
                base.b_j[j]
            );
            assert!(
                (s_f.score[j] - s_base.score[j]).abs() < 1e-4,
                "seed {}: S_j[{j}] = {:.5} vs {:.5}",
                seed * 1000,
                s_f.score[j],
                s_base.score[j]
            );
            assert!(
                (f.f_j[j] - base.f_j[j]).abs() < 1e-3,
                "seed {}: f_j[{j}]",
                seed * 1000
            );
        }
    }
}

/// Why the multi-start matters: from seed 5, a single start lands in a worse local
/// minimum that makes item 8 bridge (`b_j > 0`); the default multi-start does not.
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
    let sides = side_balanced(&f);
    for v in sides.score.iter().chain(&sides.gap) {
        assert!(v.is_finite());
    }
}
