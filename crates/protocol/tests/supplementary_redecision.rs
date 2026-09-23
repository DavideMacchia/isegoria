//! D26 supplementary re-decision (docs/01 D26, docs/08 PROTO-012/PROTO-008, T10/T30): a
//! borderline (band) item is re-decided by re-running bridging and comparing `b_j` to the
//! plain threshold τ — a bridging decision over the latent axis, NOT a weighted vote. This
//! replaces the retired `aggregate` tie-break, which advanced even polarized items the
//! bridging model itself rejects (AT-PRO-03).

use protocol::gate::{bridging_gate, supplementary_review, GateOutcome};
use protocol::lifecycle::{step, Event, RejectReason, State};
use scoring::bridging::{bridge_scores, fit, BridgingParams, Ratings};
use std::fs;
use std::path::PathBuf;

const TAU: f64 = 0.08;
const EPS: f64 = 0.008;
const APPEAL_THRESHOLD: f64 = 0.5;

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

#[test]
fn at_pro_03_the_band_is_re_decided_by_bridging_not_by_a_vote() {
    let ratings = ratings();
    let params = BridgingParams::default();
    let bridge = bridge_scores(&ratings, &params, 10, 0.85);
    let f = fit(&ratings, &params);

    // The uncertainty band on these fixtures.
    let band: Vec<usize> = (0..bridge.len())
        .filter(|&j| {
            matches!(
                bridging_gate(bridge[j], f.f_j[j], TAU, EPS, APPEAL_THRESHOLD),
                GateOutcome::SupplementaryReview
            )
        })
        .collect();
    assert_eq!(band, vec![1, 5, 6], "the bridging band on these fixtures");

    // A genuine near-threshold quality item is carried up by the re-decision.
    assert_eq!(
        supplementary_review(&ratings, &params, 6, TAU),
        GateOutcome::Pass,
        "item 6 (quality item near the threshold) passes the re-decision"
    );

    // The partisan items (07, 08) — which the retired weighted-mean tie-break advanced
    // (unit-weight means 0.59 / 0.71 ≥ 0.5) — are NOT passed here: bridging gives them a
    // `b_j` far below τ, so a larger camp does not carry a polarized item (docs/01 D2).
    for j in [7usize, 8] {
        assert_eq!(
            supplementary_review(&ratings, &params, j, TAU),
            GateOutcome::Reject,
            "polarized item {j} must not be carried by the larger camp"
        );
    }
}

#[test]
fn a_borderline_item_reaches_a_defined_terminal() {
    // AT-PRO-03: from `SupplementaryReview` the D26 re-decision gives a defined outcome —
    // pilot entry when it passes, a borderline reject when it does not — never a dead end.
    assert_eq!(
        step(State::SupplementaryReview, Event::Resolve { passed: true }).unwrap(),
        State::Pilot1 { appealed: false }
    );
    assert_eq!(
        step(State::SupplementaryReview, Event::Resolve { passed: false }).unwrap(),
        State::Rejected(RejectReason::Borderline)
    );
}
