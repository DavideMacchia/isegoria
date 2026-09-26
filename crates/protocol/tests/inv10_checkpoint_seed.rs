//! Checkpoint-seeded randomness (`docs/08` INV-10/CRYPTO-008, `docs/01` D29, T8): every
//! draw seeds from the signed consortium checkpoint head, fixed after deposits close, so
//! nobody can grind a draft to pick reviewers (AT-BR-05).

use identity::nym::Nym;
use network::consortium::{Checkpoint, Consortium, Member};
use protocol::governance::{sortition_from_beacon, Candidate};
use protocol::honeypot::inject_from_beacon;
use protocol::lottery::admit_from_beacon;
use protocol::randomness::{Beacon, HONEYPOT, LOTTERY, REVIEW_ASSIGNMENT, SORTITION};
use protocol::review::{assign_from_beacon, Reviewer};

/// A checkpoint co-signed by a threshold of the consortium, reduced to its beacon.
fn signed_beacon(head: [u8; 32]) -> Beacon {
    let members: Vec<Member> = (0u8..4).map(|i| Member::from_seed([i + 1; 32])).collect();
    let consortium = Consortium::new(members.iter().map(|m| m.public()).collect(), 3);
    let cp = Checkpoint::new([0u8; 32], consortium.member_set_hash(), 1, head);
    let sigs: Vec<(usize, _)> = members
        .iter()
        .enumerate()
        .map(|(i, m)| (i, m.sign(&cp)))
        .collect();
    assert!(
        consortium.verify(&cp, &sigs),
        "checkpoint is consortium-signed"
    );
    Beacon::from_checkpoint(&cp)
}

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

// ------------------------------------ AT-BR-05 ------------------------------------

#[test]
fn at_br_05_a_panel_does_not_depend_on_draft_bytes() {
    let beacon = signed_beacon([3u8; 32]);
    let reviewers = pool(50);
    let (k, slot) = (9, 4u64);

    // The panel is a pure function of (beacon, slot); draft bytes never enter it.
    let panel = nyms(&assign_from_beacon(&reviewers, k, &beacon, slot));
    for _draft_variant in 0..1000 {
        assert_eq!(
            nyms(&assign_from_beacon(&reviewers, k, &beacon, slot)),
            panel
        );
    }

    // The panel is bound to the signed head: a different one yields a different draw.
    let other = signed_beacon([9u8; 32]);
    assert_ne!(
        nyms(&assign_from_beacon(&reviewers, k, &other, slot)),
        panel
    );

    // Different slots draw different panels.
    assert_ne!(
        nyms(&assign_from_beacon(&reviewers, k, &beacon, slot + 1)),
        panel
    );
}

// ---------------------------------- lottery ----------------------------------

#[test]
fn admission_seeds_from_the_checkpoint_and_is_deterministic() {
    let beacon = signed_beacon([5u8; 32]);
    let deposited: Vec<usize> = (0..20).collect();
    let a = admit_from_beacon(&deposited, 5, &beacon, 7);
    let b = admit_from_beacon(&deposited, 5, &beacon, 7);
    assert_eq!(a, b, "deterministic per (beacon, epoch)");
    assert_eq!(a.len(), 5);

    let other = signed_beacon([6u8; 32]);
    assert_ne!(
        beacon.seed(LOTTERY, 7),
        other.seed(LOTTERY, 7),
        "the seed is a function of the signed head"
    );
}

// --------------------------------- the beacon ---------------------------------

#[test]
fn beacon_seeds_are_deterministic_and_domain_separated() {
    let beacon = signed_beacon([1u8; 32]);
    assert_eq!(beacon.seed(LOTTERY, 0), beacon.seed(LOTTERY, 0));
    assert_ne!(beacon.seed(LOTTERY, 0), beacon.seed(REVIEW_ASSIGNMENT, 0));
    assert_ne!(beacon.seed(HONEYPOT, 0), beacon.seed(SORTITION, 0));
    assert_ne!(beacon.seed(LOTTERY, 0), beacon.seed(LOTTERY, 1));
    // Bound to the head.
    let other = signed_beacon([2u8; 32]);
    assert_ne!(beacon.seed(LOTTERY, 0), other.seed(LOTTERY, 0));
}

#[test]
fn honeypot_and_sortition_seed_from_the_beacon() {
    let beacon = signed_beacon([8u8; 32]);

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
    // A later round draws a different committee.
    assert_ne!(
        sortition_from_beacon(&candidates, 7, 3, &beacon, 1).unwrap(),
        s1
    );
}
