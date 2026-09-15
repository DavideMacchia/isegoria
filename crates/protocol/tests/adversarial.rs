//! Adversarial scenario (`docs/06`): whitewashing. A node with a ruined reputation
//! tries to shed it by starting over. It cannot: the role pseudonym is deterministic,
//! so a re-derivation yields the same id (reputation follows it), and a second
//! enrollment of the same person is refused, so no fresh credential is available.

use identity::credential::Credential;
use identity::enrollment::{Cie, DuplicateEnrollment, EnrollmentRegistry, Spid, VoprfOracle};
use identity::nym::{Nym, Role};
use std::collections::HashMap;

#[test]
fn whitewashing_cannot_shed_a_bad_reputation() {
    // Reputation is keyed by the deterministic role pseudonym.
    let cred = Credential::from_secret([9u8; 32]);
    let judge = cred.nym(Role::Judge);
    let mut reputation: HashMap<Nym, f64> = HashMap::new();
    reputation.insert(judge, 0.05); // ruined

    // "Restarting" re-derives the very same judge pseudonym: the bad reputation follows.
    let restarted = Credential::from_secret([9u8; 32]).nym(Role::Judge);
    assert_eq!(restarted, judge, "the role pseudonym is not rotatable");
    assert_eq!(
        reputation.get(&restarted),
        Some(&0.05),
        "reputation followed the node"
    );

    // A fresh pseudonym would need a fresh credential, but the same person cannot
    // enroll a second time — the uniqueness label is the same regardless of source.
    let oracle = VoprfOracle::new([1u8; 32]);
    let mut registry = EnrollmentRegistry::new();
    registry
        .enroll(
            &Cie {
                codice_fiscale: "RSSMRA80A01H501U".into(),
            },
            &oracle,
        )
        .expect("first enrollment");
    assert_eq!(
        registry.enroll(
            &Spid {
                codice_fiscale: "RSSMRA80A01H501U".into()
            },
            &oracle
        ),
        Err(DuplicateEnrollment),
        "no second credential, so no fresh identity to whitewash into"
    );
}
