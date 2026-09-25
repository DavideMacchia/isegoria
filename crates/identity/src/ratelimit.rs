//! Rate-limiting nullifier (`docs/03` §Cost of proposing): `H(secret, role, epoch, slot)`
//! per slot; reuse beyond the quota collides and is caught as a double-spend, modeling
//! the ZK secret-reveal the real design uses.

use crate::hash::tagged;
use crate::nym::Role;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Token(pub [u8; 32]);

pub fn rln_token(secret: &[u8; 32], role: Role, epoch: u64, slot: u32) -> Token {
    Token(tagged(
        "isegoria/rln/v1",
        &[
            secret,
            role.tag().as_bytes(),
            &epoch.to_le_bytes(),
            &slot.to_le_bytes(),
        ],
    ))
}

pub fn within_quota(slot: u32, quota: u32) -> bool {
    slot < quota
}

#[derive(Debug, PartialEq, Eq)]
pub struct DoubleSpend;

/// Records spent tokens for an epoch and rejects reuse.
#[derive(Default)]
pub struct SlotLedger {
    seen: HashSet<Token>,
}

impl SlotLedger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn spend(&mut self, token: Token) -> Result<(), DoubleSpend> {
        if self.seen.insert(token) {
            Ok(())
        } else {
            Err(DoubleSpend)
        }
    }
}
