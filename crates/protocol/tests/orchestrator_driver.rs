//! The epoch orchestrator (`protocol::orchestrator`): reviewer weights from prior-epoch
//! standing (`docs/08` BRIDGE-007) and item fates decided by `lifecycle::step` (§9.1).

use identity::nym::Nym;
use network::cid::{cid, Cid};
use protocol::gate::GateOutcome;
use protocol::lifecycle::{deposit, step, Event, Invalid, RejectReason, State};
use protocol::orchestrator::{
    bridging_weights, epoch_weight_cap, review_round, run_item, weighted_ratings, ExtraRound,
    ItemVerdicts, Judgment, ReviewerStanding,
};
use protocol::probation::N_PROBATION;
use scoring::bridging::{fit, side_balanced, BridgingParams, RatingsError};

// ------------------------------- weights from standing -------------------------------

/// The bridging weight is the review vote weight: 0 on probation, 1 for a bootstrap
/// founder, the capped odds weight of the skill once established (D33).
#[test]
fn bridging_weights_map_probation_founder_established() {
    let odds = |s: f64| (35.0 * s * N_PROBATION as f64 / (N_PROBATION as f64 + 100.0)).exp();
    let prev = [
        ReviewerStanding::founder(),
        ReviewerStanding::established(-0.05), // below the crowd: less than 1
        ReviewerStanding::established(0.5),   // far above the crowd: capped
        ReviewerStanding {
            is_founder: false,
            judgments_with_outcome: 0,
            reviews: 0,
            skill: 0.9,
        },
        // a founder past the probation threshold is established, not seeded
        ReviewerStanding {
            is_founder: true,
            judgments_with_outcome: N_PROBATION,
            reviews: N_PROBATION,
            skill: 0.01,
        },
    ];
    let w = bridging_weights(&prev, 2.0);
    assert_eq!(w, vec![1.0, odds(-0.05), 2.0, 0.0, odds(0.01)]);
    assert!(w[1] < 1.0 && w[4] > 1.0);
}

/// The epoch's cap is `3 × median` of the weights that count — founders and established
/// reviewers, not probationers — and it binds on an outlier (AT-REP-04, D33).
#[test]
fn the_epoch_cap_is_three_times_the_median_of_the_counted_weights() {
    let mut prev = vec![ReviewerStanding::founder(); 8];
    prev.push(ReviewerStanding {
        is_founder: false,
        judgments_with_outcome: 400,
        reviews: 400,
        skill: 0.1, // exp(35·0.1·0.8) ≈ 16: an outlier
    });
    prev.push(ReviewerStanding {
        is_founder: false,
        judgments_with_outcome: 0,
        reviews: 0,
        skill: 0.9, // probation: not part of the crowd the cap is relative to
    });
    let cap = epoch_weight_cap(&prev);
    assert!((cap - 3.0).abs() < 1e-12, "cap {cap}");
    let w = bridging_weights(&prev, cap);
    assert!(
        (w[8] - 3.0).abs() < 1e-12,
        "the outlier is capped: {}",
        w[8]
    );
    assert_eq!(w[9], 0.0);
    // Nobody carries weight: nothing to cap.
    let fresh = [ReviewerStanding {
        is_founder: false,
        judgments_with_outcome: 0,
        reviews: 0,
        skill: 0.0,
    }];
    assert_eq!(epoch_weight_cap(&fresh), f64::INFINITY);
}

/// A bloc pushing an item up moves its bridge score less when its prior-epoch reputation
/// is low (BRIDGE-007): same reviewer count in both fits, only the standing differs.
#[test]
fn lower_reputation_moves_the_bridge_score_less() {
    let (m, t) = (5usize, 4usize);
    let (honest, bloc) = (30usize, 30usize);

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

    let mut trusted = vec![ReviewerStanding::founder(); honest];
    trusted.extend(std::iter::repeat_n(ReviewerStanding::founder(), bloc));
    let mut discounted = vec![ReviewerStanding::founder(); honest];
    discounted.extend(std::iter::repeat_n(
        ReviewerStanding::established(-0.5),
        bloc,
    ));

    let b_trusted = b_t(&trusted);
    let b_discounted = b_t(&discounted);
    assert!(
        b_discounted < b_trusted,
        "a low-reputation bloc must move b_j less: discounted={b_discounted:.4} trusted={b_trusted:.4}"
    );
}

// ------------------------------- the machine decides -------------------------------

/// Verdicts for a plain (non-band, non-appeal) item that clears both pilot stages.
fn passing() -> ItemVerdicts {
    ItemVerdicts {
        gate: GateOutcome::Pass,
        appealed: false,
        appeal_within_window: true,
        author_reputation: 0.6,
        appeal_floor: 0.4,
        enough_respondents: true,
        screen_passed: true,
        dif_passed: true,
        pilot2_batch_size: 8,
        explored: false,
    }
}

fn item() -> Cid {
    cid(b"item")
}

