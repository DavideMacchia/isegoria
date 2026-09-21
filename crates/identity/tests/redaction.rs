//! PV-4 / docs/08 §8.3: no `Debug` output prints a secret, a uniqueness label, or the
//! personal anchor. These types carry a hand-written, redacting `Debug`; this pins it.

use identity::credential::{Credential, Issuer};
use identity::enrollment::{Anchor, Label};

/// Distinctive inputs. If any byte leaked, a derived `Debug` of a `[u8; 32]` would
/// print it in decimal — 0xAB = "171", 0xCD = "205" — and the anchor string would
/// appear verbatim.
const SECRET: [u8; 32] = [0xAB; 32];
const LABEL_BYTES: [u8; 32] = [0xCD; 32];
const CF: &str = "RSSMRA80A01H501U";

#[test]
fn debug_redacts_the_root_secret() {
    let dbg = format!("{:?}", Credential::from_secret(SECRET));
    assert_eq!(dbg, "Credential { .. }");
    assert!(!dbg.contains("171"), "secret byte leaked: {dbg}");
}

#[test]
fn debug_redacts_the_uniqueness_label() {
    let dbg = format!("{:?}", Label(LABEL_BYTES));
    assert_eq!(dbg, "Label(..)");
    assert!(!dbg.contains("205"), "label byte leaked: {dbg}");
}

#[test]
fn debug_redacts_the_personal_anchor() {
    let dbg = format!("{:?}", Anchor(CF.to_string()));
    assert_eq!(dbg, "Anchor(..)");
    assert!(!dbg.contains(CF), "anchor (codice fiscale) leaked: {dbg}");
    assert!(!dbg.contains("RSSMRA"), "anchor prefix leaked: {dbg}");
}

#[test]
fn debug_redacts_the_secret_across_a_full_issuance() {
    let issuer = Issuer::new([1u8; 32]);
    let holder = Credential::from_secret(SECRET);
    let (req, pending) = holder.request_issuance(&Label(LABEL_BYTES), &issuer.public());

    // Holder-side state carrying the raw committed secret and label scalar.
    assert_eq!(format!("{pending:?}"), "PendingIssuance { .. }");

    // The request embeds the label; its derived `Debug` must inherit the redaction
    // rather than print the raw 32-byte array.
    let req_dbg = format!("{req:?}");
    assert!(
        req_dbg.contains("Label(..)"),
        "request label not redacted: {req_dbg}"
    );
    assert!(
        !req_dbg.contains("Label(["),
        "request label printed raw: {req_dbg}"
    );

    // The finished credential signs (secret, label); redact the whole struct.
    let blind = issuer.issue(&req).expect("a well-formed request issues");
    let anon = pending.finalize(blind);
    assert_eq!(format!("{anon:?}"), "AnonymousCredential { .. }");
}
