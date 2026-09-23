//! The epoch orchestrator (`protocol::orchestrator`): the two audit gaps whose core
//! already existed but were not wired at the epoch boundary.
//!
//! - **T5 / BRIDGE-007:** reviewer weights derived from prior-epoch standing flow into
//!   `bridging::fit`, so lower reputation means less influence on `b_j`.
//! - **T12 / §9.1:** an item's stage-to-stage fate is decided by `lifecycle::step`,
//!   driven by the epoch's gate and pilot verdicts, not by ad-hoc caller logic.

use protocol::gate::GateOutcome;
use protocol::lifecycle::{RejectReason, State};
use protocol::orchestrator::{
    bridging_weights, run_item, weighted_ratings, ItemVerdicts, ReviewerStanding,
};
use protocol::probation::N_PROBATION;
use scoring::bridging::{fit, BridgingParams};

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
        fit(&weighted_ratings(&rows, &mask, standings, 1.0), &params).b_j[t]
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
        item: network::cid::cid(b"item"),
        gate: GateOutcome::Pass,
        appealed: false,
        band_advances: false,
        enough_respondents: true,
        screen_passed: true,
        dif_passed: true,
        pilot2_batch_size: 8,
    }
}

#[test]
fn a_clean_item_reaches_the_pool() {
    assert_eq!(run_item(&passing()).unwrap(), State::ActivePool);
}

#[test]
fn the_screen_and_the_dif_stage_each_stop_an_item() {
    let screened = ItemVerdicts {
        screen_passed: false,
        ..passing()
    };
    assert_eq!(
        run_item(&screened).unwrap(),
        State::Rejected(RejectReason::Screen)
    );
    let dif = ItemVerdicts {
        dif_passed: false,
        ..passing()
    };
    assert_eq!(run_item(&dif).unwrap(), State::Rejected(RejectReason::Dif));
}

#[test]
fn a_defect_reject_never_enters_the_pilot() {
    let defect = ItemVerdicts {
        gate: GateOutcome::Reject,
        ..passing()
    };
    assert_eq!(
        run_item(&defect).unwrap(),
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
        run_item(&ItemVerdicts {
            appealed: false,
            ..base
        })
        .unwrap(),
        State::Rejected(RejectReason::Polarized)
    );
    // Appeal, then the evidence vindicates it.
    assert_eq!(
        run_item(&ItemVerdicts {
            appealed: true,
            ..base
        })
        .unwrap(),
        State::ActivePool
    );
}

#[test]
fn a_band_item_advances_only_when_the_tie_break_carries_it() {
    let base = ItemVerdicts {
        gate: GateOutcome::SupplementaryReview,
        ..passing()
    };
    // The provisional tie-break carries it: it enters the pilot and (passing) reaches the pool.
    assert_eq!(
        run_item(&ItemVerdicts {
            band_advances: true,
            ..base
        })
        .unwrap(),
        State::ActivePool
    );
    // The tie-break does not carry it: it rests in the band, which has no forward
    // transition (T30/PROTO-008), so it does not reach the pool.
    assert_eq!(
        run_item(&ItemVerdicts {
            band_advances: false,
            ..base
        })
        .unwrap(),
        State::SupplementaryReview
    );
}
