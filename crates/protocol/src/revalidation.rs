//! Periodic pool re-validation (`docs/05` [8], `docs/02` §B.3 multi-axis). Scattered
//! distortions add up over time and an item accepted today can develop DIF as the
//! context shifts, so the whole active pool is re-checked periodically. DIF is sought
//! on more than one latent axis (political and, e.g., socio-economic), and — where
//! the axis is unknown — via the latent-class mixture. The result feeds
//! [`crate::exposure::should_retire`] as `ItemHealth`.

use crate::exposure::{should_retire, ExposureLedger, ItemHealth, RetirementReason};
use network::cid::Cid;
#[cfg(feature = "calibration")]
use scoring::dif::{logistic_dif, BETA2_MAX};
use scoring::dif::{mixture_dif, MIXTURE_DIF_MAX};

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
/// each item whose latent-class difficulty shift exceeds the threshold. Only
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
    let res = mixture_dif(theta, responses, m, seed);
    res.delta.iter().map(|d| *d > MIXTURE_DIF_MAX).collect()
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
