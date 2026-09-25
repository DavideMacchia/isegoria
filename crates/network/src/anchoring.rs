//! Anchoring to a public chain (`docs/04` §Anchoring): real OpenTimestamps proof format
//! and verification; the calendar/Bitcoin network is an injected block source
//! (`OtsAnchor`); untrusted receipts are bounds-checked first (AT-NET-07, `docs/12` §3).

use opentimestamps::attestation::Attestation;
use opentimestamps::op::Op;
use opentimestamps::ser::{DetachedTimestampFile, DigestType};
use opentimestamps::timestamp::{Step, StepData, Timestamp};
use std::collections::HashMap;

/// A proof that a root was anchored: the serialized OpenTimestamps `.ots` timestamp.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    pub root: [u8; 32],
    /// A serialized `opentimestamps::DetachedTimestampFile`.
    pub proof: Vec<u8>,
}

/// The result of verifying a receipt against the known chain state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnchorState {
    /// A Bitcoin attestation was found and matches the block's Merkle root at `height`.
    Confirmed { height: usize },
    /// A well-formed proof still waiting on a calendar/Bitcoin attestation.
    Pending,
    /// The proof did not parse, did not commit to `root`, or its attestation did not
    /// match the known chain state.
    Invalid,
}

/// External anchoring service.
pub trait Anchor {
    fn submit(&mut self, root: [u8; 32]) -> Receipt;
    fn verify(&self, receipt: &Receipt) -> AnchorState;
}

pub struct OtsAnchor {
    calendar_uri: String,
    blocks: HashMap<usize, [u8; 32]>,
    next_height: usize,
}

impl OtsAnchor {
    pub fn new(calendar_uri: impl Into<String>) -> Self {
        OtsAnchor {
            calendar_uri: calendar_uri.into(),
            blocks: HashMap::new(),
            next_height: 0,
        }
    }

    /// Simulates the calendar upgrading a pending receipt to a Bitcoin attestation.
    /// Production fetches the real upgraded `.ots`; here one hashing step stands in for
    /// the aggregation path, recorded as a new block's Merkle root.
    pub fn upgrade(&mut self, receipt: &Receipt) -> Receipt {
        let step_op = Op::Sha256;
        let digest = step_op.execute(&receipt.root);

        let height = self.next_height;
        self.next_height += 1;
        let mut merkle_root = [0u8; 32];
        merkle_root.copy_from_slice(&digest);
        self.blocks.insert(height, merkle_root);

        let timestamp = Timestamp {
            start_digest: receipt.root.to_vec(),
            first_step: Step {
                data: StepData::Op(Op::Sha256),
                output: digest.clone(),
                next: vec![Step {
                    data: StepData::Attestation(Attestation::Bitcoin { height }),
                    output: digest,
                    next: vec![],
                }],
            },
        };
        Receipt {
            root: receipt.root,
            proof: serialize(&timestamp),
        }
    }

    /// Walk a step tree, returning the strongest attestation reached.
    fn walk(&self, step: &Step) -> AnchorState {
        match &step.data {
            StepData::Attestation(Attestation::Bitcoin { height }) => {
                match self.blocks.get(height) {
                    Some(root) if root.as_slice() == step.output.as_slice() => {
                        AnchorState::Confirmed { height: *height }
                    }
                    _ => AnchorState::Invalid,
                }
            }
            StepData::Attestation(_) => AnchorState::Pending,
            StepData::Op(_) | StepData::Fork => step
                .next
                .iter()
                .map(|n| self.walk(n))
                .fold(AnchorState::Invalid, stronger),
        }
    }
}

impl Anchor for OtsAnchor {
    fn submit(&mut self, root: [u8; 32]) -> Receipt {
        let timestamp = Timestamp {
            start_digest: root.to_vec(),
            first_step: Step {
                data: StepData::Attestation(Attestation::Pending {
                    uri: self.calendar_uri.clone(),
                }),
                output: root.to_vec(),
                next: vec![],
            },
        };
        Receipt {
            root,
            proof: serialize(&timestamp),
        }
    }

