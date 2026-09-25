//! D26 supplementary re-decision (docs/01 D26, docs/08 PROTO-012/PROTO-008, T10/T30): a
//! borderline (band) item is re-decided by re-running bridging and comparing `b_j` to the
//! plain threshold τ — a bridging decision over the latent axis, NOT a weighted vote. This
//! replaces the retired `aggregate` tie-break, which advanced even polarized items the
//! bridging model itself rejects (AT-PRO-03).

use protocol::gate::{bridging_gate, supplementary_review, GateOutcome, APPEAL_GAP, EPS, TAU};
use protocol::lifecycle::{step, Event, RejectReason, State};
use scoring::bridging::{
    bridge_scores, fit, side_balanced, BridgingParams, Obs, Ratings, RatingsError,
};
use std::fs;
use std::path::PathBuf;

fn read_matrix(name: &str) -> Vec<Vec<f64>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../scoring/tests/fixtures")
        .join(name);
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.split(',').map(|c| c.trim().parse().unwrap()).collect())
        .collect()
}

fn ratings() -> Ratings {
    let r = read_matrix("R.csv");
    let mask: Vec<Vec<bool>> = read_matrix("mask.csv")
        .iter()
        .map(|row| row.iter().map(|&v| v != 0.0).collect())
        .collect();
    Ratings::from_dense(&r, &mask)
}

/// The fixture plus an eleventh, borderline item: approval about τ from every reviewer,
/// no lean, nine in ten cells observed. On the provisional gate (τ = 0.80 ± 0.02) no
/// fixture item is borderline — the consensus items score 0.83–0.86, the partisan ones
/// 0.52–0.57 — so the band is exercised on this one.
fn ratings_with_a_borderline_item() -> (Ratings, usize) {
    let base = ratings();
    let j = base.m;
    let mut obs = base.obs.clone();
    for u in 0..base.n {
        if u % 10 != 3 {
            let wobble = (((u * 7) % 11) as f64 - 5.0) * 0.006;
            obs.push(Obs {
                u,
                j,
                r: TAU + wobble,
            });
        }
    }
    (
        Ratings {
            n: base.n,
            m: base.m + 1,
            obs,
            weights: base.weights.clone(),
        },
        j,
    )
}

#[test]
fn at_pro_03_the_band_is_re_decided_by_bridging_not_by_a_vote() {
    let (ratings, borderline) = ratings_with_a_borderline_item();
    let params = BridgingParams::default();
    let bridge = bridge_scores(&ratings, &params, 10, 0.85).unwrap();

    // The uncertainty band on these fixtures: only the borderline item.
    let band: Vec<usize> = (0..ratings.m)
        .filter(|&j| {
            matches!(
                bridging_gate(bridge.robust[j], bridge.full.gap[j], TAU, EPS, APPEAL_GAP),
                GateOutcome::SupplementaryReview
            )
        })
        .collect();
    assert_eq!(
        band,
        vec![borderline],
        "the bridging band on these fixtures"
    );

    // The re-decision is the full fit's side-balanced score against the plain τ — a
    // bridging decision over the latent axis, whichever way it falls — and below τ the
    // below-band rule (T59): the gap decides between appeal and borderline reject.
    let full = side_balanced(&fit(&ratings, &params).unwrap());
    let expected = if full.score[borderline] >= TAU {
        GateOutcome::Pass
    } else if full.gap[borderline] >= APPEAL_GAP {
        GateOutcome::AppealEligible
    } else {
        GateOutcome::Reject
    };
    assert_eq!(
        supplementary_review(&ratings, &params, borderline, TAU, APPEAL_GAP).unwrap(),
        expected,
        "the re-decision reads S_j = {:.4} against τ",
        full.score[borderline]
    );

    // The partisan items (07, 08) — which the retired weighted-mean tie-break advanced
    // (unit-weight means 0.59 / 0.71 ≥ 0.5) — are NOT passed here: bridging gives them a
    // side-balanced score far below τ, so a larger camp does not carry a polarized item
    // (docs/01 D2, D32).
    for j in [7usize, 8] {
        assert_eq!(
            supplementary_review(&ratings, &params, j, TAU, APPEAL_GAP).unwrap(),
            GateOutcome::AppealEligible,
            "polarized item {j} must not be carried by the larger camp; it keeps the appeal"
        );
    }
}

