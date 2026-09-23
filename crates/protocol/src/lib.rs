//! Question lifecycle orchestration. See `docs/05-question-lifecycle.md`.
//!
//! Ties together the three lower layers: `scoring` (the two filters), `identity`
//! (role pseudonyms and rate limits), `network` (the tamper-evident log). Each
//! stage names the attack it neutralizes; the deterministic pieces (lottery,
//! reviewer assignment) are seeded for reproducibility.

pub mod aggregate;
pub mod blueprint;
pub mod deposit;
pub mod exposure;
pub mod gate;
pub mod governance;
pub mod honeypot;
pub mod lifecycle;
pub mod lottery;
pub mod orchestrator;
pub mod pilot;
pub mod probation;
pub mod revalidation;
pub mod review;
