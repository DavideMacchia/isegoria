//! Differential test functioning of a set of items within one latent fit (`docs/02` §B.7,
//! `docs/01` D38): the unsigned expected-score gap at the worst pair of classes.

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use scoring::dtf::{BadClasses, ClassCurves, DTF_MAX};
use scoring::latent::{latent_dif_with, LatentDif, LatentParams};
use scoring::Convergence;

fn sigmoid(z: f64) -> f64 {
    1.0 / (1.0 + (-z).exp())
}

/// Two classes at `b ∓ lean` per item `(a, b, lean)`, with shares `pi` and means `eta`.
fn two_classes(pi: [f64; 2], eta: [f64; 2], items: &[(f64, f64, f64)]) -> ClassCurves {
    let a: Vec<f64> = items.iter().map(|it| it.0).collect();
    let b0: Vec<f64> = items.iter().map(|it| it.1 - it.2).collect();
    let b1: Vec<f64> = items.iter().map(|it| it.1 + it.2).collect();
    ClassCurves::new(&pi, &eta, &[a.clone(), a], &[b0, b1]).unwrap()
}

/// The statistic computed independently: the same grid, `std` transcendental functions.
fn reference(pi: [f64; 2], eta: [f64; 2], items: &[(f64, f64, f64)]) -> f64 {
    let grid: Vec<f64> = (0..41).map(|q| -5.0 + 10.0 * q as f64 / 40.0).collect();
    let density: Vec<f64> = grid
        .iter()
        .map(|t| {
            (0..2)
                .map(|g| pi[g] * (-0.5 * (t - eta[g]).powi(2)).exp())
                .sum()
        })
        .collect();
    let total: f64 = density.iter().sum();
    grid.iter()
        .zip(&density)
        .map(|(t, d)| {
            let gap: f64 = items
                .iter()
                .map(|(a, b, lean)| sigmoid(a * (t - b + lean)) - sigmoid(a * (t - b - lean)))
                .sum();
            d / total * gap.abs()
        })
        .sum()
}

/// One item has the DTF of its own curves, as `docs/02` §B.7 quotes them.
#[test]
fn a_single_item_has_the_dtf_of_its_curves() {
    let equal = ([0.5, 0.5], [0.0, 0.0]);
    for (b, lean, quoted) in [(0.0, 0.9, 0.409), (2.0, 0.9, 0.208), (0.0, 0.17, 0.081)] {
        let item = [(1.25, b, lean)];
        let dtf = two_classes(equal.0, equal.1, &item).dtf(&[0]).unwrap();
        assert!((dtf - reference(equal.0, equal.1, &item)).abs() < 1e-12);
        assert!((dtf - quoted).abs() < 5e-4, "b = {b}, lean {lean}: {dtf}");
    }
    let skewed = ([0.7, 0.3], [0.0, -0.8]);
    let items = [(1.1, 0.3, 0.6), (1.4, -0.2, -0.3), (0.9, 1.0, 0.5)];
    let curves = two_classes(skewed.0, skewed.1, &items);
    let dtf = curves.dtf(&[0, 1, 2]).unwrap();
    assert!((dtf - reference(skewed.0, skewed.1, &items)).abs() < 1e-12);
}

/// Mirror items cancel exactly; opposite leaners cancel only where their curves overlap.
#[test]
fn mirror_items_cancel_and_offset_ones_do_not() {
    let equal = ([0.5, 0.5], [0.0, 0.0]);
    let pair =
        |offset: f64| two_classes(equal.0, equal.1, &[(1.25, 0.0, 0.9), (1.25, offset, -0.9)]);
    assert_eq!(pair(0.0).dtf(&[0, 1]), Some(0.0));
    let quoted = [(0.25, 0.036), (0.5, 0.071), (1.0, 0.139)];
    for (offset, want) in quoted {
        let dtf = pair(offset).dtf(&[0, 1]).unwrap();
        assert!((dtf - want).abs() < 5e-4, "offset {offset}: {dtf}");
    }
    assert!(pair(0.5).dtf(&[0, 1]).unwrap() <= DTF_MAX);
    assert!(pair(1.0).dtf(&[0, 1]).unwrap() > DTF_MAX);
    let one = ClassCurves::new(&[1.0], &[0.0], &[vec![1.2]], &[vec![0.4]]).unwrap();
    assert_eq!(one.dtf(&[0]), Some(0.0), "one class has no pair to compare");
    assert_eq!(pair(0.3).dtf(&[]), Some(0.0));
}

