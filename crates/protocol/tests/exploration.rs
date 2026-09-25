//! Randomized exploration of gate rejections (`docs/01` D35, T52): a random 5% of the
//! items the gate rejects, drawn from the public beacon, are piloted for measurement only
//! — `Rejected → Explored → the two pilot batches → Measured`, never `ActivePool` — and
//! their outcomes enter the evaluator score at weight `1/ε`, so the score stays proper
//! (AT-REP-06, `scoring/tests/exploration_weights.rs`) and the gate's false-negative rate
//! is measured for the first time. AT-PRO-07: the draw is reproducible from the beacon
//! and cannot be chosen.

use identity::nym::Nym;
use network::cid::Cid;
use network::consortium::Checkpoint;
use protocol::exploration::{
    explore_from_beacon, outcome_of, record_outcome, FalseNegatives, Observation, Scored,
    EXPLORATION_RATE,
};
use protocol::gate::GateOutcome;
use protocol::lifecycle::{deposit, step, Event, Invalid, RejectReason, State, K_MIN};
use protocol::orchestrator::{review_round, run_item, ItemVerdicts, Judgment};
use protocol::probation::{SkillTrack, Status, N_PROBATION};
use protocol::randomness::Beacon;
use scoring::reputation::{difference_score, CusumParams};

/// A beacon over a signed checkpoint whose head is `head` repeated.
fn beacon(head: u8) -> Beacon {
    Beacon::from_checkpoint(&Checkpoint::new([1; 32], [2; 32], 5, [head; 32]))
}

/// AT-PRO-07: the same beacon and slot always give the same decision; over 20,000 slots
/// about 5% are explored; another beacon draws an unrelated set; and the machine refuses
/// an exploration whose seed is not the signed checkpoint's.
#[test]
fn at_pro_07_the_draw_is_reproducible_from_the_beacon_and_not_choosable() {
    const SLOTS: u64 = 20_000;
    let (a, b) = (beacon(1), beacon(9));
    let drawn_a: Vec<bool> = (0..SLOTS)
        .map(|slot| explore_from_beacon(&a, slot, EXPLORATION_RATE))
        .collect();
    let again: Vec<bool> = (0..SLOTS)
        .map(|slot| explore_from_beacon(&a, slot, EXPLORATION_RATE))
        .collect();
    assert_eq!(
        drawn_a, again,
        "the draw is a function of the beacon and the slot"
    );
    let drawn_b: Vec<bool> = (0..SLOTS)
        .map(|slot| explore_from_beacon(&b, slot, EXPLORATION_RATE))
        .collect();
    let share = |v: &[bool]| v.iter().filter(|&&x| x).count() as f64 / SLOTS as f64;
    let both = drawn_a
        .iter()
        .zip(&drawn_b)
        .filter(|(&x, &y)| x && y)
        .count() as f64
        / SLOTS as f64;
    println!(
        "explored: {:.2}% under one beacon, {:.2}% under another, {:.2}% under both",
        100.0 * share(&drawn_a),
        100.0 * share(&drawn_b),
        100.0 * both
    );
    assert!((0.04..=0.06).contains(&share(&drawn_a)));
    assert!((0.04..=0.06).contains(&share(&drawn_b)));
    assert!(drawn_a != drawn_b, "two beacons draw the same set");
    assert!(
        both < 0.01,
        "the two draws are not independent: {both:.4} under both"
    );
    // Not choosable: a rate of zero explores nothing, of one everything — the rate is a
    // protocol constant, the seed the beacon's.
    assert!((0..100).all(|s| !explore_from_beacon(&a, s, 0.0)));
    assert!((0..100).all(|s| explore_from_beacon(&a, s, 1.0)));
    assert_eq!(
        step(
            State::Rejected(RejectReason::Defect),
            Event::Explore {
                seed_from_checkpoint: false
            }
        ),
        Err(Invalid::SeedNotFromCheckpoint)
    );
}

fn nym(i: u8) -> Nym {
    Nym([i; 32])
}

fn item() -> Cid {
    Cid([0xE5; 32])
}

fn reviewed() -> State {
    let admitted = step(
        deposit(true, true, true, true).unwrap(),
        Event::Admit {
            seed_from_checkpoint: true,
        },
    )
    .unwrap();
    let panel: Vec<Nym> = (1..=7).map(nym).collect();
    let judgments: Vec<Judgment> = panel
        .iter()
        .map(|&nym| Judgment {
            nym,
            prob: 0.3,
            nonce: [nym.0[0]; 32],
        })
        .collect();
    review_round(admitted, item(), panel, &judgments).unwrap()
}

fn verdicts(gate: GateOutcome, explored: bool) -> ItemVerdicts {
    ItemVerdicts {
        gate,
        appealed: false,
        appeal_within_window: true,
        author_reputation: 0.6,
        appeal_floor: 0.4,
        enough_respondents: true,
        screen_passed: true,
        dif_passed: true,
        pilot2_batch_size: 8,
        explored,
    }
}

