//! The reviewer floor at the protocol boundary (`docs/02` §A.4, `docs/08` BRIDGE-001 and
//! PROTO-003, T39): a reviewer defines the latent axis once it has `N_MIN_REVIEWS` reviews
//! on record — a founder from the start — and until then it fills the space: its `f_u`
//! is fixed at 0 in the fit, it weighs 0 while on probation (D36), and it is assigned
//! like any other candidate, from the position it has.

use identity::nym::Nym;
use protocol::orchestrator::{axis_mask, weighted_ratings, ReviewerStanding, N_MIN_REVIEWS};
use protocol::probation::{SkillTrack, N_PROBATION};
use protocol::review::{assign_reviewers, Reviewer};
use scoring::reputation::CusumParams;

#[test]
fn the_axis_is_defined_by_founders_and_reviewers_past_the_floor() {
    let prev = [
        ReviewerStanding::founder(),
        ReviewerStanding::established(0.02),
        ReviewerStanding {
            is_founder: false,
            judgments_with_outcome: 0,
            skill: 0.0,
            reviews: 3,
        },
        ReviewerStanding {
            is_founder: false,
            judgments_with_outcome: 12,
            skill: 0.1,
            reviews: N_MIN_REVIEWS - 1,
        },
        ReviewerStanding {
            is_founder: false,
            judgments_with_outcome: 12,
            skill: 0.1,
            reviews: N_MIN_REVIEWS,
        },
    ];
    assert_eq!(axis_mask(&prev), vec![true, true, false, false, true]);
    assert_eq!(N_MIN_REVIEWS, 30);

    // The ratings the fit sees carry the mask.
    let r = vec![vec![0.8, 0.6, 0.7]; 5];
    let mask = vec![vec![true; 3]; 5];
    let ratings = weighted_ratings(&r, &mask, &prev, 3.0).unwrap();
    assert_eq!(ratings.axis, vec![true, true, false, false, true]);
    assert_eq!(ratings.weights[2], 0.0, "on probation: weight 0");
}

/// The track counts every reviewed item, observed or not, so a reviewer crosses the
/// floor after `N_MIN_REVIEWS` reviews — before it is out of probation if some outcomes
/// were never observed.
#[test]
fn the_track_carries_the_reviews_on_record_into_the_standing() {
    let params = CusumParams::default();
    let mut track = SkillTrack::new();
    for _ in 0..20 {
        track.record(0.05, &params);
    }
    for _ in 0..10 {
        track.record_unobserved();
    }
    let standing = track.standing(false);
    assert_eq!(standing.reviews, 30);
    assert_eq!(standing.judgments_with_outcome, 20);
    assert!(standing.judgments_with_outcome < N_PROBATION);
    assert_eq!(axis_mask(&[standing]), vec![true]);
}

/// The new-reviewer path of PROTO-003: a newcomer has the position 0 the fit gives it
/// and is a candidate like any other — drawn into panels, at weight 0 (D36).
#[test]
fn a_newcomer_is_assigned_from_the_position_it_has() {
    let mut reviewers: Vec<Reviewer> = (0..40)
        .map(|i| Reviewer {
            nym: Nym([i as u8; 32]),
            f_u: (i as f64 - 20.0) / 10.0,
        })
        .collect();
    let newcomer = Nym([200; 32]);
    reviewers.push(Reviewer {
        nym: newcomer,
        f_u: 0.0,
    });
    let drawn = (0..200u64)
        .filter(|&seed| {
            assign_reviewers(&reviewers, 9, seed)
                .iter()
                .any(|r| r.nym == newcomer)
        })
        .count();
    println!("the newcomer sits on {drawn} of 200 panels");
    assert!(drawn > 0, "a newcomer is never assigned");
    assert!(drawn < 200, "a newcomer is always assigned");
}
