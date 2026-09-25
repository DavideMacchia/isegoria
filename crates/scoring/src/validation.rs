//! Level B — iterative purification. See `docs/02` §B.4.

use crate::dif::{logistic_dif, BETA2_MAX};
use crate::irt::standardize;

#[derive(Clone, Debug)]
pub struct Purified {
    pub theta: Vec<f64>,
    /// per batch item: true if flagged for DIF
    pub flagged: Vec<bool>,
    pub iterations: usize,
}

/// `anchors` and `batch` are respondents × items.
pub fn purify_theta(
    anchors: &[Vec<f64>],
    batch: &[Vec<f64>],
    group: &[f64],
    max_rounds: usize,
) -> Purified {
    let k = if batch.is_empty() { 0 } else { batch[0].len() };
    let anchor_totals: Vec<f64> = anchors.iter().map(|r| r.iter().sum()).collect();

    let mut flagged = vec![false; k];
    let mut theta = standardize(&anchor_totals);

    for round in 1..=max_rounds {
        let mut new_flagged = vec![false; k];
        for (j, f) in new_flagged.iter_mut().enumerate() {
            let col: Vec<f64> = batch.iter().map(|r| r[j]).collect();
            *f = logistic_dif(&col, &theta, group).beta2.abs() > BETA2_MAX;
        }

        let mut totals = anchor_totals.clone();
        for (i, t) in totals.iter_mut().enumerate() {
            for (j, keep) in new_flagged.iter().enumerate() {
                if !keep {
                    *t += batch[i][j];
                }
            }
        }
        theta = standardize(&totals);

        if new_flagged == flagged {
            return Purified {
                theta,
                flagged: new_flagged,
                iterations: round,
            };
        }
        flagged = new_flagged;
    }

    Purified {
        theta,
        flagged,
        iterations: max_rounds,
    }
}