/// The machine's rows: a gate rejection drawn for exploration walks the two pilot batches
/// under the pilot's own floors and ends `Measured`, never in the pool; a pilot rejection
/// cannot be explored (its outcome is already measured); `Measured` is terminal.
#[test]
fn an_explored_rejection_is_measured_and_never_enters_the_pool() {
    let explore = Event::Explore {
        seed_from_checkpoint: true,
    };
    for reason in [
        RejectReason::Defect,
        RejectReason::Polarized,
        RejectReason::Borderline,
    ] {
        let explored = step(State::Rejected(reason), explore.clone()).unwrap();
        assert_eq!(
            explored,
            State::Explored {
                reason,
                screened: false
            }
        );
        assert_eq!(
            step(
                explored.clone(),
                Event::Pilot1Batch {
                    enough_respondents: false,
                    passed: true
                }
            ),
            Err(Invalid::NotEnoughRespondents)
        );
        assert_eq!(
            step(
                explored.clone(),
                Event::Pilot2Batch {
                    batch_size: 8,
                    passed: true
                }
            ),
            Err(Invalid::UnexpectedEvent),
            "stage 2 before the screen"
        );
        assert_eq!(
            step(
                explored.clone(),
                Event::Pilot1Batch {
                    enough_respondents: true,
                    passed: false
                }
            ),
            Ok(State::Measured {
                reason,
                passed: false
            })
        );
        let screened = step(
            explored,
            Event::Pilot1Batch {
                enough_respondents: true,
                passed: true,
            },
        )
        .unwrap();
        assert_eq!(
            screened,
            State::Explored {
                reason,
                screened: true
            }
        );
        assert_eq!(
            step(
                screened.clone(),
                Event::Pilot2Batch {
                    batch_size: K_MIN - 1,
                    passed: true
                }
            ),
            Err(Invalid::BatchTooSmall)
        );
        assert_eq!(
            step(
                screened.clone(),
                Event::Pilot1Batch {
                    enough_respondents: true,
                    passed: true
                }
            ),
            Err(Invalid::UnexpectedEvent),
            "the screen twice"
        );
        for passed in [true, false] {
            let measured = step(
                screened.clone(),
                Event::Pilot2Batch {
                    batch_size: K_MIN,
                    passed,
                },
            )
            .unwrap();
            assert_eq!(measured, State::Measured { reason, passed });
            assert_ne!(measured, State::ActivePool);
            for event in [
                Event::Administer,
                explore.clone(),
                Event::Pilot2Batch {
                    batch_size: 8,
                    passed: true,
                },
                Event::Score {
                    outcome: GateOutcome::Pass,
                },
            ] {
                assert_eq!(
                    step(measured.clone(), event),
                    Err(Invalid::UnexpectedEvent),
                    "Measured is terminal"
                );
            }
        }
        // Explored twice: the draw is once per item.
        assert_eq!(
            step(
                State::Explored {
                    reason,
                    screened: false
                },
                explore.clone()
            ),
            Err(Invalid::UnexpectedEvent)
        );
    }
    for measured_already in [RejectReason::Screen, RejectReason::Dif] {
        assert_eq!(
            step(State::Rejected(measured_already), explore.clone()),
            Err(Invalid::UnexpectedEvent),
            "a pilot rejection's outcome is already known"
        );
    }
    assert_eq!(
        step(State::ActivePool, explore.clone()),
        Err(Invalid::UnexpectedEvent)
    );

    // Through the orchestrator: the verdicts' draw takes a rejected item to `Measured` on
    // the same pilot verdicts, and leaves it `Rejected` when it was not drawn.
    assert_eq!(
        run_item(
            reviewed(),
            &verdicts(GateOutcome::Reject, true),
            None,
            |_| unreachable!()
        ),
        Ok(State::Measured {
            reason: RejectReason::Defect,
            passed: true
        })
    );
    assert_eq!(
        run_item(
            reviewed(),
            &verdicts(GateOutcome::Reject, false),
            None,
            |_| unreachable!()
        ),
        Ok(State::Rejected(RejectReason::Defect))
    );
    assert_eq!(
        run_item(
            reviewed(),
            &ItemVerdicts {
                screen_passed: false,
                ..verdicts(GateOutcome::AppealEligible, true)
            },
            None,
            |_| unreachable!()
        ),
        Ok(State::Measured {
            reason: RejectReason::Polarized,
            passed: false
        })
    );
    assert_eq!(
        run_item(
            reviewed(),
            &ItemVerdicts {
                pilot2_batch_size: 1,
                ..verdicts(GateOutcome::Reject, true)
            },
            None,
            |_| unreachable!()
        ),
        Err(Invalid::BatchTooSmall)
    );
    // An item that passes the gate is unaffected by the draw: the pool, not `Measured`.
    assert_eq!(
        run_item(
            reviewed(),
            &verdicts(GateOutcome::Pass, true),
            None,
            |_| unreachable!()
        ),
        Ok(State::ActivePool)
    );
}

