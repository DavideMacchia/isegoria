//! The epoch's beacon round (`docs/04` §The epoch's beacon, `docs/08` §9.4, D41, T37):
//! commits fixed before the deposits close, reveals after, withholders excluded and recorded.

use ed25519_dalek::Signature;
use network::beacon::{BeaconCommit, BeaconReveal, BeaconRound, RoundError, RoundId};
use network::consortium::{Checkpoint, Consortium, Member};

const NET: [u8; 32] = [0x42; 32];
const EPOCH: u64 = 7;

fn committee() -> (Vec<Member>, Consortium) {
    let members: Vec<Member> = (1u8..=4).map(|i| Member::from_seed([i; 32])).collect();
    let consortium = Consortium::new(members.iter().map(Member::public).collect(), 3);
    (members, consortium)
}

fn id(consortium: &Consortium) -> RoundId {
    RoundId {
        network_id: NET,
        member_set_hash: consortium.member_set_hash(),
        epoch: EPOCH,
    }
}

/// The members in `order` commit; returns every member's reveal, by member.
fn commit_all(round: &mut BeaconRound, members: &[Member], order: &[usize]) -> Vec<BeaconReveal> {
    let pairs: Vec<_> = (0..members.len())
        .map(|i| members[i].beacon_commit(round.id(), i))
        .collect();
    for &i in order {
        round.commit(&pairs[i].0).expect("an honest commit counts");
    }
    pairs.into_iter().map(|(_, reveal)| reveal).collect()
}

/// A round where every member commits and `revealers` reveal, in that order.
fn run(
    consortium: &Consortium,
    members: &[Member],
    revealers: &[usize],
) -> network::beacon::BeaconOutcome {
    let mut round = BeaconRound::open(consortium, NET, EPOCH);
    let reveals = commit_all(&mut round, members, &[0, 1, 2, 3]);
    round.close_commits().unwrap();
    round.close_deposits().unwrap();
    for &i in revealers {
        round.reveal(&reveals[i]).unwrap();
    }
    round.finish().unwrap()
}

/// AT-NET-10: with every member revealing the round forms a beacon and records nobody.
#[test]
fn at_net_10_an_honest_round_forms_a_beacon() {
    let (members, consortium) = committee();
    let outcome = run(&consortium, &members, &[0, 1, 2, 3]);
    assert!(outcome.value().is_some());
    assert_eq!(outcome.revealed(), &[0, 1, 2, 3]);
    assert!(outcome.withheld().is_empty());
    assert_eq!(outcome.round(), id(&consortium));
}

/// AT-NET-10: the beacon is a function of the reveals, not of the order commits or reveals arrive.
#[test]
fn at_net_10_the_beacon_does_not_depend_on_arrival_order() {
    let (members, consortium) = committee();
    let reference = run(&consortium, &members, &[0, 1, 2, 3]);
    let mut round = BeaconRound::open(&consortium, NET, EPOCH);
    let reveals = commit_all(&mut round, &members, &[3, 1, 0, 2]);
    let set = round.close_commits().unwrap();
    round.close_deposits().unwrap();
    for i in [2, 0, 3, 1] {
        round.reveal(&reveals[i]).unwrap();
    }
    let shuffled = round.finish().unwrap();
    assert_eq!(shuffled.value(), reference.value());
    assert_eq!(shuffled.record(), reference.record());

    let mut again = BeaconRound::open(&consortium, NET, EPOCH);
    commit_all(&mut again, &members, &[0, 1, 2, 3]);
    assert_eq!(
        again.close_commits().unwrap(),
        set,
        "the commit set's record is canonical"
    );
}

