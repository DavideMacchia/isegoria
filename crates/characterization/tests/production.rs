//! The harness reads the production verdict: on a batch the gates admit, its flags are those
//! of `revalidate_batch_latent`; on one they refuse, it records the refusal (`docs/13` §2).

use characterization::generate::dif_batch;
use characterization::grid::{engine_seed, DifDesign, Layout};
use characterization::run::dif;
use identity::nym::Nym;
use protocol::admission::NullifierSet;
use protocol::pilot::PilotError;
use protocol::revalidation::revalidate_batch_latent;

fn respondents(n: usize) -> NullifierSet {
    let mut set = NullifierSet::new();
    for i in 0..n {
        let mut id = [0u8; 32];
        id[..8].copy_from_slice(&(i as u64).to_le_bytes());
        set.spend(Nym(id)).unwrap();
    }
    set
}

/// An admitted batch: the same flags as the production re-check with the same engine seed.
#[test]
fn an_admitted_batch_gets_the_production_flags() {
    let d = DifDesign {
        n: 3000,
        anchors: 60,
        k: 4,
        layout: Layout::Campaign(2),
        delta: 0.9,
        ..DifDesign::default()
    };
    let seed = 24;
    let batch = dif_batch(&d, seed);
    let outcome = dif(&d, seed);
    assert!(outcome.admitted, "KR-20 {}", outcome.kr20);
    let production = revalidate_batch_latent(
        &respondents(3000),
        &batch.anchors,
        &batch.x,
        engine_seed(seed),
    );
    assert_eq!(Ok(outcome.flags.clone()), production);
    assert_eq!(outcome.roles, "++cc");
}

/// A batch whose anchors are unreliable is refused by production and recorded as refused.
#[test]
fn a_refused_batch_is_recorded_as_refused() {
    let d = DifDesign {
        n: 3000,
        anchors: 20,
        k: 2,
        ..DifDesign::default()
    };
    let batch = dif_batch(&d, 5);
    let outcome = dif(&d, 5);
    assert!(!outcome.admitted);
    assert!(matches!(
        revalidate_batch_latent(&respondents(3000), &batch.anchors, &batch.x, 5),
        Err(PilotError::UnreliableAnchors { .. })
    ));
}
