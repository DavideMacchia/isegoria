//! The coordination detector on model residuals (`docs/01` D39, `docs/08`
//! COLLUSION-002/003/005/006), tested on the paper's cartel dataset
//! (`revisions_collusion.py`, AT-COL-02, AT-COL-07).

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use scoring::bridging::{fit, BridgingParams, Ratings};
use scoring::collusion::{
    cluster_by_correlation, coordination_clusters, correlation_matrix, permutation_p_value,
    CoordinationParams, ResidualHistory, MIN_SHARED_ITEMS,
};

const N: usize = 200;
const N_A: usize = 80;
const M: usize = 60;
const CARTEL: usize = 10;

fn normal(r: &mut ChaCha8Rng) -> f64 {
    let u1: f64 = 1.0 - r.gen::<f64>();
    let u2: f64 = r.gen();
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

/// The paper's long history: ratings `R[u][j]`, the camp of each reviewer (`true_f > 0`)
/// and the cartel's target items.
fn dataset(seed: u64) -> (Vec<Vec<f64>>, Vec<bool>, Vec<usize>) {
    let mut r = ChaCha8Rng::seed_from_u64(seed);
    let true_f: Vec<f64> = (0..N)
        .map(|u| {
            if u < N_A {
                -1.0 + 0.25 * normal(&mut r)
            } else {
                1.0 + 0.25 * normal(&mut r)
            }
        })
        .collect();
    let sev: Vec<f64> = (0..N).map(|_| 0.06 * normal(&mut r)).collect();
    let q: Vec<f64> = (0..M)
        .map(|j| {
            if j < 30 {
                r.gen_range(0.8..0.9)
            } else {
                r.gen_range(0.5..0.6)
            }
        })
        .collect();
    let lean: Vec<f64> = (0..M)
        .map(|j| {
            if j < 30 {
                0.0
            } else if j % 2 == 0 {
                0.8
            } else {
                -0.8
            }
        })
        .collect();
    let mut ratings: Vec<Vec<f64>> = (0..N)
        .map(|u| {
            (0..M)
                .map(|j| {
                    (q[j] + 0.45 * true_f[u] * lean[j] + sev[u] + 0.07 * normal(&mut r))
                        .clamp(0.0, 1.0)
                })
                .collect()
        })
        .collect();
    let targets: Vec<usize> = (0..M).filter(|&j| lean[j] > 0.0).take(5).collect();
    for row in ratings.iter_mut().take(CARTEL) {
        for &j in &targets {
            row[j] = (1.0 + 0.05 * normal(&mut r)).clamp(0.0, 1.0);
        }
    }
    let camp_b: Vec<bool> = true_f.iter().map(|f| *f > 0.0).collect();
    (ratings, camp_b, targets)
}

/// The history of residuals the fit leaves on the whole dataset.
fn history(ratings: &[Vec<f64>]) -> ResidualHistory {
    let mask = vec![vec![true; M]; N];
    let data = Ratings::from_dense(ratings, &mask);
    let fitted = fit(&data, &BridgingParams::default()).unwrap();
    let mut h = ResidualHistory::new(N);
    let ids: Vec<u64> = (0..M as u64).collect();
    h.record_epoch(&data, &fitted, &ids).unwrap();
    h
}

fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len() as f64
}

/// Mean pairwise correlation over the honest same-camp, honest cross-camp and cartel
/// pairs, from `corr(u, v)`.
fn pair_means(camp_b: &[bool], corr: impl Fn(usize, usize) -> f64) -> (f64, f64, f64) {
    let (mut same, mut cross, mut cartel) = (Vec::new(), Vec::new(), Vec::new());
    for u in 0..N {
        for v in (u + 1)..N {
            let c = corr(u, v);
            if u < CARTEL && v < CARTEL {
                cartel.push(c);
            } else if u >= CARTEL && v >= CARTEL {
                if camp_b[u] == camp_b[v] {
                    same.push(c);
                } else {
                    cross.push(c);
                }
            }
        }
    }
    (mean(&same), mean(&cross), mean(&cartel))
}

