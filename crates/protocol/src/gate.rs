//! Bridging gate and appeal (`docs/05` [5]/[5b], `docs/02` §A.3).

use scoring::bridging::{fit, side_balanced, BridgingParams, Ratings, RatingsError};

/// Provisional gate parameters (`docs/02` §A.3 "Initial parameters", calibrated by T25):
/// the threshold on the side-balanced score, the half-width of its uncertainty band, and
/// the side gap at which a rejection counts as polarized, hence appealable (`docs/05` [5b]).
pub const TAU: f64 = 0.80;
pub const EPS: f64 = 0.02;
pub const APPEAL_GAP: f64 = 0.25;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateOutcome {
    Pass,
    SupplementaryReview,
    /// Rejected for polarization: may appeal to the pilot.
    AppealEligible,
    /// Rejected for defect: no appeal.
    Reject,
}

/// Pass at or above `tau + eps`, supplementary review inside the band, below it appealable
/// iff `gap >= appeal_gap` (`docs/05` [5b]).
pub fn bridging_gate(score: f64, gap: f64, tau: f64, eps: f64, appeal_gap: f64) -> GateOutcome {
    if score >= tau + eps {
        GateOutcome::Pass
    } else if score >= tau - eps {
        GateOutcome::SupplementaryReview
    } else if gap >= appeal_gap {
        GateOutcome::AppealEligible
    } else {
        GateOutcome::Reject
    }
}

/// D26 re-decision of a band item: a fresh bridging fit over the panel expanded with the
/// extra reviewers, its score read against the plain `tau`; a failing item follows the
/// below-band rule of [`bridging_gate`]. Malformed ratings or an item past the batch error.
pub fn supplementary_review(
    ratings: &Ratings,
    params: &BridgingParams,
    item: usize,
    tau: f64,
    appeal_gap: f64,
) -> Result<GateOutcome, RatingsError> {
    if item >= ratings.m {
        return Err(RatingsError::ItemOutOfRange {
            j: item,
            m: ratings.m,
        });
    }
    let sides = side_balanced(&fit(ratings, params)?);
    Ok(if sides.score[item] >= tau {
        GateOutcome::Pass
    } else if sides.gap[item] >= appeal_gap {
        GateOutcome::AppealEligible
    } else {
        GateOutcome::Reject
    })
}
