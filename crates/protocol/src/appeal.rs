//! The appeal stake (`docs/01` D27, `docs/05` [5b], T61): filing escrows a zero-quality
//! pseudo-observation inside the author's reputation average (`reputation::author_score`);
//! the verdict replaces it, or leaves it standing, never a ledger deduction.

use scoring::reputation::{author_score, AuthorPrior};

pub const STAKE_QUALITY: f64 = 0.0;

/// The stake an author's `C_a` must cover to file: the prior mean (`docs/08` §9.1; T25).
pub fn appeal_floor(prior: &AuthorPrior) -> f64 {
    prior.alpha0 / (prior.alpha0 + prior.beta0)
}

/// The author's history of measured item qualities and ages (months); what `C_a` is computed from.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AuthorHistory {
    qualities: Vec<f64>,
    ages_months: Vec<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InsufficientReputation {
    pub reputation: f64,
    pub floor: f64,
}

/// A filed appeal's escrow: an index into the author's qualities, settled once.
#[derive(Debug, PartialEq, Eq)]
pub struct Escrow(usize);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AppealOutcome {
    Promoted { quality: f64 },
    Failed,
}

impl AuthorHistory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, quality: f64, age_months: f64) {
        self.qualities.push(quality.clamp(0.0, 1.0));
        self.ages_months.push(age_months.max(0.0));
    }

    pub fn reputation(&self, prior: &AuthorPrior) -> f64 {
        author_score(&self.qualities, &self.ages_months, prior)
    }

    pub fn covers_stake(&self, prior: &AuthorPrior) -> bool {
        self.reputation(prior) >= appeal_floor(prior)
    }

    /// Files an appeal: refused unless `C_a` covers the stake, else escrows a zero observation.
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

    /// Settles an appeal (D27): the escrow takes the measured quality if promoted, else stands.
    pub fn settle(&mut self, escrow: Escrow, outcome: AppealOutcome) {
        if let AppealOutcome::Promoted { quality } = outcome {
            self.qualities[escrow.0] = quality.clamp(0.0, 1.0);
        }
    }

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
