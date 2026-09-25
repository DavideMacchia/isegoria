//! Model-based test of the epoch orchestrator (`orchestrator::{review_round, run_item}`,
//! T43). A reference model, written from the §9.1 rules for a whole review round at once
//! rather than by replaying `lifecycle::step`, predicts the state `review_round` leaves, or
//! the first invalid move, and where `run_item` takes that state, or the exact `Invalid`.
//! The rounds are complete, partial or empty, with outsiders, with a nym that judges twice,
//! with out-of-range or NaN probabilities, on a short, long or repeated panel, and start
//! from `Admitted` or from another state.

use identity::nym::Nym;
use network::cid::{cid, Cid};
use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::TestRunner;
use protocol::gate::GateOutcome;
use protocol::lifecycle::{Invalid, RejectReason, State, K_MIN};
use protocol::orchestrator::{review_round, run_item, ItemVerdicts, Judgment};
use protocol::review::commit;
use std::collections::HashSet;

// ------------------------------------ the reference model ------------------------------------

/// `review_round` as one decision. Commitments hide their content, so every commit-time
/// rule (panel membership, one commit per nym) applies to the whole list before any
/// reveal-time rule (the probability); and a judgment reveals exactly what it committed,
/// so every reveal opens.
fn model_review_round(
    start: &State,
    item: Cid,
    panel: &[Nym],
    judgments: &[Judgment],
) -> Result<State, Invalid> {
    if *start != State::Admitted {
        return Err(Invalid::UnexpectedEvent);
    }
    let members: HashSet<Nym> = panel.iter().copied().collect();
    if ![7, 9, 11].contains(&panel.len()) {
        return Err(Invalid::PanelSizeInvalid);
    }
    if members.len() != panel.len() {
        return Err(Invalid::DuplicatePanelist);
    }
    let mut judged = HashSet::new();
    for j in judgments {
        if !members.contains(&j.nym) {
            return Err(Invalid::NotInPanel);
        }
        if !judged.insert(j.nym) {
            return Err(Invalid::AlreadyCommitted);
        }
    }
    if judgments.iter().any(|j| !(0.0..=1.0).contains(&j.prob)) {
        return Err(Invalid::ProbabilityOutOfRange);
    }
    Ok(State::Revealing {
        item,
        panel: panel.to_vec(),
        commits: judgments
            .iter()
            .map(|j| (j.nym, commit(j.prob, &j.nonce, j.nym, item)))
            .collect(),
        reveals: judgments.iter().map(|j| (j.nym, j.prob)).collect(),
    })
}

/// `run_item` as one decision: the round must be complete; the gate decides whether the
/// item enters the pilot (a band item if the D26 re-decision passes, a polarized one —
/// below the band or failing the re-decision as polarized (T59) — only on appeal); the
/// pilot's respondent floor and batch minimum refuse, its verdicts reject.
fn model_run_item(reviewed: &State, v: &ItemVerdicts) -> Result<State, Invalid> {
    let State::Revealing { panel, reveals, .. } = reviewed else {
        return Err(Invalid::UnexpectedEvent);
    };
    let panel: HashSet<Nym> = panel.iter().copied().collect();
    let revealed: HashSet<Nym> = reveals.iter().map(|(n, _)| *n).collect();
    if revealed != panel {
        return Err(Invalid::PartialEpoch);
    }
    // The band re-decision has three outcomes; a second band is not one of them.
    let effective = match v.gate {
        GateOutcome::SupplementaryReview => match v.band_outcome {
            GateOutcome::SupplementaryReview => return Err(Invalid::UnexpectedEvent),
            GateOutcome::Reject => return Ok(State::Rejected(RejectReason::Borderline)),
            other => other,
        },
        other => other,
    };
    let left_at_the_gate = match effective {
        GateOutcome::Reject => Some(RejectReason::Defect),
        GateOutcome::AppealEligible if !v.appealed => Some(RejectReason::Polarized),
        _ => None,
    };
    if let Some(why) = left_at_the_gate {
        return Ok(State::Rejected(why));
    }
    if !v.enough_respondents {
        return Err(Invalid::NotEnoughRespondents);
    }
    if !v.screen_passed {
        return Ok(State::Rejected(RejectReason::Screen));
    }
    if v.pilot2_batch_size < K_MIN {
        return Err(Invalid::BatchTooSmall);
    }
    if !v.dif_passed {
        return Ok(State::Rejected(RejectReason::Dif));
    }
    Ok(State::ActivePool)
}

