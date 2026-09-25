//! Checkpoint hardening (`docs/08` NET-006 §9.4, DS-3): a light node tracks the last
//! trusted head for one network + member set. AT-NET-03 (replay → `Stale`), AT-NET-04
//! (equivocation → `Forked`), AT-NET-05 (wrong network → rejected).

use network::consortium::{
    Checkpoint, CheckpointClient, CheckpointReject, CheckpointUpdate, Consortium, Member,
};

const NET_A: [u8; 32] = [0xAA; 32];
const NET_B: [u8; 32] = [0xBB; 32];

fn committee() -> (Vec<Member>, Consortium) {
    let members: Vec<Member> = (0u8..4).map(|i| Member::from_seed([i + 1; 32])).collect();
    let consortium = Consortium::new(members.iter().map(|m| m.public()).collect(), 3);
    (members, consortium)
}

/// A checkpoint co-signed by a threshold of `members`.
fn signed(
    members: &[Member],
    network_id: [u8; 32],
    member_set_hash: [u8; 32],
    height: u64,
    head: [u8; 32],
) -> (Checkpoint, Vec<(usize, ed25519_dalek::Signature)>) {
    let cp = Checkpoint::new(network_id, member_set_hash, height, head);
    let sigs = members
        .iter()
        .enumerate()
        .take(3)
        .map(|(i, m)| (i, m.sign(&cp)))
        .collect();
    (cp, sigs)
}

fn client() -> (Vec<Member>, CheckpointClient, [u8; 32]) {
    let (members, consortium) = committee();
    let msh = consortium.member_set_hash();
    (members, CheckpointClient::new(NET_A, consortium), msh)
}

#[test]
fn at_net_03_an_old_checkpoint_is_ignored_as_a_replay() {
    let (members, mut client, msh) = client();

    let (cp10, s10) = signed(&members, NET_A, msh, 10, [10u8; 32]);
    assert_eq!(client.ingest(&cp10, &s10), CheckpointUpdate::Accepted);

    // A valid but older checkpoint (height 5) is a replay, ignored — the trusted head stays.
    let (cp5, s5) = signed(&members, NET_A, msh, 5, [5u8; 32]);
    assert_eq!(client.ingest(&cp5, &s5), CheckpointUpdate::Stale);
    assert_eq!(client.trusted().unwrap().height, 10);

    // Re-presenting the same height is also stale (idempotent), and a higher one advances.
    assert_eq!(client.ingest(&cp10, &s10), CheckpointUpdate::Stale);
    let (cp11, s11) = signed(&members, NET_A, msh, 11, [11u8; 32]);
    assert_eq!(client.ingest(&cp11, &s11), CheckpointUpdate::Accepted);
    assert_eq!(client.trusted().unwrap().height, 11);
}

#[test]
fn at_net_04_equivocation_at_one_height_raises_a_fork_alarm() {
    let (members, mut client, msh) = client();

    let (cp, s) = signed(&members, NET_A, msh, 7, [1u8; 32]);
    assert_eq!(client.ingest(&cp, &s), CheckpointUpdate::Accepted);

    // A second threshold-signed checkpoint at the SAME height with a DIFFERENT head: the
    // consortium signed two conflicting histories — equivocation, with both kept as evidence.
    let (conflict, cs) = signed(&members, NET_A, msh, 7, [2u8; 32]);
    assert_eq!(
        client.ingest(&conflict, &cs),
        CheckpointUpdate::Forked {
            trusted: cp,
            conflicting: conflict
        }
    );
}

#[test]
fn at_net_05_a_checkpoint_for_another_network_is_rejected() {
    let (members, mut client, msh) = client();

    // Same committee, but the checkpoint is stamped for network B: presented to a network-A
    // client it is rejected, even though the signatures are valid over its own message.
    let (cp_b, s_b) = signed(&members, NET_B, msh, 1, [9u8; 32]);
    assert_eq!(
        client.ingest(&cp_b, &s_b),
        CheckpointUpdate::Rejected(CheckpointReject::WrongNetwork)
    );
    assert!(client.trusted().is_none(), "nothing was accepted");
}

#[test]
fn a_different_member_set_or_too_few_signatures_is_rejected() {
    let (members, mut client, msh) = client();

    // Right network, but the checkpoint commits to a different member set.
    let (cp_wrong_set, s) = signed(&members, NET_A, [0xEE; 32], 1, [1u8; 32]);
    assert_eq!(
        client.ingest(&cp_wrong_set, &s),
        CheckpointUpdate::Rejected(CheckpointReject::WrongMemberSet)
    );

    // Correct binding, but only two signatures (threshold is three).
    let cp = Checkpoint::new(NET_A, msh, 1, [1u8; 32]);
    let two: Vec<_> = members
        .iter()
        .enumerate()
        .take(2)
        .map(|(i, m)| (i, m.sign(&cp)))
        .collect();
    assert_eq!(
        client.ingest(&cp, &two),
        CheckpointUpdate::Rejected(CheckpointReject::InsufficientSignatures)
    );
}
