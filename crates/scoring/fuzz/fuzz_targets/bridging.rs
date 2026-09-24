//! Bridging on arbitrary `Ratings` (T62): `fit` and `bridge_scores` return an error
//! exactly when `Ratings::validate` refuses the input, and never panic. Sizes are bounded
//! so a run explores malformed shapes, not giant allocations.
#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use scoring::bridging::{bridge_scores, fit, BridgingParams, Obs, Ratings};

#[derive(Debug, Arbitrary)]
struct Input {
    n: u8,
    m: u8,
    obs: Vec<(u8, u8, f64)>,
    weights: Vec<f64>,
    seed: u64,
}

fuzz_target!(|input: Input| {
    let data = Ratings {
        n: (input.n % 8) as usize,
        m: (input.m % 6) as usize,
        obs: input
            .obs
            .iter()
            .take(32)
            .map(|&(u, j, r)| Obs {
                u: u as usize,
                j: j as usize,
                r,
            })
            .collect(),
        weights: input.weights.iter().copied().take(16).collect(),
    };
    let p = BridgingParams {
        n_starts: 1,
        max_iters: 30,
        seed: input.seed,
        ..BridgingParams::default()
    };
    let valid = data.validate().is_ok();
    assert_eq!(fit(&data, &p).is_ok(), valid);
    assert_eq!(bridge_scores(&data, &p, 2, 0.85).is_ok(), valid);
});
