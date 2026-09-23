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
use identity::credential::{Credential, Issuer};
#[cfg(feature = "calibration")]
use identity::enrollment::{
    Cie, DuplicateEnrollment, EnrollmentRegistry, Label, Spid, VoprfOracle,
};
#[cfg(feature = "calibration")]
use identity::nullifier;
#[cfg(feature = "calibration")]
use identity::nym::Role;
#[cfg(feature = "calibration")]
use network::log::TransparencyLog;
#[cfg(feature = "calibration")]
use protocol::admission::QuotaLedger;
#[cfg(feature = "calibration")]
use protocol::deposit::{deposit_with_identity, Draft};
#[cfg(feature = "calibration")]
use protocol::gate::{bridging_gate, supplementary_review, GateOutcome};
#[cfg(feature = "calibration")]
use protocol::lifecycle::State;
#[cfg(feature = "calibration")]
use protocol::orchestrator::{run_item, weighted_ratings, ItemVerdicts, ReviewerStanding};
use protocol::pilot::stage1_screen;
#[cfg(feature = "calibration")]
use protocol::pilot::{dif_batch, screen, stage2_dif, DifVerdict, N1_MIN};
use protocol::revalidation::revalidate_pool_latent;
#[cfg(feature = "calibration")]
use scoring::bridging::{bridge_scores, fit, BridgingParams, Ratings};
use scoring::irt::theta_from_anchors;
#[cfg(feature = "calibration")]
use std::collections::BTreeSet;
#[cfg(feature = "calibration")]
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[cfg(feature = "calibration")]
const TAU: f64 = 0.08;
#[cfg(feature = "calibration")]
const EPS: f64 = 0.008;
#[cfg(feature = "calibration")]
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

