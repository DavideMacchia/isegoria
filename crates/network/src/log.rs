//! Append-only transparency log (`docs/04` §Signed append-only logs, `docs/08` NET-004):
//! a hash chain. [`verify_extends`](TransparencyLog::verify_extends) proves it extends an
//! externally held, signed prior [`Checkpoint`] — catching what a bare chain misses.

use crate::cid::Cid;
use crate::consortium::Checkpoint;
use crate::hash::tagged;
use std::collections::HashSet;

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
    /// Every payload appended so far: lets an entry point refuse a duplicate first
    /// (`docs/08` §9.1 row 1).
    payloads: HashSet<Cid>,
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
        self.payloads.insert(payload);
        self.entries.last().unwrap()
    }

    /// Whether `payload` is already on the log: checked first, before identity and quota.
    pub fn contains(&self, payload: &Cid) -> bool {
        self.payloads.contains(payload)
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

    /// The log's current state as a `Checkpoint` bound to `network_id` and `member_set_hash`
    /// — the object the consortium signs. A signature over the head commits the whole prefix.
    pub fn checkpoint(&self, network_id: [u8; 32], member_set_hash: [u8; 32]) -> Checkpoint {
        Checkpoint::new(
            network_id,
            member_set_hash,
            self.entries.len() as u64,
            self.head(),
        )
    }

    /// Proves the log consistently extends `prior`: the chain must verify, reach at least
    /// `prior.height`, and its entry there must still chain to `prior.head` (NET-004,
    /// AT-NET-01).
    pub fn verify_extends(&self, prior: &Checkpoint) -> Result<(), ConsistencyError> {
        if !self.verify() {
            return Err(ConsistencyError::BrokenChain);
        }
        let need = prior.height as usize;
        if self.entries.len() < need {
            return Err(ConsistencyError::Truncated {
                have: self.entries.len(),
                need,
            });
        }
        let prefix_head = if need == 0 {
            [0u8; 32]
        } else {
            self.entries[need - 1].hash
        };
        if prefix_head != prior.head {
            return Err(ConsistencyError::ForkedHistory);
        }
        Ok(())
    }
}

/// Why a log does not consistently extend a trusted prior checkpoint (NET-004).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsistencyError {
    /// The hash chain itself is broken (a tampered entry that was not re-chained).
    BrokenChain,
    /// The log is shorter than the checkpoint's height — a truncation.
    Truncated { have: usize, need: usize },
    /// The chain verifies but the prefix head differs — a consistent rewrite / fork of
    /// already-checkpointed history.
    ForkedHistory,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cid::cid;

    #[test]
    fn contains_reports_exactly_the_appended_payloads() {
        let mut log = TransparencyLog::new();
        assert!(!log.contains(&cid(b"a")));
        log.append(cid(b"a"));
        log.append(cid(b"b"));
        assert!(log.contains(&cid(b"a")));
        assert!(log.contains(&cid(b"b")));
        assert!(!log.contains(&cid(b"c")));
    }

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

    #[test]
    fn verify_extends_rejects_a_broken_chain() {
        let prior = {
            let mut l = TransparencyLog::new();
            l.append(cid(b"a"));
            l.append(cid(b"b"));
            l.checkpoint([0u8; 32], [0u8; 32])
        };
        let mut log = TransparencyLog::new();
        log.append(cid(b"a"));
        log.append(cid(b"b"));
        log.append(cid(b"c"));
        log.tamper_payload(1, cid(b"forged")); // payload edited, hashes not re-chained
        assert_eq!(
            log.verify_extends(&prior),
            Err(ConsistencyError::BrokenChain)
        );
    }
}
