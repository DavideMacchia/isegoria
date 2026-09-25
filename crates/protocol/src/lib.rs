//! Question lifecycle orchestration (`docs/05-question-lifecycle.md`): ties together
//! `scoring` (the two filters), `identity` (role pseudonyms, rate limits) and `network`
//! (the tamper-evident log); the deterministic pieces are seeded for reproducibility.

pub mod admission;
pub mod appeal;
pub mod blueprint;
pub mod deposit;
pub mod exploration;
pub mod exposure;
pub mod gate;
pub mod governance;
pub mod honeypot;
pub mod lifecycle;
pub mod lottery;
pub mod orchestrator;
pub mod pilot;
pub mod probation;
pub mod randomness;
pub mod revalidation;
pub mod review;
