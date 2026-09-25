//! Item lifecycle state machine (`docs/08` §9.1, `docs/05`): rejects every invalid
//! transition (PC-1). Preconditions from other tasks (identity nullifier T6, quota
//! proof T11, checkpoint seed T8) enter as explicit `bool` inputs.

use crate::exposure::RetirementReason;
use crate::gate::GateOutcome;
use crate::review::{reveal, Commit as Commitment};
use identity::nym::Nym;
use network::cid::Cid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RejectReason {
    /// Not appeal-eligible.
    Defect,
    /// The appeal window expired without an appeal.
    Polarized,
    /// Failed the pilot's discrimination screen (stage 1).
    Screen,
    /// Failed the DIF check (stage 2).
    Dif,
    /// D26 supplementary re-decision did not lift `b_j` over the threshold.
    Borderline,
}

impl RejectReason {
    /// True for a gate rejection: a defect, unappealed polarization, or a failed re-decision.
    pub fn at_the_gate(self) -> bool {
        matches!(
            self,
            RejectReason::Defect | RejectReason::Polarized | RejectReason::Borderline
        )
    }
}

/// The item lifecycle state (`docs/08` §9.1).
#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Deposited,
    Admitted,
    InReview {
        /// The item this panel reviews; the commit-reveal binds to it (INV-12).
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
    /// D26 borderline band: extra reviewers assigned once, then commit/reveal as the first
    /// round did; `Event::Resolve` re-decides over the first panel's ratings plus theirs.
    SupplementaryReview {
        item: Cid,
        /// The first panel: the extra panel is drawn outside it.
        panel: Vec<Nym>,
        /// The extra reviewers; empty until assigned.
        extra_panel: Vec<Nym>,
        commits: Vec<(Nym, Commitment)>,
        commits_closed: bool,
        reveals: Vec<(Nym, f64)>,
    },
    AppealEligible,
    Pilot1 {
        appealed: bool,
    },
    Pilot2 {
        appealed: bool,
    },
    ActivePool,
    Rejected(RejectReason),
    /// A gate rejection drawn for exploration (D35), piloted for measurement only until
    /// `screened` once stage 1 passes.
    Explored {
        reason: RejectReason,
        screened: bool,
    },
    /// Terminal measurement of an explored rejection; feeds reviewer scores at weight `1/ε`.
    Measured {
        reason: RejectReason,
        passed: bool,
    },
    Retired(RetirementReason),
}

/// A rejected transition (§9.1). The machine never advances on one of these.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Invalid {
    NoPrimarySource,
    UnprovenIdentity,
    OverQuota,
    DuplicateCid,
    /// The draw's seed was not the signed checkpoint head (INV-10).
    SeedNotFromCheckpoint,
    /// Panel size must be odd and in `[7, 11]`.
    PanelSizeInvalid,
    DuplicatePanelist,
    NotInPanel,
    AlreadyCommitted,
    NoCommit,
    AlreadyRevealed,
    RevealMismatch,
    /// A revealed probability outside `[0, 1]`, or NaN.
    ProbabilityOutOfRange,
    /// Not every panelist revealed yet — for `Score` or the extra round's `Resolve`.
    PartialEpoch,
    /// `Resolve` attempted before extra reviewers were assigned (D26).
    NoExtraPanel,
    /// Appeal filed after the window closed, or on a non-appealable reject.
    AppealWindowClosed,
    InsufficientReputation,
    NotEnoughRespondents,
    BatchTooSmall,
    /// The event is not defined for the current state (out-of-order).
    UnexpectedEvent,
}

/// Smallest admissible DIF batch (`docs/08` §9.1, INV-8).
pub const K_MIN: usize = 2;

/// Largest extra panel of the band's second round (D26): at least one, at most a first
/// panel's size, all outside it. The default draw is `review::K_EXTRA`.
pub const K_EXTRA_MAX: usize = 11;

/// `— → Deposited` (§9.1 row 1). The bool inputs gate the invalid cases that need
/// primitives from other tasks and the log.
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

/// Events that drive an item forward (§9.1). Guard *results* (bridging outcome, screen/DIF
/// verdicts) are passed in: this is a pure transition layer over the per-stage functions.
#[derive(Clone, Debug)]
pub enum Event {
    /// Epoch close: admitted by the lottery. `seed_from_checkpoint` must hold (INV-10).
    Admit { seed_from_checkpoint: bool },
    /// Reviewers assigned to `item`; `panel` are judge nyms, odd length in `[7,11]` (INV-12).
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
    /// Reveal deadline reached; refused unless every panelist revealed (checked, not assumed).
    Score { outcome: GateOutcome },
    /// The band's extra round (D26): `k_extra` judge nyms outside the first panel, distinct.
    AssignExtraReviewers { panel: Vec<Nym> },
    /// The D26 re-decision, refused until every extra panelist revealed: `outcome` is
    /// `gate::supplementary_review` over the combined ratings (never `SupplementaryReview`).
    Resolve { outcome: GateOutcome },
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
    /// The beacon's exploration draw (D35) sends this rejection to the pilot for measurement.
    Explore { seed_from_checkpoint: bool },
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

        // The opening recomputes the commitment for the revealer and this item, so a copied
        // commitment cannot be opened by anyone else (INV-12).
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

