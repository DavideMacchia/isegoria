//! Exact outcomes for the combinatorial protocol pieces (T41). The existing suites check
//! sizes, determinism and orderings; cargo-mutants showed they stay green when the
//! stratum boundaries of a draw move, the largest-remainder order is inverted, an empty
//! honeypot queue is returned, or the lottery stops mixing in the epoch. These pin the
//! actual results.

use identity::nym::Nym;
use network::consortium::Checkpoint;
use protocol::admission::{NullifierSet, QuotaLedger};
use protocol::blueprint::{assemble_test, Blueprint};
use protocol::exposure::RetirementReason;
use protocol::governance::{stratified_sortition, Candidate};
use protocol::honeypot::inject_from_beacon;
use protocol::lifecycle::{step, Event, Invalid, State, K_MIN};
use protocol::lottery::admit;
use protocol::probation::FounderSet;
use protocol::randomness::Beacon;
use protocol::review::{assign_reviewers, Reviewer};
use std::collections::HashSet;

fn nym(i: u8) -> Nym {
    Nym([i; 32])
}

// ------------------------------- sortition -------------------------------

/// Candidates whose position equals their id, so a stratum is an id range.
fn line(n: usize) -> Vec<Candidate<usize>> {
    (0..n)
        .map(|i| Candidate {
            id: i,
            f_u: i as f64,
        })
        .collect()
}

/// Every stratum gets exactly its apportioned seats: 10 seats over 4 strata of 25 are
/// 2, 3, 2, 3 — for every seed.
#[test]
fn sortition_seats_each_stratum_exactly() {
    for seed in 0..20 {
        let chosen = stratified_sortition(&line(100), 10, 4, seed).unwrap();
        let per: Vec<usize> = (0..4)
            .map(|s| chosen.iter().filter(|&&id| id / 25 == s).count())
            .collect();
        assert_eq!(per, vec![2, 3, 2, 3], "seed {seed}: {chosen:?}");
    }
}

/// With 8 candidates, 7 seats and 5 strata, the middle stratum holds one candidate but
/// is apportioned two seats. The deficit is filled from the rest: still 7 distinct seats.
#[test]
fn sortition_fills_a_stratum_that_is_short_of_its_seats() {
    for seed in 0..20 {
        let chosen = stratified_sortition(&line(8), 7, 5, seed).unwrap();
        let distinct: HashSet<usize> = chosen.iter().copied().collect();
        assert_eq!(chosen.len(), 7, "seed {seed}: {chosen:?}");
        assert_eq!(distinct.len(), 7);
    }
}

// ------------------------------- blueprint -------------------------------

/// Hamilton apportionment gives the leftover seat to the largest *fractional* part, not
/// the largest share: 6.1 / 3.9 → 6 / 4, and 1.4 / 5.6 → 1 / 6.
#[test]
fn blueprint_gives_leftover_seats_by_largest_remainder() {
    let b = Blueprint::new(vec![("a", 0.61), ("b", 0.39)]);
    assert_eq!(b.quotas(10), vec![("a", 6), ("b", 4)]);
    let b = Blueprint::new(vec![("a", 1.4), ("b", 5.6)]);
    assert_eq!(b.quotas(7), vec![("a", 1), ("b", 6)]);
    // Ties go to the earlier domain: 4.5 / 3.5 / 2.0 → 5 / 3 / 2.
    let b = Blueprint::new(vec![("a", 0.45), ("b", 0.35), ("c", 0.20)]);
    assert_eq!(b.quotas(10), vec![("a", 5), ("b", 3), ("c", 2)]);
}

/// No positive share: no seats, rather than apportioning a NaN.
#[test]
fn blueprint_without_shares_seats_nobody() {
    let b = Blueprint::new(vec![("a", 0.0), ("b", 0.0)]);
    assert_eq!(b.quotas(5), vec![("a", 0), ("b", 0)]);
}

/// `actual − target`: a pool of 3×a and 1×b against 50/50 is +0.25 / −0.25; with no
/// positive share every target is 0, not NaN.
#[test]
fn coverage_deviation_is_actual_minus_target_share() {
    let b = Blueprint::new(vec![("a", 1.0), ("b", 1.0)]);
    assert_eq!(
        b.coverage_deviation(&["a", "a", "a", "b"]),
        vec![("a", 0.25), ("b", -0.25)]
    );
    let zero = Blueprint::new(vec![("a", 0.0)]);
    assert_eq!(zero.coverage_deviation(&["a", "a"]), vec![("a", 1.0)]);
}

