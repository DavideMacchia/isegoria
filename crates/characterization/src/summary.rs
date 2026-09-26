//! The summaries of the records (`docs/13` §5): a table per study and the threshold tables
//! T25 reads, written as `summary.md` and CSV files beside the records.

use crate::grid::{cells, Cell, DifDesign, Grid, Kind, Study, STUDIES};
use crate::record::{read, Record};
use crate::run::{CaptureOutcome, DifOutcome, DtfOutcome, Outcome, SweepOutcome, DTF_SETS};
use crate::stats::{clustered_rate, mean, quantile, sd, sorted, wilson};
use protocol::gate::TAU;
use scoring::dif::MIXTURE_DIF_MAX;
use scoring::dtf::DTF_MAX;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

type Row = Vec<String>;

fn v(x: f64) -> String {
    x.to_string()
}

fn pct(x: f64) -> String {
    if x.is_nan() {
        "—".into()
    } else {
        format!("{:.1}%", 100.0 * x)
    }
}

fn pct_ci((r, lo, hi): (f64, f64, f64)) -> String {
    if r.is_nan() {
        "—".into()
    } else {
        format!("{:.1}% [{:.1}, {:.1}]", 100.0 * r, 100.0 * lo, 100.0 * hi)
    }
}

fn num(x: f64, digits: usize) -> String {
    if x.is_nan() {
        "—".into()
    } else if x.is_infinite() {
        "never".into()
    } else {
        format!("{x:.digits$}")
    }
}

fn rate(x: usize, n: usize) -> (f64, f64, f64) {
    let (lo, hi) = wilson(x, n);
    (x as f64 / n as f64, lo, hi)
}

fn md_table(headers: &[&str], rows: &[Row]) -> String {
    let mut out = format!(
        "| {} |\n|{}\n",
        headers.join(" | "),
        "---|".repeat(headers.len())
    );
    for row in rows {
        out.push_str(&format!("| {} |\n", row.join(" | ")));
    }
    out
}

fn csv(columns: &[&str], rows: &[Row]) -> String {
    let mut out = format!("{}\n", columns.join(","));
    for row in rows {
        out.push_str(&format!("{}\n", row.join(",")));
    }
    out
}

/// A study's records by cell, cells in grid order, then any others by key.
fn grouped(study: Study, records: &[Record]) -> Vec<(String, Vec<&Record>)> {
    let order: BTreeMap<String, usize> = cells(study, Grid::Full)
        .into_iter()
        .chain(cells(study, Grid::Smoke))
        .enumerate()
        .map(|(i, c)| (c.key(), i))
        .collect();
    let mut groups: BTreeMap<(usize, String), Vec<&Record>> = BTreeMap::new();
    for r in records {
        let rank = order.get(&r.cell).copied().unwrap_or(usize::MAX);
        groups.entry((rank, r.cell.clone())).or_default().push(r);
    }
    groups.into_iter().map(|((_, k), rs)| (k, rs)).collect()
}

fn design(study: Study, key: &str) -> Option<DifDesign> {
    match Cell::parse(study, key)? {
        Cell::Dif(d) => Some(d),
        _ => None,
    }
}

fn dif_of(r: &Record) -> Option<&DifOutcome> {
    match &r.outcome {
        Outcome::Dif(o) => Some(o),
        _ => None,
    }
}

fn trusted(o: &DifOutcome) -> bool {
    o.converged && o.classes >= 2
}

/// Per run, `(hits, items)` over the items whose role is in `roles`; None if there is none.
fn count_of(o: &DifOutcome, roles: &str, hit: impl Fn(usize) -> bool) -> Option<(usize, usize)> {
    let items: Vec<usize> = o
        .roles
        .chars()
        .enumerate()
        .filter(|(_, c)| roles.contains(*c))
        .map(|(j, _)| j)
        .collect();
    (!items.is_empty()).then(|| (items.iter().filter(|&&j| hit(j)).count(), items.len()))
}

fn flagged(o: &DifOutcome, roles: &str) -> Option<(usize, usize)> {
    count_of(o, roles, |j| o.flags.get(j).copied().unwrap_or(false))
}

fn values_of(o: &DifOutcome, roles: &str, of: &[f64]) -> Vec<f64> {
    o.roles
        .chars()
        .zip(of)
        .filter(|(c, _)| roles.contains(*c))
        .map(|(_, &x)| x)
        .collect()
}

