//! Identity and enrollment. See `docs/03-identity-enrollment.md`.
//!
//! The state authenticates but does not issue: the enrollment source (CIE/SPID/…)
//! only proves a real, unique person; a separate threshold committee issues the
//! anonymous credential, and the two never communicate.
//!
//! What is real here vs. still modeled:
//! - Real: deterministic hash role pseudonyms and rate-limiting tokens (M3, P2); the
//!   uniqueness label both as a single-server VOPRF ([`enrollment::VoprfOracle`], RFC
//!   9497) and as a real **threshold** t-of-n OPRF ([`oprf::ThresholdOprfOracle`],
//!   Shamir shares + per-share DLEQ over Ristretto255); blind credential issuance as
//!   real BBS+ both single-issuer ([`credential::Issuer`]) and **threshold** t-of-n
//!   ([`credential::ThresholdIssuer`], DKG + base-OT + MPC via `bbs_plus::threshold`),
//!   the holder's secret hidden under a commitment + proof of knowledge; and a
//!   Semaphore-style **ZK nullifier** ([`nullifier`]) proving a per-role pseudonym
//!   derives from a valid credential, without a circom/Groth16 stack.
//! - Still modeled (future work): a real distributed key-generation ceremony and
//!   network transport for both threshold committees (here trusted-dealer keygen +
//!   in-process signing); unifying the protocol pseudonym with the ZK nullifier; and
//!   selective-disclosure *presentation* of the credential. See the module docs for
//!   exactly what each leaves open.

pub mod credential;
pub mod enrollment;
pub mod nullifier;
pub mod nym;
pub mod oprf;
pub mod ratelimit;

mod hash;
