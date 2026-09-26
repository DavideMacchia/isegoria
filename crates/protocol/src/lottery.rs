//! Admission by lottery (`docs/05` [3], `docs/01` D10): each epoch a random subset of
//! deposits enters the pipeline, bounding the queue with equal expected access.
//! Deterministic given the epoch seed.

use crate::randomness::{Beacon, LOTTERY};
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

/// Admission lottery seeded from the epoch's beacon (INV-10, D41): the sanctioned entry
/// point; [`admit`] takes a raw seed for testing.
pub fn admit_from_beacon<T: Clone + Ord>(
    deposited: &[T],
    capacity: usize,
    beacon: &Beacon,
    epoch: u64,
) -> Vec<T> {
    admit(deposited, capacity, beacon.seed(LOTTERY, epoch), epoch)
}

/// Selects up to `capacity` of the distinct `deposited` at random, a function of the set and
/// `(base_seed, epoch)` alone: returned in canonical order (`T`'s, content id for a `Cid`).
pub fn admit<T: Clone + Ord>(
    deposited: &[T],
    capacity: usize,
    base_seed: u64,
    epoch: u64,
) -> Vec<T> {
    let mut set = deposited.to_vec();
    set.sort_unstable();
    set.dedup();
    let mut rng = ChaCha8Rng::seed_from_u64(base_seed ^ epoch.wrapping_mul(0x9E3779B97F4A7C15));
    let mut idx: Vec<usize> = (0..set.len()).collect();
    idx.shuffle(&mut rng);
    idx.truncate(capacity.min(set.len()));
    idx.sort_unstable();
    idx.into_iter().map(|i| set[i].clone()).collect()
}
