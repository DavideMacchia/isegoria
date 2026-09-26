//! Runs tasks on worker threads, appends each record as it completes, skips the runs a
//! previous execution recorded, and reports progress (`docs/13` §2).

use crate::grid::{Study, Task};
use crate::record::{Record, Store};
use crate::run::run;
use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub struct Options {
    pub jobs: usize,
    pub out: PathBuf,
    pub progress: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Report {
    pub ran: usize,
    pub skipped: usize,
    pub failed: usize,
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic without a message".to_string())
}

fn clock(d: Duration) -> String {
    let s = d.as_secs();
    format!("{}h{:02}m{:02}s", s / 3600, s / 60 % 60, s % 60)
}

struct Progress {
    total: usize,
    done: usize,
    started: Instant,
    last: Instant,
    show: bool,
}

impl Progress {
    fn tick(&mut self, record: &Record) {
        self.done += 1;
        let now = Instant::now();
        if !self.show || (now - self.last < Duration::from_secs(30) && self.done < self.total) {
            return;
        }
        self.last = now;
        let elapsed = now - self.started;
        let per_run = elapsed.as_secs_f64() / self.done as f64;
        let eta = Duration::from_secs_f64(per_run * (self.total - self.done) as f64);
        eprintln!(
            "[{}] {}/{} runs ({:.1}%), {:.1} runs/min, ETA {}; last: {} {} #{} in {:.1} s",
            clock(elapsed),
            self.done,
            self.total,
            100.0 * self.done as f64 / self.total as f64,
            60.0 / per_run,
            clock(eta),
            record.study.name(),
            record.cell,
            record.replicate,
            record.elapsed_ms as f64 / 1000.0
        );
    }
}

/// Runs every task `opts.out` does not hold yet on `opts.jobs` threads. A run that panics is
/// logged to `errors.log`, which lists this execution's failures only, and left unrecorded.
pub fn execute(tasks: &[Task], opts: &Options) -> io::Result<Report> {
    let mut stores: BTreeMap<Study, Store> = BTreeMap::new();
    for task in tasks {
        if let Entry::Vacant(slot) = stores.entry(task.study) {
            slot.insert(Store::open(&opts.out, task.study)?);
        }
    }
    let log = opts.out.join("errors.log");
    if log.exists() {
        std::fs::remove_file(&log)?;
    }
    let todo: Vec<&Task> = tasks
        .iter()
        .filter(|t| !stores[&t.study].contains(&t.cell.key(), t.replicate))
        .collect();
    let mut report = Report {
        ran: 0,
        skipped: tasks.len() - todo.len(),
        failed: 0,
    };
    let next = AtomicUsize::new(0);
    let jobs = opts.jobs.clamp(1, todo.len().max(1));
    let started = Instant::now();
    let mut progress = Progress {
        total: todo.len(),
        done: 0,
        started,
        last: started,
        show: opts.progress,
    };
    std::thread::scope(|scope| -> io::Result<()> {
        let (tx, rx) = mpsc::channel::<(usize, Result<Record, String>)>();
        for _ in 0..jobs {
            let (tx, next, todo) = (tx.clone(), &next, &todo);
            scope.spawn(move || loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(task) = todo.get(i) else { break };
                let t0 = Instant::now();
                let result = panic::catch_unwind(AssertUnwindSafe(|| run(task)))
                    .map(|outcome| Record::new(task, t0.elapsed().as_millis() as u64, outcome))
                    .map_err(|payload| panic_message(payload.as_ref()));
                if tx.send((i, result)).is_err() {
                    break;
                }
            });
        }
        drop(tx);
        for (i, result) in rx {
            match result {
                Ok(record) => {
                    let store = stores
                        .get_mut(&record.study)
                        .expect("every task's study has a store");
                    store.append(&record)?;
                    report.ran += 1;
                    progress.tick(&record);
                }
                Err(message) => {
                    let task = todo[i];
                    let mut log = OpenOptions::new().create(true).append(true).open(&log)?;
                    writeln!(
                        log,
                        "{} {} #{}: {message}",
                        task.study.name(),
                        task.cell.key(),
                        task.replicate
                    )?;
                    report.failed += 1;
                }
            }
        }
        Ok(())
    })?;
    Ok(report)
}
