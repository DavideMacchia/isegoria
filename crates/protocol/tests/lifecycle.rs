//! Protocol orchestration (`docs/05`): deposit, lottery, blind review, bridging
//! gate + appeal, two-stage pilot, honeypot — and one end-to-end walk of the flow.

use identity::credential::Credential;
use identity::nym::Role;
use network::log::TransparencyLog;
use protocol::blueprint::{assemble_test, Blueprint};
use protocol::deposit::{deposit, Draft, NoPrimarySource};
use protocol::exposure::{
    least_exposed_variant, should_retire, ExposureLedger, ItemHealth, RetirementReason, Template,
};
use protocol::gate::{bridging_gate, settle_appeal, GateOutcome};
use protocol::governance::{change_approved, stratified_sortition, Candidate};
use protocol::honeypot::{inject, reviewer_skill, HONEYPOT_RATE};
use protocol::lottery::admit;
use protocol::pilot::{stage1_screen, stage2_dif};
use protocol::probation::{effective_review_weight, status, FounderSet, Status, N_PROBATION};
use protocol::revalidation::{items_to_retire, revalidate_pool};
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

/// PROTO-011 / G-18 / AT-PRO-04: the content id MUST commit to the field boundary.
/// Plain concatenation hashes `item ‖ primary_source`, so moving a byte across the
/// boundary — `("ab", "c")` vs `("a", "bc")` — leaves the same bytes and collides.
#[test]
fn content_id_is_unambiguous_across_the_field_boundary() {
    let a = Draft {
        item: b"ab".to_vec(),
        primary_source: b"c".to_vec(),
    };
    let b = Draft {
        item: b"a".to_vec(),
        primary_source: b"bc".to_vec(),
    };
    assert_ne!(
        a.content_id(),
        b.content_id(),
        "drafts differing only in where the item/source boundary falls must not collide"
    );
    // A genuinely equal draft still yields the same id (deposit remains deterministic).
    let a2 = Draft {
        item: b"ab".to_vec(),
        primary_source: b"c".to_vec(),
    };
    assert_eq!(a.content_id(), a2.content_id());
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
fn probation_gates_weight_until_a_track_record_exists() {
    // A new node is on probation: measured, but weight 0 (docs/03 P2).
    assert_eq!(status(false, 0), Status::Probation);
    assert_eq!(status(false, N_PROBATION - 1), Status::Probation);
    assert_eq!(effective_review_weight(false, 50, 0.9, 1.5), 0.0);

    // A founder seeds the bootstrap at uniform weight 1 (docs/05 cold start).
    assert_eq!(status(true, 0), Status::Founder);
    assert_eq!(effective_review_weight(true, 0, 0.9, 1.5), 1.0);

    // Past the threshold, anyone becomes E_u-weighted and capped.
    assert_eq!(status(false, N_PROBATION), Status::Established);
    assert_eq!(status(true, N_PROBATION), Status::Established);
    assert_eq!(effective_review_weight(false, N_PROBATION, 0.7, 1.5), 0.7);
    assert_eq!(effective_review_weight(true, N_PROBATION, 2.0, 1.5), 1.5); // capped
}

#[test]
fn founder_set_tracks_declared_members() {
    let alice = Credential::from_secret([1u8; 32]).nym(Role::Judge);
    let bob = Credential::from_secret([2u8; 32]).nym(Role::Judge);
    let carol = Credential::from_secret([3u8; 32]).nym(Role::Judge);
    let founders = FounderSet::new([alice, bob]);

    assert_eq!(founders.len(), 2);
    assert!(founders.contains(&alice));
    assert!(!founders.contains(&carol));
    // A node's founder status feeds the weight rule.
    assert_eq!(
        effective_review_weight(founders.contains(&carol), 0, 0.9, 1.5),
        0.0
    );
    assert_eq!(
        effective_review_weight(founders.contains(&alice), 0, 0.9, 1.5),
        1.0
    );
}

#[test]
fn blueprint_apportions_quotas_proportionally() {
    let bp = Blueprint::new(vec![("law", 2.0), ("health", 1.0), ("procedure", 1.0)]);
    let quotas = bp.quotas(8);
    assert_eq!(quotas, vec![("law", 4), ("health", 2), ("procedure", 2)]);
    // Seats always sum to the requested size, even with awkward remainders.
    let odd = Blueprint::new(vec![("a", 1.0), ("b", 1.0), ("c", 1.0)]);
    assert_eq!(odd.quotas(8).iter().map(|(_, n)| n).sum::<usize>(), 8);
}

#[test]
fn blueprint_assembles_a_balanced_test() {
    // Plenty of items in each domain; the test must match the quotas exactly.
    let mut available: Vec<(u32, &str)> = Vec::new();
    for i in 0..50 {
        available.push((i, "law"));
    }
    for i in 50..80 {
        available.push((i, "health"));
    }
    for i in 80..110 {
        available.push((i, "procedure"));
    }
    let bp = Blueprint::new(vec![("law", 2.0), ("health", 1.0), ("procedure", 1.0)]);

    let test = assemble_test(&available, 8, &bp, 42).expect("enough items");
    assert_eq!(test.len(), 8);
    let by = |dom: &str| {
        test.iter()
            .filter(|id| available.iter().find(|(i, _)| i == *id).unwrap().1 == dom)
            .count()
    };
    assert_eq!(by("law"), 4);
    assert_eq!(by("health"), 2);
    assert_eq!(by("procedure"), 2);

    // Deterministic per seed.
    assert_eq!(assemble_test(&available, 8, &bp, 42).unwrap(), test);
}

#[test]
fn blueprint_reports_shortfall_when_a_domain_is_thin() {
    let available = vec![(1u32, "law"), (2, "law"), (3, "health")];
    let bp = Blueprint::new(vec![("law", 1.0), ("health", 1.0)]);
    // size 4 → 2 law + 2 health, but only 1 health item exists.
    let err = assemble_test(&available, 4, &bp, 1).unwrap_err();
    assert!(err
        .iter()
        .any(|s| s.domain == "health" && s.needed == 2 && s.available == 1));
}

#[test]
fn blueprint_detects_pool_skew() {
    // A pool that is all "law" against an even blueprint is over/under-represented.
    let bp = Blueprint::new(vec![("law", 1.0), ("health", 1.0)]);
    let pool = vec!["law", "law", "law", "law"];
    let dev = bp.coverage_deviation(&pool);
    let law = dev.iter().find(|(d, _)| *d == "law").unwrap().1;
    let health = dev.iter().find(|(d, _)| *d == "health").unwrap().1;
    assert!(law > 0.4, "law over-represented: {law:.2}");
    assert!(health < -0.4, "health under-represented: {health:.2}");
}

#[test]
fn exposure_ledger_counts_administrations() {
    let template = Template::new(b"how many deputies?".to_vec());
    let item = template.variant(b"variant-1");
    let mut ledger = ExposureLedger::new();
    assert_eq!(ledger.count(&item), 0);
    for _ in 0..5 {
        ledger.record(item);
    }
    assert_eq!(ledger.count(&item), 5);
    assert!(ledger.is_overexposed(&item, 5));
    assert!(!ledger.is_overexposed(&item, 6));
}

#[test]
fn retirement_triggers_and_precedence() {
    let clean = ItemHealth::default();
    // Exposure alone retires past the limit.
    assert_eq!(
        should_retire(2000, 2000, clean),
        Some(RetirementReason::Exposure)
    );
    assert_eq!(should_retire(1999, 2000, clean), None);
    // Emerging DIF retires regardless of exposure, and outranks exposure.
    let dif = ItemHealth {
        emerging_dif: true,
        ..Default::default()
    };
    assert_eq!(
        should_retire(0, 2000, dif),
        Some(RetirementReason::EmergingDif)
    );
    assert_eq!(
        should_retire(9999, 2000, dif),
        Some(RetirementReason::EmergingDif)
    );
    // Drift and obsolescence are their own reasons.
    let drift = ItemHealth {
        drifted: true,
        ..Default::default()
    };
    assert_eq!(should_retire(0, 2000, drift), Some(RetirementReason::Drift));
}

#[test]
fn parametric_variants_are_distinct_and_stable() {
    let t1 = Template::new(b"structure A".to_vec());
    let t2 = Template::new(b"structure B".to_vec());
    // Same (structure, values) → same content id.
    assert_eq!(t1.variant(b"x=3"), t1.variant(b"x=3"));
    // Different values → different item.
    assert_ne!(t1.variant(b"x=3"), t1.variant(b"x=4"));
    // Different structure, same values → different item (no collision).
    assert_ne!(t1.variant(b"x=3"), t2.variant(b"x=3"));
}

#[test]
fn rotation_picks_the_least_exposed_variant() {
    let template = Template::new(b"parametric item".to_vec());
    let value_sets = vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()];
    let mut ledger = ExposureLedger::new();
    // Expose variants a and b heavily; c is fresh.
    for _ in 0..10 {
        ledger.record(template.variant(b"a"));
        ledger.record(template.variant(b"b"));
    }
    let (values, id) = least_exposed_variant(&template, &value_sets, &ledger).unwrap();
    assert_eq!(values, b"c");
    assert_eq!(id, template.variant(b"c"));
}

