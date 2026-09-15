//! Append-only transparency log (`docs/04`, §Signed append-only logs): a
//! hash-chained, add-only register. Each entry carries the previous entry's hash,
//! so altering any past entry breaks the chain visibly.

use crate::cid::Cid;
use crate::hash::tagged;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub seq: u64,
    pub prev: [u8; 32],
    pub payload: Cid,
    pub hash: [u8; 32],
}

fn entry_hash(seq: u64, prev: &[u8; 32], payload: &Cid) -> [u8; 32] {
    tagged(
        "isegoria/log/entry",
        &[&seq.to_le_bytes(), prev, &payload.0],
    )
}

#[derive(Default)]
pub struct TransparencyLog {
    entries: Vec<Entry>,
}

impl TransparencyLog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn append(&mut self, payload: Cid) -> &Entry {
        let seq = self.entries.len() as u64;
        let prev = self.head();
        let hash = entry_hash(seq, &prev, &payload);
        self.entries.push(Entry {
            seq,
            prev,
            payload,
            hash,
        });
        self.entries.last().unwrap()
    }

    /// Hash of the last entry (the log head), or zeros for an empty log.
    pub fn head(&self) -> [u8; 32] {
        self.entries.last().map_or([0u8; 32], |e| e.hash)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    #[cfg(test)]
    pub(crate) fn tamper_payload(&mut self, index: usize, payload: Cid) {
        self.entries[index].payload = payload;
    }

    /// Recomputes the whole chain; any tampered entry or broken link fails.
    pub fn verify(&self) -> bool {
        let mut prev = [0u8; 32];
        for (i, e) in self.entries.iter().enumerate() {
            if e.seq != i as u64 || e.prev != prev {
                return false;
            }
            if e.hash != entry_hash(e.seq, &e.prev, &e.payload) {
                return false;
            }
            prev = e.hash;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cid::cid;

    #[test]
    fn tampering_with_a_past_payload_is_detected() {
        let mut log = TransparencyLog::new();
        log.append(cid(b"a"));
        log.append(cid(b"b"));
        log.append(cid(b"c"));
        assert!(log.verify());
        log.tamper_payload(1, cid(b"forged"));
        assert!(!log.verify(), "a rewritten entry must break verification");
    }
}