/// AT-NET-10: each member's secret moves the beacon, and so do the network and the epoch.
#[test]
fn at_net_10_the_beacon_is_bound_to_every_reveal_and_to_the_round() {
    let (members, consortium) = committee();
    let all = run(&consortium, &members, &[0, 1, 2, 3]).value().unwrap();
    let mut values = vec![all];
    for missing in 0..4 {
        let revealers: Vec<usize> = (0..4).filter(|&i| i != missing).collect();
        values.push(run(&consortium, &members, &revealers).value().unwrap());
    }
    let mut other_epoch = BeaconRound::open(&consortium, NET, EPOCH + 1);
    let reveals = commit_all(&mut other_epoch, &members, &[0, 1, 2, 3]);
    other_epoch.close_commits().unwrap();
    other_epoch.close_deposits().unwrap();
    for r in &reveals {
        other_epoch.reveal(r).unwrap();
    }
    values.push(other_epoch.finish().unwrap().value().unwrap());
    let mut other_net = BeaconRound::open(&consortium, [0x43; 32], EPOCH);
    let reveals = commit_all(&mut other_net, &members, &[0, 1, 2, 3]);
    other_net.close_commits().unwrap();
    other_net.close_deposits().unwrap();
    for r in &reveals {
        other_net.reveal(r).unwrap();
    }
    values.push(other_net.finish().unwrap().value().unwrap());
    let distinct: std::collections::HashSet<_> = values.iter().collect();
    assert_eq!(distinct.len(), values.len(), "{values:?}");
}

/// AT-NET-10: a member who withholds is recorded, the beacon forms from the others, and its
/// signature does not count toward the epoch's checkpoints.
#[test]
fn at_net_10_a_withholder_is_excluded_and_recorded() {
    let (members, consortium) = committee();
    let outcome = run(&consortium, &members, &[0, 1, 2]);
    assert!(outcome.value().is_some(), "three of four reveal: t is met");
    assert_eq!(outcome.revealed(), &[0, 1, 2]);
    assert_eq!(outcome.withheld(), &[3]);
    assert_ne!(
        outcome.record(),
        run(&consortium, &members, &[0, 1, 2, 3]).record()
    );

    let cp = Checkpoint::new(NET, consortium.member_set_hash(), 12, [9; 32]);
    let sign = |who: &[usize]| -> Vec<(usize, Signature)> {
        who.iter().map(|&i| (i, members[i].sign(&cp))).collect()
    };
    let with_withholder = sign(&[0, 1, 3]);
    assert!(consortium.verify(&cp, &with_withholder));
    assert!(!consortium.verify_excluding(&cp, &with_withholder, outcome.withheld()));
    assert!(consortium.verify_excluding(&cp, &sign(&[0, 1, 2]), outcome.withheld()));
    assert!(consortium.verify_excluding(&cp, &sign(&[0, 1, 2, 3]), outcome.withheld()));
    assert!(!consortium.verify_excluding(&cp, &sign(&[0, 1]), &[]));
}

/// AT-NET-10: with fewer than `t` reveals there is no beacon, and the withholders are recorded.
#[test]
fn at_net_10_fewer_than_t_reveals_form_no_beacon() {
    let (members, consortium) = committee();
    let outcome = run(&consortium, &members, &[1, 2]);
    assert_eq!(outcome.value(), None);
    assert_eq!(outcome.revealed(), &[1, 2]);
    assert_eq!(outcome.withheld(), &[0, 3]);

    let mut round = BeaconRound::open(&consortium, NET, EPOCH);
    let reveals = commit_all(&mut round, &members, &[0, 1]);
    round.close_commits().unwrap();
    round.close_deposits().unwrap();
    round.reveal(&reveals[0]).unwrap();
    round.reveal(&reveals[1]).unwrap();
    let outcome = round.finish().unwrap();
    assert_eq!(outcome.value(), None, "two commits cannot reach t = 3");
    assert!(
        outcome.withheld().is_empty(),
        "a member that never committed did not withhold"
    );
}

