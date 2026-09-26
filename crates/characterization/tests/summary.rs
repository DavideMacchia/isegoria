//! The summaries compute what `docs/13` §5 defines, checked on hand-built records, and every
//! study of the smoke grid runs and is summarized (`docs/13` §2).

use characterization::grid::{tasks, Cell, DifDesign, Grid, Layout, Study, STUDIES};
use characterization::record::{header, Record};
use characterization::run::{DifOutcome, Outcome};
use characterization::runner::{execute, Options};
use characterization::stats::{quantile, wilson};
use characterization::summary::summarize;
use std::fs;
use std::path::{Path, PathBuf};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("characterization-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 5e-4
}

/// The Wilson interval and the type-7 quantile match hand-computed values.
#[test]
fn the_interval_and_the_quantile_match_hand_computed_values() {
    let (lo, hi) = wilson(0, 10);
    assert!(close(lo, 0.0) && close(hi, 0.2775), "{lo} {hi}");
    let (lo, hi) = wilson(5, 10);
    assert!(close(lo, 0.2366) && close(hi, 0.7634), "{lo} {hi}");
    let (lo, hi) = wilson(2, 3);
    assert!(close(lo, 0.2077) && close(hi, 0.9385), "{lo} {hi}");
    assert_eq!(wilson(0, 0), (0.0, 1.0));
    assert_eq!(quantile(&[1.0, 2.0, 3.0, 4.0], 0.5), 2.5);
    assert_eq!(quantile(&[1.0, 2.0, 3.0, 4.0], 0.25), 1.75);
    assert!(close(quantile(&[0.0, 1.2, 1.5], 0.95), 1.47));
}

fn outcome(
    flags: [bool; 4],
    dif: [f64; 4],
    classes: usize,
    admitted: bool,
    roles: &str,
) -> Outcome {
    Outcome::Dif(DifOutcome {
        kr20: if admitted { 0.91 } else { 0.88 },
        admitted,
        classes,
        non_uniform: false,
        converged: true,
        bic_gain: 1.0,
        pi: vec![1.0],
        eta: vec![0.0],
        dif: dif.to_vec(),
        a_gap: vec![0.0; 4],
        flags: flags.to_vec(),
        roles: roles.to_string(),
    })
}

fn write(out: &Path, study: Study, cell: &str, outcomes: Vec<Outcome>) {
    let dir = out.join(study.name());
    fs::create_dir_all(&dir).unwrap();
    let mut text = format!("{}\n", header(study));
    for (i, outcome) in outcomes.into_iter().enumerate() {
        let record = Record {
            study,
            cell: cell.to_string(),
            replicate: i as u32,
            seed: i as u64,
            elapsed_ms: 1000 * (i as u64 + 1),
            outcome,
        };
        text.push_str(&record.line());
        text.push('\n');
    }
    fs::write(dir.join("records.csv"), text).unwrap();
}

/// A row of a study's summary table, by column name.
fn row(out: &Path, study: Study) -> Vec<(String, String)> {
    let text = fs::read_to_string(out.join(study.name()).join("summary.csv")).unwrap();
    let mut lines = text.lines();
    let names: Vec<String> = lines.next().unwrap().split(',').map(String::from).collect();
    let values: Vec<String> = lines.next().unwrap().split(',').map(String::from).collect();
    names.into_iter().zip(values).collect()
}

fn value(row: &[(String, String)], name: &str) -> f64 {
    let (_, v) = row
        .iter()
        .find(|(n, _)| n == name)
        .unwrap_or_else(|| panic!("no {name}"));
    v.parse().unwrap()
}

