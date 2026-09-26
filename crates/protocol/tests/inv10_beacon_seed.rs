//! Beacon-seeded randomness (`docs/08` INV-10/CRYPTO-008, `docs/01` D41, T37): every draw
//! seeds from the epoch's commit-reveal beacon, which nothing on the log after the commit set
//! can move, so neither a draft nor the log's tail picks a draw (AT-BR-05).

mod common;

use identity::nym::Nym;
use network::beacon::BeaconRound;
use network::cid::{cid, Cid};
use network::consortium::{Consortium, Member};
use network::log::TransparencyLog;
use protocol::governance::{sortition_from_beacon, Candidate};
use protocol::honeypot::inject_from_beacon;
use protocol::lottery::{admit, admit_from_beacon};
use protocol::randomness::{Beacon, HONEYPOT, LOTTERY, REVIEW_ASSIGNMENT, SORTITION};
use protocol::review::{assign_from_beacon, Reviewer};
use std::collections::HashSet;

const NET: [u8; 32] = [0x42; 32];
const EPOCH: u64 = 3;

fn pool(n: usize) -> Vec<Reviewer> {
    (0..n)
        .map(|i| Reviewer {
            nym: Nym([i as u8; 32]),
            f_u: (i as f64 / n as f64) - 0.5,
        })
        .collect()
}

fn nyms(panel: &[Reviewer]) -> Vec<Nym> {
    panel.iter().map(|r| r.nym).collect()
}

/// The epoch on a log: the commit set, then `deposits` in the order given, then the beacon
/// (every member revealing). Returns the deposit checkpoint's head and the beacon.
fn epoch_on_log(deposits: &[Cid]) -> ([u8; 32], Beacon) {
    let members: Vec<Member> = (1u8..=4).map(|i| Member::from_seed([i; 32])).collect();
    let consortium = Consortium::new(members.iter().map(Member::public).collect(), 3);
    let mut round = BeaconRound::open(&consortium, NET, EPOCH);
    let mut log = TransparencyLog::new();
    log.append(cid(b"an earlier epoch"));
    let reveals: Vec<_> = members
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let (commit, reveal) = m.beacon_commit(round.id(), i);
            round.commit(&commit).unwrap();
            reveal
        })
        .collect();
    log.append(round.close_commits().unwrap());
    for d in deposits {
        log.append(*d);
    }
    let head = log.checkpoint(NET, consortium.member_set_hash()).head;
    round.close_deposits().unwrap();
    for r in &reveals {
        round.reveal(r).unwrap();
    }
    let outcome = round.finish().unwrap();
    log.append(outcome.record());
    (head, Beacon::from_outcome(&outcome).unwrap())
}

// ------------------------------------ AT-BR-05 ------------------------------------

/// AT-BR-05: a panel is a function of the beacon and the slot, never of draft bytes.
#[test]
fn at_br_05_a_panel_does_not_depend_on_draft_bytes() {
    let beacon = common::beacon(3, EPOCH);
    let reviewers = pool(50);
    let (k, slot) = (9, 4u64);
    let panel = nyms(&assign_from_beacon(&reviewers, k, &beacon, slot));
    for _draft_variant in 0..1000 {
        assert_eq!(
            nyms(&assign_from_beacon(&reviewers, k, &beacon, slot)),
            panel
        );
    }
    let other = common::beacon(9, EPOCH);
    assert_ne!(
        nyms(&assign_from_beacon(&reviewers, k, &other, slot)),
        panel
    );
    assert_ne!(
        nyms(&assign_from_beacon(&reviewers, k, &beacon, slot + 1)),
        panel
    );
}

/// AT-BR-05: once the commit set is on the log, no deposit content or order moves the seed.
#[test]
fn at_br_05_the_last_depositor_cannot_choose_among_seeds() {
    let others: Vec<Cid> = (0..9u8).map(|i| cid(&[i])).collect();
    let mut heads = HashSet::new();
    let mut seeds = HashSet::new();
    let mut try_log = |deposits: Vec<Cid>| {
        let (head, beacon) = epoch_on_log(&deposits);
        heads.insert(head);
        seeds.insert(beacon.seed(LOTTERY, EPOCH));
    };
    for variant in 0..64u32 {
        let mut deposits = others.clone();
        deposits.push(cid(format!("my draft, variant {variant}").as_bytes()));
        try_log(deposits.clone());
        deposits.reverse();
        try_log(deposits);
    }
    try_log(others.clone());
    let mut extra = others.clone();
    extra.push(cid(b"one more deposit"));
    try_log(extra);
    assert_eq!(heads.len(), 130, "every tail is a different log");
    assert_eq!(seeds.len(), 1, "and the same seed");
}

// ---------------------------------- lottery ----------------------------------

