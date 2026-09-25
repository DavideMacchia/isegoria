//! Anti-collusion (`docs/02` §Anti-collusion, `docs/01` D7, D39, D40): coordination is the
//! correlation of two reviewers' residuals — rating minus the bridging prediction — read
//! past [`MIN_SHARED_ITEMS`] and tested against a permutation null ([`coordination_clusters`]).

use crate::bridging::{Fit, Ratings};
use crate::fmath::powf;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use std::collections::BTreeMap;

pub const ALPHA: f64 = 0.5;

/// Fewest shared items before two reviewers' residual correlation is read (D39): below
/// it, a correlation is noise regardless of value.
pub const MIN_SHARED_ITEMS: usize = 30;

/// Settings of the coordination detector (D39, T56). Provisional (T25).
#[derive(Clone, Copy, Debug)]
pub struct CoordinationParams {
    /// Shared items before a pair is read ([`MIN_SHARED_ITEMS`]).
    pub min_shared: usize,
    /// Residual correlation a flagged pair must reach, and the mean cross-pair correlation
    /// two clusters must keep to merge.
    pub rho_min: f64,
    /// The permutation p-value a flagged pair must reach.
    pub p_max: f64,
    /// Permutations of the null; the p-value's resolution is `1 / (permutations + 1)`.
    pub permutations: usize,
    pub seed: u64,
}

impl Default for CoordinationParams {
    fn default() -> Self {
        CoordinationParams {
            min_shared: MIN_SHARED_ITEMS,
            rho_min: 0.7,
            p_max: 0.001,
            permutations: 999,
            seed: 0,
        }
    }
}

/// `item_ids` does not name every item of the epoch's ratings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemIdCount {
    pub expected: usize,
    pub found: usize,
}

/// Every reviewer's residuals on the items they rated, accumulated across epochs (D39):
/// `r_uj − r̂_uj` by global item id; the caller keeps reviewer indices stable across epochs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResidualHistory {
    rows: Vec<BTreeMap<u64, f64>>,
}

impl ResidualHistory {
    pub fn new(reviewers: usize) -> Self {
        ResidualHistory {
            rows: vec![BTreeMap::new(); reviewers],
        }
    }

    pub fn reviewers(&self) -> usize {
        self.rows.len()
    }

    /// One residual of `reviewer` on `item`; a later record of the same pair replaces it.
    pub fn record(&mut self, reviewer: usize, item: u64, residual: f64) {
        if reviewer >= self.rows.len() {
            self.rows.resize(reviewer + 1, BTreeMap::new());
        }
        self.rows[reviewer].insert(item, residual);
    }

    /// One epoch's residuals from its bridging fit: for every observation `(u, j)`, `r − r̂`
    /// with `r̂ = μ + b_u + b_j + f_u·f_j`; `item_ids[j]` names item `j` across epochs.
    pub fn record_epoch(
        &mut self,
        ratings: &Ratings,
        fit: &Fit,
        item_ids: &[u64],
    ) -> Result<(), ItemIdCount> {
        if item_ids.len() != ratings.m {
            return Err(ItemIdCount {
                expected: ratings.m,
                found: item_ids.len(),
            });
        }
        for o in &ratings.obs {
            let predicted = fit.mu + fit.b_u[o.u] + fit.b_j[o.j] + fit.f_u[o.u] * fit.f_j[o.j];
            self.record(o.u, item_ids[o.j], o.r - predicted);
        }
        Ok(())
    }

    /// The items both `u` and `v` rated.
    pub fn shared(&self, u: usize, v: usize) -> usize {
        self.shared_residuals(u, v).0.len()
    }

    fn shared_residuals(&self, u: usize, v: usize) -> (Vec<f64>, Vec<f64>) {
        let (mut a, mut b) = (Vec::new(), Vec::new());
        if let (Some(ru), Some(rv)) = (self.rows.get(u), self.rows.get(v)) {
            for (item, x) in ru {
                if let Some(y) = rv.get(item) {
                    a.push(*x);
                    b.push(*y);
                }
            }
        }
        (a, b)
    }

