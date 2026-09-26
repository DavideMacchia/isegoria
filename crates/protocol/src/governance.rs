//! Meta-level governance (`docs/05` §Meta-level governance, `docs/01` D16): scoring
//! parameters, the honeypot committee, the coverage blueprint and consortium composition
//! are decided by stratified sortition, never by vote (qualified supermajority + delay).

use crate::randomness::{Beacon, SORTITION};
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use std::collections::HashSet;
use std::hash::Hash;

pub const SUPERMAJORITY: f64 = 2.0 / 3.0;
pub const CHANGE_DELAY_DAYS: u32 = 30;

#[derive(Clone, Copy, Debug)]
pub struct Candidate<Id> {
    pub id: Id,
    pub f_u: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DuplicateCandidate;

/// [`stratified_sortition`] seeded from the epoch's beacon (INV-10).
pub fn sortition_from_beacon<Id: Clone + Eq + Hash>(
    candidates: &[Candidate<Id>],
    seats: usize,
    n_strata: usize,
    beacon: &Beacon,
    round: u64,
) -> Result<Vec<Id>, DuplicateCandidate> {
    stratified_sortition(candidates, seats, n_strata, beacon.seed(SORTITION, round))
}

/// Draws `seats` members by stratified sortition on `f_u` across `n_strata` strata, so
/// every position is represented. Deterministic per `seed`; refuses a repeated id.
pub fn stratified_sortition<Id: Clone + Eq + Hash>(
    candidates: &[Candidate<Id>],
    seats: usize,
    n_strata: usize,
    seed: u64,
) -> Result<Vec<Id>, DuplicateCandidate> {
    let mut seen = HashSet::with_capacity(candidates.len());
    if !candidates.iter().all(|c| seen.insert(&c.id)) {
        return Err(DuplicateCandidate);
    }
    let n = candidates.len();
    if n == 0 || seats == 0 {
        return Ok(Vec::new());
    }
    let seats = seats.min(n);
    let strata = n_strata.clamp(1, seats);

    let mut order: Vec<usize> = (0..n).collect();
    // `total_cmp`: a NaN position sorts last instead of panicking (docs/08 IQ-2).
    order.sort_by(|&a, &b| candidates[a].f_u.total_cmp(&candidates[b].f_u));

    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut picked = vec![false; n];
    for s in 0..strata {
        let lo = s * n / strata;
        let hi = ((s + 1) * n / strata).max(lo + 1).min(n);
        let seats_here = seats * (s + 1) / strata - seats * s / strata;
        let mut idxs: Vec<usize> = order[lo..hi].to_vec();
        idxs.shuffle(&mut rng);
        for &oi in idxs.iter().take(seats_here) {
            picked[oi] = true;
        }
    }

    // Fill any deficit from strata that were smaller than their seat allotment.
    let mut count = picked.iter().filter(|&&b| b).count();
    if count < seats {
        let mut rest: Vec<usize> = (0..n).filter(|&i| !picked[i]).collect();
        rest.shuffle(&mut rng);
        for oi in rest {
            if count >= seats {
                break;
            }
            picked[oi] = true;
            count += 1;
        }
    }

    Ok(candidates
        .iter()
        .enumerate()
        .filter(|(i, _)| picked[*i])
        .map(|(_, c)| c.id.clone())
        .collect())
}

/// A meta-level change is approved only with a qualified supermajority AND after the
/// mandatory delay; a tally with more votes than eligible voters never approves (T36).
pub fn change_approved(votes_for: usize, total_eligible: usize, days_elapsed: u32) -> bool {
    if total_eligible == 0 || votes_for > total_eligible {
        return false;
    }
    let fraction = votes_for as f64 / total_eligible as f64;
    fraction >= SUPERMAJORITY && days_elapsed >= CHANGE_DELAY_DAYS
}
