//! AT-REP-07 (`docs/01` D34, `docs/08` REPUTATION-004, T51): the reviewer's weight reads a
//! symmetric long-window mean of its per-item scores, and a one-sided CUSUM against that
//! mean returns a reviewer whose scores fall for long enough to probation.
//!
//! The regime is the paper's (`paper/scripts/revisions_evaluator.py`, `panel_bias`): nine
//! forecasters who share a crowd error to different degrees (exposure 0.3 for the reviewer
//! under test, 0.8 for the others) with independent noise 0.10; items pass with probability
//! `π ~ Beta(2, 2)`. The reviewer is cautious and slightly better than the crowd. Seeded
//! (`ChaCha8`), so every number below is reproducible; the RNG differs from NumPy's, so the
//! rates match the paper's Table 12 in kind, not to the digit — and the reference here is
//! the reviewer's running mean, not the true mean the paper's script knew.

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use scoring::reputation::{difference_scores, EvaluatorHistory, CUSUM_H, CUSUM_K};

fn normal(r: &mut ChaCha8Rng) -> f64 {
    let u1: f64 = 1.0 - r.gen::<f64>();
    let u2: f64 = r.gen();
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

/// `Beta(2, 2)`: the median of three uniforms.
fn beta22(r: &mut ChaCha8Rng) -> f64 {
    let mut v = [r.gen::<f64>(), r.gen::<f64>(), r.gen::<f64>()];
    v.sort_by(f64::total_cmp);
    v[1]
}

/// The reviewer's per-item leave-one-out scores over `n` items; from `flip_from` on, its
/// forecast is flipped (`p → 1 − p`) with probability 0.2.
fn stream(seed: u64, n: usize, flip_from: Option<usize>) -> Vec<f64> {
    let mut r = ChaCha8Rng::seed_from_u64(seed);
    let exposure: Vec<f64> = std::iter::once(0.3)
        .chain(std::iter::repeat_n(0.8, 8))
        .collect();
    let mut out = Vec::with_capacity(n);
    for t in 0..n {
        let pi = beta22(&mut r);
        let o = if r.gen::<f64>() < pi { 1.0 } else { 0.0 };
        let crowd_error = 0.15 * normal(&mut r);
        let mut p: Vec<Vec<f64>> = exposure
            .iter()
            .map(|b| vec![(pi + b * crowd_error + 0.10 * normal(&mut r)).clamp(0.02, 0.98)])
            .collect();
        if flip_from.is_some_and(|f| t >= f) && r.gen::<f64>() < 0.2 {
            p[0][0] = 1.0 - p[0][0];
        }
        out.push(difference_scores(&p, &[1.0; 9], &[o])[0][0]);
    }
    out
}

/// The retired rule, kept here as the record of the defect: rises at `up`, falls at
/// `down`.
fn asymmetric_update(prev: f64, new: f64, up: f64, down: f64) -> f64 {
    prev + if new >= prev { up } else { down } * (new - prev)
}

#[test]
fn at_rep_07_an_honest_stream_raises_at_most_one_alarm_and_the_mean_tracks_the_truth() {
    let long = stream(1, 200_000, None);
    let truth = long.iter().sum::<f64>() / long.len() as f64;

    // The asymmetric update (rates 0.02 / 0.2, the paper's) sits far below the true mean,
    // and below the crowd copier's 0 — the defect D34 removes.
    let mut e = 0.0;
    let mut level = 0.0;
    for (i, &x) in long.iter().enumerate() {
        e = asymmetric_update(e, x, 0.02, 0.2);
        if i >= 1000 {
            level += e;
        }
    }
    level /= (long.len() - 1000) as f64;
    println!("honest reviewer: true mean {truth:+.4}; asymmetric update {level:+.4}");
    assert!(
        truth > 0.0,
        "the cautious reviewer beats the crowd: {truth}"
    );
    assert!(
        level < -0.05,
        "the asymmetric update should sit far below: {level}"
    );

    // Seeded honest stream of 10,000 scored items: at most one alarm.
    let mut history = EvaluatorHistory::new();
    let mut alarms = 0;
    for x in stream(1, 10_000, None) {
        if history.record(x, CUSUM_K, CUSUM_H) {
            alarms += 1;
        }
    }
    println!(
        "honest 10,000 items: {alarms} alarm(s); long-window mean {:+.4}",
        history.score()
    );
    assert!(alarms <= 1, "{alarms} alarms on an honest stream");
    assert!(history.scored() >= 4_000);
    assert!(
        (history.score() - truth).abs() < 0.01,
        "{} vs {truth}",
        history.score()
    );
    assert_eq!(history.alarms(), alarms);
}

#[test]
fn at_rep_07_a_reviewer_who_starts_flipping_forecasts_is_caught() {
    let mut delays = Vec::new();
    for seed in 100..130u64 {
        let mut history = EvaluatorHistory::new();
        let mut caught = None;
        for (t, x) in stream(seed, 1300, Some(300)).into_iter().enumerate() {
            if history.record(x, CUSUM_K, CUSUM_H) && t >= 300 && caught.is_none() {
                caught = Some(t - 300);
            }
        }
        assert!(
            history.scored() < 1000,
            "seed {seed}: the record was not restarted"
        );
        delays.push(caught.unwrap_or_else(|| panic!("seed {seed}: never caught")));
    }
    delays.sort_unstable();
    let median = delays[delays.len() / 2];
    let within_100 = delays.iter().filter(|&&d| d <= 100).count();
    println!(
        "flipper caught in {}/30 streams: median {median} items, {within_100} within 100, worst {}",
        delays.len(),
        delays.last().unwrap()
    );
    assert!(median <= 60, "median delay {median}");
    assert!(within_100 >= 24, "{within_100} of 30 within 100 items");
    assert!(*delays.last().unwrap() <= 300);
}
