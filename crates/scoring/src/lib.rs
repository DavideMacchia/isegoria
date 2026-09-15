//! Deterministic scoring engine. See `docs/02-scoring-engine.md`.
//!
//! Runs offline, with no dependency on identity or network, and is reproducible
//! given identical input (CLAUDE.md, invariant #7).

pub mod bridging;
pub mod collusion;
pub mod dif;
mod glm;
pub mod irt;
mod optim;
pub mod reputation;
pub mod validation;
