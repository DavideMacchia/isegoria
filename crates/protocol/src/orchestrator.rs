//! Epoch orchestrator (`docs/02` §A.4/§C.4, `docs/05`): wires reputation into the bridging
//! fit's per-reviewer weights ([`bridging_weights`], D33) and drives an item's stage
//! transitions through `lifecycle::step` ([`run_item`], [`review_round`]).

use crate::appeal::{AppealOutcome, AuthorHistory, Escrow};
use crate::gate::GateOutcome;
use crate::lifecycle::{step, Event, Invalid, State};
use crate::probation::{effective_review_weight, status, Status};
use crate::review::commit;
use identity::nym::Nym;
use network::cid::Cid;
use scoring::bridging::{Obs, Ratings, RatingsError};
use scoring::reputation::weight_cap;

/// A reviewer's standing from the previous epoch, row-ordered like the fit's ratings.
/// `skill` is `S_u` (`reputation::{loo_scores, mean_score}`), ignored on probation/founders.
#[derive(Clone, Copy, Debug)]
pub struct ReviewerStanding {
    pub is_founder: bool,
    pub judgments_with_outcome: usize,
    pub skill: f64,
    pub reviews: usize,
}

impl ReviewerStanding {
    /// A bootstrap founder: unit review weight until it has a track record (`docs/05` §Cold start).
    pub fn founder() -> Self {
        ReviewerStanding {
            is_founder: true,
            judgments_with_outcome: 0,
            skill: 0.0,
            reviews: 0,
        }
    }

    pub fn established(skill: f64) -> Self {
        ReviewerStanding {
            is_founder: false,
            judgments_with_outcome: crate::probation::N_PROBATION,
            skill,
            reviews: crate::probation::N_PROBATION,
        }
    }
}

/// `n_min` of `docs/02` §A.4: below it `f_u` is fixed at 0 in the fit. Provisional (T25).
pub const N_MIN_REVIEWS: usize = 30;

/// Which reviewers define the axis this epoch (`Ratings::axis`): founders, and anyone
/// with at least [`N_MIN_REVIEWS`] reviews on record, in the order of `prev`.
pub fn axis_mask(prev: &[ReviewerStanding]) -> Vec<bool> {
    prev.iter()
        .map(|r| r.is_founder || r.reviews >= N_MIN_REVIEWS)
        .collect()
}

/// Per-reviewer bridging weight `w_u` from the previous epoch's standing: 0 on probation,
/// 1 for a founder, the capped odds weight once established (D33).
pub fn bridging_weights(prev: &[ReviewerStanding], w_max: f64) -> Vec<f64> {
    prev.iter()
        .map(|r| effective_review_weight(r.is_founder, r.judgments_with_outcome, r.skill, w_max))
        .collect()
}

/// The epoch's weight cap `w_max = 3 × median(w)` (`docs/02` §C.4, D33) over the uncapped
/// weights of reviewers who carry weight (probationers excluded). `+∞` if none do.
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

/// Builds the current epoch's [`Ratings`] with weights from the previous epoch and the
/// axis review floor ([`axis_mask`]); a malformed matrix is refused (`RatingsError`).
pub fn weighted_ratings(
    r: &[Vec<f64>],
    mask: &[Vec<bool>],
    prev: &[ReviewerStanding],
    w_max: f64,
) -> Result<Ratings, RatingsError> {
    let ratings = Ratings::from_dense(r, mask)
        .with_weights(bridging_weights(prev, w_max))
        .with_axis(axis_mask(prev));
    ratings.validate()?;
    Ok(ratings)
}

/// The gate and pilot verdicts an epoch computes for one item, for [`run_item`].
#[derive(Clone, Copy, Debug)]
pub struct ItemVerdicts {
    pub gate: GateOutcome,
    pub appealed: bool,
    pub appeal_within_window: bool,
    /// The author's `C_a` and the floor it must cover; [`run_item`] derives the covers-stake check.
    pub author_reputation: f64,
    pub appeal_floor: f64,
    pub enough_respondents: bool,
    pub screen_passed: bool,
    pub dif_passed: bool,
    /// The source check's verdict on a DIF failure: a contested fact if set (D38).
    pub source_verified: bool,
    pub pilot2_batch_size: usize,
    /// The beacon's exploration draw for this item (D35): measurement only, never the pool.
    pub explored: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Judgment {
    pub nym: Nym,
    pub prob: f64,
    pub nonce: [u8; 32],
}

/// Walks one review round through `lifecycle::step` from `Admitted` to `Revealing`, for
/// [`run_item`]; an invalid move is the machine's rejection.
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

/// The band's extra round (D26): reviewers outside the first panel and their judgments.
#[derive(Clone, Debug)]
pub struct ExtraRound {
    pub panel: Vec<Nym>,
    pub judgments: Vec<Judgment>,
}

/// Walks the band's extra round through `lifecycle::step` from `SupplementaryReview`, as
/// [`review_round`] does for the first panel; an invalid move is the machine's rejection.
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

/// The ratings the band re-decision fits (D26): `base` plus one observation of `item` per
/// extra reveal, adding a weighted row for a reviewer not already in `rows`.
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

/// Drives one item from a scored review round to its terminal `State` via `lifecycle::step`:
/// resolves an `Appeal`, a band's `SupplementaryReview` (via `redecide` over the extra
/// round's reveals, [`expanded_ratings`]), an exploration draw, then the pilot batches.
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
        // A round the machine refuses (no panel, a missing panelist) falls through to `Reject`.
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

    // Only a gate rejection can be `Rejected` here; a pilot rejection is reached below.
    if v.explored && matches!(s, State::Rejected(_)) {
        s = step(
            s,
            Event::Explore {
                seed_from_beacon: true,
            },
        )?;
    }

    if matches!(
        s,
        State::Pilot1 { .. }
            | State::Explored {
                screened: false,
                ..
            }
    ) {
        s = step(
            s,
            Event::Pilot1Batch {
                enough_respondents: v.enough_respondents,
                passed: v.screen_passed,
            },
        )?;
    }

    if matches!(
        s,
        State::Pilot2 { .. } | State::Explored { screened: true, .. }
    ) {
        s = step(
            s,
            Event::Pilot2Batch {
                batch_size: v.pilot2_batch_size,
                passed: v.dif_passed,
                source_verified: v.source_verified,
            },
        )?;
    }

    Ok(s)
}

/// Settles an appeal's escrow on the item's terminal state (D27): the pool, or the contested
/// pool (D38), promotes it via `quality`; any other terminal keeps the zero standing.
pub fn settle_appeal(author: &mut AuthorHistory, escrow: Escrow, terminal: &State, quality: f64) {
    let outcome = if matches!(terminal, State::ActivePool | State::Contested) {
        AppealOutcome::Promoted { quality }
    } else {
        AppealOutcome::Failed
    };
    author.settle(escrow, outcome);
}
