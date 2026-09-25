//! [5]/[5b] Bridging gate and appeal (`docs/05`, `docs/02` §A.3, D8, D32). The gate reads
//! the side-balanced bridge score (`bridging::side_balanced`, T49) against an absolute
//! threshold on the probability scale. Scores cluster near the threshold, so a hard cut
//! is a blade: items inside a band go to supplementary review. An item rejected for
//! POLARIZATION — a wide gap between the two sides' predicted approval — not for defect,
//! is eligible to appeal directly to the pilot.

use scoring::bridging::{fit, side_balanced, BridgingParams, Ratings, RatingsError};

/// Provisional gate parameters (`docs/02` §A.3 and "Initial parameters", D32), to be
/// calibrated by T25; until then the reference implementation and its tests use them.
///
/// The threshold on the side-balanced score, on the probability scale.
pub const TAU: f64 = 0.80;
/// Half-width of the uncertainty band around `TAU`: about three times the bootstrap
/// spread of the score on the reference fixtures (≤ 0.006).
pub const EPS: f64 = 0.02;
/// The side gap at and above which a rejected item counts as polarized, hence appealable
/// (`docs/05` [5b]): on the reference fixtures the mildly partisan item sits at 0.30, the
/// consensus items below 0.02.
pub const APPEAL_GAP: f64 = 0.25;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateOutcome {
    Pass,
    SupplementaryReview,
    /// rejected for polarization → may appeal to the evidence filter
    AppealEligible,
    /// rejected for defect → no appeal
    Reject,
}

/// - `score` at or above `tau + eps` → pass
/// - within `[tau − eps, tau + eps)` → supplementary review
/// - below `tau − eps`: appealable if `gap ≥ appeal_gap` — the two sides disagree, so the
///   item was rejected for polarization (`docs/05` [5b]) — else rejected as a defect
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

/// D26 borderline re-decision (`docs/01` D26 and its amendment, `docs/08` PROTO-008,
/// roadmap T10/T30/T59).
///
/// An item whose robust bootstrap-min score landed in the uncertainty band is re-decided
/// by **re-running the bridging fit** over the panel — expanded with the extra reviewers
/// D26 calls for (T60), folded into `ratings` before this call — and comparing its
/// side-balanced score `S_j` to the plain threshold `tau`, without the band. This is a
/// bridging decision over the latent axis, **not** a weighted vote of the same ratings
/// (the retired `aggregate::resolve_band`), so a larger camp does not carry a polarized
/// item: bridging gives such an item a wide side gap and an `S_j` below `tau`.
///
/// An item that fails follows the below-band rule of [`bridging_gate`] (D26 amendment,
/// T59): with a side gap of at least `appeal_gap` it was rejected for polarization and is
/// `AppealEligible`; otherwise it is a `Reject` (a borderline reject in the lifecycle).
/// The outcome is never `SupplementaryReview`: there is no second band.
///
/// Malformed ratings, or an item index past the batch, are an error (T62), not a panic.
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