#[test]
fn revalidation_catches_dif_on_any_axis_including_the_blind_spot() {
    let (theta, axis_pol) = synthetic();
    let axis_edu: Vec<f64> = (0..theta.len())
        .map(|i| if i % 3 == 0 { 1.0 } else { -1.0 })
        .collect();

    let clean: Vec<f64> = theta
        .iter()
        .enumerate()
        .map(|(i, &t)| noisy(i, t > 0.0))
        .collect();
    let dif_pol: Vec<f64> = theta
        .iter()
        .enumerate()
        .map(|(i, &t)| noisy(i, t + 1.5 * axis_pol[i] > 0.0))
        .collect();
    let dif_edu: Vec<f64> = theta
        .iter()
        .enumerate()
        .map(|(i, &t)| noisy(i, t + 1.5 * axis_edu[i] > 0.0))
        .collect();

    // responses as respondents × items [clean, dif_pol, dif_edu]
    let responses: Vec<Vec<f64>> = (0..theta.len())
        .map(|i| vec![clean[i], dif_pol[i], dif_edu[i]])
        .collect();

    // Looking only at the political axis misses the education-biased item (blind spot).
    let single = revalidate_pool(&theta, &[axis_pol.clone()], &responses);
    assert!(
        !single[2].emerging_dif,
        "single-axis review should miss the edu-biased item"
    );

    // Multi-axis catches all real DIF and leaves the clean item alone.
    let multi = revalidate_pool(&theta, &[axis_pol, axis_edu], &responses);
    assert!(!multi[0].emerging_dif, "clean item wrongly flagged");
    assert!(multi[1].emerging_dif, "political DIF missed");
    assert!(multi[2].emerging_dif, "education DIF missed");
}

#[test]
fn revalidation_feeds_the_retirement_list() {
    let items = [
        Template::new(b"item A".to_vec()).variant(b"1"),
        Template::new(b"item B".to_vec()).variant(b"1"),
        Template::new(b"item C".to_vec()).variant(b"1"),
    ];
    // A is clean but over-exposed; B developed DIF; C is fine.
    let health = vec![
        ItemHealth::default(),
        ItemHealth {
            emerging_dif: true,
            ..Default::default()
        },
        ItemHealth::default(),
    ];
    let mut exposure = ExposureLedger::new();
    for _ in 0..2000 {
        exposure.record(items[0]);
    }

    let retire = items_to_retire(&items, &health, &exposure, 2000);
    assert_eq!(retire.len(), 2);
    assert!(retire.contains(&(items[0], RetirementReason::Exposure)));
    assert!(retire.contains(&(items[1], RetirementReason::EmergingDif)));
    assert!(
        !retire.iter().any(|(id, _)| *id == items[2]),
        "clean item should stay"
    );
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
