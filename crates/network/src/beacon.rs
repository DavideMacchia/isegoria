//! The epoch's beacon round (`docs/04` §The epoch's beacon, `docs/08` §9.4, D41): the
//! members commit before the deposits close, reveal after, and the beacon hashes the reveals.

use crate::cid::Cid;
use crate::consortium::{Consortium, Member};
use crate::hash::tagged;
use ed25519_dalek::{Signature, Signer, Verifier, VerifyingKey};

/// One round: a network, the member set the round is held under, and the epoch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoundId {
    pub network_id: [u8; 32],
    pub member_set_hash: [u8; 32],
    pub epoch: u64,
}

impl RoundId {
    fn hashed(&self, domain: &str, rest: &[&[u8]]) -> [u8; 32] {
        let epoch = self.epoch.to_le_bytes();
        let mut fields: Vec<&[u8]> = vec![&self.network_id, &self.member_set_hash, &epoch];
        fields.extend_from_slice(rest);
        tagged(domain, &fields)
    }

    fn commitment(&self, key: &VerifyingKey, secret: &[u8; 32]) -> [u8; 32] {
        self.hashed("isegoria/beacon/commitment/v1", &[key.as_bytes(), secret])
    }

    fn commit_message(&self, commitment: &[u8; 32]) -> [u8; 32] {
        self.hashed("isegoria/beacon/commit/v1", &[commitment])
    }
}

/// Member `member`'s signed commitment to its secret for `round`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BeaconCommit {
    pub round: RoundId,
    pub member: usize,
    pub commitment: [u8; 32],
    pub signature: Signature,
}

/// Member `member`'s secret, published once the epoch's deposits are closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BeaconReveal {
    pub member: usize,
    pub secret: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoundError {
    WrongNetwork,
    WrongMemberSet,
    WrongEpoch,
    NotAMember,
    BadSignature,
    DuplicateCommit,
    /// A commit after the commit set was fixed.
    CommitsClosed,
    /// A reveal before the epoch's deposits are closed.
    RevealsNotOpen,
    /// A reveal from a member without a counted commit.
    NoCommit,
    /// The secret does not open the member's commitment.
    RevealMismatch,
    AlreadyRevealed,
    /// A deadline out of order: the round moves Committing → Sealed → Revealing → finished.
    OutOfPhase,
}

impl Member {
    /// This member's commit for `round` as member `member`, and the reveal to hold back.
    pub fn beacon_commit(&self, round: RoundId, member: usize) -> (BeaconCommit, BeaconReveal) {
        let secret = round.hashed("isegoria/beacon/secret/v1", &[self.key.as_bytes()]);
        let commitment = round.commitment(&self.public(), &secret);
        let commit = BeaconCommit {
            round,
            member,
            commitment,
            signature: self.key.sign(&round.commit_message(&commitment)),
        };
        (commit, BeaconReveal { member, secret })
    }

    /// This member's signature over `commit`'s round and commitment.
    pub fn sign_beacon_commit(&self, commit: &BeaconCommit) -> Signature {
        self.key
            .sign(&commit.round.commit_message(&commit.commitment))
    }
}

/// The round's result: the beacon if at least `t` members revealed, who revealed, who
/// withheld (committed, never revealed).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BeaconOutcome {
    round: RoundId,
    value: Option<[u8; 32]>,
    revealed: Vec<usize>,
    withheld: Vec<usize>,
}

fn indices(list: &[usize]) -> Vec<u8> {
    list.iter()
        .flat_map(|&i| (i as u64).to_le_bytes())
        .collect()
}

impl BeaconOutcome {
    pub fn round(&self) -> RoundId {
        self.round
    }

    pub fn value(&self) -> Option<[u8; 32]> {
        self.value
    }

    pub fn revealed(&self) -> &[usize] {
        &self.revealed
    }

    pub fn withheld(&self) -> &[usize] {
        &self.withheld
    }

    /// The content id of the outcome's record, appended to the log with the epoch.
    pub fn record(&self) -> Cid {
        let value = self.value.as_ref().map_or(&[][..], |v| &v[..]);
        let (revealed, withheld) = (indices(&self.revealed), indices(&self.withheld));
        Cid(self
            .round
            .hashed("isegoria/beacon/outcome/v1", &[value, &revealed, &withheld]))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Committing,
    Sealed,
    Revealing,
}

pub struct BeaconRound<'a> {
    consortium: &'a Consortium,
    round: RoundId,
    phase: Phase,
    commitments: Vec<Option<[u8; 32]>>,
    secrets: Vec<Option<[u8; 32]>>,
}

