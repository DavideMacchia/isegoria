//! The appeal stake is a pseudo-observation inside the author's average (`docs/01` D27,
//! `docs/08` REPUTATION-007, T61): filing escrows a zero that the evidence filter's
//! verdict settles; an author below the floor cannot file.

use identity::nym::Nym;
use network::cid::{cid, Cid};
use protocol::appeal::{appeal_floor, AppealOutcome, AuthorHistory, InsufficientReputation};
use protocol::gate::GateOutcome;
use protocol::lifecycle::{deposit, step, Event, Invalid, RejectReason, State};
use protocol::orchestrator::{review_round, run_item, settle_appeal, ItemVerdicts, Judgment};
use scoring::reputation::AuthorPrior;

/// `run_item` for an item that is not in the band: no extra round, no re-decision.
fn run(reviewed: State, v: &ItemVerdicts) -> Result<State, Invalid> {
    run_item(reviewed, v, None, |_| unreachable!("no band item here"))
}

fn prior() -> AuthorPrior {
    AuthorPrior::default()
}

// ------------------------------ the escrow ------------------------------

#[test]
fn filing_escrows_a_zero_and_lowers_the_reputation_at_once() {
    // A fresh author sits at the prior mean, 2 / (2 + 3) = 0.4: exactly the floor.
    let mut author = AuthorHistory::new();
    let before = author.reputation(&prior());
    assert!((before - 0.4).abs() < 1e-12);
    assert!((appeal_floor(&prior()) - 0.4).abs() < 1e-12);
    assert!(author.covers_stake(&prior()));

    let escrow = author.file_appeal(&prior()).unwrap();
    assert_eq!(author.qualities(), &[0.0]);
    // (2 + 0) / (2 + 3 + 1)
    assert!((author.reputation(&prior()) - 1.0 / 3.0).abs() < 1e-12);

    author.settle(escrow, AppealOutcome::Failed);
    assert_eq!(author.qualities(), &[0.0]);
    assert!((author.reputation(&prior()) - 1.0 / 3.0).abs() < 1e-12);
}

#[test]
fn promotion_replaces_the_escrow_with_the_measured_quality() {
    let mut author = AuthorHistory::new();
    let before = author.reputation(&prior());
    let escrow = author.file_appeal(&prior()).unwrap();
    author.settle(escrow, AppealOutcome::Promoted { quality: 0.9 });
    assert_eq!(author.qualities(), &[0.9]);
    // (2 + 0.9) / 6, above the pre-appeal reputation.
    let after = author.reputation(&prior());
    assert!((after - 2.9 / 6.0).abs() < 1e-12);
    assert!(after > before);
}

#[test]
fn an_author_below_the_floor_cannot_file() {
    let mut author = AuthorHistory::new();
    let escrow = author.file_appeal(&prior()).unwrap();
    author.settle(escrow, AppealOutcome::Failed);
    // 1/3 < 0.4: a failed appeal costs the next one …
    let Err(InsufficientReputation { reputation, floor }) = author.file_appeal(&prior()) else {
        panic!("an author below the floor filed an appeal");
    };
    assert!((reputation - 1.0 / 3.0).abs() < 1e-12);
    assert!((floor - 0.4).abs() < 1e-12);
    assert_eq!(
        author.qualities(),
        &[0.0],
        "a refused filing escrows nothing"
    );
    // … until the evidence restores the average: one good item, (2 + 0 + 1) / 7 ≥ 0.4.
    author.record(1.0, 0.0);
    assert!(author.covers_stake(&prior()));
    assert!(author.file_appeal(&prior()).is_ok());
}

// ------------------------------ the orchestrator ------------------------------

fn item() -> Cid {
    cid(b"item")
}

fn panel() -> Vec<Nym> {
    (1..=9).map(|i| Nym([i; 32])).collect()
}

fn reviewed() -> State {
    let admitted = step(
        deposit(true, true, true, true).unwrap(),
        Event::Admit {
            seed_from_beacon: true,
        },
    )
    .unwrap();
    let judgments: Vec<Judgment> = panel()
        .into_iter()
        .map(|nym| Judgment {
            nym,
            prob: 0.7,
            nonce: [nym.0[0]; 32],
        })
        .collect();
    review_round(admitted, item(), panel(), &judgments).unwrap()
}

/// An appealed polarization rejection that the evidence would vindicate.
fn appealed() -> ItemVerdicts {
    ItemVerdicts {
        gate: GateOutcome::AppealEligible,
        appealed: true,
        appeal_within_window: true,
        author_reputation: 0.6,
        appeal_floor: 0.4,
        enough_respondents: true,
        screen_passed: true,
        dif_passed: true,
        source_verified: false,
        pilot2_batch_size: 8,
        explored: false,
    }
}

#[test]
fn run_item_derives_the_appeal_checks_from_the_verdicts() {
    assert_eq!(run(reviewed(), &appealed()).unwrap(), State::ActivePool);
    assert_eq!(
        run(
            reviewed(),
            &ItemVerdicts {
                appeal_within_window: false,
                ..appealed()
            }
        ),
        Err(Invalid::AppealWindowClosed)
    );
    assert_eq!(
        run(
            reviewed(),
            &ItemVerdicts {
                author_reputation: 0.3,
                ..appealed()
            }
        ),
        Err(Invalid::InsufficientReputation)
    );
    // Exactly at the floor covers the stake.
    assert_eq!(
        run(
            reviewed(),
            &ItemVerdicts {
                author_reputation: 0.4,
                ..appealed()
            }
        )
        .unwrap(),
        State::ActivePool
    );
}

#[test]
fn the_terminal_state_settles_the_escrow() {
    let mut author = AuthorHistory::new();
    author.record(0.9, 6.0);
    let before = author.reputation(&prior());

    // Promoted.
    let escrow = author.file_appeal(&prior()).unwrap();
    let terminal = run(reviewed(), &appealed()).unwrap();
    settle_appeal(&mut author, escrow, &terminal, 0.85);
    assert_eq!(author.qualities(), &[0.9, 0.85]);
    assert!(author.reputation(&prior()) > before);

    // Failed at the DIF stage.
    let mut author = AuthorHistory::new();
    author.record(0.9, 6.0);
    let before = author.reputation(&prior());
    let escrow = author.file_appeal(&prior()).unwrap();
    let terminal = run(
        reviewed(),
        &ItemVerdicts {
            dif_passed: false,
            source_verified: false,
            ..appealed()
        },
    )
    .unwrap();
    assert_eq!(terminal, State::Rejected(RejectReason::Dif));
    settle_appeal(&mut author, escrow, &terminal, 0.85);
    assert_eq!(author.qualities(), &[0.9, 0.0]);
    assert!(author.reputation(&prior()) < before);
}

/// An appeal whose item ends in the contested pool is promoted (D38, `docs/05` [5b]).
#[test]
fn an_appeal_that_ends_in_the_contested_pool_is_promoted() {
    let mut author = AuthorHistory::new();
    author.record(0.9, 6.0);
    let before = author.reputation(&prior());
    let escrow = author.file_appeal(&prior()).unwrap();
    let terminal = run(
        reviewed(),
        &ItemVerdicts {
            dif_passed: false,
            source_verified: true,
            ..appealed()
        },
    )
    .unwrap();
    assert_eq!(terminal, State::Contested);
    settle_appeal(&mut author, escrow, &terminal, 0.85);
    assert_eq!(author.qualities(), &[0.9, 0.85]);
    assert!(author.reputation(&prior()) > before);
}
