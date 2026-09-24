//! Threshold OPRF quorums (ID-003, T44): for any committee shape, anchor and claimed
//! quorum, a label comes back exactly when the quorum is at least `t` distinct members
//! of the committee — and then it is the label every valid quorum gives.
#![no_main]

use identity::oprf::ThresholdOprfOracle;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: (u8, u8, u8, Vec<u32>, Vec<u8>)| {
    let (n, t, seed, quorum, anchor) = input;
    let n = usize::from(n % 7) + 1;
    let t = usize::from(t) % n + 1;
    let oracle = ThresholdOprfOracle::new([seed; 32], n, t);

    let got = oracle.fuzz_label_with_quorum(&anchor, &quorum);
    let mut distinct = quorum.clone();
    distinct.sort_unstable();
    distinct.dedup();
    let valid = distinct.len() == quorum.len()
        && quorum.len() >= t
        && quorum.iter().all(|&i| i >= 1 && i as usize <= n);
    assert_eq!(got.is_some(), valid, "n={n} t={t} quorum={quorum:?}");
    if got.is_some() {
        let first_t: Vec<u32> = (1..=t as u32).collect();
        assert_eq!(got, oracle.fuzz_label_with_quorum(&anchor, &first_t));
    }
});