    fn verify(&self, receipt: &Receipt) -> AnchorState {
        // Untrusted bytes reach the library parser only within bounds (AT-NET-07).
        if !within_bounds(&receipt.proof) {
            return AnchorState::Invalid;
        }
        let file = match DetachedTimestampFile::from_reader(receipt.proof.as_slice()) {
            Ok(file) => file,
            Err(_) => return AnchorState::Invalid,
        };
        // The proof must commit to exactly the claimed root.
        if file.timestamp.start_digest != receipt.root {
            return AnchorState::Invalid;
        }
        self.walk(&file.timestamp.first_step)
    }
}

/// Strongest-wins ordering: a confirmed anchor beats a pending one beats invalid.
fn stronger(a: AnchorState, b: AnchorState) -> AnchorState {
    fn rank(s: &AnchorState) -> u8 {
        match s {
            AnchorState::Confirmed { .. } => 2,
            AnchorState::Pending => 1,
            AnchorState::Invalid => 0,
        }
    }
    if rank(&b) > rank(&a) {
        b
    } else {
        a
    }
}

fn serialize(timestamp: &Timestamp) -> Vec<u8> {
    let file = DetachedTimestampFile {
        digest_type: DigestType::Sha256,
        timestamp: timestamp.clone(),
    };
    let mut buf = Vec::new();
    file.to_writer(&mut buf)
        .expect("serializing an in-memory timestamp cannot fail");
    buf
}

// Bounds on an untrusted `.ots` proof, checked before the library parses it
// (`docs/12-panic-audit.md` §3, F1–F4).

const MAX_PROOF_LEN: usize = 64 * 1024;
/// Longest operation message: python-opentimestamps' limit (the Rust crate has none).
const MAX_MSG_LEN: usize = 4096;
/// Bytes of messages the library may build while parsing: every fork clone and every
/// operation's result, stored and cloned.
const MAX_WORK: usize = 1 << 20;
/// The library's own recursion limit: scan and parser must agree on depth.
const MAX_DEPTH: usize = 256;
/// The library's limits on an operation argument and a pending-attestation URI.
const MAX_OP_ARG: usize = 4096;
const MAX_URI_LEN: usize = 1000;
/// Varint bytes accepted: the value fits in `usize` and no shift reaches the width.
const MAX_VARINT_BYTES: usize = (usize::BITS as usize - 1) / 7;
/// The file header and attestation tags, as the library defines them (private there).
const OTS_MAGIC: &[u8] = b"\x00OpenTimestamps\x00\x00Proof\x00\xbf\x89\xe2\xe8\x84\xe8\x92\x94";
const BITCOIN_TAG: &[u8] = b"\x05\x88\x96\x0d\x73\xd7\x19\x01";
const PENDING_TAG: &[u8] = b"\x83\xdf\xe3\x0d\x2e\xf9\x0c\x8e";

/// Whether `proof` is safe to hand to the library parser: it follows the `.ots` grammar
/// within every bound above. Rejecting is always safe (the proof is `Invalid`); accepting
/// guarantees the library reads the same bytes with bounded depth, length and memory.
fn within_bounds(proof: &[u8]) -> bool {
    proof.len() <= MAX_PROOF_LEN
        && Scan {
            bytes: proof,
            pos: 0,
            work: 0,
        }
        .file()
        .is_some()
}

/// A cursor over the proof; every method returns `None` to reject.
struct Scan<'a> {
    bytes: &'a [u8],
    pos: usize,
    work: usize,
}

impl<'a> Scan<'a> {
    fn file(&mut self) -> Option<()> {
        if self.take(OTS_MAGIC.len())? != OTS_MAGIC || self.varint()? != 1 {
            return None;
        }
        let digest_len = match self.byte()? {
            0x02 | 0x03 => 20, // SHA-1, RIPEMD-160
            0x08 => 32,        // SHA-256
            _ => return None,
        };
        self.take(digest_len)?;
        self.step(digest_len, None, 0)?;
        (self.pos == self.bytes.len()).then_some(())
    }

