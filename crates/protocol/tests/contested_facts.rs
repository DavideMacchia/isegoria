//! Contested facts (`docs/01` D38, `docs/02` §B.5/§B.7, `docs/05` [7b], `docs/08` AT-PRO-08):
//! a DIF item whose source passes the check is kept apart, and a test draws contested facts
//! only in selections whose DTF bound is within the tolerance.

use network::cid::{cid, Cid};
use network::consortium::Checkpoint;
use protocol::blueprint::{assemble_test, Blueprint};
use protocol::contested::{ContestedPool, NoBalancedDraw, RecordError};
use protocol::exploration::{outcome_of, Observation, Scored, EXPLORATION_RATE};
use protocol::exposure::{should_retire, ItemHealth, RetirementReason, EXPOSURE_LIMIT};
use protocol::gate::GateOutcome;
use protocol::lifecycle::{step, Event, RejectReason, State};
use protocol::randomness::Beacon;
use scoring::dtf::{ClassCurves, DTF_MAX};
use std::collections::HashSet;

/// A fit of classes `pi`/`eta` whose items are `(a, b, lean)`: every class sits at
/// `b + lean·c_g`, with `c = [−1, +1, 0, …]` — a positive lean makes class 0 find it easier.
fn fit(pi: &[f64], eta: &[f64], items: &[(f64, f64, f64)]) -> ClassCurves {
    let shift = |g: usize| [-1.0, 1.0, 0.0, 0.0][g];
    let a = (0..pi.len())
        .map(|_| items.iter().map(|it| it.0).collect())
        .collect::<Vec<Vec<f64>>>();
    let b = (0..pi.len())
        .map(|g| items.iter().map(|it| it.1 + it.2 * shift(g)).collect())
        .collect::<Vec<Vec<f64>>>();
    ClassCurves::new(pi, eta, &a, &b).expect("well-formed classes")
}

fn fact(name: &str) -> Cid {
    cid(name.as_bytes())
}

/// Contested facts leaning both ways in three fits, and a lone leaner in a fourth.
fn pool() -> (ContestedPool, Vec<Cid>) {
    let mut pool = ContestedPool::new();
    let a = [
        (1.25, 0.0, 0.9),
        (1.25, 0.2, -0.9),
        (1.1, -0.6, 0.8),
        (1.3, -0.5, -0.8),
        (1.4, 1.1, 0.9),
    ];
    let b = [(1.2, 0.5, 0.9), (1.2, 0.6, -0.9), (1.0, -1.2, 0.7)];
    let three = [(1.3, 0.3, 0.8), (1.3, 0.4, -0.8), (1.2, -0.2, 0.6)];
    let lone = [(1.25, 0.0, 0.9)];
    let mut all = Vec::new();
    for (tag, curves, n) in [
        ("A", fit(&[0.5, 0.5], &[0.0, 0.0], &a), a.len()),
        ("B", fit(&[0.6, 0.4], &[0.0, 0.3], &b), b.len()),
        ("C", fit(&[0.4, 0.35, 0.25], &[0.0, 0.2, -0.2], &three), 3),
        ("L", fit(&[0.5, 0.5], &[0.0, 0.0], &lone), 1),
    ] {
        let members: Vec<(Cid, usize)> = (0..n).map(|j| (fact(&format!("{tag}{j}")), j)).collect();
        all.extend(members.iter().map(|m| m.0));
        pool.record(curves, &members).unwrap();
    }
    (pool, all)
}

/// Every subset of `items` of size `n`, by index.
fn subsets(items: usize, n: usize) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    let mut pick: Vec<usize> = (0..n).collect();
    if n > items {
        return out;
    }
    loop {
        out.push(pick.clone());
        let Some(i) = (0..n).rev().find(|&i| pick[i] < items - n + i) else {
            return out;
        };
        pick[i] += 1;
        for k in i + 1..n {
            pick[k] = pick[k - 1] + 1;
        }
    }
}

