//! Merkle proofs (T44): a proof exists exactly for an in-range leaf and verifies against
//! the root; verification of an arbitrary proof never panics.
#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use network::merkle::{merkle_proof, merkle_root, verify_proof, MerkleProof};

#[derive(Debug, Arbitrary)]
struct Input {
    leaves: Vec<[u8; 32]>,
    index: usize,
    siblings: Vec<(bool, [u8; 32])>,
    leaf: [u8; 32],
}

fuzz_target!(|input: Input| {
    let root = merkle_root(&input.leaves);
    match merkle_proof(&input.leaves, input.index) {
        Some(proof) => assert!(verify_proof(input.leaves[input.index], &proof, root)),
        None => assert!(input.index >= input.leaves.len()),
    }
    let proof = MerkleProof {
        siblings: input.siblings,
    };
    let _ = verify_proof(input.leaf, &proof, root);
});