        // Reveals are unique and come only from committed panelists, so "all in" is every
        // panelist present.
        (
            Revealing {
                item,
                panel,
                reveals,
                ..
            },
            Score { outcome },
        ) => {
            if !panel.iter().all(|p| reveals.iter().any(|(n, _)| n == p)) {
                return Err(Invalid::PartialEpoch);
            }
            Ok(match outcome {
                GateOutcome::Pass => Pilot1 { appealed: false },
                GateOutcome::SupplementaryReview => SupplementaryReview {
                    item,
                    panel,
                    extra_panel: Vec::new(),
                    commits: Vec::new(),
                    commits_closed: false,
                    reveals: Vec::new(),
                },
                GateOutcome::AppealEligible => AppealEligible,
                GateOutcome::Reject => Rejected(RejectReason::Defect),
            })
        }

        (
            SupplementaryReview {
                item,
                panel,
                extra_panel,
                commits,
                commits_closed,
                reveals,
            },
            AssignExtraReviewers { panel: extra },
        ) => {
            if !extra_panel.is_empty() {
                return Err(Invalid::UnexpectedEvent);
            }
            let k = extra.len();
            if k == 0 || k > K_EXTRA_MAX {
                return Err(Invalid::PanelSizeInvalid);
            }
            if (1..k).any(|i| extra[..i].contains(&extra[i]))
                || extra.iter().any(|n| panel.contains(n))
            {
                return Err(Invalid::DuplicatePanelist);
            }
            Ok(SupplementaryReview {
                item,
                panel,
                extra_panel: extra,
                commits,
                commits_closed,
                reveals,
            })
        }
        (
            SupplementaryReview {
                item,
                panel,
                extra_panel,
                mut commits,
                commits_closed: false,
                reveals,
            },
            Commit { nym, commitment },
        ) if !extra_panel.is_empty() => {
            if !extra_panel.contains(&nym) {
                return Err(Invalid::NotInPanel);
            }
            if commits.iter().any(|(n, _)| *n == nym) {
                return Err(Invalid::AlreadyCommitted);
            }
            commits.push((nym, commitment));
            Ok(SupplementaryReview {
                item,
                panel,
                extra_panel,
                commits,
                commits_closed: false,
                reveals,
            })
        }
        (
            SupplementaryReview {
                item,
                panel,
                extra_panel,
                commits,
                commits_closed: false,
                reveals,
            },
            CloseCommits,
        ) if !extra_panel.is_empty() => Ok(SupplementaryReview {
            item,
            panel,
            extra_panel,
            commits,
            commits_closed: true,
            reveals,
        }),
        (
            SupplementaryReview {
                item,
                panel,
                extra_panel,
                commits,
                commits_closed: true,
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
            Ok(SupplementaryReview {
                item,
                panel,
                extra_panel,
                commits,
                commits_closed: true,
                reveals,
            })
        }

        (
            SupplementaryReview {
                extra_panel,
                reveals,
                ..
            },
            Resolve { outcome },
        ) => {
            if extra_panel.is_empty() {
                return Err(Invalid::NoExtraPanel);
            }
            if !extra_panel
                .iter()
                .all(|p| reveals.iter().any(|(n, _)| n == p))
            {
                return Err(Invalid::PartialEpoch);
            }
            match outcome {
                GateOutcome::Pass => Ok(Pilot1 { appealed: false }),
                GateOutcome::AppealEligible => Ok(AppealEligible),
                GateOutcome::Reject => Ok(Rejected(RejectReason::Borderline)),
                GateOutcome::SupplementaryReview => Err(Invalid::UnexpectedEvent),
            }
        }

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

        (Pilot2 { .. }, Pilot2Batch { batch_size, passed }) => {
            if batch_size < K_MIN {
                Err(Invalid::BatchTooSmall)
            } else if passed {
                Ok(ActivePool)
            } else {
                Ok(Rejected(RejectReason::Dif))
            }
        }

        (
            Rejected(reason),
            Explore {
                seed_from_checkpoint,
            },
        ) => {
            if !reason.at_the_gate() {
                return Err(Invalid::UnexpectedEvent);
            }
            if !seed_from_checkpoint {
                return Err(Invalid::SeedNotFromCheckpoint);
            }
            Ok(Explored {
                reason,
                screened: false,
            })
        }

        (
            Explored {
                reason,
                screened: false,
            },
            Pilot1Batch {
                enough_respondents,
                passed,
            },
        ) => {
            if !enough_respondents {
                Err(Invalid::NotEnoughRespondents)
            } else if passed {
                Ok(Explored {
                    reason,
                    screened: true,
                })
            } else {
                Ok(Measured {
                    reason,
                    passed: false,
                })
            }
        }
        (
            Explored {
                reason,
                screened: true,
            },
            Pilot2Batch { batch_size, passed },
        ) => {
            if batch_size < K_MIN {
                Err(Invalid::BatchTooSmall)
            } else {
                Ok(Measured { reason, passed })
            }
        }

        (ActivePool, Administer) => Ok(ActivePool),
        (ActivePool, Revalidate { emerging_dif }) => Ok(if emerging_dif {
            Retired(RetirementReason::EmergingDif)
        } else {
            ActivePool
        }),
        (ActivePool, ExposureLimit) => Ok(Retired(RetirementReason::Exposure)),

        _ => Err(Invalid::UnexpectedEvent),
    }
}
