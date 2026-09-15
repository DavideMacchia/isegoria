//! Question lifecycle orchestration. See `docs/05-question-lifecycle.md`.
//!
//! Ties together the three lower layers: `scoring` (the two filters), `identity`
//! (role pseudonyms and rate limits), `network` (the tamper-evident log). Each
//! stage names the attack it neutralizes; the deterministic pieces (lottery,
//! reviewer assignment) are seeded for reproducibility.

pub mod deposit;
pub mod gate;
pub mod governance;
pub mod honeypot;
pub mod lottery;
pub mod pilot;
pub mod probation;
pub mod review;

/// Lifecycle stages (`docs/05`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Draft,
    Deposited,
    Admitted,
    InReview,
    Pilot1,
    Pilot2,
    ActivePool,
    Retired,
}
