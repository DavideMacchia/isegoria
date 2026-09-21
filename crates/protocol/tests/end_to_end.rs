//! End-to-end lifecycle walk over the ten civic items of the oracle fixtures,
//! exercising all four crates together: a unique author enrolls (`identity`), drafts
//! are content-addressed onto the transparency log (`network`), Level A bridging
//! gates them and Level B pilots decide (`scoring`), orchestrated by `protocol`.
//!
//! The load-bearing claim is that each item is stopped at the RIGHT stage:
//! - the ESM item (DIF) passes bridging and dies in the pilot's DIF stage, not in review;
//! - "capital of Italy" (no discrimination) dies in the pilot's screen;
//! - a true-but-divisive item, wrongly rejected by bridging for polarization, is
//!   recovered through the appeal channel because the evidence vindicates it.

// Identity enrollment, the transparency log and deposit are exercised only by the
// full-epoch walk, which is calibration-only (its DIF stage is Variant 1, docs/01 D20).
#[cfg(feature = "calibration")]
use identity::credential::Credential;
#[cfg(feature = "calibration")]
use identity::enrollment::{Cie, DuplicateEnrollment, EnrollmentRegistry, Spid, VoprfOracle};
#[cfg(feature = "calibration")]
use identity::nym::Role;
#[cfg(feature = "calibration")]
use network::log::TransparencyLog;
use protocol::aggregate::{
    aggregate_pass_probability, resolve_band, review_weights, DECISION_THRESHOLD,
};
#[cfg(feature = "calibration")]
use protocol::deposit::{deposit, Draft};
use protocol::gate::{bridging_gate, GateOutcome};
use protocol::pilot::stage1_screen;
#[cfg(feature = "calibration")]
use protocol::pilot::stage2_dif;
use protocol::probation::N_PROBATION;
use protocol::revalidation::revalidate_pool_latent;
use scoring::bridging::{bridge_scores, fit, BridgingParams, Ratings};
use scoring::irt::theta_from_anchors;
#[cfg(feature = "calibration")]
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

const TAU: f64 = 0.08;
const EPS: f64 = 0.008;
const APPEAL_THRESHOLD: f64 = 0.5;

// Items that reach the pool under the docs-faithful retention criteria (both
// r_pbis >= 0.20 AND 2PL a >= 0.6): 01 and 07 (0-indexed 0 and 6).
// The full-epoch expectations below are exercised only by the calibration-mode tests
// (the attribute-DIF stage is Variant 1, docs/01 D20), so they are gated with them.
#[cfg(feature = "calibration")]
const EXPECTED_POOL: [usize; 2] = [0, 6];
#[cfg(feature = "calibration")]
const ESM: usize = 3; // DIF, must be stopped in Level B (not A)
const CAPITAL: usize = 4; // no discrimination, dies in pilot stage 1
#[cfg(feature = "calibration")]
const WRONG_KEY: usize = 5; // negative point-biserial, dies in the pilot
#[cfg(feature = "calibration")]
const REAL_HEALTH: usize = 2; // true-but-divisive: rejected by bridging, saved by appeal
#[cfg(feature = "calibration")]
const CONSTITUTIONAL: usize = 1; // hard item with a guessing floor; see the pool test

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../scoring/tests/fixtures")
}

fn read_matrix(name: &str) -> Vec<Vec<f64>> {
    let text = fs::read_to_string(fixtures_dir().join(name)).unwrap();
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.split(',').map(|c| c.trim().parse().unwrap()).collect())
        .collect()
}

fn read_vector(name: &str) -> Vec<f64> {
    read_matrix(name).into_iter().map(|r| r[0]).collect()
}

fn column(m: &[Vec<f64>], j: usize) -> Vec<f64> {
    m.iter().map(|row| row[j]).collect()
}

/// Cluster only near-identical raters (a real cartel), not honest reviewers who merely
/// share a side of the latent axis.
///
/// Fixture artefact (docs/08 COLLUSION-003): the judgment vectors handed to the
/// correlation step below are the *dense* rows of `R.csv`, which the sim generates for
/// every cell, observed or not — the `mask` is applied to the probabilities but not to
/// the vectors. Real ratings are sparse (~9 per reviewer, `k` per item), where Pearson
/// correlation between two reviewers is undefined or meaningless; nothing here shows
/// the discount works in that regime.
const SUPP_CORR: f64 = 0.95;

