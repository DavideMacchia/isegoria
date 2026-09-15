//! Probation and cold start (`docs/03` P2, `docs/05` §Cold start).
//!
//! Non-rotatable IDs are worthless if a fresh node carries weight before it has a
//! track record, so a new node is on **probation**: its reviews measure its `E_u`
//! but do not determine outcomes (weight 0) until it has `N_PROBATION` judgments with
//! a known outcome. At bootstrap a publicly declared, heterogeneous **founder set**
//! carries uniform weight 1 to seed the first outcomes; once any node crosses the
//! probation threshold it switches to its `E_u`-based, capped weight.

use identity::nym::Nym;
use scoring::reputation::capped_weight;
use std::collections::HashSet;

pub const N_PROBATION: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// bootstrap seed: uniform weight 1 until it has a track record
    Founder,
    /// new node: weight 0 (measured, not counted) until N_PROBATION known outcomes
    Probation,
    /// has a track record: E_u-based, capped weight
    Established,
}

/// Classifies a node. A node with `N_PROBATION`+ known-outcome judgments is
/// established regardless of founder status; before that, founders seed at weight 1
/// and everyone else is on probation.
pub fn status(is_founder: bool, judgments_with_outcome: usize) -> Status {
    if judgments_with_outcome >= N_PROBATION {
        Status::Established
    } else if is_founder {
        Status::Founder
    } else {
        Status::Probation
    }
}

/// Review vote weight `w_u`: 0 on probation, 1 for a bootstrap founder, and
/// `min(w_max, E_u)` once established (`docs/02` C.2/C.4).
pub fn review_weight(status: Status, e_u: f64, w_max: f64) -> f64 {
    match status {
        Status::Probation => 0.0,
        Status::Founder => 1.0,
        Status::Established => capped_weight(e_u, w_max),
    }
}

/// Convenience: classify and weight in one step.
pub fn effective_review_weight(
    is_founder: bool,
    judgments_with_outcome: usize,
    e_u: f64,
    w_max: f64,
) -> f64 {
    review_weight(status(is_founder, judgments_with_outcome), e_u, w_max)
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
