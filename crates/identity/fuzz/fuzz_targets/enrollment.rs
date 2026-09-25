//! Enrollment of arbitrary codice-fiscale strings through every oracle (`docs/03` §M1):
//! no panic, and the same person via another source is always a duplicate. Run with
//! `-max_len` above 65535 to reach the long-anchor path.
#![no_main]

use identity::enrollment::{
    Cie, DuplicateEnrollment, EnrollmentRegistry, ReferenceOracle, Spid, UniquenessOracle,
    VoprfOracle,
};
use identity::oprf::ThresholdOprfOracle;
use libfuzzer_sys::fuzz_target;
use std::sync::OnceLock;

fn oracles() -> &'static [Box<dyn UniquenessOracle + Send + Sync>; 3] {
    static ORACLES: OnceLock<[Box<dyn UniquenessOracle + Send + Sync>; 3]> = OnceLock::new();
    ORACLES.get_or_init(|| {
        [
            Box::new(ReferenceOracle::new([1u8; 32])),
            Box::new(VoprfOracle::new([2u8; 32])),
            Box::new(ThresholdOprfOracle::new([3u8; 32], 5, 3)),
        ]
    })
}

fuzz_target!(|codice_fiscale: String| {
    for oracle in oracles() {
        let mut registry = EnrollmentRegistry::new();
        let cie = Cie {
            codice_fiscale: codice_fiscale.clone(),
        };
        let label = registry
            .enroll(&cie, oracle.as_ref())
            .expect("a fresh registry enrolls anyone");
        assert!(registry.is_enrolled(&label));
        let spid = Spid {
            codice_fiscale: codice_fiscale.clone(),
        };
        assert_eq!(
            registry.enroll(&spid, oracle.as_ref()),
            Err(DuplicateEnrollment)
        );
    }
});
