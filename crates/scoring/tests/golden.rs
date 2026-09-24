//! Golden outputs (invariant #7, `docs/08` REPRO-002, T41). `reproducibility.rs` checks
//! that two runs agree with each other; this pins what they agree *on*. Every value the
//! engine returns on the fixture datasets is compared bit-for-bit against
//! `fixtures/golden_bits.txt`, so any silent change — a different initialization, a
//! changed RNG consumption order, a reordered sum — fails here even when the result
//! still lands inside an oracle tolerance. The file is Rust output, not a sim oracle,
//! hence `.txt`: `fixture_drift` regenerates every `.csv` fixture from the sims.
//!
//! An intended change regenerates the file, and the diff shows exactly which values
//! moved: `ISEGORIA_UPDATE_GOLDEN=1 cargo test -p scoring --test golden`.

use scoring::bridging::{bridge_scores, fit, BridgingParams, Ratings};
use scoring::dif::mixture_dif;
use std::fs;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn read_matrix(name: &str) -> Vec<Vec<f64>> {
    fs::read_to_string(fixtures_dir().join(name))
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.split(',').map(|c| c.trim().parse().unwrap()).collect())
        .collect()
}

fn read_vector(name: &str) -> Vec<f64> {
    read_matrix(name).into_iter().map(|r| r[0]).collect()
}

/// FNV-1a over the IEEE-754 bits: one line for a long vector.
fn digest(v: &[f64]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for x in v {
        for b in x.to_bits().to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    h
}

/// `name,index,bits` rows: short vectors value by value, long ones as a digest.
fn record(rows: &mut Vec<String>, name: &str, v: &[f64]) {
    if v.len() <= 16 {
        for (i, x) in v.iter().enumerate() {
            rows.push(format!("{name},{i},{:016x}", x.to_bits()));
        }
    } else {
        rows.push(format!("{name},digest,{:016x}", digest(v)));
    }
}

fn current() -> Vec<String> {
    let mut rows = Vec::new();

    let r = read_matrix("R.csv");
    let mask: Vec<Vec<bool>> = read_matrix("mask.csv")
        .iter()
        .map(|row| row.iter().map(|&v| v != 0.0).collect())
        .collect();
    let data = Ratings::from_dense(&r, &mask);
    let p = BridgingParams::default();
    let f = fit(&data, &p).unwrap();
    record(&mut rows, "fit.mu", &[f.mu]);
    record(&mut rows, "fit.b_j", &f.b_j);
    record(&mut rows, "fit.f_j", &f.f_j);
    record(&mut rows, "fit.b_u", &f.b_u);
    record(&mut rows, "fit.f_u", &f.f_u);
    record(
        &mut rows,
        "bridge_scores",
        &bridge_scores(&data, &p, 10, 0.85).unwrap(),
    );

    for set in ["batch", "single"] {
        let theta = read_vector(&format!("mixture_{set}_theta.csv"));
        let x = read_matrix(&format!("mixture_{set}_X.csv"));
        let res = mixture_dif(&theta, &x, 8, 0);
        let tag = format!("mixture_{set}");
        let shape = [res.classes as f64, res.non_uniform as i32 as f64];
        record(&mut rows, &format!("{tag}.model"), &shape);
        record(&mut rows, &format!("{tag}.pi"), &res.pi);
        record(&mut rows, &format!("{tag}.dif"), &res.dif);
        record(&mut rows, &format!("{tag}.a_gap"), &res.a_gap);
        record(&mut rows, &format!("{tag}.bic_gain"), &[res.bic_gain]);
        record(
            &mut rows,
            &format!("{tag}.posterior"),
            &res.posterior.concat(),
        );
    }
    rows
}

#[test]
fn engine_outputs_match_the_golden_bits() {
    let path = fixtures_dir().join("golden_bits.txt");
    let now = current();
    if std::env::var_os("ISEGORIA_UPDATE_GOLDEN").is_some() {
        fs::write(&path, now.join("\n") + "\n").unwrap();
        return;
    }
    let want: Vec<String> = fs::read_to_string(&path)
        .expect("golden_bits.txt missing: regenerate with ISEGORIA_UPDATE_GOLDEN=1")
        .lines()
        .map(str::to_owned)
        .collect();
    let moved: Vec<String> = want
        .iter()
        .zip(&now)
        .filter(|(w, n)| w != n)
        .map(|(w, n)| format!("  want {w}\n  got  {n}"))
        .collect();
    assert!(
        moved.is_empty() && want.len() == now.len(),
        "{} of {} golden values moved (or the row count changed: {} vs {}):\n{}",
        moved.len(),
        want.len(),
        want.len(),
        now.len(),
        moved.join("\n")
    );
}
