//! The beacon of an honest round (`docs/04` §The epoch's beacon): four members, `t = 3`.

use network::beacon::BeaconRound;
use network::consortium::{Consortium, Member};
use protocol::randomness::Beacon;

/// The beacon a four-member consortium forms for `epoch` on network `[net; 32]`, all revealing.
pub fn beacon(net: u8, epoch: u64) -> Beacon {
    let members: Vec<Member> = (1u8..=4).map(|i| Member::from_seed([i; 32])).collect();
    let consortium = Consortium::new(members.iter().map(Member::public).collect(), 3);
    let mut round = BeaconRound::open(&consortium, [net; 32], epoch);
    let reveals: Vec<_> = members
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let (commit, reveal) = m.beacon_commit(round.id(), i);
            round.commit(&commit).expect("an honest commit");
            reveal
        })
        .collect();
    round.close_commits().expect("the commit deadline");
    round.close_deposits().expect("the deposit deadline");
    for r in &reveals {
        round.reveal(r).expect("an honest reveal");
    }
    let outcome = round.finish().expect("the reveal deadline");
    Beacon::from_outcome(&outcome).expect("every member revealed")
}