/// Null runs: the batch and item false-positive rates, the admitted view and the gaps.
#[test]
fn the_null_summary_counts_false_positives_per_batch_and_per_item() {
    let out = scratch("null-summary");
    let cell = Cell::Dif(DifDesign {
        k: 4,
        ..DifDesign::default()
    })
    .key();
    let f = false;
    write(
        &out,
        Study::DifNull,
        &cell,
        vec![
            outcome([f, f, f, f], [0.0; 4], 1, true, "cccc"),
            outcome([true, f, f, f], [1.2, 0.1, 0.2, 0.3], 2, true, "cccc"),
            outcome([true, true, f, f], [1.5, 1.1, 0.4, 0.2], 2, false, "cccc"),
        ],
    );
    let markdown = summarize(&out).unwrap();
    assert!(markdown.contains("dif-null"));
    let r = row(&out, Study::DifNull);
    assert_eq!(value(&r, "runs"), 3.0);
    assert!(close(value(&r, "admitted"), 2.0 / 3.0));
    assert!(close(value(&r, "mixtures"), 2.0 / 3.0));
    assert!(close(value(&r, "batch_fp"), 2.0 / 3.0));
    assert!(close(value(&r, "batch_fp_lo"), 0.2077) && close(value(&r, "batch_fp_hi"), 0.9385));
    assert!(close(value(&r, "clean_fp"), 0.25));
    assert!(close(value(&r, "clean_fp_lo"), 0.0764) && close(value(&r, "clean_fp_hi"), 0.5731));
    assert!(close(value(&r, "admitted_batch_fp"), 0.5));
    assert!(close(value(&r, "admitted_batch_fp_lo"), 0.0945));
    assert!(close(value(&r, "max_clean_gap_median"), 1.2));
    assert!(close(value(&r, "max_clean_gap_p95"), 1.47));
    assert!(close(value(&r, "fit_seconds"), 2.0));
    let _ = fs::remove_dir_all(out);
}

/// Biased runs: item power, all biased items flagged, clean items flagged, the biased gap.
#[test]
fn the_power_summary_counts_detections_per_item_and_per_batch() {
    let out = scratch("power-summary");
    let cell = Cell::Dif(DifDesign {
        k: 4,
        layout: Layout::Campaign(2),
        delta: 0.9,
        ..DifDesign::default()
    })
    .key();
    let (t, f) = (true, false);
    write(
        &out,
        Study::DifPower,
        &cell,
        vec![
            outcome([t, t, f, f], [1.8, 1.6, 0.1, 0.1], 2, true, "++cc"),
            outcome([t, f, f, t], [1.4, 0.8, 0.2, 1.1], 2, true, "++cc"),
            outcome([f, f, f, f], [0.0; 4], 1, true, "++cc"),
        ],
    );
    summarize(&out).unwrap();
    let r = row(&out, Study::DifPower);
    assert!(close(value(&r, "power"), 0.5));
    assert!(close(value(&r, "all_biased"), 1.0 / 3.0));
    assert!(close(value(&r, "clean_fp"), 1.0 / 6.0));
    assert!(close(
        value(&r, "biased_gap_mean"),
        (1.8 + 1.6 + 1.4 + 0.8) / 6.0
    ));
    assert!(close(value(&r, "true_gap"), 1.8));
    let _ = fs::remove_dir_all(out);
}

/// Every study of the smoke grid runs end to end and has a section in the summary.
#[test]
fn every_study_of_the_smoke_grid_runs_and_is_summarized() {
    let out = scratch("smoke");
    let all = tasks(&STUDIES, Grid::Smoke, Some(1), None);
    let report = execute(
        &all,
        &Options {
            jobs: 4,
            out: out.clone(),
            progress: false,
        },
    )
    .unwrap();
    assert_eq!((report.ran, report.failed), (all.len(), 0));
    let markdown = summarize(&out).unwrap();
    for study in STUDIES {
        assert!(
            markdown.contains(&format!("## {}", study.name())),
            "{}",
            study.name()
        );
        assert!(
            out.join(study.name()).join("summary.csv").exists(),
            "{}",
            study.name()
        );
    }
    assert!(out.join("summary.md").exists());
    let _ = fs::remove_dir_all(out);
}
