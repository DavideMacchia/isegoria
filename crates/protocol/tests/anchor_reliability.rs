//! The anchors must be reliable before a latent re-check (`docs/01` D37, `docs/08`
//! DIF-010, T53 — AT-DIF-11, and the diagnostic half of AT-DIF-12).
//!
//! Error in the ability proxy creates latent classes that do not exist (paper Prop. 10):
//! on null batches at N = 6,000 the detector flags clean items when θ comes from 10 or 20
//! anchors (KR-20 0.69 / 0.82, paper Table 6). The production entry point,
//! `revalidate_batch_latent`, therefore takes the anchor responses, refuses a KR-20 below
//! `KR20_MIN` before fitting (`PilotError::UnreliableAnchors`) and derives θ itself, so no
//! caller can hand it an unchecked proxy. The batches below follow the paper's design
//! (`paper/scripts/common.py::dif_generate`): θ ~ N(0, 1); anchors with a ~ U(0.9, 1.6),
//! b ~ N(0, 1); eight trial items with a ~ U(1, 1.5), b ~ N(0, 0.6); a hidden balanced
//! axis shifting the first `n_biased` items by δ = 0.9. Seeded (ChaCha8), so every value
//! below is reproducible; the RNG differs from NumPy's, so the KR-20 values match the
//! paper's table to about ±0.01, not to the digit.

use identity::nym::Nym;
use protocol::admission::NullifierSet;
use protocol::pilot::{admit_anchors, PilotError};
use protocol::revalidation::{
    latent_flags, revalidate_batch_latent, revalidate_pool_latent, N_LATENT_MIN,
};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use scoring::dif::mixture_dif;
use scoring::irt::{kr20, theta_from_anchors, KR20_MIN};

const N: usize = 6000;
const K: usize = 8;

