//! The populations the studies draw (`docs/13` §3): latent-DIF batches after the paper's
//! `dif_generate`, the Level A mirror design, and the reference fixture of `sim/`.

use crate::grid::{Attack, DifDesign, Layout, SweepDesign};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use scoring::bridging::Ratings;

/// A standard normal by Box–Muller, with the engine's `libm` (`docs/13` §2).
pub fn normal(rng: &mut ChaCha8Rng) -> f64 {
    let u1: f64 = 1.0 - rng.gen::<f64>();
    let u2: f64 = rng.gen::<f64>();
    (-2.0 * libm::log(u1)).sqrt() * libm::cos(2.0 * std::f64::consts::PI * u2)
}

pub fn sigmoid(z: f64) -> f64 {
    1.0 / (1.0 + libm::exp(-z))
}

fn bit(rng: &mut ChaCha8Rng, p: f64) -> f64 {
    f64::from(rng.gen::<f64>() < p)
}

/// Per trial item, its sign on the first and on the second axis.
pub fn signs(layout: Layout, k: usize) -> (Vec<f64>, Vec<f64>) {
    let mut first = vec![0.0; k];
    let mut second = vec![0.0; k];
    match layout {
        Layout::Campaign(n) => first.iter_mut().take(n).for_each(|s| *s = 1.0),
        Layout::Mirror => {
            for (j, s) in first.iter_mut().take(4).enumerate() {
                *s = if j % 2 == 0 { 1.0 } else { -1.0 };
            }
        }
        Layout::TwoAxes(n) => {
            first.iter_mut().take(n).for_each(|s| *s = 1.0);
            second.iter_mut().skip(n).take(n).for_each(|s| *s = 1.0);
        }
    }
    (first, second)
}

/// A drawn batch; `roles` has one character per trial item (`docs/13` §3.1).
#[derive(Clone, Debug)]
pub struct DifBatch {
    pub anchors: Vec<Vec<f64>>,
    pub x: Vec<Vec<f64>>,
    /// Each respondent's class on the first axis, `±1`.
    pub z: Vec<f64>,
    pub roles: String,
    pub a: Vec<f64>,
    pub b: Vec<f64>,
    pub signs: Vec<f64>,
}

pub fn dif_batch(d: &DifDesign, seed: u64) -> DifBatch {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let (first, second) = signs(d.layout, d.k);
    let a_anchor: Vec<f64> = (0..d.anchors).map(|_| rng.gen_range(0.9..1.6)).collect();
    let b_anchor: Vec<f64> = (0..d.anchors).map(|_| normal(&mut rng)).collect();
    let a: Vec<f64> = (0..d.k).map(|_| rng.gen_range(1.0..1.5)).collect();
    let b: Vec<f64> = (0..d.k).map(|_| 0.6 * normal(&mut rng)).collect();
    let (fraction, targets, masked) = match d.attack {
        Attack::None => (0.0, 0, false),
        Attack::Inject { fraction, targets } => (fraction, targets.min(d.k), false),
        Attack::Mask { fraction } => (fraction, 0, true),
    };
    let attackers = ((fraction * d.n as f64).round() as usize).min(d.n);
    let targeted = |j: usize| j >= d.k - targets;
    let respond = |p: f64| d.guess + (1.0 - d.guess) * p;
    let (mut anchors, mut x, mut z) = (
        Vec::with_capacity(d.n),
        Vec::with_capacity(d.n),
        Vec::with_capacity(d.n),
    );
    for i in 0..d.n {
        let z1 = if rng.gen::<f64>() < d.pi { 1.0 } else { -1.0 };
        let z2 = if rng.gen::<f64>() < 0.5 { 1.0 } else { -1.0 };
        let theta = normal(&mut rng) + if z1 > 0.0 { d.impact } else { 0.0 };
        let attacker = i >= d.n - attackers;
        let row: Vec<f64> = (0..d.anchors)
            .map(|j| {
                let p = sigmoid(a_anchor[j] * (theta - b_anchor[j]));
                bit(&mut rng, respond(p))
            })
            .collect();
        anchors.push(row);
        let row: Vec<f64> = (0..d.k)
            .map(|j| {
                let lean = if attacker && masked {
                    0.0
                } else {
                    first[j] * z1 + second[j] * z2
                };
                let slope = a[j] + d.alpha / 2.0 * lean;
                let drawn = bit(
                    &mut rng,
                    respond(sigmoid(slope * (theta - b[j] - d.delta * lean))),
                );
                if attacker && targeted(j) {
                    0.0
                } else {
                    drawn
                }
            })
            .collect();
        x.push(row);
        z.push(z1);
    }
    let roles = (0..d.k)
        .map(|j| match (first[j], second[j]) {
            (s, _) if s > 0.0 => '+',
            (s, _) if s < 0.0 => '-',
            (_, s) if s != 0.0 => '2',
            _ if targeted(j) => 't',
            _ => 'c',
        })
        .collect();
    DifBatch {
        anchors,
        x,
        z,
        roles,
        a,
        b,
        signs: first,
    }
}