const DIF_COLUMNS: [&str; 32] = [
    "cell",
    "runs",
    "admitted",
    "kr20",
    "mixtures",
    "classes_mean",
    "non_uniform",
    "power",
    "power_lo",
    "power_hi",
    "power_axis2",
    "all_biased",
    "all_biased_lo",
    "all_biased_hi",
    "clean_fp",
    "clean_fp_lo",
    "clean_fp_hi",
    "target_fp",
    "target_fp_lo",
    "target_fp_hi",
    "batch_fp",
    "batch_fp_lo",
    "batch_fp_hi",
    "admitted_batch_fp",
    "admitted_batch_fp_lo",
    "admitted_batch_fp_hi",
    "biased_gap_mean",
    "true_gap",
    "biased_a_gap_mean",
    "max_clean_gap_median",
    "max_clean_gap_p95",
    "fit_seconds",
];

struct DifCell {
    key: String,
    runs: usize,
    admitted: f64,
    kr20: f64,
    mixtures: f64,
    classes: f64,
    non_uniform: f64,
    power: (f64, f64, f64),
    power2: f64,
    all_biased: (f64, f64, f64),
    clean_fp: (f64, f64, f64),
    target_fp: (f64, f64, f64),
    batch_fp: (f64, f64, f64),
    admitted_batch_fp: (f64, f64, f64),
    biased_gap: f64,
    true_gap: f64,
    biased_a_gap: f64,
    max_clean_gap: (f64, f64),
    seconds: f64,
}

fn dif_cell(study: Study, key: &str, records: &[&Record]) -> DifCell {
    let runs: Vec<&DifOutcome> = records.iter().filter_map(|r| dif_of(r)).collect();
    let n = runs.len();
    let share =
        |f: &dyn Fn(&DifOutcome) -> bool| runs.iter().filter(|o| f(o)).count() as f64 / n as f64;
    let per_run = |roles: &str| -> Vec<(usize, usize)> {
        runs.iter().filter_map(|o| flagged(o, roles)).collect()
    };
    let any_clean = |o: &DifOutcome| flagged(o, "c").is_some_and(|(h, _)| h > 0);
    let with_biased: Vec<&&DifOutcome> = runs
        .iter()
        .filter(|o| o.roles.chars().any(|c| "+-2".contains(c)))
        .collect();
    let all_flagged = with_biased
        .iter()
        .filter(|o| flagged(o, "+-2").is_some_and(|(h, n)| h == n))
        .count();
    let admitted: Vec<&&DifOutcome> = runs.iter().filter(|o| o.admitted).collect();
    let max_clean: Vec<f64> = sorted(
        runs.iter()
            .map(|o| values_of(o, "c", &o.dif).into_iter().fold(0.0, f64::max))
            .collect(),
    );
    let biased_gaps: Vec<f64> = runs
        .iter()
        .flat_map(|o| values_of(o, "+-2", &o.dif))
        .collect();
    let biased_a_gaps: Vec<f64> = runs
        .iter()
        .flat_map(|o| values_of(o, "+-2", &o.a_gap))
        .collect();
    let seconds: Vec<f64> = records
        .iter()
        .map(|r| r.elapsed_ms as f64 / 1000.0)
        .collect();
    DifCell {
        key: key.to_string(),
        runs: n,
        admitted: share(&|o| o.admitted),
        kr20: mean(&runs.iter().map(|o| o.kr20).collect::<Vec<_>>()),
        mixtures: share(&|o| o.classes >= 2),
        classes: mean(&runs.iter().map(|o| o.classes as f64).collect::<Vec<_>>()),
        non_uniform: share(&|o| o.non_uniform),
        power: clustered_rate(&per_run("+-")),
        power2: clustered_rate(&per_run("2")).0,
        all_biased: rate(all_flagged, with_biased.len()),
        clean_fp: clustered_rate(&per_run("c")),
        target_fp: clustered_rate(&per_run("t")),
        batch_fp: rate(runs.iter().filter(|o| any_clean(o)).count(), n),
        admitted_batch_fp: rate(
            admitted.iter().filter(|o| any_clean(o)).count(),
            admitted.len(),
        ),
        biased_gap: mean(&biased_gaps),
        true_gap: design(study, key).map_or(f64::NAN, |d| 2.0 * d.delta),
        biased_a_gap: mean(&biased_a_gaps),
        max_clean_gap: (quantile(&max_clean, 0.5), quantile(&max_clean, 0.95)),
        seconds: mean(&seconds),
    }
}

fn triple((r, lo, hi): (f64, f64, f64)) -> [String; 3] {
    [v(r), v(lo), v(hi)]
}

