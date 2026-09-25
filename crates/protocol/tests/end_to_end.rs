//! End-to-end lifecycle walk over the ten civic items of the oracle fixtures, exercising
//! all four crates together (`identity`, `network`, `scoring`, `protocol`): each item must
//! be stopped at the right stage, or reach the pool.

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
use identity::nym::{Nym, Role};
#[cfg(feature = "calibration")]
use network::log::TransparencyLog;
#[cfg(feature = "calibration")]
use protocol::admission::NullifierSet;
#[cfg(feature = "calibration")]
use protocol::admission::QuotaLedger;
#[cfg(feature = "calibration")]
use protocol::appeal::{appeal_floor, AuthorHistory};
#[cfg(feature = "calibration")]
use protocol::deposit::{deposit_context, deposit_with_identity, Draft};
#[cfg(feature = "calibration")]
use protocol::exploration::FalseNegatives;
#[cfg(feature = "calibration")]
use protocol::gate::{bridging_gate, GateOutcome, APPEAL_GAP, EPS, TAU};
#[cfg(feature = "calibration")]
use protocol::lifecycle::{deposit, step, Event, State};
#[cfg(feature = "calibration")]
use protocol::orchestrator::{
    review_round, run_item, settle_appeal, weighted_ratings, ItemVerdicts, Judgment,
    ReviewerStanding,
};
use protocol::pilot::stage1_screen;
#[cfg(feature = "calibration")]
use protocol::pilot::{
    batch_id, dif_batch, response_context, screen, stage2_dif, submit_response, DifVerdict, N1_MIN,
};
use protocol::revalidation::revalidate_pool_latent;
#[cfg(feature = "calibration")]
use scoring::bridging::{bridge_scores, BridgingParams, Ratings};
use scoring::irt::theta_from_anchors;
#[cfg(feature = "calibration")]
use scoring::reputation::AuthorPrior;
#[cfg(feature = "calibration")]
use std::collections::BTreeSet;
#[cfg(feature = "calibration")]
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Items reaching the pool under the retention criteria of `docs/02` §B.2.
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
const CONSTITUTIONAL: usize = 1; // passes Level A; too flat a 2PL slope (0.47) in the screen

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

