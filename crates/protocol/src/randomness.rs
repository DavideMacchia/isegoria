//! Public randomness for the epoch: every draw is seeded from the signed checkpoint head,
//! domain-separated per purpose and per index (`docs/08` INV-10, CRYPTO-008; `docs/01` D29).

use network::consortium::Checkpoint;
use sha2::{Digest, Sha256};

/// The signed checkpoint head and height. The caller must have verified the checkpoint's
/// signatures (`Consortium::verify`) first.
#[derive(Clone, Copy, Debug)]
pub struct Beacon {
    head: [u8; 32],
    height: u64,
}

impl Beacon {
    pub fn from_checkpoint(cp: &Checkpoint) -> Self {
        Beacon {
            head: cp.head,
            height: cp.height,
        }
    }

    /// `H(head ‖ height ‖ purpose ‖ index)` as a `u64`. `index` is an epoch or admitted-slot
    /// index, never draft bytes (AT-BR-05).
    pub fn seed(&self, purpose: &[u8], index: u64) -> u64 {
        let mut h = Sha256::new();
        h.update(b"isegoria/beacon/v1");
        h.update(self.head);
        h.update(self.height.to_le_bytes());
        h.update((purpose.len() as u64).to_le_bytes());
        h.update(purpose);
        h.update(index.to_le_bytes());
        let d = h.finalize();
        u64::from_le_bytes(d[..8].try_into().expect("SHA-256 yields 32 bytes"))
    }
}

/// Domain tags for the draws (INV-10).
pub const LOTTERY: &[u8] = b"lottery";
pub const REVIEW_ASSIGNMENT: &[u8] = b"review-assignment";
pub const EXTRA_REVIEW: &[u8] = b"extra-review";
pub const EXPLORATION: &[u8] = b"exploration";
pub const HONEYPOT: &[u8] = b"honeypot";
pub const SORTITION: &[u8] = b"sortition";
