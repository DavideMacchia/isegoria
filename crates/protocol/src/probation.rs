//! Probation and cold start (`docs/03` P2, `docs/05` §Cold start): a fresh node has
//! weight 0 until `N_PROBATION` scored outcomes; founders seed at weight 1 until then,
//! then everyone uses the skill-based, capped odds weight (`docs/01` D33, D36).

use crate::orchestrator::ReviewerStanding;
use identity::nym::Nym;
use scoring::reputation::{capped_weight, odds_weight, Cusum, CusumParams, EvaluatorParams};
use std::collections::HashSet;

/// Scored outcomes before a new pseudonym carries weight (`docs/01` D36).
pub const N_PROBATION: usize = 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Founder,
    Probation,
    Established,
}

pub fn status(is_founder: bool, judgments_with_outcome: usize) -> Status {
    if judgments_with_outcome >= N_PROBATION {
        Status::Established
    } else if is_founder {
        Status::Founder
    } else {
        Status::Probation
    }
}

/// Review vote weight `w_u`: 0 on probation, 1 for a bootstrap founder, the capped odds
/// weight of skill once established (`docs/02` C.2/C.4, D33).
pub fn review_weight(status: Status, weight: f64, w_max: f64) -> f64 {
    match status {
        Status::Probation => 0.0,
        Status::Founder => 1.0,
        Status::Established => capped_weight(weight, w_max),
    }
}

/// Convenience: classifies and weights in one step. `skill` is `S_u`,
/// `judgments_with_outcome` its `k_u`; established weight `min(w_max, odds_weight(skill, k_u))`.
pub fn effective_review_weight(
    is_founder: bool,
    judgments_with_outcome: usize,
    skill: f64,
    w_max: f64,
) -> f64 {
    let weight = odds_weight(skill, judgments_with_outcome, &EvaluatorParams::default());
    review_weight(status(is_founder, judgments_with_outcome), weight, w_max)
}

/// A reviewer's scored history (`docs/01` D33–D36): the IPW mean score `S_u`, the
/// observed count `k_u`, and a CUSUM change detector; an alarm resets the track to
/// probation (`docs/05` §Cold start; exploration weighting per D35).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SkillTrack {
    sum: f64,
    plain: f64,
    reviewed: usize,
    scored: usize,
    cusum: Cusum,
    alarms: usize,
}

/// The change detector fired: the reviewer is back on probation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Alarm {
    pub count: usize,
    pub scored: usize,
}

impl SkillTrack {
    pub fn new() -> Self {
        Self::default()
    }

    /// One scored item observed with certainty ([`record_observed`](Self::record_observed) at 1).
    pub fn record(&mut self, score: f64, params: &CusumParams) -> Option<Alarm> {
        self.record_observed(score, 1.0, params)
    }

    /// Records one scored item observed with probability `inclusion`, in `(0, 1]` (D35): the
    /// CUSUM compares it to the pre-item mean once out of probation; an alarm restarts the track.
    pub fn record_observed(
        &mut self,
        score: f64,
        inclusion: f64,
        params: &CusumParams,
    ) -> Option<Alarm> {
        assert!(
            inclusion > 0.0 && inclusion <= 1.0,
            "an inclusion probability in (0, 1], not {inclusion}"
        );
        if self.scored >= N_PROBATION && self.cusum.observe(self.reference(), score, params) {
            let closed = self.scored;
            *self = SkillTrack {
                alarms: self.alarms + 1,
                ..SkillTrack::default()
            };
            return Some(Alarm {
                count: self.alarms,
                scored: closed,
            });
        }
        self.sum += score / inclusion;
        self.plain += score;
        self.reviewed += 1;
        self.scored += 1;
        None
    }

    pub fn record_unobserved(&mut self) {
        self.reviewed += 1;
    }

    /// `S_u`: the inclusion-weighted mean score of the current stretch (0 if nothing reviewed).
    pub fn skill(&self) -> f64 {
        if self.reviewed == 0 {
            0.0
        } else {
            self.sum / self.reviewed as f64
        }
    }

    /// The detector's reference: the unweighted mean of observed scores (0 if none scored).
    pub fn reference(&self) -> f64 {
        if self.scored == 0 {
            0.0
        } else {
            self.plain / self.scored as f64
        }
    }

    pub fn statistic(&self) -> f64 {
        self.cusum.statistic()
    }

    /// `k_u`: scored (observed) items in the current stretch.
    pub fn scored(&self) -> usize {
        self.scored
    }

    /// Reviewed items in the current stretch, observed or not.
    pub fn reviewed(&self) -> usize {
        self.reviewed
    }

    pub fn alarms(&self) -> usize {
        self.alarms
    }

    pub fn status(&self, is_founder: bool) -> Status {
        status(is_founder, self.scored)
    }

    pub fn standing(&self, is_founder: bool) -> ReviewerStanding {
        ReviewerStanding {
            is_founder,
            judgments_with_outcome: self.scored,
            skill: self.skill(),
            reviews: self.reviewed,
        }
    }

    pub fn weight(&self, is_founder: bool, w_max: f64) -> f64 {
        effective_review_weight(is_founder, self.scored, self.skill(), w_max)
    }
}

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
