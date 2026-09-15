//! Coverage blueprint (`docs/05` [8], `docs/06` L2). DIF removes the bias of a single
//! item, not of the *pool*: every item can pass DIF while the choice of topics is
//! skewed. The blueprint fixes per-domain quotas that constrain how a test (or the
//! active pool) is composed. The quotas themselves are set by a sortition committee
//! (`governance`), not by vote; this module only holds and enforces them.

use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

/// Relative target shares per domain (need not sum to 1). Domains are assumed distinct.
#[derive(Clone, Debug)]
pub struct Blueprint<D> {
    shares: Vec<(D, f64)>,
}

/// A domain that cannot meet its quota from the available items.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shortfall<D> {
    pub domain: D,
    pub needed: usize,
    pub available: usize,
}

impl<D: Clone + PartialEq> Blueprint<D> {
    pub fn new(shares: Vec<(D, f64)>) -> Self {
        Blueprint { shares }
    }

    /// Hamilton (largest-remainder) apportionment of `size` seats across the domains,
    /// proportional to their shares. Deterministic; the seats sum to `size`.
    pub fn quotas(&self, size: usize) -> Vec<(D, usize)> {
        let total: f64 = self.shares.iter().map(|(_, s)| s).sum();
        if total <= 0.0 || size == 0 {
            return self.shares.iter().map(|(d, _)| (d.clone(), 0)).collect();
        }
        let exact: Vec<f64> = self
            .shares
            .iter()
            .map(|(_, s)| size as f64 * s / total)
            .collect();
        let mut counts: Vec<usize> = exact.iter().map(|e| e.floor() as usize).collect();
        let mut remainder = size - counts.iter().sum::<usize>();

        let mut order: Vec<usize> = (0..self.shares.len()).collect();
        order.sort_by(|&a, &b| {
            let fa = exact[a] - exact[a].floor();
            let fb = exact[b] - exact[b].floor();
            fb.partial_cmp(&fa).unwrap().then(a.cmp(&b))
        });
        for &i in &order {
            if remainder == 0 {
                break;
            }
            counts[i] += 1;
            remainder -= 1;
        }
        self.shares
            .iter()
            .enumerate()
            .map(|(i, (d, _))| (d.clone(), counts[i]))
            .collect()
    }

    /// Per-domain `actual_share − target_share` over a pool; positive = over-represented,
    /// negative = under-represented. Surfaces topic skew the DIF test cannot see.
    pub fn coverage_deviation(&self, pool: &[D]) -> Vec<(D, f64)> {
        let n = pool.len();
        let total: f64 = self.shares.iter().map(|(_, s)| s).sum();
        self.shares
            .iter()
            .map(|(d, s)| {
                let actual = if n == 0 {
                    0.0
                } else {
                    pool.iter().filter(|x| *x == d).count() as f64 / n as f64
                };
                let target = if total > 0.0 { s / total } else { 0.0 };
                (d.clone(), actual - target)
            })
            .collect()
    }
}

/// Selects items to fill the blueprint quotas for a test of `size`. Within each domain
/// items are drawn deterministically (seeded). Returns the chosen items, or the
/// per-domain shortfalls if some domain has too few available items.
pub fn assemble_test<Item: Clone, D: Clone + PartialEq>(
    available: &[(Item, D)],
    size: usize,
    blueprint: &Blueprint<D>,
    seed: u64,
) -> Result<Vec<Item>, Vec<Shortfall<D>>> {
    let quotas = blueprint.quotas(size);
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut chosen = Vec::new();
    let mut shortfalls = Vec::new();

    for (domain, need) in &quotas {
        let mut pool: Vec<&Item> = available
            .iter()
            .filter(|(_, d)| d == domain)
            .map(|(it, _)| it)
            .collect();
        if pool.len() < *need {
            shortfalls.push(Shortfall {
                domain: domain.clone(),
                needed: *need,
                available: pool.len(),
            });
            continue;
        }
        pool.shuffle(&mut rng);
        chosen.extend(pool.into_iter().take(*need).cloned());
    }

    if shortfalls.is_empty() {
        Ok(chosen)
    } else {
        Err(shortfalls)
    }
}
