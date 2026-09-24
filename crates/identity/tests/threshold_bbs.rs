//! Threshold BBS+ issuance (`docs/03` §M2): a `t`-of-`n` committee blind-signs the
//! credential via MPC, and the aggregate is an ordinary BBS+ signature the holder
//! unblinds and verifies — exactly as with the single issuer, but no `t-1` members and
//! no single party can produce it. The holder side is unchanged.

use identity::credential::{Credential, ThresholdIssuer};
use identity::enrollment::Label;

fn label(byte: u8) -> Label {
    Label([byte; 32])
}

#[test]
fn threshold_issued_credential_verifies() {
    let issuer = ThresholdIssuer::new([1u8; 32], 4, 3);
    let holder = Credential::from_secret([9u8; 32]);

    let (request, pending) = holder.request_issuance(&label(7), &issuer.public());
    let blind = issuer
        .issue(&request)
        .expect("committee signs a valid request");
    let credential = pending.finalize(blind);

    assert!(credential.verify(&issuer.public()));
}

#[test]
fn a_smaller_quorum_also_works() {
    // t-of-n with a 2-of-3 committee.
    let issuer = ThresholdIssuer::new([5u8; 32], 3, 2);
    let holder = Credential::from_secret([42u8; 32]);

    let (request, pending) = holder.request_issuance(&label(3), &issuer.public());
    let blind = issuer.issue(&request).unwrap();
    assert!(pending.finalize(blind).verify(&issuer.public()));
}

#[test]
fn a_credential_does_not_verify_under_a_different_committee() {
    let issuer = ThresholdIssuer::new([1u8; 32], 4, 3);
    let other = ThresholdIssuer::new([2u8; 32], 4, 3);
    let holder = Credential::from_secret([9u8; 32]);

    let (request, pending) = holder.request_issuance(&label(7), &issuer.public());
    let credential = pending.finalize(issuer.issue(&request).unwrap());

    assert!(credential.verify(&issuer.public()));
    assert!(!credential.verify(&other.public()));
}

mod proptests {
    //! T42: any credential the committee issues verifies, whatever the secret and label.
    use super::*;
    use proptest::prelude::*;
    use std::sync::OnceLock;

    /// Threshold keygen (dealer + base OT) is the slow part; share one committee.
    fn committee() -> &'static ThresholdIssuer {
        static ISSUER: OnceLock<ThresholdIssuer> = OnceLock::new();
        ISSUER.get_or_init(|| ThresholdIssuer::new([3u8; 32], 3, 2))
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(6))]

        #[test]
        fn any_threshold_issued_credential_verifies(
            secret in any::<[u8; 32]>(),
            label in any::<[u8; 32]>(),
        ) {
            let issuer = committee();
            let holder = Credential::from_secret(secret);
            let (request, pending) = holder.request_issuance(&Label(label), &issuer.public());
            let credential = pending.finalize(issuer.issue(&request).unwrap());
            prop_assert!(credential.verify(&issuer.public()));
        }
    }
}