// ------------------------------------------ the rounds ------------------------------------------

/// Nyms `nym_at(0..13)`: every panel is drawn from them.
const UNIVERSE: usize = 13;

fn nym_at(i: usize) -> Nym {
    Nym([1 + (i % UNIVERSE) as u8; 32])
}

/// A nym no panel contains.
fn outsider(i: u8) -> Nym {
    Nym([100 + i % 100; 32])
}

/// The probabilities a judgment may carry: the first `IN_RANGE` are in `[0, 1]`.
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
    f64::NEG_INFINITY,
];
const IN_RANGE: u8 = 6;

fn prob(i: u8) -> f64 {
    PROBS[i as usize % PROBS.len()]
}

fn nonce(i: u8) -> [u8; 32] {
    [i; 32]
}

/// The state a round starts from.
#[derive(Clone, Copy, Debug)]
enum Start {
    Admitted,
    Deposited,
    InReview,
    Revealing { complete: bool },
    Pilot1,
    Pool,
    Rejected,
}

/// A judgment inserted besides the panelists'.
#[derive(Clone, Copy, Debug)]
enum Extra {
    /// From someone outside the panel.
    Outsider { at: u8, who: u8 },
    /// A second judgment by a panelist who already judged: the same one, or another.
    Again { at: u8, of: u8, prob: u8, nonce: u8 },
}

#[derive(Clone, Debug)]
struct Round {
    start: Start,
    other_item: bool,
    size: usize,
    offset: u8,
    dup: Option<(u8, u8)>,
    /// A permutation of `0..UNIVERSE`; its entries below the panel size are the order in
    /// which the panelists judge.
    order: Vec<u8>,
    /// `None`: every panelist judges; `Some(t)`: only the first `t % (size + 1)` do.
    judges: Option<u8>,
    extras: Vec<Extra>,
    probs: Vec<u8>,
    nonces: Vec<u8>,
    verdicts: ItemVerdicts,
}

impl Round {
    fn start_state(&self) -> State {
        let item = cid(b"an earlier item");
        let panel: Vec<Nym> = (0..7).map(nym_at).collect();
        match self.start {
            Start::Admitted => State::Admitted,
            Start::Deposited => State::Deposited,
            Start::InReview => State::InReview {
                item,
                panel,
                commits: Vec::new(),
            },
            Start::Revealing { complete } => {
                let judged = &panel[..if complete { 7 } else { 6 }];
                State::Revealing {
                    item,
                    commits: judged
                        .iter()
                        .map(|&n| (n, commit(0.5, &nonce(9), n, item)))
                        .collect(),
                    reveals: judged.iter().map(|&n| (n, 0.5)).collect(),
                    panel,
                }
            }
            Start::Pilot1 => State::Pilot1 { appealed: false },
            Start::Pool => State::ActivePool,
            Start::Rejected => State::Rejected(RejectReason::Defect),
        }
    }

    fn item(&self) -> Cid {
        cid(if self.other_item {
            b"item B"
        } else {
            b"item A"
        })
    }

    fn panel(&self) -> Vec<Nym> {
        let mut panel: Vec<Nym> = (0..self.size)
            .map(|i| nym_at(self.offset as usize + i))
            .collect();
        if let (Some((from, to)), true) = (self.dup, self.size > 0) {
            panel[to as usize % self.size] = panel[from as usize % self.size];
        }
        panel
    }

