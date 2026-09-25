//! Deterministic scoring engine: offline, no identity or network dependency, reproducible
//! for identical input (`docs/02-scoring-engine.md`, invariant #7).

pub mod bridging;
pub mod collusion;
pub mod dif;
pub mod dtf;
mod fmath;
mod glm;
pub mod irt;
pub mod latent;
mod optim;
pub mod reputation;

pub use glm::{LogisticFit, LogisticResult};
pub use optim::Convergence;
#[cfg(feature = "calibration")]
pub mod validation;
