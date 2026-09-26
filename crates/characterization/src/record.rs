//! One CSV line per run, one file per study, and the store that appends to it and resumes
//! after an interruption (`docs/13` §2).

use crate::grid::{Kind, Study, Task};
use crate::run::{CaptureOutcome, DifOutcome, DtfOutcome, Outcome, SweepOutcome};
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    pub study: Study,
    pub cell: String,
    pub replicate: u32,
    pub seed: u64,
    pub elapsed_ms: u64,
    pub outcome: Outcome,
}

const COMMON: [&str; 5] = ["study", "cell", "replicate", "seed", "elapsed_ms"];

fn columns(kind: Kind) -> &'static [&'static str] {
    match kind {
        Kind::Dif => &[
            "kr20",
            "admitted",
            "classes",
            "non_uniform",
            "converged",
            "bic_gain",
            "pi",
            "eta",
            "dif",
            "a_gap",
            "flags",
            "roles",
        ],
        Kind::Dtf => &["classes", "converged", "flags", "roles", "fitted", "truth"],
        Kind::Sweep => &[
            "converged",
            "axis_corr",
            "q",
            "lean",
            "truth",
            "full",
            "robust",
            "gap",
            "gate",
        ],
        Kind::Capture => &["opposing", "full", "robust", "plain"],
    }
}

pub fn header(study: Study) -> String {
    let all: Vec<&str> = COMMON
        .iter()
        .chain(columns(study.kind()))
        .copied()
        .collect();
    all.join(",")
}

fn nums(v: &[f64]) -> String {
    let parts: Vec<String> = v.iter().map(|x| x.to_string()).collect();
    parts.join(";")
}

fn counts(v: &[usize]) -> String {
    let parts: Vec<String> = v.iter().map(|x| x.to_string()).collect();
    parts.join(";")
}

fn bits(v: &[bool]) -> String {
    v.iter().map(|&b| if b { '1' } else { '0' }).collect()
}

fn bit(b: bool) -> String {
    bits(&[b])
}

type Parsed<T> = Result<T, String>;

fn parse<T: std::str::FromStr>(s: &str, what: &str) -> Parsed<T> {
    s.parse().map_err(|_| format!("{what}: not a value: {s:?}"))
}

fn parse_list<T: std::str::FromStr>(s: &str, what: &str) -> Parsed<Vec<T>> {
    if s.is_empty() {
        return Ok(Vec::new());
    }
    s.split(';').map(|v| parse(v, what)).collect()
}

fn parse_bits(s: &str, what: &str) -> Parsed<Vec<bool>> {
    s.chars()
        .map(|c| match c {
            '0' => Ok(false),
            '1' => Ok(true),
            _ => Err(format!("{what}: not a bit: {c:?}")),
        })
        .collect()
}

fn parse_bit(s: &str, what: &str) -> Parsed<bool> {
    match parse_bits(s, what)?.as_slice() {
        [b] => Ok(*b),
        _ => Err(format!("{what}: not one bit: {s:?}")),
    }
}

impl Record {
    pub fn new(task: &Task, elapsed_ms: u64, outcome: Outcome) -> Record {
        Record {
            study: task.study,
            cell: task.cell.key(),
            replicate: task.replicate,
            seed: task.seed(),
            elapsed_ms,
            outcome,
        }
    }

    /// The CSV line, without its newline; lists are `;`-separated, flags a string of bits.
    pub fn line(&self) -> String {
        let mut fields = vec![
            self.study.name().to_string(),
            self.cell.clone(),
            self.replicate.to_string(),
            self.seed.to_string(),
            self.elapsed_ms.to_string(),
        ];
        match &self.outcome {
            Outcome::Dif(o) => fields.extend([
                o.kr20.to_string(),
                bit(o.admitted),
                o.classes.to_string(),
                bit(o.non_uniform),
                bit(o.converged),
                o.bic_gain.to_string(),
                nums(&o.pi),
                nums(&o.eta),
                nums(&o.dif),
                nums(&o.a_gap),
                bits(&o.flags),
                o.roles.clone(),
            ]),
            Outcome::Dtf(o) => fields.extend([
                o.classes.to_string(),
                bit(o.converged),
                bits(&o.flags),
                o.roles.clone(),
                nums(&o.fitted),
                nums(&o.truth),
            ]),
            Outcome::Sweep(o) => fields.extend([
                bit(o.converged),
                o.axis_corr.to_string(),
                nums(&o.q),
                nums(&o.lean),
                nums(&o.truth),
                nums(&o.full),
                nums(&o.robust),
                nums(&o.gap),
                o.gate.clone(),
            ]),
            Outcome::Capture(o) => fields.extend([
                counts(&o.opposing),
                nums(&o.full),
                nums(&o.robust),
                nums(&o.plain),
            ]),
        }
        fields.join(",")
    }