    fn judgments(&self, panel: &[Nym]) -> Vec<Judgment> {
        let mut judges: Vec<Nym> = self
            .order
            .iter()
            .map(|&p| p as usize)
            .filter(|&p| p < panel.len())
            .map(|p| panel[p])
            .collect();
        if let Some(t) = self.judges {
            judges.truncate(t as usize % (panel.len() + 1));
        }
        let mut judgments: Vec<Judgment> = judges
            .iter()
            .enumerate()
            .map(|(i, &nym)| Judgment {
                nym,
                prob: prob(self.probs[i]),
                nonce: nonce(self.nonces[i]),
            })
            .collect();
        for extra in &self.extras {
            let at = |a: u8, len: usize| a as usize % (len + 1);
            match *extra {
                Extra::Outsider { at: a, who } => judgments.insert(
                    at(a, judgments.len()),
                    Judgment {
                        nym: outsider(who),
                        prob: 0.5,
                        nonce: nonce(0),
                    },
                ),
                Extra::Again {
                    at: a,
                    of,
                    prob: p,
                    nonce: n,
                } if !judgments.is_empty() => {
                    let first = judgments[of as usize % judgments.len()];
                    let again = if p % 2 == 0 {
                        first
                    } else {
                        Judgment {
                            prob: prob(p),
                            nonce: nonce(n),
                            ..first
                        }
                    };
                    judgments.insert(at(a, judgments.len()), again);
                }
                Extra::Again { .. } => {}
            }
        }
        judgments
    }
}

fn verdicts() -> impl Strategy<Value = ItemVerdicts> {
    (
        prop::sample::select(vec![
            GateOutcome::Pass,
            GateOutcome::SupplementaryReview,
            GateOutcome::AppealEligible,
            GateOutcome::Reject,
        ]),
        (
            any::<bool>(),
            prop::sample::select(vec![
                GateOutcome::Pass,
                GateOutcome::AppealEligible,
                GateOutcome::Reject,
                GateOutcome::SupplementaryReview,
            ]),
        ),
        prop::bool::weighted(0.85),
        prop::bool::weighted(0.75),
        prop::bool::weighted(0.75),
        prop_oneof![1 => 0..K_MIN, 5 => K_MIN..K_MIN + 10],
    )
        .prop_map(
            |(gate, (appealed, band_outcome), enough, screen, dif, batch)| ItemVerdicts {
                gate,
                appealed,
                band_outcome,
                enough_respondents: enough,
                screen_passed: screen,
                dif_passed: dif,
                pilot2_batch_size: batch,
            },
        )
}

fn round() -> impl Strategy<Value = Round> {
    let start = prop_oneof![
        12 => Just(Start::Admitted),
        1 => Just(Start::Deposited),
        1 => Just(Start::InReview),
        1 => any::<bool>().prop_map(|complete| Start::Revealing { complete }),
        1 => Just(Start::Pilot1),
        1 => Just(Start::Pool),
        1 => Just(Start::Rejected),
    ];
    let size = prop_oneof![
        6 => prop::sample::select(vec![7usize, 9, 11]),
        1 => 0usize..=UNIVERSE,
    ];
    let extra = prop_oneof![
        any::<(u8, u8)>().prop_map(|(at, who)| Extra::Outsider { at, who }),
        any::<(u8, u8, u8, u8)>().prop_map(|(at, of, prob, nonce)| Extra::Again {
            at,
            of,
            prob,
            nonce
        }),
    ];
    let prob = prop_oneof![30 => 0..IN_RANGE, 1 => any::<u8>()];
    (
        start,
        any::<bool>(),
        size,
        any::<u8>(),
        prop::option::weighted(0.1, any::<(u8, u8)>()),
        Just((0..UNIVERSE as u8).collect::<Vec<u8>>()).prop_shuffle(),
        prop::option::weighted(0.4, any::<u8>()),
        prop_oneof![4 => Just(Vec::new()), 1 => prop::collection::vec(extra, 1..=2)],
        prop::collection::vec(prob, UNIVERSE),
        prop::collection::vec(any::<u8>(), UNIVERSE),
        verdicts(),
    )
        .prop_map(
            |(
                start,
                other_item,
                size,
                offset,
                dup,
                order,
                judges,
                extras,
                probs,
                nonces,
                verdicts,
            )| {
                Round {
                    start,
                    other_item,
                    size,
                    offset,
                    dup,
                    order,
                    judges,
                    extras,
                    probs,
                    nonces,
                    verdicts,
                }
            },
        )
}

