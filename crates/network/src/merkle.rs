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

/// Hashes one level into the next, RFC 6962 style: pairs are hashed, and a lone odd node
/// is *promoted* (carried up unchanged) rather than paired with itself. Self-pairing would
/// let `[x, y, z]` and `[x, y, z, z]` share a root (CVE-2012-2459).
fn next_level(level: &[[u8; 32]]) -> Vec<[u8; 32]> {
    let mut next = Vec::with_capacity(level.len().div_ceil(2));
    for pair in level.chunks(2) {
        next.push(if pair.len() == 2 {
            node_hash(&pair[0], &pair[1])
        } else {
            pair[0] // promote the odd node unchanged
        });
    }
    next
}

/// Root over leaf hashes. An odd node is promoted (see [`next_level`]).
pub fn merkle_root(leaves: &[[u8; 32]]) -> [u8; 32] {
    if leaves.is_empty() {
        return tagged("isegoria/merkle/empty", &[]);
    }
    let mut level = leaves.to_vec();
    while level.len() > 1 {
        level = next_level(&level);
    }
    level[0]
}

/// An inclusion proof: sibling hashes bottom-up, each tagged with its side.
#[derive(Clone, Debug)]
pub struct MerkleProof {
    pub siblings: Vec<(bool, [u8; 32])>, // (sibling_is_right, hash)
}

/// The inclusion proof for `leaves[index]`, or `None` if there is no such leaf: an
/// out-of-range index is refused rather than indexed.
pub fn merkle_proof(leaves: &[[u8; 32]], mut index: usize) -> Option<MerkleProof> {
    if index >= leaves.len() {
        return None;
    }
    let mut siblings = Vec::new();
    let mut level = leaves.to_vec();
    while level.len() > 1 {
        // A promoted odd node has no sibling at this level: it is carried straight
        // up, so we record nothing and let `index /= 2` place it in the next level.
        let is_promoted = index == level.len() - 1 && level.len() % 2 == 1;
        if !is_promoted {
            let sibling_is_right = index % 2 == 0;
            let sib = if sibling_is_right {
                index + 1
            } else {
                index - 1
            };
            siblings.push((sibling_is_right, level[sib]));
        }
        level = next_level(&level);
        index /= 2;
    }
    Some(MerkleProof { siblings })
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