/// Exactly as many items as a domain needs is enough — no shortfall.
#[test]
fn assemble_test_accepts_exactly_enough_items() {
    let b = Blueprint::new(vec![("a", 1.0), ("b", 1.0)]);
    let available = [(1, "a"), (2, "a"), (3, "b"), (4, "b")];
    let mut got = assemble_test(&available, 4, &b, 7).expect("exactly enough");
    got.sort_unstable();
    assert_eq!(got, vec![1, 2, 3, 4]);
}

// ------------------------------- honeypot -------------------------------

#[test]
fn beacon_placed_honeypots_are_actually_inserted() {
    let beacon = Beacon::from_checkpoint(&Checkpoint::new([1; 32], [2; 32], 5, [3; 32]));
    let queue: Vec<u32> = (0..100).collect();
    let golden: Vec<u32> = (1000..1010).collect();
    let out = inject_from_beacon(&queue, &golden, 0.05, &beacon, 0);
    assert_eq!(out.len(), 105, "5% of 100 queue items");
    assert!(queue.iter().all(|q| out.contains(q)), "no queue item lost");
    assert_eq!(out.iter().filter(|&&x| x >= 1000).count(), 5);
}

// -------------------------------- lottery --------------------------------

/// The epoch is mixed into the seed for any base seed, including the degenerate all-0
/// and all-1 bit patterns (where `&` or `|` in place of `^` would erase it).
#[test]
fn lottery_draws_differ_across_epochs_for_any_base_seed() {
    let deposited: Vec<u32> = (0..200).collect();
    for base in [0u64, u64::MAX, 0x1234_5678] {
        let draws: HashSet<Vec<u32>> = (1..=5).map(|e| admit(&deposited, 20, base, e)).collect();
        assert_eq!(draws.len(), 5, "base {base:#x}: epochs repeat a draw");
    }
}

// ------------------------------- lifecycle -------------------------------

/// `K_MIN` itself is an admissible stage-2 batch; one fewer is not (INV-8).
#[test]
fn a_pilot2_batch_of_exactly_k_min_is_admitted() {
    let pilot2 = State::Pilot2 { appealed: false };
    assert_eq!(
        step(
            pilot2.clone(),
            Event::Pilot2Batch {
                batch_size: K_MIN,
                passed: true
            }
        ),
        Ok(State::ActivePool)
    );
    assert_eq!(
        step(
            pilot2,
            Event::Pilot2Batch {
                batch_size: K_MIN - 1,
                passed: true
            }
        ),
        Err(Invalid::BatchTooSmall)
    );
}

#[test]
fn reaching_the_exposure_limit_retires_a_pool_item() {
    assert_eq!(
        step(State::ActivePool, Event::ExposureLimit),
        Ok(State::Retired(RetirementReason::Exposure))
    );
}

// --------------------------- accessors and panels ---------------------------

#[test]
fn nullifier_set_and_quota_ledger_report_what_they_recorded() {
    let mut seen = NullifierSet::new();
    assert!(!seen.contains(&nym(1)));
    seen.spend(nym(1)).unwrap();
    assert!(seen.contains(&nym(1)));
    assert!(!seen.contains(&nym(2)));

    let mut ledger = QuotaLedger::new();
    assert_eq!(ledger.used(&nym(1)), 0);
    ledger.charge(nym(1), 5).unwrap();
    ledger.charge(nym(1), 5).unwrap();
    assert_eq!(ledger.used(&nym(1)), 2);
    assert_eq!(ledger.used(&nym(2)), 0);
}

#[test]
fn founder_set_emptiness() {
    assert!(FounderSet::new([]).is_empty());
    let f = FounderSet::new([nym(1), nym(2)]);
    assert!(!f.is_empty());
    assert_eq!(f.len(), 2);
}

/// One reviewer per stratum: with 90 reviewers at positions 0..90 and k = 9, the s-th
/// pick comes from positions [10s, 10s + 10), for every seed.
#[test]
fn reviewer_assignment_draws_one_per_stratum() {
    let reviewers: Vec<Reviewer> = (0..90)
        .map(|i| Reviewer {
            nym: nym(i as u8),
            f_u: i as f64,
        })
        .collect();
    for seed in 0..20 {
        let panel = assign_reviewers(&reviewers, 9, seed);
        for (s, r) in panel.iter().enumerate() {
            let lo = 10.0 * s as f64;
            assert!(
                r.f_u >= lo && r.f_u < lo + 10.0,
                "seed {seed}: pick {s} at {}",
                r.f_u
            );
        }
    }
}
