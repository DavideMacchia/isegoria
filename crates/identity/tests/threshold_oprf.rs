//! The threshold OPRF (`docs/03` §M1) as a drop-in `UniquenessOracle`: a `t`-of-`n`
//! committee produces the label and cross-source duplicates are still rejected. The
//! threshold cryptography itself is covered by unit tests in `src/oprf.rs`.

use identity::enrollment::{Cie, DuplicateEnrollment, EnrollmentRegistry, Spid};
use identity::oprf::ThresholdOprfOracle;

#[test]
fn threshold_committee_deduplicates_across_sources() {
    let oracle = ThresholdOprfOracle::new([7u8; 32], 5, 3);
    let mut registry = EnrollmentRegistry::new();

    let first = registry.enroll(
        &Cie {
            codice_fiscale: "RSSMRA80A01H501U".into(),
        },
        &oracle,
    );
    assert!(first.is_ok());

    // The same person via a different source yields the same committee-computed label.
    let again = registry.enroll(
        &Spid {
            codice_fiscale: "rssmra80a01h501u".into(),
        },
        &oracle,
    );
    assert_eq!(again, Err(DuplicateEnrollment));
}

#[test]
fn distinct_people_get_distinct_labels() {
    let oracle = ThresholdOprfOracle::new([7u8; 32], 4, 2);
    let mut registry = EnrollmentRegistry::new();
    assert!(registry
        .enroll(
            &Cie {
                codice_fiscale: "AAABBB00A00A000A".into()
            },
            &oracle
        )
        .is_ok());
    assert!(registry
        .enroll(
            &Spid {
                codice_fiscale: "CCCDDD11B11B111B".into()
            },
            &oracle
        )
        .is_ok());
}
