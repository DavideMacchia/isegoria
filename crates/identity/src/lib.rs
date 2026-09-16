//! Identity and enrollment. See `docs/03-identity-enrollment.md`.
//!
//! The state authenticates but does not issue: the enrollment source (CIE/SPID/…)
//! only proves a real, unique person; a separate threshold committee issues the
//! anonymous credential, and the two never communicate.
//!
//! What is real here vs. still modeled:
//! - Real: deterministic role nullifiers and rate-limiting tokens (hash-based) —
//!   the mechanism that makes negative reputation inescapable (M3, P2); the
//!   uniqueness label both as a single-server VOPRF ([`enrollment::VoprfOracle`], RFC
//!   9497) and as a real **threshold** t-of-n OPRF ([`oprf::ThresholdOprfOracle`],
//!   Shamir shares + per-share DLEQ over Ristretto255); and blind credential issuance
//!   as real BBS+ ([`credential::Issuer`], the holder's secret hidden under a
//!   commitment + proof of knowledge).
//! - Still modeled (future work): the *threshold* t-of-n split of the BBS+ issuing
//!   key; a real distributed key-generation ceremony and network transport for the
//!   threshold OPRF (here a trusted dealer + in-process committee); and the Semaphore
//!   ZK nullifier. See the module docs for exactly what each leaves open.

pub mod credential;
pub mod enrollment;
pub mod nym;
pub mod oprf;
pub mod ratelimit;

mod hash;