/// A drawn mirror design: the ratings, and per item its quality `q` and lean.
#[derive(Clone, Debug)]
pub struct SweepData {
    pub ratings: Ratings,
    pub q: Vec<f64>,
    pub lean: Vec<f64>,
    pub true_f: Vec<f64>,
}

pub const CONSENSUS_ITEMS: usize = 10;
pub const PARTISAN_ITEMS: usize = 10;

pub fn sweep_data(d: &SweepDesign, seed: u64) -> SweepData {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let n_b = (d.n as f64 * d.share).round() as usize;
    let true_f: Vec<f64> = (0..d.n)
        .map(|u| if u < d.n - n_b { -1.0 } else { 1.0 } + 0.25 * normal(&mut rng))
        .collect();
    let severity: Vec<f64> = (0..d.n).map(|_| 0.06 * normal(&mut rng)).collect();
    let m = CONSENSUS_ITEMS + PARTISAN_ITEMS;
    let q: Vec<f64> = (0..m)
        .map(|j| {
            if j < CONSENSUS_ITEMS {
                rng.gen_range(0.70..0.95)
            } else {
                0.55
            }
        })
        .collect();
    let lean: Vec<f64> = (0..m)
        .map(|j| match j.checked_sub(CONSENSUS_ITEMS) {
            None => 0.0,
            Some(p) if p % 2 == 0 => 0.8,
            Some(_) => -0.8,
        })
        .collect();
    let per = d.per_reviewer.min(m);
    let mut r = vec![vec![0.0; m]; d.n];
    let mut mask = vec![vec![false; m]; d.n];
    for u in 0..d.n {
        let mut items: Vec<usize> = (0..m).collect();
        for slot in 0..per {
            let pick = slot + rng.gen_range(0..m - slot);
            items.swap(slot, pick);
            mask[u][items[slot]] = true;
        }
        for j in 0..m {
            let rating =
                q[j] + 0.45 * true_f[u] * lean[j] + severity[u] + d.noise * normal(&mut rng);
            r[u][j] = rating.clamp(0.0, 1.0);
        }
    }
    SweepData {
        ratings: Ratings::from_dense(&r, &mask),
        q,
        lean,
        true_f,
    }
}

/// The reference simulation's Level A dataset (`sim/export_fixtures.py`).
#[derive(Clone, Debug)]
pub struct Fixture {
    pub r: Vec<Vec<f64>>,
    pub mask: Vec<Vec<bool>>,
    pub true_f: Vec<f64>,
}

/// The fixture items' leans (`paper/scripts/common.py::sim_levelA_dataset`).
pub const FIXTURE_LEAN: [f64; 10] = [0.0, 0.0, 0.75, 0.05, 0.0, 0.0, 0.0, 0.80, -0.80, 0.30];

const R_CSV: &str = include_str!("../../scoring/tests/fixtures/R.csv");
const MASK_CSV: &str = include_str!("../../scoring/tests/fixtures/mask.csv");
const TRUE_F_CSV: &str = include_str!("../../scoring/tests/fixtures/true_f.csv");

fn matrix(text: &str) -> Vec<Vec<f64>> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            l.split(',')
                .map(|c| c.trim().parse().expect("a number in a committed fixture"))
                .collect()
        })
        .collect()
}

pub fn fixture() -> Fixture {
    Fixture {
        r: matrix(R_CSV),
        mask: matrix(MASK_CSV)
            .iter()
            .map(|row| row.iter().map(|&v| v != 0.0).collect())
            .collect(),
        true_f: matrix(TRUE_F_CSV).iter().map(|row| row[0]).collect(),
    }
}
