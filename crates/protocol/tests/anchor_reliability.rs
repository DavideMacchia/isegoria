//! The anchor-reliability precondition of the latent re-check: the production detector
//! refuses to run below `KR20_MIN` (`docs/01` D37, `docs/08` DIF-010, T53).

use identity::nym::Nym;
use protocol::admission::NullifierSet;
use protocol::pilot::{admit_anchors, PilotError};
use protocol::revalidation::{latent_flags, revalidate_batch_latent, N_LATENT_MIN};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use scoring::dif::mixture_dif;
use scoring::irt::{kr20, theta_from_anchors, KR20_MIN};

const N: usize = 6000;
const K: usize = 8;

fn normal(r: &mut ChaCha8Rng) -> f64 {
    let u1: f64 = 1.0 - r.gen::<f64>();
    let u2: f64 = r.gen();
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

fn sigmoid(z: f64) -> f64 {
    1.0 / (1.0 + (-z).exp())
}

/// The paper's null batch (`paper/scripts/common.py::dif_generate`, `n_biased = 0`).
/// Returns (anchors, responses), both respondents × items.
fn null_batch(seed: u64, n_anchor: usize) -> (Vec<Vec<f64>>, Vec<Vec<f64>>) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let theta: Vec<f64> = (0..N).map(|_| normal(&mut rng)).collect();
    let a_anchor: Vec<f64> = (0..n_anchor).map(|_| rng.gen_range(0.9..1.6)).collect();
    let b_anchor: Vec<f64> = (0..n_anchor).map(|_| normal(&mut rng)).collect();
    let anchors: Vec<Vec<f64>> = theta
        .iter()
        .map(|&t| {
            (0..n_anchor)
                .map(|j| f64::from(rng.gen::<f64>() < sigmoid(a_anchor[j] * (t - b_anchor[j]))))
                .collect()
        })
        .collect();
    let a: Vec<f64> = (0..K).map(|_| rng.gen_range(1.0..1.5)).collect();
    let b: Vec<f64> = (0..K).map(|_| 0.6 * normal(&mut rng)).collect();
    let responses: Vec<Vec<f64>> = theta
        .iter()
        .map(|&t| {
            (0..K)
                .map(|j| f64::from(rng.gen::<f64>() < sigmoid(a[j] * (t - b[j]))))
                .collect()
        })
        .collect();
    (anchors, responses)
}

fn respondents(n: usize) -> NullifierSet {
    let mut set = NullifierSet::new();
    for i in 0..n {
        let mut id = [0u8; 32];
        id[..8].copy_from_slice(&(i as u64).to_le_bytes());
        set.spend(Nym(id)).unwrap();
    }
    set
}

/// The KR-20 proxy's reliability rises with the anchor count, as in the paper's Table.
#[test]
fn kr20_follows_the_anchor_count_as_in_the_paper() {
    let expect = [(10, 0.694), (20, 0.822), (30, 0.874), (60, 0.932)];
    for (n_anchor, paper) in expect {
        let mean = (1300..1304)
            .map(|seed| kr20(&null_batch(seed, n_anchor).0))
            .sum::<f64>()
            / 4.0;
        println!("{n_anchor} anchors: KR-20 {mean:.3} (paper {paper:.3})");
        assert!(
            (mean - paper).abs() < 0.03,
            "{n_anchor} anchors: KR-20 {mean:.3} vs the paper's {paper:.3}"
        );
    }
    assert!(kr20(&null_batch(1300, 30).0) < KR20_MIN);
    assert!(kr20(&null_batch(1300, 60).0) >= KR20_MIN);
}

/// AT-DIF-11, the refusal: 20 anchors are refused before any fit; the bare engine on the
/// same batch shows why, selecting a spurious two-class model.
#[test]
fn at_dif_11_twenty_anchors_are_refused_before_the_fit() {
    let (anchors, responses) = null_batch(1300, 20);
    let people = respondents(N);
    let refused = revalidate_batch_latent(&people, &anchors, &responses, 0);
    let Err(PilotError::UnreliableAnchors { kr20: r, need }) = refused else {
        panic!("a 20-anchor proxy was accepted: {refused:?}");
    };
    assert!((0.78..0.87).contains(&r), "KR-20 {r:.3}");
    assert_eq!(need, KR20_MIN);

    // Why (DIF-010): the same batch through the bare engine.
    let theta = theta_from_anchors(&anchors);
    let res = mixture_dif(&theta, &responses, K, 0);
    let flags = latent_flags(&res);
    println!(
        "20 anchors, unguarded: {} classes, BIC gain {:.1}, max gap {:.3}, {} clean items flagged",
        res.classes,
        res.bic_gain,
        res.dif.iter().cloned().fold(0.0, f64::max),
        flags.iter().filter(|&&f| f).count()
    );
    assert!(
        res.classes >= 2 && res.bic_gain > 0.0,
        "no spurious mixture on this null batch: {:?}",
        res.candidates
    );
}

/// AT-DIF-11, the acceptance: with 60 anchors the proxy is reliable, the batch is
/// admitted, and no clean item is flagged.
#[test]
fn at_dif_11_sixty_anchors_are_accepted_and_raise_no_flag() {
    let (anchors, responses) = null_batch(1300, 60);
    let r = admit_anchors(&anchors).unwrap();
    assert!(r >= KR20_MIN, "KR-20 {r:.3}");
    let flags = revalidate_batch_latent(&respondents(N), &anchors, &responses, 0).unwrap();
    println!("60 anchors: KR-20 {r:.3}, flags {flags:?}");
    assert_eq!(flags, vec![false; K]);
}

/// The precondition is undefined — and therefore refused — with fewer than two anchors,
/// with anchors nobody spreads on, and it never lets a NaN through.
#[test]
fn an_undefined_reliability_is_refused() {
    let one = vec![vec![1.0]; N_LATENT_MIN];
    assert!(matches!(
        admit_anchors(&one),
        Err(PilotError::UnreliableAnchors { kr20, .. }) if kr20 == 0.0
    ));
    let flat = vec![vec![1.0, 0.0, 1.0]; N_LATENT_MIN];
    assert!(matches!(
        admit_anchors(&flat),
        Err(PilotError::UnreliableAnchors { kr20, .. }) if kr20 == 0.0
    ));
    let mut poisoned = null_batch(7, 20).0;
    poisoned[0][0] = f64::NAN;
    assert!(matches!(
        admit_anchors(&poisoned),
        Err(PilotError::UnreliableAnchors { kr20, .. }) if kr20.is_nan() || kr20 == 0.0
    ));
    assert!(admit_anchors(&[]).is_err());
}

/// The anchors are the respondents' rows too: a count or a length that does not match is
/// a `RowCountMismatch`, checked before the reliability.
#[test]
fn anchor_rows_must_be_the_respondents() {
    let (anchors, responses) = null_batch(1300, 60);
    let people = respondents(N - 1);
    assert_eq!(
        revalidate_batch_latent(&people, &anchors, &responses[..N - 1], 0),
        Err(PilotError::RowCountMismatch {
            rows: N,
            respondents: N - 1
        })
    );
    let mut ragged = anchors.clone();
    ragged[5].pop();
    assert_eq!(
        revalidate_batch_latent(&respondents(N), &ragged, &responses, 0),
        Err(PilotError::RowCountMismatch {
            rows: 59,
            respondents: 60
        })
    );
}
