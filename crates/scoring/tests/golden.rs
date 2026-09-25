//! Golden outputs (invariant #7, `docs/08` REPRO-002): every value the engine returns on
//! the fixture datasets, compared bit-for-bit against `fixtures/golden_bits.txt`. An
//! intended change regenerates it: `ISEGORIA_UPDATE_GOLDEN=1 cargo test -p scoring --test golden`.

use scoring::bridging::{bridge_scores, fit, BridgingParams, Ratings};
use scoring::dif::mixture_dif;
use scoring::latent::{latent_dif_with, LatentParams};
use std::fs;
use std::path::PathBuf;

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

fn read_vector(name: &str) -> Vec<f64> {
    read_matrix(name).into_iter().map(|r| r[0]).collect()
}

/// FNV-1a over the IEEE-754 bits: one line for a long vector.
fn digest(v: &[f64]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for x in v {
        for b in x.to_bits().to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    h
}

/// `name,index,bits` rows: short vectors value by value, long ones as a digest.
fn record(rows: &mut Vec<String>, name: &str, v: &[f64]) {
    if v.len() <= 16 {
        for (i, x) in v.iter().enumerate() {
            rows.push(format!("{name},{i},{:016x}", x.to_bits()));
        }
    } else {
        rows.push(format!("{name},digest,{:016x}", digest(v)));
    }
}

/// A seeded batch for the target model: generated here, since the fixtures carry no
/// anchor responses. Returns (anchors, responses).
fn latent_batch(seed: u64) -> (Vec<Vec<f64>>, Vec<Vec<f64>>) {
    use rand::{Rng, SeedableRng};
    use rand_chacha::ChaCha8Rng;
    let (n, na, k) = (1500, 20, 8);
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let normal = |rng: &mut ChaCha8Rng| {
        let u1: f64 = 1.0 - rng.gen::<f64>();
        let u2: f64 = rng.gen();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    };
    let sigmoid = |z: f64| 1.0 / (1.0 + (-z).exp());
    let theta: Vec<f64> = (0..n).map(|_| normal(&mut rng)).collect();
    let z: Vec<f64> = (0..n)
        .map(|_| if rng.gen::<bool>() { 1.0 } else { -1.0 })
        .collect();
    let a_anchor: Vec<f64> = (0..na).map(|_| rng.gen_range(0.9..1.6)).collect();
    let b_anchor: Vec<f64> = (0..na).map(|_| normal(&mut rng)).collect();
    let anchors = theta
        .iter()
        .map(|&t| {
            (0..na)
                .map(|j| f64::from(rng.gen::<f64>() < sigmoid(a_anchor[j] * (t - b_anchor[j]))))
                .collect()
        })
        .collect();
    let a: Vec<f64> = (0..k).map(|_| rng.gen_range(1.0..1.5)).collect();
    let b: Vec<f64> = (0..k).map(|_| 0.6 * normal(&mut rng)).collect();
    let responses = theta
        .iter()
        .zip(&z)
        .map(|(&t, &zi)| {
            (0..k)
                .map(|j| {
                    let d = if j < 2 { 0.9 } else { 0.0 };
                    f64::from(rng.gen::<f64>() < sigmoid(a[j] * (t - b[j] - d * zi)))
                })
                .collect()
        })
        .collect();
    (anchors, responses)
}

fn current() -> Vec<String> {
    let mut rows = Vec::new();

    let r = read_matrix("R.csv");
    let mask: Vec<Vec<bool>> = read_matrix("mask.csv")
        .iter()
        .map(|row| row.iter().map(|&v| v != 0.0).collect())
        .collect();
    let data = Ratings::from_dense(&r, &mask);
    let p = BridgingParams::default();
    let f = fit(&data, &p).unwrap();
    record(&mut rows, "fit.mu", &[f.mu]);
    record(&mut rows, "fit.b_j", &f.b_j);
    record(&mut rows, "fit.f_j", &f.f_j);
    record(&mut rows, "fit.b_u", &f.b_u);
    record(&mut rows, "fit.f_u", &f.f_u);
    let bridge = bridge_scores(&data, &p, 10, 0.85).unwrap();
    record(&mut rows, "bridge.robust", &bridge.robust);
    record(&mut rows, "bridge.score", &bridge.full.score);
    record(&mut rows, "bridge.gap", &bridge.full.gap);
    record(&mut rows, "bridge.side_a", &bridge.full.side_a);
    record(&mut rows, "bridge.side_b", &bridge.full.side_b);
    let sides: Vec<f64> = bridge.full.side.iter().map(|s| *s as u8 as f64).collect();
    record(&mut rows, "bridge.side", &sides);

    for set in ["batch", "single"] {
        let theta = read_vector(&format!("mixture_{set}_theta.csv"));
        let x = read_matrix(&format!("mixture_{set}_X.csv"));
        let res = mixture_dif(&theta, &x, 8, 0);
        let tag = format!("mixture_{set}");
        let shape = [res.classes as f64, res.non_uniform as i32 as f64];
        record(&mut rows, &format!("{tag}.model"), &shape);
        record(&mut rows, &format!("{tag}.pi"), &res.pi);
        record(&mut rows, &format!("{tag}.dif"), &res.dif);
        record(&mut rows, &format!("{tag}.a_gap"), &res.a_gap);
        record(&mut rows, &format!("{tag}.differential"), &res.differential);
        record(&mut rows, &format!("{tag}.bic_gain"), &[res.bic_gain]);
        record(
            &mut rows,
            &format!("{tag}.posterior"),
            &res.posterior.concat(),
        );
    }

    // The target model (D37): the anchors inside the likelihood, θ integrated out.
    let (anchors, x) = latent_batch(54);
    let res = latent_dif_with(
        &anchors,
        &x,
        &LatentParams {
            n_starts: 2,
            max_classes: 2,
            ..LatentParams::default()
        },
    );
    let shape = [res.classes as f64, res.non_uniform as i32 as f64];
    record(&mut rows, "latent.model", &shape);
    record(&mut rows, "latent.pi", &res.pi);
    record(&mut rows, "latent.eta", &res.eta);
    record(&mut rows, "latent.dif", &res.dif);
    record(&mut rows, "latent.a_gap", &res.a_gap);
    record(&mut rows, "latent.anchor_a", &res.anchor_a);
    record(&mut rows, "latent.anchor_b", &res.anchor_b);
    record(&mut rows, "latent.item_b", &res.item_b.concat());
    record(&mut rows, "latent.bic_gain", &[res.bic_gain]);
    record(&mut rows, "latent.posterior", &res.posterior.concat());
    rows
}

#[test]
fn engine_outputs_match_the_golden_bits() {
    let path = fixtures_dir().join("golden_bits.txt");
    let now = current();
    if std::env::var_os("ISEGORIA_UPDATE_GOLDEN").is_some() {
        fs::write(&path, now.join("\n") + "\n").unwrap();
        return;
    }
    let want: Vec<String> = fs::read_to_string(&path)
        .expect("golden_bits.txt missing: regenerate with ISEGORIA_UPDATE_GOLDEN=1")
        .lines()
        .map(str::to_owned)
        .collect();
    let moved: Vec<String> = want
        .iter()
        .zip(&now)
        .filter(|(w, n)| w != n)
        .map(|(w, n)| format!("  want {w}\n  got  {n}"))
        .collect();
    assert!(
        moved.is_empty() && want.len() == now.len(),
        "{} of {} golden values moved (or the row count changed: {} vs {}):\n{}",
        moved.len(),
        want.len(),
        want.len(),
        now.len(),
        moved.join("\n")
    );
}
