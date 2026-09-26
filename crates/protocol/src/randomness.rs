//! Public randomness for the epoch: every draw seeds from the epoch's commit-reveal beacon,
//! domain-separated per purpose and per index (`docs/08` INV-10, CRYPTO-008; `docs/01` D41).

use network::beacon::BeaconOutcome;
use sha2::{Digest, Sha256};

/// The value of an epoch's beacon round (`network::beacon`, `docs/04` §The epoch's beacon).
#[derive(Clone, Copy, Debug)]
pub struct Beacon {
    value: [u8; 32],
}

impl Beacon {
    /// The epoch's beacon, if its round formed one (at least `t` reveals).
    pub fn from_outcome(outcome: &BeaconOutcome) -> Option<Self> {
        outcome.value().map(|value| Beacon { value })
    }

    /// `H(beacon ‖ purpose ‖ index)` as a `u64`. `index` is an epoch or admitted-slot index,
    /// never draft bytes (AT-BR-05).
    pub fn seed(&self, purpose: &[u8], index: u64) -> u64 {
        let mut h = Sha256::new();
        h.update(b"isegoria/beacon/v2");
        h.update(self.value);
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
pub const CONTESTED: &[u8] = b"contested";
pub const HONEYPOT: &[u8] = b"honeypot";
pub const SORTITION: &[u8] = b"sortition";
