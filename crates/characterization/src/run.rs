//! One run: draw the population from the run's seed, apply the production estimator and
//! gates, and return what the study records (`docs/13` §4).

use crate::generate::{dif_batch, fixture, sweep_data, DifBatch, FIXTURE_LEAN};
use crate::grid::{CaptureDesign, Cell, DifDesign, Kind, SweepDesign, Task};
use protocol::gate::{bridging_gate, GateOutcome, APPEAL_GAP, EPS, TAU};
use protocol::lifecycle::K_MIN;
use protocol::revalidation::{target_flags, N_LATENT_MIN};
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use scoring::bridging::{bridge_scores, fit, BridgingParams, Obs, Ratings};
use scoring::dtf::ClassCurves;
use scoring::irt::{kr20, KR20_MIN};
use scoring::latent::latent_dif;
use scoring::Convergence;
use std::collections::BTreeSet;

/// The bootstrap of the gate's robust score, as the epoch runs it (`docs/02` §A.4).
pub const BOOTSTRAPS: usize = 10;
pub const KEEP: f64 = 0.85;

/// The item sets of the mirror layout whose DTF the `dtf-error` study compares.
pub const DTF_SETS: [&[usize]; 8] = [
    &[0],
    &[1],
    &[0, 1],
    &[2, 3],
    &[0, 2],
    &[0, 1, 2, 3],
    &[4, 5, 6, 7],
    &[0, 1, 4, 5],
];

#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Dif(DifOutcome),
    Dtf(DtfOutcome),
    Sweep(SweepOutcome),
    Capture(CaptureOutcome),
}

/// A latent-DIF run: the gates' inputs, the selected fit, the verdict and each item's role.
#[derive(Clone, Debug, PartialEq)]
pub struct DifOutcome {
    pub kr20: f64,
    pub admitted: bool,
    pub classes: usize,
    pub non_uniform: bool,
    pub converged: bool,
    pub bic_gain: f64,
    pub pi: Vec<f64>,
    pub eta: Vec<f64>,
    pub dif: Vec<f64>,
    pub a_gap: Vec<f64>,
    pub flags: Vec<bool>,
    pub roles: String,
}

/// Per set of [`DTF_SETS`], the fitted DTF and the one of the true curves.
#[derive(Clone, Debug, PartialEq)]
pub struct DtfOutcome {
    pub classes: usize,
    pub converged: bool,
    pub flags: Vec<bool>,
    pub roles: String,
    pub fitted: Vec<f64>,
    pub truth: Vec<f64>,
}

/// Per item its quality, lean, full and robust side-balanced scores, side gap and gate outcome.
#[derive(Clone, Debug, PartialEq)]
pub struct SweepOutcome {
    pub converged: bool,
    pub axis_corr: f64,
    pub q: Vec<f64>,
    pub lean: Vec<f64>,
    pub full: Vec<f64>,
    pub robust: Vec<f64>,
    pub gap: Vec<f64>,
    pub gate: String,
}

/// Per count of opposing boosters, the item's full and robust scores and its plain mean.
#[derive(Clone, Debug, PartialEq)]
pub struct CaptureOutcome {
    pub opposing: Vec<usize>,
    pub full: Vec<f64>,
    pub robust: Vec<f64>,
    pub plain: Vec<f64>,
}

pub fn run(task: &Task) -> Outcome {
    let seed = task.seed();
    match (task.study.kind(), task.cell) {
        (Kind::Dtf, Cell::Dif(d)) => Outcome::Dtf(dtf(&d, seed)),
        (_, Cell::Dif(d)) => Outcome::Dif(dif(&d, seed)),
        (_, Cell::Sweep(d)) => Outcome::Sweep(sweep(&d, seed)),
        (_, Cell::Capture(d)) => Outcome::Capture(capture(&d, seed)),
    }
}

/// The production re-check on the drawn batch: the gates of `latent_batch` are recorded in
/// `admitted`, never applied, so the study also sees what they refuse (`docs/13` §4).
pub fn dif(d: &DifDesign, seed: u64) -> DifOutcome {
    let batch = dif_batch(d, seed);
    let reliability = kr20(&batch.anchors);
    let admitted = d.k >= K_MIN && d.n >= N_LATENT_MIN && reliability >= KR20_MIN;
    let fit = latent_dif(&batch.anchors, &batch.x, seed);
    DifOutcome {
        kr20: reliability,
        admitted,
        classes: fit.classes,
        non_uniform: fit.non_uniform,
        converged: fit.status == Convergence::Converged,
        bic_gain: fit.bic_gain,
        flags: target_flags(&fit),
        pi: fit.pi,
        eta: fit.eta,
        dif: fit.dif,
        a_gap: fit.a_gap,
        roles: batch.roles,
    }
}