/// The worst pair of three classes is read, whatever the order the classes are listed in.
#[test]
fn the_worst_pair_of_classes_is_read_in_any_order() {
    let a = vec![vec![1.2, 1.0]; 3];
    let b = [vec![0.0, 0.3], vec![0.0, 0.3], vec![1.4, 1.2]];
    let curves = ClassCurves::new(&[0.5, 0.3, 0.2], &[0.0, 0.1, -0.2], &a, &b).unwrap();
    let permuted = ClassCurves::new(
        &[0.2, 0.5, 0.3],
        &[-0.2, 0.0, 0.1],
        &a,
        &[b[2].clone(), b[0].clone(), b[1].clone()],
    )
    .unwrap();
    let dtf = curves.dtf(&[0, 1]).unwrap();
    assert!(dtf > 0.4, "class 2 finds both items harder: {dtf}");
    assert!((permuted.dtf(&[0, 1]).unwrap() - dtf).abs() < 1e-12);
    assert_eq!(curves.dtf(&[1, 0]), curves.dtf(&[0, 1]), "the set's order");
}

/// The DTF is bounded by the set's size and subadditive over disjoint sets.
#[test]
fn the_dtf_is_bounded_and_subadditive() {
    let mut rng = ChaCha8Rng::seed_from_u64(55);
    for _ in 0..200 {
        let g = rng.gen_range(1..=4);
        let k = rng.gen_range(1..=6);
        let raw: Vec<f64> = (0..g).map(|_| rng.gen_range(0.1..1.0)).collect();
        let eta: Vec<f64> = (0..g).map(|_| rng.gen_range(-1.0..1.0)).collect();
        let a: Vec<Vec<f64>> = (0..g)
            .map(|_| (0..k).map(|_| rng.gen_range(0.5..2.0)).collect())
            .collect();
        let b: Vec<Vec<f64>> = (0..g)
            .map(|_| (0..k).map(|_| rng.gen_range(-2.5..2.5)).collect())
            .collect();
        let curves = ClassCurves::new(&raw, &eta, &a, &b).unwrap();
        let split = rng.gen_range(0..=k);
        let (left, right): (Vec<usize>, Vec<usize>) = (0..k).partition(|&j| j < split);
        let all: Vec<usize> = (0..k).collect();
        let (whole, l, r) = (
            curves.dtf(&all).unwrap(),
            curves.dtf(&left).unwrap(),
            curves.dtf(&right).unwrap(),
        );
        assert!(whole.is_finite() && (0.0..=k as f64).contains(&whole));
        assert!(whole <= l + r + 1e-12, "{whole} > {l} + {r}");
    }
}

/// A class below the 5% share does not count, and the shares are renormalized over the rest.
#[test]
fn a_class_below_the_share_floor_does_not_count() {
    let items = [(1.2, 0.1, 0.9), (1.0, -0.4, -0.5)];
    let two = two_classes([0.6, 0.4], [0.0, 0.5], &items);
    let fit = |pi: Vec<f64>, eta: Vec<f64>, a: Vec<Vec<f64>>, b: Vec<Vec<f64>>| LatentDif {
        classes: pi.len(),
        non_uniform: false,
        pi,
        eta,
        anchor_a: Vec::new(),
        anchor_b: Vec::new(),
        item_a: a,
        item_b: b,
        dif: vec![0.0; 2],
        a_gap: vec![0.0; 2],
        posterior: Vec::new(),
        bic_gain: 0.0,
        candidates: Vec::new(),
        status: Convergence::Converged,
    };
    let a = vec![vec![1.2, 1.0]; 3];
    let b = vec![vec![-0.8, 0.1], vec![1.0, -0.9], vec![3.0, 3.0]];
    let with_tiny = fit(vec![0.582, 0.388, 0.03], vec![0.0, 0.5, 2.0], a, b);
    let counted = ClassCurves::of(&with_tiny).unwrap();
    assert_eq!(counted.classes(), 2);
    let (x, y) = (counted.dtf(&[0, 1]).unwrap(), two.dtf(&[0, 1]).unwrap());
    assert!((x - y).abs() < 1e-12, "{x} vs {y}");
}

