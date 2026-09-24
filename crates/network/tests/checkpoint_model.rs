//! Model-based test of the consortium checkpoint client (`CheckpointClient::{ingest,
//! ingest_with_log}`, `docs/08` §9.4, T43). A reference model, written here from the §9.4
//! table and the `CheckpointUpdate` / `CheckpointReject` contracts rather than from the
//! client, predicts the update and the trusted checkpoint after every incoming checkpoint
//! of a random walk. Histories are payload sequences that share prefixes, and the model
//! decides whether a log extends a checkpoint as a prefix relation on them, never through
//! `verify_extends`. The walks mix honest extensions, replays, lower and equal heights,
//! forks, arbitrary heads, foreign networks and member sets, and signature sets that are
//! short, repeated, misattributed, over another message (up to a whole quorum), by a
//! stranger or listed past the member list; the local logs are behind, level with or
//! ahead of the checkpoints, on the same history or another. Independently of the model,
//! every step is checked against the client invariants:
//!
//! - the trusted height never decreases;
//! - a wrong network or member set, or too few signatures, never changes the state, nor
//!   does any update other than `Accepted`;
//! - two different heads at the trusted height are always `Forked`;
//! - with the log, a higher checkpoint is accepted iff the log extends both the trusted
//!   checkpoint and the new one.

use ed25519_dalek::Signature;
use network::cid::cid;
use network::consortium::{
    Checkpoint, CheckpointClient, CheckpointReject, CheckpointUpdate, Consortium, Member,
};
use network::log::TransparencyLog;
use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::TestRunner;
use std::collections::HashSet;

const NET: [u8; 32] = [0xAA; 32];

/// Every history has this many entries.
const LEN: usize = 12;

/// The same bytes with the first one changed: another network, member set or head.
fn flipped(mut bytes: [u8; 32]) -> [u8; 32] {
    bytes[0] ^= 1;
    bytes
}

fn log_of(payloads: &[u8]) -> TransparencyLog {
    let mut log = TransparencyLog::new();
    for p in payloads {
        log.append(cid(&[*p]));
    }
    log
}

// ------------------------------------ the reference model ------------------------------------

/// What the model knows of a checkpoint's head: the payloads it commits to, or that it is
/// an arbitrary value no log has.
#[derive(Clone, Debug, PartialEq)]
enum Head {
    Payloads(Vec<u8>),
    Raw(u8),
}

impl Head {
    /// A log whose payloads are `log` extends a checkpoint with this head iff the
    /// checkpoint's payloads are a prefix of the log's.
    fn extended_by(&self, log: &[u8]) -> bool {
        match self {
            Head::Payloads(p) => log.starts_with(p),
            Head::Raw(_) => false,
        }
    }
}

/// An incoming checkpoint as the model sees it.
struct Incoming<'a> {
    cp: Checkpoint,
    head: &'a Head,
    right_network: bool,
    right_member_set: bool,
    /// Distinct members with a genuine signature over exactly `cp`, listed under their index.
    signers: usize,
}

/// The model: the trusted checkpoint, if any, and what its head commits to.
#[derive(Clone, Debug, Default)]
struct Model {
    trusted: Option<(Checkpoint, Head)>,
}