/// The design's two classes on the fit's grid: class 0 is `z = −1`, class 1 is `z = +1`.
fn true_curves(d: &DifDesign, batch: &DifBatch) -> Option<ClassCurves> {
    let classes = [-1.0, 1.0];
    let per_class = |base: &[f64], step: f64| -> Vec<Vec<f64>> {
        classes
            .iter()
            .map(|z| {
                (0..d.k)
                    .map(|j| base[j] + step * batch.signs[j] * z)
                    .collect()
            })
            .collect()
    };
    ClassCurves::new(
        &[1.0 - d.pi, d.pi],
        &[0.0, d.impact],
        &per_class(&batch.a, d.alpha / 2.0),
        &per_class(&batch.b, d.delta),
    )
    .ok()
}

pub fn dtf(d: &DifDesign, seed: u64) -> DtfOutcome {
    let batch = dif_batch(d, seed);
    let fit = latent_dif(&batch.anchors, &batch.x, seed);
    let fitted = ClassCurves::of(&fit).ok();
    let truth = true_curves(d, &batch);
    let over = |curves: &Option<ClassCurves>| -> Vec<f64> {
        DTF_SETS
            .iter()
            .map(|set| curves.as_ref().and_then(|c| c.dtf(set)).unwrap_or(f64::NAN))
            .collect()
    };
    DtfOutcome {
        classes: fit.classes,
        converged: fit.status == Convergence::Converged,
        flags: target_flags(&fit),
        roles: batch.roles.clone(),
        fitted: over(&fitted),
        truth: over(&truth),
    }
}

fn correlation(x: &[f64], y: &[f64]) -> f64 {
    let n = x.len() as f64;
    let (mx, my) = (x.iter().sum::<f64>() / n, y.iter().sum::<f64>() / n);
    let (mut sxy, mut sxx, mut syy) = (0.0, 0.0, 0.0);
    for (a, b) in x.iter().zip(y) {
        sxy += (a - mx) * (b - my);
        sxx += (a - mx) * (a - mx);
        syy += (b - my) * (b - my);
    }
    sxy / (sxx * syy).sqrt()
}

fn gate_code(outcome: GateOutcome) -> char {
    match outcome {
        GateOutcome::Pass => 'P',
        GateOutcome::SupplementaryReview => 'S',
        GateOutcome::AppealEligible => 'A',
        GateOutcome::Reject => 'R',
    }
}

pub fn sweep(d: &SweepDesign, seed: u64) -> SweepOutcome {
    let data = sweep_data(d, seed);
    let params = BridgingParams {
        seed,
        ..BridgingParams::default()
    };
    let full_fit = fit(&data.ratings, &params).expect("generated ratings are well formed");
    let scores = bridge_scores(&data.ratings, &params, BOOTSTRAPS, KEEP)
        .expect("generated ratings are well formed");
    let gate = scores
        .robust
        .iter()
        .zip(&scores.full.gap)
        .map(|(&s, &g)| gate_code(bridging_gate(s, g, TAU, EPS, APPEAL_GAP)))
        .collect();
    SweepOutcome {
        converged: full_fit.status == Convergence::Converged,
        axis_corr: correlation(&full_fit.f_u, &data.true_f).abs(),
        q: data.q,
        lean: data.lean,
        full: scores.full.score,
        robust: scores.robust,
        gap: scores.full.gap,
        gate,
    }
}

/// Boosters rate the item 1.0: `own` drawn from the camp it favours, then the opposing camp
/// in a drawn order, `step` at a time; the score is read at every step (`docs/13` §3.3).
pub fn capture(d: &CaptureDesign, seed: u64) -> CaptureOutcome {
    let fx = fixture();
    let base = Ratings::from_dense(&fx.r, &fx.mask);
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let favours = |u: &usize| fx.true_f[*u] * FIXTURE_LEAN[d.item] > 0.0;
    let (mut own, mut opposing): (Vec<usize>, Vec<usize>) = (0..base.n).partition(favours);
    own.shuffle(&mut rng);
    opposing.shuffle(&mut rng);
    own.truncate(d.own);
    let params = BridgingParams {
        seed,
        ..BridgingParams::default()
    };
    let mut out = CaptureOutcome {
        opposing: Vec::new(),
        full: Vec::new(),
        robust: Vec::new(),
        plain: Vec::new(),
    };
    for count in (0..=opposing.len()).step_by(d.step.max(1)) {
        let boosters: BTreeSet<usize> = own.iter().chain(&opposing[..count]).copied().collect();
        let mut obs: Vec<Obs> = base
            .obs
            .iter()
            .copied()
            .filter(|o| !(o.j == d.item && boosters.contains(&o.u)))
            .collect();
        obs.extend(boosters.iter().map(|&u| Obs {
            u,
            j: d.item,
            r: 1.0,
        }));
        let item: Vec<f64> = obs.iter().filter(|o| o.j == d.item).map(|o| o.r).collect();
        let data = Ratings {
            obs,
            ..base.clone()
        };
        let scores = bridge_scores(&data, &params, BOOTSTRAPS, KEEP)
            .expect("the fixture's ratings are well formed");
        out.opposing.push(count);
        out.full.push(scores.full.score[d.item]);
        out.robust.push(scores.robust[d.item]);
        out.plain.push(item.iter().sum::<f64>() / item.len() as f64);
    }
    out
}