/// The fixture plus one item rated by everyone: at `approval` on side A (camp A, `f < 0`)
/// and at `approval + lift` on side B.
fn ratings_with_a_leaning_item(approval: f64, lift: f64) -> (Ratings, usize) {
    let base = ratings();
    let true_f = read_matrix("true_f.csv");
    let j = base.m;
    let mut obs = base.obs.clone();
    for (u, f) in true_f.iter().enumerate().take(base.n) {
        let wobble = (((u * 7) % 11) as f64 - 5.0) * 0.004;
        let r = if f[0] > 0.0 {
            approval + lift
        } else {
            approval
        };
        obs.push(Obs {
            u,
            j,
            r: (r + wobble).clamp(0.0, 1.0),
        });
    }
    (
        Ratings {
            n: base.n,
            m: base.m + 1,
            obs,
            weights: base.weights.clone(),
        },
        j,
    )
}

/// T59 (D26 amendment): an item that fails the re-decision follows the below-band rule.
/// Rejected for polarization — one side approves, the other does not — it keeps the
/// appeal channel; rejected as a defect — both sides lukewarm — it is a borderline
/// reject; and an item both sides approve passes.
#[test]
fn a_failing_re_decision_keeps_the_appeal_for_a_polarized_item_only() {
    let params = BridgingParams::default();

    // Polarized: side A at 0.55, side B at 0.95 — S_j ≈ 0.75 < τ, gap ≈ 0.40 ≥ 0.25.
    let (ratings, j) = ratings_with_a_leaning_item(0.55, 0.40);
    let sides = side_balanced(&fit(&ratings, &params).unwrap());
    assert!(
        sides.score[j] < TAU && sides.gap[j] >= APPEAL_GAP,
        "S_j = {:.3}, gap = {:.3}",
        sides.score[j],
        sides.gap[j]
    );
    assert_eq!(
        supplementary_review(&ratings, &params, j, TAU, APPEAL_GAP).unwrap(),
        GateOutcome::AppealEligible
    );

    // A defect: both sides at 0.75 — S_j ≈ 0.75 < τ, gap ≈ 0.
    let (ratings, j) = ratings_with_a_leaning_item(0.75, 0.0);
    let sides = side_balanced(&fit(&ratings, &params).unwrap());
    assert!(
        sides.score[j] < TAU && sides.gap[j] < APPEAL_GAP,
        "S_j = {:.3}, gap = {:.3}",
        sides.score[j],
        sides.gap[j]
    );
    assert_eq!(
        supplementary_review(&ratings, &params, j, TAU, APPEAL_GAP).unwrap(),
        GateOutcome::Reject
    );

    // Approved by both sides: passes.
    let (ratings, j) = ratings_with_a_leaning_item(0.90, 0.0);
    assert_eq!(
        supplementary_review(&ratings, &params, j, TAU, APPEAL_GAP).unwrap(),
        GateOutcome::Pass
    );
}

#[test]
fn a_borderline_item_reaches_a_defined_terminal() {
    // AT-PRO-03: from `SupplementaryReview` the D26 re-decision gives a defined outcome —
    // pilot entry when it passes, a borderline reject when it does not — never a dead end.
    assert_eq!(
        step(
            State::SupplementaryReview,
            Event::Resolve {
                outcome: GateOutcome::Pass
            }
        )
        .unwrap(),
        State::Pilot1 { appealed: false }
    );
    assert_eq!(
        step(
            State::SupplementaryReview,
            Event::Resolve {
                outcome: GateOutcome::Reject
            }
        )
        .unwrap(),
        State::Rejected(RejectReason::Borderline)
    );
    // A polarized item that fails the re-decision keeps the appeal channel (T59).
    assert_eq!(
        step(
            State::SupplementaryReview,
            Event::Resolve {
                outcome: GateOutcome::AppealEligible
            }
        )
        .unwrap(),
        State::AppealEligible
    );
}

#[test]
fn a_re_decision_of_an_item_outside_the_batch_is_an_error_not_a_panic() {
    // T62: the gate refuses an item index past the batch, as it refuses malformed ratings.
    let ratings = ratings();
    let m = ratings.m;
    assert_eq!(
        supplementary_review(&ratings, &BridgingParams::default(), m, TAU, APPEAL_GAP),
        Err(RatingsError::ItemOutOfRange { j: m, m })
    );
}
