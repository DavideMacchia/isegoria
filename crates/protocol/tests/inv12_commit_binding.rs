//! Commit–reveal binding (`docs/08` CRYPTO-007/INV-12, T7): a blind review commitment
//! binds who cast it and which item, so only that committer can open it, for that item
//! (AT-BR-06).

use identity::nym::Nym;
use network::cid::cid;
use protocol::review::{commit, reveal};

fn alice() -> Nym {
    Nym([1u8; 32])
}
fn mallory() -> Nym {
    Nym([2u8; 32])
}

#[test]
fn at_br_06_a_copied_commitment_cannot_be_opened_by_someone_else() {
    let item = cid(b"item-under-review");
    let (prob, nonce) = (0.8, [7u8; 32]);

    // Alice commits to her judgment, bound to her nym and the item.
    let c = commit(prob, &nonce, alice(), item);
    assert!(
        reveal(c, prob, &nonce, alice(), item),
        "the committer can open it"
    );

    // Bound to Alice: knowing the opening does not let Mallory recompute it as her own.
    assert!(
        !reveal(c, prob, &nonce, mallory(), item),
        "a copied commitment must not open under another committer (AT-BR-06)"
    );
}

#[test]
fn a_commitment_cannot_be_replayed_onto_another_item() {
    let (item_a, item_b) = (cid(b"item-A"), cid(b"item-B"));
    let (prob, nonce) = (0.6, [9u8; 32]);
    let c = commit(prob, &nonce, alice(), item_a);
    assert!(reveal(c, prob, &nonce, alice(), item_a));
    assert!(
        !reveal(c, prob, &nonce, alice(), item_b),
        "a commitment for one item must not open for another (INV-12)"
    );
}

#[test]
fn distinct_committers_or_items_give_distinct_commitments() {
    let item = cid(b"item");
    let (prob, nonce) = (0.5, [0u8; 32]);
    let base = commit(prob, &nonce, alice(), item);
    assert_ne!(
        base,
        commit(prob, &nonce, mallory(), item),
        "bound to committer"
    );
    assert_ne!(
        base,
        commit(prob, &nonce, alice(), cid(b"other")),
        "bound to item"
    );
    // Same inputs still reproduce the same commitment (determinism).
    assert_eq!(base, commit(prob, &nonce, alice(), item));
}
