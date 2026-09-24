//! Model-based test of the §9.1 item lifecycle (`lifecycle::{deposit, step}`, T43).
//!
//! A reference model, written here from the §9.1 table (`docs/08`) and the documented
//! `Invalid` cases rather than from `step`, predicts for every event of a random walk
//! whether the transition is legal and which state results, or the exact `Invalid` (§9.1
//! lists the invalid cases, not which one is reported when several hold at once: the model
//! pins that down, see `guard`). The walks interleave the event the protocol expects next
//! with arbitrary ones: out of order, from outsiders, repeated, with copied, lifted or
//! opaque commitments, with out-of-range or NaN probabilities. Independently of the model,
//! every walk is checked against the lifecycle invariants:
//!
//! - a `Score` succeeds only once every panelist has revealed;
//! - no nym commits twice or reveals twice, and only a committer reveals;
//! - the panel is distinct, of odd size in `[7, 11]`;
//! - a rejected event leaves the state as it was: the accepted events alone replay the walk;
//! - `Rejected` and `Retired` are never left;
//! - `ActivePool` is entered only from `Pilot2`, and `Pilot2` only from `Pilot1`.

use identity::nym::Nym;
use network::cid::{cid, Cid};
use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::TestRunner;
use protocol::exposure::RetirementReason;
use protocol::gate::GateOutcome;
use protocol::lifecycle::{deposit, step, Event, Invalid, RejectReason, State, K_MIN};
use protocol::review::{commit, Commit};
use std::collections::HashSet;

// ------------------------------------ the reference model ------------------------------------

/// Nyms `nym_at(0..13)`: every panel is drawn from them, the rest of them are outsiders.
const UNIVERSE: usize = 13;

fn nym_at(i: usize) -> Nym {
    Nym([1 + (i % UNIVERSE) as u8; 32])
}

/// The item a round reviews, or the other one (a commitment lifted onto it must not open).
fn item(other: bool) -> Cid {
    cid(if other { b"item B" } else { b"item A" })
}

fn other_item(of: Cid) -> Cid {
    item(of == item(false))
}

/// The probabilities a judgment may carry. The first `IN_RANGE` are in `[0, 1]` (the two
/// zeros are equal numbers but different committed bytes); the others are not.
const PROBS: [f64; 10] = [
    0.5,
    0.8,
    0.25,
    1.0,
    0.0,
    -0.0,
    1.5,
    -0.1,
    f64::NAN,
    f64::INFINITY,
];
const IN_RANGE: u8 = 6;

fn prob(i: u8) -> f64 {
    PROBS[i as usize % PROBS.len()]
}

fn nonce(i: u8) -> [u8; 32] {
    [i % 3; 32]
}

const OUTCOMES: [GateOutcome; 4] = [
    GateOutcome::Pass,
    GateOutcome::SupplementaryReview,
    GateOutcome::AppealEligible,
    GateOutcome::Reject,
];

/// What a commitment was built from. The model decides whether a reveal opens it from
/// this, never by recomputing the hash: it opens iff the reveal repeats the committed
/// probability (bit for bit) and nonce, by the committer, for the round's item (INV-12).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Preimage {
    Of {
        prob_bits: u64,
        nonce: [u8; 32],
        committer: Nym,
        item: Cid,
    },
    /// Bytes nobody can open.
    Opaque,
}

/// A review round as the model keeps it: every commitment with its preimage.
#[derive(Clone, Debug)]
struct Round {
    item: Cid,
    panel: Vec<Nym>,
    commits: Vec<(Nym, Commit, Preimage)>,
    reveals: Vec<(Nym, f64)>,
}

impl Round {
    fn commitment_of(&self, n: &Nym) -> Option<Preimage> {
        self.commits
            .iter()
            .find(|(c, ..)| c == n)
            .map(|&(.., pre)| pre)
    }

    fn has_revealed(&self, n: &Nym) -> bool {
        self.reveals.iter().any(|(r, _)| r == n)
    }
}

