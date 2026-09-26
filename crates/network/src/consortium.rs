//! Consortium checkpoints (`docs/04` §The consortium as backbone): a threshold `t` of `n`
//! heterogeneous signers co-sign the log head.

use crate::hash::tagged;
use crate::log::{ConsistencyError, TransparencyLog};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use std::collections::HashSet;

/// A signed log head bound to its network and signing member set (NET-006).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Checkpoint {
    pub network_id: [u8; 32],
    pub member_set_hash: [u8; 32],
    pub height: u64,
    pub head: [u8; 32],
}

impl Checkpoint {
    pub fn new(
        network_id: [u8; 32],
        member_set_hash: [u8; 32],
        height: u64,
        head: [u8; 32],
    ) -> Self {
        Checkpoint {
            network_id,
            member_set_hash,
            height,
            head,
        }
    }

    fn message(&self) -> [u8; 32] {
        // Network id and member-set hash sit inside the message (v2): a signature binds
        // to its own network and consortium.
        tagged(
            "isegoria/checkpoint/v2",
            &[
                &self.network_id,
                &self.member_set_hash,
                &self.height.to_le_bytes(),
                &self.head,
            ],
        )
    }
}

pub struct Member {
    key: SigningKey,
}

impl Member {
    pub fn from_seed(seed: [u8; 32]) -> Self {
        Member {
            key: SigningKey::from_bytes(&seed),
        }
    }

    pub fn public(&self) -> VerifyingKey {
        self.key.verifying_key()
    }

    pub fn sign(&self, cp: &Checkpoint) -> Signature {
        self.key.sign(&cp.message())
    }
}

pub struct Consortium {
    members: Vec<VerifyingKey>,
    threshold: usize,
    member_set_hash: [u8; 32],
}

impl Consortium {
    /// # Panics
    /// Unless `1 <= threshold <= n` with distinct keys: operator configuration (`docs/12` §2.2).
    pub fn new(members: Vec<VerifyingKey>, threshold: usize) -> Self {
        assert!(
            (1..=members.len()).contains(&threshold),
            "need 1 <= threshold <= members, got {threshold} of {}",
            members.len()
        );
        let keys: Vec<[u8; 32]> = members.iter().map(|k| k.to_bytes()).collect();
        let distinct: HashSet<&[u8; 32]> = keys.iter().collect();
        assert_eq!(distinct.len(), keys.len(), "need distinct member keys");
        let refs: Vec<&[u8]> = keys.iter().map(|k| k.as_slice()).collect();
        Consortium {
            members,
            threshold,
            member_set_hash: tagged("isegoria/consortium/member-set/v1", &refs),
        }
    }

    /// Hash of the ordered member public keys: commits a checkpoint to *which* set signed it.
    pub fn member_set_hash(&self) -> [u8; 32] {
        self.member_set_hash
    }

