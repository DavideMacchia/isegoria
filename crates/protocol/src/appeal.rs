//! [5b] The appeal stake (`docs/01` D27, `docs/05` [5b], `docs/08` REPUTATION-007, T61).
//!
//! The author's reputation `C_a` is `reputation::author_score`: a shrunk, time-decayed
//! average of the measured quality of the author's items. The cost of an appeal is not a
//! deduction from a ledger but a *negative pseudo-observation inside that average*:
//! filing escrows a zero-quality observation, and the verdict replaces it with the item's
//! measured quality when the evidence promotes the item, or leaves it standing — the item
//! failed, so the observation is real — when it does not. An author may file only while
//! `C_a` covers the stake, at least the prior mean, so a failed appeal costs the next one
//! until the evidence has restored the average.

use scoring::reputation::{author_score, AuthorPrior};

/// The quality of the escrowed pseudo-observation: the worst outcome.
pub const STAKE_QUALITY: f64 = 0.0;

/// The reputation an author needs to file an appeal (`docs/08` §9.1: `C_a ≥ stake`): the
/// prior mean `α₀ / (α₀ + β₀)`, 0.4 with the default prior. A fresh author can appeal
/// once; after a failed appeal, only once the evidence has restored the average.
/// Provisional (T25).
pub fn appeal_floor(prior: &AuthorPrior) -> f64 {
    prior.alpha0 / (prior.alpha0 + prior.beta0)
}

/// The author's history of measured item qualities and their ages in months — what
/// `C_a` is computed from, and where an appeal's stake lives.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AuthorHistory {
    qualities: Vec<f64>,
    ages_months: Vec<f64>,
}

/// The author's reputation does not cover the stake.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InsufficientReputation {
    pub reputation: f64,
    pub floor: f64,
}

/// A filed appeal's escrow: the pseudo-observation it holds in the author's history.
/// Settled once, by value ([`AuthorHistory::settle`]).
#[derive(Debug, PartialEq, Eq)]
pub struct Escrow(usize);

/// The evidence filter's verdict on an appealed item.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AppealOutcome {
    /// Promoted: the item's measured quality replaces the pseudo-observation.
    Promoted { quality: f64 },
    /// Failed: the zero stands — the item's real result.
    Failed,
}

impl AuthorHistory {
    pub fn new() -> Self {
        Self::default()
    }

    /// One observation per item: its measured quality in `[0, 1]` and its age in months.
    pub fn record(&mut self, quality: f64, age_months: f64) {
        self.qualities.push(quality.clamp(0.0, 1.0));
        self.ages_months.push(age_months.max(0.0));
    }

    /// `C_a` (`docs/02` §C.1).
    pub fn reputation(&self, prior: &AuthorPrior) -> f64 {
        author_score(&self.qualities, &self.ages_months, prior)
    }

    /// Whether the author may file an appeal: `C_a` at or above [`appeal_floor`].
    pub fn covers_stake(&self, prior: &AuthorPrior) -> bool {
        self.reputation(prior) >= appeal_floor(prior)
    }

    /// Files an appeal: refused unless the reputation covers the stake; otherwise the
    /// zero-quality pseudo-observation is escrowed (age 0), and `C_a` falls at once.
    pub fn file_appeal(&mut self, prior: &AuthorPrior) -> Result<Escrow, InsufficientReputation> {
        let reputation = self.reputation(prior);
        let floor = appeal_floor(prior);
        if reputation < floor {
            return Err(InsufficientReputation { reputation, floor });
        }
        self.qualities.push(STAKE_QUALITY);
        self.ages_months.push(0.0);
        Ok(Escrow(self.qualities.len() - 1))
    }

    /// Settles an appeal on the evidence filter's verdict (D27): the escrow is replaced
    /// by the item's measured quality if promoted, and left standing if not.
    pub fn settle(&mut self, escrow: Escrow, outcome: AppealOutcome) {
        if let AppealOutcome::Promoted { quality } = outcome {
            self.qualities[escrow.0] = quality.clamp(0.0, 1.0);
        }
    }

    /// The qualities on record, escrows included.
    pub fn qualities(&self) -> &[f64] {
        &self.qualities
    }

    pub fn len(&self) -> usize {
        self.qualities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.qualities.is_empty()
    }
}