/// AT-PRO-08: a DIF item without a verified source is rejected as before; with one it is contested.
#[test]
fn at_pro_08_a_dif_item_is_contested_only_with_a_verified_source() {
    let pilot2 = State::Pilot2 { appealed: true };
    let batch = |passed, source_verified| Event::Pilot2Batch {
        batch_size: 8,
        passed,
        source_verified,
    };
    assert_eq!(
        step(pilot2.clone(), batch(false, false)),
        Ok(State::Rejected(RejectReason::Dif))
    );
    assert_eq!(
        step(pilot2.clone(), batch(false, true)),
        Ok(State::Contested)
    );
    assert_eq!(step(pilot2, batch(true, true)), Ok(State::ActivePool));
    let revalidate = |emerging_dif, source_verified| Event::Revalidate {
        emerging_dif,
        source_verified,
    };
    assert_eq!(
        step(State::ActivePool, revalidate(true, false)),
        Ok(State::Retired(RetirementReason::EmergingDif))
    );
    assert_eq!(
        step(State::ActivePool, revalidate(true, true)),
        Ok(State::Contested)
    );
}

/// A contested fact is administered, re-measured, returns to the pool without DIF, and retires.
#[test]
fn a_contested_fact_is_re_measured_and_retires_like_a_pool_item() {
    let revalidate = |emerging_dif, source_verified| Event::Revalidate {
        emerging_dif,
        source_verified,
    };
    assert_eq!(
        step(State::Contested, Event::Administer),
        Ok(State::Contested)
    );
    assert_eq!(
        step(State::Contested, revalidate(true, true)),
        Ok(State::Contested)
    );
    assert_eq!(
        step(State::Contested, revalidate(false, true)),
        Ok(State::ActivePool)
    );
    assert_eq!(
        step(State::Contested, revalidate(true, false)),
        Ok(State::Retired(RetirementReason::EmergingDif))
    );
    assert_eq!(
        step(State::Contested, Event::ExposureLimit),
        Ok(State::Retired(RetirementReason::Exposure))
    );
    assert!(step(
        State::Contested,
        Event::Score {
            outcome: GateOutcome::Pass
        }
    )
    .is_err());
}

/// DIF on a sourced fact is no retirement, from either pool; exposure still retires it (D38).
#[test]
fn dif_on_a_sourced_fact_is_no_retirement() {
    let sourced = ItemHealth {
        emerging_dif: true,
        source_verified: true,
        ..Default::default()
    };
    assert_eq!(should_retire(0, EXPOSURE_LIMIT, sourced), None);
    assert_eq!(
        should_retire(EXPOSURE_LIMIT, EXPOSURE_LIMIT, sourced),
        Some(RetirementReason::Exposure)
    );
    let unsourced = ItemHealth {
        source_verified: false,
        ..sourced
    };
    assert_eq!(
        should_retire(0, EXPOSURE_LIMIT, unsourced),
        Some(RetirementReason::EmergingDif)
    );
}

/// A contested fact is a Level B pass: observed at 1 when live, measured as passed when explored.
#[test]
fn a_contested_fact_scores_as_a_pass() {
    assert_eq!(
        outcome_of(&State::Contested, EXPLORATION_RATE),
        Scored::Observed(Observation {
            outcome: 1.0,
            inclusion: 1.0
        })
    );
    let explored = State::Explored {
        reason: RejectReason::Polarized,
        screened: true,
    };
    assert_eq!(
        step(
            explored,
            Event::Pilot2Batch {
                batch_size: 8,
                passed: false,
                source_verified: true
            }
        ),
        Ok(State::Measured {
            reason: RejectReason::Polarized,
            passed: true
        })
    );
}

/// AT-PRO-08: every test drawn from a pool leaning both ways has a DTF bound within tolerance.
#[test]
fn at_pro_08_every_assembled_test_is_within_the_dtf_tolerance() {
    let (pool, all) = pool();
    let blueprint = Blueprint::new(vec![("contested", 1.0)]);
    let tagged: Vec<(Cid, &str)> = all.iter().map(|&c| (c, "contested")).collect();
    for n in 1..=6 {
        let (mut drawn, mut unbalanced) = (0, 0);
        for seed in 0..200 {
            if let Ok(test) = pool.draw(n, DTF_MAX, seed) {
                drawn += 1;
                let distinct: HashSet<Cid> = test.iter().copied().collect();
                assert_eq!(distinct.len(), n, "n = {n}, seed {seed}: {test:?}");
                let bound = pool.dtf(&test).expect("drawn from the pool");
                assert!(bound <= DTF_MAX, "n = {n}, seed {seed}: bound {bound}");
            }
            let naive = assemble_test(&tagged, n, &blueprint, seed).unwrap();
            unbalanced += usize::from(pool.dtf(&naive).unwrap() > DTF_MAX);
        }
        println!("n = {n}: {drawn} of 200 draws, {unbalanced} naive tests over the tolerance");
        assert!(
            drawn == 0 || drawn == 200,
            "n = {n}: the draw depends on the seed"
        );
        assert!(
            unbalanced > 100,
            "n = {n}: the naive draw is balanced by luck"
        );
    }
    assert!(pool.draw(2, DTF_MAX, 0).is_ok() && pool.draw(4, DTF_MAX, 0).is_ok());
}

