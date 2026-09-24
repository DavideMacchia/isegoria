//! Authenticated erasure recovery (docs/08 NET-007 / DS-4, T16): a corrupted (wrong, not
//! missing) shard must be detected before decoding, not silently fed to Reed–Solomon.
//! `reconstruct_verified` drops any shard that fails its committed manifest hash
//! (AT-NET-06).

use network::erasure::{encode, reconstruct, reconstruct_verified, RecoverError};

const DATA: usize = 4;
const PARITY: usize = 2; // 6 shards, need any 4

#[test]
fn at_net_06_a_corrupted_shard_is_detected_and_dropped() {
    let data = b"the quick brown fox jumps over the lazy dog, twice over".to_vec();
    let enc = encode(&data, DATA, PARITY);

    // All shards present, but shard 1 has a flipped byte (present-but-wrong, not missing).
    let mut shards: Vec<Option<Vec<u8>>> = enc.shards.iter().cloned().map(Some).collect();
    shards[1].as_mut().unwrap()[0] ^= 0xFF;

    // Unverified reconstruction trusts the corrupt shard and yields WRONG bytes — the
    // NET-007 vulnerability.
    let naive = reconstruct(shards.clone(), DATA, PARITY, enc.orig_len).unwrap();
    assert_ne!(naive, data, "unverified decode is silently corrupted");

    // Verified reconstruction drops the corrupt shard (it fails its manifest hash) and
    // recovers correctly from the remaining authentic shards.
    let recovered =
        reconstruct_verified(shards, &enc.manifest, DATA, PARITY, enc.orig_len).unwrap();
    assert_eq!(recovered, data, "the corrupt shard was authenticated out");
}

#[test]
fn too_many_corrupt_shards_fail_rather_than_trust_one() {
    let data = b"another payload to spread across shards".to_vec();
    let enc = encode(&data, DATA, PARITY);
    let mut shards: Vec<Option<Vec<u8>>> = enc.shards.iter().cloned().map(Some).collect();

    // Corrupt 3 of 6 shards: only 3 authentic remain, below the 4 needed — recovery must
    // fail rather than trust a corrupt shard.
    for shard in shards.iter_mut().take(3) {
        shard.as_mut().unwrap()[0] ^= 0xAA;
    }
    assert_eq!(
        reconstruct_verified(shards, &enc.manifest, DATA, PARITY, enc.orig_len),
        Err(RecoverError::TooFewAuthenticShards {
            authentic: 3,
            need: 4
        })
    );
}

#[test]
fn authentic_shards_with_losses_still_recover() {
    let data = b"lose some, keep the rest".to_vec();
    let enc = encode(&data, DATA, PARITY);
    let mut shards: Vec<Option<Vec<u8>>> = enc.shards.iter().cloned().map(Some).collect();

    // Two shards genuinely lost (None), the other four authentic — recovers.
    shards[0] = None;
    shards[5] = None;
    let recovered =
        reconstruct_verified(shards, &enc.manifest, DATA, PARITY, enc.orig_len).unwrap();
    assert_eq!(recovered, data);
}

#[test]
fn an_impossible_layout_is_refused_not_sized() {
    // The shard counts and `orig_len` travel with the shards, so they are untrusted
    // (T44): an overflowing count used to panic inside `ReedSolomon::new`, and a huge
    // `orig_len` used to be allocated before anything checked it.
    let enc = encode(b"layout", DATA, PARITY);
    let shards = || enc.shards.iter().cloned().map(Some).collect::<Vec<_>>();
    let verified =
        |shards, data, parity, len| reconstruct_verified(shards, &enc.manifest, data, parity, len);

    assert_eq!(
        verified(shards(), usize::MAX, 1, enc.orig_len),
        Err(RecoverError::InvalidLayout)
    );
    assert_eq!(
        verified(shards(), DATA, PARITY, usize::MAX),
        Err(RecoverError::InvalidLayout)
    );
    assert_eq!(
        verified(shards(), 0, PARITY, 0),
        Err(RecoverError::InvalidLayout)
    );
    assert_eq!(
        verified(shards()[..5].to_vec(), DATA, PARITY, enc.orig_len),
        Err(RecoverError::InvalidLayout)
    );
    assert!(reconstruct(shards(), usize::MAX, 1, enc.orig_len).is_none());
    assert!(reconstruct(shards(), DATA, PARITY, usize::MAX).is_none());
    // The true layout still recovers.
    assert_eq!(
        verified(shards(), DATA, PARITY, enc.orig_len).unwrap(),
        b"layout"
    );
}