/// AT-COL-07 / AT-COL-02 (D39): on residuals the cartel stands out and nothing else does
/// — honest reviewers of the same camp are not flagged, opposite camps are not joined,
/// and the cartel is one cluster. Raw correlations put honest same-camp pairs level with it.
#[test]
fn at_col_07_the_residual_detector_flags_the_cartel_and_no_honest_pair() {
    let (ratings, camp_b, _) = dataset(11);
    let raw = correlation_matrix(&ratings);
    let (raw_same, raw_cross, raw_cartel) = pair_means(&camp_b, |u, v| raw[u][v]);
    println!("raw: same camp {raw_same:+.3}, cross camp {raw_cross:+.3}, cartel {raw_cartel:+.3}");
    assert!(
        raw_same > 0.8 && raw_cartel > 0.8 && (raw_same - raw_cartel).abs() < 0.1,
        "raw correlation cannot tell a cartel from a camp"
    );

    let h = history(&ratings);
    let (res_same, res_cross, res_cartel) = pair_means(&camp_b, |u, v| {
        h.pair(u, v, MIN_SHARED_ITEMS).expect("60 shared items").0
    });
    println!(
        "residual: same camp {res_same:+.3}, cross camp {res_cross:+.3}, cartel {res_cartel:+.3}"
    );
    assert!(
        res_same.abs() < 0.1 && res_cross.abs() < 0.1,
        "honest residuals correlate"
    );
    assert!(
        res_cartel > 0.7,
        "the cartel's residuals agree beyond the model"
    );

    let report = coordination_clusters(&h, &CoordinationParams::default());
    let cartel_pairs = report
        .flagged
        .iter()
        .filter(|e| e.u < CARTEL && e.v < CARTEL)
        .count();
    let honest_flagged: Vec<_> = report
        .flagged
        .iter()
        .filter(|e| e.u >= CARTEL || e.v >= CARTEL)
        .collect();
    println!(
        "flagged: {} of 45 cartel pairs, {} pairs with an honest member",
        cartel_pairs,
        honest_flagged.len()
    );
    assert!(
        cartel_pairs >= 40,
        "{cartel_pairs} of 45 cartel pairs flagged"
    );
    assert!(
        honest_flagged.is_empty(),
        "honest pairs flagged: {honest_flagged:?}"
    );
    for e in &report.flagged {
        assert!(e.shared == M && e.rho >= 0.7 && e.p_value <= 0.001, "{e:?}");
    }
    // One cluster: the cartel. Every honest reviewer is a singleton.
    let groups = report.groups();
    assert_eq!(groups.len(), 1, "{groups:?}");
    assert!(
        groups[0].len() >= 9 && groups[0].iter().all(|&u| u < CARTEL),
        "{:?}",
        groups[0]
    );
    for u in CARTEL..N {
        assert_eq!(
            report.clusters[u], u,
            "honest reviewer {u} is not a singleton"
        );
    }
}

/// Connected components over raw correlations, at the threshold that catches the
/// cartel, merge honest reviewers of a camp into one cluster too (COLLUSION-005).
#[test]
fn the_retired_raw_correlation_rule_chains_honest_reviewers_together() {
    let (ratings, _, _) = dataset(11);
    let raw = correlation_matrix(&ratings);
    let clusters = cluster_by_correlation(&raw, 0.84);
    let mut sizes = std::collections::BTreeMap::new();
    for &c in &clusters {
        *sizes.entry(c).or_insert(0usize) += 1;
    }
    let largest = sizes.values().copied().max().unwrap();
    println!("raw rule: largest cluster {largest} of {N}");
    assert!(
        largest > 50,
        "the raw rule chains honest reviewers: largest cluster {largest}"
    );
    assert!(
        (N_A..N).all(|u| clusters[u] == clusters[N_A]),
        "camp B is not one cluster under the raw rule"
    );
}