/// The draw is exact: it succeeds for exactly the sizes a brute-force search can balance.
#[test]
fn the_draw_finds_a_balanced_selection_whenever_one_exists() {
    let (pool, all) = pool();
    let brute: Vec<usize> = (0..=7)
        .filter(|&n| {
            subsets(all.len(), n).iter().any(|s| {
                let pick: Vec<Cid> = s.iter().map(|&i| all[i]).collect();
                pool.dtf(&pick).unwrap() <= DTF_MAX
            })
        })
        .collect();
    println!("balanced sizes: {brute:?}");
    for n in 0..=7 {
        match pool.draw(n, DTF_MAX, 11) {
            Ok(test) => assert!(brute.contains(&n) && test.len() == n),
            Err(NoBalancedDraw {
                requested,
                feasible,
            }) => {
                assert!(!brute.contains(&n), "n = {n} is balanced by brute force");
                assert_eq!(requested, n);
                let expect: Vec<usize> = brute.iter().copied().filter(|&s| s <= n).collect();
                assert_eq!(feasible, expect);
            }
        }
    }
    assert!(brute.contains(&0) && brute.contains(&2) && !brute.contains(&1));
}

/// Facts from different fits never cancel: two mirror leaners of two fits add up (`docs/02` §B.7).
#[test]
fn leaners_from_different_fits_do_not_cancel() {
    let mut pool = ContestedPool::new();
    let (x, y) = (fact("x"), fact("y"));
    let one = fit(&[0.5, 0.5], &[0.0, 0.0], &[(1.25, 0.0, 0.9)]);
    let mirror = fit(&[0.5, 0.5], &[0.0, 0.0], &[(1.25, 0.0, -0.9)]);
    let single = one.dtf(&[0]).unwrap();
    pool.record(one, &[(x, 0)]).unwrap();
    pool.record(mirror, &[(y, 0)]).unwrap();
    let joint = fit(
        &[0.5, 0.5],
        &[0.0, 0.0],
        &[(1.25, 0.0, 0.9), (1.25, 0.0, -0.9)],
    );
    assert_eq!(
        joint.dtf(&[0, 1]),
        Some(0.0),
        "mirror items cancel in one fit"
    );
    let bound = pool.dtf(&[x, y]).unwrap();
    assert!(
        (bound - 2.0 * single).abs() < 1e-9,
        "{bound} vs 2 × {single}"
    );
    assert_eq!(
        pool.draw(2, DTF_MAX, 3),
        Err(NoBalancedDraw {
            requested: 2,
            feasible: vec![0]
        })
    );
    pool.record(joint, &[(x, 0), (y, 1)]).unwrap();
    assert_eq!(
        pool.len(),
        2,
        "re-measured together, the two facts moved to one fit"
    );
    assert_eq!(pool.dtf(&[x, y]), Some(0.0));
    let both: HashSet<Cid> = pool.draw(2, DTF_MAX, 3).unwrap().into_iter().collect();
    assert_eq!(both, HashSet::from([x, y]));
}

/// The draw reproduces from the beacon, and over its indices draws every balanced selection.
#[test]
fn the_draw_reproduces_from_the_beacon_and_reaches_every_balanced_test() {
    let (pool, all) = pool();
    let beacon = Beacon::from_checkpoint(&Checkpoint::new([1; 32], [2; 32], 7, [3; 32]));
    let key = |mut test: Vec<Cid>| -> Vec<[u8; 32]> {
        test.sort_by_key(|c| c.0);
        test.iter().map(|c| c.0).collect()
    };
    for n in [2, 4, 6] {
        let first = pool.draw_from_beacon(n, DTF_MAX, &beacon, 0).unwrap();
        assert_eq!(pool.draw_from_beacon(n, DTF_MAX, &beacon, 0), Ok(first));
        let balanced: HashSet<Vec<[u8; 32]>> = subsets(all.len(), n)
            .iter()
            .map(|s| s.iter().map(|&i| all[i]).collect::<Vec<Cid>>())
            .filter(|pick| pool.dtf(pick).unwrap() <= DTF_MAX)
            .map(key)
            .collect();
        let drawn: HashSet<Vec<[u8; 32]>> = (0..600)
            .map(|t| key(pool.draw_from_beacon(n, DTF_MAX, &beacon, t).unwrap()))
            .collect();
        println!("n = {n}: {} balanced tests, all drawn", balanced.len());
        assert_eq!(drawn, balanced, "n = {n}");
        assert!(drawn.iter().flatten().all(|c| *c != fact("L0").0));
    }
}

