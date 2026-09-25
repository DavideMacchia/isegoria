//! Respondents pass the identity gate, and the pilot floors count persons, not rows
//! (`docs/08` §9.1 `Pilot1` row, PROTO-013, INV-9, T65 — AT-PRO-09).
//!
//! A respondent enters a batch through `pilot::submit_response`, proving a `Respond`
//! nullifier bound to the batch and the epoch; the admitted `NullifierSet` is what the
//! floors of `screen`, `dif_batch` and `revalidate_batch_latent` count. So the same
//! person cannot answer a batch twice, 300 answer sheets from one person do not meet
//! `N1_MIN`, a proof for one batch is refused on another, and a row that no admitted
//! respondent stands behind is refused.

use identity::credential::{AnonymousCredential, Credential, Issuer};
use identity::enrollment::Label;
use identity::nullifier::{prove, NullifierProof};
use identity::nym::{Nym, Role};
use network::cid::{cid, Cid};
use protocol::admission::{NullifierSet, Unproven};
use protocol::pilot::{
    batch_id, response_context, screen, submit_response, PilotError, ResponseRejected, N1_MIN,
};
use protocol::revalidation::{revalidate_batch_latent, N_LATENT_MIN};

const EPOCH: u64 = 7;

fn issuer() -> Issuer {
    Issuer::new([1u8; 32])
}

/// One person: a credential issued for its own label.
fn person(issuer: &Issuer, i: u32) -> AnonymousCredential {
    let mut secret = [0u8; 32];
    secret[..4].copy_from_slice(&i.to_le_bytes());
    secret[4] = 0x51;
    let mut label = [0u8; 32];
    label[..4].copy_from_slice(&i.to_le_bytes());
    label[4] = 0x1B;
    let holder = Credential::from_secret(secret);
    let (req, pending) = holder.request_issuance(&Label(label), &issuer.public());
    pending.finalize(issuer.issue(&req).unwrap())
}

/// A `Respond` proof bound to `batch` at `EPOCH`, the way a real respondent client builds it.
fn respond_proof(issuer: &Issuer, cred: &AnonymousCredential, batch: Cid) -> NullifierProof {
    prove(
        cred,
        &issuer.public(),
        Role::Respond,
        &response_context(batch, EPOCH),
    )
}

/// A respondent set of `n` admitted ids, built directly (the gate itself is tested above;
/// the floors only read the set's size).
fn admitted(n: usize) -> NullifierSet {
    let mut set = NullifierSet::new();
    for i in 0..n {
        let mut id = [0u8; 32];
        id[..8].copy_from_slice(&(i as u64).to_le_bytes());
        set.spend(Nym(id)).unwrap();
    }
    set
}

/// 40 anchors answered by `n` respondents in a perfect Guttman pattern: reliable well
/// above `KR20_MIN`, so the D37 anchor gate of the latent re-check (T53,
/// `anchor_reliability.rs`) admits them and only the respondent checks decide here.
fn reliable_anchors(n: usize) -> Vec<Vec<f64>> {
    (0..n)
        .map(|i| {
            let total = i * 41 / n;
            (0..40).map(|j| if j < total { 1.0 } else { 0.0 }).collect()
        })
        .collect()
}

/// `n` respondents' abilities and `m` item columns of `n` answers each, non-degenerate.
fn sample(n: usize, m: usize) -> (Vec<f64>, Vec<Vec<f64>>) {
    let theta: Vec<f64> = (0..n).map(|i| (i as f64 / n as f64) - 0.5).collect();
    let items: Vec<Vec<f64>> = (0..m)
        .map(|j| (0..n).map(|i| ((i * 7 + j * 3) % 5) as f64 * 0.2).collect())
        .collect();
    (theta, items)
}

// -------------------------------- AT-PRO-09 --------------------------------

#[test]
fn the_same_person_cannot_answer_a_batch_twice() {
    let issuer = issuer();
    let alice = person(&issuer, 1);
    let batch = batch_id(&[cid(b"item-A"), cid(b"item-B")]);
    let mut respondents = NullifierSet::new();

    // Two independent proofs (fresh randomness) by the same person share the `Respond`
    // nullifier id: the first is admitted, the second is a duplicate.
    let p1 = respond_proof(&issuer, &alice, batch);
    let p2 = respond_proof(&issuer, &alice, batch);
    let id = submit_response(&p1, &issuer.public(), batch, EPOCH, &mut respondents).unwrap();
    assert_eq!(id, p1.id());
    assert_eq!(
        submit_response(&p2, &issuer.public(), batch, EPOCH, &mut respondents),
        Err(ResponseRejected::Duplicate)
    );
    assert_eq!(respondents.len(), 1, "one person, one respondent");

    // Another person is admitted alongside.
    let bob = person(&issuer, 2);
    let pb = respond_proof(&issuer, &bob, batch);
    assert!(submit_response(&pb, &issuer.public(), batch, EPOCH, &mut respondents).is_ok());
    assert_eq!(respondents.len(), 2);
}

