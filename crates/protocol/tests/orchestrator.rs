//! §9.1 item lifecycle state machine: one full valid walk, and every checkable invalid
//! case is rejected (docs/08 §9.1, PC-1).

use identity::nym::Nym;
use network::cid::{cid, Cid};
use protocol::gate::GateOutcome;
use protocol::lifecycle::{deposit, step, Event, Invalid, RejectReason, State, K_MIN};
use protocol::review::commit;

fn nym(i: u8) -> Nym {
    Nym([i; 32])
}

/// The item this panel reviews (the commit-reveal binds to it, INV-12).
fn item() -> Cid {
    cid(b"the item under review")
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
    step(
        s,
        Event::AssignReviewers {
            panel: panel(),
            item: item(),
        },
    )
    .unwrap()
}

/// `InReview` past the commit deadline, so a `Reveal` is in-order.
fn revealing() -> State {
    step(in_review(), Event::CloseCommits).unwrap()
}

/// Every panelist in `who` commits then (once commits close) reveals the same judgment.
fn round_with(who: &[Nym], reveal_too: &[Nym]) -> State {
    let (prob, nonce) = (0.8, [7u8; 32]);
    let mut s = in_review();
    for &n in who {
        let commitment = commit(prob, &nonce, n, item());
        s = step(s, Event::Commit { nym: n, commitment }).unwrap();
    }
    s = step(s, Event::CloseCommits).unwrap();
    for &n in reveal_too {
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
    s
}

/// A round where the whole panel committed and revealed.
fn fully_revealed() -> State {
    round_with(&panel(), &panel())
}

/// The state reached by scoring a full epoch with the given gate outcome.
fn scored(outcome: GateOutcome) -> State {
    step(fully_revealed(), Event::Score { outcome }).unwrap()
}

#[test]
fn a_full_valid_walk_reaches_the_pool_then_retires() {
    let mut s = in_review();
    let (prob, nonce) = (0.8, [7u8; 32]);
    for n in panel() {
        s = step(
            s,
            Event::Commit {
                nym: n,
                commitment: commit(prob, &nonce, n, item()),
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
            source_verified: false,
        },
    )
    .unwrap();
    assert_eq!(s, State::ActivePool);
    s = step(s, Event::Administer).unwrap();
    s = step(
        s,
        Event::Revalidate {
            emerging_dif: true,
            source_verified: false,
        },
    )
    .unwrap();
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
        step(
            admitted.clone(),
            Event::AssignReviewers {
                panel: even,
                item: item()
            }
        ),
        Err(Invalid::PanelSizeInvalid)
    );
    let too_big: Vec<Nym> = (1..=13).map(nym).collect();
    assert_eq!(
        step(
            admitted,
            Event::AssignReviewers {
                panel: too_big,
                item: item()
            }
        ),
        Err(Invalid::PanelSizeInvalid)
    );
}

#[test]
fn a_commit_from_a_non_panel_nym_is_rejected() {
    let c = commit(0.5, &[0u8; 32], nym(99), item());
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
    let c = commit(0.5, &[0u8; 32], nym(1), item());
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
    let c = commit(0.6, &nonce, nym(1), item());
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
            commitment: commit(0.5, &nonce, nym(1), item()),
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

/// Scoring checks the round itself: the caller cannot assert "all revealed" (T33).
#[test]
fn scoring_a_partial_epoch_is_rejected() {
    let score = |s: State| {
        step(
            s,
            Event::Score {
                outcome: GateOutcome::Pass,
            },
        )
    };
    let all = panel();
    // Nobody revealed.
    assert_eq!(score(revealing()), Err(Invalid::PartialEpoch));
    // Everybody committed, one did not reveal.
    assert_eq!(
        score(round_with(&all, &all[..8])),
        Err(Invalid::PartialEpoch)
    );
    // One panelist never committed, so it can never reveal.
    assert_eq!(
        score(round_with(&all[1..], &all[1..])),
        Err(Invalid::PartialEpoch)
    );
    // A single reveal is not a full panel.
    assert_eq!(
        score(round_with(&all, &all[..1])),
        Err(Invalid::PartialEpoch)
    );
    assert!(score(fully_revealed()).is_ok());
}

#[test]
fn a_second_reveal_by_the_same_nym_is_rejected() {
    let s = round_with(&panel(), &[nym(1)]);
    assert_eq!(
        step(
            s,
            Event::Reveal {
                nym: nym(1),
                prob: 0.8,
                nonce: [7u8; 32]
            }
        ),
        Err(Invalid::AlreadyRevealed)
    );
}

/// Seven slots filled by six people is not a panel of seven (T33).
#[test]
fn a_panel_with_a_repeated_nym_is_rejected() {
    let admitted = step(
        deposit(true, true, true, true).unwrap(),
        Event::Admit {
            seed_from_checkpoint: true,
        },
    )
    .unwrap();
    for dup_at in 1..7 {
        let mut p: Vec<Nym> = (1..=7).map(nym).collect();
        p[dup_at] = p[dup_at - 1];
        assert_eq!(
            step(
                admitted.clone(),
                Event::AssignReviewers {
                    panel: p,
                    item: item()
                }
            ),
            Err(Invalid::DuplicatePanelist),
            "duplicate at slot {dup_at}"
        );
    }
    // Repeats far apart are caught too.
    let mut p: Vec<Nym> = (1..=11).map(nym).collect();
    p[10] = p[0];
    assert_eq!(
        step(
            admitted,
            Event::AssignReviewers {
                panel: p,
                item: item()
            }
        ),
        Err(Invalid::DuplicatePanelist)
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
                passed: true,
                source_verified: false,
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

/// The band's extra panel: four nyms outside the first panel.
fn extra_panel() -> Vec<Nym> {
    (20..24).map(nym).collect()
}

/// A band item whose extra round is complete: the extra panel assigned, every member
/// committed and revealed (T60).
fn band_ready() -> State {
    let (prob, nonce) = (0.3, [5u8; 32]);
    let mut s = step(
        scored(GateOutcome::SupplementaryReview),
        Event::AssignExtraReviewers {
            panel: extra_panel(),
        },
    )
    .unwrap();
    for n in extra_panel() {
        s = step(
            s,
            Event::Commit {
                nym: n,
                commitment: commit(prob, &nonce, n, item()),
            },
        )
        .unwrap();
    }
    s = step(s, Event::CloseCommits).unwrap();
    for n in extra_panel() {
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
    s
}

/// The D26 re-decision (T60): pass lifts to the pilot, fail is a borderline reject or,
/// if polarized, appeal-eligible; never a second band.
#[test]
fn a_band_item_is_resolved_by_the_d26_re_decision() {
    let s = band_ready();
    assert!(matches!(
        &s,
        State::SupplementaryReview { extra_panel, reveals, commits_closed: true, .. }
            if extra_panel.len() == 4 && reveals.len() == 4
    ));
    assert_eq!(
        step(s.clone(), Event::Administer),
        Err(Invalid::UnexpectedEvent)
    );
    assert_eq!(
        step(
            s.clone(),
            Event::Resolve {
                outcome: GateOutcome::Pass
            }
        )
        .unwrap(),
        State::Pilot1 { appealed: false }
    );
    assert_eq!(
        step(
            s.clone(),
            Event::Resolve {
                outcome: GateOutcome::Reject
            }
        )
        .unwrap(),
        State::Rejected(RejectReason::Borderline)
    );
    assert_eq!(
        step(
            s.clone(),
            Event::Resolve {
                outcome: GateOutcome::AppealEligible
            }
        )
        .unwrap(),
        State::AppealEligible
    );
    assert_eq!(
        step(
            s,
            Event::Resolve {
                outcome: GateOutcome::SupplementaryReview
            }
        ),
        Err(Invalid::UnexpectedEvent)
    );
}

/// The extra round's rules (D26, T60): the re-decision waits for a complete round of
/// reviewers outside the first panel, under the first round's commit-reveal rules.
#[test]
fn the_band_re_decision_needs_a_complete_extra_round() {
    let band = scored(GateOutcome::SupplementaryReview);
    let resolve = |s: State| {
        step(
            s,
            Event::Resolve {
                outcome: GateOutcome::Pass,
            },
        )
    };
    // No extra reviewers yet: nothing to re-decide on.
    assert_eq!(resolve(band.clone()), Err(Invalid::NoExtraPanel));
    // Commits, closing and reveals wait for the assignment.
    let c = commit(0.5, &[0u8; 32], nym(20), item());
    assert_eq!(
        step(
            band.clone(),
            Event::Commit {
                nym: nym(20),
                commitment: c
            }
        ),
        Err(Invalid::UnexpectedEvent)
    );
    assert_eq!(
        step(band.clone(), Event::CloseCommits),
        Err(Invalid::UnexpectedEvent)
    );
    // The extra panel: one to eleven distinct reviewers, none from the first panel.
    let assign = |s: State, panel: Vec<Nym>| step(s, Event::AssignExtraReviewers { panel });
    assert_eq!(
        assign(band.clone(), Vec::new()),
        Err(Invalid::PanelSizeInvalid)
    );
    assert_eq!(
        assign(band.clone(), (20..32).map(nym).collect()),
        Err(Invalid::PanelSizeInvalid)
    );
    assert_eq!(
        assign(band.clone(), vec![nym(20), nym(21), nym(20)]),
        Err(Invalid::DuplicatePanelist)
    );
    assert_eq!(
        assign(band.clone(), vec![nym(20), nym(1)]),
        Err(Invalid::DuplicatePanelist)
    );
    let assigned = assign(band, extra_panel()).unwrap();
    // Assigned once.
    assert_eq!(
        assign(assigned.clone(), vec![nym(30)]),
        Err(Invalid::UnexpectedEvent)
    );
    // Before anyone revealed, the round is partial.
    assert_eq!(resolve(assigned.clone()), Err(Invalid::PartialEpoch));
    // Only the extra panel commits, once each; a reveal waits for the deadline.
    assert_eq!(
        step(
            assigned.clone(),
            Event::Commit {
                nym: nym(1),
                commitment: commit(0.5, &[0u8; 32], nym(1), item())
            }
        ),
        Err(Invalid::NotInPanel)
    );
    let s = step(
        assigned,
        Event::Commit {
            nym: nym(20),
            commitment: c,
        },
    )
    .unwrap();
    assert_eq!(
        step(
            s.clone(),
            Event::Commit {
                nym: nym(20),
                commitment: c
            }
        ),
        Err(Invalid::AlreadyCommitted)
    );
    assert_eq!(
        step(
            s.clone(),
            Event::Reveal {
                nym: nym(20),
                prob: 0.5,
                nonce: [0u8; 32]
            }
        ),
        Err(Invalid::UnexpectedEvent)
    );
    let s = step(s, Event::CloseCommits).unwrap();
    // A reveal that does not open, or with no commit, is refused as in the first round.
    assert_eq!(
        step(
            s.clone(),
            Event::Reveal {
                nym: nym(20),
                prob: 0.6,
                nonce: [0u8; 32]
            }
        ),
        Err(Invalid::RevealMismatch)
    );
    assert_eq!(
        step(
            s.clone(),
            Event::Reveal {
                nym: nym(21),
                prob: 0.5,
                nonce: [0u8; 32]
            }
        ),
        Err(Invalid::NoCommit)
    );
    // One of four revealed: still partial.
    let s = step(
        s,
        Event::Reveal {
            nym: nym(20),
            prob: 0.5,
            nonce: [0u8; 32],
        },
    )
    .unwrap();
    assert_eq!(resolve(s), Err(Invalid::PartialEpoch));
    // The complete round resolves.
    assert_eq!(
        resolve(band_ready()).unwrap(),
        State::Pilot1 { appealed: false }
    );
}