/// AT-BR-05: the lottery reads the set of deposits, not the order the log lists them in.
#[test]
fn the_lottery_depends_on_the_set_of_deposits_not_their_order() {
    let deposits: Vec<Cid> = (0..20u8).map(|i| cid(&[i])).collect();
    let mut reversed = deposits.clone();
    reversed.reverse();
    let mut rotated = deposits.clone();
    rotated.rotate_left(7);
    let (evens, odds): (Vec<Cid>, Vec<Cid>) = deposits.iter().partition(|c| c.0[0] % 2 == 0);
    let interleaved: Vec<Cid> = odds.into_iter().chain(evens).collect();
    let mut duplicated = deposits.clone();
    duplicated.extend_from_slice(&deposits[..4]);
    for epoch in 0..50u64 {
        let beacon = common::beacon(1, epoch);
        let reference = admit_from_beacon(&deposits, 5, &beacon, epoch);
        for order in [&reversed, &rotated, &interleaved, &duplicated] {
            assert_eq!(admit_from_beacon(order, 5, &beacon, epoch), reference);
        }
        let raw = admit(&deposits, 5, epoch, 1);
        assert_eq!(admit(&reversed, 5, epoch, 1), raw);
        assert_eq!(admit(&duplicated, 5, epoch, 1), raw);
    }
    let everyone = admit(&duplicated, 100, 7, 1);
    assert_eq!(
        everyone.len(),
        20,
        "a deposit listed twice is admitted once"
    );
    assert!(
        everyone.windows(2).all(|w| w[0] < w[1]),
        "in content-id order"
    );
}

/// The lottery is deterministic per (beacon, epoch) and moves with the beacon.
#[test]
fn admission_seeds_from_the_beacon_and_is_deterministic() {
    let beacon = common::beacon(5, EPOCH);
    let deposited: Vec<usize> = (0..20).collect();
    let a = admit_from_beacon(&deposited, 5, &beacon, 7);
    let b = admit_from_beacon(&deposited, 5, &beacon, 7);
    assert_eq!(a, b, "deterministic per (beacon, epoch)");
    assert_eq!(a.len(), 5);
    let other = common::beacon(6, EPOCH);
    assert_ne!(
        beacon.seed(LOTTERY, 7),
        other.seed(LOTTERY, 7),
        "the seed is a function of the round"
    );
}

// --------------------------------- the beacon ---------------------------------

/// The seeds are deterministic, separated by purpose and index, and bound to the round.
#[test]
fn beacon_seeds_are_deterministic_and_domain_separated() {
    let beacon = common::beacon(1, EPOCH);
    assert_eq!(
        beacon.seed(LOTTERY, 0),
        common::beacon(1, EPOCH).seed(LOTTERY, 0)
    );
    assert_ne!(beacon.seed(LOTTERY, 0), beacon.seed(REVIEW_ASSIGNMENT, 0));
    assert_ne!(beacon.seed(HONEYPOT, 0), beacon.seed(SORTITION, 0));
    assert_ne!(beacon.seed(LOTTERY, 0), beacon.seed(LOTTERY, 1));
    assert_ne!(
        beacon.seed(LOTTERY, 0),
        common::beacon(2, EPOCH).seed(LOTTERY, 0)
    );
    assert_ne!(
        beacon.seed(LOTTERY, 0),
        common::beacon(1, EPOCH + 1).seed(LOTTERY, 0)
    );
}

/// A round with fewer than `t` reveals gives no beacon: there is nothing to draw from.
#[test]
fn a_round_without_a_beacon_seeds_nothing() {
    let members: Vec<Member> = (1u8..=4).map(|i| Member::from_seed([i; 32])).collect();
    let consortium = Consortium::new(members.iter().map(Member::public).collect(), 3);
    let mut round = BeaconRound::open(&consortium, NET, EPOCH);
    let reveals: Vec<_> = members
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let (commit, reveal) = m.beacon_commit(round.id(), i);
            round.commit(&commit).unwrap();
            reveal
        })
        .collect();
    round.close_commits().unwrap();
    round.close_deposits().unwrap();
    round.reveal(&reveals[0]).unwrap();
    round.reveal(&reveals[2]).unwrap();
    assert!(Beacon::from_outcome(&round.finish().unwrap()).is_none());
}

/// Honeypot placement and sortition are deterministic per (beacon, index).
#[test]
fn honeypot_and_sortition_seed_from_the_beacon() {
    let beacon = common::beacon(8, EPOCH);
    let queue: Vec<u32> = (0..40).collect();
    let golden: Vec<u32> = (100..110).collect();
    let one = inject_from_beacon(&queue, &golden, 0.05, &beacon, 3);
    let two = inject_from_beacon(&queue, &golden, 0.05, &beacon, 3);
    assert_eq!(
        one, two,
        "honeypot placement is deterministic per (beacon, epoch)"
    );

    let candidates: Vec<Candidate<usize>> = (0..30)
        .map(|i| Candidate {
            id: i,
            f_u: (i as f64 / 30.0) - 0.5,
        })
        .collect();
    let s1 = sortition_from_beacon(&candidates, 7, 3, &beacon, 0).unwrap();
    let s2 = sortition_from_beacon(&candidates, 7, 3, &beacon, 0).unwrap();
    assert_eq!(s1, s2, "sortition is deterministic per (beacon, round)");
    assert_eq!(s1.len(), 7);
    assert_ne!(
        sortition_from_beacon(&candidates, 7, 3, &beacon, 1).unwrap(),
        s1
    );
}