impl Model {
    /// The §9.4 decision on `incoming`, for a consortium of threshold `t`, with the local
    /// log's payloads when the client holds a log.
    fn ingest(&mut self, incoming: &Incoming, t: usize, log: Option<&[u8]>) -> CheckpointUpdate {
        use CheckpointReject::*;
        use CheckpointUpdate::*;
        let cp = incoming.cp;
        let update = if !incoming.right_network {
            Rejected(WrongNetwork)
        } else if !incoming.right_member_set {
            Rejected(WrongMemberSet)
        } else if incoming.signers < t {
            Rejected(InsufficientSignatures)
        } else {
            match (&self.trusted, log) {
                // Trust on first use, with or without a log.
                (None, _) => Accepted,
                (Some((trusted, _)), _) if cp.height < trusted.height => Stale,
                (Some((trusted, head)), _) if cp.height == trusted.height => {
                    if incoming.head == head {
                        Stale
                    } else {
                        Forked {
                            trusted: *trusted,
                            conflicting: cp,
                        }
                    }
                }
                // Higher: without the log it cannot be vetted.
                (Some(_), None) => Accepted,
                // Higher, with the log: a log too short to show a head has not caught up;
                // one that shows another head than the trusted one left the trusted
                // history; one that shows another head than the new one proves a fork.
                (Some((trusted, head)), Some(log)) => {
                    let shows = |height: u64| log.len() as u64 >= height;
                    if !shows(trusted.height) {
                        Rejected(LogBehind)
                    } else if !head.extended_by(log) {
                        Rejected(LocalLogDiverged)
                    } else if incoming.head.extended_by(log) {
                        Accepted
                    } else if !shows(cp.height) {
                        Rejected(LogBehind)
                    } else {
                        Forked {
                            trusted: *trusted,
                            conflicting: cp,
                        }
                    }
                }
            }
        };
        if update == Accepted {
            self.trusted = Some((cp, incoming.head.clone()));
        }
        update
    }
}

// ------------------------------------------ the walks ------------------------------------------

/// Where an incoming checkpoint's head comes from.
#[derive(Clone, Copy, Debug)]
enum Source {
    /// The head of history `h` at the chosen height.
    History(u8),
    /// An arbitrary head.
    Raw(u8),
    /// The trusted checkpoint, replayed (else `History(0)`).
    Trusted,
}

#[derive(Clone, Copy, Debug)]
enum Height {
    /// Relative to the trusted height (to 0 when nothing is trusted).
    Step(i8),
    Absolute(u8),
}

/// One entry of a signature list.
#[derive(Clone, Copy, Debug)]
enum Sig {
    /// Member `i` signs the checkpoint, listed under its own index.
    Genuine(u8),
    /// Member `i`'s genuine signature listed under index `j`.
    Misattributed(u8, u8),
    /// Member `i` signs a checkpoint that differs in one field.
    OtherMessage(u8, u8),
    /// A key outside the consortium, listed under index `j`.
    Stranger(u8),
    /// A genuine signature listed under an index past the member list.
    OutOfRange(u8),
}

/// The history a local log follows.
#[derive(Clone, Copy, Debug)]
enum Follows {
    /// The incoming checkpoint's (history 0 for an arbitrary head).
    Incoming,
    /// The trusted checkpoint's (history 0 if none).
    Trusted,
    History(u8),
}

#[derive(Clone, Debug)]
struct Step {
    source: Source,
    height: Height,
    foreign_network: bool,
    foreign_member_set: bool,
    /// `None`: a quorum of exactly `t` members signs; `Some(k)`: `k % (n + 1)` members do,
    /// starting from member `rotate`.
    signers: Option<u8>,
    rotate: u8,
    /// The quorum signed a checkpoint that differs in one field, not this one.
    forged: Option<u8>,
    extra: Vec<Sig>,
    /// `None`: `ingest`; `Some`: `ingest_with_log` over a log following a history, with a
    /// length relative to the incoming height.
    log: Option<(Follows, i8)>,
}

#[derive(Clone, Debug)]
struct Walk {
    n: usize,
    t: usize,
    /// A history and two that follow it up to a fork point, then go their own way (which
    /// may be the same way).
    histories: Vec<Vec<u8>>,
    steps: Vec<Step>,
}

fn sig() -> impl Strategy<Value = Sig> {
    prop_oneof![
        3 => any::<u8>().prop_map(Sig::Genuine),
        1 => any::<(u8, u8)>().prop_map(|(i, j)| Sig::Misattributed(i, j)),
        1 => any::<(u8, u8)>().prop_map(|(i, which)| Sig::OtherMessage(i, which)),
        1 => any::<u8>().prop_map(Sig::Stranger),
        1 => any::<u8>().prop_map(Sig::OutOfRange),
    ]
}

