//! [6]/[7] Two-stage pilot (`docs/05`, `docs/01` D11). Respondents are the scarce
//! resource: a cheap first stage kills broken and non-discriminating items; only
//! survivors reach the large second stage, which runs DIF in batches (`docs/02` §B).

#[cfg(feature = "calibration")]
use scoring::dif::{logistic_dif, BETA2_MAX};
use scoring::irt::{fit_2pl_item, point_biserial, A_MIN, R_PBIS_MIN};
#[cfg(feature = "calibration")]
use scoring::LogisticFit;

/// Outcome of the attribute-based DIF screen for one item (calibration-only, D20).
#[cfg(feature = "calibration")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DifVerdict {
    /// No uniform DIF beyond the threshold.
    Pass,
    /// Uniform DIF beyond the threshold.
    Reject,
    /// The fit is separated, so `β₂` is undetermined (docs/08 AT-DIF-06).
    Undetermined,
}

/// Stage 1 screen (~300 respondents): keep items that discriminate. A negative
/// point-biserial signals a wrong answer key.
pub fn stage1_screen(theta: &[f64], item_responses: &[Vec<f64>]) -> Vec<bool> {
    item_responses
        .iter()
        .map(|item| {
            let rp = point_biserial(item, theta);
            let (a, _b) = fit_2pl_item(theta, item);
            rp >= R_PBIS_MIN && a >= A_MIN
        })
        .collect()
}

/// Stage 2 (~1500 respondents), run on the surviving batch: reject items with uniform
/// DIF against the axis. Variant 1, calibration-only (`docs/01` D20); production uses
/// [`crate::revalidation::revalidate_pool_latent`] (Variant 2).
#[cfg(feature = "calibration")]
pub fn stage2_dif(theta: &[f64], group: &[f64], item_responses: &[Vec<f64>]) -> Vec<DifVerdict> {
    item_responses
        .iter()
        .map(|item| {
            let c = logistic_dif(item, theta, group);
            match c.status {
                LogisticFit::Separated => DifVerdict::Undetermined,
                _ if c.beta2.abs() <= BETA2_MAX => DifVerdict::Pass,
                _ => DifVerdict::Reject,
            }
        })
        .collect()
}