impl DifCell {
    fn csv_row(&self) -> Row {
        let mut row = vec![
            self.key.clone(),
            self.runs.to_string(),
            v(self.admitted),
            v(self.kr20),
            v(self.mixtures),
            v(self.classes),
            v(self.non_uniform),
        ];
        row.extend(triple(self.power));
        row.push(v(self.power2));
        for r in [
            self.all_biased,
            self.clean_fp,
            self.target_fp,
            self.batch_fp,
            self.admitted_batch_fp,
        ] {
            row.extend(triple(r));
        }
        row.extend([
            v(self.biased_gap),
            v(self.true_gap),
            v(self.biased_a_gap),
            v(self.max_clean_gap.0),
            v(self.max_clean_gap.1),
            v(self.seconds),
        ]);
        row
    }
}

fn dif_markdown(study: Study, table: &[DifCell]) -> String {
    let (headers, pick): (Vec<&str>, fn(&DifCell) -> Row) = match study {
        Study::DifNull => (
            vec![
                "cell",
                "runs",
                "admitted",
                "KR-20",
                "mixture",
                "clean items flagged",
                "batches with a flag",
                "admitted batches with a flag",
                "max clean gap p95",
                "fit s",
            ],
            |c| {
                vec![
                    c.key.clone(),
                    c.runs.to_string(),
                    pct(c.admitted),
                    num(c.kr20, 3),
                    pct(c.mixtures),
                    pct_ci(c.clean_fp),
                    pct_ci(c.batch_fp),
                    pct_ci(c.admitted_batch_fp),
                    num(c.max_clean_gap.1, 2),
                    num(c.seconds, 1),
                ]
            },
        ),
        Study::DifNonuniform => (
            vec![
                "cell",
                "runs",
                "mixture",
                "non-uniform",
                "power (b-gap)",
                "biased a-gap",
                "biased b-gap (true)",
                "clean items flagged",
                "fit s",
            ],
            |c| {
                vec![
                    c.key.clone(),
                    c.runs.to_string(),
                    pct(c.mixtures),
                    pct(c.non_uniform),
                    pct_ci(c.power),
                    num(c.biased_a_gap, 2),
                    format!("{} ({})", num(c.biased_gap, 2), num(c.true_gap, 2)),
                    pct_ci(c.clean_fp),
                    num(c.seconds, 1),
                ]
            },
        ),
        Study::DifTwoAxes => (
            vec![
                "cell",
                "runs",
                "classes",
                "power, axis 1",
                "power, axis 2",
                "all leaners",
                "clean items flagged",
                "fit s",
            ],
            |c| {
                vec![
                    c.key.clone(),
                    c.runs.to_string(),
                    num(c.classes, 2),
                    pct_ci(c.power),
                    pct(c.power2),
                    pct_ci(c.all_biased),
                    pct_ci(c.clean_fp),
                    num(c.seconds, 1),
                ]
            },
        ),
        Study::DifPoisoning => (
            vec![
                "cell",
                "runs",
                "mixture",
                "power",
                "targets flagged",
                "clean items flagged",
                "batches with a flag",
                "fit s",
            ],
            |c| {
                vec![
                    c.key.clone(),
                    c.runs.to_string(),
                    pct(c.mixtures),
                    pct_ci(c.power),
                    pct_ci(c.target_fp),
                    pct_ci(c.clean_fp),
                    pct_ci(c.batch_fp),
                    num(c.seconds, 1),
                ]
            },
        ),
        Study::DifPoolScale => (
            vec![
                "cell",
                "runs",
                "mixture",
                "clean items flagged",
                "batches with a flag",
                "fit s",
            ],
            |c| {
                vec![
                    c.key.clone(),
                    c.runs.to_string(),
                    pct(c.mixtures),
                    pct_ci(c.clean_fp),
                    pct_ci(c.batch_fp),
                    num(c.seconds, 1),
                ]
            },
        ),
        _ => (
            vec![
                "cell",
                "runs",
                "admitted",
                "mixture",
                "power",
                "all leaners flagged",
                "clean items flagged",
                "biased gap (true)",
                "fit s",
            ],
            |c| {
                vec![
                    c.key.clone(),
                    c.runs.to_string(),
                    pct(c.admitted),
                    pct(c.mixtures),
                    pct_ci(c.power),
                    pct_ci(c.all_biased),
                    pct_ci(c.clean_fp),
                    format!("{} ({})", num(c.biased_gap, 2), num(c.true_gap, 2)),
                    num(c.seconds, 1),
                ]
            },
        ),
    };
    let rows: Vec<Row> = table.iter().map(pick).collect();
    md_table(&headers, &rows)
}

/// Flags at another cut, by the production rule: a trusted fit and a gap above the cut.
fn flags_at(o: &DifOutcome, gaps: &[f64], cut: f64) -> Vec<bool> {
    gaps.iter().map(|&g| trusted(o) && g > cut).collect()
}

