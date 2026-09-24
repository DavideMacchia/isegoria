//! AT-NET-07: `OtsAnchor::verify` on arbitrary receipt bytes — no panic, no abort, and
//! bounded time and memory (T44). The claimed root is the digest the bytes carry, when
//! they carry one, so inputs get past the root check into the step tree.
#![no_main]

use libfuzzer_sys::fuzz_target;
use network::anchoring::{Anchor, OtsAnchor, Receipt};

/// Magic (31 bytes), version and digest type precede a SHA-256 digest.
const DIGEST_AT: usize = 33;

fuzz_target!(|data: &[u8]| {
    let mut anchor = OtsAnchor::new("https://calendar.example");
    // One confirmed block, so a Bitcoin attestation can meet a known height.
    let pending = anchor.submit([0u8; 32]);
    anchor.upgrade(&pending);

    let root = data
        .get(DIGEST_AT..DIGEST_AT + 32)
        .and_then(|d| d.try_into().ok())
        .unwrap_or([0u8; 32]);
    let _ = anchor.verify(&Receipt {
        root,
        proof: data.to_vec(),
    });
});
