//! Batch and sample-size gating (docs/08 INV-8 / PROTO-006 / G-15, T9): a DIF stage is
//! never run on a single item, and each stage needs enough distinct respondents. The
//! per-item DIF math stays available; the `pilot`/`revalidation` batch gates enforce the
//! floors (AT-PRO-02: a batch of one is rejected).
//!
//! The attribute-DIF stage (`dif_batch`) is calibration-only (Variant 1, D20); the
//! production INV-8 gate is on the latent re-check (`revalidate_batch_latent`), tested
//! unconditionally below.

use protocol::pilot::{admit_dif_batch, screen, PilotError, N1_MIN, N2_MIN};
use protocol::revalidation::{revalidate_batch_latent, N_LATENT_MIN};

/// A responses matrix of `n` respondents × `m` items, deterministic and non-degenerate.
fn responses(n: usize, m: usize) -> Vec<Vec<f64>> {
    (0..n)
        .map(|i| (0..m).map(|j| ((i * 7 + j * 3) % 5) as f64 * 0.2).collect())
        .collect()
}

fn theta(n: usize) -> Vec<f64> {
    (0..n).map(|i| (i as f64 / n as f64) - 0.5).collect()
}

// -------------------------------- AT-PRO-02: batch of one --------------------------------

#[test]
fn at_pro_02_the_production_latent_recheck_refuses_one_item() {
    // Enough respondents, but a single item: the production DIF re-check must refuse it.
    let t = theta(N_LATENT_MIN);
    assert_eq!(
        revalidate_batch_latent(&t, &responses(N_LATENT_MIN, 1), 0),
        Err(PilotError::BatchTooSmall { items: 1 })
    );
    // A batch of two is admissible.
    assert!(revalidate_batch_latent(&t, &responses(N_LATENT_MIN, 2), 0).is_ok());
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
    let group: Vec<f64> = (0..N2_MIN).map(|i| (i % 2) as f64).collect();
    assert_eq!(
        dif_batch(&t, &group, &items(1)),
        Err(PilotError::BatchTooSmall { items: 1 })
    );
    assert!(dif_batch(&t, &group, &items(2)).is_ok());
}

// -------------------------------- sample-size floors --------------------------------

#[test]
fn a_stage_below_its_respondent_floor_is_rejected() {
    // Stage 1: one fewer respondent than the screen floor.
    assert_eq!(
        screen(&theta(N1_MIN - 1), &responses(N1_MIN - 1, 3)),
        Err(PilotError::NotEnoughRespondents {
            have: N1_MIN - 1,
            need: N1_MIN
        })
    );
    assert!(screen(&theta(N1_MIN), &responses(N1_MIN, 3)).is_ok());

    // The latent re-check needs the largest sample (§B.6): enough items, too few people.
    assert_eq!(
        revalidate_batch_latent(&theta(N_LATENT_MIN - 1), &responses(N_LATENT_MIN - 1, 8), 0),
        Err(PilotError::NotEnoughRespondents {
            have: N_LATENT_MIN - 1,
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
