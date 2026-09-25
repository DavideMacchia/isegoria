//! The change detector on a reviewer's per-item scores: a one-sided CUSUM against the
//! reviewer's own mean sends a dropping reviewer back to probation (`docs/01` D34,
//! `docs/08` REPUTATION-004, T51).

use protocol::orchestrator::bridging_weights;
use protocol::probation::{Alarm, SkillTrack, Status, N_PROBATION};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use scoring::reputation::{difference_score, CusumParams};

fn normal(r: &mut ChaCha8Rng) -> f64 {
    let u1: f64 = 1.0 - r.gen::<f64>();
    let u2: f64 = r.gen();
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

/// One scored item (paper's `panel_bias`): the reviewer's forecast is flipped with
/// probability `flip`. Returns the reviewer's leave-one-out difference score.
fn scored_item(r: &mut ChaCha8Rng, flip: f64) -> f64 {
    // Beta(2, 2) approximated by the mean of two uniforms.
    let pi = (r.gen::<f64>() + r.gen::<f64>()) / 2.0;
    let o = f64::from(r.gen::<f64>() < pi);
    let crowd = (pi + 0.05 * normal(r)).clamp(0.02, 0.98);
    let mut p = (pi + 0.10 * normal(r)).clamp(0.02, 0.98);
    if r.gen::<f64>() < flip {
        p = 1.0 - p;
    }
    difference_score(p, crowd, o)
}

/// AT-REP-07: a stream of honest scores raises at most one alarm.
#[test]
fn at_rep_07_an_honest_stream_raises_at_most_one_alarm() {
    let mut r = ChaCha8Rng::seed_from_u64(7);
    let mut track = SkillTrack::new();
    let params = CusumParams::default();
    let mut alarms = 0;
    for _ in 0..10_000 {
        if track.record(scored_item(&mut r, 0.0), &params).is_some() {
            alarms += 1;
        }
    }
    println!(
        "honest stream: {alarms} alarms, skill {:.4} over {} items",
        track.skill(),
        track.scored()
    );
    assert!(alarms <= 1, "{alarms} alarms on an honest stream");
    assert_eq!(track.status(false), Status::Established);
}

/// AT-REP-07, the long con: a reviewer who flips 20% of forecasts after 300 honest items
/// is caught and sent back to probation, at weight 0 until 30 new honest outcomes.
#[test]
fn at_rep_07_a_reviewer_who_starts_flipping_forecasts_is_caught() {
    let params = CusumParams::default();
    let mut delays = Vec::new();
    for seed in 100..110 {
        let mut r = ChaCha8Rng::seed_from_u64(seed);
        let mut track = SkillTrack::new();
        for _ in 0..300 {
            assert!(track.record(scored_item(&mut r, 0.0), &params).is_none());
        }
        assert_eq!(track.status(false), Status::Established);
        // An honest reviewer scores near 0 against the crowd; the detector watches for
        // a drop, whatever its sign.
        let honest_skill = track.skill();
        assert!(honest_skill.abs() < 0.05, "honest skill {honest_skill}");
        let mut caught = None;
        for t in 1..=400 {
            if let Some(Alarm { count, scored }) = track.record(scored_item(&mut r, 0.2), &params) {
                assert_eq!(count, 1);
                assert_eq!(scored, 300 + t - 1);
                caught = Some(t);
                break;
            }
        }
        let t = caught.unwrap_or_else(|| panic!("seed {seed}: not caught within 400 items"));
        println!("seed {seed}: caught after {t} items");
        delays.push(t);
        assert_eq!(track.status(false), Status::Probation);
        assert_eq!(track.scored(), 0);
        assert_eq!(track.weight(false, 3.0), 0.0);
        assert_eq!(bridging_weights(&[track.standing(false)], 3.0), vec![0.0]);
        for _ in 0..N_PROBATION {
            track.record(scored_item(&mut r, 0.0), &params);
        }
        assert_eq!(track.status(false), Status::Established);
        assert!(track.weight(false, 3.0) > 0.0);
    }
    delays.sort_unstable();
    println!("delays: {delays:?} (paper median 36)");
    assert!(delays[delays.len() / 2] <= 60, "median delay {delays:?}");
    let within_100 = delays.iter().filter(|&&t| t <= 100).count();
    assert!(
        within_100 >= 8,
        "{within_100} of 10 within 100 items: {delays:?}"
    );
}

/// The detector does not run during probation, and a founder on probation keeps weight 1.
#[test]
fn no_alarm_during_probation() {
    let params = CusumParams::default();
    let mut track = SkillTrack::new();
    for _ in 0..N_PROBATION - 1 {
        assert!(track.record(-1.0, &params).is_none());
    }
    assert_eq!(track.status(false), Status::Probation);
    assert_eq!(track.weight(true, 3.0), 1.0);
    assert_eq!(track.weight(false, 3.0), 0.0);
    assert_eq!(track.alarms(), 0);
}
