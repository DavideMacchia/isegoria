//! Merkle tree (`docs/04`, §Merkle tree): one root summarizes many records;
//! changing any record changes the root, visibly. Leaves and internal nodes are
//! domain-separated to prevent second-preimage attacks.

use crate::hash::tagged;

pub fn leaf_hash(data: &[u8]) -> [u8; 32] {
    tagged("isegoria/merkle/leaf", &[data])
}

fn node_hash(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    tagged("isegoria/merkle/node", &[left, right])
}

/// Root over leaf hashes. An odd node is paired with itself.
pub fn merkle_root(leaves: &[[u8; 32]]) -> [u8; 32] {
    if leaves.is_empty() {
        return tagged("isegoria/merkle/empty", &[]);
    }
    let mut level = leaves.to_vec();
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        for pair in level.chunks(2) {
            let right = if pair.len() == 2 { &pair[1] } else { &pair[0] };
            next.push(node_hash(&pair[0], right));
        }
        level = next;
    }
    level[0]
}

/// An inclusion proof: sibling hashes bottom-up, each tagged with its side.
#[derive(Clone, Debug)]
pub struct MerkleProof {
    pub siblings: Vec<(bool, [u8; 32])>, // (sibling_is_right, hash)
}

pub fn merkle_proof(leaves: &[[u8; 32]], mut index: usize) -> MerkleProof {
    let mut siblings = Vec::new();
    let mut level = leaves.to_vec();
    while level.len() > 1 {
        let sibling_is_right = index % 2 == 0;
        let sib = if sibling_is_right {
            if index + 1 < level.len() {
                index + 1
            } else {
                index // odd node paired with itself
            }
        } else {
            index - 1
        };
        siblings.push((sibling_is_right, level[sib]));

        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        for pair in level.chunks(2) {
            let right = if pair.len() == 2 { &pair[1] } else { &pair[0] };
            next.push(node_hash(&pair[0], right));
        }
        level = next;
        index /= 2;
    }
    MerkleProof { siblings }
}

pub fn verify_proof(leaf: [u8; 32], proof: &MerkleProof, root: [u8; 32]) -> bool {
    let mut acc = leaf;
    for (sibling_is_right, sib) in &proof.siblings {
        acc = if *sibling_is_right {
            node_hash(&acc, sib)
        } else {
            node_hash(sib, &acc)
        };
    }
    acc == root
}
