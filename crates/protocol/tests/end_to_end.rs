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

use identity::credential::Credential;
use identity::enrollment::{Cie, DuplicateEnrollment, EnrollmentRegistry, ReferenceOracle, Spid};
use identity::nym::Role;
use network::log::TransparencyLog;
use protocol::deposit::{deposit, Draft};
use protocol::gate::{bridging_gate, GateOutcome};
use protocol::pilot::{stage1_screen, stage2_dif};
use protocol::revalidation::revalidate_pool_latent;
use scoring::bridging::{bridge_scores, fit, BridgingParams, Ratings};
use scoring::irt::theta_from_anchors;
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

const TAU: f64 = 0.08;
const EPS: f64 = 0.008;
const APPEAL_THRESHOLD: f64 = 0.5;

// Items that reach the pool under the docs-faithful retention criteria (both
// r_pbis >= 0.20 AND 2PL a >= 0.6): 01 and 07 (0-indexed 0 and 6).
const EXPECTED_POOL: [usize; 2] = [0, 6];
const ESM: usize = 3; // DIF, must be stopped in Level B (not A)
const CAPITAL: usize = 4; // no discrimination, dies in pilot stage 1
const WRONG_KEY: usize = 5; // negative point-biserial, dies in the pilot
const REAL_HEALTH: usize = 2; // true-but-divisive: rejected by bridging, saved by appeal
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
fn run_epoch(appeals: &BTreeSet<usize>) -> BTreeSet<usize> {
    let m = 10;

    // --- identity: one real person enrolls once; a duplicate is refused ---
    let oracle = ReferenceOracle::new([7u8; 32]);
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

    let mut advancing: Vec<usize> = Vec::new();
    for (j, &b) in bridge.iter().enumerate() {
        match bridging_gate(b, f.f_j[j], TAU, EPS, APPEAL_THRESHOLD) {
            // Above the band or inside it (supplementary review, assumed to pass here).
            GateOutcome::Pass | GateOutcome::SupplementaryReview => advancing.push(j),
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