fn step() -> impl Strategy<Value = Step> {
    let source = prop_oneof![
        6 => (0u8..3).prop_map(Source::History),
        1 => any::<u8>().prop_map(Source::Raw),
        1 => Just(Source::Trusted),
    ];
    let height = prop_oneof![
        5 => (-2i8..=3).prop_map(Height::Step),
        1 => (0..=LEN as u8 + 2).prop_map(Height::Absolute),
    ];
    let follows = prop_oneof![
        3 => Just(Follows::Incoming),
        2 => Just(Follows::Trusted),
        1 => (0u8..3).prop_map(Follows::History),
    ];
    (
        source,
        height,
        prop::bool::weighted(0.06),
        prop::bool::weighted(0.06),
        prop_oneof![4 => Just(None), 1 => any::<u8>().prop_map(Some)],
        any::<u8>(),
        prop::option::weighted(0.08, any::<u8>()),
        prop_oneof![6 => Just(Vec::new()), 1 => prop::collection::vec(sig(), 1..=2)],
        prop::option::weighted(0.5, (follows, -3i8..=2)),
    )
        .prop_map(
            |(
                source,
                height,
                foreign_network,
                foreign_member_set,
                signers,
                rotate,
                forged,
                extra,
                log,
            )| Step {
                source,
                height,
                foreign_network,
                foreign_member_set,
                signers,
                rotate,
                forged,
                extra,
                log,
            },
        )
}

fn walk() -> impl Strategy<Value = Walk> {
    let committee = (1usize..=3).prop_flat_map(|n| (Just(n), 1..=n));
    let histories = (
        prop::collection::vec(0u8..3, LEN),
        prop::collection::vec((0..=LEN, prop::collection::vec(0u8..3, LEN)), 2),
    )
        .prop_map(|(first, forks)| {
            let mut histories = vec![first.clone()];
            for (at, other) in forks {
                let mut h = first[..at].to_vec();
                h.extend_from_slice(&other[at..]);
                histories.push(h);
            }
            histories
        });
    (committee, histories, prop::collection::vec(step(), 0..20)).prop_map(
        |((n, t), histories, steps)| Walk {
            n,
            t,
            histories,
            steps,
        },
    )
}

/// A checkpoint that differs from `cp` in one field.
fn altered(cp: &Checkpoint, which: u8) -> Checkpoint {
    let mut other = *cp;
    match which % 4 {
        0 => other.height ^= 1,
        1 => other.head = flipped(other.head),
        2 => other.network_id = flipped(other.network_id),
        _ => other.member_set_hash = flipped(other.member_set_hash),
    }
    other
}

/// A walk's fixed parts: the consortium, its members and a stranger's key.
struct Committee {
    members: Vec<Member>,
    stranger: Member,
    member_set_hash: [u8; 32],
    /// The member set of another consortium: these members and the stranger.
    foreign_member_set_hash: [u8; 32],
}

impl Committee {
    fn new(n: usize) -> (Self, Vec<ed25519_dalek::VerifyingKey>) {
        let members: Vec<Member> = (0..n as u8)
            .map(|i| Member::from_seed([i + 1; 32]))
            .collect();
        let stranger = Member::from_seed([0xEE; 32]);
        let keys: Vec<_> = members.iter().map(|m| m.public()).collect();
        let mut other = keys.clone();
        other.push(stranger.public());
        let committee = Committee {
            member_set_hash: Consortium::new(keys.clone(), 1).member_set_hash(),
            foreign_member_set_hash: Consortium::new(other, 1).member_set_hash(),
            members,
            stranger,
        };
        (committee, keys)
    }

    /// The signature list `s` asks for over `cp`, and how many distinct members in it
    /// genuinely signed exactly `cp` under their own index.
    fn sign(&self, s: &Step, t: usize, cp: &Checkpoint) -> (Vec<(usize, Signature)>, usize) {
        let n = self.members.len();
        let member = |i: u8| i as usize % n;
        let k = s.signers.map_or(t, |k| k as usize % (n + 1));
        let quorum = (0..k).map(|i| (i + s.rotate as usize) % n);
        let mut sigs: Vec<(usize, Signature)> = Vec::new();
        let mut genuine = HashSet::new();
        for i in quorum {
            match s.forged {
                None => {
                    sigs.push((i, self.members[i].sign(cp)));
                    genuine.insert(i);
                }
                Some(which) => sigs.push((i, self.members[i].sign(&altered(cp, which)))),
            }
        }
        for extra in &s.extra {
            match *extra {
                Sig::Genuine(i) => {
                    sigs.push((member(i), self.members[member(i)].sign(cp)));
                    genuine.insert(member(i));
                }
                Sig::Misattributed(i, j) => {
                    sigs.push((member(j), self.members[member(i)].sign(cp)));
                    if member(i) == member(j) {
                        genuine.insert(member(i));
                    }
                }
                Sig::OtherMessage(i, which) => {
                    let other = altered(cp, which);
                    sigs.push((member(i), self.members[member(i)].sign(&other)));
                }
                Sig::Stranger(j) => sigs.push((member(j), self.stranger.sign(cp))),
                Sig::OutOfRange(j) => sigs.push((n + j as usize, self.members[0].sign(cp))),
            }
        }
        (sigs, genuine.len())
    }
}