    /// Residual correlation of `u`, `v` and the shared count; `None` under `min_shared` (D39).
    pub fn pair(&self, u: usize, v: usize, min_shared: usize) -> Option<(f64, usize)> {
        let (a, b) = self.shared_residuals(u, v);
        if a.len() < min_shared.max(2) {
            return None;
        }
        Some((pearson(&a, &b), a.len()))
    }
}

/// The permutation null (D39): the share of `permutations` seeded re-pairings of `b` with
/// `a` whose correlation reaches `rho`, one-sided. Seeded, so the verdict is reproducible (INV-7).
pub fn permutation_p_value(a: &[f64], b: &[f64], rho: f64, permutations: usize, seed: u64) -> f64 {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut shuffled = b.to_vec();
    let mut hits = 0usize;
    for _ in 0..permutations {
        shuffled.shuffle(&mut rng);
        if pearson(a, &shuffled) >= rho {
            hits += 1;
        }
    }
    (1 + hits) as f64 / (permutations + 1) as f64
}

/// The evidence on one flagged pair.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PairEvidence {
    pub u: usize,
    pub v: usize,
    pub shared: usize,
    pub rho: f64,
    pub p_value: f64,
}

/// The flagged pairs and a cluster id per reviewer (the smallest index in the cluster).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CoordinationReport {
    pub flagged: Vec<PairEvidence>,
    pub clusters: Vec<usize>,
}

impl CoordinationReport {
    /// The members of each cluster with at least two, in index order.
    pub fn groups(&self) -> Vec<Vec<usize>> {
        let mut by_id: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (u, &c) in self.clusters.iter().enumerate() {
            by_id.entry(c).or_default().push(u);
        }
        by_id.into_values().filter(|g| g.len() > 1).collect()
    }
}

/// The coordination detector (D39, T56): pairs past `min_shared` are flagged at `rho_min`
/// and `p_max`; average linkage merges groups while their mean cross-pair correlation
/// still reaches `rho_min` (COLLUSION-005).
pub fn coordination_clusters(
    history: &ResidualHistory,
    params: &CoordinationParams,
) -> CoordinationReport {
    let n = history.reviewers();
    let mut flagged = Vec::new();
    let mut sim: BTreeMap<(usize, usize), f64> = BTreeMap::new();
    for u in 0..n {
        for v in (u + 1)..n {
            let Some((rho, shared)) = history.pair(u, v, params.min_shared) else {
                continue;
            };
            if rho < params.rho_min {
                continue;
            }
            let (a, b) = history.shared_residuals(u, v);
            let seed =
                params.seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ ((u as u64) << 32 | v as u64);
            let p_value = permutation_p_value(&a, &b, rho, params.permutations, seed);
            if p_value <= params.p_max {
                flagged.push(PairEvidence {
                    u,
                    v,
                    shared,
                    rho,
                    p_value,
                });
                sim.insert((u, v), rho);
            }
        }
    }

    // Average linkage over the flagged reviewers; everyone else stays a singleton.
    let mut groups: Vec<Vec<usize>> = {
        let mut members: Vec<usize> = flagged.iter().flat_map(|e| [e.u, e.v]).collect();
        members.sort_unstable();
        members.dedup();
        members.into_iter().map(|u| vec![u]).collect()
    };
    let similarity =
        |a: usize, b: usize| -> f64 { *sim.get(&(a.min(b), a.max(b))).unwrap_or(&0.0) };
    loop {
        let mut best: Option<(usize, usize, f64)> = None;
        for i in 0..groups.len() {
            for j in (i + 1)..groups.len() {
                let total: f64 = groups[i]
                    .iter()
                    .flat_map(|&a| groups[j].iter().map(move |&b| (a, b)))
                    .map(|(a, b)| similarity(a, b))
                    .sum();
                let mean = total / (groups[i].len() * groups[j].len()) as f64;
                if mean >= params.rho_min && best.is_none_or(|(_, _, m)| mean > m) {
                    best = Some((i, j, mean));
                }
            }
        }
        let Some((i, j, _)) = best else {
            break;
        };
        let merged = groups.remove(j);
        groups[i].extend(merged);
        groups[i].sort_unstable();
    }

    let mut clusters: Vec<usize> = (0..n).collect();
    for g in &groups {
        let id = g[0];
        for &u in g {
            clusters[u] = id;
        }
    }
    CoordinationReport { flagged, clusters }
}