/// The reviewer's track (D33–D36 with D35): an observed outcome enters the mean at
/// `1/π`, a reviewed item whose outcome was not observed enters the denominator only, the
/// count that decides probation is the observed one, and the change detector reads the
/// unweighted stream against its own mean — one explored item never fires it by weight.
#[test]
fn the_track_weighs_an_explored_outcome_by_the_inverse_rate() {
    let params = CusumParams::default();
    let mut track = SkillTrack::new();
    for d in [0.1, 0.2, 0.3] {
        assert!(track.record(d, &params).is_none());
    }
    assert!(track
        .record_observed(0.4, EXPLORATION_RATE, &params)
        .is_none());
    for _ in 0..16 {
        track.record_unobserved();
    }
    assert_eq!(track.scored(), 4);
    assert_eq!(track.reviewed(), 20);
    assert!((track.skill() - (0.6 + 0.4 / EXPLORATION_RATE) / 20.0).abs() < 1e-12);
    assert!((track.reference() - 0.25).abs() < 1e-12);
    assert_eq!(track.status(false), Status::Probation);
    assert_eq!(track.standing(false).judgments_with_outcome, 4);

    // Out of probation on a steady 0.1, an explored item scored −0.5 weighs −10 in the
    // mean but reads −0.5 to the detector: no alarm, the statistic well under `h`.
    let mut track = SkillTrack::new();
    for _ in 0..N_PROBATION {
        track.record(0.1, &params);
    }
    assert_eq!(track.status(false), Status::Established);
    let skill_before = track.skill();
    assert!(track
        .record_observed(-0.5, EXPLORATION_RATE, &params)
        .is_none());
    assert!(track.skill() < skill_before);
    assert!(
        track.statistic() < params.h,
        "statistic {}",
        track.statistic()
    );
    assert_eq!(track.alarms(), 0);

    // The terminal states as the track reads them.
    assert!(matches!(
        outcome_of(&State::ActivePool, EXPLORATION_RATE),
        Scored::Observed(Observation { outcome, inclusion }) if outcome == 1.0 && inclusion == 1.0
    ));
    assert!(matches!(
        outcome_of(&State::Rejected(RejectReason::Screen), EXPLORATION_RATE),
        Scored::Observed(Observation { outcome, inclusion }) if outcome == 0.0 && inclusion == 1.0
    ));
    assert!(matches!(
        outcome_of(
            &State::Measured {
                reason: RejectReason::Defect,
                passed: true
            },
            EXPLORATION_RATE
        ),
        Scored::Observed(Observation { outcome, inclusion })
            if outcome == 1.0 && inclusion == EXPLORATION_RATE
    ));
    assert!(matches!(
        outcome_of(&State::Rejected(RejectReason::Polarized), EXPLORATION_RATE),
        Scored::Unobserved
    ));
    assert!(matches!(
        outcome_of(&State::Pilot1 { appealed: false }, EXPLORATION_RATE),
        Scored::Pending
    ));
    let mut track = SkillTrack::new();
    let score = |o: f64| difference_score(0.8, 0.5, o);
    record_outcome(
        &mut track,
        &State::ActivePool,
        EXPLORATION_RATE,
        score,
        &params,
    );
    record_outcome(
        &mut track,
        &State::Rejected(RejectReason::Defect),
        EXPLORATION_RATE,
        score,
        &params,
    );
    record_outcome(
        &mut track,
        &State::Pilot2 { appealed: false },
        EXPLORATION_RATE,
        score,
        &params,
    );
    assert_eq!((track.scored(), track.reviewed()), (1, 2));
    assert!((track.skill() - score(1.0) / 2.0).abs() < 1e-12);
}

/// The gate's false-negative rate: the share of explored rejections that pass Level B.
#[test]
fn the_gate_s_false_negative_rate_is_recorded() {
    let mut fn_rate = FalseNegatives::default();
    assert_eq!(fn_rate.rate(), None);
    for terminal in [
        State::Measured {
            reason: RejectReason::Polarized,
            passed: true,
        },
        State::Measured {
            reason: RejectReason::Defect,
            passed: false,
        },
        State::Rejected(RejectReason::Defect),
        State::ActivePool,
        State::Measured {
            reason: RejectReason::Borderline,
            passed: true,
        },
    ] {
        fn_rate.record(&terminal);
    }
    assert_eq!((fn_rate.explored, fn_rate.passed), (3, 2));
    assert!((fn_rate.rate().unwrap() - 2.0 / 3.0).abs() < 1e-12);
}
