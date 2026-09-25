//! Checkpoint ingestion (NET-006): arbitrary sequences of checkpoints, some honestly
//! signed and some carrying attacker-chosen signature bytes/indices, fed to a
//! `CheckpointClient` with and without the log. No panic, and the §9.4 invariants hold.
#![no_main]

use arbitrary::Arbitrary;
use ed25519_dalek::Signature;
use libfuzzer_sys::fuzz_target;
use network::cid::cid;
use network::consortium::{
    Checkpoint, CheckpointClient, CheckpointReject, CheckpointUpdate, Consortium, Member,
};
use network::log::TransparencyLog;

const MEMBERS: usize = 5;
const THRESHOLD: usize = 3;
const NETWORK: [u8; 32] = [7u8; 32];

#[derive(Debug, Arbitrary)]
enum Sig {
    /// Member `i % MEMBERS` signs, under its own index.
    Member(u8),
    /// Arbitrary bytes under an arbitrary index.
    Raw(usize, [u8; 64]),
}

#[derive(Debug, Arbitrary)]
enum Op {
    Append(Vec<u8>),
    Ingest {
        /// `None`: the local log's current length and head.
        height: Option<u64>,
        head: Option<[u8; 32]>,
        foreign_network: bool,
        foreign_members: bool,
        sigs: Vec<Sig>,
        with_log: bool,
    },
}

fn consortium(members: &[Member]) -> Consortium {
    Consortium::new(members.iter().map(Member::public).collect(), THRESHOLD)
}

fuzz_target!(|ops: Vec<Op>| {
    let members: Vec<Member> = (0..MEMBERS as u8)
        .map(|i| Member::from_seed([i; 32]))
        .collect();
    let member_set = consortium(&members).member_set_hash();
    let mut client = CheckpointClient::new(NETWORK, consortium(&members));
    let mut log = TransparencyLog::new();

    for op in ops {
        match op {
            Op::Append(payload) => {
                log.append(cid(&payload));
            }
            Op::Ingest {
                height,
                head,
                foreign_network,
                foreign_members,
                sigs,
                with_log,
            } => {
                let cp = Checkpoint::new(
                    if foreign_network { [8u8; 32] } else { NETWORK },
                    if foreign_members {
                        [9u8; 32]
                    } else {
                        member_set
                    },
                    height.unwrap_or(log.len() as u64),
                    head.unwrap_or(log.head()),
                );
                let mut signers = [false; MEMBERS];
                let sigs: Vec<(usize, Signature)> = sigs
                    .iter()
                    .map(|s| match s {
                        Sig::Member(i) => {
                            let i = usize::from(*i) % MEMBERS;
                            signers[i] = true;
                            (i, members[i].sign(&cp))
                        }
                        Sig::Raw(i, bytes) => (*i, Signature::from_bytes(bytes)),
                    })
                    .collect();
                let honest = signers.iter().filter(|s| **s).count();

                let before = client.trusted().copied();
                let update = if with_log {
                    client.ingest_with_log(&cp, &sigs, &log)
                } else {
                    client.ingest(&cp, &sigs)
                };
                let after = client.trusted().copied();

                if update == CheckpointUpdate::Accepted {
                    assert_eq!(after, Some(cp));
                    assert!(!foreign_network && !foreign_members);
                    // Forging an ed25519 signature is out of reach: acceptance means a
                    // threshold of honest member signatures.
                    assert!(honest >= THRESHOLD);
                    if let Some(prior) = before {
                        assert!(cp.height > prior.height);
                        // Every checkpoint after the first must be extended by the log.
                        if with_log {
                            assert!(log.verify_extends(&cp).is_ok());
                        }
                    }
                } else {
                    assert_eq!(after, before);
                }
                if honest >= THRESHOLD && !foreign_network && !foreign_members {
                    assert_ne!(
                        update,
                        CheckpointUpdate::Rejected(CheckpointReject::InsufficientSignatures)
                    );
                }
            }
        }
    }
});