    /// One step of the tree on a message of `msg_len` bytes, as the library's
    /// `deserialize_step_recurse` reads it (`tag` is set when the caller already read it).
    fn step(&mut self, msg_len: usize, tag: Option<u8>, depth: usize) -> Option<()> {
        if depth >= MAX_DEPTH {
            return None;
        }
        let tag = match tag {
            Some(tag) => tag,
            None => self.byte()?,
        };
        match tag {
            0x00 => self.attestation(),
            0xff => loop {
                // Each branch gets its own clone of the message.
                self.charge(msg_len)?;
                self.step(msg_len, None, depth + 1)?;
                let next = self.byte()?;
                if next != 0xff {
                    self.charge(msg_len)?;
                    return self.step(msg_len, Some(next), depth + 1);
                }
            },
            op => {
                let out = match op {
                    0x02 | 0x03 => 20,               // SHA-1, RIPEMD-160
                    0x08 => 32,                      // SHA-256
                    0xf3 => msg_len.checked_mul(2)?, // Hexlify
                    0xf2 => msg_len,                 // Reverse
                    0xf0 | 0xf1 => {
                        // Append / Prepend carry 1..=4096 argument bytes.
                        let n = self.varint()?;
                        if !(1..=MAX_OP_ARG).contains(&n) {
                            return None;
                        }
                        self.take(n)?;
                        msg_len.checked_add(n)?
                    }
                    _ => return None,
                };
                if out > MAX_MSG_LEN {
                    return None;
                }
                // The result is stored in the step and cloned for the next one.
                self.charge(out.checked_mul(2)?)?;
                self.step(out, None, depth + 1)
            }
        }
    }

    /// An attestation as read: 8-byte tag, declared length, then a height, a URI or `len` bytes.
    fn attestation(&mut self) -> Option<()> {
        let tag = self.take(8)?;
        let len = self.varint()?;
        if tag == BITCOIN_TAG {
            self.varint()?;
        } else if tag == PENDING_TAG {
            let n = self.varint()?;
            if n > MAX_URI_LEN {
                return None;
            }
            self.take(n)?;
        } else {
            // Must fit in the proof: the library allocates `len` before reading.
            self.take(len)?;
        }
        Some(())
    }

    fn byte(&mut self) -> Option<u8> {
        let b = *self.bytes.get(self.pos)?;
        self.pos += 1;
        Some(b)
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        let slice = self.bytes.get(self.pos..end)?;
        self.pos = end;
        Some(slice)
    }

    /// An unsigned LEB128 varint, as the library's `read_uint`, but refusing an encoding
    /// long enough to overflow its shift.
    fn varint(&mut self) -> Option<usize> {
        let mut value = 0usize;
        for i in 0..MAX_VARINT_BYTES {
            let b = self.byte()?;
            value |= usize::from(b & 0x7f) << (7 * i);
            if b & 0x80 == 0 {
                return Some(value);
            }
        }
        None
    }

