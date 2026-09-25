//! Randomized exploration of gate rejections (`docs/01` D35, T52): a random `ε` fraction
//! is drawn from the beacon and piloted for measurement only, so evaluator scores stay
//! proper and the gate's false-negative rate is measured (AT-REP-06).

use crate::lifecycle::{RejectReason, State};
use crate::probation::{Alarm, SkillTrack};
use crate::randomness::{Beacon, EXPLORATION};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use scoring::reputation::CusumParams;

pub const EXPLORATION_RATE: f64 = 0.05;

pub fn explore(seed: u64, rate: f64) -> bool {
    ChaCha8Rng::seed_from_u64(seed).gen::<f64>() < rate
}

/// The exploration draw for the item at `slot` (INV-10); never the draft bytes (AT-PRO-07).
pub fn explore_from_beacon(beacon: &Beacon, slot: u64, rate: f64) -> bool {
    explore(beacon.seed(EXPLORATION, slot), rate)
}

/// A scored outcome (D35): `outcome` is Level B's result (1/0); `inclusion` is `π_j`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Observation {
    pub outcome: f64,
    pub inclusion: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Scored {
    Observed(Observation),
    Unobserved,
    Pending,
}

/// Reads `state` as a scored outcome at exploration `rate`: pool (contested too) and pilot
/// rejection certain, `Measured` observed at `rate`, an unpicked gate rejection unobserved.
pub fn outcome_of(state: &State, rate: f64) -> Scored {
    match state {
        State::ActivePool | State::Contested | State::Retired(_) => Scored::Observed(Observation {
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

/// Feeds one item's state into a reviewer's track via `score`; returns an alarm if raised.
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

/// The gate's false-negative rate (D35): explored rejections that Level B would pass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FalseNegatives {
    pub explored: usize,
    pub passed: usize,
}

impl FalseNegatives {
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