/// The model's phases, one per §9.1 state.
#[derive(Clone, Debug)]
enum Phase {
    Deposited,
    Admitted,
    Committing(Round),
    Revealing(Round),
    Band,
    Appealable,
    Pilot1 { appealed: bool },
    Pilot2 { appealed: bool },
    Pool,
    Rejected(RejectReason),
    Retired(RetirementReason),
}

/// The first violated precondition. Where several hold, the one reported is: who acts
/// before what it discloses, an admissible probability before its opening, a panel's size
/// before its members, and otherwise the order in which the inputs are declared.
fn guard(preconditions: &[(bool, Invalid)]) -> Result<(), Invalid> {
    match preconditions.iter().find(|(violated, _)| *violated) {
        Some(&(_, why)) => Err(why),
        None => Ok(()),
    }
}

/// `— → Deposited`: a primary source, an identity proof, a free quota slot and a fresh
/// cid, in the order `deposit` takes them.
fn model_deposit([source, identity, quota, fresh]: [bool; 4]) -> Result<Phase, Invalid> {
    guard(&[
        (!source, Invalid::NoPrimarySource),
        (!identity, Invalid::UnprovenIdentity),
        (!quota, Invalid::OverQuota),
        (!fresh, Invalid::DuplicateCid),
    ])?;
    Ok(Phase::Deposited)
}

/// The §9.1 row for `(phase, event)`; `preimage` is what a `Commit`'s commitment was built
/// from (ignored for every other event). Any pair without a row is out of order.
fn model_step(phase: &Phase, event: &Event, preimage: Preimage) -> Result<Phase, Invalid> {
    use Invalid::*;
    use Phase::*;
    match (phase, event) {
        (
            Deposited,
            Event::Admit {
                seed_from_checkpoint,
            },
        ) => {
            guard(&[(!seed_from_checkpoint, SeedNotFromCheckpoint)])?;
            Ok(Admitted)
        }

        (Admitted, Event::AssignReviewers { panel, item }) => {
            let distinct: HashSet<Nym> = panel.iter().copied().collect();
            guard(&[
                (![7, 9, 11].contains(&panel.len()), PanelSizeInvalid),
                (distinct.len() != panel.len(), DuplicatePanelist),
            ])?;
            Ok(Committing(Round {
                item: *item,
                panel: panel.clone(),
                commits: Vec::new(),
                reveals: Vec::new(),
            }))
        }

        (Committing(r), Event::Commit { nym, commitment }) => {
            guard(&[
                (!r.panel.contains(nym), NotInPanel),
                (r.commitment_of(nym).is_some(), AlreadyCommitted),
            ])?;
            let mut r = r.clone();
            r.commits.push((*nym, *commitment, preimage));
            Ok(Committing(r))
        }
        (Committing(r), Event::CloseCommits) => Ok(Revealing(r.clone())),

        (Revealing(r), Event::Reveal { nym, prob, nonce }) => {
            let stored = r.commitment_of(nym);
            let opening = Preimage::Of {
                prob_bits: prob.to_bits(),
                nonce: *nonce,
                committer: *nym,
                item: r.item,
            };
            guard(&[
                (stored.is_none(), NoCommit),
                (r.has_revealed(nym), AlreadyRevealed),
                (!(0.0..=1.0).contains(prob), ProbabilityOutOfRange),
                (stored != Some(opening), RevealMismatch),
            ])?;
            let mut r = r.clone();
            r.reveals.push((*nym, *prob));
            Ok(Revealing(r))
        }
        (Revealing(r), Event::Score { outcome }) => {
            let panel: HashSet<Nym> = r.panel.iter().copied().collect();
            let revealed: HashSet<Nym> = r.reveals.iter().map(|(n, _)| *n).collect();
            guard(&[(revealed != panel, PartialEpoch)])?;
            Ok(match outcome {
                GateOutcome::Pass => Pilot1 { appealed: false },
                GateOutcome::SupplementaryReview => Band,
                GateOutcome::AppealEligible => Appealable,
                GateOutcome::Reject => Rejected(RejectReason::Defect),
            })
        }

        (Band, Event::Resolve { passed: true }) => Ok(Pilot1 { appealed: false }),
        (Band, Event::Resolve { passed: false }) => Ok(Rejected(RejectReason::Borderline)),

        (
            Appealable,
            Event::Appeal {
                within_window,
                reputation_covers_stake,
            },
        ) => {
            guard(&[
                (!within_window, AppealWindowClosed),
                (!reputation_covers_stake, InsufficientReputation),
            ])?;
            Ok(Pilot1 { appealed: true })
        }
        (Appealable, Event::AppealExpires) => Ok(Rejected(RejectReason::Polarized)),

        (
            Pilot1 { appealed },
            Event::Pilot1Batch {
                enough_respondents,
                passed,
            },
        ) => {
            guard(&[(!enough_respondents, NotEnoughRespondents)])?;
            Ok(if *passed {
                Pilot2 {
                    appealed: *appealed,
                }
            } else {
                Rejected(RejectReason::Screen)
            })
        }

        (Pilot2 { .. }, Event::Pilot2Batch { batch_size, passed }) => {
            guard(&[(*batch_size < K_MIN, BatchTooSmall)])?;
            Ok(if *passed {
                Pool
            } else {
                Rejected(RejectReason::Dif)
            })
        }

        (Pool, Event::Administer) => Ok(Pool),
        (
            Pool,
            Event::Revalidate {
                emerging_dif: false,
            },
        ) => Ok(Pool),
        (Pool, Event::Revalidate { emerging_dif: true }) => {
            Ok(Retired(RetirementReason::EmergingDif))
        }
        (Pool, Event::ExposureLimit) => Ok(Retired(RetirementReason::Exposure)),

        _ => Err(UnexpectedEvent),
    }
}

