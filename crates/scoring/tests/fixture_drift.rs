//! Guard against drift between the Python sims and the committed oracle fixtures:
//! regenerate the fixtures and compare them to what is checked in. Ignored by default
//! because it needs numpy/scipy; run with:
//!
//!   cargo test -p scoring --test fixture_drift -- --ignored
//!
//! catches editing a sim without regenerating fixtures, or vice versa.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn committed_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn python() -> String {
    let venv = repo_root().join(".venv/bin/python");
    if venv.exists() {
        venv.to_string_lossy().into_owned()
    } else {
        "python3".into()
    }
}

#[test]
#[ignore = "needs numpy/scipy; run with --ignored"]
fn committed_fixtures_match_the_sims() {
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("fixtures_regen");
    fs::create_dir_all(&out).unwrap();
    let script = repo_root().join("sim/export_fixtures.py");

    let status = Command::new(python())
        .arg(&script)
        .arg(&out)
        .status()
        .expect("could not run export_fixtures.py — is numpy/scipy installed? see sim/");
    assert!(status.success(), "export_fixtures.py exited with failure");

    for entry in fs::read_dir(committed_dir()).unwrap() {
        let name = entry.unwrap().file_name();
        let regen = out.join(&name);
        assert!(regen.exists(), "the sims no longer produce {name:?}");
        compare_csv(
            &committed_dir().join(&name),
            &regen,
            &name.to_string_lossy(),
        );
    }
}

/// Compares two CSVs token-by-token: numbers within tolerance, everything else exact.
/// The tolerance absorbs harmless formatting noise but not a real change in the data.
fn compare_csv(a: &Path, b: &Path, name: &str) {
    let tokens = |t: &str| -> Vec<String> {
        t.split([',', '\n'])
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    };
    let xa = tokens(&fs::read_to_string(a).unwrap());
    let xb = tokens(&fs::read_to_string(b).unwrap());
    assert_eq!(xa.len(), xb.len(), "{name}: token count changed");

    for (i, (sa, sb)) in xa.iter().zip(xb.iter()).enumerate() {
        match (sa.parse::<f64>(), sb.parse::<f64>()) {
            (Ok(fa), Ok(fb)) => assert!(
                (fa - fb).abs() <= 1e-4 + 1e-4 * fa.abs(),
                "{name} token {i}: {fa} vs {fb} (sim/fixture drift)"
            ),
            _ => assert_eq!(sa, sb, "{name} token {i}"),
        }
    }
}
