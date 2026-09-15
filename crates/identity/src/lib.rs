//! Identity and enrollment. See `docs/03-identity-enrollment.md`.
//!
//! The state authenticates but does not issue: the enrollment source (CIE/SPID/…)
//! only proves a real, unique person; a separate threshold committee issues the
//! anonymous credential, and the two never communicate.
//!
//! What is real here vs. a plug point:
//! - Real: deterministic role nullifiers and rate-limiting tokens (hash-based) —
//!   the mechanism that makes negative reputation inescapable (M3, P2).
//! - Plug points (traits + non-production reference impls): the threshold OPRF that
//!   computes the uniqueness label ([`enrollment::UniquenessOracle`]) and the blind
//!   threshold issuance ([`credential::BlindIssuer`]). Production wires these to
//!   mature libraries (a threshold OPRF, BBS+, Semaphore); the placeholders exist
//!   only to exercise the pipeline in tests.

pub mod credential;
pub mod enrollment;
pub mod nym;
pub mod ratelimit;

mod hash;