/// The `State` the machine must be in when the model is in `phase`.
fn concrete(phase: &Phase) -> State {
    let commits = |r: &Round| r.commits.iter().map(|&(n, c, _)| (n, c)).collect();
    match phase {
        Phase::Deposited => State::Deposited,
        Phase::Admitted => State::Admitted,
        Phase::Committing(r) => State::InReview {
            item: r.item,
            panel: r.panel.clone(),
            commits: commits(r),
        },
        Phase::Revealing(r) => State::Revealing {
            item: r.item,
            panel: r.panel.clone(),
            commits: commits(r),
            reveals: r.reveals.clone(),
        },
        Phase::Band => State::SupplementaryReview,
        Phase::Appealable => State::AppealEligible,
        Phase::Pilot1 { appealed } => State::Pilot1 {
            appealed: *appealed,
        },
        Phase::Pilot2 { appealed } => State::Pilot2 {
            appealed: *appealed,
        },
        Phase::Pool => State::ActivePool,
        Phase::Rejected(why) => State::Rejected(*why),
        Phase::Retired(why) => State::Retired(*why),
    }
}

// ------------------------------------------ the walks ------------------------------------------

/// Who acts in a commit or reveal, resolved against the model's round: mostly a panelist
/// the protocol is waiting for, otherwise a near miss.
#[derive(Clone, Copy, Debug)]
enum Who {
    /// A panelist still expected to act (to commit, or to reveal a commitment), else `nym_at`.
    Pending(u8),
    /// Any panelist, else `nym_at`.
    Panelist(u8),
    /// Any nym, often outside the panel.
    Anyone(u8),
}

/// How a commitment is built.
#[derive(Clone, Copy, Debug)]
enum Seal {
    /// By the committer, for the round's item.
    Honest { prob: u8, nonce: u8 },
    /// Another nym's commitment, submitted as one's own (AT-BR-06).
    Copied { from: u8, prob: u8, nonce: u8 },
    /// Made for the other item.
    Lifted { prob: u8, nonce: u8 },
    /// Random bytes.
    Opaque(u8),
}