/// `run_item` for an item that is not in the band: no extra round, no re-decision.
fn run(reviewed: State, v: &ItemVerdicts) -> Result<State, Invalid> {
    run_item(reviewed, v, None, |_| unreachable!("no band item here"))
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

/// A panelist who never judged blocks the epoch, and a repeated nym never forms a panel.
#[test]
fn run_item_refuses_an_incomplete_or_forged_review_round() {
    let p = panel();
    let partial = review_round(admitted(), item(), p.clone(), &judgments(&p[..8])).unwrap();
    assert_eq!(run(partial, &passing()), Err(Invalid::PartialEpoch));

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
    assert_eq!(run(reviewed(), &passing()).unwrap(), State::ActivePool);
}

#[test]
fn the_screen_and_the_dif_stage_each_stop_an_item() {
    let screened = ItemVerdicts {
        screen_passed: false,
        ..passing()
    };
    assert_eq!(
        run(reviewed(), &screened).unwrap(),
        State::Rejected(RejectReason::Screen)
    );
    let dif = ItemVerdicts {
        dif_passed: false,
        ..passing()
    };
    assert_eq!(
        run(reviewed(), &dif).unwrap(),
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
        run(reviewed(), &defect).unwrap(),
        State::Rejected(RejectReason::Defect)
    );
}

#[test]
fn a_polarized_item_is_recovered_only_by_appeal() {
    let base = ItemVerdicts {
        gate: GateOutcome::AppealEligible,
        ..passing()
    };
    assert_eq!(
        run(
            reviewed(),
            &ItemVerdicts {
                appealed: false,
                ..base
            }
        )
        .unwrap(),
        State::Rejected(RejectReason::Polarized)
    );
    assert_eq!(
        run(
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

/// The band's extra round (D26): four reviewers outside the first panel, judging at `prob`.
fn extra(prob: f64) -> ExtraRound {
    let panel: Vec<Nym> = (20..24).map(|i| Nym([i; 32])).collect();
    let judgments = panel
        .iter()
        .map(|&nym| Judgment {
            nym,
            prob,
            nonce: [nym.0[0]; 32],
        })
        .collect();
    ExtraRound { panel, judgments }
}

#[test]
fn a_band_item_advances_only_when_the_d26_re_decision_passes() {
    let base = ItemVerdicts {
        gate: GateOutcome::SupplementaryReview,
        ..passing()
    };
    assert_eq!(
        run_item(reviewed(), &base, Some(&extra(0.9)), |_| GateOutcome::Pass).unwrap(),
        State::ActivePool
    );
    // A defect failure is a borderline reject.
    assert_eq!(
        run_item(reviewed(), &base, Some(&extra(0.2)), |_| {
            GateOutcome::Reject
        })
        .unwrap(),
        State::Rejected(RejectReason::Borderline)
    );
    // A polarized failure keeps the appeal channel open.
    assert_eq!(
        run_item(
            reviewed(),
            &ItemVerdicts {
                appealed: true,
                ..base
            },
            Some(&extra(0.5)),
            |_| GateOutcome::AppealEligible
        )
        .unwrap(),
        State::ActivePool
    );
    assert_eq!(
        run_item(
            reviewed(),
            &ItemVerdicts {
                appealed: false,
                ..base
            },
            Some(&extra(0.5)),
            |_| GateOutcome::AppealEligible
        )
        .unwrap(),
        State::Rejected(RejectReason::Polarized)
    );
    // A second band is not an outcome of a re-decision.
    assert_eq!(
        run_item(reviewed(), &base, Some(&extra(0.5)), |_| {
            GateOutcome::SupplementaryReview
        }),
        Err(Invalid::UnexpectedEvent)
    );
}

/// The re-decision reads exactly the extra round's reveals, and never runs without one.
#[test]
fn the_re_decision_reads_the_extra_round_the_machine_walked() {
    let base = ItemVerdicts {
        gate: GateOutcome::SupplementaryReview,
        ..passing()
    };
    let round = extra(0.35);
    let mut seen: Vec<(Nym, f64)> = Vec::new();
    let terminal = run_item(reviewed(), &base, Some(&round), |reveals| {
        seen = reveals.to_vec();
        GateOutcome::Reject
    })
    .unwrap();
    assert_eq!(terminal, State::Rejected(RejectReason::Borderline));
    let expected: Vec<(Nym, f64)> = round.judgments.iter().map(|j| (j.nym, j.prob)).collect();
    assert_eq!(seen, expected);

    // No extra round: the re-decision is never asked.
    assert_eq!(
        run_item(reviewed(), &base, None, |_| unreachable!(
            "resolved without a round"
        )),
        Err(Invalid::NoExtraPanel)
    );
    // An extra panel that repeats a first-round reviewer is not extra evidence.
    let mut overlapping = extra(0.9);
    overlapping.panel[0] = panel()[0];
    overlapping.judgments[0].nym = panel()[0];
    assert_eq!(
        run_item(reviewed(), &base, Some(&overlapping), |_| GateOutcome::Pass),
        Err(Invalid::DuplicatePanelist)
    );
    // A partial extra round is not re-decided.
    let mut partial = extra(0.9);
    partial.judgments.pop();
    assert_eq!(
        run_item(reviewed(), &base, Some(&partial), |_| GateOutcome::Pass),
        Err(Invalid::PartialEpoch)
    );
    // An item not in the band never asks for a re-decision, with or without a round.
    assert_eq!(
        run_item(reviewed(), &passing(), Some(&round), |_| unreachable!()).unwrap(),
        State::ActivePool
    );
}

#[test]
fn a_standing_short_of_the_rows_is_refused_before_the_fit() {
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
