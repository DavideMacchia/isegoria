//! The side-balanced bridge score is neutral to camp size and to the batch; the plain
//! item intercept is not, and each test checks that contrast too (D32; `docs/08`
//! AT-BR-08, AT-BR-09; paper §3.3–3.4, §7.1).

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use scoring::bridging::{fit, side_balanced, BridgingParams, Ratings};
use std::fs;
use std::path::PathBuf;

/// The provisional threshold on the side-balanced score (`protocol::gate::TAU`).
const TAU: f64 = 0.80;

fn normal(rng: &mut ChaCha8Rng) -> f64 {
    let u1: f64 = 1.0 - rng.gen::<f64>();
    let u2: f64 = rng.gen::<f64>();
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

/// The paper's mirror design (`paper/scripts/common.py::mirror_design`). Returns the
/// ratings and the leans.
fn mirror_design(
    n: usize,
    share_b: f64,
    seed: u64,
    m_consensus: usize,
    m_partisan: usize,
) -> (Ratings, Vec<f64>) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let n_b = (n as f64 * share_b).round() as usize;
    let true_f: Vec<f64> = (0..n)
        .map(|u| if u < n - n_b { -1.0 } else { 1.0 } + 0.25 * normal(&mut rng))
        .collect();
    let severity: Vec<f64> = (0..n).map(|_| 0.06 * normal(&mut rng)).collect();
    let m = m_consensus + m_partisan;
    let q: Vec<f64> = (0..m)
        .map(|j| {
            if j < m_consensus {
                0.82 + 0.06 * rng.gen::<f64>()
            } else {
                0.55
            }
        })
        .collect();
    let lean: Vec<f64> = (0..m)
        .map(|j| {
            if j < m_consensus {
                0.0
            } else if (j - m_consensus) % 2 == 0 {
                0.8
            } else {
                -0.8
            }
        })
        .collect();
    let per_reviewer = 9.min(m);
    let mut r = vec![vec![0.0; m]; n];
    let mut mask = vec![vec![false; m]; n];
    for u in 0..n {
        let mut items: Vec<usize> = (0..m).collect();
        for k in 0..per_reviewer {
            let pick = k + rng.gen_range(0..m - k);
            items.swap(k, pick);
            mask[u][items[k]] = true;
        }
        for j in 0..m {
            r[u][j] = (q[j] + 0.45 * true_f[u] * lean[j] + severity[u] + 0.07 * normal(&mut rng))
                .clamp(0.0, 1.0);
        }
    }
    (Ratings::from_dense(&r, &mask), lean)
}

/// The majoritarian leak of a score on a mirror design (paper §3.4): 1 means majority
/// rule, 0 means camp balance.
fn leak(scores: &[f64], lean: &[f64], share_b: f64) -> f64 {
    let mean = |sign: f64| {
        let v: Vec<f64> = scores
            .iter()
            .zip(lean)
            .filter(|(_, l)| **l * sign > 0.0)
            .map(|(s, _)| *s)
            .collect();
        v.iter().sum::<f64>() / v.len() as f64
    };
    (mean(1.0) - mean(-1.0)) / (2.0 * (2.0 * share_b - 1.0) * 0.36)
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn read_matrix(name: &str) -> Vec<Vec<f64>> {
    fs::read_to_string(fixtures_dir().join(name))
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.split(',').map(|c| c.trim().parse().unwrap()).collect())
        .collect()
}

/// The reference simulation's ratings and mask (`sim/export_fixtures.py`).
fn fixture() -> (Vec<Vec<f64>>, Vec<Vec<bool>>) {
    let r = read_matrix("R.csv");
    let mask = read_matrix("mask.csv")
        .iter()
        .map(|row| row.iter().map(|&v| v != 0.0).collect())
        .collect();
    (r, mask)
}

// -------------------------------- AT-BR-08 --------------------------------

/// AT-BR-08: the side-balanced leak stays within 0.1 of camp balance at 60/40 and 80/20
/// from 200 to 3,200 reviewers; at 50–100 reviewers it stays under 0.2 and under a third
/// of the intercept's leak.
#[test]
fn at_br_08_the_score_is_neutral_to_camp_size() {
    let p = BridgingParams::default();
    let designs: [(f64, &[usize], u64); 3] = [
        (0.6, &[200, 800], 3),
        (0.8, &[200, 800], 3),
        (0.6, &[3200], 1),
    ];
    for (share, sizes, seeds) in designs {
        for &n in sizes {
            let (mut leak_side, mut leak_intercept) = (0.0, 0.0);
            for seed in 0..seeds {
                let (data, lean) = mirror_design(n, share, seed, 10, 10);
                let f = fit(&data, &p).unwrap();
                leak_side += leak(&side_balanced(&f).score, &lean, share) / seeds as f64;
                leak_intercept += leak(&f.b_j, &lean, share) / seeds as f64;
            }
            assert!(
                leak_side.abs() <= 0.1,
                "camps {share}: n = {n}: side-balanced leak {leak_side:.3}"
            );
            assert!(
                leak_intercept > 0.4,
                "camps {share}: n = {n}: the intercept leaks only {leak_intercept:.3}"
            );
        }
    }
    for n in [50usize, 100] {
        let seeds = 6u64;
        let (mut leak_side, mut leak_intercept) = (0.0, 0.0);
        for seed in 0..seeds {
            let (data, lean) = mirror_design(n, 0.6, seed, 10, 10);
            let f = fit(&data, &p).unwrap();
            leak_side += leak(&side_balanced(&f).score, &lean, 0.6) / seeds as f64;
            leak_intercept += leak(&f.b_j, &lean, 0.6) / seeds as f64;
        }
        assert!(
            leak_side.abs() <= 0.2 && leak_side.abs() < leak_intercept / 3.0,
            "n = {n}: side-balanced leak {leak_side:.3} vs intercept {leak_intercept:.3}"
        );
    }
}