/// What a reveal discloses.
#[derive(Clone, Copy, Debug)]
enum Disclosure {
    /// The probability and nonce of the revealer's commitment, if it made one.
    Opening,
    /// Any probability and nonce.
    Value { prob: u8, nonce: u8 },
}

/// Chooses the event the model's phase expects next (see `Op::Next`).
#[derive(Clone, Copy, Debug)]
struct Knob {
    /// A detour that can leave the round unscorable for good: an early close, a commitment
    /// its committer cannot open. Rare, so that most walks go on.
    derail: bool,
    /// A detour the machine refuses and the walk survives: a bad seed, panel, appeal or
    /// batch, an early score, a wrong opening, an outsider.
    stray: bool,
    pick: u8,
}

/// One step of a walk.
#[derive(Clone, Debug)]
enum Op {
    /// The event the protocol expects next in the model's phase.
    Next(Knob),
    Admit(bool),
    Assign {
        size: usize,
        start: u8,
        dup: Option<(u8, u8)>,
        other: bool,
    },
    Commit(Who, Seal),
    CloseCommits,
    Reveal(Who, Disclosure),
    Score(GateOutcome),
    Resolve(bool),
    Appeal {
        within_window: bool,
        covers_stake: bool,
    },
    AppealExpires,
    Pilot1Batch {
        enough: bool,
        passed: bool,
    },
    Pilot2Batch {
        batch: usize,
        passed: bool,
    },
    Administer,
    Revalidate(bool),
    ExposureLimit,
}

/// The event the protocol expects next in `phase`, or a detour (see `Knob`).
fn next_op(
    Knob {
        derail,
        stray,
        pick,
    }: Knob,
    phase: &Phase,
) -> Op {
    let outcome = OUTCOMES[pick as usize % OUTCOMES.len()];
    let bit = |k: u8| (pick >> k) & 1 == 1;
    let honest = Seal::Honest {
        prob: pick % IN_RANGE,
        nonce: pick,
    };
    match phase {
        Phase::Deposited => Op::Admit(!stray),
        Phase::Admitted if stray => Op::Assign {
            size: pick as usize % 14,
            start: pick,
            dup: Some((pick, pick / 14)),
            other: false,
        },
        Phase::Admitted => Op::Assign {
            size: [7, 9, 11][pick as usize % 3],
            start: pick,
            dup: None,
            other: bit(7),
        },
        Phase::Committing(_) if derail && bit(0) => Op::CloseCommits,
        Phase::Committing(_) if derail => Op::Commit(
            Who::Pending(pick),
            Seal::Lifted {
                prob: pick % IN_RANGE,
                nonce: pick,
            },
        ),
        Phase::Committing(_) if stray => Op::Commit(Who::Anyone(pick), honest),
        Phase::Committing(r) if r.commits.len() < r.panel.len() => {
            Op::Commit(Who::Pending(pick), honest)
        }
        Phase::Committing(_) => Op::CloseCommits,
        Phase::Revealing(_) if stray && bit(0) => Op::Score(outcome),
        Phase::Revealing(_) if stray => Op::Reveal(
            if bit(1) {
                Who::Anyone(pick)
            } else {
                Who::Pending(pick)
            },
            Disclosure::Value {
                prob: pick,
                nonce: pick / 3,
            },
        ),
        Phase::Revealing(r) if r.reveals.len() < r.commits.len() => {
            Op::Reveal(Who::Pending(pick), Disclosure::Opening)
        }
        Phase::Revealing(_) => Op::Score(outcome),
        Phase::Band => Op::Resolve(bit(0)),
        Phase::Appealable if stray => Op::Appeal {
            within_window: bit(0),
            covers_stake: !bit(0),
        },
        Phase::Appealable if bit(0) => Op::AppealExpires,
        Phase::Appealable => Op::Appeal {
            within_window: true,
            covers_stake: true,
        },
        Phase::Pilot1 { .. } => Op::Pilot1Batch {
            enough: !stray,
            passed: pick % 4 != 0,
        },
        Phase::Pilot2 { .. } => Op::Pilot2Batch {
            batch: if stray {
                pick as usize % K_MIN
            } else {
                K_MIN + pick as usize % 8
            },
            passed: pick % 4 != 0,
        },
        Phase::Pool => match pick % 8 {
            0..=4 => Op::Administer,
            5 => Op::Revalidate(false),
            6 => Op::Revalidate(true),
            _ => Op::ExposureLimit,
        },
        Phase::Rejected(_) | Phase::Retired(_) => match pick % 6 {
            0 => Op::Administer,
            1 => Op::Admit(true),
            2 => Op::Score(outcome),
            3 => Op::Resolve(true),
            4 => Op::AppealExpires,
            _ => Op::Pilot1Batch {
                enough: true,
                passed: true,
            },
        },
    }
}

