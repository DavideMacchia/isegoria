//! Anti-collusion acceptance tests. See `docs/02`, §Anti-collusion.

use scoring::bridging::{fit, BridgingParams, Ratings};
use scoring::collusion::{
    cluster_by_correlation, correlation_matrix, discount_weights, sublinear_group_weight, ALPHA,
};

/// Builds `k` identical voters (a cartel) plus `n_indep` distinct independents,
/// each with a non-constant judgment vector over `m` items.
fn cartel_plus_independents(k: usize, n_indep: usize, m: usize) -> Vec<Vec<f64>> {
    let cartel_pattern: Vec<f64> = (0..m).map(|j| ((j * 7) % 5) as f64 / 4.0).collect();
    let mut rows = vec![cartel_pattern; k];
    for i in 0..n_indep {
        // deterministic but mutually distinct patterns
        rows.push(
            (0..m)
                .map(|j| (((j + i) * 13 + i * 3) % 11) as f64 / 10.0)
                .collect(),
        );
    }
    rows
}

#[test]
fn coordinated_block_forms_one_cluster() {
    let judgments = cartel_plus_independents(500, 22, 20);
    let corr = correlation_matrix(&judgments);
    let clusters = cluster_by_correlation(&corr, 0.99);

    // The 500 colluders share one cluster id.
    let cartel_id = clusters[0];
    assert!(
        clusters[..500].iter().all(|&c| c == cartel_id),
        "the cartel should be a single cluster"
    );
    // Independents are not swallowed into the cartel cluster.
    assert!(
        clusters[500..].iter().all(|&c| c != cartel_id),
        "independents must not join the cartel"
    );
}

#[test]
fn five_hundred_coordinated_count_as_about_twenty_two() {
    // docs/02: √500 ≈ 22.36, so a 500-node cartel ≈ 22 independents.
    let cartel = vec![1.0; 500];
    let group = sublinear_group_weight(&cartel, ALPHA);
    assert!((group - 500f64.sqrt()).abs() < 1e-9);
    assert!((group - 22.36).abs() < 0.05, "group weight = {group:.2}");
}

#[test]
fn unit_weight_singletons_are_untouched() {
    let weights = vec![1.0; 22];
    let clusters: Vec<usize> = (0..22).collect(); // all singletons
    let discounted = discount_weights(&weights, &clusters, ALPHA);
    for w in &discounted {
        assert!((w - 1.0).abs() < 1e-9, "singleton weight changed: {w}");
    }
    let total: f64 = discounted.iter().sum();
    assert!((total - 22.0).abs() < 1e-9);
}

#[test]
fn cartel_total_influence_matches_independents() {
    // 500 colluders (one cluster) vs 22 independents (singletons), all unit weight:
    // the cartel's summed discounted influence should be ~22.
    let mut weights = vec![1.0; 522];
    let mut clusters = vec![0usize; 500]; // cartel
    for i in 0..22 {
        weights[500 + i] = 1.0;
        clusters.push(1 + i); // distinct singleton ids
    }
    let discounted = discount_weights(&weights, &clusters, ALPHA);

    let cartel_influence: f64 = discounted[..500].iter().sum();
    let indep_influence: f64 = discounted[500..].iter().sum();
    assert!((cartel_influence - 500f64.sqrt()).abs() < 1e-9);
    assert!((indep_influence - 22.0).abs() < 1e-9);
    assert!(
        (cartel_influence - indep_influence).abs() < 0.5,
        "500 coordinated ({cartel_influence:.2}) should ≈ 22 independent ({indep_influence:.2})"
    );
}

/// Honest reviewers: two polarized camps on the axis items, both rating the target `t`
/// low, plus a small per-reviewer jitter so they stay distinct (singletons, not a
/// cluster).
fn honest_rows(honest: usize, m: usize, t: usize) -> Vec<Vec<f64>> {
    (0..honest)
        .map(|u| {
            let camp = if u % 2 == 0 { 1.0 } else { 0.0 };
            (0..m)
                .map(|j| {
                    if j == t {
                        0.2
                    } else {
                        let base = if j % 2 == 0 { camp } else { 1.0 - camp };
                        let jit = (((u * 31 + j * 17) % 7) as f64 - 3.0) * 0.02;
                        (base + jit).clamp(0.0, 1.0)
                    }
                })
                .collect()
        })
        .collect()
}

