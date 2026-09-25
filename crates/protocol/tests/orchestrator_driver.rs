//! The epoch orchestrator (`protocol::orchestrator`): the two audit gaps whose core
//! already existed but were not wired at the epoch boundary.
//!
//! - **T5 / BRIDGE-007:** reviewer weights derived from prior-epoch standing flow into
//!   `bridging::fit`, so lower reputation means less influence on `b_j`.
//! - **T12 / §9.1:** an item's stage-to-stage fate is decided by `lifecycle::step`,
//!   driven by the epoch's gate and pilot verdicts, not by ad-hoc caller logic.

use identity::nym::Nym;
use network::cid::{cid, Cid};
use protocol::gate::GateOutcome;
use protocol::lifecycle::{deposit, step, Event, Invalid, RejectReason, State};
use protocol::orchestrator::{
    bridging_weights, review_round, run_item, weighted_ratings, ItemVerdicts, Judgment,
    ReviewerStanding,
};
use protocol::probation::N_PROBATION;
use scoring::bridging::{fit, side_balanced, BridgingParams, RatingsError};

// ------------------------------- T5: weights from standing -------------------------------

/// The bridging weight is the review vote weight: 0 on probation, 1 for a bootstrap
/// founder, `min(w_max, E_u)` once established.
#[test]
fn bridging_weights_map_probation_founder_established() {
    let prev = [
        ReviewerStanding::founder(),
        ReviewerStanding::established(0.3),
        ReviewerStanding::established(2.0), // above the cap
        // a fresh node with no track record: on probation, weight 0
        ReviewerStanding {
            is_founder: false,
            judgments_with_outcome: 0,
            e_u: 0.9,
        },
        // a founder that has crossed the probation threshold is established, not seeded
        ReviewerStanding {
            is_founder: true,
            judgments_with_outcome: N_PROBATION,
            e_u: 0.4,
        },
    ];
    let w = bridging_weights(&prev, 1.0);
    assert_eq!(w, vec![1.0, 0.3, 1.0, 0.0, 0.4]);
}

/// The load-bearing T5 claim at the protocol boundary: a coordinated bloc pushing an
/// item up moves its bridge score LESS when its prior-epoch reputation is low. Same
/// reviewer count in both fits (so the fit's seeded init is identical) — only the
/// standing differs — which isolates the weight as the cause.
#[test]
fn lower_reputation_moves_the_bridge_score_less() {
    let (m, t) = (5usize, 4usize);
    let (honest, bloc) = (30usize, 30usize);

    // Honest reviewers hover near 0.5 with a little spread; the bloc all shove item t to
    // 1.0. Every other cell is ~0.5 so the difference is concentrated on item t.
    let mut rows: Vec<Vec<f64>> = Vec::new();
    for u in 0..honest {
        rows.push(
            (0..m)
                .map(|j| 0.5 + (((u * 7 + j * 3) % 5) as f64 - 2.0) * 0.02)
                .collect(),
        );
    }
    for _ in 0..bloc {
        rows.push((0..m).map(|j| if j == t { 1.0 } else { 0.5 }).collect());
    }
    let mask = vec![vec![true; m]; rows.len()];
    let params = BridgingParams::default();

    let b_t = |standings: &[ReviewerStanding]| {
        let f = fit(
            &weighted_ratings(&rows, &mask, standings, 1.0).unwrap(),
            &params,
        )
        .unwrap();
        side_balanced(&f).score[t]
    };

    // Case A: the bloc are founders (full weight 1).
    let mut trusted = vec![ReviewerStanding::founder(); honest];
    trusted.extend(std::iter::repeat_n(ReviewerStanding::founder(), bloc));
    // Case B: the bloc are established with a tiny evaluator score (weight ≈ 0.02).
    let mut discounted = vec![ReviewerStanding::founder(); honest];
    discounted.extend(std::iter::repeat_n(
        ReviewerStanding::established(0.02),
        bloc,
    ));

    let b_trusted = b_t(&trusted);
    let b_discounted = b_t(&discounted);
    assert!(
        b_discounted < b_trusted,
        "a low-reputation bloc must move b_j less: discounted={b_discounted:.4} trusted={b_trusted:.4}"
    );
}

// ------------------------------- T12: the machine decides -------------------------------

/// Verdicts for a plain (non-band, non-appeal) item that clears both pilot stages.
fn passing() -> ItemVerdicts {
    ItemVerdicts {
        gate: GateOutcome::Pass,
        appealed: false,
        appeal_within_window: true,
        author_reputation: 0.6,
        appeal_floor: 0.4,
        band_outcome: GateOutcome::Reject,
        enough_respondents: true,
        screen_passed: true,
        dif_passed: true,
        pilot2_batch_size: 8,
    }
}

fn item() -> Cid {
    cid(b"item")
}

fn panel() -> Vec<Nym> {
    (1..=9).map(|i| Nym([i; 32])).collect()
}

fn admitted() -> State {
    step(
        deposit(true, true, true, true).unwrap(),
        Event::Admit {
            seed_from_checkpoint: true,
        },
    )
    .unwrap()
}

fn judgments(who: &[Nym]) -> Vec<Judgment> {
    who.iter()
        .map(|&nym| Judgment {
            nym,
            prob: 0.7,
            nonce: [nym.0[0]; 32],
        })
        .collect()
}