// ------------------------------------------ the checks ------------------------------------------

/// A name for an update in its context, to measure what the walks cover.
fn label(
    update: &CheckpointUpdate,
    before: Option<Checkpoint>,
    cp: &Checkpoint,
    log_len: Option<usize>,
) -> String {
    let path = if log_len.is_some() {
        "with log"
    } else {
        "without log"
    };
    let level = |h: u64| {
        if h == cp.height {
            "at the trusted height"
        } else {
            "higher"
        }
    };
    match (update, before) {
        (CheckpointUpdate::Accepted, None) => format!("Accepted first {path}"),
        (CheckpointUpdate::Accepted, Some(b)) => format!("Accepted {} {path}", level(b.height)),
        (CheckpointUpdate::Forked { trusted, .. }, _) => {
            format!("Forked {} {path}", level(trusted.height))
        }
        (CheckpointUpdate::Stale, Some(b)) if b.height == cp.height => "Stale replay".into(),
        (CheckpointUpdate::Stale, _) => "Stale lower".into(),
        (CheckpointUpdate::Rejected(CheckpointReject::LogBehind), Some(b)) => {
            let behind = if log_len < Some(b.height as usize) {
                "trusted"
            } else {
                "new"
            };
            format!("Rejected(LogBehind) the {behind} checkpoint")
        }
        (other, _) => format!("{other:?}"),
    }
}

