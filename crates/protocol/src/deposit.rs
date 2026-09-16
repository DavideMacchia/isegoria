//! [2] Deposit (`docs/05`): the draft is content-addressed and its hash recorded
//! on the append-only log. The bond is in reputation, never money (invariant #3).

use network::cid::{cid, Cid};
use network::log::TransparencyLog;

/// A draft to deposit: the item text and its mandatory primary source, in the
/// structured form the identity layer normalizes for anonymity.
pub struct Draft {
    pub item: Vec<u8>,
    pub primary_source: Vec<u8>,
}

impl Draft {
    pub fn content_id(&self) -> Cid {
        // Length-prefix each field before hashing. Plain concatenation makes the
        // fields ambiguous — `("ab", "c")` and `("a", "bc")` both hash `"abc"` and
        // collide (PROTO-011); the length prefixes make the boundary unambiguous.
        let mut buf = Vec::with_capacity(16 + self.item.len() + self.primary_source.len());
        buf.extend_from_slice(&(self.item.len() as u64).to_le_bytes());
        buf.extend_from_slice(&self.item);
        buf.extend_from_slice(&(self.primary_source.len() as u64).to_le_bytes());
        buf.extend_from_slice(&self.primary_source);
        cid(&buf)
    }
}

/// Records the draft on the log and returns its content id. The primary source is
/// mandatory: a draft without one is not depositable.
pub fn deposit(log: &mut TransparencyLog, draft: &Draft) -> Result<Cid, NoPrimarySource> {
    if draft.primary_source.is_empty() {
        return Err(NoPrimarySource);
    }
    let id = draft.content_id();
    log.append(id);
    Ok(id)
}

#[derive(Debug, PartialEq, Eq)]
pub struct NoPrimarySource;
