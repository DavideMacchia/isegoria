//! Consortium checkpoints (`docs/04`, §The consortium as backbone). A few dozen
//! heterogeneous signers co-sign the log head. Security comes from the diversity of
//! who controls the machines, so a checkpoint needs a threshold `t` of `n` signers.

use crate::hash::tagged;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

/// A signed state: the log head at a given height, bound to the network and the member
/// set that signs it (`network_id`, `member_set_hash`) so it cannot be replayed onto
/// another network or a different consortium (NET-006, T15).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Checkpoint {
    pub network_id: [u8; 32],
    pub member_set_hash: [u8; 32],
    pub height: u64,
    pub head: [u8; 32],
}

impl Checkpoint {
    /// Convenience constructor.
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
        // v2 (T15): the network id and member-set hash are inside the signed message, so a
        // signature is valid only for its own network and consortium.
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

/// One consortium signer. In production keys are held by distinct organizations in
/// different jurisdictions; here a key is built deterministically from a seed.
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

/// The set of member public keys and the signature threshold.
pub struct Consortium {
    members: Vec<VerifyingKey>,
    threshold: usize,
}

impl Consortium {
    pub fn new(members: Vec<VerifyingKey>, threshold: usize) -> Self {
        Consortium { members, threshold }
    }

    /// Hash of the ordered member public keys (NET-006, T15). Placed in a checkpoint's
    /// signed message so the checkpoint commits to *which* set signed it; a client with a
    /// different member set rejects it.
    pub fn member_set_hash(&self) -> [u8; 32] {
        let keys: Vec<[u8; 32]> = self.members.iter().map(|k| k.to_bytes()).collect();
        let refs: Vec<&[u8]> = keys.iter().map(|k| k.as_slice()).collect();
        tagged("isegoria/consortium/member-set/v1", &refs)
    }

    /// Accepts a checkpoint if at least `threshold` distinct members produced a
    /// valid signature over it.
    pub fn verify(&self, cp: &Checkpoint, sigs: &[(usize, Signature)]) -> bool {
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

/// A light node's decision on an incoming checkpoint (NET-006, §9.4, T15).
// `Forked` carries two checkpoints (the equivocation evidence); the size gap is fine for
// a value returned once on a rare alarm path, not stored in bulk.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckpointUpdate {
    /// A strictly higher checkpoint with correct binding and enough signatures: the new
    /// trusted head.
    Accepted,
    /// A checkpoint at or below the trusted height (or a duplicate of it): a replay,
    /// ignored (AT-NET-03).
    Stale,
    /// Two threshold-signed checkpoints at the same height with different heads — the
    /// consortium equivocated (AT-NET-04). Both are kept as accountable evidence.
    Forked {
        trusted: Checkpoint,
        conflicting: Checkpoint,
    },
    /// Rejected before trust: wrong network, wrong member set, or too few signatures.
    Rejected(CheckpointReject),
}

/// Why a checkpoint was rejected outright (before the monotonicity/fork rules).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckpointReject {
    /// The checkpoint is for another network (cross-network replay, AT-NET-05).
    WrongNetwork,
    /// The checkpoint commits to a different member set than this client trusts.
    WrongMemberSet,
    /// Fewer than the threshold of valid distinct signatures.
    InsufficientSignatures,
}

/// A light node that follows one network's checkpoints under a fixed member set,
/// enforcing the §9.4 rules: reject a foreign network or member set, ignore a replayed
/// (non-monotonic) height, and alarm on equivocation. Higher-height fork detection (a new
/// head that does not extend the trusted one) additionally needs a log consistency proof
/// (`log::verify_extends`, T14), which a client holding the log can combine.
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

    /// The last accepted checkpoint, if any.
    pub fn trusted(&self) -> Option<&Checkpoint> {
        self.trusted.as_ref()
    }

    /// Processes an incoming checkpoint and its signatures against the trusted state.
    pub fn ingest(&mut self, cp: &Checkpoint, sigs: &[(usize, Signature)]) -> CheckpointUpdate {
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
            Some(t) if cp.height > t.height => {
                self.trusted = Some(*cp);
                CheckpointUpdate::Accepted
            }
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