/// Each member's entry in member order, empty where it has none.
fn by_member(entries: &[Option<[u8; 32]>]) -> Vec<&[u8]> {
    entries
        .iter()
        .map(|e| e.as_ref().map_or(&[][..], |v| &v[..]))
        .collect()
}

impl<'a> BeaconRound<'a> {
    pub fn open(consortium: &'a Consortium, network_id: [u8; 32], epoch: u64) -> Self {
        let n = consortium.keys().len();
        BeaconRound {
            round: RoundId {
                network_id,
                member_set_hash: consortium.member_set_hash(),
                epoch,
            },
            consortium,
            phase: Phase::Committing,
            commitments: vec![None; n],
            secrets: vec![None; n],
        }
    }

    pub fn id(&self) -> RoundId {
        self.round
    }

    pub fn commit(&mut self, commit: &BeaconCommit) -> Result<(), RoundError> {
        if self.phase != Phase::Committing {
            return Err(RoundError::CommitsClosed);
        }
        if commit.round.network_id != self.round.network_id {
            return Err(RoundError::WrongNetwork);
        }
        if commit.round.member_set_hash != self.round.member_set_hash {
            return Err(RoundError::WrongMemberSet);
        }
        if commit.round.epoch != self.round.epoch {
            return Err(RoundError::WrongEpoch);
        }
        let key = self
            .consortium
            .keys()
            .get(commit.member)
            .ok_or(RoundError::NotAMember)?;
        key.verify(
            &self.round.commit_message(&commit.commitment),
            &commit.signature,
        )
        .map_err(|_| RoundError::BadSignature)?;
        let slot = &mut self.commitments[commit.member];
        if slot.is_some() {
            return Err(RoundError::DuplicateCommit);
        }
        *slot = Some(commit.commitment);
        Ok(())
    }

    /// The commit deadline: fixes the commit set and returns its record's content id, to
    /// append to the log before the deposits close.
    pub fn close_commits(&mut self) -> Result<Cid, RoundError> {
        if self.phase != Phase::Committing {
            return Err(RoundError::OutOfPhase);
        }
        self.phase = Phase::Sealed;
        let set = by_member(&self.commitments);
        Ok(Cid(self
            .round
            .hashed("isegoria/beacon/commit-set/v1", &set)))
    }

    /// The epoch's deposit checkpoint is signed: the reveals open.
    pub fn close_deposits(&mut self) -> Result<(), RoundError> {
        if self.phase != Phase::Sealed {
            return Err(RoundError::OutOfPhase);
        }
        self.phase = Phase::Revealing;
        Ok(())
    }

    pub fn reveal(&mut self, reveal: &BeaconReveal) -> Result<(), RoundError> {
        if self.phase != Phase::Revealing {
            return Err(RoundError::RevealsNotOpen);
        }
        let commitment = self
            .commitments
            .get(reveal.member)
            .copied()
            .flatten()
            .ok_or(RoundError::NoCommit)?;
        if self.secrets[reveal.member].is_some() {
            return Err(RoundError::AlreadyRevealed);
        }
        let key = &self.consortium.keys()[reveal.member];
        if self.round.commitment(key, &reveal.secret) != commitment {
            return Err(RoundError::RevealMismatch);
        }
        self.secrets[reveal.member] = Some(reveal.secret);
        Ok(())
    }

    /// The reveal deadline.
    pub fn finish(self) -> Result<BeaconOutcome, RoundError> {
        if self.phase != Phase::Revealing {
            return Err(RoundError::OutOfPhase);
        }
        let members = 0..self.secrets.len();
        let revealed: Vec<usize> = members
            .clone()
            .filter(|&i| self.secrets[i].is_some())
            .collect();
        let withheld: Vec<usize> = members
            .filter(|&i| self.commitments[i].is_some() && self.secrets[i].is_none())
            .collect();
        let value = (revealed.len() >= self.consortium.threshold()).then(|| {
            let secrets = by_member(&self.secrets);
            self.round.hashed("isegoria/beacon/value/v1", &secrets)
        });
        Ok(BeaconOutcome {
            round: self.round,
            value,
            revealed,
            withheld,
        })
    }
}
