//! Guard against drift between the Python sims and the committed oracle fixtures:
//! regenerate and compare (catches editing a sim without regenerating, or vice versa).
//! Needs `python3` + numpy/scipy (`sim/requirements.txt`, `fixtures/PROVENANCE.md`);
//! self-skips when that environment is absent so `cargo test` stays green without it.

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

/// Whether `python3` (or the repo `.venv`) can import numpy and scipy.
fn sim_env_available() -> bool {
    Command::new(python())
        .args(["-c", "import numpy, scipy"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn committed_fixtures_match_the_sims() {
    if !sim_env_available() {
        eprintln!(
            "SKIP fixture_drift: python3 with numpy/scipy is not available. Install \
             `sim/requirements.txt` (see fixtures/PROVENANCE.md) to run the drift guard."
        );
        return;
    }

    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("fixtures_regen");
    fs::create_dir_all(&out).unwrap();
    let script = repo_root().join("sim/export_fixtures.py");

    let status = Command::new(python())
        .arg(&script)
        .arg(&out)
        .status()
        .expect("could not run export_fixtures.py — is numpy/scipy installed? see sim/");
    assert!(status.success(), "export_fixtures.py exited with failure");

    // Compare only the CSV oracle files (PROVENANCE.md is docs, not sim output).
    for entry in fs::read_dir(committed_dir()).unwrap() {
        let name = entry.unwrap().file_name();
        if Path::new(&name).extension().and_then(|e| e.to_str()) != Some("csv") {
            continue;
        }
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

    // Files holding optimizer outputs (`*expected*`: the bridging b_j, the logistic
    // fits) vary by ~1e-3 across platforms and scipy releases (docs/08 REPRO-003); the
    // raw seeded data files are exact.
    let tol = if name.contains("expected") {
        2e-3
    } else {
        1e-4
    };
    for (i, (sa, sb)) in xa.iter().zip(xb.iter()).enumerate() {
        match (sa.parse::<f64>(), sb.parse::<f64>()) {
            (Ok(fa), Ok(fb)) => assert!(
                (fa - fb).abs() <= tol + tol * fa.abs(),
                "{name} token {i}: {fa} vs {fb} (sim/fixture drift)"
            ),
            _ => assert_eq!(sa, sb, "{name} token {i}"),
        }
    }
}
