//! Golden items (`docs/05`, §Golden items). A fraction η≈5% of the review queue are
//! items of known quality, indistinguishable from the rest, giving a continuous
//! direct measure of E_u and catching nodes that vote at random or in blocks.
//! They must be produced by a sortition committee (defended in `governance`).

use rand::seq::SliceRandom;
use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use scoring::reputation::{base_rate_baseline, brier_skill_score};

pub const HONEYPOT_RATE: f64 = 0.05;

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

/// Reviewer skill on golden items: Brier Skill Score of their predictions against
/// the known outcomes, over the crowd base-rate baseline. Random or block voting
/// scores at or below zero.
pub fn reviewer_skill(predictions: &[f64], known_outcomes: &[f64]) -> f64 {
    let baseline = base_rate_baseline(known_outcomes);
    brier_skill_score(predictions, known_outcomes, &baseline)
}
