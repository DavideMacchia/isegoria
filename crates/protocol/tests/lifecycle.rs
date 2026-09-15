//! Protocol orchestration (`docs/05`): deposit, lottery, blind review, bridging
//! gate + appeal, two-stage pilot, honeypot — and one end-to-end walk of the flow.

use identity::credential::Credential;
use identity::nym::Role;
use network::log::TransparencyLog;
use protocol::deposit::{deposit, Draft, NoPrimarySource};
use protocol::gate::{bridging_gate, settle_appeal, GateOutcome};
use protocol::governance::{change_approved, stratified_sortition, Candidate};
use protocol::honeypot::{inject, reviewer_skill, HONEYPOT_RATE};
use protocol::lottery::admit;
use protocol::pilot::{stage1_screen, stage2_dif};
use protocol::review::{assign_reviewers, commit, reveal, Reviewer};

const TAU: f64 = 0.08;
const EPS: f64 = 0.008;

#[test]
fn deposit_requires_a_primary_source_and_records_on_the_log() {
    let mut log = TransparencyLog::new();
    let ok = Draft {
        item: b"How many deputies sit in the Camera?".to_vec(),
        primary_source: b"Costituzione art.56".to_vec(),
    };
    let id = deposit(&mut log, &ok).unwrap();
    assert_eq!(log.len(), 1);
    assert!(log.verify());
    assert_eq!(id, ok.content_id());

    let no_source = Draft {
        item: b"claim".to_vec(),
        primary_source: vec![],
    };
    assert_eq!(deposit(&mut log, &no_source), Err(NoPrimarySource));
    assert_eq!(log.len(), 1);
}

#[test]
fn lottery_is_bounded_deterministic_and_epoch_varying() {
    let deposited: Vec<u32> = (0..1000).collect();
    let a = admit(&deposited, 300, 42, 1);
    let b = admit(&deposited, 300, 42, 1);
    assert_eq!(a.len(), 300);
    assert_eq!(a, b, "same epoch/seed → same admission");

    let c = admit(&deposited, 300, 42, 2);
    assert_ne!(a, c, "different epoch → different admission");

    // Capacity above supply admits everyone.
    assert_eq!(admit(&deposited, 5000, 42, 1).len(), 1000);
}

fn reviewers(n: usize) -> Vec<Reviewer> {
    (0..n)
        .map(|i| {
            let cred = Credential::from_secret([i as u8; 32]);
            Reviewer {
                nym: cred.nym(Role::Judge),
                f_u: -1.0 + 2.0 * (i as f64) / (n as f64 - 1.0),
            }
        })
        .collect()
}

#[test]
fn reviewer_assignment_is_stratified_and_deterministic() {
    let pool = reviewers(100);
    let panel = assign_reviewers(&pool, 9, 12345);
    assert_eq!(panel.len(), 9);

    // Deterministic per item seed.
    assert_eq!(
        assign_reviewers(&pool, 9, 12345)
            .iter()
            .map(|r| r.nym)
            .collect::<Vec<_>>(),
        panel.iter().map(|r| r.nym).collect::<Vec<_>>()
    );

    // Stratified: the panel spans the axis (both extremes represented).
    let min = panel.iter().map(|r| r.f_u).fold(f64::MAX, f64::min);
    let max = panel.iter().map(|r| r.f_u).fold(f64::MIN, f64::max);
    assert!(
        min < -0.5 && max > 0.5,
        "panel not spread: [{min:.2},{max:.2}]"
    );
}

#[test]
fn commit_reveal_binds_the_judgment() {
    let nonce = [3u8; 32];
    let c = commit(0.72, &nonce);
    assert!(reveal(c, 0.72, &nonce));
    assert!(
        !reveal(c, 0.71, &nonce),
        "changed probability must not verify"
    );
    assert!(
        !reveal(c, 0.72, &[9u8; 32]),
        "changed nonce must not verify"
    );
}

#[test]
fn bridging_gate_covers_pass_band_reject_and_appeal() {
    // clearly above the band → pass
    assert_eq!(bridging_gate(0.20, 0.0, TAU, EPS, 0.5), GateOutcome::Pass);
    // inside the band → supplementary review
    assert_eq!(
        bridging_gate(TAU, 0.0, TAU, EPS, 0.5),
        GateOutcome::SupplementaryReview
    );
    // below band, low polarization → plain reject (defect)
    assert_eq!(
        bridging_gate(-0.30, 0.1, TAU, EPS, 0.5),
        GateOutcome::Reject
    );
    // below band, high polarization → appeal eligible (true-but-divisive)
    assert_eq!(
        bridging_gate(-0.30, 1.6, TAU, EPS, 0.5),
        GateOutcome::AppealEligible
    );
}

