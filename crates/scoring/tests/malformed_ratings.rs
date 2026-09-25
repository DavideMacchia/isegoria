//! The engine rejects malformed ratings instead of panicking (T62, `docs/12` §2.3): an
//! out-of-range index, a wrong weight count, a non-finite rating, a non-finite or
//! negative weight and a duplicate `(u, j)` pair each return a `RatingsError` from `fit`
//! and `bridge_scores` — and no `Ratings` at all makes them panic (a property test over
//! arbitrary values; the `fuzz/bridging` target explores the same entry points for
//! longer on a nightly toolchain).

use proptest::prelude::*;
use scoring::bridging::{bridge_scores, fit, BridgingParams, Obs, Ratings, RatingsError};

/// Three reviewers × two items, every cell observed, unit weights.
fn well_formed() -> Ratings {
    let r = vec![vec![0.9, 0.2], vec![0.8, 0.3], vec![0.7, 0.4]];
    let mask = vec![vec![true; 2]; 3];
    Ratings::from_dense(&r, &mask)
}

/// Both entry points refuse `data` with exactly `err`.
fn both_refuse(data: &Ratings, err: RatingsError) {
    let p = BridgingParams::default();
    assert_eq!(fit(data, &p).map(|_| ()), Err(err));
    assert_eq!(bridge_scores(data, &p, 3, 0.85).map(|_| ()), Err(err));
    assert_eq!(data.validate(), Err(err));
}

#[test]
fn well_formed_ratings_fit() {
    let data = well_formed();
    assert_eq!(data.validate(), Ok(()));
    let p = BridgingParams::default();
    assert_eq!(fit(&data, &p).unwrap().b_j.len(), 2);
    assert_eq!(bridge_scores(&data, &p, 3, 0.85).unwrap().robust.len(), 2);
}

#[test]
fn an_observation_out_of_range_is_an_error() {
    let mut data = well_formed();
    data.obs.push(Obs { u: 3, j: 0, r: 0.5 });
    both_refuse(
        &data,
        RatingsError::IndexOutOfRange {
            u: 3,
            j: 0,
            n: 3,
            m: 2,
        },
    );

    let mut data = well_formed();
    data.obs.push(Obs { u: 0, j: 2, r: 0.5 });
    both_refuse(
        &data,
        RatingsError::IndexOutOfRange {
            u: 0,
            j: 2,
            n: 3,
            m: 2,
        },
    );
}

#[test]
fn a_wrong_weight_count_is_an_error() {
    let data = well_formed().with_weights(vec![1.0, 1.0]);
    both_refuse(
        &data,
        RatingsError::WeightCount {
            expected: 3,
            found: 2,
        },
    );
    let data = well_formed().with_weights(vec![1.0; 4]);
    both_refuse(
        &data,
        RatingsError::WeightCount {
            expected: 3,
            found: 4,
        },
    );
}

#[test]
fn a_non_finite_rating_is_an_error() {
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut data = well_formed();
        data.obs[4].r = bad;
        both_refuse(&data, RatingsError::NonFiniteRating { u: 2, j: 0 });
    }
}

#[test]
fn a_non_finite_or_negative_weight_is_an_error() {
    for bad in [f64::NAN, f64::INFINITY, -1.0, -1e-300] {
        let data = well_formed().with_weights(vec![1.0, bad, 1.0]);
        both_refuse(&data, RatingsError::BadWeight { u: 1 });
    }
    // Zero is a weight (probation): accepted.
    assert_eq!(
        well_formed().with_weights(vec![0.0, 1.0, 1.0]).validate(),
        Ok(())
    );
}

#[test]
fn a_duplicate_pair_is_an_error() {
    let mut data = well_formed();
    data.obs.push(Obs {
        u: 1,
        j: 1,
        r: 0.35,
    });
    both_refuse(&data, RatingsError::DuplicateObservation { u: 1, j: 1 });
}

#[test]
fn the_first_problem_is_reported_in_a_fixed_order() {
    // Weights before observations: a wrong weight count is reported even when an
    // observation is also out of range.
    let mut data = well_formed().with_weights(vec![1.0]);
    data.obs.push(Obs { u: 9, j: 9, r: 0.5 });
    assert_eq!(
        data.validate(),
        Err(RatingsError::WeightCount {
            expected: 3,
            found: 1
        })
    );
}

// ------------------------------ never a panic ------------------------------

/// Any float: ordinary values, but also NaN, the infinities and negatives.
fn any_value() -> impl Strategy<Value = f64> {
    prop_oneof![
        4 => -2.0f64..2.0,
        1 => Just(f64::NAN),
        1 => Just(f64::INFINITY),
        1 => Just(f64::NEG_INFINITY),
    ]
}

/// A `Ratings` built field by field, with no regard for consistency: observation indices
/// that may exceed `n`/`m`, any rating, any number of weights of any value.
fn arbitrary_ratings() -> impl Strategy<Value = Ratings> {
    (0usize..6, 0usize..5).prop_flat_map(|(n, m)| {
        (
            prop::collection::vec((0usize..8, 0usize..7, any_value()), 0..12),
            prop::collection::vec(any_value(), 0..8),
        )
            .prop_map(move |(obs, weights)| Ratings {
                n,
                m,
                obs: obs.into_iter().map(|(u, j, r)| Obs { u, j, r }).collect(),
                weights,
            })
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(192))]

    /// `fit` and `bridge_scores` succeed exactly when `validate` accepts the input, and
    /// never panic.
    #[test]
    fn any_ratings_either_fit_or_return_an_error(data in arbitrary_ratings()) {
        let p = BridgingParams {
            n_starts: 1,
            max_iters: 40,
            ..BridgingParams::default()
        };
        let valid = data.validate().is_ok();
        prop_assert_eq!(fit(&data, &p).is_ok(), valid);
        prop_assert_eq!(bridge_scores(&data, &p, 2, 0.85).is_ok(), valid);
    }
}