/// Runs the pipeline for one epoch. `appeals`: item indices whose author appeals a
/// polarization rejection; `explored`: gate rejections drawn for exploration (D35).
/// Returns the pool, the author's reputation after the epoch, and the false-negative tally.
#[cfg(feature = "calibration")]
fn run_epoch(
    appeals: &BTreeSet<usize>,
    explored: &BTreeSet<usize>,
) -> (BTreeSet<usize>, f64, FalseNegatives) {
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
    // Each deposit proves a `Propose` nullifier bound to the draft and epoch (INV-9): a
    // bare pseudonym cannot propose, and the proof does not outlive its epoch.
    const EPOCH: u64 = 1;
    let issuer = Issuer::new([1u8; 32]);
    let author = Credential::from_secret([42u8; 32]);
    let (req, pending) = author.request_issuance(&Label([3u8; 32]), &issuer.public());
    let author_cred = pending.finalize(issuer.issue(&req).unwrap());

    // --- network: deposit onto the tamper-evident log, identity-gated and rate-limited
    // (INV-9/ID-008); the epoch quota here is a generous placeholder, not the real rate ---
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
            &deposit_context(draft.content_id(), EPOCH),
        );
        let (id, _proposer) = deposit_with_identity(
            &mut log,
            &draft,
            &proof,
            &issuer.public(),
            EPOCH,
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

    // --- scoring Level A: bridging gate, on ratings weighted by prior-epoch standing (T5,
    // BRIDGE-007) — a bootstrap epoch, so every reviewer seeds as a founder at unit weight ---
    let r_dense = read_matrix("R.csv");
    let mask_bool: Vec<Vec<bool>> = read_matrix("mask.csv")
        .iter()
        .map(|row| row.iter().map(|&v| v != 0.0).collect())
        .collect();
    let standings = vec![ReviewerStanding::founder(); r_dense.len()];
    let ratings = weighted_ratings(&r_dense, &mask_bool, &standings, 1.0).unwrap();

    let params = BridgingParams::default();
    let bridge = bridge_scores(&ratings, &params, 10, 0.85).unwrap();

    // Gates every item on its robust side-balanced score and side gap (D32): a pass, a
    // band item the D26 re-decision later resolves, or a polarization reject to appeal.
    let gate: Vec<GateOutcome> = (0..m)
        .map(|j| bridging_gate(bridge.robust[j], bridge.full.gap[j], TAU, EPS, APPEAL_GAP))
        .collect();
    // No fixture item lands in the band on the provisional gate, so the effective outcome
    // is the gate's; the band's extra round is exercised in `supplementary_redecision.rs`.
    assert!(
        gate.iter()
            .all(|g| !matches!(g, GateOutcome::SupplementaryReview)),
        "no fixture item is borderline on the provisional gate"
    );
    let effective: Vec<GateOutcome> = gate.clone();
    let advancing: Vec<usize> = (0..m)
        .filter(|&j| {
            matches!(effective[j], GateOutcome::Pass)
                || (matches!(effective[j], GateOutcome::AppealEligible) && appeals.contains(&j))
        })
        .collect();
    // The gate rejections the exploration draw picked (D35, T52) are piloted alongside
    // the advancing items, for measurement only; `explored` stands for that draw.
    let piloted: Vec<usize> = (0..m)
        .filter(|&j| {
            advancing.contains(&j)
                || (explored.contains(&j)
                    && matches!(
                        effective[j],
                        GateOutcome::Reject | GateOutcome::AppealEligible
                    )
                    && !appeals.contains(&j))
        })
        .collect();

    // --- scoring Level B: two-stage pilot on the advancing items (INV-8, §B.6, T9), via
    // `pilot::{screen, dif_batch}`; the fixtures meet both admission floors ---
    let theta = theta_from_anchors(&read_matrix("levelb_XA.csv"));
    let grp = read_vector("levelb_grp.csv");
    let x = read_matrix("levelb_X.csv");

    // --- identity: every respondent proves a `Respond` nullifier bound to this batch and
    // epoch (INV-9, T65); the floors count the admitted set, so one person cannot fill a sample ---
    let batch = batch_id(&piloted.iter().map(|&j| item_cid[j]).collect::<Vec<_>>());
    let mut respondents = NullifierSet::new();
    for i in 0..theta.len() as u32 {
        let mut secret = [0u8; 32];
        secret[..4].copy_from_slice(&i.to_le_bytes());
        secret[4] = 0xA5;
        let mut label = [0u8; 32];
        label[..4].copy_from_slice(&i.to_le_bytes());
        label[4] = 0x5A;
        let holder = Credential::from_secret(secret);
        let (req, pending) = holder.request_issuance(&Label(label), &issuer.public());
        let cred = pending.finalize(issuer.issue(&req).unwrap());
        let proof = nullifier::prove(
            &cred,
            &issuer.public(),
            Role::Respond,
            &response_context(batch, EPOCH),
        );
        submit_response(&proof, &issuer.public(), batch, EPOCH, &mut respondents)
            .expect("each fixture row is a distinct person");
    }
    assert_eq!(respondents.len(), theta.len());

    let cols: Vec<Vec<f64>> = piloted.iter().map(|&j| column(&x, j)).collect();
    let keep1 =
        screen(&respondents, &theta, &cols).expect("stage-1 respondent floor met on the fixtures");
    let screen_passed: HashMap<usize, bool> =
        piloted.iter().copied().zip(keep1.iter().copied()).collect();
    let after1: Vec<usize> = piloted
        .iter()
        .zip(keep1.iter())
        .filter_map(|(&j, &k)| k.then_some(j))
        .collect();

    let cols2: Vec<Vec<f64>> = after1.iter().map(|&j| column(&x, j)).collect();
    let keep2 = dif_batch(&respondents, &theta, &grp, &cols2)
        .expect("stage-2 batch and respondent floors met");
    // Only a clean Pass advances; a Reject or an Undetermined (separated) fit does not.
    let dif_passed: HashMap<usize, bool> = after1
        .iter()
        .copied()
        .zip(keep2.iter().map(|&k| k == DifVerdict::Pass))
        .collect();
    let pilot2_batch_size = after1.len();

    // --- protocol: each item's review round and every stage transition go through the
    // lifecycle state machine (T12, T33); the pool is exactly what `ActivePool` holds ---
    let reviewed = |j: usize| {
        let admitted = step(
            deposit(true, true, true, true).unwrap(),
            Event::Admit {
                seed_from_checkpoint: true,
            },
        )
        .unwrap();
        let judgments: Vec<Judgment> = (0..r_dense.len())
            .filter(|&u| mask_bool[u][j])
            .take(9)
            .map(|u| Judgment {
                nym: Nym([u as u8; 32]),
                prob: r_dense[u][j],
                nonce: [(u ^ j) as u8; 32],
            })
            .collect();
        let panel = judgments.iter().map(|jd| jd.nym).collect();
        review_round(admitted, item_cid[j], panel, &judgments).unwrap()
    };
    // --- the author's standing: enough accepted items that an appeal's stake is covered;
    // an appeal escrows a zero-quality observation the terminal state settles (D27, T61) ---
    let prior = AuthorPrior::default();
    let mut author = AuthorHistory::new();
    author.record(0.9, 6.0);
    author.record(0.8, 12.0);

    let mut pool = BTreeSet::new();
    let mut false_negatives = FalseNegatives::default();
    for j in 0..m {
        let reputation = author.reputation(&prior);
        let escrow = if appeals.contains(&j) && matches!(effective[j], GateOutcome::AppealEligible)
        {
            Some(
                author
                    .file_appeal(&prior)
                    .expect("the author's standing covers the stake"),
            )
        } else {
            None
        };
        let verdicts = ItemVerdicts {
            gate: gate[j],
            appealed: appeals.contains(&j),
            appeal_within_window: true,
            author_reputation: reputation,
            appeal_floor: appeal_floor(&prior),
            enough_respondents: respondents.len() >= N1_MIN,
            screen_passed: *screen_passed.get(&j).unwrap_or(&false),
            dif_passed: *dif_passed.get(&j).unwrap_or(&false),
            source_verified: false,
            pilot2_batch_size,
            explored: explored.contains(&j),
        };
        let terminal = run_item(reviewed(j), &verdicts, None, |_| {
            unreachable!("no fixture item is in the band")
        })
        .unwrap();
        false_negatives.record(&terminal);
        if let Some(escrow) = escrow {
            // The item's measured quality is its later pool record; 0.8 stands in for it.
            settle_appeal(&mut author, escrow, &terminal, 0.8);
        }
        if terminal == State::ActivePool {
            pool.insert(j);
        }
    }
    (pool, author.reputation(&prior), false_negatives)
}