/// The pool refuses a member past the fit, a repeated member, and a selection it does not hold.
#[test]
fn the_pool_refuses_malformed_records_and_selections() {
    let mut pool = ContestedPool::new();
    let curves = fit(
        &[0.5, 0.5],
        &[0.0, 0.0],
        &[(1.0, 0.0, 0.9), (1.0, 0.1, -0.9)],
    );
    assert_eq!(
        pool.record(curves.clone(), &[(fact("p"), 2)]),
        Err(RecordError::IndexOutOfRange { index: 2, items: 2 })
    );
    assert_eq!(
        pool.record(curves.clone(), &[(fact("p"), 0), (fact("p"), 1)]),
        Err(RecordError::DuplicateMember)
    );
    assert_eq!(
        pool.record(curves.clone(), &[(fact("p"), 0), (fact("q"), 0)]),
        Err(RecordError::DuplicateMember)
    );
    assert!(pool.is_empty());
    pool.record(curves, &[(fact("p"), 0), (fact("q"), 1)])
        .unwrap();
    assert_eq!(pool.dtf(&[fact("p"), fact("p")]), None);
    assert_eq!(pool.dtf(&[fact("r")]), None);
    assert!(pool.remove(&fact("p")) && !pool.remove(&fact("p")));
    assert!(pool.contains(&fact("q")) && pool.len() == 1);
    assert_eq!(pool.draw(0, DTF_MAX, 0), Ok(Vec::new()));
}

/// The contested pool on batches fitted by the target model (paper scale, `calibration`).
#[cfg(feature = "calibration")]
mod fitted {
    use super::*;
    use identity::nym::Nym;
    use protocol::admission::NullifierSet;
    use protocol::revalidation::{latent_batch, target_flags, N_LATENT_MIN};
    use rand::{Rng, SeedableRng};
    use rand_chacha::ChaCha8Rng;

    /// The fitted bound's estimation error at N = 3,000 (`docs/02` §B.7).
    const ESTIMATION: f64 = 0.05;

    fn sigmoid(z: f64) -> f64 {
        1.0 / (1.0 + (-z).exp())
    }

    /// Trial items `(a, b, lean)` answered by two equal classes `z = ±1` at `b − lean·z`, with 60
    /// anchors (KR-20 above the floor). Returns (anchors, responses).
    fn batch(items: &[(f64, f64, f64)], seed: u64) -> (Vec<Vec<f64>>, Vec<Vec<f64>>) {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let normal = |rng: &mut ChaCha8Rng| {
            let u1: f64 = 1.0 - rng.gen::<f64>();
            (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * rng.gen::<f64>()).cos()
        };
        let aa: Vec<f64> = (0..60).map(|_| rng.gen_range(0.9..1.6)).collect();
        let ba: Vec<f64> = (0..60).map(|_| normal(&mut rng)).collect();
        let (mut anchors, mut x) = (Vec::new(), Vec::new());
        for _ in 0..N_LATENT_MIN {
            let (t, z) = (normal(&mut rng), if rng.gen::<bool>() { 1.0 } else { -1.0 });
            let mut bit = |p: f64| f64::from(rng.gen::<f64>() < p);
            let row: Vec<f64> = (0..60).map(|j| bit(sigmoid(aa[j] * (t - ba[j])))).collect();
            anchors.push(row);
            x.push(
                items
                    .iter()
                    .map(|&(a, b, lean)| bit(sigmoid(a * (t - b + lean * z))))
                    .collect(),
            );
        }
        (anchors, x)
    }

    /// The true DTF of `items` between the classes `z = ±1`, on a fine grid of θ ~ N(0, 1).
    fn true_dtf(items: &[(f64, f64, f64)]) -> f64 {
        let grid: Vec<f64> = (0..801).map(|q| -8.0 + 16.0 * q as f64 / 800.0).collect();
        let density: Vec<f64> = grid.iter().map(|t| (-0.5 * t * t).exp()).collect();
        let total: f64 = density.iter().sum();
        grid.iter()
            .zip(&density)
            .map(|(t, d)| {
                let gap: f64 = items
                    .iter()
                    .map(|(a, b, lean)| sigmoid(a * (t - b + lean)) - sigmoid(a * (t - b - lean)))
                    .sum();
                d / total * gap.abs()
            })
            .sum()
    }

