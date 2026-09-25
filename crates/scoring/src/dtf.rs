//! Differential test functioning (`docs/02` §B.7, `docs/01` D38): how far a set of items
//! measured in one latent fit favours one class over another at equal ability.

use crate::dif::MIN_CLASS_SHARE;
use crate::fmath::exp;
use crate::latent::LatentDif;

/// Tolerance on a test's DTF bound, in score points (`docs/02` §B.7); provisional (T24/T25).
pub const DTF_MAX: f64 = 0.10;

const NODES: usize = 41;
const THETA_MAX: f64 = 5.0;

/// Class parameters that describe no fit: no class, lengths that disagree, a share that is
/// not positive, or a value that is not finite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BadClasses;

/// One fit's classes on the quadrature grid: the weights of its ability density, and per
/// item and class the curve on the grid, `curves[j][g][q] = P_jg(θ_q)`.
#[derive(Clone, Debug, PartialEq)]
pub struct ClassCurves {
    classes: usize,
    weights: Vec<f64>,
    curves: Vec<Vec<Vec<f64>>>,
}

fn sigmoid(z: f64) -> f64 {
    if z >= 0.0 {
        1.0 / (1.0 + exp(-z))
    } else {
        let e = exp(z);
        e / (1.0 + e)
    }
}

impl ClassCurves {
    /// Classes `g` with share `pi[g]` (renormalized), ability mean `eta[g]` and item parameters
    /// `a[g][j]`, `b[g][j]`, laid out as [`LatentDif::item_a`] and [`LatentDif::item_b`].
    pub fn new(
        pi: &[f64],
        eta: &[f64],
        a: &[Vec<f64>],
        b: &[Vec<f64>],
    ) -> Result<ClassCurves, BadClasses> {
        let classes = pi.len();
        let k = a.first().map_or(0, Vec::len);
        let shaped = classes > 0
            && eta.len() == classes
            && a.len() == classes
            && b.len() == classes
            && a.iter().chain(b).all(|row| row.len() == k);
        let finite = pi
            .iter()
            .chain(eta)
            .chain(a.iter().flatten())
            .chain(b.iter().flatten())
            .all(|v| v.is_finite());
        if !shaped || !finite || pi.iter().any(|&p| p <= 0.0) {
            return Err(BadClasses);
        }
        let grid: Vec<f64> = (0..NODES)
            .map(|q| -THETA_MAX + 2.0 * THETA_MAX * q as f64 / (NODES - 1) as f64)
            .collect();
        let density: Vec<f64> = grid
            .iter()
            .map(|t| {
                (0..classes)
                    .map(|g| pi[g] * exp(-0.5 * (t - eta[g]) * (t - eta[g])))
                    .sum()
            })
            .collect();
        let total: f64 = density.iter().sum();
        let curves = (0..k)
            .map(|j| {
                (0..classes)
                    .map(|g| {
                        grid.iter()
                            .map(|t| sigmoid(a[g][j] * (t - b[g][j])))
                            .collect()
                    })
                    .collect()
            })
            .collect();
        Ok(ClassCurves {
            classes,
            weights: density.iter().map(|d| d / total).collect(),
            curves,
        })
    }

    /// The counted classes of a target-model fit: share at least [`MIN_CLASS_SHARE`], as for
    /// `DIF_j` (`docs/02` §B.3).
    pub fn of(fit: &LatentDif) -> Result<ClassCurves, BadClasses> {
        let counted: Vec<usize> = (0..fit.pi.len())
            .filter(|&g| fit.pi[g] >= MIN_CLASS_SHARE)
            .collect();
        let pick = |v: &[f64]| -> Vec<f64> {
            counted
                .iter()
                .map(|&g| v.get(g).copied().unwrap_or(f64::NAN))
                .collect()
        };
        let rows = |m: &[Vec<f64>]| -> Vec<Vec<f64>> {
            counted
                .iter()
                .map(|&g| m.get(g).cloned().unwrap_or_default())
                .collect()
        };
        ClassCurves::new(
            &pick(&fit.pi),
            &pick(&fit.eta),
            &rows(&fit.item_a),
            &rows(&fit.item_b),
        )
    }

    pub fn classes(&self) -> usize {
        self.classes
    }

    pub fn items(&self) -> usize {
        self.curves.len()
    }

    /// `DTF_F(set)` in score points over the items `set` of this fit, whatever their order;
    /// `None` if an index is past the fit or repeated. 0 with one class or an empty set.
    pub fn dtf(&self, set: &[usize]) -> Option<f64> {
        let mut items = set.to_vec();
        items.sort_unstable();
        if items.windows(2).any(|w| w[0] == w[1])
            || items.last().is_some_and(|&j| j >= self.curves.len())
        {
            return None;
        }
        let mut worst = 0.0_f64;
        for g in 0..self.classes {
            for h in g + 1..self.classes {
                let total: f64 = self
                    .weights
                    .iter()
                    .enumerate()
                    .map(|(q, w)| {
                        let gap: f64 = items
                            .iter()
                            .map(|&j| self.curves[j][g][q] - self.curves[j][h][q])
                            .sum();
                        w * gap.abs()
                    })
                    .sum();
                worst = worst.max(total);
            }
        }
        Some(worst)
    }
}