/// The nym `who` names in `phase`.
fn actor(who: Who, phase: &Phase) -> Nym {
    let pick = |candidates: Vec<Nym>, i: u8| match candidates.len() {
        0 => nym_at(i as usize),
        n => candidates[i as usize % n],
    };
    match (who, phase) {
        (Who::Pending(i), Phase::Committing(r)) => pick(
            r.panel
                .iter()
                .filter(|n| r.commitment_of(n).is_none())
                .copied()
                .collect(),
            i,
        ),
        (Who::Pending(i), Phase::Revealing(r)) => pick(
            r.commits
                .iter()
                .map(|&(n, ..)| n)
                .filter(|n| !r.has_revealed(n))
                .collect(),
            i,
        ),
        (Who::Panelist(i), Phase::Committing(r) | Phase::Revealing(r)) => pick(r.panel.clone(), i),
        (Who::Pending(i) | Who::Panelist(i) | Who::Anyone(i), _) => nym_at(i as usize),
    }
}

/// A commitment built with `review::commit`, and what it was built from.
fn sealed(p: u8, n: u8, committer: Nym, item: Cid) -> (Commit, Preimage) {
    let (prob, nonce) = (prob(p), nonce(n));
    let preimage = Preimage::Of {
        prob_bits: prob.to_bits(),
        nonce,
        committer,
        item,
    };
    (commit(prob, &nonce, committer, item), preimage)
}

/// The concrete event `op` stands for in `phase`, and the preimage of its commitment (for a
/// `Commit`; `Opaque` otherwise).
fn plan(op: &Op, phase: &Phase) -> (Event, Preimage) {
    let round = match phase {
        Phase::Committing(r) | Phase::Revealing(r) => Some(r),
        _ => None,
    };
    let round_item = round.map_or(item(false), |r| r.item);
    let plain = |event| (event, Preimage::Opaque);
    match *op {
        Op::Next(knob) => plan(&next_op(knob, phase), phase),
        Op::Admit(seed_from_checkpoint) => plain(Event::Admit {
            seed_from_checkpoint,
        }),
        Op::Assign {
            size,
            start,
            dup,
            other,
        } => {
            let mut panel: Vec<Nym> = (0..size).map(|i| nym_at(start as usize + i)).collect();
            if let (Some((from, to)), true) = (dup, size > 0) {
                panel[to as usize % size] = panel[from as usize % size];
            }
            plain(Event::AssignReviewers {
                panel,
                item: item(other),
            })
        }
        Op::Commit(who, seal) => {
            let nym = actor(who, phase);
            let (commitment, preimage) = match seal {
                Seal::Honest { prob, nonce } => sealed(prob, nonce, nym, round_item),
                Seal::Copied { from, prob, nonce } => {
                    sealed(prob, nonce, nym_at(from as usize), round_item)
                }
                Seal::Lifted { prob, nonce } => sealed(prob, nonce, nym, other_item(round_item)),
                Seal::Opaque(b) => (Commit([b; 32]), Preimage::Opaque),
            };
            (Event::Commit { nym, commitment }, preimage)
        }
        Op::CloseCommits => plain(Event::CloseCommits),
        Op::Reveal(who, disclosure) => {
            let nym = actor(who, phase);
            let stored = round.and_then(|r| r.commitment_of(&nym));
            let (prob, nonce) = match (disclosure, stored) {
                (
                    Disclosure::Opening,
                    Some(Preimage::Of {
                        prob_bits, nonce, ..
                    }),
                ) => (f64::from_bits(prob_bits), nonce),
                (Disclosure::Opening, _) => (0.5, nonce(0)),
                (Disclosure::Value { prob: p, nonce: n }, _) => (prob(p), nonce(n)),
            };
            plain(Event::Reveal { nym, prob, nonce })
        }
        Op::Score(outcome) => plain(Event::Score { outcome }),
        Op::Resolve(passed) => plain(Event::Resolve { passed }),
        Op::Appeal {
            within_window,
            covers_stake,
        } => plain(Event::Appeal {
            within_window,
            reputation_covers_stake: covers_stake,
        }),
        Op::AppealExpires => plain(Event::AppealExpires),
        Op::Pilot1Batch { enough, passed } => plain(Event::Pilot1Batch {
            enough_respondents: enough,
            passed,
        }),
        Op::Pilot2Batch { batch, passed } => plain(Event::Pilot2Batch {
            batch_size: batch,
            passed,
        }),
        Op::Administer => plain(Event::Administer),
        Op::Revalidate(emerging_dif) => plain(Event::Revalidate { emerging_dif }),
        Op::ExposureLimit => plain(Event::ExposureLimit),
    }
}

