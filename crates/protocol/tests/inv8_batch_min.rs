//! Batch and sample-size gating (docs/08 INV-8 / PROTO-006 / G-15, T9): a DIF stage is
//! never run on a single item, and each stage needs enough distinct respondents. The
//! per-item DIF math stays available; the `pilot`/`revalidation` batch gates enforce the
//! floors (AT-PRO-02: a batch of one is rejected).
//!
//! The attribute-DIF stage (`dif_batch`) is calibration-only (Variant 1, D20); the
//! production INV-8 gate is on the latent re-check (`revalidate_batch_latent`), tested
//! unconditionally below. That re-check also needs reliable anchors (D37, T53), tested in
//! `anchor_reliability.rs`; here the anchors are a perfect Guttman scale (KR-20 ≈ 0.97).

use identity::nym::Nym;
use protocol::admission::NullifierSet;
use protocol::pilot::{admit_dif_batch, screen, PilotError, N1_MIN, N2_MIN};
use protocol::revalidation::{revalidate_batch_latent, N_LATENT_MIN};

/// A responses matrix of `n` respondents × `m` items, deterministic and non-degenerate.
fn responses(n: usize, m: usize) -> Vec<Vec<f64>> {
    (0..n)
        .map(|i| (0..m).map(|j| ((i * 7 + j * 3) % 5) as f64 * 0.2).collect())
        .collect()
}

/// The same answers item-major: one column of `n` answers per item, as `screen` and
/// `dif_batch` take them.
fn columns(n: usize, m: usize) -> Vec<Vec<f64>> {
    let rows = responses(n, m);
    (0..m)
        .map(|j| rows.iter().map(|r| r[j]).collect())
        .collect()
}

fn theta(n: usize) -> Vec<f64> {
    (0..n).map(|i| (i as f64 / n as f64) - 0.5).collect()
}

/// 40 anchors answered by `n` respondents in a perfect Guttman pattern (respondent `i`
/// gets the first `⌊41·i/n⌋` right): reliable well above `KR20_MIN`, so the D37 gate of
/// the latent re-check admits them and only the floors under test decide.
fn anchors(n: usize) -> Vec<Vec<f64>> {
    (0..n)
        .map(|i| {
            let total = i * 41 / n;
            (0..40).map(|j| if j < total { 1.0 } else { 0.0 }).collect()
        })
        .collect()
}

/// `n` admitted respondents (T65): the floors count this set, not the rows. The gate that
/// fills it from `Respond` proofs is tested in `proto013_respondent_gate.rs`.
fn respondents(n: usize) -> NullifierSet {
    let mut set = NullifierSet::new();
    for i in 0..n {
        let mut id = [0u8; 32];
        id[..8].copy_from_slice(&(i as u64).to_le_bytes());
        set.spend(Nym(id)).unwrap();
    }
    set
}

// -------------------------------- AT-PRO-02: batch of one --------------------------------

#[test]
fn at_pro_02_the_production_latent_recheck_refuses_one_item() {
    // Enough respondents, but a single item: the production DIF re-check must refuse it.
    let xa = anchors(N_LATENT_MIN);
    let people = respondents(N_LATENT_MIN);
    assert_eq!(
        revalidate_batch_latent(&people, &xa, &responses(N_LATENT_MIN, 1), 0),
        Err(PilotError::BatchTooSmall { items: 1 })
    );
    // A batch of two is admissible.
    assert!(revalidate_batch_latent(&people, &xa, &responses(N_LATENT_MIN, 2), 0).is_ok());
}

#[cfg(feature = "calibration")]
#[test]
fn at_pro_02_the_attribute_dif_stage_refuses_one_item() {
    use protocol::pilot::dif_batch;
    // `dif_batch` is item-major: one Vec per item, each of length = respondents.
    let items = |n_items: usize| -> Vec<Vec<f64>> {
        (0..n_items)
            .map(|k| (0..N2_MIN).map(|i| ((i + k) % 2) as f64).collect())
            .collect()
    };
    let t = theta(N2_MIN);
    let people = respondents(N2_MIN);
    let group: Vec<f64> = (0..N2_MIN).map(|i| (i % 2) as f64).collect();
    assert_eq!(
        dif_batch(&people, &t, &group, &items(1)),
        Err(PilotError::BatchTooSmall { items: 1 })
    );
    assert!(dif_batch(&people, &t, &group, &items(2)).is_ok());
}

// -------------------------------- sample-size floors --------------------------------

#[test]
fn a_stage_below_its_respondent_floor_is_rejected() {
    // Stage 1: one fewer admitted respondent than the screen floor.
    let n = N1_MIN - 1;
    assert_eq!(
        screen(&respondents(n), &theta(n), &columns(n, 3)),
        Err(PilotError::NotEnoughRespondents {
            have: n,
            need: N1_MIN
        })
    );
    assert!(screen(&respondents(N1_MIN), &theta(N1_MIN), &columns(N1_MIN, 3)).is_ok());

    // The latent re-check needs the largest sample (§B.6): enough items, too few people.
    let n = N_LATENT_MIN - 1;
    assert_eq!(
        revalidate_batch_latent(&respondents(n), &anchors(n), &responses(n, 8), 0),
        Err(PilotError::NotEnoughRespondents {
            have: n,
            need: N_LATENT_MIN
        })
    );
}

#[test]
fn admit_dif_batch_checks_items_before_respondents() {
    // The item floor is the load-bearing INV-8 rule; it is reported even when the sample
    // is also short, so a batch of one is always a BatchTooSmall.
    assert_eq!(
        admit_dif_batch(1, 0, N2_MIN),
        Err(PilotError::BatchTooSmall { items: 1 })
    );
    assert!(admit_dif_batch(2, N2_MIN, N2_MIN).is_ok());
}
