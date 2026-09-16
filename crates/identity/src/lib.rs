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
//!   as real BBS+ both single-issuer ([`credential::Issuer`]) and **threshold** t-of-n
//!   ([`credential::ThresholdIssuer`], DKG + base-OT + MPC via `bbs_plus::threshold`),
//!   the holder's secret hidden under a commitment + proof of knowledge either way.
//! - Still modeled (future work): a real distributed key-generation ceremony and
//!   network transport for both threshold committees (here trusted-dealer keygen +
//!   in-process signing); selective-disclosure *presentation* of the credential
//!   (`PoKOfSignature`); and the Semaphore ZK nullifier. See the module docs for
//!   exactly what each leaves open.

pub mod credential;
pub mod enrollment;
pub mod nym;
pub mod oprf;
pub mod ratelimit;

mod hash;