/// Resolve a bridging-band item `j` with the reviewers who actually rated it: their
/// rating `R[u][j]` is their pass-probability, weighted (established, unit E_u — the
/// fixtures carry no per-reviewer E_u) and discounted for collusion over their full
/// rating rows. Advances iff the weighted probability reaches the decision threshold;
/// an undecided panel (no eligible weight) does not advance.
fn resolve_supplementary(r_dense: &[Vec<f64>], mask: &[Vec<f64>], j: usize) -> bool {
    let mut probs = Vec::new();
    let mut vectors = Vec::new();
    for (u, row) in r_dense.iter().enumerate() {
        if mask[u][j] != 0.0 {
            probs.push(row[j]);
            vectors.push(row.clone());
        }
    }
    let n = probs.len();
    let weights = review_weights(
        &vec![false; n],
        &vec![N_PROBATION; n],
        &vec![1.0; n],
        1.0,
        &vectors,
        SUPP_CORR,
    );
    aggregate_pass_probability(&probs, &weights)
        .map(|p| resolve_band(p, DECISION_THRESHOLD))
        .unwrap_or(false)
}

fn load_ratings() -> Ratings {
    let r = read_matrix("R.csv");
    let mask: Vec<Vec<bool>> = read_matrix("mask.csv")
        .iter()
        .map(|row| row.iter().map(|&v| v != 0.0).collect())
        .collect();
    Ratings::from_dense(&r, &mask)
}

/// Runs the pipeline; `appeals` is the set of item indices whose author appeals a
/// polarization rejection to the evidence filter. Returns the pool (item indices).
///
/// Calibration-only: the DIF stage is `stage2_dif` (Variant 1, docs/01 D20), which a
/// production build does not compile. In production the pilot has no attribute-DIF
/// stage; a lone ESM item like this fixture's is caught only by the batched latent
/// re-validation (`revalidate_pool_latent`, exercised by `pool_revalidation_flags_latent_bias`).
#[cfg(feature = "calibration")]
fn run_epoch(appeals: &BTreeSet<usize>) -> BTreeSet<usize> {
    let m = 10;

    // --- identity: one real person enrolls once; a duplicate is refused ---
    let oracle = VoprfOracle::new([7u8; 32]);
    let mut registry = EnrollmentRegistry::new();
    let author_cf = "RSSMRA80A01H501U";
    registry
        .enroll(
            &Cie {
                codice_fiscale: author_cf.into(),
            },
            &oracle,
        )
        .expect("first enrollment");
    assert_eq!(
        registry.enroll(
            &Spid {
                codice_fiscale: author_cf.into()
            },
            &oracle
        ),
        Err(DuplicateEnrollment),
        "same person cannot enroll twice, even via another source"
    );
    let author = Credential::from_secret([42u8; 32]);
    let _propose_nym = author.nym(Role::Propose); // the author acts under a role pseudonym

    // --- network: deposit the ten drafts onto the tamper-evident log ---
    let mut log = TransparencyLog::new();
    let mut item_cid = Vec::with_capacity(m);
    for j in 0..m {
        let draft = Draft {
            item: format!("item {j}").into_bytes(),
            primary_source: b"Gazzetta Ufficiale".to_vec(),
        };
        item_cid.push(deposit(&mut log, &draft).unwrap());
    }
    assert_eq!(log.len(), m);
    assert!(log.verify());
    assert_eq!(
        item_cid
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        m,
        "content addressing gives each draft a distinct id"
    );

    // --- scoring Level A: bridging gate ---
    let ratings = load_ratings();
    let params = BridgingParams::default();
    let bridge = bridge_scores(&ratings, &params, 10, 0.85);
    let f = fit(&ratings, &params);

    // Dense ratings + mask, to resolve the uncertainty band with a weighted,
    // anti-collusion-discounted second look at the same panel (docs/05 [4]/[5]).
    let r_dense = read_matrix("R.csv");
    let mask = read_matrix("mask.csv");

    let mut advancing: Vec<usize> = Vec::new();
    for (j, &b) in bridge.iter().enumerate() {
        match bridging_gate(b, f.f_j[j], TAU, EPS, APPEAL_THRESHOLD) {
            GateOutcome::Pass => advancing.push(j),
            // Inside the band: resolved by the reviewers who rated it, weighted
            // (established, unit E_u — the fixtures carry no per-reviewer E_u) and
            // discounted for collusion, not passed by default.
            GateOutcome::SupplementaryReview => {
                if resolve_supplementary(&r_dense, &mask, j) {
                    advancing.push(j);
                }
            }
            // Rejected for polarization: advances only if the author appeals.
            GateOutcome::AppealEligible if appeals.contains(&j) => advancing.push(j),
            _ => {}
        }
    }

    // --- scoring Level B: two-stage pilot on the advancing items ---
    let theta = theta_from_anchors(&read_matrix("levelb_XA.csv"));
    let grp = read_vector("levelb_grp.csv");
    let x = read_matrix("levelb_X.csv");

    let cols: Vec<Vec<f64>> = advancing.iter().map(|&j| column(&x, j)).collect();
    let keep1 = stage1_screen(&theta, &cols);
    let after1: Vec<usize> = advancing
        .iter()
        .zip(keep1.iter())
        .filter_map(|(&j, &k)| k.then_some(j))
        .collect();

    let cols2: Vec<Vec<f64>> = after1.iter().map(|&j| column(&x, j)).collect();
    let keep2 = stage2_dif(&theta, &grp, &cols2);
    after1
        .iter()
        .zip(keep2.iter())
        .filter_map(|(&j, &k)| k.then_some(j))
        .collect()
}