fn who() -> impl Strategy<Value = Who> {
    prop_oneof![
        any::<u8>().prop_map(Who::Pending),
        any::<u8>().prop_map(Who::Panelist),
        any::<u8>().prop_map(Who::Anyone),
    ]
}

fn seal() -> impl Strategy<Value = Seal> {
    prop_oneof![
        3 => any::<(u8, u8)>().prop_map(|(prob, nonce)| Seal::Honest { prob, nonce }),
        1 => any::<(u8, u8, u8)>()
            .prop_map(|(from, prob, nonce)| Seal::Copied { from, prob, nonce }),
        1 => any::<(u8, u8)>().prop_map(|(prob, nonce)| Seal::Lifted { prob, nonce }),
        1 => any::<u8>().prop_map(Seal::Opaque),
    ]
}

fn disclosure() -> impl Strategy<Value = Disclosure> {
    prop_oneof![
        Just(Disclosure::Opening),
        any::<(u8, u8)>().prop_map(|(prob, nonce)| Disclosure::Value { prob, nonce }),
    ]
}

/// Any event, whatever the phase.
fn arbitrary_op() -> impl Strategy<Value = Op> {
    prop_oneof![
        any::<bool>().prop_map(Op::Admit),
        (
            0usize..=13,
            any::<u8>(),
            prop::option::weighted(0.3, any::<(u8, u8)>()),
            any::<bool>()
        )
            .prop_map(|(size, start, dup, other)| Op::Assign {
                size,
                start,
                dup,
                other
            }),
        (who(), seal()).prop_map(|(w, s)| Op::Commit(w, s)),
        Just(Op::CloseCommits),
        (who(), disclosure()).prop_map(|(w, d)| Op::Reveal(w, d)),
        prop::sample::select(OUTCOMES.to_vec()).prop_map(Op::Score),
        any::<bool>().prop_map(Op::Resolve),
        any::<(bool, bool)>().prop_map(|(within_window, covers_stake)| Op::Appeal {
            within_window,
            covers_stake
        }),
        Just(Op::AppealExpires),
        any::<(bool, bool)>().prop_map(|(enough, passed)| Op::Pilot1Batch { enough, passed }),
        (0usize..=4, any::<bool>()).prop_map(|(batch, passed)| Op::Pilot2Batch { batch, passed }),
        Just(Op::Administer),
        any::<bool>().prop_map(Op::Revalidate),
        Just(Op::ExposureLimit),
    ]
}

