//! The contested-facts pool (`docs/05` [7b], `docs/02` §B.7, `docs/01` D38): DIF items whose
//! key the source check established, drawn into a test only in selections whose DTF bound —
//! the sum over fits of each fit's DTF — stays within the tolerance.

use crate::randomness::{Beacon, CONTESTED};
use network::cid::Cid;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use scoring::dtf::ClassCurves;

/// `2³²`: the bound is summed in whole units of `2⁻³²` score points, each DTF rounded up.
const SCALE: f64 = 4_294_967_296.0;

fn units(dtf: f64) -> u64 {
    (dtf * SCALE).ceil() as u64
}

#[derive(Clone, Debug)]
struct Fit {
    curves: ClassCurves,
    members: Vec<(Cid, usize)>,
}

/// A fit's subset the draw may take: positions in `Fit::members`, and its DTF in units.
struct Candidate {
    members: Vec<usize>,
    units: u64,
}

/// Contested facts grouped by the latent fit that last measured them, in canonical order:
/// members by content id within a fit, fits by their least member (`docs/02` §B.7).
#[derive(Clone, Debug, Default)]
pub struct ContestedPool {
    fits: Vec<Fit>,
}

/// Why a fit's members were not recorded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordError {
    IndexOutOfRange {
        index: usize,
        items: usize,
    },
    /// The same item, or the same index, listed twice.
    DuplicateMember,
}

/// No selection of `requested` facts is balanced; `feasible` lists the sizes up to it that are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoBalancedDraw {
    pub requested: usize,
    pub feasible: Vec<usize>,
}

impl ContestedPool {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.fits.iter().map(|f| f.members.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.fits.is_empty()
    }

    pub fn contains(&self, item: &Cid) -> bool {
        self.fits
            .iter()
            .any(|f| f.members.iter().any(|(c, _)| c == item))
    }

    /// Records `curves` as the latest fit of `members` (an item and its index in the fit): a
    /// member already in the pool moves to it (`docs/05` [7b] re-measurement).
    pub fn record(
        &mut self,
        curves: ClassCurves,
        members: &[(Cid, usize)],
    ) -> Result<(), RecordError> {
        let items = curves.items();
        if let Some(&(_, index)) = members.iter().find(|(_, j)| *j >= items) {
            return Err(RecordError::IndexOutOfRange { index, items });
        }
        let repeated = (1..members.len()).any(|i| {
            members[..i]
                .iter()
                .any(|(c, j)| *c == members[i].0 || *j == members[i].1)
        });
        if repeated {
            return Err(RecordError::DuplicateMember);
        }
        for (item, _) in members {
            self.remove(item);
        }
        if !members.is_empty() {
            self.fits.push(Fit {
                curves,
                members: members.to_vec(),
            });
            self.canonicalize();
        }
        Ok(())
    }

    /// Takes `item` out of the pool (to the active pool, or retired); false if absent.
    pub fn remove(&mut self, item: &Cid) -> bool {
        let before = self.len();
        for fit in &mut self.fits {
            fit.members.retain(|(c, _)| c != item);
        }
        self.fits.retain(|f| !f.members.is_empty());
        self.canonicalize();
        self.len() != before
    }

    fn canonicalize(&mut self) {
        for fit in &mut self.fits {
            fit.members.sort_unstable_by_key(|(c, _)| c.0);
        }
        self.fits
            .sort_unstable_by_key(|f| f.members.first().map(|(c, _)| c.0));
    }

    /// The bound `D(T)` of `selection`, rounded up to `2⁻³²` score points; `None` if an item is
    /// not in the pool or listed twice.
    pub fn dtf(&self, selection: &[Cid]) -> Option<f64> {
        let distinct = (1..selection.len()).all(|i| !selection[..i].contains(&selection[i]));
        let mut found = 0;
        let mut total: u64 = 0;
        for fit in &self.fits {
            let set: Vec<usize> = fit
                .members
                .iter()
                .filter(|(c, _)| selection.contains(c))
                .map(|&(_, j)| j)
                .collect();
            found += set.len();
            total = total.checked_add(units(fit.curves.dtf(&set)?))?;
        }
        (distinct && found == selection.len()).then(|| total as f64 / SCALE)
    }

