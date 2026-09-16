//! Anchoring to a public chain (`docs/04`, §Anchoring). Periodically one root
//! summarizing the whole state is published to Bitcoin, so rewriting the past would
//! require rewriting the public chain too.
//!
//! This uses the real **OpenTimestamps** proof format (crate `opentimestamps`): a
//! receipt carries a serialized `.ots` timestamp, and verification runs the actual
//! OTS algorithm — parse the proof, execute its operation tree from the root, and
//! check the result against a Bitcoin block's Merkle root.
//!
//! **What is real:** the OTS proof format (built, serialized, and parsed by the
//! library) and the verification walk (`Op::execute` over the step tree, compared to
//! the block Merkle root).
//!
//! **What is modeled (`docs/04`, future work):** the live network parts. A real
//! submission POSTs the digest to a calendar server and, once the aggregated commit
//! confirms, upgrades the proof with a Bitcoin attestation; a real verifier reads the
//! block's Merkle root from a Bitcoin node or SPV client. Here [`OtsAnchor`] holds an
//! injected block source (`height -> Merkle root`) and [`OtsAnchor::upgrade`] stands in
//! for the calendar's aggregation with a single hashing step. No network is contacted.

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
    /// Timestamp `root`, returning a (still pending) OTS receipt.
    fn submit(&mut self, root: [u8; 32]) -> Receipt;
    /// Verify a receipt against the current known chain state.
    fn verify(&self, receipt: &Receipt) -> AnchorState;
}

/// An OpenTimestamps-backed anchor. Real proof format and verification; the calendar
/// and Bitcoin network are represented by an injected block source so the flow runs
/// offline and deterministically in tests.
pub struct OtsAnchor {
    calendar_uri: String,
    /// Stand-in for a Bitcoin node/SPV client: block height -> block Merkle root.
    blocks: HashMap<usize, [u8; 32]>,
    next_height: usize,
}

impl OtsAnchor {
    /// A fresh anchor targeting `calendar_uri`, with no confirmed blocks yet.
    pub fn new(calendar_uri: impl Into<String>) -> Self {
        OtsAnchor {
            calendar_uri: calendar_uri.into(),
            blocks: HashMap::new(),
            next_height: 0,
        }
    }

    /// Simulate the calendar upgrading a pending receipt to a Bitcoin attestation
    /// once its aggregated commitment has confirmed. Production fetches the real
    /// upgraded `.ots` from the calendar; here we apply one hashing step (standing in
    /// for the aggregation Merkle path) and record the resulting digest as the Merkle
    /// root of a new block, exactly what a verifier would later read from the chain.
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
        // A freshly submitted timestamp: committed to a calendar, not yet on-chain.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_proof_is_a_parseable_ots_committing_to_the_root() {
        let mut anchor = OtsAnchor::new("https://calendar.example");
        let root = [3u8; 32];
        let receipt = anchor.submit(root);

        // The bytes are a real `.ots` file that parses and commits to our root.
        let file = DetachedTimestampFile::from_reader(receipt.proof.as_slice()).unwrap();
        assert_eq!(file.timestamp.start_digest, root.to_vec());
        assert_eq!(anchor.verify(&receipt), AnchorState::Pending);
    }

    #[test]
    fn a_bitcoin_attestation_against_the_wrong_block_root_is_rejected() {
        // Two anchors confirm *different* roots, both landing at height 0. Verifying
        // one's proof against the other hits a Bitcoin attestation whose height is
        // known but whose Merkle root does not match — the mismatch branch.
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
}
