//! The T24 studies, their grids of cells and the seed of every run (`docs/13` §2, §4).

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Study {
    DifNull,
    DifPower,
    DifMisspec,
    DifNonuniform,
    DifTwoAxes,
    DifPoisoning,
    DifPoolScale,
    DtfError,
    BridgingSweep,
    BridgingCapture,
}

pub const STUDIES: [Study; 10] = [
    Study::DifNull,
    Study::DifPower,
    Study::DifMisspec,
    Study::DifNonuniform,
    Study::DifTwoAxes,
    Study::DifPoisoning,
    Study::DifPoolScale,
    Study::DtfError,
    Study::BridgingSweep,
    Study::BridgingCapture,
];

/// What a study's runs produce, hence its record schema.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Dif,
    Dtf,
    Sweep,
    Capture,
}

impl Study {
    pub fn name(self) -> &'static str {
        match self {
            Study::DifNull => "dif-null",
            Study::DifPower => "dif-power",
            Study::DifMisspec => "dif-misspec",
            Study::DifNonuniform => "dif-nonuniform",
            Study::DifTwoAxes => "dif-two-axes",
            Study::DifPoisoning => "dif-poisoning",
            Study::DifPoolScale => "dif-pool-scale",
            Study::DtfError => "dtf-error",
            Study::BridgingSweep => "bridging-sweep",
            Study::BridgingCapture => "bridging-capture",
        }
    }

    pub fn parse(name: &str) -> Option<Study> {
        STUDIES.into_iter().find(|s| s.name() == name)
    }

    pub fn kind(self) -> Kind {
        match self {
            Study::DtfError => Kind::Dtf,
            Study::BridgingSweep => Kind::Sweep,
            Study::BridgingCapture => Kind::Capture,
            _ => Kind::Dif,
        }
    }

    /// The `docs/08` claims and tests the study measures (`docs/13` §4).
    pub fn claims(self) -> &'static str {
        match self {
            Study::DifNull => "AT-DIF-01, DIF-008, the KR-20 floor (T53)",
            Study::DifPower => "AT-DIF-02, SC-2, STAT-001, the DIF cut (T35/T54)",
            Study::DifMisspec => "robustness to impact and guessing (docs/07 §14, D25)",
            Study::DifNonuniform => "AT-DIF-03, the discrimination-gap threshold (T40)",
            Study::DifTwoAxes => "AT-DIF-04",
            Study::DifPoisoning => "AT-DIF-07, SC-7",
            Study::DifPoolScale => "AT-DIF-09, DIF-009",
            Study::DtfError => "DIF-011, the sampling error of the DTF bound (T55)",
            Study::BridgingSweep => "SC-3, BRIDGE-002, BRIDGE-003, the gate's τ and ε",
            Study::BridgingCapture => "AT-BR-02, BRIDGE-005",
        }
    }
}

/// Which trial items lean, and how (`docs/13` §3.1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Layout {
    /// The first `n` trial items lean the same way on one axis.
    Campaign(usize),
    /// Two mirror pairs on one axis, `+ − + −`, the other items clean.
    Mirror,
    /// `n` items lean on each of two independent axes.
    TwoAxes(usize),
}

/// Coordinated respondents: the last `fraction` of the sample (`docs/13` §3.1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Attack {
    None,
    /// They answer the last `targets` trial items wrong, the rest honestly.
    Inject {
        fraction: f64,
        targets: usize,
    },
    /// They answer the leaning items as if they leaned on nothing.
    Mask {
        fraction: f64,
    },
}

/// A latent-DIF batch: `n` respondents, `anchors` anchors, `k` trial items (`docs/13` §3.1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DifDesign {
    pub n: usize,
    pub anchors: usize,
    pub k: usize,
    pub layout: Layout,
    pub delta: f64,
    pub alpha: f64,
    pub pi: f64,
    pub impact: f64,
    pub guess: f64,
    pub attack: Attack,
}

impl Default for DifDesign {
    fn default() -> Self {
        DifDesign {
            n: 3000,
            anchors: 60,
            k: 8,
            layout: Layout::Campaign(0),
            delta: 0.0,
            alpha: 0.0,
            pi: 0.5,
            impact: 0.0,
            guess: 0.0,
            attack: Attack::None,
        }
    }
}

/// The Level A mirror design (`docs/13` §3.2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SweepDesign {
    pub n: usize,
    pub share: f64,
    pub per_reviewer: usize,
    pub noise: f64,
}