/// A walk: the deposit's four proofs, then the events. Three steps in four are the event
/// the protocol expects next, so walks reach every phase; the fourth is anything.
fn walk() -> impl Strategy<Value = ([bool; 4], Vec<Op>)> {
    let op = prop_oneof![
        3 => (prop::bool::weighted(0.03), prop::bool::weighted(0.2), any::<u8>())
            .prop_map(|(derail, stray, pick)| Op::Next(Knob { derail, stray, pick })),
        1 => arbitrary_op(),
    ];
    (
        prop_oneof![3 => Just([true; 4]), 1 => any::<[bool; 4]>()],
        prop::collection::vec(op, 0..128),
    )
}

// ------------------------------------------ the checks ------------------------------------------

/// The round invariants, on every state a walk reaches.
fn well_formed(s: &State) -> Result<(), TestCaseError> {
    let none: Vec<(Nym, f64)> = Vec::new();
    let (panel, commits, reveals) = match s {
        State::InReview { panel, commits, .. } => (panel, commits, &none),
        State::Revealing {
            panel,
            commits,
            reveals,
            ..
        } => (panel, commits, reveals),
        _ => return Ok(()),
    };
    let panelists: HashSet<Nym> = panel.iter().copied().collect();
    prop_assert!(
        [7, 9, 11].contains(&panel.len()),
        "panel of {}",
        panel.len()
    );
    prop_assert_eq!(panelists.len(), panel.len(), "a repeated panelist");
    let committers: HashSet<Nym> = commits.iter().map(|(n, _)| *n).collect();
    prop_assert_eq!(committers.len(), commits.len(), "a nym committed twice");
    prop_assert!(committers.is_subset(&panelists), "a commit from outside");
    let revealers: HashSet<Nym> = reveals.iter().map(|(n, _)| *n).collect();
    prop_assert_eq!(revealers.len(), reveals.len(), "a nym revealed twice");
    prop_assert!(revealers.is_subset(&committers), "a reveal with no commit");
    prop_assert!(reveals.iter().all(|(_, p)| (0.0..=1.0).contains(p)));
    Ok(())
}

/// The transition invariants. `pilot1_seen`: the walk has been in `Pilot1`.
fn legal_transition(
    before: &State,
    event: &Event,
    got: &Result<State, Invalid>,
    pilot1_seen: bool,
) -> Result<(), TestCaseError> {
    if matches!(before, State::Rejected(_) | State::Retired(_)) {
        prop_assert_eq!(
            got,
            &Err(Invalid::UnexpectedEvent),
            "a terminal state was left"
        );
    }
    if let (Event::Score { .. }, Ok(_)) = (event, got) {
        let everyone = match before {
            State::Revealing { panel, reveals, .. } => {
                panel.iter().all(|p| reveals.iter().any(|(n, _)| n == p))
            }
            _ => false,
        };
        prop_assert!(everyone, "scored before every panelist revealed");
    }
    match got {
        Ok(State::ActivePool) if *before != State::ActivePool => {
            let from_pilot2 = matches!(before, State::Pilot2 { .. });
            prop_assert!(
                from_pilot2 && pilot1_seen,
                "the pool entered from {:?}",
                before
            );
        }
        Ok(State::Pilot2 { .. }) if !matches!(before, State::Pilot2 { .. }) => {
            let from_pilot1 = matches!(before, State::Pilot1 { .. });
            prop_assert!(from_pilot1, "Pilot2 entered from {:?}", before);
        }
        _ => {}
    }
    Ok(())
}

/// A name for the state, and for each rejection, to measure what the walks cover.
fn label(s: &State) -> String {
    match s {
        State::InReview { .. } => "InReview".into(),
        State::Revealing { .. } => "Revealing".into(),
        other => format!("{other:?}"),
    }
}

