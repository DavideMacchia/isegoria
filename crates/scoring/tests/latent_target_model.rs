//! The target model of `docs/01` D37: the anchors inside the likelihood and θ integrated
//! out, fixing the spurious latent classes a θ proxy creates (paper Prop. 10, `docs/08`
//! DIF-010, AT-DIF-01, AT-DIF-12). Paper-scale cases run under `calibration`.

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use scoring::dif::{mixture_dif, MIXTURE_DIF_MAX};
use scoring::irt::{kr20, theta_from_anchors};
#[cfg(feature = "calibration")]
use scoring::latent::latent_dif;
use scoring::latent::{latent_dif_with, LatentParams};
use std::time::Instant;

const K: usize = 8;

fn normal(r: &mut ChaCha8Rng) -> f64 {
    let u1: f64 = 1.0 - r.gen::<f64>();
    let u2: f64 = r.gen();
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

fn sigmoid(z: f64) -> f64 {
    1.0 / (1.0 + (-z).exp())
}

/// The paper's batch (`paper/scripts/common.py::dif_generate`): `n_anchor` clean anchors
/// and `K` trial items over a 2PL, the first `n_biased` trial items shifted by `δ·z` on a
/// hidden axis `z = ±1`. Returns (anchors, responses), both respondents × items.
fn batch(
    n: usize,
    n_anchor: usize,
    n_biased: usize,
    delta: f64,
    seed: u64,
) -> (Vec<Vec<f64>>, Vec<Vec<f64>>) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let theta: Vec<f64> = (0..n).map(|_| normal(&mut rng)).collect();
    let z: Vec<f64> = (0..n)
        .map(|_| if rng.gen::<bool>() { 1.0 } else { -1.0 })
        .collect();
    let a_anchor: Vec<f64> = (0..n_anchor).map(|_| rng.gen_range(0.9..1.6)).collect();
    let b_anchor: Vec<f64> = (0..n_anchor).map(|_| normal(&mut rng)).collect();
    let anchors: Vec<Vec<f64>> = theta
        .iter()
        .map(|&t| {
            (0..n_anchor)
                .map(|j| f64::from(rng.gen::<f64>() < sigmoid(a_anchor[j] * (t - b_anchor[j]))))
                .collect()
        })
        .collect();
    let a: Vec<f64> = (0..K).map(|_| rng.gen_range(1.0..1.5)).collect();
    let b: Vec<f64> = (0..K).map(|_| 0.6 * normal(&mut rng)).collect();
    let responses: Vec<Vec<f64>> = theta
        .iter()
        .zip(&z)
        .map(|(&t, &zi)| {
            (0..K)
                .map(|j| {
                    let d = if j < n_biased { delta } else { 0.0 };
                    f64::from(rng.gen::<f64>() < sigmoid(a[j] * (t - b[j] - d * zi)))
                })
                .collect()
        })
        .collect();
    (anchors, responses)
}

fn max(v: &[f64]) -> f64 {
    v.iter().copied().fold(f64::NEG_INFINITY, f64::max)
}

fn rounded(v: &[f64]) -> Vec<f64> {
    v.iter().map(|x| (x * 100.0).round() / 100.0).collect()
}

/// The paper's null batches with 10 and 20 anchors at N = 6,000: the proxy model selects
/// a mixture and flags clean items, the target model selects one class and flags none.
#[test]
fn the_target_model_finds_no_mixture_where_the_proxy_did() {
    for n_anchor in [10usize, 20] {
        let (anchors, x) = batch(6000, n_anchor, 0, 0.0, 1300);
        let t0 = Instant::now();
        let res = latent_dif_with(
            &anchors,
            &x,
            &LatentParams {
                n_starts: 2,
                max_classes: 2,
                ..LatentParams::default()
            },
        );
        let secs = t0.elapsed().as_secs_f64();
        let proxy = mixture_dif(&theta_from_anchors(&anchors), &x, K, 0);
        println!(
            "{n_anchor} anchors (KR-20 {:.3}): target model {} class(es), bic gain {:+.1}, max gap {:.3}, {:?}, {secs:.1}s; proxy model {} class(es), bic gain {:+.1}, max gap {:.3}, {} flag(s)",
            kr20(&anchors),
            res.classes,
            res.bic_gain,
            max(&res.dif),
            res.status,
            proxy.classes,
            proxy.bic_gain,
            max(&proxy.dif),
            proxy.dif.iter().filter(|&&d| d > MIXTURE_DIF_MAX).count()
        );
        assert!(
            res.flags(MIXTURE_DIF_MAX).iter().all(|&f| !f),
            "{n_anchor} anchors: a clean item flagged, gaps {:?}",
            res.dif
        );
        assert_eq!(res.classes, 1, "{n_anchor} anchors: a spurious mixture");
        assert!(
            proxy.classes >= 2,
            "{n_anchor} anchors: the proxy model no longer finds the spurious mixture"
        );
    }
}