/// AT-NET-10: a commit after the commit set is fixed is refused, before and after the reveals.
#[test]
fn at_net_10_a_commit_after_the_deadline_is_refused() {
    let (members, consortium) = committee();
    let mut round = BeaconRound::open(&consortium, NET, EPOCH);
    let reveals = commit_all(&mut round, &members, &[0, 1, 2]);
    round.close_commits().unwrap();
    let (late, late_reveal) = members[3].beacon_commit(round.id(), 3);
    assert_eq!(round.commit(&late), Err(RoundError::CommitsClosed));
    round.close_deposits().unwrap();
    assert_eq!(round.commit(&late), Err(RoundError::CommitsClosed));
    assert_eq!(round.reveal(&late_reveal), Err(RoundError::NoCommit));
    for r in &reveals[..3] {
        round.reveal(r).unwrap();
    }
    let outcome = round.finish().unwrap();
    assert_eq!(outcome.revealed(), &[0, 1, 2]);
    assert!(outcome.withheld().is_empty());
}

/// AT-NET-10: a commit bound to another network, member set or epoch, or signed by no member.
#[test]
fn at_net_10_a_foreign_or_unsigned_commit_is_refused() {
    let (members, consortium) = committee();
    let mut round = BeaconRound::open(&consortium, NET, EPOCH);
    let here = round.id();
    let elsewhere = |f: fn(&mut RoundId)| {
        let mut r = here;
        f(&mut r);
        members[0].beacon_commit(r, 0).0
    };
    let net = elsewhere(|r| r.network_id = [0x43; 32]);
    let set = elsewhere(|r| r.member_set_hash = [0; 32]);
    let epoch = elsewhere(|r| r.epoch += 1);
    assert_eq!(round.commit(&net), Err(RoundError::WrongNetwork));
    assert_eq!(round.commit(&set), Err(RoundError::WrongMemberSet));
    assert_eq!(round.commit(&epoch), Err(RoundError::WrongEpoch));

    let stranger = Member::from_seed([0xEE; 32]);
    let (as_member_1, _) = stranger.beacon_commit(here, 1);
    assert_eq!(round.commit(&as_member_1), Err(RoundError::BadSignature));
    let (as_member_9, _) = stranger.beacon_commit(here, 9);
    assert_eq!(round.commit(&as_member_9), Err(RoundError::NotAMember));

    let (genuine, _) = members[0].beacon_commit(here, 0);
    let relabeled = BeaconCommit {
        round: RoundId {
            epoch: EPOCH + 1,
            ..here
        },
        ..genuine
    };
    let tampered = BeaconCommit {
        commitment: [1; 32],
        ..genuine
    };
    assert_eq!(round.commit(&relabeled), Err(RoundError::WrongEpoch));
    assert_eq!(round.commit(&tampered), Err(RoundError::BadSignature));
    round.commit(&genuine).unwrap();
    assert_eq!(round.commit(&genuine), Err(RoundError::DuplicateCommit));
}

/// AT-NET-10: a reveal that does not open its commitment is refused, and a copied commitment
/// opens for nobody but its author.
#[test]
fn at_net_10_a_reveal_that_does_not_open_its_commit_is_refused() {
    let (members, consortium) = committee();
    let mut round = BeaconRound::open(&consortium, NET, EPOCH);
    let reveals = commit_all(&mut round, &members, &[0, 2, 3]);
    round.close_commits().unwrap();
    let early = round.reveal(&reveals[0]);
    assert_eq!(early, Err(RoundError::RevealsNotOpen));
    round.close_deposits().unwrap();

    let wrong = BeaconReveal {
        secret: [5; 32],
        ..reveals[0]
    };
    assert_eq!(round.reveal(&wrong), Err(RoundError::RevealMismatch));
    let swapped = BeaconReveal {
        member: 2,
        ..reveals[0]
    };
    assert_eq!(round.reveal(&swapped), Err(RoundError::RevealMismatch));
    let absent = BeaconReveal {
        member: 1,
        ..reveals[0]
    };
    assert_eq!(round.reveal(&absent), Err(RoundError::NoCommit));
    let outsider = BeaconReveal {
        member: 9,
        ..reveals[0]
    };
    assert_eq!(round.reveal(&outsider), Err(RoundError::NoCommit));
    round.reveal(&reveals[0]).unwrap();
    assert_eq!(round.reveal(&reveals[0]), Err(RoundError::AlreadyRevealed));
    round.reveal(&reveals[2]).unwrap();
    let outcome = round.finish().unwrap();
    assert_eq!(outcome.revealed(), &[0, 2]);
    assert_eq!(outcome.withheld(), &[3], "a refused reveal is no reveal");
    assert_eq!(outcome.value(), None);
}

