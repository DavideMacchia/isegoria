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

impl RejectReason {
    /// A rejection by the gate — a defect, an unappealed polarization, a failed band
    /// re-decision — whose Level B outcome is unknown, so the exploration draw (D35, T52)
    /// can measure it; a pilot rejection's outcome is already known.
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
    /// Borderline band: the D26 extra round, then the re-decision (T10/T30/T59/T60).
    /// `k_extra` reviewers drawn outside the first panel are assigned
    /// (`Event::AssignExtraReviewers`), commit and reveal on the same item under the same
    /// binding as the first round, and `Event::Resolve` — refused until every extra
    /// panelist revealed — carries the re-decision computed over the first panel's
    /// ratings *plus* theirs (`gate::supplementary_review`): pass, or the below-band rule
    /// (a polarized item keeps the appeal channel, D26 amendment).
    SupplementaryReview {
        item: Cid,
        /// The first panel: the extra panel is drawn outside it.
        panel: Vec<Nym>,
        /// The extra reviewers; empty until assigned.
        extra_panel: Vec<Nym>,
        commits: Vec<(Nym, Commitment)>,
        /// The extra round's commit deadline has passed (`CloseCommits`).
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
    /// A gate rejection drawn for exploration (D35, T52): piloted for measurement only,
    /// through the same two batches as a passing item; `screened` once stage 1 is passed.
    Explored {
        reason: RejectReason,
        screened: bool,
    },
    /// The pilot's measurement of an explored rejection: whether Level B would have
    /// passed it. Terminal — the item never enters the pool; the outcome enters its
    /// reviewers' scores at weight `1/ε` and the gate's false-negative rate.
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
    /// The proposer did not present a valid identity nullifier (INV-9, T6).
    UnprovenIdentity,
    /// Over the per-credential rate-limit quota (ID-008, T11).
    OverQuota,
    DuplicateCid,
    /// The lottery seed — or the exploration draw's (D35, T52) — was not the signed
    /// checkpoint head (INV-10, T8).
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
    /// Scoring attempted before every panelist revealed (partial epoch) — of the first
    /// round (`Score`) or of the band's extra round (`Resolve`, T60).
    PartialEpoch,
    /// The band re-decision was attempted before extra reviewers were assigned (D26, T60).
    NoExtraPanel,
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

/// Largest extra panel of the band's second round (D26, T60): at least one and at most
/// as many reviewers as a first panel, all outside it. The default draw is
/// `review::K_EXTRA`.
pub const K_EXTRA_MAX: usize = 11;

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
    /// The band's extra round (D26, T60): `panel` are `k_extra` judge nyms outside the
    /// first panel, `1..=K_EXTRA_MAX`, distinct. They then `Commit`, `CloseCommits` and
    /// `Reveal` on the same item, as the first panel did.
    AssignExtraReviewers { panel: Vec<Nym> },
    /// The D26 supplementary re-decision of a band item, refused until every extra
    /// panelist revealed (T60): `outcome` is `gate::supplementary_review` over the first
    /// panel's ratings plus the extra round's (a re-run bridging fit, the side-balanced
    /// score vs the plain threshold) — `Pass`, or, below it, the below-band rule of the
    /// gate: `AppealEligible` for a polarized item, `Reject` for a defect (D26 amendment,
    /// T59). `SupplementaryReview` is not an outcome of a re-decision and is refused.
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
    /// The beacon's exploration draw picked this gate rejection (D35, T52): it goes to
    /// the pilot for measurement only. `seed_from_checkpoint` must hold (INV-10).
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

        // SupplementaryReview: the D26 extra round (T60). The extra panel is assigned once,
        // outside the first panel; its members commit, the commits close, they reveal —
        // the same rules and the same binding as the first round.
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

        // SupplementaryReview → the D26 re-decision (T10/T30, amended by T59, T60): only
        // once the extra round is complete. The re-run bridging fit over the expanded
        // ratings lifts the score over the plain threshold (→ pilot) or it does not — then
        // a polarized item keeps the appeal channel (→ AppealEligible) and a defect is a
        // borderline reject. A second band is not an outcome of a re-decision.
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

        // Rejected at the gate → Explored (D35, T52): the beacon's exploration draw sends
        // a random 5% of gate rejections to the pilot for measurement only. A pilot
        // rejection has its outcome already; the draw's seed must be the checkpoint's.
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

        // Explored: the two pilot batches under the pilot's own floors, ending `Measured`
        // — a pass here measures the gate, it never enters the pool.
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