#[cfg(feature = "calibration")]
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
    // The author holds a committee-issued credential; each deposit proves a `Propose`
    // nullifier bound to that draft (INV-9, T6) — a bare pseudonym cannot propose.
    let issuer = Issuer::new([1u8; 32]);
    let author = Credential::from_secret([42u8; 32]);
    let (req, pending) = author.request_issuance(&Label([3u8; 32]), &issuer.public());
    let author_cred = pending.finalize(issuer.issue(&req).unwrap());

    // --- network: deposit the ten drafts onto the tamper-evident log, identity-gated
    // and rate-limited (INV-9/ID-008). The per-credential epoch quota is set from the
    // author score (here a generous constant; production uses `reputation::proposal_rate`).
    let mut log = TransparencyLog::new();
    let mut quota_ledger = QuotaLedger::new();
    const PROPOSAL_QUOTA: u32 = 32;
    let mut item_cid = Vec::with_capacity(m);
    for j in 0..m {
        let draft = Draft {
            item: format!("item {j}").into_bytes(),
            primary_source: b"Gazzetta Ufficiale".to_vec(),
        };
        let proof = nullifier::prove(
            &author_cred,
            &issuer.public(),
            Role::Propose,
            &draft.content_id().0,
        );
        let (id, _proposer) = deposit_with_identity(
            &mut log,
            &draft,
            &proof,
            &issuer.public(),
            &mut quota_ledger,
            PROPOSAL_QUOTA,
        )
        .unwrap();
        item_cid.push(id);
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

    // --- scoring Level A: bridging gate, on ratings weighted by prior-epoch standing ---
    // The fit consumes per-reviewer weights (T5, BRIDGE-007). This is a bootstrap epoch,
    // so every reviewer seeds as a founder at unit weight — but the weight now comes from
    // the orchestrator (`bridging_weights`), the same reputation path that weights the
    // aggregation, instead of being an implicit `1.0` baked into `Ratings::from_dense`.
    let r_dense = read_matrix("R.csv");
    let mask_bool: Vec<Vec<bool>> = read_matrix("mask.csv")
        .iter()
        .map(|row| row.iter().map(|&v| v != 0.0).collect())
        .collect();
    let standings = vec![ReviewerStanding::founder(); r_dense.len()];
    let ratings = weighted_ratings(&r_dense, &mask_bool, &standings, 1.0);

    let params = BridgingParams::default();
    let bridge = bridge_scores(&ratings, &params, 10, 0.85);
    let f = fit(&ratings, &params);

    // Gate every item and record whether it advances: a straight pass, a band item the
    // D26 re-decision carries (a re-run bridging fit vs the plain threshold τ — a bridging
    // decision, not a vote), or a polarization reject whose author appeals.
    let gate: Vec<GateOutcome> = (0..m)
        .map(|j| bridging_gate(bridge[j], f.f_j[j], TAU, EPS, APPEAL_THRESHOLD))
        .collect();
    let band_advances: Vec<bool> = (0..m)
        .map(|j| {
            matches!(gate[j], GateOutcome::SupplementaryReview)
                && supplementary_review(&ratings, &params, j, TAU) == GateOutcome::Pass
        })
        .collect();
    let advancing: Vec<usize> = (0..m)
        .filter(|&j| {
            matches!(gate[j], GateOutcome::Pass)
                || band_advances[j]
                || (matches!(gate[j], GateOutcome::AppealEligible) && appeals.contains(&j))
        })
        .collect();

    // --- scoring Level B: two-stage pilot on the advancing items, batch/sample-gated ---
    // The pilot runs through `pilot::{screen, dif_batch}` (INV-8, §B.6, T9): the fixtures
    // meet both floors (1500 respondents; ≥ 2 advancing items), so admission succeeds.
    let theta = theta_from_anchors(&read_matrix("levelb_XA.csv"));
    let grp = read_vector("levelb_grp.csv");
    let x = read_matrix("levelb_X.csv");

    let cols: Vec<Vec<f64>> = advancing.iter().map(|&j| column(&x, j)).collect();
    let keep1 = screen(&theta, &cols).expect("stage-1 respondent floor met on the fixtures");
    let screen_passed: HashMap<usize, bool> = advancing
        .iter()
        .copied()
        .zip(keep1.iter().copied())
        .collect();
    let after1: Vec<usize> = advancing
        .iter()
        .zip(keep1.iter())
        .filter_map(|(&j, &k)| k.then_some(j))
        .collect();

    let cols2: Vec<Vec<f64>> = after1.iter().map(|&j| column(&x, j)).collect();
    let keep2 = dif_batch(&theta, &grp, &cols2).expect("stage-2 batch and respondent floors met");
    // Only a clean Pass advances; a Reject or an Undetermined (separated) fit does not.
    let dif_passed: HashMap<usize, bool> = after1
        .iter()
        .copied()
        .zip(keep2.iter().map(|&k| k == DifVerdict::Pass))
        .collect();
    let pilot2_batch_size = after1.len();

    // --- protocol: the lifecycle state machine decides each item (T12) ---
    // Every stage-to-stage transition (gate outcome → pilot entry, pilot verdict → pool
    // or reject) goes through `lifecycle::step`; the pool is exactly the items the
    // machine leaves in `ActivePool`.
    (0..m)
        .filter(|&j| {
            let verdicts = ItemVerdicts {
                item: item_cid[j],
                gate: gate[j],
                appealed: appeals.contains(&j),
                band_advances: band_advances[j],
                enough_respondents: theta.len() >= N1_MIN,
                screen_passed: *screen_passed.get(&j).unwrap_or(&false),
                dif_passed: *dif_passed.get(&j).unwrap_or(&false),
                pilot2_batch_size,
            };
            run_item(&verdicts).unwrap() == State::ActivePool
        })
        .collect()
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
    // … but is caught by the DIF stage (a real DIF rejection, not a separated fit).
    assert_eq!(
        stage2_dif(&theta, &grp, &[column(&x, ESM)])[0],
        DifVerdict::Reject
    );
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
