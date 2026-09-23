//! Property-based tests over arbitrary inputs (`proptest`) for the combinatorial
//! pieces: blueprint apportionment, lottery admission, and stratified sortition.

use proptest::prelude::*;
use protocol::blueprint::Blueprint;
use protocol::governance::{stratified_sortition, Candidate, DuplicateCandidate};
use protocol::lottery::admit;
use std::collections::HashSet;

proptest! {
    /// Hamilton apportionment: seats sum to `size`, and each domain gets its exact
    /// proportional share rounded up or down (never off by more than one).
    #[test]
    fn blueprint_quotas_sum_and_stay_proportional(
        shares in prop::collection::vec(0.1f64..10.0, 1..8),
        size in 0usize..200,
    ) {
        let bp = Blueprint::new(shares.iter().copied().enumerate().collect());
        let quotas = bp.quotas(size);

        let seats: usize = quotas.iter().map(|(_, n)| n).sum();
        prop_assert_eq!(seats, size);

        let total: f64 = shares.iter().sum();
        for ((_, n), s) in quotas.iter().zip(shares.iter()) {
            let exact = size as f64 * s / total;
            prop_assert!((*n as f64) >= exact.floor() - 1e-9);
            prop_assert!((*n as f64) <= exact.floor() + 1.0 + 1e-9);
        }
    }

    /// Lottery admission is bounded, deterministic, and a distinct subset.
    #[test]
    fn lottery_is_bounded_deterministic_subset(
        n in 0usize..200,
        capacity in 0usize..250,
        seed in any::<u64>(),
        epoch in any::<u64>(),
    ) {
        let items: Vec<usize> = (0..n).collect();
        let drawn = admit(&items, capacity, seed, epoch);

        prop_assert_eq!(drawn.len(), capacity.min(n));
        prop_assert_eq!(&drawn, &admit(&items, capacity, seed, epoch));

        let set: HashSet<usize> = drawn.iter().copied().collect();
        prop_assert_eq!(set.len(), drawn.len(), "no duplicates");
        prop_assert!(drawn.iter().all(|&x| x < n), "within range");
    }

    /// Stratified sortition draws exactly `min(seats, n)` distinct members.
    #[test]
    fn sortition_draws_min_seats_distinct(
        n in 0usize..200,
        seats in 0usize..60,
        strata in 1usize..10,
        seed in any::<u64>(),
    ) {
        let candidates: Vec<Candidate<usize>> = (0..n)
            .map(|i| Candidate { id: i, f_u: i as f64 })
            .collect();
        let chosen = stratified_sortition(&candidates, seats, strata, seed).unwrap();

        prop_assert_eq!(chosen.len(), seats.min(n));
        let set: HashSet<usize> = chosen.iter().copied().collect();
        prop_assert_eq!(set.len(), chosen.len(), "no duplicates");
    }

    /// A repeated id anywhere in the list is refused, whatever the draw (T36).
    #[test]
    fn sortition_refuses_any_repeated_candidate(
        n in 2usize..80,
        seats in 0usize..20,
        strata in 1usize..6,
        seed in any::<u64>(),
        pick in any::<(usize, usize)>(),
    ) {
        let mut candidates: Vec<Candidate<usize>> = (0..n)
            .map(|i| Candidate { id: i, f_u: i as f64 })
            .collect();
        let (from, to) = (pick.0 % n, pick.1 % n);
        prop_assume!(from != to);
        candidates[to].id = candidates[from].id;
        prop_assert_eq!(
            stratified_sortition(&candidates, seats, strata, seed),
            Err(DuplicateCandidate)
        );
    }
}