/// AT-BR-08: from 50/50 to 95/5 camps, the mirror items stay polarized and fail while
/// the eight consensus items score at least 0.75 (at most one miss at 95/5); the
/// side-balanced leak of the mirror gap stays within 0.1, the intercept's over 0.5.
#[test]
fn at_br_08_mirror_items_get_the_same_verdict_whatever_the_camp_sizes() {
    let p = BridgingParams::default();
    let (a, b) = (8usize, 9usize); // lean +0.8 (camp B's item) and −0.8 (camp A's)
    let measure = |share: f64| {
        let (data, _lean) = mirror_design(200, share, 7, 8, 2);
        let f = fit(&data, &p).unwrap();
        let s = side_balanced(&f);
        let extreme = !(0.1..=0.9).contains(&share);
        let missed = (0..8).filter(|&j| s.score[j] < TAU).count();
        assert!(
            missed <= usize::from(extreme),
            "camps {share}: {missed} consensus items below τ: {:?}",
            &s.score[..8]
        );
        for j in 0..8 {
            assert!(
                s.score[j] >= 0.75,
                "camps {share}: consensus item {j} scores {:.3}",
                s.score[j]
            );
        }
        assert!(
            s.score[a] < TAU && s.score[b] < TAU,
            "camps {share}: a mirror item passes ({:.3}, {:.3})",
            s.score[a],
            s.score[b]
        );
        assert!(
            s.gap[a] > 0.25 && s.gap[b] > 0.25,
            "camps {share}: the mirror items are polarized ({:.3}, {:.3})",
            s.gap[a],
            s.gap[b]
        );
        (s.score[a] - s.score[b], f.b_j[a] - f.b_j[b])
    };
    let (base_side, base_intercept) = measure(0.5);
    assert!(
        base_side.abs() < 0.05,
        "item noise at 50/50: {base_side:.3}"
    );
    for share in [0.4, 0.6, 0.2, 0.8, 0.05, 0.95] {
        let (diff_side, diff_intercept) = measure(share);
        // Positive when the majority's item scores higher.
        let full = 2.0 * (2.0 * share - 1.0) * 0.36;
        let leak_side = (diff_side - base_side) / full;
        let leak_intercept = (diff_intercept - base_intercept) / full;
        assert!(
            leak_side.abs() <= 0.1,
            "camps {share}: side-balanced leak {leak_side:.3} (difference {diff_side:.3})"
        );
        assert!(
            leak_intercept > 0.5,
            "camps {share}: the intercept leaks only {leak_intercept:.3}"
        );
    }
}

// -------------------------------- AT-BR-09 --------------------------------

/// AT-BR-09: ten weak decoy items move no score by more than 0.02 and change no verdict;
/// six consensus items scored alone stay within 0.02 too. The intercept moves by more
/// than 0.1 next to the decoys.
#[test]
fn at_br_09_decoys_and_the_batch_do_not_move_the_score() {
    let (r, mask) = fixture();
    let p = BridgingParams::default();
    let base_fit = fit(&Ratings::from_dense(&r, &mask), &p).unwrap();
    let base = side_balanced(&base_fit);
    let m = r[0].len();

    let mut rng = ChaCha8Rng::seed_from_u64(99);
    let (mut r_decoys, mut mask_decoys) = (r.clone(), mask.clone());
    for u in 0..r.len() {
        let severity = 0.06 * normal(&mut rng);
        for _ in 0..10 {
            r_decoys[u].push((0.30 + severity + 0.07 * normal(&mut rng)).clamp(0.0, 1.0));
            mask_decoys[u].push(rng.gen::<f64>() < 0.9);
        }
    }
    let decoys_fit = fit(&Ratings::from_dense(&r_decoys, &mask_decoys), &p).unwrap();
    let with_decoys = side_balanced(&decoys_fit);
    for j in 0..m {
        assert!(
            (with_decoys.score[j] - base.score[j]).abs() <= 0.02,
            "item {j}: {:.4} in the batch, {:.4} next to the decoys",
            base.score[j],
            with_decoys.score[j]
        );
        assert_eq!(
            with_decoys.score[j] >= TAU,
            base.score[j] >= TAU,
            "verdict of item {j}"
        );
    }
    let intercept_move = (0..m)
        .map(|j| (decoys_fit.b_j[j] - base_fit.b_j[j]).abs())
        .fold(0.0_f64, f64::max);
    assert!(
        intercept_move > 0.1,
        "the intercept moved by at most {intercept_move:.3} next to the decoys"
    );

    let consensus = [0usize, 1, 3, 4, 5, 6];
    let r_alone: Vec<Vec<f64>> = r
        .iter()
        .map(|row| consensus.iter().map(|&j| row[j]).collect())
        .collect();
    let mask_alone: Vec<Vec<bool>> = mask
        .iter()
        .map(|row| consensus.iter().map(|&j| row[j]).collect())
        .collect();
    let alone = side_balanced(&fit(&Ratings::from_dense(&r_alone, &mask_alone), &p).unwrap());
    for (i, &j) in consensus.iter().enumerate() {
        assert!(
            (alone.score[i] - base.score[j]).abs() <= 0.02,
            "item {j}: {:.4} in the batch, {:.4} alone",
            base.score[j],
            alone.score[i]
        );
    }
}