fn normal(r: &mut ChaCha8Rng) -> f64 {
    let u1: f64 = 1.0 - r.gen::<f64>();
    let u2: f64 = r.gen();
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

fn sigmoid(z: f64) -> f64 {
    1.0 / (1.0 + (-z).exp())
}

/// The paper's batch: `(anchors, items)`, both respondents × columns, 0/1.
fn batch(seed: u64, n: usize, n_anchor: usize, n_biased: usize) -> (Vec<Vec<f64>>, Vec<Vec<f64>>) {
    let mut r = ChaCha8Rng::seed_from_u64(seed);
    let a_anchor: Vec<f64> = (0..n_anchor).map(|_| 0.9 + 0.7 * r.gen::<f64>()).collect();
    let b_anchor: Vec<f64> = (0..n_anchor).map(|_| normal(&mut r)).collect();
    let a: Vec<f64> = (0..K).map(|_| 1.0 + 0.5 * r.gen::<f64>()).collect();
    let b: Vec<f64> = (0..K).map(|_| 0.6 * normal(&mut r)).collect();
    let (mut xa, mut x) = (Vec::with_capacity(n), Vec::with_capacity(n));
    for _ in 0..n {
        let theta = normal(&mut r);
        let z = if r.gen::<bool>() { 1.0 } else { -1.0 };
        xa.push(
            (0..n_anchor)
                .map(|j| {
                    (r.gen::<f64>() < sigmoid(a_anchor[j] * (theta - b_anchor[j]))) as i32 as f64
                })
                .collect::<Vec<f64>>(),
        );
        x.push(
            (0..K)
                .map(|j| {
                    let d = if j < n_biased { 0.9 } else { 0.0 };
                    (r.gen::<f64>() < sigmoid(a[j] * (theta - b[j] - d * z))) as i32 as f64
                })
                .collect::<Vec<f64>>(),
        );
    }
    (xa, x)
}

/// `n` admitted respondents (T65); the gate that fills the set from `Respond` proofs is
/// tested in `proto013_respondent_gate.rs`.
fn respondents(n: usize) -> NullifierSet {
    let mut set = NullifierSet::new();
    for i in 0..n {
        let mut id = [0u8; 32];
        id[..8].copy_from_slice(&(i as u64).to_le_bytes());
        set.spend(Nym(id)).unwrap();
    }
    set
}

// -------------------------------- AT-DIF-11 --------------------------------

/// The paper's null batches: with 10 or 20 anchors the re-check is refused before any
/// fit, with the anchors' KR-20 in the error; with 60 it runs and flags nothing.
#[test]
fn at_dif_11_null_batches_with_ten_or_twenty_anchors_are_refused_and_sixty_accepted() {
    let people = respondents(N);
    for (n_anchor, lo, hi) in [(10, 0.62, 0.76), (20, 0.78, 0.86)] {
        let (xa, x) = batch(1300, N, n_anchor, 0);
        match revalidate_batch_latent(&people, &xa, &x, 0) {
            Err(PilotError::UnreliableAnchors { kr20: r, anchors }) => {
                println!("{n_anchor} anchors: KR-20 {r:.3}, refused");
                assert_eq!(anchors, n_anchor);
                assert!((lo..hi).contains(&r), "{n_anchor} anchors: KR-20 = {r:.3}");
                assert_eq!(r, kr20(&xa), "the error reports the engine's KR-20");
                assert!(r < KR20_MIN);
            }
            other => panic!("{n_anchor} anchors: expected UnreliableAnchors, got {other:?}"),
        }
        assert!(matches!(
            admit_anchors(&xa),
            Err(PilotError::UnreliableAnchors { anchors, .. }) if anchors == n_anchor
        ));
    }

    let (xa, x) = batch(1300, N, 60, 0);
    let r = kr20(&xa);
    println!("60 anchors: KR-20 {r:.3}, admitted");
    assert!((0.91..0.95).contains(&r), "60 anchors: KR-20 = {r:.3}");
    let flags =
        revalidate_batch_latent(&people, &xa, &x, 0).expect("reliable anchors admit the batch");
    assert_eq!(flags, vec![false; K], "a null batch raises no flag");
    assert_eq!(admit_anchors(&xa).unwrap(), theta_from_anchors(&xa));
}

/// Why the gate exists (DIF-010, paper Table 6): fed θ from 10 anchors, the ungated
/// detector finds a mixture on a null batch and flags clean items. `documents_limitation`:
/// this is the artefact the gate refuses, not a guarantee; it is removed by the target
/// model of T54, at which point this test is inverted.
#[test]
fn documents_limitation_the_ungated_detector_flags_clean_items_from_ten_anchors() {
    let (xa, x) = batch(1300, N, 10, 0);
    let theta = theta_from_anchors(&xa);
    let flags = revalidate_pool_latent(&theta, &x, 0);
    let false_flags = flags.iter().filter(|&&f| f).count();
    println!(
        "10 anchors (KR-20 {:.3}), ungated: {false_flags} of 8 clean items flagged",
        kr20(&xa)
    );
    assert!(false_flags >= 1, "no clean item flagged: {flags:?}");
}

/// The anchor gate is the last precondition: the item floor, the respondent floor and the
/// shape checks come first (D37: "like the respondent and item floors"), so a batch that
/// fails several is reported by the load-bearing INV-8 rule, as before.
#[test]
fn the_anchor_gate_runs_after_the_floors_and_the_shape_checks() {
    let n = N_LATENT_MIN;
    let (xa, x) = batch(7, n, 10, 0);
    assert!(
        kr20(&xa) < KR20_MIN,
        "the anchors are unreliable by construction"
    );
    let people = respondents(n);

    let one_item: Vec<Vec<f64>> = x.iter().map(|row| row[..1].to_vec()).collect();
    assert_eq!(
        revalidate_batch_latent(&people, &xa, &one_item, 0),
        Err(PilotError::BatchTooSmall { items: 1 })
    );
    assert_eq!(
        revalidate_batch_latent(&respondents(n - 1), &xa[..n - 1], &x[..n - 1], 0),
        Err(PilotError::NotEnoughRespondents {
            have: n - 1,
            need: N_LATENT_MIN
        })
    );
    assert_eq!(
        revalidate_batch_latent(&people, &xa[..n - 1], &x, 0),
        Err(PilotError::RowCountMismatch {
            rows: n - 1,
            respondents: n
        })
    );
    let mut ragged = xa.clone();
    ragged[5].pop();
    assert_eq!(
        revalidate_batch_latent(&people, &ragged, &x, 0),
        Err(PilotError::RowCountMismatch {
            rows: 9,
            respondents: 10
        })
    );
    assert!(matches!(
        revalidate_batch_latent(&people, &xa, &x, 0),
        Err(PilotError::UnreliableAnchors { anchors: 10, .. })
    ));
}

// -------------------------------- AT-DIF-12 (diagnostic) --------------------------------

/// The differential gap is reported as a diagnostic and never decides (D37; paper Table
/// 15). With reliable anchors (60, KR-20 ≈ 0.93) the verdict reads the raw gap and flags
/// exactly the shifted items whether 2, 4 or 6 of 8 are shifted. The differential gap
/// agrees when few items are shifted, cannot separate them at 4 of 8 (the paper: 0.81 on
/// the shifted items against 1.01 on the clean) and inverts in a campaign: with 6 of 8
/// shifted the same way the common shift *is* the campaign, so the shifted items show
/// almost no differential gap and the two clean items a large one. Reading it as the
/// verdict would accuse the clean items; the detector does not.
#[test]
fn at_dif_12_the_differential_gap_is_a_diagnostic_that_inverts_in_a_campaign() {
    for n_biased in [2usize, 4, 6] {
        let (xa, x) = batch(700, N, 60, n_biased);
        let theta = admit_anchors(&xa).expect("reliable anchors");
        let res = mixture_dif(&theta, &x, K, 0);
        let flags = latent_flags(&res);
        let expected: Vec<bool> = (0..K).map(|j| j < n_biased).collect();
        assert_eq!(
            flags, expected,
            "{n_biased} of 8 shifted: dif = {:?}",
            res.dif
        );

        let (shifted, clean) = res.differential_gap.split_at(n_biased);
        let min_shifted = shifted.iter().cloned().fold(f64::INFINITY, f64::min);
        let max_shifted = shifted.iter().cloned().fold(0.0, f64::max);
        let min_clean = clean.iter().cloned().fold(f64::INFINITY, f64::min);
        let max_clean = clean.iter().cloned().fold(0.0, f64::max);
        let gaps = &res.differential_gap;
        println!(
            "{n_biased} of 8 shifted (60 anchors, KR-20 {:.3}): {} classes, dif {:?}, differential {:?}",
            kr20(&xa),
            res.classes,
            res.dif.iter().map(|d| format!("{d:.2}")).collect::<Vec<_>>(),
            gaps.iter().map(|d| format!("{d:.2}")).collect::<Vec<_>>()
        );
        match n_biased {
            2 => assert!(
                min_shifted > 1.0 && max_clean < 0.5,
                "2 of 8: the differential gap should agree with the verdict: {gaps:?}"
            ),
            4 => assert!(
                min_shifted < max_clean,
                "4 of 8: the differential gap is not expected to separate the items: {gaps:?}"
            ),
            _ => assert!(
                max_shifted < 0.5 && min_clean > 1.0,
                "6 of 8: the differential gap should invert in a campaign: {gaps:?}"
            ),
        }
    }
}
