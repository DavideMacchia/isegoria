//! Monte-Carlo power check for the sample-size claim in `docs/02` §B.6 (a Rust port of
//! `sim/latent_dif_and_capacity.py`). Slow; ignored by default: `cargo test -p scoring
//! --test power -- --ignored --nocapture`.

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use scoring::dif::{mixture_dif, MIXTURE_DIF_MAX};

fn normal(rng: &mut ChaCha8Rng) -> f64 {
    let u1: f64 = 1.0 - rng.gen::<f64>();
    let u2: f64 = rng.gen::<f64>();
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

fn standardize(v: &[f64]) -> Vec<f64> {
    let n = v.len() as f64;
    let mean = v.iter().sum::<f64>() / n;
    let sd = (v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n).sqrt();
    v.iter().map(|x| (x - mean) / sd).collect()
}

/// Generates a batch of `k` items for `nt` respondents, `n_biased` of which are
/// distorted on a hidden ±1 axis; returns (θ estimated on anchors, responses).
fn generate(nt: usize, k: usize, n_biased: usize, seed: u64) -> (Vec<f64>, Vec<Vec<f64>>) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let theta: Vec<f64> = (0..nt).map(|_| normal(&mut rng)).collect();
    let edu: Vec<f64> = (0..nt)
        .map(|_| if rng.gen::<bool>() { 1.0 } else { -1.0 })
        .collect();

    let na = 30;
    let aa: Vec<f64> = (0..na).map(|_| 0.9 + 0.7 * rng.gen::<f64>()).collect();
    let ba: Vec<f64> = (0..na).map(|_| normal(&mut rng)).collect();
    let mut anchor_sum = vec![0.0; nt];
    for (i, s) in anchor_sum.iter_mut().enumerate() {
        for j in 0..na {
            let p = 1.0 / (1.0 + (-aa[j] * (theta[i] - ba[j])).exp());
            if rng.gen::<f64>() < p {
                *s += 1.0;
            }
        }
    }
    let th = standardize(&anchor_sum);

    let a: Vec<f64> = (0..k).map(|_| 1.0 + 0.5 * rng.gen::<f64>()).collect();
    let b: Vec<f64> = (0..k).map(|_| 0.6 * normal(&mut rng)).collect();
    let mut d = vec![0.0; k];
    for dj in d.iter_mut().take(n_biased) {
        *dj = 0.9;
    }
    let x: Vec<Vec<f64>> = (0..nt)
        .map(|i| {
            (0..k)
                .map(|j| {
                    let p = 1.0 / (1.0 + (-a[j] * (theta[i] - b[j] - d[j] * edu[i])).exp());
                    if rng.gen::<f64>() < p {
                        1.0
                    } else {
                        0.0
                    }
                })
                .collect()
        })
        .collect();
    (th, x)
}

fn detection_rate(nt: usize, seeds: u64) -> f64 {
    let (k, n_biased) = (8, 2);
    let mut hits = 0;
    for s in 0..seeds {
        let (th, x) = generate(nt, k, n_biased, 1000 + s);
        let res = mixture_dif(&th, &x, k, s);
        let biased = res.dif[..n_biased].iter().sum::<f64>() / n_biased as f64;
        let clean = res.dif[n_biased..].iter().sum::<f64>() / (k - n_biased) as f64;
        if biased > MIXTURE_DIF_MAX && clean < MIXTURE_DIF_MAX && res.classes >= 2 {
            hits += 1;
        }
    }
    hits as f64 / seeds as f64
}

#[test]
#[ignore = "slow Monte-Carlo; run with --ignored --nocapture"]
fn mixture_detection_improves_with_sample_size() {
    let seeds = 5;
    let r1500 = detection_rate(1500, seeds);
    let r3000 = detection_rate(3000, seeds);
    println!("latent-class DIF detection rate (2 of 8 biased): N=1500 -> {r1500:.2}, N=3000 -> {r3000:.2}");

    assert!(r3000 >= r1500, "a larger sample should not detect worse");
    assert!(
        r3000 >= 0.8,
        "N=3000 should reliably detect planted bias, got {r3000:.2} (supports docs/02 §B.6)"
    );
}
