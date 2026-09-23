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
use network::cid::Cid;

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
    /// Borderline: the D26 supplementary re-decision did not lift `b_j` over the
    /// threshold (roadmap T10/T30).
    Borderline,
}

/// The item lifecycle state (`docs/08` §9.1).
#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Deposited,
    Admitted,
    InReview {
        /// The item this panel reviews; the commit-reveal binds to it (INV-12, T7).
        item: Cid,
        panel: Vec<Nym>,
        commits: Vec<(Nym, Commitment)>,
    },
    Revealing {
        item: Cid,
        /// Carried from `InReview` so `Score` can check every panelist revealed.
        panel: Vec<Nym>,
        commits: Vec<(Nym, Commitment)>,
        reveals: Vec<(Nym, f64)>,
    },
    /// Borderline band: awaiting the D26 re-decision (`Event::Resolve`), which re-runs
    /// bridging over the expanded panel and re-decides `b_j` against the plain threshold
    /// (PROTO-008/PROTO-012 closed, roadmap T10/T30).
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
    /// The same nym appears twice in a panel (fewer distinct reviewers than slots).
    DuplicatePanelist,
    /// Commit/reveal from a nym that is not in the panel.
    NotInPanel,
    AlreadyCommitted,
    /// Reveal from a nym that never committed.
    NoCommit,
    /// A second reveal from the same nym.
    AlreadyRevealed,
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
    /// Reviewers assigned to `item`; `panel` are their judge nyms, `k = panel.len()` odd
    /// ∈ [7,11]. The item enters the state so the commit-reveal can bind to it (INV-12).
    AssignReviewers { panel: Vec<Nym>, item: Cid },
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
    /// Reveal deadline reached; the epoch is scored. Refused unless every panelist has
    /// revealed (checked against the state, not asserted by the caller).
    Score { outcome: GateOutcome },
    /// The D26 supplementary re-decision of a band item: `passed` is `gate::
    /// supplementary_review` (a re-run bridging fit, `b_j` vs the plain threshold).
    Resolve { passed: bool },
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

        // Admitted → InReview: k odd ∈ [7, 11] distinct reviewers.
        (Admitted, AssignReviewers { panel, item }) => {
            let k = panel.len();
            if k % 2 == 0 || !(7..=11).contains(&k) {
                Err(Invalid::PanelSizeInvalid)
            } else if (1..k).any(|i| panel[..i].contains(&panel[i])) {
                Err(Invalid::DuplicatePanelist)
            } else {
                Ok(InReview {
                    item,
                    panel,
                    commits: Vec::new(),
                })
            }
        }

        // InReview: accumulate commits from distinct panelists.
        (
            InReview {
                item,
                panel,
                mut commits,
            },
            Commit { nym, commitment },
        ) => {
            if !panel.contains(&nym) {
                return Err(Invalid::NotInPanel);
            }
            if commits.iter().any(|(n, _)| *n == nym) {
                return Err(Invalid::AlreadyCommitted);
            }
            commits.push((nym, commitment));
            Ok(InReview {
                item,
                panel,
                commits,
            })
        }
        (
            InReview {
                item,
                panel,
                commits,
            },
            CloseCommits,
        ) => Ok(Revealing {
            item,
            panel,
            commits,
            reveals: Vec::new(),
        }),

        // Revealing: a reveal must open a stored commitment with an in-range probability.
        // The opening recomputes the commitment for the revealer and this item, so a
        // copied commitment cannot be opened by anyone else (INV-12, T7).
        (
            Revealing {
                item,
                panel,
                commits,
                mut reveals,
            },
            Reveal { nym, prob, nonce },
        ) => {
            let Some((_, commitment)) = commits.iter().find(|(n, _)| *n == nym) else {
                return Err(Invalid::NoCommit);
            };
            if reveals.iter().any(|(n, _)| *n == nym) {
                return Err(Invalid::AlreadyRevealed);
            }
            if prob.is_nan() || !(0.0..=1.0).contains(&prob) {
                return Err(Invalid::ProbabilityOutOfRange);
            }
            if !reveal(*commitment, prob, &nonce, nym, item) {
                return Err(Invalid::RevealMismatch);
            }
            reveals.push((nym, prob));
            Ok(Revealing {
                item,
                panel,
                commits,
                reveals,
            })
        }

        // Revealing → Gated outcome: never on a partial epoch. Reveals are unique and
        // come only from committed panelists, so "all in" is every panelist present.
        (Revealing { panel, reveals, .. }, Score { outcome }) => {
            if !panel.iter().all(|p| reveals.iter().any(|(n, _)| n == p)) {
                return Err(Invalid::PartialEpoch);
            }
            Ok(match outcome {
                GateOutcome::Pass => Pilot1 { appealed: false },
                GateOutcome::SupplementaryReview => SupplementaryReview,
                GateOutcome::AppealEligible => AppealEligible,
                GateOutcome::Reject => Rejected(RejectReason::Defect),
            })
        }

        // SupplementaryReview → the D26 re-decision (T10/T30): the re-run bridging fit
        // either lifts b_j over the threshold (→ pilot) or it does not (→ borderline reject).
        (SupplementaryReview, Resolve { passed }) => Ok(if passed {
            Pilot1 { appealed: false }
        } else {
            Rejected(RejectReason::Borderline)
        }),

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
