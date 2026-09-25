//! Item exposure and retirement (`docs/05` [9], `docs/00` [8]): an item used a lot
//! gets memorized and loses value. Countermeasures: pool rotation, parametric items,
//! and retirement on exposure, drift, obsolescence, or emerging DIF.

use network::cid::{cid, Cid};
use std::collections::HashMap;

pub const EXPOSURE_LIMIT: usize = 2000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetirementReason {
    EmergingDif,
    Drift,
    Obsolescence,
    Exposure,
}

/// Validity flags an item carries from periodic pool re-validation (`docs/05` [8]).
#[derive(Clone, Copy, Debug, Default)]
pub struct ItemHealth {
    pub emerging_dif: bool,
    pub drifted: bool,
    pub obsolete: bool,
}

/// Whether an item should leave the active pool, and why. Validity triggers are
/// checked before exposure; the first that applies wins.
pub fn should_retire(
    exposure: usize,
    limit: usize,
    health: ItemHealth,
) -> Option<RetirementReason> {
    if health.emerging_dif {
        Some(RetirementReason::EmergingDif)
    } else if health.drifted {
        Some(RetirementReason::Drift)
    } else if health.obsolete {
        Some(RetirementReason::Obsolescence)
    } else if exposure >= limit {
        Some(RetirementReason::Exposure)
    } else {
        None
    }
}

/// Counts how many times each item has been administered.
#[derive(Default)]
pub struct ExposureLedger {
    counts: HashMap<Cid, usize>,
}

impl ExposureLedger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, item: Cid) {
        *self.counts.entry(item).or_insert(0) += 1;
    }

    pub fn count(&self, item: &Cid) -> usize {
        self.counts.get(item).copied().unwrap_or(0)
    }

    pub fn is_overexposed(&self, item: &Cid, limit: usize) -> bool {
        self.count(item) >= limit
    }
}

/// A parametric item: a fixed structure whose slots are filled by different values,
/// so many concrete items share one template. Rotating variants spreads exposure so
/// no single concrete item is memorized.
pub struct Template {
    structure: Vec<u8>,
}

impl Template {
    pub fn new(structure: impl Into<Vec<u8>>) -> Self {
        Template {
            structure: structure.into(),
        }
    }

    /// Content id of the concrete item for these parameter values. The structure is
    /// length-prefixed so no `(structure, values)` pair collides with another.
    pub fn variant(&self, values: &[u8]) -> Cid {
        let mut buf = Vec::with_capacity(8 + self.structure.len() + values.len());
        buf.extend_from_slice(&(self.structure.len() as u64).to_le_bytes());
        buf.extend_from_slice(&self.structure);
        buf.extend_from_slice(values);
        cid(&buf)
    }
}

/// Picks the least-exposed variant among `value_sets` (rotation to spread exposure).
/// Ties go to the earliest; returns `None` only for an empty set.
pub fn least_exposed_variant<'a>(
    template: &Template,
    value_sets: &'a [Vec<u8>],
    ledger: &ExposureLedger,
) -> Option<(&'a [u8], Cid)> {
    value_sets
        .iter()
        .map(|v| {
            let id = template.variant(v);
            (ledger.count(&id), v, id)
        })
        .min_by_key(|(count, _, _)| *count)
        .map(|(_, v, id)| (v.as_slice(), id))
}
