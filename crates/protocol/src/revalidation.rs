//! Periodic pool re-validation (`docs/05` [8], `docs/02` §B.3 multi-axis). Scattered
//! distortions add up over time and an item accepted today can develop DIF as the
//! context shifts, so the whole active pool is re-checked periodically. DIF is sought
//! on more than one latent axis (political and, e.g., socio-economic), and — where
//! the axis is unknown — via the latent-class mixture. The result feeds
//! [`crate::exposure::should_retire`] as `ItemHealth`.

use crate::admission::NullifierSet;
use crate::exposure::{should_retire, ExposureLedger, ItemHealth, RetirementReason};
use crate::pilot::{admit_anchors, admit_dif_batch, PilotError};
use network::cid::Cid;
#[cfg(feature = "calibration")]
use scoring::dif::{logistic_dif, BETA2_MAX};
use scoring::dif::{mixture_dif, MixtureDif, MIXTURE_DIF_MAX};
use scoring::irt::theta_from_anchors;
use scoring::Convergence;

/// Respondent floor for the latent-class mixture re-check (`docs/02` §B.6): the
/// anonymity-compatible detector needs the largest sample (~3000), more than the
/// group-signal DIF stage.
pub const N_LATENT_MIN: usize = 3000;

#[cfg(feature = "calibration")]
fn column(responses: &[Vec<f64>], j: usize) -> Vec<f64> {
    responses.iter().map(|row| row[j]).collect()
}

/// Multi-axis DIF re-check over the pool. `responses` is respondents × items; `theta`
/// is ability (from anchors); `axes` is one grouping vector per latent axis. An item
/// is flagged with emerging DIF if it shows uniform DIF on ANY axis — this is what
/// catches the elite blind spot (neutral on the political axis, biased on another).
/// Variant 1, calibration-only (`docs/01` D20); production uses [`revalidate_pool_latent`].
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

/// Latent-class re-check over the whole pool (`docs/02` §B.3, Variant 2): the
/// anonymity-compatible detector for a distorting axis that was never observed. Flags
/// each item whose latent-class difficulty gap exceeds the threshold. Only
/// identifiable in batches, which is what a whole-pool pass provides.
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

/// The per-item verdict of a mixture fit (T35, T40). Per-item gaps are read only off a
/// fit that is evidence of a mixture: if the selected fit did not converge, or the BIC
/// selected a single class, no item is flagged — the gaps are then optimizer output,
/// not an estimate.
pub fn latent_flags(res: &MixtureDif) -> Vec<bool> {
    let trustworthy = res.status == Convergence::Converged && res.classes >= 2;
    res.dif
        .iter()
        .map(|&d| trustworthy && d > MIXTURE_DIF_MAX)
        .collect()
}

/// Batch-admission gate for the production latent re-check (`docs/08` INV-8, §B.6, D37):
/// the pool is re-checked only as a batch of at least `K_MIN` items with at least
/// `N_LATENT_MIN` admitted respondents — persons, counted from the [`NullifierSet`] that
/// `pilot::submit_response` filled, never rows (T65) — whose rows are those respondents
/// one to one, and only when the anchors those respondents answered are reliable enough
/// to stand in for θ (`pilot::admit_anchors`: KR-20 ≥ `KR20_MIN`, T53). θ is computed
/// here, from `anchors` (respondents × anchors, `irt::theta_from_anchors`), so a caller
/// cannot vouch for a proxy the gate has not measured. A single item is rejected
/// (AT-PRO-02) — it cannot reveal latent bias. `responses` is respondents × items. The
/// floors are checked in this order: items, respondents, the rows, the anchors.
pub fn revalidate_batch_latent(
    respondents: &NullifierSet,
    anchors: &[Vec<f64>],
    responses: &[Vec<f64>],
    seed: u64,
) -> Result<Vec<bool>, PilotError> {
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
    let theta = theta_from_anchors(anchors);
    Ok(revalidate_pool_latent(&theta, responses, seed))
}

/// Composes re-validation health with exposure into the retirement list: for each
/// pool item, the reason it should leave (or nothing). Ties the evidence filter back
/// to the item lifecycle.
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
