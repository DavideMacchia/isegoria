//! The reviewer floor of the latent axis (`docs/02` §A.4, `docs/08` BRIDGE-001, T39):
//! a reviewer with fewer than `n_min = 30` reviews on record does not define the `f`
//! space — its `f_u` is fixed at 0 — and only fills it: its ratings still support `μ`,
//! `b_u` and `b_j`. With every reviewer on the axis the fit is the one it always was,
//! bit for bit.

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use scoring::bridging::{
    bridge_scores, fit, side_balanced, BridgingParams, Obs, Ratings, RatingsError,
};

fn normal(r: &mut ChaCha8Rng) -> f64 {
    let u1: f64 = 1.0 - r.gen::<f64>();
    let u2: f64 = r.gen();
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

/// Two camps of reviewers on an axis, ten items of which four lean one way and four the
/// other; every reviewer rates every item with noise. Returns (ratings, the reviewers'
/// true positions).
fn two_camps(n: usize, seed: u64) -> (Vec<Vec<f64>>, Vec<f64>) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let m = 10;
    let f_u: Vec<f64> = (0..n)
        .map(|u| if u < n / 2 { -1.0 } else { 1.0 } + 0.2 * normal(&mut rng))
        .collect();
    let f_j: Vec<f64> = (0..m)
        .map(|j| match j % 5 {
            0 | 1 => 0.4,
            2 | 3 => -0.4,
            _ => 0.0,
        })
        .collect();
    let r = f_u
        .iter()
        .map(|&fu| {
            (0..m)
                .map(|j| (0.7 + fu * f_j[j] + 0.08 * normal(&mut rng)).clamp(0.0, 1.0))
                .collect()
        })
        .collect();
    (r, f_u)
}

fn dense(r: &[Vec<f64>]) -> Ratings {
    let mask: Vec<Vec<bool>> = r.iter().map(|row| vec![true; row.len()]).collect();
    Ratings::from_dense(r, &mask)
}

/// A reviewer off the axis is placed on it without defining it: the axis and the item
/// levels are bit for bit those of the fit without the reviewer, and its own position is
/// what its ratings say. On the axis, the same three ratings — extreme on the leaning
/// items — would move `f_j` and everyone's position.
#[test]
fn a_reviewer_below_the_floor_is_placed_on_the_axis_it_does_not_define() {
    let (r, _) = two_camps(40, 39);
    let n = r.len();
    let params = BridgingParams::default();
    let base = fit(&dense(&r), &params).unwrap();

    // The sleeper: three ratings, 1.0 on two items leaning one way, 0.0 on one leaning
    // the other — a position the fit would read as extreme.
    let mut obs = dense(&r).obs;
    obs.push(Obs { u: n, j: 0, r: 1.0 });
    obs.push(Obs { u: n, j: 1, r: 1.0 });
    obs.push(Obs { u: n, j: 2, r: 0.0 });
    let with_sleeper = |axis: bool| Ratings {
        n: n + 1,
        m: 10,
        obs: obs.clone(),
        weights: vec![1.0; n + 1],
        axis: {
            let mut a = vec![true; n + 1];
            a[n] = axis;
            a
        },
    };
    let on = fit(&with_sleeper(true), &params).unwrap();
    let off = fit(&with_sleeper(false), &params).unwrap();
    // The same reviewers with the sleeper's ratings removed: the fit an absent sleeper
    // gives (a zero-weight reviewer is the same as an absent one, T42).
    let absent = fit(
        &Ratings {
            obs: obs.iter().copied().filter(|o| o.u != n).collect(),
            ..with_sleeper(true)
        },
        &params,
    )
    .unwrap();
    let bits = |v: &[f64]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
    println!(
        "sleeper on the axis: f_u {:+.3}, max |Δf_j| {:.4}; off the axis: f_u {:+.3}, b_u {:+.3}",
        on.f_u[n],
        on.f_j
            .iter()
            .zip(&base.f_j)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f64::max),
        off.f_u[n],
        off.b_u[n]
    );
    // Off the axis: the space is exactly the one fitted without the sleeper …
    assert_eq!(bits(&off.f_j), bits(&absent.f_j), "the sleeper moved f_j");
    assert_eq!(bits(&off.b_j), bits(&absent.b_j), "the sleeper moved b_j");
    assert_eq!(off.mu.to_bits(), absent.mu.to_bits());
    assert_eq!(
        bits(&off.f_u[..n]),
        bits(&absent.f_u[..n]),
        "the sleeper moved the others"
    );
    // … and the sleeper has the position its ratings say, on the side it leaned to.
    assert!(off.f_u[n].is_finite() && off.b_u[n].is_finite());
    assert!(
        off.f_u[n] * base.f_j[0] > 0.0 && off.f_u[n].abs() > 0.2,
        "placed on the side it leaned to: f_u {:+.3} for f_j[0] {:+.3}",
        off.f_u[n],
        base.f_j[0]
    );
    // On the axis, the same ratings define: `f_j` and the others' positions move.
    assert_ne!(bits(&on.f_j), bits(&absent.f_j));
    assert!(on.f_u[n].abs() > 0.2);
}

