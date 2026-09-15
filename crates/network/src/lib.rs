//! Storage and network layer. See `docs/04-storage-network.md`.
//!
//! The property that matters: no one can delete or rewrite questions and votes, and
//! no one can falsify the scores without it being visible. That comes from operator
//! diversity plus reproducible computation, not from permissionless consensus.
//!
//! Real here: content addressing, Merkle trees, the append-only transparency log,
//! consortium checkpoints (ed25519), and erasure coding. Plug points: gossip/DHT
//! transport (libp2p), convergent state (CRDT), and public-chain anchoring
//! (OpenTimestamps) — behind traits, wired to mature libraries in production.

pub mod anchoring;
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
