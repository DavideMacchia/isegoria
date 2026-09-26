//! Storage and network layer (`docs/04-storage-network.md`): content addressing, Merkle
//! trees, the transparency log, consortium checkpoints, and erasure coding are real; the
//! gossip/DHT transport, CRDT state, and OpenTimestamps anchoring are plug points.

pub mod anchoring;
pub mod beacon;
pub mod cid;
pub mod consortium;
pub mod erasure;
pub mod log;
pub mod merkle;

mod hash;

/// Infrastructural node roles (`docs/04`), distinct from the three person actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    /// pseudonymous participant
    Person,
    /// always-on consortium signer holding a full copy
    Signer,
    /// light client: a slice of data + signature verification
    Light,
}
