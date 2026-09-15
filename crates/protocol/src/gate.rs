//! [5]/[5b] Bridging gate and appeal (`docs/05`, `docs/02` §A.3, D8). The score
//! clusters near the threshold, so a hard cut is a blade: items inside a band go to
//! supplementary review. An item rejected for POLARIZATION (high |f_j|), not defect,
//! is eligible to appeal directly to the pilot.

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
