//! Differential oracles on random datasets (`docs/08` REPRO-003, T45): the engine against
//! the paper's NumPy/SciPy implementations of the same models (`paper/scripts/common.py`,
//! driven by `sim/oracle_bridging.py` and `sim/oracle_mixture.py`) — not only on the
//! fixed fixtures. Needs the pinned sim environment (`sim/requirements.txt`, the repo
//! `.venv` or a `python3` that imports numpy/scipy); self-skips without it, so
//! `cargo test` stays green, and runs for real in CI, which installs it.
//!
//! Both sides run seeded multi-starts and keep the lowest objective. The engine may
//! find a better minimum than the oracle — never a worse one — and where the two
//! objectives agree the parameters agree too.

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use scoring::bridging::{fit, BridgingParams, Fit, Ratings};
use scoring::dif::{mixture_dif_with, MixtureParams};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn python() -> String {
    let venv = repo_root().join(".venv/bin/python");
    if venv.exists() {
        venv.to_string_lossy().into_owned()
    } else {
        "python3".into()
    }
}

fn sim_env_available() -> bool {
    Command::new(python())
        .args(["-c", "import numpy, scipy"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn normal(r: &mut ChaCha8Rng) -> f64 {
    let u1: f64 = 1.0 - r.gen::<f64>();
    let u2: f64 = r.gen();
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

fn write_matrix(path: &PathBuf, m: &[Vec<f64>]) {
    let text: String = m
        .iter()
        .map(|row| {
            row.iter()
                .map(|v| format!("{v:.17e}"))
                .collect::<Vec<_>>()
                .join(",")
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(path, text + "\n").unwrap();
}

fn read_oracle(path: &PathBuf) -> HashMap<String, f64> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter_map(|l| {
            let (k, v) = l.split_once(',')?;
            Some((k.to_string(), v.trim().parse::<f64>().ok()?))
        })
        .collect()
}

fn run_oracle(script: &str, dir: &PathBuf) -> HashMap<String, f64> {
    let out = Command::new(python())
        .arg(repo_root().join("sim").join(script))
        .arg(dir)
        .output()
        .expect("could not run the oracle script");
    assert!(
        out.status.success(),
        "{script} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    read_oracle(&dir.join("oracle.csv"))
}

/// The weighted objective of `docs/02` §A on a fit, recomputed from the public model.
fn objective(data: &Ratings, p: &BridgingParams, f: &Fit) -> f64 {
    let mut se = 0.0;
    for o in &data.obs {
        let e = f.mu + f.b_u[o.u] + f.b_j[o.j] + f.f_u[o.u] * f.f_j[o.j] - o.r;
        se += data.weights[o.u] * e * e;
    }
    let sq = |v: &[f64]| v.iter().map(|x| x * x).sum::<f64>();
    se + p.lam_b * (sq(&f.b_u) + sq(&f.b_j)) + p.lam_f * (sq(&f.f_u) + sq(&f.f_j))
}

/// A random two-camp dataset: `n` reviewers at `±1 + N(0, 0.2)`, `m` items with random
/// leans, ratings `0.7 + f_u·lean + N(0, 0.08)` clipped to `[0, 1]`, a random mask at
/// `density`, and either uniform or random weights (one of them zero).
fn dataset(seed: u64, n: usize, m: usize, density: f64, random_weights: bool) -> Ratings {
    let mut r = ChaCha8Rng::seed_from_u64(seed);
    let f_u: Vec<f64> = (0..n)
        .map(|u| if u < n / 2 { -1.0 } else { 1.0 } + 0.2 * normal(&mut r))
        .collect();
    let lean: Vec<f64> = (0..m).map(|_| 0.5 * normal(&mut r)).collect();
    let ratings: Vec<Vec<f64>> = f_u
        .iter()
        .map(|&fu| {
            (0..m)
                .map(|j| (0.7 + fu * lean[j] + 0.08 * normal(&mut r)).clamp(0.0, 1.0))
                .collect()
        })
        .collect();
    // Every reviewer rates at least one item; every item is rated at least once.
    let mut mask: Vec<Vec<bool>> = (0..n)
        .map(|_| (0..m).map(|_| r.gen::<f64>() < density).collect())
        .collect();
    for (u, row) in mask.iter_mut().enumerate() {
        if !row.iter().any(|&b| b) {
            row[u % m] = true;
        }
    }
    for j in 0..m {
        if !mask.iter().any(|row| row[j]) {
            mask[j % n][j] = true;
        }
    }
    let weights: Vec<f64> = if random_weights {
        (0..n)
            .map(|u| if u == 0 { 0.0 } else { r.gen_range(0.2..2.0) })
            .collect()
    } else {
        vec![1.0; n]
    };
    Ratings::from_dense(&ratings, &mask).with_weights(weights)
}

/// The bridging fit against the paper's SciPy fit on six random datasets: the engine's
/// objective is never worse than the oracle's (within 1e-6 relative), and where the
/// two agree the parameters agree to 1e-3 — `μ`, `b_j`, and `f_j` up to the sign the
/// engine canonicalizes.
#[test]
fn the_bridging_fit_matches_the_scipy_oracle_on_random_datasets() {
    if !sim_env_available() {
        eprintln!("SKIP differential_oracle: python3 with numpy/scipy is not available (sim/requirements.txt)");
        return;
    }
    let cases = [
        (1u64, 12usize, 5usize, 0.9, false),
        (2, 25, 8, 0.7, false),
        (3, 40, 10, 0.5, true),
        (4, 16, 12, 0.6, true),
        (5, 30, 6, 0.8, false),
        (6, 20, 9, 0.65, true),
    ];
    let p = BridgingParams::default();
    let mut same_basin = 0;
    for (seed, n, m, density, random_weights) in cases {
        let data = dataset(seed, n, m, density, random_weights);
        let dir =
            PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("oracle_bridging_{seed}"));
        fs::create_dir_all(&dir).unwrap();
        let mut r = vec![vec![0.0; m]; n];
        let mut mask = vec![vec![0.0; m]; n];
        for o in &data.obs {
            r[o.u][o.j] = o.r;
            mask[o.u][o.j] = 1.0;
        }
        write_matrix(&dir.join("R.csv"), &r);
        write_matrix(&dir.join("mask.csv"), &mask);
        write_matrix(&dir.join("weights.csv"), &[data.weights.clone()]);
        let oracle = run_oracle("oracle_bridging.py", &dir);
        let f = fit(&data, &p).unwrap();
        let (ours, theirs) = (objective(&data, &p, &f), oracle["objective"]);
        println!(
            "seed {seed} ({n}×{m}, density {density}, {}): objective {ours:.9} vs oracle {theirs:.9}",
            if random_weights { "random weights" } else { "uniform weights" }
        );
        assert!(
            ours <= theirs * (1.0 + 1e-6) + 1e-9,
            "seed {seed}: the engine stopped at a worse minimum ({ours} vs {theirs})"
        );
        if (ours - theirs).abs() <= 1e-6 * theirs.abs() {
            same_basin += 1;
            assert!((f.mu - oracle["mu"]).abs() < 1e-3, "seed {seed}: μ");
            for j in 0..m {
                assert!(
                    (f.b_j[j] - oracle[&format!("b_j[{j}]")]).abs() < 1e-3,
                    "seed {seed}: b_j[{j}] {} vs {}",
                    f.b_j[j],
                    oracle[&format!("b_j[{j}]")]
                );
            }
            // `f` is identified up to sign: align on the largest |f_j|.
            let lead = (0..m)
                .max_by(|&a, &b| f.f_j[a].abs().total_cmp(&f.f_j[b].abs()))
                .unwrap();
            let sign = if f.f_j[lead] * oracle[&format!("f_j[{lead}]")] < 0.0 {
                -1.0
            } else {
                1.0
            };
            for j in 0..m {
                assert!(
                    (f.f_j[j] - sign * oracle[&format!("f_j[{j}]")]).abs() < 1e-3,
                    "seed {seed}: f_j[{j}]"
                );
            }
        }
    }
    println!(
        "{same_basin} of {} datasets in the same basin as the oracle",
        cases.len()
    );
    assert!(
        same_basin >= 4,
        "only {same_basin} datasets in the same basin"
    );
}

/// The two-class uniform mixture on a fixed ability against the paper's SciPy fit on
/// four random batches: the one-class negative log-likelihoods agree to 1e-6 relative,
/// the engine's two-class uniform candidate is never worse than the oracle's (within
/// 1e-4 relative on the BIC), and where they agree the gaps are the oracle's `2|d|` to
/// 0.05.
#[test]
fn the_mixture_matches_the_scipy_oracle_on_random_batches() {
    if !sim_env_available() {
        eprintln!("SKIP differential_oracle: python3 with numpy/scipy is not available (sim/requirements.txt)");
        return;
    }
    let sigmoid = |z: f64| 1.0 / (1.0 + (-z).exp());
    let mut agree = 0;
    for (seed, n, k, n_biased) in [
        (11u64, 600usize, 6usize, 2usize),
        (12, 900, 8, 3),
        (13, 500, 5, 0),
        (14, 1200, 8, 4),
    ] {
        let mut r = ChaCha8Rng::seed_from_u64(seed);
        let theta: Vec<f64> = (0..n).map(|_| normal(&mut r)).collect();
        let z: Vec<f64> = (0..n)
            .map(|_| if r.gen::<bool>() { 1.0 } else { -1.0 })
            .collect();
        let a: Vec<f64> = (0..k).map(|_| r.gen_range(1.0..1.5)).collect();
        let b: Vec<f64> = (0..k).map(|_| 0.6 * normal(&mut r)).collect();
        let x: Vec<Vec<f64>> = theta
            .iter()
            .zip(&z)
            .map(|(&t, &zi)| {
                (0..k)
                    .map(|j| {
                        let d = if j < n_biased { 0.9 } else { 0.0 };
                        f64::from(r.gen::<f64>() < sigmoid(a[j] * (t - b[j] - d * zi)))
                    })
                    .collect()
            })
            .collect();
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("oracle_mixture_{seed}"));
        fs::create_dir_all(&dir).unwrap();
        write_matrix(
            &dir.join("theta.csv"),
            &theta.iter().map(|&t| vec![t]).collect::<Vec<_>>(),
        );
        write_matrix(&dir.join("X.csv"), &x);
        let oracle = run_oracle("oracle_mixture.py", &dir);
        let res = mixture_dif_with(
            &theta,
            &x,
            k,
            &MixtureParams {
                max_classes: 2,
                n_starts: 4,
                seed: 0,
            },
        );
        let ln_n = (n as f64).ln();
        let bic_of = |classes: usize, non_uniform: bool| {
            res.candidates
                .iter()
                .find(|c| c.0 == classes && c.1 == non_uniform)
                .map(|c| c.2)
                .expect("candidate fitted")
        };
        let (ours1, theirs1) = (
            bic_of(1, false),
            2.0 * oracle["null_nll"] + 2.0 * k as f64 * ln_n,
        );
        let (ours2, theirs2) = (
            bic_of(2, false),
            2.0 * oracle["mixture_nll"] + (1.0 + 3.0 * k as f64) * ln_n,
        );
        println!(
            "seed {seed} (N = {n}, K = {k}, {n_biased} biased): one class {ours1:.4} vs {theirs1:.4}; two classes {ours2:.4} vs {theirs2:.4}; selected {} class(es)",
            res.classes
        );
        assert!(
            (ours1 - theirs1).abs() <= 1e-6 * theirs1.abs(),
            "seed {seed}: the one-class fits differ"
        );
        assert!(
            ours2 <= theirs2 + 1e-4 * theirs2.abs(),
            "seed {seed}: the engine's two-class fit is worse ({ours2} vs {theirs2})"
        );
        if (ours2 - theirs2).abs() <= 1e-4 * theirs2.abs() && res.classes == 2 && !res.non_uniform {
            agree += 1;
            for j in 0..k {
                let gap = 2.0 * oracle[&format!("d[{j}]")].abs();
                assert!(
                    (res.dif[j] - gap).abs() < 0.05,
                    "seed {seed}: gap of item {j}: {} vs {gap}",
                    res.dif[j]
                );
            }
        }
    }
    println!("{agree} of 4 batches with the same two-class optimum");
    assert!(
        agree >= 2,
        "only {agree} batches agree on the two-class optimum"
    );
}
