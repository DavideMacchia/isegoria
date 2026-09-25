//! Anti-collusion — sublinear discount. See `docs/02`, §Anti-collusion, `docs/01` D7.
//!
//! An individual weight cap does not stop a cartel. Correlated nodes are clustered
//! by behavior and a group's weight grows as `(Σ w)^α` with `α ≈ 0.5`, so `k`
//! coordinated nodes count as `√k` (500 ≈ 22). Independent nodes fall into
//! singletons and, at unit weight, are left untouched.

use crate::fmath::powf;

pub const ALPHA: f64 = 0.5;

/// Pearson correlation between every pair of node judgment vectors (dense rows).
/// Constant rows have undefined correlation and are treated as 0.
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

/// Clusters nodes into connected components of the correlation graph, joining any
/// pair with `|ρ| ≥ threshold`. Returns a cluster id per node.
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

/// Total weight a group contributes: `min(Σ w, (Σ w)^α)`. The `min` enforces
/// INV-14 — the transform is a *discount*, never a boost: when `Σ w < 1` the raw
/// power `(Σ w)^α` exceeds `Σ w` (e.g. `0.25^0.5 = 0.5`), so it is capped at `Σ w`.
pub fn sublinear_group_weight(group_weights: &[f64], alpha: f64) -> f64 {
    let sum = group_weights.iter().sum::<f64>();
    powf(sum, alpha).min(sum)
}

/// Per-node discounted weights: each cluster's total is shrunk to `(Σ w)^α` and
/// split back across its members in proportion to their raw weight. The per-node
/// multiplier `s^{α−1}` is capped at 1 (INV-14): the discount MUST NOT increase any
/// node's weight, so a cluster whose total weight is below 1 — a singleton honest
/// node with `E_u ∈ (0,1)`, in particular — is left untouched instead of boosted.
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
                // s^{α−1} is the fraction of its raw weight each member keeps; capped
                // at 1 so `s < 1` clusters are never inflated (INV-14).
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
