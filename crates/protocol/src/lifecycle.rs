//! Item lifecycle state machine (`docs/08` §9.1, `docs/05`). Until now the flow lived
//! only as an imperative sequence inside `tests/end_to_end.rs`; this owns per-item state
//! and **rejects every invalid transition** (§9.1's "invalid cases" column, PC-1).
//!
//! Preconditions that need primitives built by other tasks — a verified identity
//! nullifier (T6), an RLN quota proof (T11), a checkpoint-derived lottery seed (T8) —
//! enter as explicit `bool` inputs the machine checks, so the invalid case is rejected
//! here while the proof itself is computed by the caller once those land.

use crate::exposure::RetirementReason;
use crate::gate::GateOutcome;
use crate::review::{reveal, Commit as Commitment};
use identity::nym::Nym;

/// Why an item left the pipeline without reaching the pool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RejectReason {
    /// Bridging rejected it for a defect (not appeal-eligible).
    Defect,
    /// Appeal-eligible (polarized) but the appeal window expired.
    Polarized,
    /// Failed the pilot's discrimination screen (stage 1).
    Screen,
    /// Failed the DIF check (stage 2).
    Dif,
}

/// The item lifecycle state (`docs/08` §9.1).
#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Deposited,
    Admitted,
    InReview {
        panel: Vec<Nym>,
        commits: Vec<(Nym, Commitment)>,
    },
    Revealing {
        commits: Vec<(Nym, Commitment)>,
        reveals: Vec<(Nym, f64)>,
    },
    /// Borderline band. Its forward transition is deliberately undefined (PROTO-008,
    /// roadmap T10/T30): the provisional tie-break is not the decided D26 mechanism.
    SupplementaryReview,
    AppealEligible,
    Pilot1 {
        appealed: bool,
    },
    Pilot2 {
        appealed: bool,
    },
    ActivePool,
    Rejected(RejectReason),
    Retired(RetirementReason),
}

/// A rejected transition (§9.1). The machine never advances on one of these.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Invalid {
    NoPrimarySource,
    /// The proposer did not present a valid identity nullifier (INV-9, T6).
    UnprovenIdentity,
    /// Over the per-credential rate-limit quota (ID-008, T11).
    OverQuota,
    DuplicateCid,
    /// The lottery seed was not the signed checkpoint head (INV-10, T8).
    SeedNotFromCheckpoint,
    /// Panel size must be odd and in `[7, 11]`.
    PanelSizeInvalid,
    /// Commit/reveal from a nym that is not in the panel.
    NotInPanel,
    AlreadyCommitted,
    /// Reveal from a nym that never committed.
    NoCommit,
    /// The reveal does not open the stored commitment (INV-12).
    RevealMismatch,
    /// A revealed probability outside `[0, 1]`, or NaN.
    ProbabilityOutOfRange,
    /// Scoring attempted before every panelist revealed (partial epoch).
    PartialEpoch,
    /// Appeal filed after the window closed, or on a non-appealable reject.
    AppealWindowClosed,
    /// Author reputation does not cover the appeal stake.
    InsufficientReputation,
    /// Pilot stage 1 with fewer than the required distinct respondents (INV-8).
    NotEnoughRespondents,
    /// Pilot stage 2 batch below `K_min` (never validate a single item, INV-8).
    BatchTooSmall,
    /// The event is not defined for the current state (out-of-order).
    UnexpectedEvent,
}

/// Smallest admissible DIF batch (`docs/08` §9.1 / INV-8: `K_min ≥ 2`).
pub const K_MIN: usize = 2;

/// `— → Deposited` (§9.1 row 1). The three proof inputs gate the invalid cases that
/// need primitives from T6/T11 and the log.
pub fn deposit(
    source_present: bool,
    identity_proven: bool,
    within_quota: bool,
    fresh_cid: bool,
) -> Result<State, Invalid> {
    if !source_present {
        return Err(Invalid::NoPrimarySource);
    }
    if !identity_proven {
        return Err(Invalid::UnprovenIdentity);
    }
    if !within_quota {
        return Err(Invalid::OverQuota);
    }
    if !fresh_cid {
        return Err(Invalid::DuplicateCid);
    }
    Ok(State::Deposited)
}

/// Events that drive an item forward (§9.1). Guard *results* (the bridging outcome, the
/// screen/DIF verdicts) are passed in, so the machine is a pure transition layer over
/// the existing per-stage functions.
#[derive(Clone, Debug)]
pub enum Event {
    /// Epoch close: admitted by the lottery. `seed_from_checkpoint` must hold (INV-10).
    Admit { seed_from_checkpoint: bool },
    /// Reviewers assigned; `panel` are their judge nyms, `k = panel.len()` odd ∈ [7,11].
    AssignReviewers { panel: Vec<Nym> },
    /// A panelist commits to a judgment before the deadline.
    Commit { nym: Nym, commitment: Commitment },
    /// Commit deadline: commitments are published.
    CloseCommits,
    /// A panelist reveals its judgment.
    Reveal {
        nym: Nym,
        prob: f64,
        nonce: [u8; 32],
    },
    /// Reveal deadline reached; the epoch is scored. `all_reveals_in` guards against
    /// scoring a partial epoch.
    Score {
        all_reveals_in: bool,
        outcome: GateOutcome,
    },
    /// The author appeals a polarization rejection.
    Appeal {
        within_window: bool,
        reputation_covers_stake: bool,
    },
    /// The appeal window expired with no appeal.
    AppealExpires,
    /// Pilot stage 1 batch. `enough_respondents` ≥ N₁; `passed` = discrimination screen.
    Pilot1Batch {
        enough_respondents: bool,
        passed: bool,
    },
    /// Pilot stage 2 batch of `batch_size` items; `passed` = DIF (Variant 2).
    Pilot2Batch { batch_size: usize, passed: bool },
    /// The item is administered (adds exposure).
    Administer,
    /// Periodic re-validation; `emerging_dif` retires it if set.
    Revalidate { emerging_dif: bool },
    /// Exposure reached the limit.
    ExposureLimit,
}

