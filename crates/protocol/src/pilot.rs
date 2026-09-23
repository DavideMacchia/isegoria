//! [6]/[7] Two-stage pilot (`docs/05`, `docs/01` D11). Respondents are the scarce
//! resource: a cheap first stage kills broken and non-discriminating items; only
//! survivors reach the large second stage, which runs DIF in batches (`docs/02` §B).
//!
//! Two floors are load-bearing (`docs/08` INV-8, PROTO-006, G-15): a DIF stage is never
//! run on a single item — one item cannot reveal latent bias, and isolating an item is
//! how an adversary would probe it — and each stage needs enough distinct respondents to
//! estimate its statistic (`docs/02` §B.6). The per-item math below stays pure; the
//! **batch-admission gates** [`screen`] / [`dif_batch`] enforce the floors and are what a
//! caller uses. `K_MIN` is shared with [`crate::lifecycle`].

use crate::lifecycle::K_MIN;
#[cfg(feature = "calibration")]
use scoring::dif::{logistic_dif, BETA2_MAX};
use scoring::irt::{fit_2pl_item, point_biserial, A_MIN, R_PBIS_MIN};
#[cfg(feature = "calibration")]
use scoring::LogisticFit;

/// Stage-1 distinct-respondent floor (`docs/02` §B.6: the classic discrimination screen).
pub const N1_MIN: usize = 300;
/// Stage-2 (Variant-1, attribute DIF) respondent floor (`docs/02` §B.6).
pub const N2_MIN: usize = 1500;

/// Why a pilot batch is not admissible (`docs/08` INV-8, §B.6 sample sizes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PilotError {
    /// Fewer distinct respondents than the stage requires.
    NotEnoughRespondents { have: usize, need: usize },
    /// A DIF batch below `K_MIN` items — a single item cannot reveal latent bias (INV-8).
    BatchTooSmall { items: usize },
}

/// INV-8 batch admission for a DIF stage: never a single item, and enough respondents.
/// `n_min` is the stage's respondent floor (`N2_MIN` here, `N_LATENT_MIN` for the
/// production latent re-check).
pub fn admit_dif_batch(
    n_items: usize,
    n_respondents: usize,
    n_min: usize,
) -> Result<(), PilotError> {
    if n_items < K_MIN {
        return Err(PilotError::BatchTooSmall { items: n_items });
    }
    if n_respondents < n_min {
        return Err(PilotError::NotEnoughRespondents {
            have: n_respondents,
            need: n_min,
        });
    }
    Ok(())
}

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

/// Stage-1 admission gate: runs [`stage1_screen`] only if the sample meets the
/// distinct-respondent floor `N1_MIN` (`docs/08` §B.6). `theta.len()` is the sample size.
pub fn screen(theta: &[f64], item_responses: &[Vec<f64>]) -> Result<Vec<bool>, PilotError> {
    if theta.len() < N1_MIN {
        return Err(PilotError::NotEnoughRespondents {
            have: theta.len(),
            need: N1_MIN,
        });
    }
    Ok(stage1_screen(theta, item_responses))
}

/// Stage-2 admission gate: runs [`stage2_dif`] only on a batch of at least `K_MIN` items
/// (INV-8) with at least `N2_MIN` respondents. A batch of one is rejected (AT-PRO-02).
#[cfg(feature = "calibration")]
pub fn dif_batch(
    theta: &[f64],
    group: &[f64],
    item_responses: &[Vec<f64>],
) -> Result<Vec<DifVerdict>, PilotError> {
    admit_dif_batch(item_responses.len(), theta.len(), N2_MIN)?;
    Ok(stage2_dif(theta, group, item_responses))
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