/// Runs one walk, checking every step against the model and the invariants. Returns the
/// states reached and the rejections met.
fn run_walk(flags: [bool; 4], ops: &[Op]) -> Result<HashSet<String>, TestCaseError> {
    let mut seen = HashSet::new();
    let [source, identity, quota, fresh] = flags;
    let deposited = deposit(source, identity, quota, fresh);
    prop_assert_eq!(&deposited, &model_deposit(flags).map(|p| concrete(&p)));
    if let Err(why) = deposited {
        seen.insert(format!("{why:?}"));
    }

    // A rejected deposit never exists; the walk goes on from an accepted one.
    let mut phase = Phase::Deposited;
    let mut state = deposit(true, true, true, true).unwrap();
    let mut accepted = Vec::new();
    let mut pilot1_seen = false;
    seen.insert(label(&state));
    for op in ops {
        let (event, preimage) = plan(op, &phase);
        let expected = model_step(&phase, &event, preimage);
        let got = step(state.clone(), event.clone());
        prop_assert_eq!(
            &got,
            &expected.clone().map(|p| concrete(&p)),
            "{:?} in {:?}",
            event,
            state
        );
        legal_transition(&state, &event, &got, pilot1_seen)?;
        match (got, expected) {
            (Ok(next), Ok(next_phase)) => {
                well_formed(&next)?;
                pilot1_seen |= matches!(next, State::Pilot1 { .. });
                seen.insert(label(&next));
                accepted.push(event);
                state = next;
                phase = next_phase;
            }
            (Err(why), _) => {
                seen.insert(format!("{why:?}"));
            }
            _ => unreachable!("compared above"),
        }
    }

    // The rejected events changed nothing: the accepted ones alone reach the same state.
    let mut replay = deposit(true, true, true, true).unwrap();
    for event in accepted {
        let again = step(replay, event.clone());
        prop_assert!(again.is_ok(), "{:?} is refused on replay", event);
        replay = again.unwrap();
    }
    prop_assert_eq!(replay, state);
    Ok(seen)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    /// Every step of every walk is the transition, or the exact rejection, the model
    /// predicts; every walk keeps the lifecycle invariants.
    #[test]
    fn random_walks_agree_with_the_model((flags, ops) in walk()) {
        run_walk(flags, &ops)?;
    }
}

/// The walks are not vacuous: over a fixed sample, they reach every state the machine can
/// be in (both `Pilot` flavours, every reject and retirement reason) and meet every
/// `Invalid`.
#[test]
fn the_walks_cover_every_state_and_every_rejection() {
    let mut runner = TestRunner::deterministic();
    let strategy = walk();
    let mut seen = HashSet::new();
    for _ in 0..512 {
        let (flags, ops) = strategy.new_tree(&mut runner).unwrap().current();
        seen.extend(run_walk(flags, &ops).unwrap());
    }
    let states = [
        "Deposited",
        "Admitted",
        "InReview",
        "Revealing",
        "SupplementaryReview",
        "AppealEligible",
        "Pilot1 { appealed: false }",
        "Pilot1 { appealed: true }",
        "Pilot2 { appealed: false }",
        "Pilot2 { appealed: true }",
        "ActivePool",
        "Rejected(Defect)",
        "Rejected(Polarized)",
        "Rejected(Screen)",
        "Rejected(Dif)",
        "Rejected(Borderline)",
        "Retired(EmergingDif)",
        "Retired(Exposure)",
    ];
    let rejections = [
        "NoPrimarySource",
        "UnprovenIdentity",
        "OverQuota",
        "DuplicateCid",
        "SeedNotFromCheckpoint",
        "PanelSizeInvalid",
        "DuplicatePanelist",
        "NotInPanel",
        "AlreadyCommitted",
        "NoCommit",
        "AlreadyRevealed",
        "RevealMismatch",
        "ProbabilityOutOfRange",
        "PartialEpoch",
        "AppealWindowClosed",
        "InsufficientReputation",
        "NotEnoughRespondents",
        "BatchTooSmall",
        "UnexpectedEvent",
    ];
    for name in states.iter().chain(&rejections) {
        assert!(seen.contains(*name), "no walk reached {name}");
    }
}