/// Boosters on one partisan item of the reference fixture (`docs/13` §3.3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaptureDesign {
    pub item: usize,
    pub own: usize,
    pub step: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cell {
    Dif(DifDesign),
    Sweep(SweepDesign),
    Capture(CaptureDesign),
}

fn layout_key(layout: Layout) -> String {
    match layout {
        Layout::Campaign(n) => format!("c{n}"),
        Layout::Mirror => "m".to_string(),
        Layout::TwoAxes(n) => format!("x{n}"),
    }
}

fn parse_layout(s: &str) -> Option<Layout> {
    match s.split_at_checked(1)? {
        ("c", n) => Some(Layout::Campaign(n.parse().ok()?)),
        ("m", "") => Some(Layout::Mirror),
        ("x", n) => Some(Layout::TwoAxes(n.parse().ok()?)),
        _ => None,
    }
}

fn attack_key(attack: Attack) -> String {
    match attack {
        Attack::None => "none".to_string(),
        Attack::Inject { fraction, targets } => format!("inj:{fraction}:{targets}"),
        Attack::Mask { fraction } => format!("mask:{fraction}"),
    }
}

fn parse_attack(s: &str) -> Option<Attack> {
    let parts: Vec<&str> = s.split(':').collect();
    match parts.as_slice() {
        ["none"] => Some(Attack::None),
        ["inj", f, t] => Some(Attack::Inject {
            fraction: f.parse().ok()?,
            targets: t.parse().ok()?,
        }),
        ["mask", f] => Some(Attack::Mask {
            fraction: f.parse().ok()?,
        }),
        _ => None,
    }
}

/// `key=value` pairs separated by spaces, exactly the `names` given, in any order.
fn fields<'a>(key: &'a str, names: &[&str]) -> Option<BTreeMap<&'a str, &'a str>> {
    let map: BTreeMap<&str, &str> = key
        .split(' ')
        .map(|kv| kv.split_once('='))
        .collect::<Option<_>>()?;
    let expected = map.len() == names.len() && names.iter().all(|n| map.contains_key(n));
    expected.then_some(map)
}

impl Cell {
    /// The cell's name in records and seeds: stable, space-separated `key=value` pairs.
    pub fn key(&self) -> String {
        match self {
            Cell::Dif(d) => format!(
                "n={} a={} k={} lay={} d={} al={} pi={} im={} g={} atk={}",
                d.n,
                d.anchors,
                d.k,
                layout_key(d.layout),
                d.delta,
                d.alpha,
                d.pi,
                d.impact,
                d.guess,
                attack_key(d.attack)
            ),
            Cell::Sweep(s) => format!("n={} s={} r={} e={}", s.n, s.share, s.per_reviewer, s.noise),
            Cell::Capture(c) => format!("item={} own={} step={}", c.item, c.own, c.step),
        }
    }

