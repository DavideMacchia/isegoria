//! Consortium checkpoints (`docs/04`, §The consortium as backbone). A few dozen
//! heterogeneous signers co-sign the log head. Security comes from the diversity of
//! who controls the machines, so a checkpoint needs a threshold `t` of `n` signers.

use crate::hash::tagged;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

/// A signed state: the log head at a given height.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Checkpoint {
    pub height: u64,
    pub head: [u8; 32],
}

impl Checkpoint {
    fn message(&self) -> [u8; 32] {
        tagged(
            "isegoria/checkpoint/v1",
            &[&self.height.to_le_bytes(), &self.head],
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
