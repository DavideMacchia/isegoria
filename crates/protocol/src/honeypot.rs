//! Golden items (`docs/05`, §Golden items). A fraction η≈5% of the review queue are
//! items of known quality, indistinguishable from the rest, giving a continuous
//! direct measure of E_u and catching nodes that vote at random or in blocks.
//! They must be produced by a sortition committee (defended in `governance`).

use crate::randomness::{Beacon, HONEYPOT};
use rand::seq::SliceRandom;
use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use scoring::reputation::{brier_skill_score, crowd_baseline};

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

/// Skill of each panel reviewer on the golden items: BSS of their predictions against
/// the known outcomes, over the crowd baseline (`docs/01` D23 — the weight-adjusted mean
/// of the panel's own predictions). A consensus follower scores ≈ 0; random or block
/// voting scores at or below zero. `predictions[u][j]`.
pub fn reviewer_skills(
    predictions: &[Vec<f64>],
    weights: &[f64],
    known_outcomes: &[f64],
) -> Vec<f64> {
    let baseline = crowd_baseline(predictions, weights);
    predictions
        .iter()
        .map(|p| brier_skill_score(p, known_outcomes, &baseline))
        .collect()
}
