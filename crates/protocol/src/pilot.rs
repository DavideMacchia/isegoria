//! Two-stage pilot (`docs/05` [6]/[7], `docs/01` D11): a cheap stage 1 screen, then DIF
//! in batches at stage 2 (`docs/02` §B). `screen`/`dif_batch` enforce the floors (INV-8,
//! PROTO-006); "distinct respondents" counts persons via [`NullifierSet`] (INV-9, T65).

use crate::admission::{admit, DuplicateNullifier, NullifierSet, Unproven};
use crate::lifecycle::K_MIN;
use identity::credential::IssuerPublic;
use identity::nullifier::NullifierProof;
use identity::nym::{Nym, Role};
use network::cid::{cid, Cid};
#[cfg(feature = "calibration")]
use scoring::dif::{logistic_dif, BETA2_MAX};
use scoring::irt::{fit_2pl_item, kr20, point_biserial, A_MIN, KR20_MIN, R_PBIS_MIN};
use scoring::LogisticFit;

/// Stage-1 distinct-respondent floor (`docs/02` §B.6: the classic discrimination screen).
pub const N1_MIN: usize = 300;
/// Stage-2 (Variant-1, attribute DIF) respondent floor (`docs/02` §B.6).
pub const N2_MIN: usize = 1500;

/// Why a pilot batch is not admissible (`docs/08` INV-8, §B.6 sample sizes, D37).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PilotError {
    NotEnoughRespondents { have: usize, need: usize },
    BatchTooSmall { items: usize },
    UnreliableAnchors { kr20: f64, need: f64 },
    RowCountMismatch { rows: usize, respondents: usize },
}

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

/// The action context a `Respond` proof is bound to: this batch and epoch (T65).
pub fn response_context(batch: Cid, epoch: u64) -> Vec<u8> {
    let mut ctx = Vec::with_capacity(40);
    ctx.extend_from_slice(&batch.0);
    ctx.extend_from_slice(&epoch.to_le_bytes());
    ctx
}

/// Why an answer sheet was refused at the identity-gated respondent entry point.
#[derive(Debug, PartialEq, Eq)]
pub enum ResponseRejected {
    Unproven(Unproven),
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
/// records the proven `Respond` id in `respondents`, rejecting a repeat sheet (AT-PRO-09).
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

/// INV-8 batch admission: never a single item, and enough respondents (`n_min`).
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

/// Anchor-reliability precondition of the latent re-check (`docs/01` D37, T53): refused
/// below `KR20_MIN` or when undefined. `anchors` is respondents × anchors, 0/1.
pub fn admit_anchors(anchors: &[Vec<f64>]) -> Result<f64, PilotError> {
    let r = kr20(anchors);
    if r.is_nan() || r < KR20_MIN {
        return Err(PilotError::UnreliableAnchors {
            kr20: r,
            need: KR20_MIN,
        });
    }
    Ok(r)
}

/// Outcome of the attribute-based DIF screen for one item (calibration-only, D20).
#[cfg(feature = "calibration")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DifVerdict {
    Pass,
    Reject,
    Undetermined,
}

/// Stage-1 admission gate: runs [`stage1_screen`] once admitted `respondents` meet the
/// floor `N1_MIN`, and the rows (`theta`, each item column) match them one to one.
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

/// Stage-2 admission gate: runs [`stage2_dif`] on ≥ `K_MIN` items with ≥ `N2_MIN` respondents.
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

/// Stage 1 screen: keep items that discriminate. A negative point-biserial signals a
/// wrong answer key; a non-converged 2PL fit says nothing about `a` (T34).
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

/// Stage 2, run on the surviving batch: rejects items with uniform DIF against the axis
/// (Variant 1, calibration-only, D20; production uses `revalidate_pool_latent`, Variant 2).
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