/// With every reviewer on the axis the fit and the bridge scores are bit for bit the
/// fit and the scores of a `Ratings` without a mask.
#[test]
fn every_reviewer_on_the_axis_is_the_fit_it_always_was() {
    let (r, _) = two_camps(30, 3);
    let plain = dense(&r);
    let masked = dense(&r).with_axis(vec![true; r.len()]);
    let params = BridgingParams::default();
    let (a, b) = (
        fit(&plain, &params).unwrap(),
        fit(&masked, &params).unwrap(),
    );
    let bits = |v: &[f64]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
    assert_eq!(bits(&a.f_u), bits(&b.f_u));
    assert_eq!(bits(&a.b_j), bits(&b.b_j));
    let (sa, sb) = (
        bridge_scores(&plain, &params, 4, 0.85).unwrap(),
        bridge_scores(&masked, &params, 4, 0.85).unwrap(),
    );
    assert_eq!(bits(&sa.robust), bits(&sb.robust));
    assert_eq!(sa.full.side, sb.full.side);
}

/// The sides and the side averages are formed by the reviewers who define the axis: an
/// off-axis reviewer's prediction enters no side mean, whatever nominal side it carries.
#[test]
fn the_sides_are_formed_by_the_reviewers_who_define_the_axis() {
    let (r, _) = two_camps(30, 5);
    let n = r.len();
    let mut axis = vec![true; n];
    for a in axis.iter_mut().take(6) {
        *a = false;
    }
    let data = dense(&r).with_axis(axis.clone());
    let f = fit(&data, &BridgingParams::default()).unwrap();
    let sides = side_balanced(&f);
    for j in 0..f.b_j.len() {
        let mut sum = [0.0; 2];
        let mut count = [0usize; 2];
        for (u, &on_axis) in axis.iter().enumerate() {
            if !on_axis {
                continue;
            }
            let pred = f.mu + f.b_u[u] + f.b_j[j] + f.f_u[u] * f.f_j[j];
            let k = sides.side[u] as usize;
            sum[k] += pred;
            count[k] += 1;
        }
        assert!(
            count[0] > 0 && count[1] > 0,
            "both sides populated by axis reviewers"
        );
        assert!((sides.side_a[j] - sum[0] / count[0] as f64).abs() < 1e-12);
        assert!((sides.side_b[j] - sum[1] / count[1] as f64).abs() < 1e-12);
    }
    // The off-axis reviewers were placed on the axis: finite positions of their own.
    assert!(f.f_u[..6].iter().all(|v| v.is_finite()));
}

/// A mask of the wrong length is refused before anything is computed.
#[test]
fn an_axis_mask_of_the_wrong_length_is_refused() {
    let (r, _) = two_camps(10, 1);
    let data = dense(&r).with_axis(vec![true; 9]);
    assert_eq!(
        fit(&data, &BridgingParams::default()).err(),
        Some(RatingsError::AxisCount {
            expected: 10,
            found: 9
        })
    );
}
