//! Identity and enrollment. See `docs/03-identity-enrollment.md`.
//!
//! The state authenticates but does not issue: the enrollment source (CIE/SPID/…)
//! only proves a real, unique person; a separate threshold committee issues the
//! anonymous credential, and the two never communicate.
//!
//! What is real here vs. still modeled:
//! - Real: deterministic role nullifiers and rate-limiting tokens (hash-based) —
//!   the mechanism that makes negative reputation inescapable (M3, P2); the
//!   uniqueness label as a single-server VOPRF ([`enrollment::VoprfOracle`], RFC
//!   9497); and blind credential issuance as real BBS+ ([`credential::Issuer`], with
//!   the holder's secret hidden under a commitment + proof of knowledge).
//! - Still modeled (future work): the *threshold* t-of-n split of both the OPRF key
//!   and the BBS+ issuing key across the committee, and the Semaphore ZK nullifier.
//!   Each real backend here is single-party; the threshold distribution is what the
//!   spec ultimately requires. See the module docs for exactly what that leaves open.

pub mod credential;
pub mod enrollment;
pub mod nym;
pub mod ratelimit;

mod hash;
