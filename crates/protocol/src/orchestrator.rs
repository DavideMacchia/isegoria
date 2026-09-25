//! Epoch orchestrator: the glue that closes two audit gaps whose *core* already exists
//! but was never wired at the epoch boundary.
//!
//! - **T5 / BRIDGE-007 / G-03 — reputation actually counts.** `bridging::fit` already
//!   minimizes the weighted objective `Σ w_u (r − r̂)²`; what was missing is the piece
//!   that turns a reviewer's *previous-epoch* standing into the `w_u` the current fit
//!   consumes. [`bridging_weights`] is that piece (probation = 0, founder = 1,
//!   established = the odds weight of the evaluator score, D33, then the epoch's
//!   `3 × median` cap), and [`weighted_ratings`] hands it to the fit.
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
use crate::probation::effective_review_weight;
use crate::review::commit;
use identity::nym::Nym;
use network::cid::Cid;
use scoring::bridging::{Ratings, RatingsError};
use scoring::reputation::{cap_weights, EvaluatorHistory};

/// A reviewer's standing carried from the previous epochs, in the same order as the
/// ratings rows the fit will see. `score` is the evaluator score `S_u` — the mean
/// leave-one-out difference score over its `scored` outcomes (`honeypot::reviewer_skills`,
/// `docs/01` D33); both are ignored on probation and for founders.
#[derive(Clone, Copy, Debug)]
pub struct ReviewerStanding {
    pub is_founder: bool,
    pub scored: usize,
    pub score: f64,
}

impl ReviewerStanding {
    /// A bootstrap founder: unit review weight until it has a track record
    /// (`docs/05` §Cold start).
    pub fn founder() -> Self {
        ReviewerStanding {
            is_founder: true,
            scored: 0,
            score: 0.0,
        }
    }

    /// An established reviewer with evaluator score `score` over `scored` outcomes (at
    /// least `N_PROBATION`, or it is on probation).
    pub fn established(score: f64, scored: usize) -> Self {
        ReviewerStanding {
            is_founder: false,
            scored,
            score,
        }
    }

    /// The standing a scored history gives (D33, D34, T51): its long-window mean and its
    /// count since the last restart — so a reviewer the CUSUM caught is back on probation
    /// with nothing to its name. A founder's seed weight is a bootstrap privilege, lost
    /// at the first alarm: a caught founder is on probation like anyone else.
    pub fn from_history(is_founder: bool, history: &EvaluatorHistory) -> Self {
        ReviewerStanding {
            is_founder: is_founder && history.alarms() == 0,
            scored: history.scored(),
            score: history.score(),
        }
    }
}

/// Per-reviewer bridging weight `w_u` from the previous epochs' standing (T5,
/// BRIDGE-007; D33, T50). This is exactly the review vote weight — 0 on probation, 1
/// for a bootstrap founder, the odds weight `exp(γ · S_u · k_u / (k_u + k_0))` once
/// established, then the epoch's cap at `3 × median` of the counted weights
/// (`reputation::cap_weights`) — so the same reputation that weights the aggregation
/// also weights the fit that produces the bridge score, and no reviewer outweighs three
/// median ones.
pub fn bridging_weights(prev: &[ReviewerStanding]) -> Vec<f64> {
    let raw: Vec<f64> = prev
        .iter()
        .map(|r| effective_review_weight(r.is_founder, r.scored, r.score))
        .collect();
    cap_weights(&raw)
}

/// Builds the current epoch's [`Ratings`] with the per-reviewer weights derived from the
/// previous epochs (T5). `prev` is indexed like the rows of `r`/`mask`: a standing count
/// that differs from the rows, or a malformed matrix, is refused (`RatingsError`, T62).
pub fn weighted_ratings(
    r: &[Vec<f64>],
    mask: &[Vec<bool>],
    prev: &[ReviewerStanding],
) -> Result<Ratings, RatingsError> {
    let ratings = Ratings::from_dense(r, mask).with_weights(bridging_weights(prev));
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
    /// D26 supplementary re-decision outcome (`gate::supplementary_review`, T10/T30/T59):
    /// `Pass`, `AppealEligible` or `Reject`; consulted only when
    /// `gate == SupplementaryReview`.
    pub band_outcome: GateOutcome,
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

/// Drives one item from a scored review round to its terminal `State` via
/// `lifecycle::step` (T12). `reviewed` is the state [`review_round`] left; scoring it is
/// refused unless every panelist revealed. `ActivePool` means it reached the pool.
///
/// An appealed item's `Appeal` event carries what the verdicts say: filed within the
/// window, and an author reputation that covers the stake (`appeal_floor`); the escrow
/// the filing left in the author's history is settled by [`settle_appeal`] on the
/// terminal state (D27, T61).
///
/// A band item is scored to `SupplementaryReview` and then resolved by the D26 mechanism
/// (T10/T30, amended by T59): `band_outcome` is the outcome of
/// `gate::supplementary_review` — a re-run bridging fit re-deciding the side-balanced
/// score against the plain threshold — so a passing band item advances to the pilot, a
/// polarized failing one keeps the appeal channel, and a defect is a `Borderline`
/// reject, never a dead end.
pub fn run_item(reviewed: State, v: &ItemVerdicts) -> Result<State, Invalid> {
    let mut s = step(reviewed, Event::Score { outcome: v.gate })?;

    if matches!(s, State::SupplementaryReview) {
        s = step(
            s,
            Event::Resolve {
                outcome: v.band_outcome,
            },
        )?;
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
