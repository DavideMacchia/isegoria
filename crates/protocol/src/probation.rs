//! Probation and cold start (`docs/03` P2, `docs/05` §Cold start).
//!
//! Non-rotatable IDs are worthless if a fresh node carries weight before it has a
//! track record, so a new node is on **probation**: its reviews measure its skill `S_u`
//! but do not determine outcomes (weight 0) until it has `N_PROBATION` scored outcomes.
//! At bootstrap a publicly declared, heterogeneous **founder set** carries uniform weight
//! 1 to seed the first outcomes; once any node crosses the probation threshold it
//! switches to its skill-based, capped odds weight (`docs/01` D33, D36).

use crate::orchestrator::ReviewerStanding;
use identity::nym::Nym;
use scoring::reputation::{capped_weight, odds_weight, Cusum, CusumParams, EvaluatorParams};
use std::collections::HashSet;

/// Scored outcomes before a new pseudonym carries weight (`docs/01` D36, T50: 30, was
/// 200). Probation exists so that a pseudonym with no track record has no influence;
/// after it the shrinkage of `odds_weight` moves the weight away from 1 only as evidence
/// accumulates, and whitewashing is prevented by non-rotatable pseudonyms (`docs/03` P2),
/// not by the length of probation.
pub const N_PROBATION: usize = 30;

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

/// Review vote weight `w_u`: 0 on probation, 1 for a bootstrap founder, and the capped
/// `weight` — the odds weight of the reviewer's skill — once established (`docs/02`
/// C.2/C.4, D33).
pub fn review_weight(status: Status, weight: f64, w_max: f64) -> f64 {
    match status {
        Status::Probation => 0.0,
        Status::Founder => 1.0,
        Status::Established => capped_weight(weight, w_max),
    }
}

/// Convenience: classify and weight in one step. `skill` is the reviewer's `S_u` (the
/// mean leave-one-out difference score, `reputation::mean_score`) and
/// `judgments_with_outcome` its `k_u`; the established weight is
/// `min(w_max, odds_weight(skill, k_u))` with the default `EvaluatorParams`.
pub fn effective_review_weight(
    is_founder: bool,
    judgments_with_outcome: usize,
    skill: f64,
    w_max: f64,
) -> f64 {
    let weight = odds_weight(skill, judgments_with_outcome, &EvaluatorParams::default());
    review_weight(status(is_founder, judgments_with_outcome), weight, w_max)
}

/// A reviewer's scored history as the weight reads it (`docs/01` D33, D34, D36; T51):
/// the running mean of the per-item leave-one-out difference scores (`S_u`, the weight's
/// symmetric long-window mean), the count of scored items (`k_u`, which also decides
/// probation), and a one-sided CUSUM on the per-item scores against that mean. Once the
/// reviewer is out of probation an alarm — a sustained drop, a long con beginning to
/// spend its reputation — sends them back to probation: the mean, the count and the
/// statistic restart, so the weight is 0 until `N_PROBATION` new scored outcomes and
/// then shrunk again as evidence accumulates.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SkillTrack {
    sum: f64,
    scored: usize,
    cusum: Cusum,
    alarms: usize,
}

/// The change detector fired: the reviewer is back on probation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Alarm {
    /// Alarms so far, this one included.
    pub count: usize,
    /// Scored items in the stretch the alarm closed.
    pub scored: usize,
}

impl SkillTrack {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records one scored item. The CUSUM reads the score against the mean of the items
    /// before it, and only once the reviewer is out of probation — a mean over fewer than
    /// `N_PROBATION` items is not a reference. On an alarm the track restarts.
    pub fn record(&mut self, score: f64, params: &CusumParams) -> Option<Alarm> {
        if self.scored >= N_PROBATION && self.cusum.observe(self.skill(), score, params) {
            self.alarms += 1;
            let closed = self.scored;
            self.sum = 0.0;
            self.scored = 0;
            self.cusum = Cusum::new();
            return Some(Alarm {
                count: self.alarms,
                scored: closed,
            });
        }
        self.sum += score;
        self.scored += 1;
        None
    }

    /// `S_u`: the mean score of the current stretch (0 with nothing scored).
    pub fn skill(&self) -> f64 {
        if self.scored == 0 {
            0.0
        } else {
            self.sum / self.scored as f64
        }
    }

    /// `k_u`: scored items in the current stretch.
    pub fn scored(&self) -> usize {
        self.scored
    }

    pub fn alarms(&self) -> usize {
        self.alarms
    }

    pub fn status(&self, is_founder: bool) -> Status {
        status(is_founder, self.scored)
    }

    /// The standing the next epoch's fit reads (`orchestrator::bridging_weights`).
    pub fn standing(&self, is_founder: bool) -> ReviewerStanding {
        ReviewerStanding {
            is_founder,
            judgments_with_outcome: self.scored,
            skill: self.skill(),
        }
    }

    /// The review weight now: 0 on probation, 1 for a founder, the capped odds weight
    /// of the skill once established.
    pub fn weight(&self, is_founder: bool, w_max: f64) -> f64 {
        effective_review_weight(is_founder, self.scored, self.skill(), w_max)
    }
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
