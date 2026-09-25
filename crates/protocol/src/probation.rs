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

/// A reviewer's scored history as the weight reads it (`docs/01` D33–D36; T51, T52):
/// the running mean of the per-item leave-one-out difference scores (`S_u`, the weight's
/// symmetric long-window mean), the count of scored items (`k_u`, which also decides
/// probation), and a one-sided CUSUM on the per-item scores against that mean. Once the
/// reviewer is out of probation an alarm — a sustained drop, a long con beginning to
/// spend its reputation — sends them back to probation: the mean, the counts and the
/// statistic restart, so the weight is 0 until `N_PROBATION` new scored outcomes and
/// then shrunk again as evidence accumulates.
///
/// With randomized exploration (D35) not every reviewed item's outcome is observed: an
/// observed score enters the mean at `1/π`, its inverse inclusion probability — 1 for an
/// item that entered the pilot on its own account, `1/ε` for an explored rejection — and
/// a reviewed item whose outcome was not observed enters the denominator only, so the
/// mean's expectation is the mean with every outcome observed (paper, "Exploration
/// restores properness"). The detector reads the *unweighted* observed scores against
/// their own mean: a change of behaviour shifts that stream too, and one explored item
/// cannot fire the detector by its weight alone.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SkillTrack {
    /// `Σ score / π` over the observed items.
    sum: f64,
    /// `Σ score` over the observed items: the detector's reference.
    plain: f64,
    /// Reviewed items whose outcome could have been observed: the mean's denominator.
    reviewed: usize,
    /// Observed (scored) items, `k_u`.
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

    /// Records one scored item observed with certainty — a golden item, a pilot outcome:
    /// [`record_observed`](Self::record_observed) at inclusion 1.
    pub fn record(&mut self, score: f64, params: &CusumParams) -> Option<Alarm> {
        self.record_observed(score, 1.0, params)
    }

    /// Records one scored item whose outcome was observed with probability `inclusion`,
    /// in `(0, 1]` (D35). The CUSUM reads the unweighted score against the mean of the
    /// observed items before it, and only once the reviewer is out of probation — a mean
    /// over fewer than `N_PROBATION` items is not a reference. On an alarm the track
    /// restarts.
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

    /// Records a reviewed item whose outcome was not observed — a gate rejection the
    /// exploration draw did not pick (D35): it counts in the mean's denominator, where
    /// the explored items' weights are set against it, and nowhere else.
    pub fn record_unobserved(&mut self) {
        self.reviewed += 1;
    }

    /// `S_u`: the inverse-probability-weighted mean score of the current stretch over
    /// its reviewed items (0 with nothing reviewed).
    pub fn skill(&self) -> f64 {
        if self.reviewed == 0 {
            0.0
        } else {
            self.sum / self.reviewed as f64
        }
    }

    /// The detector's reference: the unweighted mean of the observed scores (0 with
    /// nothing scored).
    pub fn reference(&self) -> f64 {
        if self.scored == 0 {
            0.0
        } else {
            self.plain / self.scored as f64
        }
    }

    /// The detector's current statistic.
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