/// Malformed classes are refused; an index past the fit, or repeated, has no DTF.
#[test]
fn malformed_classes_and_sets_are_refused() {
    let a = vec![vec![1.0, 1.0]; 2];
    let b = vec![vec![0.0, 0.5]; 2];
    let ok = ClassCurves::new(&[0.5, 0.5], &[0.0, 0.0], &a, &b).unwrap();
    assert_eq!((ok.classes(), ok.items()), (2, 2));
    assert_eq!(ok.dtf(&[2]), None);
    assert_eq!(ok.dtf(&[1, 1]), None);
    for bad in [
        ClassCurves::new(&[], &[], &[], &[]),
        ClassCurves::new(&[0.5, 0.5], &[0.0], &a, &b),
        ClassCurves::new(&[0.5, 0.5], &[0.0, 0.0], &a[..1], &b),
        ClassCurves::new(&[0.5, 0.5], &[0.0, 0.0], &a, &[vec![0.0], vec![0.0, 0.5]]),
        ClassCurves::new(&[0.5, 0.0], &[0.0, 0.0], &a, &b),
        ClassCurves::new(&[0.5, f64::NAN], &[0.0, 0.0], &a, &b),
        ClassCurves::new(&[0.5, 0.5], &[0.0, f64::INFINITY], &a, &b),
        ClassCurves::new(&[0.5, 0.5], &[0.0, 0.0], &a, &vec![vec![0.0, f64::NAN]; 2]),
    ] {
        assert_eq!(bad, Err(BadClasses));
    }
}

/// On a fitted batch with items leaning both ways, each set's DTF is that of its true curves.
#[test]
fn the_dtf_of_a_fitted_batch_tracks_the_true_curves() {
    let (n, na, k, delta) = (3000, 30, 8, 0.9);
    let mut rng = ChaCha8Rng::seed_from_u64(38);
    let normal = |rng: &mut ChaCha8Rng| {
        let u1: f64 = 1.0 - rng.gen::<f64>();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * rng.gen::<f64>()).cos()
    };
    let aa: Vec<f64> = (0..na).map(|_| rng.gen_range(0.9..1.6)).collect();
    let ba: Vec<f64> = (0..na).map(|_| normal(&mut rng)).collect();
    let a = [1.2, 1.3, 1.1, 1.25, 1.4, 1.0, 1.2, 1.3];
    let b = [0.0, 0.2, -0.5, -0.3, 0.8, -0.2, 0.4, -0.8];
    let lean = [delta, -delta, delta, -delta, 0.0, 0.0, 0.0, 0.0];
    let (mut anchors, mut x) = (Vec::with_capacity(n), Vec::with_capacity(n));
    for _ in 0..n {
        let (t, z) = (normal(&mut rng), if rng.gen::<bool>() { 1.0 } else { -1.0 });
        let bit = |p: f64, rng: &mut ChaCha8Rng| f64::from(rng.gen::<f64>() < p);
        anchors.push(
            (0..na)
                .map(|j| bit(sigmoid(aa[j] * (t - ba[j])), &mut rng))
                .collect(),
        );
        x.push(
            (0..k)
                .map(|j| bit(sigmoid(a[j] * (t - b[j] - lean[j] * z)), &mut rng))
                .collect::<Vec<f64>>(),
        );
    }
    let lp = LatentParams {
        n_starts: 2,
        max_classes: 2,
        ..LatentParams::default()
    };
    let res = latent_dif_with(&anchors, &x, &lp);
    assert_eq!(res.classes, 2, "gaps {:?}", res.dif);
    let fitted = ClassCurves::of(&res).unwrap();
    let truth = |set: &[usize]| {
        let items: Vec<(f64, f64, f64)> = set.iter().map(|&j| (a[j], b[j], lean[j])).collect();
        reference([0.5, 0.5], [0.0, 0.0], &items)
    };
    for set in [&[0][..], &[1], &[0, 1], &[2, 3], &[4, 5, 6, 7]] {
        let (got, want) = (fitted.dtf(set).unwrap(), truth(set));
        println!("{set:?}: fitted {got:.3}, true {want:.3}");
        assert!(
            (got - want).abs() < 0.05,
            "{set:?}: fitted {got}, true {want}"
        );
    }
}
