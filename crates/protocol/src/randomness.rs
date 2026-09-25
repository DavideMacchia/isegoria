//! Public randomness for the epoch (`docs/08` INV-10 / CRYPTO-008, `docs/01` D29, T8).
//!
//! Every draw — the admission lottery, reviewer assignment, honeypot placement, meta-level
//! sortition — must be seeded from randomness that is fixed *after* the candidate set is
//! closed and that no participant can influence or predict. The signed consortium
//! [`Checkpoint`] head is that beacon: it commits to every deposit in the epoch and is
//! fixed once a threshold of members co-signs it (verify with `Consortium::verify` before
//! trusting one). So an author cannot grind a draft to steer a draw — the beacon is not
//! known at deposit time, and the per-item seed is keyed on a byte-independent slot index,
//! never the draft bytes (AT-BR-05).
//!
//! Residual (T37, `docs/08` CRYPTO-008): the head is a deterministic function of the log
//! content, so whoever controls the last deposits before the checkpoint — the publisher,
//! a signing threshold, or a last depositor who sees the log — can try variants and keep
//! the seed they prefer. The beacon is not yet separated from the state commitment.
//!
//! Seeds are domain-separated per purpose and per index, so the draws never share a
//! stream. The `_from_beacon` wrappers in `lottery`, `review`, `honeypot`, `exploration`
//! and `governance` are the sanctioned entry points; the raw draws take a `u64` seed for
//! unit testing.

use network::consortium::Checkpoint;
use sha2::{Digest, Sha256};

/// The epoch's public randomness beacon: the signed checkpoint head and height.
#[derive(Clone, Copy, Debug)]
pub struct Beacon {
    head: [u8; 32],
    height: u64,
}

impl Beacon {
    /// From a checkpoint the consortium has signed. This type only reads the head; the
    /// caller MUST have verified the signatures (`Consortium::verify`) first.
    pub fn from_checkpoint(cp: &Checkpoint) -> Self {
        Beacon {
            head: cp.head,
            height: cp.height,
        }
    }

    /// A `u64` seed for `purpose` at `index`: `H(head ‖ height ‖ purpose ‖ index)`. The
    /// `index` is a byte-independent selector — an epoch number, or an admitted-slot index
    /// for a per-item draw — never the draft bytes, so a panel does not depend on draft
    /// content (AT-BR-05).
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

/// Domain tags for the draws (INV-10), so their seed streams never coincide.
pub const LOTTERY: &[u8] = b"lottery";
pub const REVIEW_ASSIGNMENT: &[u8] = b"review-assignment";
/// The band's extra panel (D26, T60): a stream of its own, so the extra reviewers of an
/// item are not a function of its first panel's draw.
pub const EXTRA_REVIEW: &[u8] = b"extra-review";
/// The exploration draw (D35, T52): which gate rejections are piloted for measurement
/// only, keyed on the admitted slot — a stream of its own, so it is not a function of the
/// item's panel or of the lottery.
pub const EXPLORATION: &[u8] = b"exploration";
pub const HONEYPOT: &[u8] = b"honeypot";
pub const SORTITION: &[u8] = b"sortition";
