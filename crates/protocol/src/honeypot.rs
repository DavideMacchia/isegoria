//! Golden items (`docs/05`, §Golden items): a fraction `η` of the review queue are items
//! of known quality, indistinguishable from the rest, giving a continuous direct measure
//! of `E_u`. Produced by a sortition committee (`governance`).

use crate::randomness::{Beacon, HONEYPOT};
use rand::seq::SliceRandom;
use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use scoring::reputation::{loo_scores, mean_score};

pub const HONEYPOT_RATE: f64 = 0.05;

/// Honeypot placement seeded from the epoch's beacon (INV-10, D41): the golden
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

/// Skill `S_u` of each panel reviewer on the golden items (`docs/01` D33, T50): the mean
/// leave-one-out difference score against the known outcomes. Strictly proper; a reviewer
/// who copies the others scores exactly 0; random or block voting scores below zero.
pub fn reviewer_skills(
    predictions: &[Vec<f64>],
    weights: &[f64],
    known_outcomes: &[f64],
) -> Vec<f64> {
    loo_scores(predictions, weights, known_outcomes)
        .iter()
        .map(|row| mean_score(row))
        .collect()
}