fn count_at(o: &DifOutcome, roles: &str, gaps: &[f64], cut: f64) -> Option<(usize, usize)> {
    let flags = flags_at(o, gaps, cut);
    count_of(o, roles, |j| flags[j])
}

struct CutTable {
    markdown: String,
    csv: String,
}

/// FP on the null runs and power on `power` cells, for each cut of `cuts` (`docs/13` §5).
fn cut_table(
    cuts: &[f64],
    nulls: &[&DifOutcome],
    power: &[(String, String, Vec<&DifOutcome>)],
    gaps: fn(&DifOutcome) -> &Vec<f64>,
) -> CutTable {
    let mut headers = vec!["cut".to_string(), "null: clean items flagged".to_string()];
    headers.extend(power.iter().map(|(label, _, _)| format!("power {label}")));
    let mut md_rows = Vec::new();
    let mut csv_rows = Vec::new();
    for &cut in cuts {
        let null: Vec<(usize, usize)> = nulls
            .iter()
            .filter_map(|o| count_at(o, "c", gaps(o), cut))
            .collect();
        let fp = clustered_rate(&null);
        let mut md = vec![num(cut, 2), pct_ci(fp)];
        csv_rows.push(vec![
            v(cut),
            "null".into(),
            null.len().to_string(),
            v(fp.0),
            v(fp.1),
            v(fp.2),
        ]);
        for (_, key, runs) in power {
            let hits: Vec<(usize, usize)> = runs
                .iter()
                .filter_map(|o| count_at(o, "+-", gaps(o), cut))
                .collect();
            let p = clustered_rate(&hits);
            md.push(pct_ci(p));
            csv_rows.push(vec![
                v(cut),
                key.clone(),
                hits.len().to_string(),
                v(p.0),
                v(p.1),
                v(p.2),
            ]);
        }
        md_rows.push(md);
    }
    let headers: Vec<&str> = headers.iter().map(String::as_str).collect();
    CutTable {
        markdown: md_table(&headers, &md_rows),
        csv: csv(
            &["cut", "cell", "runs", "rate", "rate_lo", "rate_hi"],
            &csv_rows,
        ),
    }
}

fn dif_outcomes(records: &[Record]) -> impl Iterator<Item = &DifOutcome> {
    records.iter().filter_map(dif_of)
}

fn power_cells(
    study: Study,
    records: &[Record],
    keep: impl Fn(&DifDesign) -> Option<String>,
) -> Vec<(String, String, Vec<&DifOutcome>)> {
    grouped(study, records)
        .into_iter()
        .filter_map(|(key, rs)| {
            let label = keep(&design(study, &key)?)?;
            Some((label, key, rs.iter().filter_map(|r| dif_of(r)).collect()))
        })
        .collect()
}

fn dtf_of(r: &Record) -> Option<&DtfOutcome> {
    match &r.outcome {
        Outcome::Dtf(o) => Some(o),
        _ => None,
    }
}

const SET_NAMES: [&str; 8] = [
    "0", "1", "0+1", "2+3", "0+2", "0+1+2+3", "4+5+6+7", "0+1+4+5",
];

fn dtf_section(study: Study, records: &[Record]) -> (String, String) {
    let columns = [
        "cell",
        "set",
        "runs",
        "mixtures",
        "runs_fitted",
        "truth_mean",
        "error_mean",
        "error_sd",
        "error_p5",
        "error_p95",
        "error_max_abs",
        "false_admission",
        "false_admission_lo",
        "false_admission_hi",
        "severe",
    ];
    let (mut csv_rows, mut md_rows) = (Vec::new(), Vec::new());
    for (key, rs) in grouped(study, records) {
        let runs: Vec<&DtfOutcome> = rs.iter().filter_map(|r| dtf_of(r)).collect();
        let mixtures = runs.iter().filter(|o| o.classes >= 2).count();
        for (s, name) in SET_NAMES.iter().enumerate().take(DTF_SETS.len()) {
            let pairs: Vec<(f64, f64)> = runs
                .iter()
                .filter(|o| o.classes >= 2)
                .filter_map(|o| Some((*o.fitted.get(s)?, *o.truth.get(s)?)))
                .filter(|(f, t)| f.is_finite() && t.is_finite())
                .collect();
            let errors = sorted(pairs.iter().map(|(f, t)| f - t).collect());
            let admitted = pairs.iter().filter(|(f, _)| *f <= DTF_MAX);
            let false_admissions = admitted.clone().filter(|(_, t)| *t > DTF_MAX).count();
            let severe = admitted.filter(|(_, t)| *t > DTF_MAX + 0.05).count();
            let fa = rate(false_admissions, pairs.len());
            let truth = mean(&pairs.iter().map(|(_, t)| *t).collect::<Vec<_>>());
            let max_abs = errors.iter().fold(f64::NAN, |m, e| m.max(e.abs()));
            let (p5, p95) = (quantile(&errors, 0.05), quantile(&errors, 0.95));
            csv_rows.push(vec![
                key.clone(),
                name.to_string(),
                runs.len().to_string(),
                mixtures.to_string(),
                pairs.len().to_string(),
                v(truth),
                v(mean(&errors)),
                v(sd(&errors)),
                v(p5),
                v(p95),
                v(max_abs),
                v(fa.0),
                v(fa.1),
                v(fa.2),
                v(severe as f64 / pairs.len() as f64),
            ]);
            md_rows.push(vec![
                key.clone(),
                name.to_string(),
                format!("{} of {}", pairs.len(), runs.len()),
                num(truth, 3),
                num(mean(&errors), 3),
                format!("[{}, {}]", num(p5, 3), num(p95, 3)),
                num(max_abs, 3),
                pct_ci(fa),
            ]);
        }
    }
    let headers = [
        "cell",
        "set",
        "fitted runs",
        "true DTF",
        "error mean",
        "error p5–p95",
        "max |error|",
        "admitted but over the tolerance",
    ];
    (md_table(&headers, &md_rows), csv(&columns, &csv_rows))
}

