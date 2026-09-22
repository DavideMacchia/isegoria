//! §9.1 item lifecycle state machine: one full valid walk, and every checkable invalid
//! case is rejected (docs/08 §9.1, PC-1).

use identity::nym::Nym;
use protocol::gate::GateOutcome;
use protocol::lifecycle::{deposit, step, Event, Invalid, RejectReason, State, K_MIN};
use protocol::review::commit;

fn nym(i: u8) -> Nym {
    Nym([i; 32])
}

/// A panel of 9 distinct judge nyms (odd, in [7, 11]).
fn panel() -> Vec<Nym> {
    (1..=9).map(nym).collect()
}

/// A fresh item driven to `InReview` with a full panel.
fn in_review() -> State {
    let s = deposit(true, true, true, true).unwrap();
    let s = step(
        s,
        Event::Admit {
            seed_from_checkpoint: true,
        },
    )
    .unwrap();
    step(s, Event::AssignReviewers { panel: panel() }).unwrap()
}

/// `InReview` past the commit deadline, so a `Reveal` is in-order.
fn revealing() -> State {
    step(in_review(), Event::CloseCommits).unwrap()
}

/// The state reached by scoring a full epoch with the given gate outcome.
fn scored(outcome: GateOutcome) -> State {
    step(
        revealing(),
        Event::Score {
            all_reveals_in: true,
            outcome,
        },
    )
    .unwrap()
}

#[test]
fn a_full_valid_walk_reaches_the_pool_then_retires() {
    let mut s = in_review();
    // Every panelist commits, then reveals a real opening of that commitment.
    let (prob, nonce) = (0.8, [7u8; 32]);
    let c = commit(prob, &nonce);
    for n in panel() {
        s = step(
            s,
            Event::Commit {
                nym: n,
                commitment: c,
            },
        )
        .unwrap();
    }
    s = step(s, Event::CloseCommits).unwrap();
    for n in panel() {
        s = step(
            s,
            Event::Reveal {
                nym: n,
                prob,
                nonce,
            },
        )
        .unwrap();
    }
    s = step(
        s,
        Event::Score {
            all_reveals_in: true,
            outcome: GateOutcome::Pass,
        },
    )
    .unwrap();
    assert_eq!(s, State::Pilot1 { appealed: false });
    s = step(
        s,
        Event::Pilot1Batch {
            enough_respondents: true,
            passed: true,
        },
    )
    .unwrap();
    assert_eq!(s, State::Pilot2 { appealed: false });
    s = step(
        s,
        Event::Pilot2Batch {
            batch_size: 8,
            passed: true,
        },
    )
    .unwrap();
    assert_eq!(s, State::ActivePool);
    s = step(s, Event::Administer).unwrap();
    s = step(s, Event::Revalidate { emerging_dif: true }).unwrap();
    assert!(matches!(s, State::Retired(_)));
}

#[test]
fn an_appeal_recovers_a_polarized_item() {
    let s = scored(GateOutcome::AppealEligible);
    assert_eq!(s, State::AppealEligible);
    let s = step(
        s,
        Event::Appeal {
            within_window: true,
            reputation_covers_stake: true,
        },
    )
    .unwrap();
    assert_eq!(s, State::Pilot1 { appealed: true });
}

// ------------------------- invalid cases (§9.1), MUST be rejected -------------------------

#[test]
fn deposit_guards() {
    assert_eq!(
        deposit(false, true, true, true),
        Err(Invalid::NoPrimarySource)
    );
    assert_eq!(
        deposit(true, false, true, true),
        Err(Invalid::UnprovenIdentity)
    );
    assert_eq!(deposit(true, true, false, true), Err(Invalid::OverQuota));
    assert_eq!(deposit(true, true, true, false), Err(Invalid::DuplicateCid));
}

#[test]
fn a_participant_chosen_lottery_seed_is_rejected() {
    let s = deposit(true, true, true, true).unwrap();
    assert_eq!(
        step(
            s,
            Event::Admit {
                seed_from_checkpoint: false
            }
        ),
        Err(Invalid::SeedNotFromCheckpoint)
    );
}

#[test]
fn an_even_or_out_of_range_panel_is_rejected() {
    let admitted = step(
        deposit(true, true, true, true).unwrap(),
        Event::Admit {
            seed_from_checkpoint: true,
        },
    )
    .unwrap();
    let even: Vec<Nym> = (1..=8).map(nym).collect();
    assert_eq!(
        step(admitted.clone(), Event::AssignReviewers { panel: even }),
        Err(Invalid::PanelSizeInvalid)
    );
    let too_big: Vec<Nym> = (1..=13).map(nym).collect();
    assert_eq!(
        step(admitted, Event::AssignReviewers { panel: too_big }),
        Err(Invalid::PanelSizeInvalid)
    );
}

