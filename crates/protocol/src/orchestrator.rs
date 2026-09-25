//! Epoch orchestrator: the glue that closes two audit gaps whose *core* already exists
//! but was never wired at the epoch boundary.
//!
//! - **T5 / BRIDGE-007 / G-03 — reputation actually counts.** `bridging::fit` already
//!   minimizes the weighted objective `Σ w_u (r − r̂)²`; what was missing is the piece
//!   that turns a reviewer's *previous-epoch* standing into the `w_u` the current fit
//!   consumes. [`bridging_weights`] is that piece (probation = 0, founder = 1,
//!   established = `min(w_max, exp(γ·S_u·k_u/(k_u+k₀)))`, D33), and
//!   [`weighted_ratings`] hands it to the fit; [`epoch_weight_cap`] is the epoch's
//!   `3 × median` over the weights that count.
//!
//! - **T12 / §9.1 — one lifecycle, one decision path.** The stage-to-stage decisions of
//!   an epoch (a gate outcome becomes a pilot entry, a pilot verdict becomes pool or
//!   reject) used to be re-implemented imperatively by every caller. [`run_item`] drives
//!   those transitions through `lifecycle::step`, so the state machine — not the caller
//!   — owns them. [`review_round`] walks the commit/reveal sub-machine for one panel, and
//!   `run_item` scores the state it leaves: the machine itself checks that every
//!   panelist revealed (T33), so an epoch cannot be scored on a partial round.

use crate::appeal::{AppealOutcome, AuthorHistory, Escrow};
use crate::gate::GateOutcome;
use crate::lifecycle::{step, Event, Invalid, State};
use crate::probation::{effective_review_weight, status, Status};
use crate::review::commit;
use identity::nym::Nym;
use network::cid::Cid;
use scoring::bridging::{Obs, Ratings, RatingsError};
use scoring::reputation::weight_cap;

/// A reviewer's standing carried from the previous epoch, in the same order as the
/// ratings rows the fit will see. `skill` is `S_u`, the mean leave-one-out difference
/// score over the reviewer's `judgments_with_outcome` scored items (`docs/01` D33 / T50,
/// `reputation::{loo_scores, mean_score}`); it is ignored on probation and for founders.
#[derive(Clone, Copy, Debug)]
pub struct ReviewerStanding {
    pub is_founder: bool,
    pub judgments_with_outcome: usize,
    pub skill: f64,
}

impl ReviewerStanding {
    /// A bootstrap founder: unit review weight until it has a track record
    /// (`docs/05` §Cold start).
    pub fn founder() -> Self {
        ReviewerStanding {
            is_founder: true,
            judgments_with_outcome: 0,
            skill: 0.0,
        }
    }

    /// An established reviewer with skill `S_u`, just past probation.
    pub fn established(skill: f64) -> Self {
        ReviewerStanding {
            is_founder: false,
            judgments_with_outcome: crate::probation::N_PROBATION,
            skill,
        }
    }
}

/// Per-reviewer bridging weight `w_u` from the previous epoch's standing (T5,
/// BRIDGE-007). This is exactly the review vote weight — 0 on probation, 1 for a
/// bootstrap founder, the capped odds weight of the skill once established (D33) — so
/// the same reputation that weights the aggregation also weights the fit that produces
/// the bridge score.
pub fn bridging_weights(prev: &[ReviewerStanding], w_max: f64) -> Vec<f64> {
    prev.iter()
        .map(|r| effective_review_weight(r.is_founder, r.judgments_with_outcome, r.skill, w_max))
        .collect()
}

/// The epoch's weight cap `w_max = 3 × median(w)` (`docs/02` §C.4, D33), over the
/// uncapped weights of the reviewers who carry weight — founders at 1, established
/// reviewers at their odds weight; probationers, at 0, are not part of the crowd the cap
/// is relative to. With nobody carrying weight there is nothing to cap: `+∞`.
pub fn epoch_weight_cap(prev: &[ReviewerStanding]) -> f64 {
    let counted: Vec<f64> = prev
        .iter()
        .filter(|r| status(r.is_founder, r.judgments_with_outcome) != Status::Probation)
        .map(|r| {
            effective_review_weight(
                r.is_founder,
                r.judgments_with_outcome,
                r.skill,
                f64::INFINITY,
            )
        })
        .collect();
    if counted.is_empty() {
        f64::INFINITY
    } else {
        weight_cap(&counted)
    }
}