/// AT-NET-10: member 1 signs member 0's commitment as its own; neither secret opens it for 1.
#[test]
fn at_net_10_a_copied_commitment_cannot_be_opened_by_the_copier() {
    let (members, consortium) = committee();
    let mut round = BeaconRound::open(&consortium, NET, EPOCH);
    let (c0, r0) = members[0].beacon_commit(round.id(), 0);
    let (c1, r1) = members[1].beacon_commit(round.id(), 1);
    let copied = BeaconCommit {
        commitment: c0.commitment,
        ..c1
    };
    let copied = BeaconCommit {
        signature: members[1].sign_beacon_commit(&copied),
        ..copied
    };
    round.commit(&c0).unwrap();
    round.commit(&copied).unwrap();
    round.close_commits().unwrap();
    round.close_deposits().unwrap();
    round.reveal(&r0).unwrap();
    let as_1 = BeaconReveal { member: 1, ..r0 };
    assert_eq!(round.reveal(&as_1), Err(RoundError::RevealMismatch));
    assert_eq!(round.reveal(&r1), Err(RoundError::RevealMismatch));
}

/// AT-NET-10: the deadlines run in order: commit set, then deposits, then the reveal deadline.
#[test]
fn at_net_10_the_deadlines_run_in_order() {
    let (members, consortium) = committee();
    let mut round = BeaconRound::open(&consortium, NET, EPOCH);
    commit_all(&mut round, &members, &[0, 1, 2, 3]);
    assert_eq!(round.close_deposits(), Err(RoundError::OutOfPhase));
    round.close_commits().unwrap();
    assert_eq!(round.close_commits(), Err(RoundError::OutOfPhase));
    round.close_deposits().unwrap();
    assert_eq!(round.close_deposits(), Err(RoundError::OutOfPhase));
    assert_eq!(round.close_commits(), Err(RoundError::OutOfPhase));

    let early = BeaconRound::open(&consortium, NET, EPOCH);
    assert_eq!(early.finish(), Err(RoundError::OutOfPhase));
    let mut sealed = BeaconRound::open(&consortium, NET, EPOCH);
    sealed.close_commits().unwrap();
    assert_eq!(sealed.finish(), Err(RoundError::OutOfPhase));
}

/// AT-NET-10: the record tells outcomes apart by who revealed and who withheld, beacon or not.
#[test]
fn at_net_10_the_record_binds_who_revealed_and_who_withheld() {
    let (members, consortium) = committee();
    let outcome = |committers: &[usize], revealers: &[usize]| {
        let mut round = BeaconRound::open(&consortium, NET, EPOCH);
        let reveals = commit_all(&mut round, &members, committers);
        round.close_commits().unwrap();
        round.close_deposits().unwrap();
        for &i in revealers {
            round.reveal(&reveals[i]).unwrap();
        }
        round.finish().unwrap()
    };
    let outcomes = [
        outcome(&[0, 1, 2, 3], &[1, 2]),
        outcome(&[1, 2, 3], &[1, 2]),
        outcome(&[0, 1], &[0]),
        outcome(&[0, 1], &[1]),
        outcome(&[0, 1], &[]),
    ];
    assert!(outcomes.iter().all(|o| o.value().is_none()));
    let records: std::collections::HashSet<_> = outcomes.iter().map(|o| o.record()).collect();
    assert_eq!(records.len(), outcomes.len());
}