    pub fn parse(study: Study, line: &str) -> Parsed<Record> {
        let f: Vec<&str> = line.split(',').collect();
        let expected = COMMON.len() + columns(study.kind()).len();
        if f.len() != expected {
            return Err(format!("{} fields where {expected} were expected", f.len()));
        }
        if f[0] != study.name() {
            return Err(format!("a {} record among {}", f[0], study.name()));
        }
        let o = &f[COMMON.len()..];
        let outcome = match study.kind() {
            Kind::Dif => Outcome::Dif(DifOutcome {
                kr20: parse(o[0], "kr20")?,
                admitted: parse_bit(o[1], "admitted")?,
                classes: parse(o[2], "classes")?,
                non_uniform: parse_bit(o[3], "non_uniform")?,
                converged: parse_bit(o[4], "converged")?,
                bic_gain: parse(o[5], "bic_gain")?,
                pi: parse_list(o[6], "pi")?,
                eta: parse_list(o[7], "eta")?,
                dif: parse_list(o[8], "dif")?,
                a_gap: parse_list(o[9], "a_gap")?,
                flags: parse_bits(o[10], "flags")?,
                roles: o[11].to_string(),
            }),
            Kind::Dtf => Outcome::Dtf(DtfOutcome {
                classes: parse(o[0], "classes")?,
                converged: parse_bit(o[1], "converged")?,
                flags: parse_bits(o[2], "flags")?,
                roles: o[3].to_string(),
                fitted: parse_list(o[4], "fitted")?,
                truth: parse_list(o[5], "truth")?,
            }),
            Kind::Sweep => Outcome::Sweep(SweepOutcome {
                converged: parse_bit(o[0], "converged")?,
                axis_corr: parse(o[1], "axis_corr")?,
                q: parse_list(o[2], "q")?,
                lean: parse_list(o[3], "lean")?,
                truth: parse_list(o[4], "truth")?,
                full: parse_list(o[5], "full")?,
                robust: parse_list(o[6], "robust")?,
                gap: parse_list(o[7], "gap")?,
                gate: o[8].to_string(),
            }),
            Kind::Capture => Outcome::Capture(CaptureOutcome {
                opposing: parse_list(o[0], "opposing")?,
                full: parse_list(o[1], "full")?,
                robust: parse_list(o[2], "robust")?,
                plain: parse_list(o[3], "plain")?,
            }),
        };
        Ok(Record {
            study,
            cell: f[1].to_string(),
            replicate: parse(f[2], "replicate")?,
            seed: parse(f[3], "seed")?,
            elapsed_ms: parse(f[4], "elapsed_ms")?,
            outcome,
        })
    }
}

pub fn records_path(out: &Path, study: Study) -> PathBuf {
    out.join(study.name()).join("records.csv")
}

fn invalid(path: &Path, why: String) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("{}: {why}", path.display()),
    )
}

/// The complete records of a file: the header checked, a torn last line left out, a
/// repeated run kept once. Returns them with the length of the complete part.
fn load(path: &Path, study: Study) -> io::Result<(Vec<Record>, usize)> {
    let text = fs::read_to_string(path)?;
    let complete = text.rfind('\n').map_or(0, |i| i + 1);
    let mut lines = text[..complete].lines();
    let mut records = Vec::new();
    match lines.next() {
        None => return Ok((records, 0)),
        Some(first) if first == header(study) => {}
        Some(_) => {
            return Err(invalid(
                path,
                "written under another schema; move it away or choose another --out".into(),
            ))
        }
    }
    let mut seen = BTreeSet::new();
    for (i, line) in lines.enumerate() {
        let record = Record::parse(study, line)
            .map_err(|e| invalid(path, format!("line {}: {e}", i + 2)))?;
        if seen.insert((record.cell.clone(), record.replicate)) {
            records.push(record);
        }
    }
    Ok((records, complete))
}

/// Every complete record of a study in `out`; none if the study has no file yet.
pub fn read(out: &Path, study: Study) -> io::Result<Vec<Record>> {
    let path = records_path(out, study);
    if !path.exists() {
        return Ok(Vec::new());
    }
    load(&path, study).map(|(records, _)| records)
}

/// Appends records to a study's file, knowing which runs it already holds.
pub struct Store {
    file: File,
    done: BTreeSet<(String, u32)>,
}

impl Store {
    /// Opens or creates the study's file under `out`; a torn last line is cut off.
    pub fn open(out: &Path, study: Study) -> io::Result<Store> {
        let path = records_path(out, study);
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut done = BTreeSet::new();
        let mut complete = 0;
        if path.exists() {
            let (records, length) = load(&path, study)?;
            done.extend(records.into_iter().map(|r| (r.cell, r.replicate)));
            complete = length;
            OpenOptions::new()
                .write(true)
                .open(&path)?
                .set_len(complete as u64)?;
        }
        let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
        if complete == 0 {
            file.write_all(format!("{}\n", header(study)).as_bytes())?;
            file.flush()?;
        }
        Ok(Store { file, done })
    }

    pub fn contains(&self, cell: &str, replicate: u32) -> bool {
        self.done.contains(&(cell.to_string(), replicate))
    }

    pub fn append(&mut self, record: &Record) -> io::Result<()> {
        self.file
            .write_all(format!("{}\n", record.line()).as_bytes())?;
        self.file.flush()?;
        self.done.insert((record.cell.clone(), record.replicate));
        Ok(())
    }
}