#[test]
fn the_bridging_band_is_resolved_by_weighted_review_not_by_default() {
    // On these fixtures three items land in the bridging uncertainty band; instead of
    // passing them by default, the panel that rated each one resolves it — weighted and
    // anti-collusion-discounted. Item 6 (a genuine quality item near the threshold) is
    // carried upward on merit and is what keeps it in the pool.
    let ratings = load_ratings();
    let params = BridgingParams::default();
    let bridge = bridge_scores(&ratings, &params, 10, 0.85);
    let f = fit(&ratings, &params);

    let band: Vec<usize> = (0..bridge.len())
        .filter(|&j| {
            matches!(
                bridging_gate(bridge[j], f.f_j[j], TAU, EPS, APPEAL_THRESHOLD),
                GateOutcome::SupplementaryReview
            )
        })
        .collect();
    assert_eq!(band, vec![1, 5, 6], "the bridging band on these fixtures");

    let r_dense = read_matrix("R.csv");
    let mask = read_matrix("mask.csv");
    assert!(
        resolve_supplementary(&r_dense, &mask, 6),
        "item 6 advances on the weighted merit of its reviewers, not by default"
    );
}

#[cfg(feature = "calibration")]
#[test]
fn full_epoch_filters_each_item_at_the_right_stage() {
    let pool = run_epoch(&BTreeSet::new());

    // The clean, cross-cutting quality items reach the pool.
    for good in EXPECTED_POOL {
        assert!(
            pool.contains(&good),
            "clean item {good} missing from {pool:?}"
        );
    }
    // Everything the two filters must stop is absent, each for its own reason:
    // ESM (DIF), capital (no discrimination), wrong key (negative point-biserial),
    // and the un-appealed polarized items 08/09 and real-health.
    for bad in [ESM, CAPITAL, WRONG_KEY, REAL_HEALTH, 7, 8, 9] {
        assert!(!pool.contains(&bad), "item {bad} should not reach the pool");
    }
    // The "constitutional majority" item is a hard item with a guessing floor: fitting
    // a 2PL to 3PL-with-guessing data underestimates its discrimination, so it falls in
    // the pilot's screen (docs/02 B.1 notes 3PL exists for exactly this). It is thus
    // legitimately absent under the current 2PL screen.
    assert!(!pool.contains(&CONSTITUTIONAL));
    assert_eq!(pool, EXPECTED_POOL.into_iter().collect::<BTreeSet<_>>());
}

