//! [6]/[7] Two-stage pilot (`docs/05`, `docs/01` D11). Respondents are the scarce
//! resource: a cheap first stage kills broken and non-discriminating items; only
//! survivors reach the large second stage, which runs DIF in batches (`docs/02` §B).
//!
//! Two floors are load-bearing (`docs/08` INV-8, PROTO-006, G-15): a DIF stage is never
//! run on a single item — one item cannot reveal latent bias, and isolating an item is
//! how an adversary would probe it — and each stage needs enough distinct respondents to
//! estimate its statistic (`docs/02` §B.6). The per-item math below stays pure; the
//! **batch-admission gates** [`screen`] / [`dif_batch`] enforce the floors and are what a
//! caller uses. `K_MIN` is shared with [`crate::lifecycle`]. The latent re-check has a
//! third precondition, on the anchors that give the ability proxy: [`admit_anchors`]
//! refuses anchors whose KR-20 on the batch's respondents is below `KR20_MIN` (D37, T53).
//!
//! "Distinct respondents" is enforced on persons, not rows (INV-9, T65): a respondent
//! enters a batch through [`submit_response`], proving a `Respond` nullifier bound to
//! the batch and the epoch, and the floors count the admitted [`NullifierSet`] — so 300
//! answer sheets from one person are one respondent, not three hundred.

use crate::admission::{admit, DuplicateNullifier, NullifierSet, Unproven};
use crate::lifecycle::K_MIN;
use identity::credential::IssuerPublic;
use identity::nullifier::NullifierProof;
use identity::nym::{Nym, Role};
use network::cid::{cid, Cid};
#[cfg(feature = "calibration")]
use scoring::dif::{logistic_dif, BETA2_MAX};
use scoring::irt::{
    fit_2pl_item, kr20, point_biserial, theta_from_anchors, A_MIN, KR20_MIN, R_PBIS_MIN,
};
use scoring::LogisticFit;

/// Stage-1 distinct-respondent floor (`docs/02` §B.6: the classic discrimination screen).
pub const N1_MIN: usize = 300;
/// Stage-2 (Variant-1, attribute DIF) respondent floor (`docs/02` §B.6).
pub const N2_MIN: usize = 1500;

/// Why a pilot batch is not admissible (`docs/08` INV-8, §B.6 sample sizes, D37).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PilotError {
    /// Fewer distinct respondents than the stage requires.
    NotEnoughRespondents { have: usize, need: usize },
    /// A DIF batch below `K_MIN` items — a single item cannot reveal latent bias (INV-8).
    BatchTooSmall { items: usize },
    /// The answer rows are not the admitted respondents one to one (T65): a row without a
    /// respondent, or an item column of another length, is refused.
    RowCountMismatch { rows: usize, respondents: usize },
    /// The anchors' KR-20 on the batch's respondents is below `KR20_MIN` (D37, T53): an
    /// ability proxy that unreliable creates latent classes that do not exist (`docs/08`
    /// DIF-010), so the latent re-check is refused before anything is fitted.
    UnreliableAnchors { kr20: f64, anchors: usize },
}

/// Names a pilot batch by its content: the id of its item cids in canonical (sorted,
/// deduplicated) order, so the same set of items is the same batch however it is listed.
pub fn batch_id(items: &[Cid]) -> Cid {
    let mut sorted: Vec<[u8; 32]> = items.iter().map(|c| c.0).collect();
    sorted.sort_unstable();
    sorted.dedup();
    let mut buf = Vec::with_capacity(32 + 32 * sorted.len());
    buf.extend_from_slice(b"isegoria/pilot-batch/v1");
    buf.extend_from_slice(&(sorted.len() as u64).to_le_bytes());
    for c in &sorted {
        buf.extend_from_slice(c);
    }
    cid(&buf)
}

/// The action context a `Respond` proof is bound to: this batch and epoch (AT-ID-05,
/// T65), as `review::review_context` and `deposit::deposit_context` do for the other
/// roles. A proof made for batch A does not verify on batch B, nor in another epoch.
pub fn response_context(batch: Cid, epoch: u64) -> Vec<u8> {
    let mut ctx = Vec::with_capacity(40);
    ctx.extend_from_slice(&batch.0);
    ctx.extend_from_slice(&epoch.to_le_bytes());
    ctx
}

/// Why an answer sheet was refused at the identity-gated respondent entry point.
#[derive(Debug, PartialEq, Eq)]
pub enum ResponseRejected {
    /// No valid `Respond` nullifier proof for this batch and epoch.
    Unproven(Unproven),
    /// This role-nullifier already answered this batch.
    Duplicate,
}

impl From<Unproven> for ResponseRejected {
    fn from(u: Unproven) -> Self {
        ResponseRejected::Unproven(u)
    }
}

impl From<DuplicateNullifier> for ResponseRejected {
    fn from(_: DuplicateNullifier) -> Self {
        ResponseRejected::Duplicate
    }
}

