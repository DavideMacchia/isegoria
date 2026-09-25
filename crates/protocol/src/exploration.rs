//! Randomized exploration of gate rejections (`docs/01` D35, T52). Evaluators are scored
//! on every reviewed item whose Level B outcome is known: the golden items (`honeypot`),
//! every live item that reaches a pilot, and — so that the score stays proper when the
//! gate decides which outcomes are observed — a random fraction `ε` of the items the gate
//! rejects, drawn from the public beacon and piloted for measurement only. An explored
//! item never enters the pool; its outcome enters the score at weight `1/ε` (inverse
//! probability), so the expected score is the score with every outcome observed and
//! truthful reporting stays optimal (paper, "Exploration restores properness";
//! AT-REP-06, `scoring/tests/exploration_weights.rs`). Exploration also measures the
//! gate's false-negative rate — how many rejected items would have passed Level B — and
//! costs about `ε` of pilot capacity.
//!
//! The draw is keyed on the beacon and the admitted slot (INV-10): reproducible by anyone,
//! chosen by no one (AT-PRO-07), and — until T37 — as grind-free as the beacon itself.

use crate::lifecycle::{RejectReason, State};
use crate::probation::{Alarm, SkillTrack};
use crate::randomness::{Beacon, EXPLORATION};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use scoring::reputation::CusumParams;

/// The share of gate rejections piloted for measurement only (D35): the inclusion
/// probability of a rejected item's outcome, and the inverse of its score weight.
/// Provisional (T25).
pub const EXPLORATION_RATE: f64 = 0.05;

/// Whether a rejection is explored: a Bernoulli(`rate`) draw from `seed`.
/// [`explore_from_beacon`] is the sanctioned entry point; this takes a raw seed for tests.
pub fn explore(seed: u64, rate: f64) -> bool {
    ChaCha8Rng::seed_from_u64(seed).gen::<f64>() < rate
}

/// The exploration draw for the rejected item admitted at `slot`, seeded from the signed
/// checkpoint (INV-10) in a stream of its own (`randomness::EXPLORATION`) and keyed on
/// the slot — never the draft bytes — so an author can neither regenerate a draft to
/// have it explored nor predict the beacon to avoid it.
pub fn explore_from_beacon(beacon: &Beacon, slot: u64, rate: f64) -> bool {
    explore(beacon.seed(EXPLORATION, slot), rate)
}

/// A scored outcome as the evaluator score reads it (D35): the Level B result `o_j`
/// (1 passed, 0 failed) and the probability `π_j` with which it was observed — 1 for an
/// item that entered the pilot on its own account, `ε` for an explored rejection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Observation {
    pub outcome: f64,
    pub inclusion: f64,
}

/// What a reviewed item's state contributes to its reviewers' scores.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Scored {
    /// The outcome is known, at its inclusion probability.
    Observed(Observation),
    /// A gate rejection the draw did not explore: it counts among the reviewed items —
    /// the mean's denominator — but has no outcome to score.
    Unobserved,
    /// Not a terminal state: nothing to score yet.
    Pending,
}

/// Reads `state` as a scored outcome at exploration `rate`: the pool (and a retirement
/// from it) is a pass and a pilot rejection a fail, both observed with certainty; a
/// `Measured` item is its pilot result, observed with probability `rate`; a gate
/// rejection the draw did not pick is unobserved.
pub fn outcome_of(state: &State, rate: f64) -> Scored {
    match state {
        State::ActivePool | State::Retired(_) => Scored::Observed(Observation {
            outcome: 1.0,
            inclusion: 1.0,
        }),
        State::Rejected(RejectReason::Screen | RejectReason::Dif) => {
            Scored::Observed(Observation {
                outcome: 0.0,
                inclusion: 1.0,
            })
        }
        State::Measured { passed, .. } => Scored::Observed(Observation {
            outcome: f64::from(*passed),
            inclusion: rate,
        }),
        State::Rejected(_) => Scored::Unobserved,
        _ => Scored::Pending,
    }
}

/// Feeds one reviewed item's state into a reviewer's track: the reviewer's per-item score
/// of the observed outcome — `score(o_j)`, their leave-one-out difference score — at its
/// inclusion weight; an unobserved item into the denominator; a pending item nothing.
/// Returns the change detector's alarm if the item raised one.
pub fn record_outcome(
    track: &mut SkillTrack,
    state: &State,
    rate: f64,
    score: impl FnOnce(f64) -> f64,
    params: &CusumParams,
) -> Option<Alarm> {
    match outcome_of(state, rate) {
        Scored::Observed(obs) => track.record_observed(score(obs.outcome), obs.inclusion, params),
        Scored::Unobserved => {
            track.record_unobserved();
            None
        }
        Scored::Pending => None,
    }
}

/// The gate's false-negative rate, measured on the explored rejections (D35): how many of
/// them Level B passes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FalseNegatives {
    /// Explored rejections measured so far.
    pub explored: usize,
    /// Those that passed Level B: items the gate should have let through.
    pub passed: usize,
}

impl FalseNegatives {
    /// Records a terminal state; only a `Measured` item counts.
    pub fn record(&mut self, terminal: &State) {
        if let State::Measured { passed, .. } = terminal {
            self.explored += 1;
            self.passed += usize::from(*passed);
        }
    }

    /// `passed / explored`, or nothing before the first measurement.
    pub fn rate(&self) -> Option<f64> {
        (self.explored > 0).then(|| self.passed as f64 / self.explored as f64)
    }
}
