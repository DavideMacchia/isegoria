//! The identity-layer invariants (`docs/03`): uniqueness (P1), non-rotatability
//! and no whitewashing (P2, M3), unlinkability (P3), cross-source dedup (M1), and
//! the rate-limiting nullifier.

use identity::credential::{BlindIssuer, Credential, ReferenceIssuer};
use identity::enrollment::{
    Cie, DuplicateEnrollment, EnrollmentRegistry, Label, Spid, VoprfOracle,
};
use identity::nym::Role;
use identity::ratelimit::{rln_token, within_quota, DoubleSpend, SlotLedger};

fn oracle() -> VoprfOracle {
    VoprfOracle::new([7u8; 32])
}

#[test]
fn role_nyms_are_deterministic_and_unlinkable() {
    let cred = Credential::from_secret([42u8; 32]);

    // Deterministic: same role → same nym (P2, no fresh pseudonym per role).
    assert_eq!(cred.nym(Role::Judge), cred.nym(Role::Judge));

    // Unlinkable: the three role nyms differ (M3, P3).
    assert_ne!(cred.nym(Role::Propose), cred.nym(Role::Judge));
    assert_ne!(cred.nym(Role::Judge), cred.nym(Role::Respond));
    assert_ne!(cred.nym(Role::Propose), cred.nym(Role::Respond));
}

#[test]
fn different_people_get_different_nyms() {
    let a = Credential::from_secret([1u8; 32]);
    let b = Credential::from_secret([2u8; 32]);
    assert_ne!(a.nym(Role::Judge), b.nym(Role::Judge));
}

#[test]
fn nym_does_not_leak_the_secret() {
    // The nym is a 32-byte hash; it must not equal the secret it derives from.
    let secret = [9u8; 32];
    let cred = Credential::from_secret(secret);
    assert_ne!(cred.nym(Role::Propose).0, secret);
}

#[test]
fn second_enrollment_same_person_is_rejected() {
    let mut reg = EnrollmentRegistry::new();
    let o = oracle();

    let first = reg.enroll(
        &Cie {
            codice_fiscale: "RSSMRA80A01H501U".into(),
        },
        &o,
    );
    assert!(first.is_ok());

    // Same person via a different source → same label → duplicate (M1, P1).
    let again = reg.enroll(
        &Spid {
            codice_fiscale: "rssmra80a01h501u".into(),
        },
        &o,
    );
    assert_eq!(again, Err(DuplicateEnrollment));
}

#[test]
fn distinct_people_enroll_independently() {
    let mut reg = EnrollmentRegistry::new();
    let o = oracle();
    assert!(reg
        .enroll(
            &Cie {
                codice_fiscale: "AAABBB00A00A000A".into()
            },
            &o
        )
        .is_ok());
    assert!(reg
        .enroll(
            &Spid {
                codice_fiscale: "CCCDDD11B11B111B".into()
            },
            &o
        )
        .is_ok());
}

#[test]
fn cie_and_spid_of_the_same_person_map_to_one_label() {
    let o = oracle();
    let mut reg = EnrollmentRegistry::new();
    let label = reg
        .enroll(
            &Cie {
                codice_fiscale: "ZZZ".into(),
            },
            &o,
        )
        .unwrap();
    assert!(reg.is_enrolled(&label));
    // The SPID label of the same CF is the same label, hence already enrolled.
    let mut reg2 = EnrollmentRegistry::new();
    let via_spid = reg2
        .enroll(
            &Spid {
                codice_fiscale: "ZZZ".into(),
            },
            &o,
        )
        .unwrap();
    assert_eq!(label, via_spid);
}

#[test]
fn rate_limit_tokens_are_stable_and_reuse_is_caught() {
    let secret = [5u8; 32];
    let mut ledger = SlotLedger::new();

    // Spend the quota (slots 0..3).
    for slot in 0..3u32 {
        assert!(within_quota(slot, 3));
        let t = rln_token(&secret, Role::Propose, 10, slot);
        assert!(ledger.spend(t).is_ok());
    }

    // Reusing a slot yields the same token → double-spend detected.
    let reused = rln_token(&secret, Role::Propose, 10, 1);
    assert_eq!(ledger.spend(reused), Err(DoubleSpend));

    // The 4th slot is outside the quota.
    assert!(!within_quota(3, 3));
}

#[test]
fn rate_limit_tokens_differ_across_epoch_role_and_person() {
    let s1 = [1u8; 32];
    let s2 = [2u8; 32];
    assert_ne!(
        rln_token(&s1, Role::Propose, 1, 0),
        rln_token(&s1, Role::Propose, 2, 0)
    );
    assert_ne!(
        rln_token(&s1, Role::Propose, 1, 0),
        rln_token(&s1, Role::Judge, 1, 0)
    );
    assert_ne!(
        rln_token(&s1, Role::Propose, 1, 0),
        rln_token(&s2, Role::Propose, 1, 0)
    );
}

#[test]
fn reference_issuer_is_deterministic() {
    let issuer = ReferenceIssuer;
    let label = Label([3u8; 32]);
    let cred = Credential::from_secret([4u8; 32]);
    assert_eq!(issuer.issue(&label, &cred), issuer.issue(&label, &cred));
}
