//! Golden items (`docs/05`, §Golden items). A fraction η≈5% of the review queue are
//! items of known quality, indistinguishable from the rest, giving a continuous
//! direct measure of the evaluator score and catching nodes that vote at random or in
//! blocks. They must be produced by a sortition committee (defended in `governance`).

use crate::randomness::{Beacon, HONEYPOT};
use rand::seq::SliceRandom;
use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use scoring::reputation::{difference_scores, mean_score, EvaluatorHistory, CUSUM_H, CUSUM_K};

pub const HONEYPOT_RATE: f64 = 0.05;

/// Honeypot placement seeded from the signed checkpoint (INV-10, D29, T8): the golden
/// items' positions are fixed by the beacon, so a reviewer cannot predict which queue
/// slots are golden. Sanctioned entry point; [`inject`] takes a raw seed for testing.
pub fn inject_from_beacon<T: Clone>(
    queue: &[T],
    golden: &[T],
    rate: f64,
    beacon: &Beacon,
    epoch: u64,
) -> Vec<T> {
    inject(queue, golden, rate, beacon.seed(HONEYPOT, epoch))
}

/// Interleaves golden items into a queue at approximately `rate`, positioned
/// deterministically per `seed` so they are not distinguishable by order.
pub fn inject<T: Clone>(queue: &[T], golden: &[T], rate: f64, seed: u64) -> Vec<T> {
    let target = ((queue.len() as f64) * rate).round() as usize;
    let n_golden = target.min(golden.len());
    let mut out: Vec<T> = queue.to_vec();
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    for g in golden.iter().take(n_golden) {
        let pos = rng.gen_range(0..=out.len());
        out.insert(pos, g.clone());
    }
    out.shuffle(&mut rng);
    out
}

/// Skill of each panel reviewer on the golden items (`docs/01` D33, T50): the mean
/// leave-one-out difference score `S_u` of its forecasts against the known outcomes — by
/// how much its Brier score beats the other panelists' weight-adjusted mean forecast
/// (`reputation::difference_scores`; D23's crowd baseline minus the reviewer). A strictly
/// proper rule: a reviewer who copies the crowd scores exactly 0, random or block voting
/// scores at or below zero. `predictions[u][j]`; `weights[u]` the review weights. Each
/// value is one epoch's contribution to the reviewer's long-run mean over its
/// `known_outcomes.len()` scored items.
pub fn reviewer_skills(
    predictions: &[Vec<f64>],
    weights: &[f64],
    known_outcomes: &[f64],
) -> Vec<f64> {
    difference_scores(predictions, weights, known_outcomes)
        .iter()
        .map(|per_item| mean_score(per_item))
        .collect()
}

/// Feeds the golden items' per-item scores into each panelist's history (D33, D34, T51):
/// the long-window mean that sets its weight, and the CUSUM that sends a reviewer whose
/// scores fall for long enough back to probation. `histories[u]` belongs to
/// `predictions[u]`. Returns which panelists raised an alarm on this batch (their history
/// has restarted).
pub fn record_golden_scores(
    histories: &mut [EvaluatorHistory],
    predictions: &[Vec<f64>],
    weights: &[f64],
    known_outcomes: &[f64],
) -> Vec<bool> {
    difference_scores(predictions, weights, known_outcomes)
        .iter()
        .zip(histories.iter_mut())
        .map(|(per_item, history)| {
            per_item.iter().fold(false, |alarmed, &x| {
                history.record(x, CUSUM_K, CUSUM_H) || alarmed
            })
        })
        .collect()
}