/// Builds the current epoch's [`Ratings`] with the per-reviewer weights derived from the
/// previous epoch (T5). `prev` is indexed like the rows of `r`/`mask`: a standing count
/// that differs from the rows, or a malformed matrix, is refused (`RatingsError`, T62).
pub fn weighted_ratings(
    r: &[Vec<f64>],
    mask: &[Vec<bool>],
    prev: &[ReviewerStanding],
    w_max: f64,
) -> Result<Ratings, RatingsError> {
    let ratings = Ratings::from_dense(r, mask).with_weights(bridging_weights(prev, w_max));
    ratings.validate()?;
    Ok(ratings)
}

/// The gate and pilot verdicts an epoch computes for one item, handed to the state
/// machine so the *lifecycle* decisions are made by [`run_item`], not the caller.
#[derive(Clone, Copy, Debug)]
pub struct ItemVerdicts {
    /// Level A bridging gate outcome.
    pub gate: GateOutcome,
    /// The author appealed a polarization rejection.
    pub appealed: bool,
    /// The appeal was filed inside the appeal window.
    pub appeal_within_window: bool,
    /// The author's `C_a` when the appeal was filed (`appeal::AuthorHistory::reputation`),
    /// and the floor it must cover (`appeal::appeal_floor`): `run_item` derives
    /// `reputation_covers_stake` from the two, it is never asserted by the caller (T61).
    pub author_reputation: f64,
    pub appeal_floor: f64,
    /// Pilot stage 1 (discrimination screen) had enough distinct respondents (INV-8).
    pub enough_respondents: bool,
    /// Pilot stage 1 verdict.
    pub screen_passed: bool,
    /// Pilot stage 2 (DIF) verdict.
    pub dif_passed: bool,
    /// Number of items in the stage-2 DIF batch (never validate below `K_MIN`, INV-8).
    pub pilot2_batch_size: usize,
}

/// One panelist's blind judgment: the probability it commits to, and the nonce it later
/// reveals (INV-12).
#[derive(Clone, Copy, Debug)]
pub struct Judgment {
    pub nym: Nym,
    pub prob: f64,
    pub nonce: [u8; 32],
}

/// Walks one review round through `lifecycle::step` from `Admitted`: assign `panel` to
/// `item`, every judgment commits, commits close, every judgment reveals. Returns the
/// `Revealing` state to hand to [`run_item`]; any invalid move (duplicate panelist,
/// outsider, double commit or reveal) is the machine's rejection.
pub fn review_round(
    admitted: State,
    item: Cid,
    panel: Vec<Nym>,
    judgments: &[Judgment],
) -> Result<State, Invalid> {
    let mut s = step(admitted, Event::AssignReviewers { panel, item })?;
    for j in judgments {
        let commitment = commit(j.prob, &j.nonce, j.nym, item);
        s = step(
            s,
            Event::Commit {
                nym: j.nym,
                commitment,
            },
        )?;
    }
    s = step(s, Event::CloseCommits)?;
    for j in judgments {
        s = step(
            s,
            Event::Reveal {
                nym: j.nym,
                prob: j.prob,
                nonce: j.nonce,
            },
        )?;
    }
    Ok(s)
}

/// The band's extra round (D26, T60): `k_extra` reviewers outside the first panel and
/// their blind judgments on the same item.
#[derive(Clone, Debug)]
pub struct ExtraRound {
    pub panel: Vec<Nym>,
    pub judgments: Vec<Judgment>,
}

/// Walks the band's extra round through `lifecycle::step` from `SupplementaryReview`:
/// assign the extra panel, every judgment commits, commits close, every judgment reveals
/// — as [`review_round`] does for the first panel. Returns the state to resolve; any
/// invalid move (a panelist of the first round, a repeated nym, an outsider, a double
/// commit or reveal) is the machine's rejection.
pub fn extra_round(band: State, extra: &ExtraRound) -> Result<State, Invalid> {
    let item = match &band {
        State::SupplementaryReview { item, .. } => *item,
        _ => return Err(Invalid::UnexpectedEvent),
    };
    let mut s = step(
        band,
        Event::AssignExtraReviewers {
            panel: extra.panel.clone(),
        },
    )?;
    for j in &extra.judgments {
        let commitment = commit(j.prob, &j.nonce, j.nym, item);
        s = step(
            s,
            Event::Commit {
                nym: j.nym,
                commitment,
            },
        )?;
    }
    s = step(s, Event::CloseCommits)?;
    for j in &extra.judgments {
        s = step(
            s,
            Event::Reveal {
                nym: j.nym,
                prob: j.prob,
                nonce: j.nonce,
            },
        )?;
    }
    Ok(s)
}

