//! Probation and cold start (`docs/03` P2, `docs/05` §Cold start).
//!
//! Non-rotatable IDs are worthless if a fresh node carries weight before it has a
//! track record, so a new node is on **probation**: its reviews measure its evaluator
//! score but do not determine outcomes (weight 0) until it has `N_PROBATION` scored
//! outcomes. At bootstrap a publicly declared, heterogeneous **founder set** carries
//! uniform weight 1 to seed the first outcomes; once any node crosses the probation
//! threshold it switches to its score-based odds weight (D33), capped across the epoch.

use identity::nym::Nym;
use scoring::reputation::{odds_weight, GAMMA, K_SHRINK};
use std::collections::HashSet;

/// Scored outcomes before a new pseudonym carries weight (`docs/01` D36, T50): 30, was
/// 200 — probation only denies influence to a pseudonym with no track record; the
/// shrinkage of D33 handles the rest, and whitewashing is prevented by non-rotatable
/// pseudonyms (invariant #5), not by the length of probation.
pub const N_PROBATION: usize = 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// bootstrap seed: uniform weight 1 until it has a track record
    Founder,
    /// new node: weight 0 (measured, not counted) until N_PROBATION scored outcomes
    Probation,
    /// has a track record: score-based odds weight, capped across the epoch
    Established,
}

/// Classifies a node. A node with `N_PROBATION`+ scored outcomes is established
/// regardless of founder status; before that, founders seed at weight 1 and everyone
/// else is on probation.
pub fn status(is_founder: bool, scored: usize) -> Status {
    if scored >= N_PROBATION {
        Status::Established
    } else if is_founder {
        Status::Founder
    } else {
        Status::Probation
    }
}

/// Review vote weight `w_u` before the epoch's cap: 0 on probation, 1 for a bootstrap
/// founder, and the odds weight `exp(γ · S_u · k_u / (k_u + k_0))` of the evaluator
/// score `score` over `scored` outcomes once established (`docs/02` C.2/C.4, D33). The
/// `3 × median` cap is applied across the epoch's reviewers by
/// `reputation::cap_weights` (`orchestrator::bridging_weights`).
pub fn review_weight(status: Status, score: f64, scored: usize) -> f64 {
    match status {
        Status::Probation => 0.0,
        Status::Founder => 1.0,
        Status::Established => odds_weight(score, scored, GAMMA, K_SHRINK),
    }
}

/// Convenience: classify and weight in one step (uncapped, see [`review_weight`]).
pub fn effective_review_weight(is_founder: bool, scored: usize, score: f64) -> f64 {
    review_weight(status(is_founder, scored), score, scored)
}

/// The publicly declared bootstrap founders (`docs/05` §Cold start). Heterogeneity is
/// a governance decision made at declaration time; here we only track membership.
#[derive(Default)]
pub struct FounderSet {
    members: HashSet<Nym>,
}

impl FounderSet {
    pub fn new(members: impl IntoIterator<Item = Nym>) -> Self {
        FounderSet {
            members: members.into_iter().collect(),
        }
    }

    pub fn contains(&self, nym: &Nym) -> bool {
        self.members.contains(nym)
    }

    pub fn len(&self) -> usize {
        self.members.len()
    }

    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }
}