/// Pearson correlation between every pair of dense judgment rows; a constant row counts as 0.
pub fn correlation_matrix(judgments: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let n = judgments.len();
    let mut c = vec![vec![0.0; n]; n];
    for i in 0..n {
        c[i][i] = 1.0;
        for j in (i + 1)..n {
            let r = pearson(&judgments[i], &judgments[j]);
            c[i][j] = r;
            c[j][i] = r;
        }
    }
    c
}

/// Connected components of the `|ρ| ≥ threshold` graph; a cluster id per node.
pub fn cluster_by_correlation(corr: &[Vec<f64>], threshold: f64) -> Vec<usize> {
    let n = corr.len();
    let mut parent: Vec<usize> = (0..n).collect();
    for (i, row) in corr.iter().enumerate() {
        for (j, &rij) in row.iter().enumerate().skip(i + 1) {
            if rij.abs() >= threshold {
                union(&mut parent, i, j);
            }
        }
    }
    (0..n).map(|i| find(&mut parent, i)).collect()
}

/// Total weight a group contributes: `min(Σ w, (Σ w)^α)` (INV-14): a discount, never a boost.
pub fn sublinear_group_weight(group_weights: &[f64], alpha: f64) -> f64 {
    let sum = group_weights.iter().sum::<f64>();
    powf(sum, alpha).min(sum)
}

/// Per-node discounted weights: each cluster's total is shrunk to `(Σ w)^α` and split back
/// in proportion to raw weight, capped at 1 per INV-14 so a cluster under 1 is never boosted.
pub fn discount_weights(weights: &[f64], cluster_ids: &[usize], alpha: f64) -> Vec<f64> {
    let n = weights.len();
    let max_id = cluster_ids.iter().copied().max().map_or(0, |m| m + 1);
    let mut group_sum = vec![0.0; max_id];
    for i in 0..n {
        group_sum[cluster_ids[i]] += weights[i];
    }
    (0..n)
        .map(|i| {
            let s = group_sum[cluster_ids[i]];
            if s > 0.0 {
                // s^{α−1} is the fraction of raw weight each member keeps, capped at 1 (INV-14).
                weights[i] * (powf(s, alpha) / s).min(1.0)
            } else {
                0.0
            }
        })
        .collect()
}

fn pearson(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len() as f64;
    let ma = a.iter().sum::<f64>() / n;
    let mb = b.iter().sum::<f64>() / n;
    let (mut cov, mut va, mut vb) = (0.0, 0.0, 0.0);
    for i in 0..a.len() {
        cov += (a[i] - ma) * (b[i] - mb);
        va += (a[i] - ma).powi(2);
        vb += (b[i] - mb).powi(2);
    }
    if va == 0.0 || vb == 0.0 {
        0.0
    } else {
        cov / (va.sqrt() * vb.sqrt())
    }
}

fn find(parent: &mut [usize], x: usize) -> usize {
    let mut root = x;
    while parent[root] != root {
        root = parent[root];
    }
    let mut cur = x;
    while parent[cur] != root {
        let next = parent[cur];
        parent[cur] = root;
        cur = next;
    }
    root
}

fn union(parent: &mut [usize], a: usize, b: usize) {
    let ra = find(parent, a);
    let rb = find(parent, b);
    if ra != rb {
        parent[ra.max(rb)] = ra.min(rb);
    }
}