/// Pairs with fewer than `MIN_SHARED_ITEMS` shared items are never read, whatever their
/// correlation: on the first 20 items alone the cartel is invisible.
#[test]
fn a_pair_below_the_shared_floor_is_never_flagged() {
    let (ratings, _, _) = dataset(11);
    let full = history(&ratings);
    let mut short = ResidualHistory::new(N);
    let mask = vec![vec![true; M]; N];
    let data = Ratings::from_dense(&ratings, &mask);
    let fitted = fit(&data, &BridgingParams::default()).unwrap();
    for o in &data.obs {
        if o.j < 20 {
            let predicted =
                fitted.mu + fitted.b_u[o.u] + fitted.b_j[o.j] + fitted.f_u[o.u] * fitted.f_j[o.j];
            short.record(o.u, o.j as u64, o.r - predicted);
        }
    }
    assert_eq!(short.shared(0, 1), 20);
    assert_eq!(short.pair(0, 1, MIN_SHARED_ITEMS), None);
    let report = coordination_clusters(&short, &CoordinationParams::default());
    assert!(report.flagged.is_empty());
    assert!(report.groups().is_empty());
    assert!(full.pair(0, 1, MIN_SHARED_ITEMS).is_some());
}

/// Histories accumulate across epochs in the design's sparse regime (COLLUSION-003): with
/// nine items per reviewer per epoch, a pair reaches 30 shared items only after many
/// epochs, and the detector reads it then.
#[test]
fn the_history_accumulates_across_epochs() {
    let mut r = ChaCha8Rng::seed_from_u64(5);
    let mut h = ResidualHistory::new(4);
    for epoch in 0..12u64 {
        for k in 0..3u64 {
            let item = epoch * 3 + k;
            let pattern = 0.3 * normal(&mut r);
            h.record(0, item, pattern + 0.05 * normal(&mut r));
            h.record(1, item, pattern + 0.05 * normal(&mut r));
            h.record(2, item, 0.3 * normal(&mut r));
            h.record(3, item, 0.3 * normal(&mut r));
        }
        let read = h.pair(0, 1, MIN_SHARED_ITEMS).is_some();
        assert_eq!(
            read,
            (epoch + 1) * 3 >= MIN_SHARED_ITEMS as u64,
            "epoch {epoch}"
        );
    }
    let report = coordination_clusters(&h, &CoordinationParams::default());
    assert_eq!(report.groups(), vec![vec![0, 1]]);
    assert_eq!(report.clusters, vec![0, 0, 2, 3]);
}

/// The permutation null: a strong correlation on 30 items is essentially never reached
/// by chance; an uncorrelated pair's p-value is large. Seeded, hence reproducible.
#[test]
fn the_permutation_null_is_seeded_and_discriminates() {
    let mut r = ChaCha8Rng::seed_from_u64(9);
    let a: Vec<f64> = (0..30).map(|_| normal(&mut r)).collect();
    let b: Vec<f64> = a.iter().map(|x| x + 0.3 * normal(&mut r)).collect();
    let c: Vec<f64> = (0..30).map(|_| normal(&mut r)).collect();
    let strong = permutation_p_value(&a, &b, 0.9, 999, 1);
    assert!(strong <= 0.002, "p = {strong}");
    let none = permutation_p_value(&a, &c, 0.0, 999, 1);
    assert!(none > 0.2, "p = {none}");
    assert_eq!(
        permutation_p_value(&a, &b, 0.9, 999, 1),
        permutation_p_value(&a, &b, 0.9, 999, 1)
    );
}

/// Griefing (AT-COL-05): an attacker who copies an honest reviewer's ratings gets a pair
/// with them, and nothing more — average linkage keeps the rest of the honest population
/// out, and the pair costs the honest reviewer no weight (D40: only co-assignment).
#[test]
fn a_mimic_forms_a_pair_with_its_target_and_chains_nobody_else() {
    let (mut ratings, _, _) = dataset(11);
    let target = 150;
    ratings.push(ratings[target].clone());
    let mask = vec![vec![true; M]; N + 1];
    let data = Ratings::from_dense(&ratings, &mask);
    let fitted = fit(&data, &BridgingParams::default()).unwrap();
    let mut h = ResidualHistory::new(N + 1);
    let ids: Vec<u64> = (0..M as u64).collect();
    h.record_epoch(&data, &fitted, &ids).unwrap();
    let report = coordination_clusters(&h, &CoordinationParams::default());
    let groups = report.groups();
    assert!(groups.contains(&vec![target, N]), "{groups:?}");
    for g in &groups {
        assert!(
            g == &vec![target, N] || g.iter().all(|&u| u < CARTEL),
            "{g:?}"
        );
    }
}