/// Near-neutral, cross-cutting endorsers of `t`: `t = 1.0`, other items ≈ 0.5 with a
/// per-reviewer jitter. `distinct` makes each row different (independents) or identical
/// (a cartel).
fn endorsers_of(count: usize, m: usize, t: usize, distinct: bool) -> Vec<Vec<f64>> {
    (0..count)
        .map(|i| {
            let seed = if distinct { i } else { 0 };
            (0..m)
                .map(|j| {
                    if j == t {
                        1.0
                    } else {
                        0.5 + (((seed * 29 + j * 13) % 7) as f64 - 3.0) * 0.03
                    }
                })
                .collect()
        })
        .collect()
}

/// AT-COL-06 / BRIDGE-007 / G-03: weights are consumed by bridging, so a discounted
/// cartel of `k` moves the target's `b_j` less than `k` genuine independents would
/// (√k ≈ 20 for k = 400).
#[test]
fn at_col_06_cartel_moves_the_bridge_score_less_than_independents() {
    let m = 9;
    let t = m - 1;
    let (honest, k) = (120usize, 400usize);
    let params = BridgingParams::default();
    let honest_only = honest_rows(honest, m, t);

    let bj_t = |rows: &[Vec<f64>], weights: Vec<f64>| {
        let mask = vec![vec![true; m]; rows.len()];
        fit(
            &Ratings::from_dense(rows, &mask).with_weights(weights),
            &params,
        )
        .unwrap()
        .b_j[t]
    };

    let b_base = bj_t(&honest_only, vec![1.0; honest]);

    // k genuine independents endorsing t at face value: distinct rows → singletons.
    let mut with_indep = honest_only.clone();
    with_indep.extend(endorsers_of(k, m, t, true));
    let b_independents = bj_t(&with_indep, vec![1.0; with_indep.len()]);

    // k identical colluders endorsing t, anti-collusion discounted.
    let mut with_cartel = honest_only.clone();
    with_cartel.extend(endorsers_of(k, m, t, false));
    let clusters = cluster_by_correlation(&correlation_matrix(&with_cartel), 0.99);
    assert!(
        clusters[honest..].iter().all(|&c| c == clusters[honest]),
        "the cartel should form one cluster"
    );
    let discounted = discount_weights(&vec![1.0; with_cartel.len()], &clusters, ALPHA);
    let b_cartel = bj_t(&with_cartel, discounted);

    // Both push the target up from the honest baseline, but the discounted cartel moves
    // it less than the same number of independents.
    assert!(
        b_independents > b_base,
        "independents should raise b_j: {b_independents:.4} vs {b_base:.4}"
    );
    assert!(
        b_cartel < b_independents,
        "base={b_base:.4} cartel={b_cartel:.4} independents={b_independents:.4}"
    );
}

/// COLLUSION-004 / INV-14 / AT-COL-04: the discount is a discount, never a boost.
/// With real evaluator weights `E_u ∈ (0,1)` a cluster's total is below 1, where the
/// raw multiplier `s^{α−1} > 1` would *inflate* a node (a `0.25` singleton became
/// `0.5`). No discounted weight may exceed its input, at any weight scale.
#[test]
fn discount_never_increases_a_weight() {
    // Singletons across the whole sub-unit range, plus a small honest cluster.
    let weights = vec![0.05, 0.1, 0.25, 0.5, 0.75, 0.9, 0.3, 0.3];
    let clusters = vec![0, 1, 2, 3, 4, 5, 6, 6]; // last two share a cluster (s = 0.6 < 1)
    let discounted = discount_weights(&weights, &clusters, ALPHA);
    for (i, (&w, &d)) in weights.iter().zip(&discounted).enumerate() {
        assert!(d <= w + 1e-12, "node {i}: discounted {d} exceeds input {w}");
        assert!(d >= 0.0);
    }
    // The specific regression: a sub-unit singleton is left untouched, not boosted.
    assert!(
        (discounted[2] - 0.25).abs() < 1e-12,
        "0.25 singleton must stay 0.25"
    );
}

/// A cluster whose members carry `E_u < 1` must not contribute more than its raw
/// total: `sublinear_group_weight` is capped at `Σ w`.
#[test]
fn group_weight_is_capped_at_its_raw_total() {
    for group in [vec![0.25], vec![0.1, 0.2], vec![0.4, 0.5]] {
        let raw: f64 = group.iter().sum();
        let g = sublinear_group_weight(&group, ALPHA);
        assert!(g <= raw + 1e-12, "group {group:?}: {g} exceeds raw {raw}");
    }
    // Above 1 the discount still bites: a big cartel is sublinear as before.
    let big = vec![1.0; 400];
    assert!((sublinear_group_weight(&big, ALPHA) - 400f64.sqrt()).abs() < 1e-9);
}
