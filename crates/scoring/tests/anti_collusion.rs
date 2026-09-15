//! Anti-collusion acceptance tests. See `docs/02`, §Anti-collusion.

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