/// A complete review round: the full panel commits and reveals.
fn reviewed() -> State {
    review_round(admitted(), item(), panel(), &judgments(&panel())).unwrap()
}

/// `run_item` scores the round the machine walked: a panelist who never judged blocks
/// the epoch, and a repeated nym never forms a panel (T33).
#[test]
fn run_item_refuses_an_incomplete_or_forged_review_round() {
    let p = panel();
    let partial = review_round(admitted(), item(), p.clone(), &judgments(&p[..8])).unwrap();
    assert_eq!(run_item(partial, &passing()), Err(Invalid::PartialEpoch));

    let mut dup = p.clone();
    dup[8] = dup[0];
    assert_eq!(
        review_round(admitted(), item(), dup, &judgments(&p[..8])),
        Err(Invalid::DuplicatePanelist)
    );

    // An outsider's judgment is refused at its commit.
    let outsider = judgments(&[Nym([99; 32])]);
    assert_eq!(
        review_round(admitted(), item(), p, &outsider),
        Err(Invalid::NotInPanel)
    );
}

#[test]
fn a_clean_item_reaches_the_pool() {
    assert_eq!(run_item(reviewed(), &passing()).unwrap(), State::ActivePool);
}

#[test]
fn the_screen_and_the_dif_stage_each_stop_an_item() {
    let screened = ItemVerdicts {
        screen_passed: false,
        ..passing()
    };
    assert_eq!(
        run_item(reviewed(), &screened).unwrap(),
        State::Rejected(RejectReason::Screen)
    );
    let dif = ItemVerdicts {
        dif_passed: false,
        ..passing()
    };
    assert_eq!(
        run_item(reviewed(), &dif).unwrap(),
        State::Rejected(RejectReason::Dif)
    );
}

#[test]
fn a_defect_reject_never_enters_the_pilot() {
    let defect = ItemVerdicts {
        gate: GateOutcome::Reject,
        ..passing()
    };
    assert_eq!(
        run_item(reviewed(), &defect).unwrap(),
        State::Rejected(RejectReason::Defect)
    );
}

#[test]
fn a_polarized_item_is_recovered_only_by_appeal() {
    let base = ItemVerdicts {
        gate: GateOutcome::AppealEligible,
        ..passing()
    };
    // No appeal: the window closes and it is rejected for polarization.
    assert_eq!(
        run_item(
            reviewed(),
            &ItemVerdicts {
                appealed: false,
                ..base
            }
        )
        .unwrap(),
        State::Rejected(RejectReason::Polarized)
    );
    // Appeal, then the evidence vindicates it.
    assert_eq!(
        run_item(
            reviewed(),
            &ItemVerdicts {
                appealed: true,
                ..base
            }
        )
        .unwrap(),
        State::ActivePool
    );
}

#[test]
fn a_band_item_advances_only_when_the_d26_re_decision_passes() {
    let base = ItemVerdicts {
        gate: GateOutcome::SupplementaryReview,
        ..passing()
    };
    // The D26 re-decision passes (re-fit S_j ≥ τ): it enters the pilot and reaches the pool.
    assert_eq!(
        run_item(
            reviewed(),
            &ItemVerdicts {
                band_outcome: GateOutcome::Pass,
                ..base
            }
        )
        .unwrap(),
        State::ActivePool
    );
    // The re-decision fails as a defect: a defined borderline reject (no dead end, T10/T30).
    assert_eq!(
        run_item(
            reviewed(),
            &ItemVerdicts {
                band_outcome: GateOutcome::Reject,
                ..base
            }
        )
        .unwrap(),
        State::Rejected(RejectReason::Borderline)
    );
    // The re-decision fails as a polarized item (T59): the appeal channel stays open — an
    // appeal carries it to the pilot and the pool, no appeal is a polarization reject.
    assert_eq!(
        run_item(
            reviewed(),
            &ItemVerdicts {
                band_outcome: GateOutcome::AppealEligible,
                appealed: true,
                ..base
            }
        )
        .unwrap(),
        State::ActivePool
    );
    assert_eq!(
        run_item(
            reviewed(),
            &ItemVerdicts {
                band_outcome: GateOutcome::AppealEligible,
                appealed: false,
                ..base
            }
        )
        .unwrap(),
        State::Rejected(RejectReason::Polarized)
    );
    // A second band is not an outcome of a re-decision.
    assert_eq!(
        run_item(
            reviewed(),
            &ItemVerdicts {
                band_outcome: GateOutcome::SupplementaryReview,
                ..base
            }
        ),
        Err(Invalid::UnexpectedEvent)
    );
}

#[test]
fn a_standing_short_of_the_rows_is_refused_before_the_fit() {
    // T62: the weights come from the standings, one per reviewer row; a mismatch is a
    // `RatingsError`, not a panic inside the fit.
    let rows = vec![vec![0.5; 3]; 4];
    let mask = vec![vec![true; 3]; 4];
    let short = vec![ReviewerStanding::founder(); 3];
    assert_eq!(
        weighted_ratings(&rows, &mask, &short, 1.0).map(|_| ()),
        Err(RatingsError::WeightCount {
            expected: 4,
            found: 3
        })
    );
    let full = vec![ReviewerStanding::founder(); 4];
    assert!(weighted_ratings(&rows, &mask, &full, 1.0).is_ok());
}
