//! Deterministic scoring engine. See `docs/02-scoring-engine.md`.
//!
//! Runs offline, with no dependency on identity or network, and is reproducible
//! given identical input (CLAUDE.md, invariant #7).

pub mod bridging;
pub mod collusion;
pub mod dif;
mod fmath;
mod glm;
pub mod irt;
mod optim;
pub mod reputation;

// Status types on public results (`bridging::Fit`, `dif::*`), re-exported from the
// private engine modules (docs/08 OPT-001).
pub use glm::{LogisticFit, LogisticResult};
pub use optim::Convergence;
// Iterative θ purification is attribute-based DIF (Variant 1): calibration-only (D20).
#[cfg(feature = "calibration")]
pub mod validation;
