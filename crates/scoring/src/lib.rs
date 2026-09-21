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

// Convergence / separation status types surface on public results (`bridging::Fit`,
// `dif::DifCoefs`, `dif::MixtureDif`) and are consumed by callers, so re-export them
// from the private engine modules (docs/08 OPT-001). `LogisticResult` is the fit's
// result vocabulary; only the calibration DIF path reads its `status`, so it is
// exported to keep it part of the public API in every build.
pub use glm::{LogisticFit, LogisticResult};
pub use optim::Convergence;
// Iterative θ purification is attribute-based DIF (Variant 1): calibration-only (D20).
#[cfg(feature = "calibration")]
pub mod validation;