/// The identity-gated respondent entry point (`docs/08` §9.1 `Pilot1` row, INV-9, T65):
/// the respondent presents a `NullifierProof(Respond)` bound to this batch and epoch, and
/// the proven id is recorded in `respondents`, rejecting a second answer sheet by the same
/// person on this batch (AT-PRO-09). That set is what the floors of [`screen`],
/// [`dif_batch`] and [`crate::revalidation::revalidate_batch_latent`] count. Returns the
/// respondent's proven, non-rotatable id.
pub fn submit_response(
    proof: &NullifierProof,
    issuer: &IssuerPublic,
    batch: Cid,
    epoch: u64,
    respondents: &mut NullifierSet,
) -> Result<Nym, ResponseRejected> {
    let id = admit(
        proof,
        issuer,
        Role::Respond,
        &response_context(batch, epoch),
    )?;
    respondents.spend(id)?;
    Ok(id)
}

/// The rows of a sample must be the admitted respondents one to one (T65): `theta` and
/// every column in `columns` hold exactly one entry per respondent.
pub(crate) fn respondent_rows(
    respondents: &NullifierSet,
    theta: &[f64],
    columns: impl IntoIterator<Item = usize>,
) -> Result<(), PilotError> {
    let n = respondents.len();
    let mismatch = |rows: usize| PilotError::RowCountMismatch {
        rows,
        respondents: n,
    };
    if theta.len() != n {
        return Err(mismatch(theta.len()));
    }
    for len in columns {
        if len != n {
            return Err(mismatch(len));
        }
    }
    Ok(())
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

/// D37 anchor-reliability gate (T53): the ability proxy of a latent re-check comes only
/// from anchors whose KR-20, on these respondents, is at least `KR20_MIN`; below it the
/// batch is refused like the item and respondent floors, because with an unreliable proxy
/// the detector finds classes that do not exist (`docs/08` DIF-010; paper §4.5: at
/// N = 6,000 clean items are flagged from 10 or 20 anchors, KR-20 0.69 / 0.82). `anchors`
/// is respondents × anchor items (0/1), the DIF-free anchors answered alongside the batch
/// (`docs/02` §B.4). Returns θ, the standardized anchor total.
pub fn admit_anchors(anchors: &[Vec<f64>]) -> Result<Vec<f64>, PilotError> {
    let r = kr20(anchors);
    if r < KR20_MIN {
        return Err(PilotError::UnreliableAnchors {
            kr20: r,
            anchors: anchors.first().map_or(0, Vec::len),
        });
    }
    Ok(theta_from_anchors(anchors))
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

/// Stage-1 admission gate: runs [`stage1_screen`] only if the batch's admitted
/// `respondents` meet the distinct-respondent floor `N1_MIN` (`docs/02` §B.6) — persons,
/// counted from the [`NullifierSet`] that [`submit_response`] filled, never rows (T65) —
/// and the rows (`theta`, each item column) are those respondents one to one.
pub fn screen(
    respondents: &NullifierSet,
    theta: &[f64],
    item_responses: &[Vec<f64>],
) -> Result<Vec<bool>, PilotError> {
    if respondents.len() < N1_MIN {
        return Err(PilotError::NotEnoughRespondents {
            have: respondents.len(),
            need: N1_MIN,
        });
    }
    respondent_rows(respondents, theta, item_responses.iter().map(Vec::len))?;
    Ok(stage1_screen(theta, item_responses))
}

/// Stage-2 admission gate: runs [`stage2_dif`] only on a batch of at least `K_MIN` items
/// (INV-8) with at least `N2_MIN` admitted respondents (persons, T65), whose rows match
/// them one to one. A batch of one is rejected (AT-PRO-02).
#[cfg(feature = "calibration")]
pub fn dif_batch(
    respondents: &NullifierSet,
    theta: &[f64],
    group: &[f64],
    item_responses: &[Vec<f64>],
) -> Result<Vec<DifVerdict>, PilotError> {
    admit_dif_batch(item_responses.len(), respondents.len(), N2_MIN)?;
    respondent_rows(
        respondents,
        theta,
        std::iter::once(group.len()).chain(item_responses.iter().map(Vec::len)),
    )?;
    Ok(stage2_dif(theta, group, item_responses))
}

/// Stage 1 screen (~300 respondents): keep items that discriminate. A negative
/// point-biserial signals a wrong answer key. A 2PL fit that did not converge (e.g. a
/// separated item, whose slope diverges) says nothing about `a`, so it fails (T34).
pub fn stage1_screen(theta: &[f64], item_responses: &[Vec<f64>]) -> Vec<bool> {
    item_responses
        .iter()
        .map(|item| {
            let rp = point_biserial(item, theta);
            let fit = fit_2pl_item(theta, item);
            rp >= R_PBIS_MIN && fit.status == LogisticFit::Converged && fit.a >= A_MIN
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
