//! Epoch orchestrator: the glue that closes two audit gaps whose *core* already exists
//! but was never wired at the epoch boundary.
//!
//! - **T5 / BRIDGE-007 / G-03 — reputation actually counts.** `bridging::fit` already
//!   minimizes the weighted objective `Σ w_u (r − r̂)²`; what was missing is the piece
//!   that turns a reviewer's *previous-epoch* standing into the `w_u` the current fit
//!   consumes. [`bridging_weights`] is that piece (probation = 0, founder = 1,
//!   established = `min(w_max, E_u)`), and [`weighted_ratings`] hands it to the fit.
//!
//! - **T12 / §9.1 — one lifecycle, one decision path.** The stage-to-stage decisions of
//!   an epoch (a gate outcome becomes a pilot entry, a pilot verdict becomes pool or
//!   reject) used to be re-implemented imperatively by every caller. [`run_item`] drives
//!   those transitions through `lifecycle::step`, so the state machine — not the caller
//!   — owns them. The commit/reveal sub-walk is entered at its close (`Revealing` +
//!   `Score`): that half is exercised move-by-move in `tests/orchestrator.rs`, while an
//!   epoch enters the machine with the gate outcome its review round produced.

use crate::gate::GateOutcome;
use crate::lifecycle::{step, Event, Invalid, State};
use crate::probation::effective_review_weight;
use network::cid::Cid;
use scoring::bridging::Ratings;

/// A reviewer's standing carried from the previous epoch, in the same order as the
/// ratings rows the fit will see. `e_u` is the evaluator score from that epoch
/// (`reputation::evaluator_score` of the Brier skill score against the crowd baseline,
/// `docs/01` D23 / T31); it is ignored on probation and for founders.
#[derive(Clone, Copy, Debug)]
pub struct ReviewerStanding {
    pub is_founder: bool,
    pub judgments_with_outcome: usize,
    pub e_u: f64,
}

impl ReviewerStanding {
    /// A bootstrap founder: unit review weight until it has a track record
    /// (`docs/05` §Cold start).
    pub fn founder() -> Self {
        ReviewerStanding {
            is_founder: true,
            judgments_with_outcome: 0,
            e_u: 0.0,
        }
    }

    /// An established reviewer with evaluator score `e_u`.
    pub fn established(e_u: f64) -> Self {
        ReviewerStanding {
            is_founder: false,
            judgments_with_outcome: crate::probation::N_PROBATION,
            e_u,
        }
    }
}

/// Per-reviewer bridging weight `w_u` from the previous epoch's standing (T5,
/// BRIDGE-007). This is exactly the review vote weight — 0 on probation, 1 for a
/// bootstrap founder, `min(w_max, E_u)` once established — so the same reputation that
/// weights the aggregation also weights the fit that produces `b_j`.
pub fn bridging_weights(prev: &[ReviewerStanding], w_max: f64) -> Vec<f64> {
    prev.iter()
        .map(|r| effective_review_weight(r.is_founder, r.judgments_with_outcome, r.e_u, w_max))
        .collect()
}

/// Builds the current epoch's [`Ratings`] with the per-reviewer weights derived from the
/// previous epoch (T5). `prev` is indexed like the rows of `r`/`mask`.
pub fn weighted_ratings(
    r: &[Vec<f64>],
    mask: &[Vec<bool>],
    prev: &[ReviewerStanding],
    w_max: f64,
) -> Ratings {
    Ratings::from_dense(r, mask).with_weights(bridging_weights(prev, w_max))
}

/// The gate and pilot verdicts an epoch computes for one item, handed to the state
/// machine so the *lifecycle* decisions are made by [`run_item`], not the caller.
#[derive(Clone, Copy, Debug)]
pub struct ItemVerdicts {
    /// The item's content id (its identity through the epoch).
    pub item: Cid,
    /// Level A bridging gate outcome.
    pub gate: GateOutcome,
    /// The author appealed a polarization rejection.
    pub appealed: bool,
    /// D26 supplementary re-decision result (`gate::supplementary_review`, T10/T30):
    /// consulted only when `gate == SupplementaryReview`.
    pub band_advances: bool,
    /// Pilot stage 1 (discrimination screen) had enough distinct respondents (INV-8).
    pub enough_respondents: bool,
    /// Pilot stage 1 verdict.
    pub screen_passed: bool,
    /// Pilot stage 2 (DIF) verdict.
    pub dif_passed: bool,
    /// Number of items in the stage-2 DIF batch (never validate below `K_MIN`, INV-8).
    pub pilot2_batch_size: usize,
}

/// Drives one item from a scored review round to its terminal `State` via
/// `lifecycle::step` (T12). `ActivePool` means it reached the pool.
///
/// A band item is scored to `SupplementaryReview` and then resolved by the D26 mechanism
/// (T10/T30): `band_advances` is the outcome of `gate::supplementary_review` — a re-run
/// bridging fit re-deciding `b_j` against the plain threshold — so a passing band item
/// advances to the pilot and a failing one is a `Borderline` reject, not a dead end.
pub fn run_item(v: &ItemVerdicts) -> Result<State, Invalid> {
    // Enter at the close of the review round to apply the gate outcome; the commit-reveal
    // sub-walk (and its INV-12 binding) is exercised move-by-move in `tests/orchestrator.rs`.
    let mut s = step(
        State::Revealing {
            item: v.item,
            commits: Vec::new(),
            reveals: Vec::new(),
        },
        Event::Score {
            all_reveals_in: true,
            outcome: v.gate,
        },
    )?;

    if matches!(s, State::SupplementaryReview) {
        s = step(
            s,
            Event::Resolve {
                passed: v.band_advances,
            },
        )?;
    }

    if matches!(s, State::AppealEligible) {
        s = if v.appealed {
            step(
                s,
                Event::Appeal {
                    within_window: true,
                    reputation_covers_stake: true,
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