    pub fn parse(study: Study, key: &str) -> Option<Cell> {
        match study.kind() {
            Kind::Dif | Kind::Dtf => {
                let f = fields(
                    key,
                    &["n", "a", "k", "lay", "d", "al", "pi", "im", "g", "atk"],
                )?;
                Some(Cell::Dif(DifDesign {
                    n: f["n"].parse().ok()?,
                    anchors: f["a"].parse().ok()?,
                    k: f["k"].parse().ok()?,
                    layout: parse_layout(f["lay"])?,
                    delta: f["d"].parse().ok()?,
                    alpha: f["al"].parse().ok()?,
                    pi: f["pi"].parse().ok()?,
                    impact: f["im"].parse().ok()?,
                    guess: f["g"].parse().ok()?,
                    attack: parse_attack(f["atk"])?,
                }))
            }
            Kind::Sweep => {
                let f = fields(key, &["n", "s", "r", "e"])?;
                Some(Cell::Sweep(SweepDesign {
                    n: f["n"].parse().ok()?,
                    share: f["s"].parse().ok()?,
                    per_reviewer: f["r"].parse().ok()?,
                    noise: f["e"].parse().ok()?,
                }))
            }
            Kind::Capture => {
                let f = fields(key, &["item", "own", "step"])?;
                Some(Cell::Capture(CaptureDesign {
                    item: f["item"].parse().ok()?,
                    own: f["own"].parse().ok()?,
                    step: f["step"].parse().ok()?,
                }))
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grid {
    /// The grid `docs/13` §4 specifies.
    Full,
    /// One tiny cell per study: for tests and a first check of a machine.
    Smoke,
}

fn dif(d: DifDesign) -> Cell {
    Cell::Dif(d)
}

fn biased(n: usize, k: usize, count: usize, delta: f64, pi: f64) -> DifDesign {
    DifDesign {
        n,
        k,
        layout: Layout::Campaign(count),
        delta,
        pi,
        ..DifDesign::default()
    }
}

fn full(study: Study) -> Vec<Cell> {
    let mut out = Vec::new();
    match study {
        Study::DifNull => {
            for n in [1500, 3000, 6000] {
                for k in [4, 8, 16] {
                    for anchors in [20, 40, 60] {
                        out.push(dif(DifDesign {
                            anchors,
                            ..biased(n, k, 0, 0.0, 0.5)
                        }));
                    }
                }
            }
        }
        Study::DifPower => {
            for n in [1500, 3000, 6000] {
                for delta in [0.3, 0.5, 0.7, 0.9] {
                    for count in [1, 2, 3] {
                        for pi in [0.5, 0.3, 0.1] {
                            out.push(dif(biased(n, 8, count, delta, pi)));
                        }
                    }
                }
            }
            for k in [4, 16] {
                for delta in [0.5, 0.9] {
                    for count in [1, 2, 3] {
                        for pi in [0.5, 0.3] {
                            out.push(dif(biased(3000, k, count, delta, pi)));
                        }
                    }
                }
            }
        }
        Study::DifMisspec => {
            for impact in [0.0, 0.5, 1.0] {
                for guess in [0.0, 0.2] {
                    for count in [0, 2] {
                        for pi in [0.5, 0.2] {
                            if impact == 0.0 && count == 0 && pi != 0.5 {
                                continue;
                            }
                            let delta = if count > 0 { 0.9 } else { 0.0 };
                            out.push(dif(DifDesign {
                                impact,
                                guess,
                                ..biased(3000, 8, count, delta, pi)
                            }));
                        }
                    }
                }
            }
        }
        Study::DifNonuniform => {
            for n in [3000, 6000] {
                for alpha in [0.4, 0.8] {
                    for delta in [0.0, 0.5] {
                        out.push(dif(DifDesign {
                            alpha,
                            ..biased(n, 8, 2, delta, 0.5)
                        }));
                    }
                }
            }
        }
        Study::DifTwoAxes => {
            for n in [3000, 6000] {
                for delta in [0.5, 0.9] {
                    out.push(dif(DifDesign {
                        layout: Layout::TwoAxes(2),
                        ..biased(n, 8, 0, delta, 0.5)
                    }));
                }
            }
        }
        Study::DifPoisoning => {
            for fraction in [0.0, 0.01, 0.02, 0.05, 0.1] {
                for targets in [1, 2] {
                    out.push(dif(DifDesign {
                        attack: Attack::Inject { fraction, targets },
                        ..DifDesign::default()
                    }));
                }
            }
            for fraction in [0.0, 0.01, 0.02, 0.05, 0.1] {
                out.push(dif(DifDesign {
                    attack: Attack::Mask { fraction },
                    ..biased(3000, 8, 2, 0.9, 0.5)
                }));
            }
        }
        Study::DifPoolScale => {
            for k in [32, 100] {
                out.push(dif(biased(3000, k, 0, 0.0, 0.5)));
                out.push(dif(biased(3000, k, k / 10, 0.9, 0.5)));
            }
        }
        Study::DtfError => {
            for n in [3000, 6000] {
                for delta in [0.5, 0.9] {
                    for pi in [0.5, 0.3] {
                        out.push(dif(DifDesign {
                            layout: Layout::Mirror,
                            ..biased(n, 8, 0, delta, pi)
                        }));
                    }
                }
            }
        }
        Study::BridgingSweep => {
            for n in [100, 200, 800] {
                for share in [0.5, 0.6, 0.8] {
                    for per_reviewer in [5, 9] {
                        for noise in [0.07, 0.15] {
                            out.push(Cell::Sweep(SweepDesign {
                                n,
                                share,
                                per_reviewer,
                                noise,
                            }));
                        }
                    }
                }
            }
        }
        Study::BridgingCapture => {
            for item in [7, 8] {
                for own in [0, 40] {
                    out.push(Cell::Capture(CaptureDesign { item, own, step: 5 }));
                }
            }
        }
    }
    out
}

fn smoke(study: Study) -> Vec<Cell> {
    let tiny = |layout, delta| DifDesign {
        n: 400,
        anchors: 10,
        k: 4,
        layout,
        delta,
        ..DifDesign::default()
    };
    let one = |d: DifDesign| vec![dif(d)];
    match study {
        Study::DifNull => one(tiny(Layout::Campaign(0), 0.0)),
        Study::DifPower => one(tiny(Layout::Campaign(2), 0.9)),
        Study::DifMisspec => one(DifDesign {
            impact: 0.5,
            guess: 0.2,
            pi: 0.2,
            ..tiny(Layout::Campaign(2), 0.9)
        }),
        Study::DifNonuniform => one(DifDesign {
            alpha: 0.8,
            ..tiny(Layout::Campaign(2), 0.0)
        }),
        Study::DifTwoAxes => one(tiny(Layout::TwoAxes(1), 0.9)),
        Study::DifPoisoning => vec![
            dif(DifDesign {
                attack: Attack::Inject {
                    fraction: 0.1,
                    targets: 1,
                },
                ..tiny(Layout::Campaign(0), 0.0)
            }),
            dif(DifDesign {
                attack: Attack::Mask { fraction: 0.1 },
                ..tiny(Layout::Campaign(2), 0.9)
            }),
        ],
        Study::DifPoolScale => one(DifDesign {
            k: 12,
            ..tiny(Layout::Campaign(0), 0.0)
        }),
        Study::DtfError => one(DifDesign {
            k: 8,
            ..tiny(Layout::Mirror, 0.9)
        }),
        Study::BridgingSweep => vec![Cell::Sweep(SweepDesign {
            n: 40,
            share: 0.6,
            per_reviewer: 5,
            noise: 0.07,
        })],
        Study::BridgingCapture => vec![Cell::Capture(CaptureDesign {
            item: 7,
            own: 40,
            step: 40,
        })],
    }
}

pub fn cells(study: Study, grid: Grid) -> Vec<Cell> {
    match grid {
        Grid::Full => full(study),
        Grid::Smoke => smoke(study),
    }
}

pub fn replicates(study: Study, grid: Grid) -> u32 {
    match (grid, study) {
        (Grid::Full, Study::DifPoolScale) => 3,
        (Grid::Full, _) => 200,
        (Grid::Smoke, _) => 2,
    }
}

/// One run: a replicate of a cell of a study.
#[derive(Clone, Debug, PartialEq)]
pub struct Task {
    pub study: Study,
    pub cell: Cell,
    pub replicate: u32,
}

impl Task {
    pub fn seed(&self) -> u64 {
        seed(self.study, &self.cell.key(), self.replicate)
    }
}

fn first_eight(digest: &[u8]) -> u64 {
    let mut first = [0u8; 8];
    first.copy_from_slice(&digest[..8]);
    u64::from_le_bytes(first)
}

/// The run's seed: SHA-256 over the study, the cell key and the replicate, length-prefixed.
pub fn seed(study: Study, key: &str, replicate: u32) -> u64 {
    let mut h = Sha256::new();
    h.update(b"isegoria/characterization/v1");
    for part in [study.name().as_bytes(), key.as_bytes()] {
        h.update((part.len() as u64).to_le_bytes());
        h.update(part);
    }
    h.update(replicate.to_le_bytes());
    first_eight(&h.finalize())
}

/// The seed of the engine's own random starts, drawn apart from the population's stream.
pub fn engine_seed(seed: u64) -> u64 {
    let mut h = Sha256::new();
    h.update(b"isegoria/characterization/engine/v1");
    h.update(seed.to_le_bytes());
    first_eight(&h.finalize())
}

/// The runs of `studies`, replicate by replicate so that a partial run covers every cell.
/// `replicates` caps each study's own count; `filter` keeps the cells whose key contains it.
pub fn tasks(
    studies: &[Study],
    grid: Grid,
    replicates_cap: Option<u32>,
    filter: Option<&str>,
) -> Vec<Task> {
    let mut distinct: Vec<Study> = Vec::new();
    for &s in studies {
        if !distinct.contains(&s) {
            distinct.push(s);
        }
    }
    let plan: Vec<(Study, Vec<Cell>, u32)> = distinct
        .iter()
        .map(|&s| {
            let kept = cells(s, grid)
                .into_iter()
                .filter(|c| filter.is_none_or(|f| c.key().contains(f)))
                .collect();
            let own = replicates(s, grid);
            (s, kept, replicates_cap.map_or(own, |cap| cap.min(own)))
        })
        .collect();
    let most = plan.iter().map(|p| p.2).max().unwrap_or(0);
    let mut out = Vec::new();
    for replicate in 0..most {
        for (study, kept, reps) in &plan {
            if replicate < *reps {
                out.extend(kept.iter().map(|&cell| Task {
                    study: *study,
                    cell,
                    replicate,
                }));
            }
        }
    }
    out
}