#[test]
fn a_commit_from_a_non_panel_nym_is_rejected() {
    let c = commit(0.5, &[0u8; 32]);
    assert_eq!(
        step(
            in_review(),
            Event::Commit {
                nym: nym(99),
                commitment: c
            }
        ),
        Err(Invalid::NotInPanel)
    );
}

#[test]
fn a_second_commit_by_the_same_nym_is_rejected() {
    let c = commit(0.5, &[0u8; 32]);
    let s = step(
        in_review(),
        Event::Commit {
            nym: nym(1),
            commitment: c,
        },
    )
    .unwrap();
    assert_eq!(
        step(
            s,
            Event::Commit {
                nym: nym(1),
                commitment: c
            }
        ),
        Err(Invalid::AlreadyCommitted)
    );
}

#[test]
fn a_reveal_without_a_commit_is_rejected() {
    assert_eq!(
        step(
            revealing(),
            Event::Reveal {
                nym: nym(1),
                prob: 0.5,
                nonce: [0u8; 32]
            }
        ),
        Err(Invalid::NoCommit)
    );
}

#[test]
fn a_reveal_that_does_not_open_the_commitment_is_rejected() {
    let nonce = [1u8; 32];
    let c = commit(0.6, &nonce);
    let s = step(
        in_review(),
        Event::Commit {
            nym: nym(1),
            commitment: c,
        },
    )
    .unwrap();
    let s = step(s, Event::CloseCommits).unwrap();
    assert_eq!(
        step(
            s,
            Event::Reveal {
                nym: nym(1),
                prob: 0.7,
                nonce
            }
        ),
        Err(Invalid::RevealMismatch)
    );
}

#[test]
fn an_out_of_range_or_nan_probability_is_rejected() {
    let nonce = [1u8; 32];
    let s = step(
        in_review(),
        Event::Commit {
            nym: nym(1),
            commitment: commit(0.5, &nonce),
        },
    )
    .unwrap();
    let s = step(s, Event::CloseCommits).unwrap();
    assert_eq!(
        step(
            s.clone(),
            Event::Reveal {
                nym: nym(1),
                prob: 1.5,
                nonce
            }
        ),
        Err(Invalid::ProbabilityOutOfRange)
    );
    assert_eq!(
        step(
            s,
            Event::Reveal {
                nym: nym(1),
                prob: f64::NAN,
                nonce
            }
        ),
        Err(Invalid::ProbabilityOutOfRange)
    );
}

#[test]
fn scoring_a_partial_epoch_is_rejected() {
    assert_eq!(
        step(
            revealing(),
            Event::Score {
                all_reveals_in: false,
                outcome: GateOutcome::Pass
            }
        ),
        Err(Invalid::PartialEpoch)
    );
}

#[test]
fn an_appeal_after_the_window_or_without_reputation_is_rejected() {
    assert_eq!(
        step(
            scored(GateOutcome::AppealEligible),
            Event::Appeal {
                within_window: false,
                reputation_covers_stake: true
            }
        ),
        Err(Invalid::AppealWindowClosed)
    );
    assert_eq!(
        step(
            scored(GateOutcome::AppealEligible),
            Event::Appeal {
                within_window: true,
                reputation_covers_stake: false
            }
        ),
        Err(Invalid::InsufficientReputation)
    );
}

#[test]
fn a_pilot1_batch_below_the_respondent_floor_is_rejected() {
    assert_eq!(
        step(
            State::Pilot1 { appealed: false },
            Event::Pilot1Batch {
                enough_respondents: false,
                passed: true
            }
        ),
        Err(Invalid::NotEnoughRespondents)
    );
}

#[test]
fn a_pilot2_batch_of_one_is_rejected() {
    assert_eq!(
        step(
            State::Pilot2 { appealed: false },
            Event::Pilot2Batch {
                batch_size: K_MIN - 1,
                passed: true
            }
        ),
        Err(Invalid::BatchTooSmall)
    );
}

#[test]
fn out_of_order_events_are_rejected() {
    // Reveal before the commit deadline.
    assert_eq!(
        step(
            in_review(),
            Event::Reveal {
                nym: nym(1),
                prob: 0.5,
                nonce: [0u8; 32]
            }
        ),
        Err(Invalid::UnexpectedEvent)
    );
    // A defect reject is terminal.
    assert_eq!(
        step(scored(GateOutcome::Reject), Event::Administer),
        Err(Invalid::UnexpectedEvent)
    );
    assert_eq!(
        scored(GateOutcome::Reject),
        State::Rejected(RejectReason::Defect)
    );
}

#[test]
fn supplementary_review_has_no_forward_transition() {
    // PROTO-008 / roadmap T10+T30: the band's forward transition is deliberately undefined.
    let s = scored(GateOutcome::SupplementaryReview);
    assert_eq!(s, State::SupplementaryReview);
    assert_eq!(step(s, Event::Administer), Err(Invalid::UnexpectedEvent));
}
