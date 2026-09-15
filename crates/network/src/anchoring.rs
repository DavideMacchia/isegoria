//! Anchoring to a public chain (`docs/04`, §Anchoring). Periodically one root
//! summarizing the whole state is published to Bitcoin/Ethereum, so rewriting the
//! past would require rewriting the public chain too.

/// A proof that a root was anchored. Production wires this to OpenTimestamps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    pub root: [u8; 32],
    pub proof: Vec<u8>,
}

/// External anchoring service. The real backend is OpenTimestamps; implementors
/// provide it.
pub trait Anchor {
    fn submit(&mut self, root: [u8; 32]) -> Receipt;
    fn verify(&self, receipt: &Receipt) -> bool;
}

/// Non-production reference anchor: records published roots in memory instead of a
/// real public chain. Exercises the flow; provides no external guarantee.
#[derive(Default)]
pub struct ReferenceAnchor {
    published: std::collections::HashSet<[u8; 32]>,
}

impl ReferenceAnchor {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Anchor for ReferenceAnchor {
    fn submit(&mut self, root: [u8; 32]) -> Receipt {
        self.published.insert(root);
        Receipt {
            root,
            proof: b"reference-anchor".to_vec(),
        }
    }

    fn verify(&self, receipt: &Receipt) -> bool {
        self.published.contains(&receipt.root)
    }
}