fn sweep_of(r: &Record) -> Option<&SweepOutcome> {
    match &r.outcome {
        Outcome::Sweep(o) => Some(o),
        _ => None,
    }
}

/// Per run, `(hits, items)` over the items `pick` selects, a hit a gate code in `codes`.
fn gate_count(
    o: &SweepOutcome,
    pick: impl Fn(usize) -> bool,
    codes: &str,
) -> Option<(usize, usize)> {
    let items: Vec<char> = o
        .gate
        .chars()
        .enumerate()
        .filter(|(j, _)| pick(*j))
        .map(|(_, c)| c)
        .collect();
    (!items.is_empty()).then(|| {
        (
            items.iter().filter(|c| codes.contains(**c)).count(),
            items.len(),
        )
    })
}

fn leak(o: &SweepOutcome, share: f64) -> f64 {
    let side = |sign: f64| {
        let v: Vec<f64> = o
            .full
            .iter()
            .zip(&o.lean)
            .filter(|(_, l)| **l * sign > 0.0)
            .map(|(s, _)| *s)
            .collect();
        mean(&v)
    };
    (side(1.0) - side(-1.0)) / (2.0 * (2.0 * share - 1.0) * 0.36)
}

fn sweep_section(study: Study, records: &[Record]) -> (String, String, String) {
    let columns = [
        "cell",
        "runs",
        "converged",
        "axis_corr_mean",
        "axis_corr_min",
        "consensus_bias",
        "consensus_rmse",
        "high_pass",
        "high_pass_lo",
        "high_pass_hi",
        "low_pass",
        "low_pass_lo",
        "low_pass_hi",
        "band",
        "partisan_pass",
        "partisan_pass_lo",
        "partisan_pass_hi",
        "partisan_appeal",
        "leak_mean",
        "leak_sd",
        "fit_seconds",
    ];
    let (mut csv_rows, mut md_rows) = (Vec::new(), Vec::new());
    let mut by_n: BTreeMap<usize, Vec<&SweepOutcome>> = BTreeMap::new();
    for (key, rs) in grouped(study, records) {
        let runs: Vec<&SweepOutcome> = rs.iter().filter_map(|r| sweep_of(r)).collect();
        let Some(Cell::Sweep(d)) = Cell::parse(study, &key) else {
            continue;
        };
        by_n.entry(d.n).or_default().extend(runs.iter().copied());
        let consensus = |o: &SweepOutcome, j: usize| o.lean[j] == 0.0;
        let errors: Vec<f64> = runs
            .iter()
            .flat_map(|o| {
                (0..o.q.len())
                    .filter(|&j| consensus(o, j))
                    .map(|j| o.robust[j] - o.q[j])
                    .collect::<Vec<_>>()
            })
            .collect();
        let rmse = mean(&errors.iter().map(|e| e * e).collect::<Vec<_>>()).sqrt();
        let rate_of = |pick: &dyn Fn(&SweepOutcome, usize) -> bool, codes: &str| {
            clustered_rate(
                &runs
                    .iter()
                    .filter_map(|o| gate_count(o, |j| pick(o, j), codes))
                    .collect::<Vec<_>>(),
            )
        };
        let high = rate_of(&|o, j| consensus(o, j) && o.q[j] >= TAU + 0.05, "P");
        let low = rate_of(&|o, j| consensus(o, j) && o.q[j] <= TAU - 0.05, "P");
        let band = rate_of(&|o, j| consensus(o, j), "S").0;
        let partisan = rate_of(&|o, j| !consensus(o, j), "P");
        let appeal = rate_of(&|o, j| !consensus(o, j), "A").0;
        let leaks: Vec<f64> = if d.share == 0.5 {
            Vec::new()
        } else {
            runs.iter().map(|o| leak(o, d.share)).collect()
        };
        let corr: Vec<f64> = runs.iter().map(|o| o.axis_corr).collect();
        let converged = runs.iter().filter(|o| o.converged).count() as f64 / runs.len() as f64;
        let seconds = mean(
            &rs.iter()
                .map(|r| r.elapsed_ms as f64 / 1000.0)
                .collect::<Vec<_>>(),
        );
        let corr_min = corr.iter().copied().fold(f64::NAN, f64::min);
        let mut row = vec![
            key.clone(),
            runs.len().to_string(),
            v(converged),
            v(mean(&corr)),
            v(corr_min),
            v(mean(&errors)),
            v(rmse),
        ];
        for r in [high, low] {
            row.extend(triple(r));
        }
        row.push(v(band));
        row.extend(triple(partisan));
        row.extend([v(appeal), v(mean(&leaks)), v(sd(&leaks)), v(seconds)]);
        csv_rows.push(row);
        md_rows.push(vec![
            key.clone(),
            runs.len().to_string(),
            num(mean(&corr), 3),
            num(corr_min, 3),
            format!("{} / {}", num(mean(&errors), 3), num(rmse, 3)),
            pct_ci(high),
            pct_ci(low),
            pct(band),
            pct_ci(partisan),
            pct(appeal),
            format!("{} ± {}", num(mean(&leaks), 2), num(sd(&leaks), 2)),
        ]);
    }
    let headers = [
        "cell",
        "runs",
        "axis corr",
        "axis corr min",
        "consensus bias / RMSE",
        "q ≥ τ+0.05 passed",
        "q ≤ τ−0.05 passed",
        "band",
        "partisan passed",
        "partisan appealable",
        "leak",
    ];
    let mut tau_md = Vec::new();
    let mut tau_csv = Vec::new();
    for tau in [0.70, 0.75, 0.80, 0.85, 0.90] {
        let mut md = vec![num(tau, 2)];
        for (n, runs) in &by_n {
            let should_fail = |o: &SweepOutcome, j: usize| o.lean[j] != 0.0 || o.q[j] < tau - 0.05;
            let should_pass = |o: &SweepOutcome, j: usize| o.lean[j] == 0.0 && o.q[j] > tau + 0.05;
            let over = |o: &SweepOutcome,
                        pick: &dyn Fn(usize) -> bool,
                        above: bool|
             -> Option<(usize, usize)> {
                let items: Vec<usize> = (0..o.q.len()).filter(|&j| pick(j)).collect();
                (!items.is_empty()).then(|| {
                    (
                        items
                            .iter()
                            .filter(|&&j| (o.robust[j] >= tau) == above)
                            .count(),
                        items.len(),
                    )
                })
            };
            let fp = clustered_rate(
                &runs
                    .iter()
                    .filter_map(|o| over(o, &|j| should_fail(o, j), true))
                    .collect::<Vec<_>>(),
            );
            let fn_ = clustered_rate(
                &runs
                    .iter()
                    .filter_map(|o| over(o, &|j| should_pass(o, j), false))
                    .collect::<Vec<_>>(),
            );
            md.push(format!("{} / {}", pct(fp.0), pct(fn_.0)));
            tau_csv.push(vec![
                v(tau),
                n.to_string(),
                v(fp.0),
                v(fp.1),
                v(fp.2),
                v(fn_.0),
                v(fn_.1),
                v(fn_.2),
            ]);
        }
        tau_md.push(md);
    }
    let mut tau_headers = vec!["τ".to_string()];
    tau_headers.extend(
        by_n.keys()
            .map(|n| format!("n = {n}: false pass / false fail")),
    );
    let tau_headers: Vec<&str> = tau_headers.iter().map(String::as_str).collect();
    let markdown = format!(
        "{}\nThe gate's threshold against the design's quality `q` (robust score, no band; items within 0.05 of τ are not counted):\n\n{}",
        md_table(&headers, &md_rows),
        md_table(&tau_headers, &tau_md)
    );
    let tau_columns = [
        "tau",
        "n",
        "false_pass",
        "false_pass_lo",
        "false_pass_hi",
        "false_fail",
        "false_fail_lo",
        "false_fail_hi",
    ];
    (
        markdown,
        csv(&columns, &csv_rows),
        csv(&tau_columns, &tau_csv),
    )
}

