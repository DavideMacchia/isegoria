//! The OPRF wire messages (RFC 9497, `docs/03` §M1; T44). Enrollment will exchange them
//! over a network: the server decodes a blinded element from an untrusted client, and the
//! client decodes an evaluation and a proof from an untrusted server. Decoding arbitrary
//! bytes never panics, and no decoded evaluation passes verification without the server
//! key.
#![no_main]

use libfuzzer_sys::fuzz_target;
use rand_core::{CryptoRng, RngCore};
use std::sync::OnceLock;
use voprf::{BlindedElement, EvaluationElement, Proof, Ristretto255, VoprfClient, VoprfServer};

/// Deterministic randomness so a crash replays (SplitMix64; fuzzing only).
struct FuzzRng(u64);

impl RngCore for FuzzRng {
    fn next_u32(&mut self) -> u32 {
        self.next_u64() as u32
    }
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        for chunk in dest.chunks_mut(8) {
            let v = self.next_u64().to_le_bytes();
            chunk.copy_from_slice(&v[..chunk.len()]);
        }
    }
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> {
        self.fill_bytes(dest);
        Ok(())
    }
}
impl CryptoRng for FuzzRng {}

fn server() -> &'static VoprfServer<Ristretto255> {
    static SERVER: OnceLock<VoprfServer<Ristretto255>> = OnceLock::new();
    SERVER.get_or_init(|| {
        VoprfServer::new_from_seed(&[7u8; 32], b"isegoria/uniqueness/v1").expect("valid key")
    })
}

fuzz_target!(|input: (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>)| {
    let (blinded, element, proof, anchor) = input;
    let mut rng = FuzzRng(anchor.len() as u64);

    // Server side: whatever a client sends.
    if let Ok(blinded) = BlindedElement::<Ristretto255>::deserialize(&blinded) {
        let _ = server().blind_evaluate(&mut rng, &blinded);
    }

    // Client side: whatever a server sends back.
    let (Ok(element), Ok(proof)) = (
        EvaluationElement::<Ristretto255>::deserialize(&element),
        Proof::<Ristretto255>::deserialize(&proof),
    ) else {
        return;
    };
    if let Ok(blind) = VoprfClient::<Ristretto255>::blind(&anchor, &mut rng) {
        let output = blind
            .state
            .finalize(&anchor, &element, &proof, server().get_public_key());
        assert!(
            output.is_err(),
            "an evaluation verified without the server key"
        );
    }
});
