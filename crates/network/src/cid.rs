//! Content addressing (`docs/04`, §Content-addressed storage): the address is the
//! hash of the content, so it guarantees the content — no one can swap a question's
//! text while keeping its reference.

use crate::hash::tagged;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Cid(pub [u8; 32]);

pub fn cid(bytes: &[u8]) -> Cid {
    Cid(tagged("isegoria/cid/v1", &[bytes]))
}