fn capture_of(r: &Record) -> Option<&CaptureOutcome> {
    match &r.outcome {
        Outcome::Capture(o) => Some(o),
        _ => None,
    }
}

/// The first count of opposing boosters at which `scores` reaches τ; infinite if never.
fn crossing(o: &CaptureOutcome, scores: &[f64]) -> f64 {
    o.opposing
        .iter()
        .zip(scores)
        .find(|(_, &s)| s >= TAU)
        .map_or(f64::INFINITY, |(&k, _)| k as f64)
}

fn capture_section(study: Study, records: &[Record]) -> (String, String, String) {
    let columns = [
        "cell",
        "runs",
        "opposing",
        "robust_median",
        "robust_p5",
        "robust_p95",
        "robust_never",
        "full_median",
        "full_p5",
        "full_p95",
        "full_never",
        "plain_at_zero",
    ];
    let (mut csv_rows, mut md_rows, mut curve_rows, mut curve_md) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for (key, rs) in grouped(study, records) {
        let runs: Vec<&CaptureOutcome> = rs.iter().filter_map(|r| capture_of(r)).collect();
        let robust = sorted(runs.iter().map(|o| crossing(o, &o.robust)).collect());
        let full = sorted(runs.iter().map(|o| crossing(o, &o.full)).collect());
        let never = |v: &[f64]| v.iter().filter(|x| x.is_infinite()).count();
        let opposing = runs
            .iter()
            .filter_map(|o| o.opposing.last())
            .copied()
            .max()
            .unwrap_or(0);
        let plain0 = mean(
            &runs
                .iter()
                .filter_map(|o| o.plain.first())
                .copied()
                .collect::<Vec<_>>(),
        );
        let q = |v: &[f64], p: f64| quantile(v, p);
        csv_rows.push(vec![
            key.clone(),
            runs.len().to_string(),
            opposing.to_string(),
            v(q(&robust, 0.5)),
            v(q(&robust, 0.05)),
            v(q(&robust, 0.95)),
            v(never(&robust) as f64 / runs.len() as f64),
            v(q(&full, 0.5)),
            v(q(&full, 0.05)),
            v(q(&full, 0.95)),
            v(never(&full) as f64 / runs.len() as f64),
            v(plain0),
        ]);
        md_rows.push(vec![
            key.clone(),
            runs.len().to_string(),
            opposing.to_string(),
            format!(
                "{} [{}, {}]",
                num(q(&robust, 0.5), 0),
                num(q(&robust, 0.05), 0),
                num(q(&robust, 0.95), 0)
            ),
            pct_ci(rate(never(&robust), runs.len())),
            format!(
                "{} [{}, {}]",
                num(q(&full, 0.5), 0),
                num(q(&full, 0.05), 0),
                num(q(&full, 0.95), 0)
            ),
            num(plain0, 3),
        ]);
        let steps: Vec<usize> = runs.first().map(|o| o.opposing.clone()).unwrap_or_default();
        for (i, &k) in steps.iter().enumerate() {
            let at = |f: fn(&CaptureOutcome) -> &Vec<f64>| -> Vec<f64> {
                runs.iter().filter_map(|o| f(o).get(i).copied()).collect()
            };
            let (robust_i, full_i, plain_i) =
                (at(|o| &o.robust), at(|o| &o.full), at(|o| &o.plain));
            let passing = robust_i.iter().filter(|&&s| s >= TAU).count();
            curve_rows.push(vec![
                key.clone(),
                k.to_string(),
                robust_i.len().to_string(),
                v(mean(&robust_i)),
                v(sd(&robust_i)),
                v(mean(&full_i)),
                v(mean(&plain_i)),
                v(passing as f64 / robust_i.len() as f64),
            ]);
            if k % 20 == 0 {
                curve_md.push(vec![
                    key.clone(),
                    k.to_string(),
                    format!("{} ± {}", num(mean(&robust_i), 3), num(sd(&robust_i), 3)),
                    num(mean(&full_i), 3),
                    num(mean(&plain_i), 3),
                    pct(passing as f64 / robust_i.len() as f64),
                ]);
            }
        }
    }
    let headers = [
        "cell",
        "runs",
        "opposing camp",
        "crossing, robust: median [p5, p95]",
        "never crosses",
        "crossing, full fit",
        "plain mean, no opposing booster",
    ];
    let curve_headers = [
        "cell",
        "opposing boosters",
        "robust score",
        "full score",
        "plain mean",
        "robust ≥ τ",
    ];
    let markdown = format!(
        "{}\nThe score as opposing boosters join, every 20 (all steps in `curve.csv`):\n\n{}",
        md_table(&headers, &md_rows),
        md_table(&curve_headers, &curve_md)
    );
    let curve_columns = [
        "cell",
        "opposing",
        "runs",
        "robust_mean",
        "robust_sd",
        "full_mean",
        "plain_mean",
        "robust_pass",
    ];
    (
        markdown,
        csv(&columns, &csv_rows),
        csv(&curve_columns, &curve_rows),
    )
}

