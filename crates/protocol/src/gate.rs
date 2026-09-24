//! [5]/[5b] Bridging gate and appeal (`docs/05`, `docs/02` §A.3, D8). The score
//! clusters near the threshold, so a hard cut is a blade: items inside a band go to
//! supplementary review. An item rejected for POLARIZATION (high |f_j|), not defect,
//! is eligible to appeal directly to the pilot.

use scoring::bridging::{fit, BridgingParams, Ratings, RatingsError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateOutcome {
    Pass,
    SupplementaryReview,
    /// rejected for polarization → may appeal to the evidence filter
    AppealEligible,
    /// rejected for defect → no appeal
    Reject,
}

/// - `b_j` above `tau + eps` → pass
/// - within `[tau - eps, tau + eps]` → supplementary review
/// - below `tau - eps`: appealable if `|f_j| ≥ appeal_threshold`, else rejected
pub fn bridging_gate(b_j: f64, f_j: f64, tau: f64, eps: f64, appeal_threshold: f64) -> GateOutcome {
    if b_j >= tau + eps {
        GateOutcome::Pass
    } else if b_j >= tau - eps {
        GateOutcome::SupplementaryReview
    } else if f_j.abs() >= appeal_threshold {
        GateOutcome::AppealEligible
    } else {
        GateOutcome::Reject
    }
}

/// D26 borderline re-decision (`docs/01` D26, `docs/08` PROTO-012, roadmap T10/T30).
///
/// An item whose robust bootstrap-min score landed in the uncertainty band is re-decided
/// by **re-running the bridging fit** over the panel — expanded with the extra reviewers
/// D26 calls for, folded into `ratings` before this call — and comparing its bridge score
/// `b_j` to the plain threshold `tau`. This is a bridging decision over the latent axis,
/// **not** a weighted vote of the same ratings (the retired `aggregate::resolve_band`), so
/// a larger camp does not carry a polarized item: bridging gives such an item a high `|f_j|`
/// and a `b_j` below `tau`.
///
/// Malformed ratings, or an item index past the batch, are an error (T62), not a panic.
pub fn supplementary_review(
    ratings: &Ratings,
    params: &BridgingParams,
    item: usize,
    tau: f64,
) -> Result<GateOutcome, RatingsError> {
    if item >= ratings.m {
        return Err(RatingsError::ItemOutOfRange {
            j: item,
            m: ratings.m,
        });
    }
    Ok(if fit(ratings, params)?.b_j[item] >= tau {
        GateOutcome::Pass
    } else {
        GateOutcome::Reject
    })
}

/// Appeal to the evidence filter (`docs/05` [5b], `docs/02` D8): the author stakes
/// reputation to skip review and go straight to the pilot; the stake is refunded
/// (with a gain) if the psychometrics promote the item, and lost otherwise.
pub fn settle_appeal(reputation: f64, stake: f64, promoted: bool, gain: f64) -> f64 {
    if promoted {
        reputation + gain
    } else {
        (reputation - stake).max(0.0)
    }
}
