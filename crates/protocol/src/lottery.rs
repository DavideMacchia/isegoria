//! [3] Admission by lottery (`docs/05`, `docs/01` D10). The bottleneck is pilot
//! respondents, so a quota would explode the queue. Anyone may deposit; each epoch
//! a random subset enters the pipeline, giving equal expected access with a bounded
//! queue. Deterministic given the epoch seed.

use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

/// Selects up to `capacity` drafts at random from `deposited`, deterministically
/// per `(base_seed, epoch)`.
pub fn admit<T: Clone>(deposited: &[T], capacity: usize, base_seed: u64, epoch: u64) -> Vec<T> {
    let mut rng = ChaCha8Rng::seed_from_u64(base_seed ^ epoch.wrapping_mul(0x9E3779B97F4A7C15));
    let mut idx: Vec<usize> = (0..deposited.len()).collect();
    idx.shuffle(&mut rng);
    idx.truncate(capacity.min(deposited.len()));
    idx.sort_unstable();
    idx.into_iter().map(|i| deposited[i].clone()).collect()
}