/// Runs one walk against a fresh client, checking every step against the model and the
/// invariants. Returns what happened.
fn run_walk(w: &Walk) -> Result<HashSet<String>, TestCaseError> {
    let (committee, keys) = Committee::new(w.n);
    let msh = committee.member_set_hash;
    let mut client = CheckpointClient::new(NET, Consortium::new(keys, w.t));
    let mut model = Model::default();
    // The history each trusted checkpoint was taken from (for logs that follow it).
    let mut trusted_history = 0usize;
    let mut seen = HashSet::new();

    for s in &w.steps {
        let before = client.trusted().copied();
        let base = model.trusted.as_ref().map_or(0, |(cp, _)| cp.height as i64);
        let height = |cap: usize| match s.height {
            Height::Step(d) => (base + d as i64).clamp(0, cap as i64) as usize,
            Height::Absolute(h) => h as usize % (cap + 1),
        };

        // The incoming checkpoint, its head as the model sees it, and its history.
        let (mut cp, head, history) = match (s.source, &model.trusted) {
            (Source::Trusted, Some((cp, head))) => (*cp, head.clone(), Some(trusted_history)),
            (Source::Raw(tag), _) => {
                let tag = 1 + tag % 255;
                let cp = Checkpoint::new(NET, msh, height(LEN + 2) as u64, [tag; 32]);
                (cp, Head::Raw(tag), None)
            }
            (Source::History(_), _) | (Source::Trusted, None) => {
                let h = match s.source {
                    Source::History(h) => h as usize % w.histories.len(),
                    _ => 0,
                };
                let payloads = w.histories[h][..height(LEN)].to_vec();
                let cp = log_of(&payloads).checkpoint(NET, msh);
                (cp, Head::Payloads(payloads), Some(h))
            }
        };
        if s.foreign_network {
            cp.network_id = flipped(cp.network_id);
        }
        if s.foreign_member_set {
            cp.member_set_hash = committee.foreign_member_set_hash;
        }
        let (sigs, signers) = committee.sign(s, w.t, &cp);
        let incoming = Incoming {
            cp,
            head: &head,
            right_network: !s.foreign_network,
            right_member_set: !s.foreign_member_set,
            signers,
        };

        // The local log, for `ingest_with_log`.
        let local = s.log.map(|(follows, len)| {
            let h = match follows {
                Follows::Incoming => history.unwrap_or(0),
                Follows::Trusted => trusted_history,
                Follows::History(h) => h as usize % w.histories.len(),
            };
            let len = (cp.height as i64 + len as i64).clamp(0, LEN as i64) as usize;
            w.histories[h][..len].to_vec()
        });

        let expected = model.ingest(&incoming, w.t, local.as_deref());
        let (update, log) = match &local {
            None => (client.ingest(&cp, &sigs), None),
            Some(payloads) => {
                let log = log_of(payloads);
                (client.ingest_with_log(&cp, &sigs, &log), Some(log))
            }
        };
        let after = client.trusted().copied();
        prop_assert_eq!(&update, &expected, "{:?} after {:?}", cp, before);
        prop_assert_eq!(after, model.trusted.as_ref().map(|(cp, _)| *cp));
        if update == CheckpointUpdate::Accepted {
            trusted_history = history.unwrap_or(0);
        }

        // The invariants, from the client alone.
        if let Some(b) = before {
            prop_assert!(
                after.is_some_and(|a| a.height >= b.height),
                "trust went down"
            );
        }
        if update == CheckpointUpdate::Accepted {
            prop_assert_eq!(after, Some(cp));
        } else {
            prop_assert_eq!(after, before, "trust moved on {:?}", update);
        }
        let admissible = incoming.right_network && incoming.right_member_set && signers >= w.t;
        if !admissible {
            let refused = matches!(
                update,
                CheckpointUpdate::Rejected(
                    CheckpointReject::WrongNetwork
                        | CheckpointReject::WrongMemberSet
                        | CheckpointReject::InsufficientSignatures
                )
            );
            prop_assert!(refused, "an inadmissible checkpoint got {:?}", update);
        }
        if let (true, Some(b)) = (admissible, before) {
            if cp.height == b.height && cp.head != b.head {
                prop_assert_eq!(
                    &update,
                    &CheckpointUpdate::Forked {
                        trusted: b,
                        conflicting: cp
                    }
                );
            }
            if let (Some(log), true) = (&log, cp.height > b.height) {
                let both = log.verify_extends(&b).is_ok() && log.verify_extends(&cp).is_ok();
                prop_assert_eq!(update == CheckpointUpdate::Accepted, both);
            }
        }
        seen.insert(label(&update, before, &cp, local.as_ref().map(Vec::len)));
    }
    Ok(seen)
}

// Signature checks dominate the cost (milliseconds each in a debug build), so the walks are
// few but long; every run draws new ones.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// Every incoming checkpoint of every walk gets the update the model predicts, leaves
    /// the trust where the model has it, and keeps the client invariants.
    #[test]
    fn random_walks_agree_with_the_model(w in walk()) {
        run_walk(&w)?;
    }
}

/// The walks are not vacuous: over a fixed sample they meet every update the client can
/// give, on both paths.
#[test]
fn the_walks_cover_every_update() {
    let mut runner = TestRunner::deterministic();
    let strategy = walk();
    let mut seen = HashSet::new();
    for _ in 0..24 {
        let w = strategy.new_tree(&mut runner).unwrap().current();
        seen.extend(run_walk(&w).unwrap());
    }
    let wanted = [
        "Accepted first without log",
        "Accepted first with log",
        "Accepted higher without log",
        "Accepted higher with log",
        "Stale replay",
        "Stale lower",
        "Forked at the trusted height without log",
        "Forked at the trusted height with log",
        "Forked higher with log",
        "Rejected(WrongNetwork)",
        "Rejected(WrongMemberSet)",
        "Rejected(InsufficientSignatures)",
        "Rejected(LogBehind) the trusted checkpoint",
        "Rejected(LogBehind) the new checkpoint",
        "Rejected(LocalLogDiverged)",
    ];
    for name in wanted {
        assert!(seen.contains(name), "no walk reached {name}");
    }
}