fn is_admitted_null(o: &DifOutcome) -> bool {
    o.admitted && !o.roles.chars().any(|c| "+-2t".contains(c))
}

/// Reads every study's records under `out`, writes `summary.md` and the CSV tables, and
/// returns the markdown.
pub fn summarize(out: &Path) -> io::Result<String> {
    let mut all: BTreeMap<Study, Vec<Record>> = BTreeMap::new();
    for study in STUDIES {
        all.insert(study, read(out, study)?);
    }
    let total: usize = all.values().map(Vec::len).sum();
    let mut md = format!(
        "# T24 characterization: summary\n\n{total} runs recorded in `{}`. Rates are shares of runs or \
         items with 95% intervals: Wilson over runs; over items, Wilson on the effective sample size \
         of items grouped in batches (`docs/13` §5).\n",
        out.display()
    );
    for study in STUDIES {
        let records = &all[&study];
        md.push_str(&format!(
            "\n## {}\n\n*{}*: {} runs.\n\n",
            study.name(),
            study.claims(),
            records.len()
        ));
        if records.is_empty() {
            md.push_str("No records yet.\n");
            continue;
        }
        let dir = out.join(study.name());
        match study.kind() {
            Kind::Dif => {
                let table: Vec<DifCell> = grouped(study, records)
                    .iter()
                    .map(|(k, rs)| dif_cell(study, k, rs))
                    .collect();
                let rows: Vec<Row> = table.iter().map(DifCell::csv_row).collect();
                fs::write(dir.join("summary.csv"), csv(&DIF_COLUMNS, &rows))?;
                md.push_str(&dif_markdown(study, &table));
            }
            Kind::Dtf => {
                let (markdown, table) = dtf_section(study, records);
                fs::write(dir.join("summary.csv"), table)?;
                md.push_str(&markdown);
            }
            Kind::Sweep => {
                let (markdown, table, tau) = sweep_section(study, records);
                fs::write(dir.join("summary.csv"), table)?;
                fs::write(dir.join("tau.csv"), tau)?;
                md.push_str(&markdown);
            }
            Kind::Capture => {
                let (markdown, table, curve) = capture_section(study, records);
                fs::write(dir.join("summary.csv"), table)?;
                fs::write(dir.join("curve.csv"), curve)?;
                md.push_str(&markdown);
            }
        }
    }
    let nulls: Vec<&DifOutcome> = dif_outcomes(&all[&Study::DifNull])
        .filter(|o| is_admitted_null(o))
        .collect();
    let power = power_cells(Study::DifPower, &all[&Study::DifPower], |d| {
        (d.k == 8 && d.pi == 0.5 && d.layout == crate::grid::Layout::Campaign(2) && d.n >= 3000)
            .then(|| format!("N={} δ={}", d.n, d.delta))
    });
    let dif_cut = cut_table(
        &[0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 1.1, 1.2, 1.3, 1.4, 1.5],
        &nulls,
        &power,
        |o| &o.dif,
    );
    let nonuniform = power_cells(Study::DifNonuniform, &all[&Study::DifNonuniform], |d| {
        (d.delta == 0.0).then(|| format!("N={} α={}", d.n, d.alpha))
    });
    let a_cut = cut_table(
        &[0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0],
        &nulls,
        &nonuniform,
        |o| &o.a_gap,
    );
    fs::write(out.join("thresholds-dif-cut.csv"), &dif_cut.csv)?;
    fs::write(out.join("thresholds-a-gap.csv"), &a_cut.csv)?;
    md.push_str(&format!(
        "\n## Threshold tables\n\n### The DIF cut on `DIF_j` (production: {MIXTURE_DIF_MAX:.1})\n\nClean items \
         flagged on admitted null batches, and power on two leaning items of eight (π = 0.5), at each \
         cut of the production rule (a converged fit with two or more classes, a gap above the cut):\n\n{}\n\
         ### A cut on the discrimination gap `a_gap` (none in production)\n\nThe same, with `a_gap` in place \
         of `DIF_j`; power on the pure non-uniform cells of `dif-nonuniform`:\n\n{}",
        dif_cut.markdown, a_cut.markdown
    ));
    fs::write(out.join("summary.md"), &md)?;
    Ok(md)
}