#[cfg(feature = "calibration")]
#[test]
fn full_epoch_filters_each_item_at_the_right_stage() {
    let (pool, _, _) = run_epoch(&BTreeSet::new(), &BTreeSet::new());

    // The clean, cross-cutting quality items reach the pool.
    for good in EXPECTED_POOL {
        assert!(
            pool.contains(&good),
            "clean item {good} missing from {pool:?}"
        );
    }
    // Everything the two filters must stop is absent, each for its own reason (DIF, no
    // discrimination, negative point-biserial, a flat 2PL slope, or unappealed polarization).
    for bad in [
        CONSTITUTIONAL,
        ESM,
        CAPITAL,
        WRONG_KEY,
        REAL_HEALTH,
        7,
        8,
        9,
    ] {
        assert!(!pool.contains(&bad), "item {bad} should not reach the pool");
    }
    // CONSTITUTIONAL is a 3PL-with-guessing item: fitting a 2PL underestimates its
    // discrimination, so it falls in the screen as `docs/02` B.1 anticipates.
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
    let bridge = bridge_scores(&ratings, &params, 10, 0.85).unwrap();
    assert_eq!(
        bridging_gate(
            bridge.robust[ESM],
            bridge.full.gap[ESM],
            TAU,
            EPS,
            APPEAL_GAP
        ),
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
    let bridge = bridge_scores(&ratings, &params, 10, 0.85).unwrap();
    assert_eq!(
        bridging_gate(
            bridge.robust[REAL_HEALTH],
            bridge.full.gap[REAL_HEALTH],
            TAU,
            EPS,
            APPEAL_GAP
        ),
        GateOutcome::AppealEligible,
        "a polarized item should be appeal-eligible, not a plain reject"
    );

    let (without, reputation_without, _) = run_epoch(&BTreeSet::new(), &BTreeSet::new());
    assert!(!without.contains(&REAL_HEALTH), "lost without an appeal");

    let (with, reputation_with, _) = run_epoch(&BTreeSet::from([REAL_HEALTH]), &BTreeSet::new());
    assert!(
        with.contains(&REAL_HEALTH),
        "recovered through the appeal channel"
    );
    // The stake was escrowed and, the item promoted, replaced by its measured quality: a
    // good observation the author would not have without the appeal (D27, T61).
    assert!(
        reputation_with > reputation_without,
        "reputation {reputation_with:.4} after a successful appeal vs {reputation_without:.4}"
    );
}

/// D35 on the fixtures (T52): the real-health item, a gate rejection, is drawn for
/// exploration and piloted for measurement only — never entering the pool, and never
/// touching the author's standing.
#[cfg(feature = "calibration")]
#[test]
fn an_explored_rejection_is_measured_and_never_pooled() {
    let (pool, reputation, none) = run_epoch(&BTreeSet::new(), &BTreeSet::new());
    assert_eq!(none, FalseNegatives::default());
    assert_eq!(none.rate(), None);
    let (explored_pool, explored_reputation, measured) =
        run_epoch(&BTreeSet::new(), &BTreeSet::from([REAL_HEALTH]));
    assert_eq!(
        explored_pool, pool,
        "an explored item never enters the pool"
    );
    assert!(!explored_pool.contains(&REAL_HEALTH));
    assert_eq!(
        measured,
        FalseNegatives {
            explored: 1,
            passed: 1
        }
    );
    assert_eq!(measured.rate(), Some(1.0));
    assert_eq!(
        explored_reputation, reputation,
        "measurement costs the author nothing"
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