/// The ratings the band re-decision fits (D26, T60): `base` — the epoch's ratings, one
/// row per reviewer in `rows` — plus one observation of `item` per extra reveal. An extra
/// reviewer with a row rates the item from it (their position on the axis is what they
/// rated elsewhere); one without a row gets a new row, weighted by `weight_of_new`.
pub fn expanded_ratings(
    base: &Ratings,
    rows: &[Nym],
    item: usize,
    reveals: &[(Nym, f64)],
    weight_of_new: impl Fn(&Nym) -> f64,
) -> Ratings {
    let mut expanded = base.clone();
    for &(nym, r) in reveals {
        let u = match rows.iter().position(|n| *n == nym) {
            Some(u) => u,
            None => {
                expanded.n += 1;
                expanded.weights.push(weight_of_new(&nym));
                expanded.n - 1
            }
        };
        expanded.obs.push(Obs { u, j: item, r });
    }
    expanded
}

/// Drives one item from a scored review round to its terminal `State` via
/// `lifecycle::step` (T12). `reviewed` is the state [`review_round`] left; scoring it is
/// refused unless every panelist revealed. `ActivePool` means it reached the pool.
///
/// An appealed item's `Appeal` event carries what the verdicts say: filed within the
/// window, and an author reputation that covers the stake (`appeal_floor`); the escrow
/// the filing left in the author's history is settled by [`settle_appeal`] on the
/// terminal state (D27, T61).
///
/// A band item is scored to `SupplementaryReview`, walks the D26 extra round `extra`
/// ([`extra_round`]; a band item without one is refused, `NoExtraPanel`), and is resolved
/// by `redecide`, called on the extra round's reveals as the machine recorded them, once
/// the round is complete: the caller folds them into the epoch's ratings
/// ([`expanded_ratings`]) and re-runs `gate::supplementary_review` — a re-run bridging
/// fit re-deciding the side-balanced score against the plain threshold (T10/T30, amended
/// by T59, T60) — so a passing band item advances to the pilot, a polarized failing one
/// keeps the appeal channel, and a defect is a `Borderline` reject, never a dead end.
pub fn run_item(
    reviewed: State,
    v: &ItemVerdicts,
    extra: Option<&ExtraRound>,
    redecide: impl FnOnce(&[(Nym, f64)]) -> GateOutcome,
) -> Result<State, Invalid> {
    let mut s = step(reviewed, Event::Score { outcome: v.gate })?;

    if matches!(s, State::SupplementaryReview { .. }) {
        if let Some(extra) = extra {
            s = extra_round(s, extra)?;
        }
        // The re-decision reads the round's reveals; a round the machine would refuse
        // (no extra panel, a panelist missing) is not re-decided at all.
        let outcome = match &s {
            State::SupplementaryReview {
                extra_panel,
                reveals,
                ..
            } if !extra_panel.is_empty()
                && extra_panel
                    .iter()
                    .all(|p| reveals.iter().any(|(n, _)| n == p)) =>
            {
                redecide(reveals)
            }
            _ => GateOutcome::Reject,
        };
        s = step(s, Event::Resolve { outcome })?;
    }

    if matches!(s, State::AppealEligible) {
        s = if v.appealed {
            step(
                s,
                Event::Appeal {
                    within_window: v.appeal_within_window,
                    reputation_covers_stake: v.author_reputation >= v.appeal_floor,
                },
            )?
        } else {
            step(s, Event::AppealExpires)?
        };
    }

    if matches!(s, State::Pilot1 { .. }) {
        s = step(
            s,
            Event::Pilot1Batch {
                enough_respondents: v.enough_respondents,
                passed: v.screen_passed,
            },
        )?;
    }

    if matches!(s, State::Pilot2 { .. }) {
        s = step(
            s,
            Event::Pilot2Batch {
                batch_size: v.pilot2_batch_size,
                passed: v.dif_passed,
            },
        )?;
    }

    Ok(s)
}

/// Settles an appeal's escrow on the item's terminal state (D27, T61): reaching the pool
/// promotes the item, and its measured `quality` replaces the zero-quality
/// pseudo-observation in the author's history; any other terminal leaves the zero
/// standing — the item's real result.
pub fn settle_appeal(author: &mut AuthorHistory, escrow: Escrow, terminal: &State, quality: f64) {
    let outcome = if matches!(terminal, State::ActivePool) {
        AppealOutcome::Promoted { quality }
    } else {
        AppealOutcome::Failed
    };
    author.settle(escrow, outcome);
}