    /// AT-PRO-08 on batches fitted through the production gate: every drawn test's bound is within
    /// the tolerance, its true DTF within the estimation error, and an unsourced DIF item rejected.
    #[test]
    fn at_pro_08_on_batches_fitted_through_the_gate() {
        let lean_both = [
            (1.25, 0.0, 0.9),
            (1.2, 0.8, 0.9),
            (1.3, 0.1, -0.9),
            (1.15, -0.7, -0.9),
            (1.1, 0.4, 0.0),
            (1.4, -0.3, 0.0),
            (1.0, 0.9, 0.0),
            (1.3, -1.0, 0.0),
        ];
        let lean_one = [
            (1.2, 0.2, 0.9),
            (1.3, -0.4, 0.9),
            (1.1, 0.6, 0.9),
            (1.25, 0.0, 0.0),
            (1.0, -0.8, 0.0),
            (1.35, 0.5, 0.0),
            (1.2, 1.1, 0.0),
            (1.1, -0.2, 0.0),
        ];
        let mut respondents = NullifierSet::new();
        for i in 0..N_LATENT_MIN {
            let mut id = [0u8; 32];
            id[..8].copy_from_slice(&(i as u64).to_le_bytes());
            respondents.spend(Nym(id)).unwrap();
        }
        let mut pool = ContestedPool::new();
        let mut truth = Vec::new();
        for (tag, items, seed, unsourced) in
            [("A", &lean_both, 81, Some(3)), ("B", &lean_one, 82, None)]
        {
            let (anchors, x) = batch(items, seed);
            let fit =
                latent_batch(&respondents, &anchors, &x, 0).expect("the gate admits the batch");
            let flags = target_flags(&fit);
            let expect: Vec<bool> = items.iter().map(|it| it.2 != 0.0).collect();
            assert_eq!(flags, expect, "{tag}: gaps {:?}", fit.dif);
            let mut members = Vec::new();
            for (j, item) in items.iter().enumerate() {
                let verdict = Event::Pilot2Batch {
                    batch_size: items.len(),
                    passed: !flags[j],
                    source_verified: unsourced != Some(j),
                };
                let terminal = step(State::Pilot2 { appealed: false }, verdict).unwrap();
                let want = match (flags[j], unsourced == Some(j)) {
                    (false, _) => State::ActivePool,
                    (true, true) => State::Rejected(RejectReason::Dif),
                    (true, false) => State::Contested,
                };
                assert_eq!(terminal, want, "{tag}{j}");
                if terminal == State::Contested {
                    members.push((fact(&format!("{tag}{j}")), j));
                    truth.push((fact(&format!("{tag}{j}")), *item));
                }
            }
            pool.record(ClassCurves::of(&fit).unwrap(), &members)
                .unwrap();
        }
        assert_eq!(pool.len(), 6);
        let (mut drawn, mut worst) = (HashSet::new(), (0.0_f64, 0.0_f64));
        for seed in 0..100 {
            let test = pool
                .draw(2, DTF_MAX, seed)
                .expect("A holds a balanced pair");
            let items: Vec<(f64, f64, f64)> = test
                .iter()
                .map(|c| truth.iter().find(|(t, _)| t == c).unwrap().1)
                .collect();
            let (bound, real) = (pool.dtf(&test).unwrap(), true_dtf(&items));
            assert!(bound <= DTF_MAX, "{test:?}: bound {bound}");
            assert!(
                real <= DTF_MAX + ESTIMATION,
                "{test:?}: bound {bound}, true {real}"
            );
            worst = if real > worst.1 { (bound, real) } else { worst };
            drawn.extend(test);
        }
        let lone: Vec<Cid> = (0..3).map(|j| fact(&format!("B{j}"))).collect();
        assert!(
            drawn.iter().all(|c| !lone.contains(c)),
            "B's facts have no counterweight"
        );
        let naive = true_dtf(&[lean_one[0], lean_one[1]]);
        println!(
            "drawn {} facts; the worst true DTF {:.3} (bound {:.3}); B0 with B1: {naive:.3}",
            drawn.len(),
            worst.1,
            worst.0
        );
        assert!(naive > 3.0 * DTF_MAX);
    }
}