// ------------------------------------------ the checks ------------------------------------------

/// A name for a result, to measure what the rounds cover.
fn outcome(r: &Result<State, Invalid>) -> String {
    match r {
        Ok(State::Revealing { .. }) => "Revealing".into(),
        Ok(s) => format!("{s:?}"),
        Err(why) => format!("{why:?}"),
    }
}

/// Runs `review_round` on one round and `run_item` on the state it leaves and on the start
/// state itself, checking each against the model. Returns what happened.
fn check_round(r: &Round) -> Result<Vec<String>, TestCaseError> {
    let (start, item, panel) = (r.start_state(), r.item(), r.panel());
    let judgments = r.judgments(&panel);
    let expected = model_review_round(&start, item, &panel, &judgments);
    let got = review_round(start.clone(), item, panel, &judgments);
    prop_assert_eq!(&got, &expected);
    let mut seen = vec![format!("round: {}", outcome(&got))];
    for reviewed in got.iter().chain([&start]) {
        let scored = run_item(reviewed.clone(), &r.verdicts);
        prop_assert_eq!(&scored, &model_run_item(reviewed, &r.verdicts));
        // Independently of the model: only a round every panelist revealed is scored.
        if scored.is_ok() {
            let complete = match reviewed {
                State::Revealing { panel, reveals, .. } => {
                    panel.iter().all(|p| reveals.iter().any(|(n, _)| n == p))
                }
                _ => false,
            };
            prop_assert!(complete, "an item went on from {:?}", reviewed);
        }
        seen.push(format!("item: {}", outcome(&scored)));
    }
    Ok(seen)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    /// For any review round, `review_round` and `run_item` do what the model predicts.
    #[test]
    fn review_rounds_and_items_agree_with_the_model(r in round()) {
        check_round(&r)?;
    }
}

/// The rounds are not vacuous: over a fixed sample, `review_round` meets every way a round
/// can fail and succeed, and `run_item` reaches every end of the pipeline.
#[test]
fn the_rounds_cover_every_outcome() {
    let mut runner = TestRunner::deterministic();
    let strategy = round();
    let mut seen = HashSet::new();
    for _ in 0..512 {
        let r = strategy.new_tree(&mut runner).unwrap().current();
        seen.extend(check_round(&r).unwrap());
    }
    let round = [
        "Revealing",
        "UnexpectedEvent",
        "PanelSizeInvalid",
        "DuplicatePanelist",
        "NotInPanel",
        "AlreadyCommitted",
        "ProbabilityOutOfRange",
    ];
    let item = [
        "ActivePool",
        "Rejected(Defect)",
        "Rejected(Borderline)",
        "Rejected(Polarized)",
        "Rejected(Screen)",
        "Rejected(Dif)",
        "PartialEpoch",
        "NotEnoughRespondents",
        "BatchTooSmall",
        "UnexpectedEvent",
    ];
    let wanted = round
        .iter()
        .map(|o| format!("round: {o}"))
        .chain(item.iter().map(|o| format!("item: {o}")));
    for name in wanted {
        assert!(seen.contains(&name), "no round reached {name}");
    }
}