    /// Accepts a checkpoint declaring this member set, signed by `threshold` distinct members.
    pub fn verify(&self, cp: &Checkpoint, sigs: &[(usize, Signature)]) -> bool {
        if cp.member_set_hash != self.member_set_hash {
            return false;
        }
        let msg = cp.message();
        let mut seen = vec![false; self.members.len()];
        let mut valid = 0;
        for (idx, sig) in sigs {
            let Some(pk) = self.members.get(*idx) else {
                continue;
            };
            if seen[*idx] {
                continue;
            }
            if pk.verify(&msg, sig).is_ok() {
                seen[*idx] = true;
                valid += 1;
            }
        }
        valid >= self.threshold
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckpointUpdate {
    /// A strictly higher, correctly bound checkpoint with enough signatures: the new trusted head.
    Accepted,
    /// At or below the trusted height, or a duplicate: an ignored replay (AT-NET-03).
    Stale,
    /// Same height, different heads, both threshold-signed: equivocation (AT-NET-04).
    Forked {
        trusted: Checkpoint,
        conflicting: Checkpoint,
    },
    /// Rejected before trust: wrong network, wrong member set, or too few signatures.
    Rejected(CheckpointReject),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckpointReject {
    /// Another network: cross-network replay (AT-NET-05).
    WrongNetwork,
    WrongMemberSet,
    InsufficientSignatures,
    /// The local log reaches neither head ([`CheckpointClient::ingest_with_log`]): sync, retry.
    LogBehind,
    /// [`CheckpointClient::ingest_with_log`]: the local log diverges from the trusted
    /// checkpoint — the local copy, not the new one, is at fault.
    LocalLogDiverged,
}

/// A light node following one network's checkpoints under a fixed member set (§9.4).
pub struct CheckpointClient {
    network_id: [u8; 32],
    member_set_hash: [u8; 32],
    consortium: Consortium,
    trusted: Option<Checkpoint>,
}

impl CheckpointClient {
    pub fn new(network_id: [u8; 32], consortium: Consortium) -> Self {
        let member_set_hash = consortium.member_set_hash();
        CheckpointClient {
            network_id,
            member_set_hash,
            consortium,
            trusted: None,
        }
    }

    pub fn trusted(&self) -> Option<&Checkpoint> {
        self.trusted.as_ref()
    }

    /// Accepts any correctly signed higher checkpoint; without the log it cannot check
    /// that the new head extends the trusted one (use [`Self::ingest_with_log`]).
    pub fn ingest(&mut self, cp: &Checkpoint, sigs: &[(usize, Signature)]) -> CheckpointUpdate {
        self.ingest_checked(cp, sigs, |_| Ok(()))
    }

    /// Like [`Self::ingest`], but accepts a higher checkpoint only if `log` extends both
    /// (`log::verify_extends`); a fork on a different history is `Forked`, not trusted.
    pub fn ingest_with_log(
        &mut self,
        cp: &Checkpoint,
        sigs: &[(usize, Signature)],
        log: &TransparencyLog,
    ) -> CheckpointUpdate {
        self.ingest_checked(cp, sigs, |trusted| {
            match log.verify_extends(trusted) {
                Ok(()) => {}
                // Shorter than the trusted height: behind, nothing shows it diverged (T43).
                Err(ConsistencyError::Truncated { .. }) => {
                    return Err(CheckpointUpdate::Rejected(CheckpointReject::LogBehind));
                }
                Err(_) => {
                    return Err(CheckpointUpdate::Rejected(
                        CheckpointReject::LocalLogDiverged,
                    ));
                }
            }
            match log.verify_extends(cp) {
                Ok(()) => Ok(()),
                Err(ConsistencyError::Truncated { .. }) => {
                    Err(CheckpointUpdate::Rejected(CheckpointReject::LogBehind))
                }
                Err(_) => Err(CheckpointUpdate::Forked {
                    trusted: *trusted,
                    conflicting: *cp,
                }),
            }
        })
    }

    fn ingest_checked(
        &mut self,
        cp: &Checkpoint,
        sigs: &[(usize, Signature)],
        extends: impl FnOnce(&Checkpoint) -> Result<(), CheckpointUpdate>,
    ) -> CheckpointUpdate {
        if cp.network_id != self.network_id {
            return CheckpointUpdate::Rejected(CheckpointReject::WrongNetwork);
        }
        if cp.member_set_hash != self.member_set_hash {
            return CheckpointUpdate::Rejected(CheckpointReject::WrongMemberSet);
        }
        if !self.consortium.verify(cp, sigs) {
            return CheckpointUpdate::Rejected(CheckpointReject::InsufficientSignatures);
        }
        match self.trusted {
            None => {
                self.trusted = Some(*cp);
                CheckpointUpdate::Accepted
            }
            Some(t) if cp.height > t.height => match extends(&t) {
                Ok(()) => {
                    self.trusted = Some(*cp);
                    CheckpointUpdate::Accepted
                }
                Err(update) => update,
            },
            // Same height, different head from a threshold of signers: equivocation.
            Some(t) if cp.height == t.height && cp.head != t.head => CheckpointUpdate::Forked {
                trusted: t,
                conflicting: *cp,
            },
            // Same head, or a lower height: a stale replay to ignore.
            Some(_) => CheckpointUpdate::Stale,
        }
    }
}
