//! Periodic pool re-validation (`docs/05` [8], `docs/02` §B.3 multi-axis): the active
//! pool is re-checked for DIF on more than one latent axis, or via the latent-class
//! mixture where the axis is unknown. Feeds [`crate::exposure::should_retire`].

use crate::admission::NullifierSet;
use crate::exposure::{should_retire, ExposureLedger, ItemHealth, RetirementReason};
use crate::pilot::{admit_anchors, admit_dif_batch, PilotError};
use network::cid::Cid;
#[cfg(feature = "calibration")]
use scoring::dif::{logistic_dif, BETA2_MAX};
use scoring::dif::{mixture_dif, MixtureDif, MIXTURE_DIF_MAX};
use scoring::latent::{latent_dif, LatentDif};
use scoring::Convergence;

pub const N_LATENT_MIN: usize = 3000;

#[cfg(feature = "calibration")]
fn column(responses: &[Vec<f64>], j: usize) -> Vec<f64> {
    responses.iter().map(|row| row[j]).collect()
}

/// Multi-axis DIF re-check over the pool: flags an item with uniform DIF on ANY axis
/// (catching bias neutral on one axis but not another). Variant 1, calibration-only
/// (D20); production uses [`revalidate_pool_latent`].
#[cfg(feature = "calibration")]
pub fn revalidate_pool(
    theta: &[f64],
    axes: &[Vec<f64>],
    responses: &[Vec<f64>],
) -> Vec<ItemHealth> {
    let m = if responses.is_empty() {
        0
    } else {
        responses[0].len()
    };
    (0..m)
        .map(|j| {
            let item = column(responses, j);
            let emerging = axes
                .iter()
                .any(|axis| logistic_dif(&item, theta, axis).beta2.abs() > BETA2_MAX);
            ItemHealth {
                emerging_dif: emerging,
                ..Default::default()
            }
        })
        .collect()
}

/// Latent-class re-check on a θ proxy (`docs/02` §B.3, Variant 2): flags an item whose
/// latent-class difficulty gap exceeds the threshold. Retired from the production path
/// (D37, T54; error in the proxy creates classes that do not exist); kept for fixtures.
pub fn revalidate_pool_latent(theta: &[f64], responses: &[Vec<f64>], seed: u64) -> Vec<bool> {
    let m = if responses.is_empty() {
        0
    } else {
        responses[0].len()
    };
    if m == 0 {
        return Vec::new();
    }
    latent_flags(&mixture_dif(theta, responses, m, seed))
}

/// Per-item verdict of a proxy-θ mixture fit (T35): flags only a converged, multi-class fit.
pub fn latent_flags(res: &MixtureDif) -> Vec<bool> {
    let trustworthy = res.status == Convergence::Converged && res.classes >= 2;
    res.dif
        .iter()
        .map(|&d| trustworthy && d > MIXTURE_DIF_MAX)
        .collect()
}

pub fn target_flags(res: &LatentDif) -> Vec<bool> {
    res.flags(MIXTURE_DIF_MAX)
}

/// Batch-admission gate for the production latent re-check (`docs/08` INV-8, D37): a
/// batch of at least `K_MIN` items, `N_LATENT_MIN` respondents, reliable anchors
/// (`pilot::admit_anchors`); the target model integrates θ out via the anchors (T54).
pub fn revalidate_batch_latent(
    respondents: &NullifierSet,
    anchors: &[Vec<f64>],
    responses: &[Vec<f64>],
    seed: u64,
) -> Result<Vec<bool>, PilotError> {
    latent_batch(respondents, anchors, responses, seed).map(|fit| target_flags(&fit))
}

/// The fit [`revalidate_batch_latent`] flags from, behind the same gates: the contested pool
/// records its curves (`scoring::dtf::ClassCurves::of`, D38).
pub fn latent_batch(
    respondents: &NullifierSet,
    anchors: &[Vec<f64>],
    responses: &[Vec<f64>],
    seed: u64,
) -> Result<LatentDif, PilotError> {
    let n = respondents.len();
    let m = responses.first().map_or(0, |row| row.len());
    admit_dif_batch(m, n, N_LATENT_MIN)?;
    if responses.len() != n {
        return Err(PilotError::RowCountMismatch {
            rows: responses.len(),
            respondents: n,
        });
    }
    if let Some(row) = responses.iter().find(|row| row.len() != m) {
        return Err(PilotError::RowCountMismatch {
            rows: row.len(),
            respondents: m,
        });
    }
    if anchors.len() != n {
        return Err(PilotError::RowCountMismatch {
            rows: anchors.len(),
            respondents: n,
        });
    }
    let k_anchor = anchors.first().map_or(0, Vec::len);
    if let Some(row) = anchors.iter().find(|row| row.len() != k_anchor) {
        return Err(PilotError::RowCountMismatch {
            rows: row.len(),
            respondents: k_anchor,
        });
    }
    admit_anchors(anchors)?;
    Ok(latent_dif(anchors, responses, seed))
}

pub fn items_to_retire(
    items: &[Cid],
    health: &[ItemHealth],
    exposure: &ExposureLedger,
    limit: usize,
) -> Vec<(Cid, RetirementReason)> {
    items
        .iter()
        .enumerate()
        .filter_map(|(i, &id)| {
            let h = health.get(i).copied().unwrap_or_default();
            should_retire(exposure.count(&id), limit, h).map(|reason| (id, reason))
        })
        .collect()
}