    /// Draws `n` contested facts whose bound is at most `tolerance`, at random among the
    /// balanced selections (`docs/02` §B.7), a function of the pool's content and `seed`
    /// alone. A negative or NaN `tolerance` admits only a bound of 0.
    pub fn draw(&self, n: usize, tolerance: f64, seed: u64) -> Result<Vec<Cid>, NoBalancedDraw> {
        let budget = (tolerance * SCALE).floor() as u64;
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let mut order: Vec<usize> = (0..self.fits.len()).collect();
        order.shuffle(&mut rng);
        let options: Vec<Vec<Candidate>> = order
            .iter()
            .map(|&f| self.candidates(f, n, budget))
            .collect();

        // `least[i][s]`: the least bound that takes exactly `s` facts from the fits `order[i..]`.
        let m = order.len();
        let mut least = vec![vec![None; n + 1]; m + 1];
        least[m][0] = Some(0u64);
        for i in (0..m).rev() {
            for s in 0..=n {
                least[i][s] = options[i]
                    .iter()
                    .filter(|c| c.members.len() <= s)
                    .filter_map(|c| least[i + 1][s - c.members.len()]?.checked_add(c.units))
                    .min();
            }
        }
        let within = |bound: Option<u64>| bound.is_some_and(|b| b <= budget);
        if !within(least[0][n]) {
            return Err(NoBalancedDraw {
                requested: n,
                feasible: (0..=n).filter(|&s| within(least[0][s])).collect(),
            });
        }

        let (mut left, mut room) = (n, budget);
        let mut drawn = Vec::with_capacity(n);
        for (i, &f) in order.iter().enumerate() {
            let open: Vec<&Candidate> = options[i]
                .iter()
                .filter(|c| c.members.len() <= left)
                .filter(|c| {
                    least[i + 1][left - c.members.len()]
                        .and_then(|rest| rest.checked_add(c.units))
                        .is_some_and(|b| b <= room)
                })
                .collect();
            let pick = open[rng.gen_range(0..open.len())];
            room -= pick.units;
            left -= pick.members.len();
            drawn.extend(pick.members.iter().map(|&k| self.fits[f].members[k].0));
        }
        drawn.shuffle(&mut rng);
        Ok(drawn)
    }

    /// [`ContestedPool::draw`] seeded from the signed checkpoint (INV-10), keyed on the test's
    /// index, never on its content.
    pub fn draw_from_beacon(
        &self,
        n: usize,
        tolerance: f64,
        beacon: &Beacon,
        test: u64,
    ) -> Result<Vec<Cid>, NoBalancedDraw> {
        self.draw(n, tolerance, beacon.seed(CONTESTED, test))
    }

    /// Fit `f`'s subsets of at most `n` members with a DTF within `budget`, the empty one first.
    fn candidates(&self, f: usize, n: usize, budget: u64) -> Vec<Candidate> {
        let fit = &self.fits[f];
        let mut out = Vec::new();
        let mut stack: Vec<Vec<usize>> = vec![Vec::new()];
        while let Some(members) = stack.pop() {
            if members.len() < n {
                let next = members.last().map_or(0, |&k| k + 1);
                for k in (next..fit.members.len()).rev() {
                    let mut grown = members.clone();
                    grown.push(k);
                    stack.push(grown);
                }
            }
            let set: Vec<usize> = members.iter().map(|&k| fit.members[k].1).collect();
            if let Some(cost) = fit.curves.dtf(&set).map(units).filter(|&u| u <= budget) {
                out.push(Candidate {
                    members,
                    units: cost,
                });
            }
        }
        out
    }
}