/// AT-DIF-12, the inverting case: 6 of 8 items shifted the same way (δ = 0.9, 30
/// anchors, N = 6,000) — the batch's common shift *is* the campaign, so the proxy's
/// differential gap accused the two clean items; the target model flags exactly the six.
#[test]
fn at_dif_12_the_campaign_that_inverted_the_differential_gap_is_flagged_on_the_shifted_items() {
    let (anchors, x) = batch(6000, 30, 6, 0.9, 700);
    let t0 = Instant::now();
    let res = latent_dif_with(
        &anchors,
        &x,
        &LatentParams {
            n_starts: 2,
            max_classes: 3,
            ..LatentParams::default()
        },
    );
    println!(
        "6 of 8 biased: {} class(es), pi {:?}, eta {:?}, gaps {:?}, {:?}, {:.1}s",
        res.classes,
        rounded(&res.pi),
        rounded(&res.eta),
        rounded(&res.dif),
        res.status,
        t0.elapsed().as_secs_f64()
    );
    let expected: Vec<bool> = (0..K).map(|j| j < 6).collect();
    assert_eq!(res.flags(MIXTURE_DIF_MAX), expected, "gaps {:?}", res.dif);
}

/// The paper's Table on the target model: null batches with 10, 20, 30 and 60 anchors
/// at N = 6,000 (`calibration`): one class, no flag, at every anchor count.
#[cfg(feature = "calibration")]
#[test]
fn the_paper_s_null_table_on_the_target_model() {
    for n_anchor in [10usize, 20, 30, 60] {
        let (anchors, x) = batch(6000, n_anchor, 0, 0.0, 1300);
        let t0 = Instant::now();
        let res = latent_dif(&anchors, &x, 0);
        println!(
            "{n_anchor} anchors (KR-20 {:.3}): {} class(es), bic gain {:+.1}, max gap {:.3}, {:?}, {:.1}s",
            kr20(&anchors),
            res.classes,
            res.bic_gain,
            max(&res.dif),
            res.status,
            t0.elapsed().as_secs_f64()
        );
        assert!(res.flags(MIXTURE_DIF_MAX).iter().all(|&f| !f));
        assert_eq!(res.classes, 1);
    }
}

/// AT-DIF-12 in full (`calibration`): 2, 4 and 6 of 8 items shifted by δ = 0.9 with 30
/// anchors at N = 6,000 — exactly the shifted items are flagged in every case.
#[cfg(feature = "calibration")]
#[test]
fn at_dif_12_campaigns_of_2_4_and_6_of_8_are_flagged_exactly() {
    for n_biased in [2usize, 4, 6] {
        let (anchors, x) = batch(6000, 30, n_biased, 0.9, 700);
        let t0 = Instant::now();
        let res = latent_dif(&anchors, &x, 0);
        println!(
            "{n_biased} of 8 biased: {} class(es), pi {:?}, eta {:?}, gaps {:?}, {:?}, {:.1}s",
            res.classes,
            rounded(&res.pi),
            rounded(&res.eta),
            rounded(&res.dif),
            res.status,
            t0.elapsed().as_secs_f64()
        );
        let expected: Vec<bool> = (0..K).map(|j| j < n_biased).collect();
        assert_eq!(
            res.flags(MIXTURE_DIF_MAX),
            expected,
            "{n_biased} of 8: gaps {:?}",
            res.dif
        );
    }
}

/// AT-DIF-01 on the target model (`calibration`): null batches with 20, 40 and 60
/// anchors, four seeds at N = 3,000 and one at N = 12,000 — the item-level
/// false-positive rate over the 120 clean items is at most 5%.
#[cfg(feature = "calibration")]
#[test]
fn at_dif_01_the_item_level_false_positive_rate_on_null_batches() {
    let mut items = 0usize;
    let mut flagged = 0usize;
    let mut mixtures = 0usize;
    for (n, seeds) in [(3000usize, 2000u64..2004), (12_000, 2100..2101)] {
        for n_anchor in [20usize, 40, 60] {
            for seed in seeds.clone() {
                let (anchors, x) = batch(n, n_anchor, 0, 0.0, seed);
                let t0 = Instant::now();
                let res = latent_dif(&anchors, &x, 0);
                let flags = res.flags(MIXTURE_DIF_MAX).iter().filter(|&&f| f).count();
                items += K;
                flagged += flags;
                mixtures += usize::from(res.classes >= 2);
                println!(
                    "N = {n}, {n_anchor} anchors (KR-20 {:.3}), seed {seed}: {} class(es), max gap {:.3}, {flags} flag(s), {:?}, {:.1}s",
                    kr20(&anchors),
                    res.classes,
                    max(&res.dif),
                    res.status,
                    t0.elapsed().as_secs_f64()
                );
            }
        }
    }
    println!(
        "{flagged} of {items} clean items flagged ({:.1}%); {mixtures} batches with a mixture",
        100.0 * flagged as f64 / items as f64
    );
    assert!(
        flagged as f64 <= 0.05 * items as f64,
        "{flagged} of {items} clean items flagged"
    );
}