#[test]
fn three_hundred_rows_from_one_respondent_are_not_enough() {
    // One admitted person hands in 300 answer sheets: the stage-1 floor counts persons.
    let issuer = issuer();
    let batch = batch_id(&[cid(b"item-A"), cid(b"item-B"), cid(b"item-C")]);
    let mut respondents = NullifierSet::new();
    let proof = respond_proof(&issuer, &person(&issuer, 1), batch);
    submit_response(&proof, &issuer.public(), batch, EPOCH, &mut respondents).unwrap();

    let (theta, items) = sample(300, 3);
    assert_eq!(
        screen(&respondents, &theta, &items),
        Err(PilotError::NotEnoughRespondents {
            have: 1,
            need: N1_MIN
        })
    );

    // The same for the production latent re-check: 3,000 rows (and 3,000 anchor rows),
    // one person.
    let anchors = reliable_anchors(N_LATENT_MIN);
    let responses: Vec<Vec<f64>> = (0..N_LATENT_MIN)
        .map(|i| (0..8).map(|j| ((i + j) % 2) as f64).collect())
        .collect();
    assert_eq!(
        revalidate_batch_latent(&respondents, &anchors, &responses, 0),
        Err(PilotError::NotEnoughRespondents {
            have: 1,
            need: N_LATENT_MIN
        })
    );
}

#[test]
fn a_proof_for_one_batch_is_refused_on_another() {
    let issuer = issuer();
    let alice = person(&issuer, 1);
    let batch_a = batch_id(&[cid(b"item-A"), cid(b"item-B")]);
    let batch_b = batch_id(&[cid(b"item-C"), cid(b"item-D")]);
    let proof = respond_proof(&issuer, &alice, batch_a);

    let mut on_b = NullifierSet::new();
    assert_eq!(
        submit_response(&proof, &issuer.public(), batch_b, EPOCH, &mut on_b),
        Err(ResponseRejected::Unproven(Unproven::BadProof))
    );
    assert!(on_b.is_empty(), "a refused proof admits nobody");

    // Nor does it outlive its epoch.
    let mut later = NullifierSet::new();
    assert_eq!(
        submit_response(&proof, &issuer.public(), batch_a, EPOCH + 1, &mut later),
        Err(ResponseRejected::Unproven(Unproven::BadProof))
    );

    // On its own batch and epoch it is admitted.
    let mut on_a = NullifierSet::new();
    assert!(submit_response(&proof, &issuer.public(), batch_a, EPOCH, &mut on_a).is_ok());
}

#[test]
fn a_proof_for_another_role_cannot_respond() {
    let issuer = issuer();
    let alice = person(&issuer, 1);
    let batch = batch_id(&[cid(b"item-A"), cid(b"item-B")]);
    let judge = prove(
        &alice,
        &issuer.public(),
        Role::Judge,
        &response_context(batch, EPOCH),
    );
    let mut respondents = NullifierSet::new();
    assert_eq!(
        submit_response(&judge, &issuer.public(), batch, EPOCH, &mut respondents),
        Err(ResponseRejected::Unproven(Unproven::WrongRole))
    );
}

// -------------------------------- rows are persons --------------------------------

#[test]
fn rows_without_a_respondent_are_refused() {
    let respondents = admitted(N1_MIN);

    // One row more than the admitted respondents: somebody's answers stand behind no
    // proof.
    let (theta, items) = sample(N1_MIN + 1, 3);
    assert_eq!(
        screen(&respondents, &theta, &items),
        Err(PilotError::RowCountMismatch {
            rows: N1_MIN + 1,
            respondents: N1_MIN
        })
    );

    // An item column of another length is refused too.
    let (theta, mut items) = sample(N1_MIN, 3);
    items[1].push(0.0);
    assert_eq!(
        screen(&respondents, &theta, &items),
        Err(PilotError::RowCountMismatch {
            rows: N1_MIN + 1,
            respondents: N1_MIN
        })
    );

    // Exactly the admitted respondents: the screen runs.
    let (theta, items) = sample(N1_MIN, 3);
    assert!(screen(&respondents, &theta, &items).is_ok());

    // The latent re-check: 3,000 admitted persons, 3,001 rows.
    let respondents = admitted(N_LATENT_MIN);
    let anchors = reliable_anchors(N_LATENT_MIN + 1);
    let responses: Vec<Vec<f64>> = (0..N_LATENT_MIN + 1)
        .map(|i| (0..8).map(|j| ((i + j) % 2) as f64).collect())
        .collect();
    assert_eq!(
        revalidate_batch_latent(&respondents, &anchors, &responses, 0),
        Err(PilotError::RowCountMismatch {
            rows: N_LATENT_MIN + 1,
            respondents: N_LATENT_MIN
        })
    );
    // The anchor sheets are persons too: 3,000 answer rows but 3,001 anchor rows.
    assert_eq!(
        revalidate_batch_latent(&respondents, &anchors, &responses[..N_LATENT_MIN], 0),
        Err(PilotError::RowCountMismatch {
            rows: N_LATENT_MIN + 1,
            respondents: N_LATENT_MIN
        })
    );
}

#[test]
fn a_batch_is_named_by_its_items_whatever_their_order() {
    let (a, b, c) = (cid(b"item-A"), cid(b"item-B"), cid(b"item-C"));
    assert_eq!(batch_id(&[a, b, c]), batch_id(&[c, a, b]));
    assert_eq!(
        batch_id(&[a, b, c]),
        batch_id(&[a, b, c, b]),
        "a batch is a set"
    );
    assert_ne!(batch_id(&[a, b]), batch_id(&[a, b, c]));
    assert_ne!(batch_id(&[a]), a, "a batch id is not an item id");
}