#[test]
fn appeal_refunds_when_evidence_promotes_the_item() {
    // Promoted: stake refunded and the author gains for being right against opinion.
    assert!((settle_appeal(1.0, 0.3, true, 0.2) - 1.2).abs() < 1e-9);
    // Rejected: stake lost.
    assert!((settle_appeal(1.0, 0.3, false, 0.2) - 0.7).abs() < 1e-9);
}

#[test]
fn honeypot_injects_about_the_target_rate_and_catches_random_voters() {
    let queue: Vec<u32> = (0..100).collect();
    let golden: Vec<u32> = (1000..1100).collect();
    let mixed = inject(&queue, &golden, HONEYPOT_RATE, 7);
    assert_eq!(mixed.len(), queue.len() + 5, "5% of 100 = 5 golden");

    // Known outcomes: half the golden items are good, half bad.
    let outcomes: Vec<f64> = (0..10).map(|i| (i % 2) as f64).collect();
    let expert: Vec<f64> = outcomes
        .iter()
        .map(|&o| if o > 0.5 { 0.95 } else { 0.05 })
        .collect();
    let random: Vec<f64> = vec![0.5; 10];
    assert!(
        reviewer_skill(&expert, &outcomes) > 0.5,
        "expert should score well"
    );
    assert!(
        reviewer_skill(&random, &outcomes) <= 0.0,
        "random voting should not pay"
    );
}

// Synthetic respondents spread along the ability axis, with a small deterministic
// noise flip to avoid perfect separation.
fn synthetic() -> (Vec<f64>, Vec<f64>) {
    let n = 400;
    let theta: Vec<f64> = (0..n)
        .map(|i| -3.0 + 6.0 * i as f64 / (n as f64 - 1.0))
        .collect();
    let group: Vec<f64> = (0..n)
        .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
        .collect();
    (theta, group)
}

fn noisy(i: usize, base: bool) -> f64 {
    let flip = i * 13 % 11 == 0;
    ((base ^ flip) as i32) as f64
}

#[test]
fn pilot_stage1_drops_non_discriminating_items() {
    let (theta, _group) = synthetic();
    let discriminating: Vec<f64> = theta
        .iter()
        .enumerate()
        .map(|(i, &t)| noisy(i, t > 0.0))
        .collect();
    let random: Vec<f64> = (0..theta.len()).map(|i| (i % 2) as f64).collect();

    let keep = stage1_screen(&theta, &[discriminating, random]);
    assert!(keep[0], "a discriminating item should survive stage 1");
    assert!(!keep[1], "a non-discriminating item should be killed");
}

#[test]
fn pilot_stage2_drops_dif_items() {
    let (theta, group) = synthetic();
    let clean: Vec<f64> = theta
        .iter()
        .enumerate()
        .map(|(i, &t)| noisy(i, t > 0.0))
        .collect();
    let biased: Vec<f64> = theta
        .iter()
        .enumerate()
        .map(|(i, &t)| noisy(i, t + 1.5 * group[i] > 0.0))
        .collect();

    let keep = stage2_dif(&theta, &group, &[clean, biased]);
    assert!(keep[0], "a neutral item should pass DIF");
    assert!(!keep[1], "an item favoring one group should be rejected");
}

#[test]
fn sortition_is_stratified_deterministic_and_sized() {
    // 100 candidates spread along the axis; draw a committee of 9 across 3 strata.
    let candidates: Vec<Candidate<usize>> = (0..100)
        .map(|i| Candidate {
            id: i,
            f_u: -1.0 + 2.0 * i as f64 / 99.0,
        })
        .collect();

    let a = stratified_sortition(&candidates, 9, 3, 999);
    assert_eq!(a.len(), 9);

    // Deterministic per seed.
    assert_eq!(a, stratified_sortition(&candidates, 9, 3, 999));
    // A different seed generally draws a different committee.
    assert_ne!(a, stratified_sortition(&candidates, 9, 3, 1000));

    // Stratified: both axis extremes are represented (ids map monotonically to f_u).
    assert!(a.iter().any(|&id| id < 33), "no one from the low stratum");
    assert!(a.iter().any(|&id| id >= 66), "no one from the high stratum");
}

#[test]
fn sortition_handles_more_seats_than_candidates() {
    let candidates: Vec<Candidate<usize>> = (0..5)
        .map(|i| Candidate {
            id: i,
            f_u: i as f64,
        })
        .collect();
    let all = stratified_sortition(&candidates, 20, 4, 1);
    assert_eq!(all.len(), 5, "cannot draw more than exist");
}

#[test]
fn meta_level_change_needs_supermajority_and_delay() {
    // 2/3 + 30 days both required (docs/05).
    assert!(
        change_approved(70, 100, 30),
        "70% after 30 days should pass"
    );
    assert!(
        !change_approved(60, 100, 60),
        "below 2/3 must fail even after delay"
    );
    assert!(
        !change_approved(90, 100, 10),
        "before the delay must fail even at 90%"
    );
    assert!(
        !change_approved(1, 0, 60),
        "no eligible voters → not approved"
    );
}