/// Applies `event` to `state`, returning the next state or the reason the transition is
/// invalid. This is the single place the §9.1 table is enforced.
pub fn step(state: State, event: Event) -> Result<State, Invalid> {
    use Event::*;
    use State::*;
    match (state, event) {
        // Deposited → Admitted: the lottery seed must come from the signed checkpoint.
        (
            Deposited,
            Admit {
                seed_from_checkpoint,
            },
        ) => {
            if seed_from_checkpoint {
                Ok(Admitted)
            } else {
                Err(Invalid::SeedNotFromCheckpoint)
            }
        }

        // Admitted → InReview: k odd ∈ [7, 11].
        (Admitted, AssignReviewers { panel }) => {
            let k = panel.len();
            if k % 2 == 1 && (7..=11).contains(&k) {
                Ok(InReview {
                    panel,
                    commits: Vec::new(),
                })
            } else {
                Err(Invalid::PanelSizeInvalid)
            }
        }

        // InReview: accumulate commits from distinct panelists.
        (InReview { panel, mut commits }, Commit { nym, commitment }) => {
            if !panel.contains(&nym) {
                return Err(Invalid::NotInPanel);
            }
            if commits.iter().any(|(n, _)| *n == nym) {
                return Err(Invalid::AlreadyCommitted);
            }
            commits.push((nym, commitment));
            Ok(InReview { panel, commits })
        }
        (InReview { commits, .. }, CloseCommits) => Ok(Revealing {
            commits,
            reveals: Vec::new(),
        }),

        // Revealing: a reveal must open a stored commitment with an in-range probability.
        (
            Revealing {
                commits,
                mut reveals,
            },
            Reveal { nym, prob, nonce },
        ) => {
            let Some((_, commitment)) = commits.iter().find(|(n, _)| *n == nym) else {
                return Err(Invalid::NoCommit);
            };
            if prob.is_nan() || !(0.0..=1.0).contains(&prob) {
                return Err(Invalid::ProbabilityOutOfRange);
            }
            if !reveal(*commitment, prob, &nonce) {
                return Err(Invalid::RevealMismatch);
            }
            reveals.push((nym, prob));
            Ok(Revealing { commits, reveals })
        }

        // Revealing → Gated outcome: never on a partial epoch.
        (
            Revealing { .. },
            Score {
                all_reveals_in,
                outcome,
            },
        ) => {
            if !all_reveals_in {
                return Err(Invalid::PartialEpoch);
            }
            Ok(match outcome {
                GateOutcome::Pass => Pilot1 { appealed: false },
                GateOutcome::SupplementaryReview => SupplementaryReview,
                GateOutcome::AppealEligible => AppealEligible,
                GateOutcome::Reject => Rejected(RejectReason::Defect),
            })
        }

        // AppealEligible: appeal within the window with reputation to cover the stake.
        (
            AppealEligible,
            Appeal {
                within_window,
                reputation_covers_stake,
            },
        ) => {
            if !within_window {
                Err(Invalid::AppealWindowClosed)
            } else if !reputation_covers_stake {
                Err(Invalid::InsufficientReputation)
            } else {
                Ok(Pilot1 { appealed: true })
            }
        }
        (AppealEligible, AppealExpires) => Ok(Rejected(RejectReason::Polarized)),

        // Pilot 1 → Pilot 2 / Rejected(Screen).
        (
            Pilot1 { appealed },
            Pilot1Batch {
                enough_respondents,
                passed,
            },
        ) => {
            if !enough_respondents {
                Err(Invalid::NotEnoughRespondents)
            } else if passed {
                Ok(Pilot2 { appealed })
            } else {
                Ok(Rejected(RejectReason::Screen))
            }
        }

        // Pilot 2 → ActivePool / Rejected(DIF): never validate a batch below K_min.
        (Pilot2 { .. }, Pilot2Batch { batch_size, passed }) => {
            if batch_size < K_MIN {
                Err(Invalid::BatchTooSmall)
            } else if passed {
                Ok(ActivePool)
            } else {
                Ok(Rejected(RejectReason::Dif))
            }
        }

        // ActivePool loop and retirements.
        (ActivePool, Administer) => Ok(ActivePool),
        (ActivePool, Revalidate { emerging_dif }) => Ok(if emerging_dif {
            Retired(RetirementReason::EmergingDif)
        } else {
            ActivePool
        }),
        (ActivePool, ExposureLimit) => Ok(Retired(RetirementReason::Exposure)),

        // Everything else is an out-of-order transition.
        _ => Err(Invalid::UnexpectedEvent),
    }
}