    fn charge(&mut self, bytes: usize) -> Option<()> {
        self.work = self.work.checked_add(bytes)?;
        (self.work <= MAX_WORK).then_some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_proof_is_a_parseable_ots_committing_to_the_root() {
        let mut anchor = OtsAnchor::new("https://calendar.example");
        let root = [3u8; 32];
        let receipt = anchor.submit(root);

        let file = DetachedTimestampFile::from_reader(receipt.proof.as_slice()).unwrap();
        assert_eq!(file.timestamp.start_digest, root.to_vec());
        assert_eq!(anchor.verify(&receipt), AnchorState::Pending);
    }

    #[test]
    fn a_bitcoin_attestation_against_the_wrong_block_root_is_rejected() {
        let mut a = OtsAnchor::new("https://a.example");
        let mut b = OtsAnchor::new("https://b.example");
        let pending_a = a.submit([1u8; 32]);
        let confirmed_a = a.upgrade(&pending_a);
        let pending_b = b.submit([2u8; 32]);
        let _confirmed_b = b.upgrade(&pending_b);

        assert!(matches!(
            a.verify(&confirmed_a),
            AnchorState::Confirmed { height: 0 }
        ));
        assert_eq!(b.verify(&confirmed_a), AnchorState::Invalid);
    }

    // --- Hostile proofs (AT-NET-07) ---------------------------------------------

    const SMALL: &[u8] = include_bytes!("../tests/fixtures/ots/pending-two-calendars.ots");
    const LARGE: &[u8] = include_bytes!("../tests/fixtures/ots/bitcoin-attested.ots");

    /// A proof header for a SHA-256 digest of `root`, ready for a step tree.
    fn header(root: [u8; 32]) -> Vec<u8> {
        let mut v = OTS_MAGIC.to_vec();
        v.push(0x01); // version
        v.push(0x08); // SHA-256
        v.extend_from_slice(&root);
        v
    }

    fn varint(mut n: u64, out: &mut Vec<u8>) {
        loop {
            let b = (n & 0x7f) as u8;
            n >>= 7;
            if n == 0 {
                out.push(b);
                return;
            }
            out.push(b | 0x80);
        }
    }

    /// A pending attestation to a one-character calendar URI.
    fn pending(out: &mut Vec<u8>) {
        out.push(0x00);
        out.extend_from_slice(PENDING_TAG);
        out.extend_from_slice(&[2, 1, b'a']);
    }

    /// Hostile bytes are refused before parsing, and `verify` answers `Invalid`.
    fn refused(proof: Vec<u8>) {
        assert!(!within_bounds(&proof));
        let receipt = Receipt {
            root: [0u8; 32],
            proof,
        };
        assert_eq!(OtsAnchor::new("x").verify(&receipt), AnchorState::Invalid);
    }

    #[test]
    fn genuine_proofs_are_within_bounds() {
        assert!(within_bounds(SMALL));
        assert!(within_bounds(LARGE));
        let root: [u8; 32] = SMALL[OTS_MAGIC.len() + 2..][..32].try_into().unwrap();
        let receipt = Receipt {
            root,
            proof: SMALL.to_vec(),
        };
        assert_eq!(OtsAnchor::new("x").verify(&receipt), AnchorState::Pending);
        let mut anchor = OtsAnchor::new("https://calendar.example");
        let pending = anchor.submit([5u8; 32]);
        assert!(within_bounds(&pending.proof));
        assert!(within_bounds(&anchor.upgrade(&pending).proof));
    }

    #[test]
    fn an_overlong_varint_is_refused() {
        // Twelve continuation bytes as the version overflow the library's shift.
        let mut proof = OTS_MAGIC.to_vec();
        proof.extend_from_slice(&[0x80; 12]);
        proof.push(0x00);
        refused(proof);
    }

    #[test]
    fn an_attestation_longer_than_the_proof_is_refused() {
        // Declares 2^62 bytes: the library allocates that much before reading.
        let mut proof = header([0u8; 32]);
        proof.push(0x00);
        proof.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        varint(1 << 62, &mut proof);
        refused(proof);
    }

    #[test]
    fn a_message_longer_than_4096_bytes_is_refused() {
        // Seven Hexlify reach exactly 4096 bytes; unchecked, `k` of them build 32·2^k bytes.
        for (ops, ok) in [(7, true), (8, false), (40, false)] {
            let mut proof = header([0u8; 32]);
            proof.extend(std::iter::repeat_n(0xf3, ops));
            pending(&mut proof);
            if ok {
                assert!(within_bounds(&proof), "{ops} Hexlify");
            } else {
                refused(proof);
            }
        }
    }

    #[test]
    fn a_tree_deeper_than_the_library_limit_is_refused() {
        let mut proof = header([0u8; 32]);
        proof.extend(std::iter::repeat_n(0xf2, MAX_DEPTH)); // Reverse
        pending(&mut proof);
        refused(proof);
    }

    #[test]
    fn fork_amplification_is_bounded() {
        // Growing to 4096 bytes then forking clones the message per branch.
        for (branches, ok) in [(4, true), (300, false)] {
            let mut proof = header([0u8; 32]);
            proof.extend(std::iter::repeat_n(0xf3, 7));
            proof.push(0xff);
            for i in 0..branches {
                if i > 0 {
                    proof.push(0xff);
                }
                pending(&mut proof);
            }
            // The last branch is read without a fork marker.
            pending(&mut proof);
            if ok {
                assert!(within_bounds(&proof), "{branches} branches");
            } else {
                refused(proof);
            }
        }
    }

    #[test]
    fn trailing_bytes_and_oversized_proofs_are_refused() {
        let mut proof = SMALL.to_vec();
        proof.push(0x00);
        refused(proof);
        let mut big = header([0u8; 32]);
        big.push(0x00);
        big.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        varint(MAX_PROOF_LEN as u64, &mut big);
        big.resize(big.len() + MAX_PROOF_LEN, 0);
        refused(big);
    }
}
