//! Identity and enrollment: the state authenticates, a separate threshold committee
//! issues the anonymous credential (role pseudonyms, rate limiting, VOPRF/BBS+ single-
//! and threshold, ZK nullifier). See `docs/03-identity-enrollment.md`.

pub mod credential;
pub mod enrollment;
pub mod nullifier;
pub mod nym;
pub mod oprf;
pub mod ratelimit;

mod hash;