#[cfg(feature = "calibration")]
#[test]
fn esm_passes_bridging_and_is_stopped_by_dif_not_review() {
    // The ESM item is exactly the scenario the evidence filter exists for: human
    // review does not see the bias, the data does.
    let ratings = load_ratings();
    let params = BridgingParams::default();
    let bridge = bridge_scores(&ratings, &params, 10, 0.85);
    let f = fit(&ratings, &params);
    assert_eq!(
        bridging_gate(bridge[ESM], f.f_j[ESM], TAU, EPS, APPEAL_THRESHOLD),
        GateOutcome::Pass,
        "ESM should pass peer review"
    );

    let theta = theta_from_anchors(&read_matrix("levelb_XA.csv"));
    let grp = read_vector("levelb_grp.csv");
    let x = read_matrix("levelb_X.csv");
    // survives the discrimination screen …
    assert!(stage1_screen(&theta, &[column(&x, ESM)])[0]);
    // … but is caught by the DIF stage.
    assert!(!stage2_dif(&theta, &grp, &[column(&x, ESM)])[0]);
}

#[test]
fn non_discriminating_item_dies_in_the_pilot_screen() {
    let theta = theta_from_anchors(&read_matrix("levelb_XA.csv"));
    let x = read_matrix("levelb_X.csv");
    assert!(
        !stage1_screen(&theta, &[column(&x, CAPITAL)])[0],
        "an item that measures nothing must not survive stage 1"
    );
}

#[cfg(feature = "calibration")]
#[test]
fn appeal_recovers_a_true_but_divisive_item() {
    // Bridging rejects the real-health item for polarization; without appeal it is
    // lost, but the evidence vindicates it, so the appeal channel brings it back.
    let ratings = load_ratings();
    let params = BridgingParams::default();
    let bridge = bridge_scores(&ratings, &params, 10, 0.85);
    let f = fit(&ratings, &params);
    assert_eq!(
        bridging_gate(
            bridge[REAL_HEALTH],
            f.f_j[REAL_HEALTH],
            TAU,
            EPS,
            APPEAL_THRESHOLD
        ),
        GateOutcome::AppealEligible,
        "a polarized item should be appeal-eligible, not a plain reject"
    );

    let without = run_epoch(&BTreeSet::new());
    assert!(!without.contains(&REAL_HEALTH), "lost without an appeal");

    let with = run_epoch(&BTreeSet::from([REAL_HEALTH]));
    assert!(
        with.contains(&REAL_HEALTH),
        "recovered through the appeal channel"
    );
}

#[test]
fn pool_revalidation_flags_latent_bias() {
    // The whole-pool latent-class re-check catches bias on an axis no one observed.
    // The mixture fixture plants 3 of 8 biased items on a hidden (education) axis.
    let theta = read_vector("mixture_batch_theta.csv");
    let responses = read_matrix("mixture_batch_X.csv");
    let flagged = revalidate_pool_latent(&theta, &responses, 0);

    assert!(
        flagged[..3].iter().all(|&f| f),
        "the biased items should be flagged: {flagged:?}"
    );
    assert!(
        flagged[3..].iter().filter(|&&f| f).count() <= 1,
        "clean items should be mostly unflagged: {flagged:?}"
    );
}

#[test]
fn documents_limitation_the_band_tie_break_has_no_cross_axis_requirement() {
    // docs/08 PROTO-012. The tie-break is a weighted mean of the same ratings, not a
    // bridging score. Applied to the items bridging *rejects* for polarization (08, 09)
    // it advances them (unit-weight means 0.59 and 0.71 ≥ 0.5): it carries no
    // cross-axis requirement. It is only ever reached for band items, but for those it
    // is the deciding rule — which is why docs/01 D26 replaces it (roadmap T10/T30).
    let r_dense = read_matrix("R.csv");
    let mask = read_matrix("mask.csv");
    for j in [7usize, 8] {
        assert!(
            resolve_supplementary(&r_dense, &mask, j),
            "partisan item {j} would be advanced by the tie-break"
        );
    }
    // And every band item advances too: on these fixtures "resolved by review" and
    // "passed by default" are not distinguishable (all band means are ≥ 0.83).
    for j in [1usize, 5, 6] {
        assert!(resolve_supplementary(&r_dense, &mask, j), "band item {j}");
    }
}
