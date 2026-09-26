//! The command line of the T24 harness: `plan`, `run` and `summarize` (`docs/13` §2).

use characterization::grid::{cells, replicates, tasks, Grid, Study, STUDIES};
use characterization::runner::{execute, Options};
use characterization::summary::summarize;
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
usage: characterization <plan|run|summarize> [options]

  plan        the studies, their cells and runs, and the claims they measure
  run         run the studies; records go to <out>/<study>/records.csv, and an
              interrupted run resumes where it stopped when started again
  summarize   write <out>/summary.md and the CSV tables from the records

options:
  --grid full|smoke        the grid of docs/13 §4, or one tiny cell per study (full)
  --study NAME[,NAME...]   the studies to plan or run (all)
  --replicates R           the first R replicates of each study, at most its own count
  --filter TEXT            only the cells whose key contains TEXT
  --jobs J                 worker threads (the number of cores)
  --out DIR                where the records go (characterization-results)
  --quiet                  no progress lines
";

struct Args {
    command: String,
    grid: Grid,
    studies: Vec<Study>,
    replicates: Option<u32>,
    filter: Option<String>,
    jobs: usize,
    out: PathBuf,
    quiet: bool,
}

fn parse(mut argv: impl Iterator<Item = String>) -> Result<Args, String> {
    let command = argv.next().ok_or("no command")?;
    let mut args = Args {
        command,
        grid: Grid::Full,
        studies: STUDIES.to_vec(),
        replicates: None,
        filter: None,
        jobs: std::thread::available_parallelism().map_or(1, |n| n.get()),
        out: PathBuf::from("characterization-results"),
        quiet: false,
    };
    while let Some(flag) = argv.next() {
        if flag == "--quiet" {
            args.quiet = true;
            continue;
        }
        let value = argv.next().ok_or(format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--grid" => {
                args.grid = match value.as_str() {
                    "full" => Grid::Full,
                    "smoke" => Grid::Smoke,
                    other => return Err(format!("unknown grid {other:?}")),
                }
            }
            "--study" if value == "all" => args.studies = STUDIES.to_vec(),
            "--study" => {
                args.studies = value
                    .split(',')
                    .map(|n| Study::parse(n).ok_or(format!("unknown study {n:?}")))
                    .collect::<Result<_, _>>()?
            }
            "--replicates" => {
                args.replicates = Some(value.parse().map_err(|_| "--replicates needs a number")?)
            }
            "--filter" => args.filter = Some(value),
            "--jobs" => args.jobs = value.parse().map_err(|_| "--jobs needs a number")?,
            "--out" => args.out = PathBuf::from(value),
            other => return Err(format!("unknown option {other:?}")),
        }
    }
    Ok(args)
}

fn plan(args: &Args) {
    println!(
        "{:<18} {:>6} {:>10} {:>8}  measures",
        "study", "cells", "replicates", "runs"
    );
    let mut total = 0;
    for &study in &args.studies {
        let runs = tasks(&[study], args.grid, args.replicates, args.filter.as_deref()).len();
        let cap = args.replicates.map_or(u32::MAX, |r| r);
        let reps = replicates(study, args.grid).min(cap);
        let kept = cells(study, args.grid)
            .iter()
            .filter(|c| args.filter.as_deref().is_none_or(|f| c.key().contains(f)))
            .count();
        println!(
            "{:<18} {kept:>6} {reps:>10} {runs:>8}  {}",
            study.name(),
            study.claims()
        );
        total += runs;
    }
    println!("{:<18} {:>6} {:>10} {total:>8}", "total", "", "");
}

fn main() -> ExitCode {
    let args = match parse(std::env::args().skip(1)) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let result = match args.command.as_str() {
        "plan" => {
            plan(&args);
            Ok(())
        }
        "run" => {
            let all = tasks(
                &args.studies,
                args.grid,
                args.replicates,
                args.filter.as_deref(),
            );
            eprintln!(
                "{} runs planned; {} workers; records in {}",
                all.len(),
                args.jobs,
                args.out.display()
            );
            let opts = Options {
                jobs: args.jobs,
                out: args.out.clone(),
                progress: !args.quiet,
            };
            execute(&all, &opts).and_then(|report| {
                eprintln!(
                    "{} run, {} already recorded, {} failed{}",
                    report.ran,
                    report.skipped,
                    report.failed,
                    if report.failed > 0 {
                        " (see errors.log)"
                    } else {
                        ""
                    }
                );
                summarize(&args.out).map(|_| {
                    eprintln!("summary: {}", args.out.join("summary.md").display());
                })
            })
        }
        "summarize" => summarize(&args.out).map(|md| println!("{md}")),
        "help" | "--help" | "-h" => {
            println!("{USAGE}");
            Ok(())
        }
        other => {
            eprintln!("unknown command {other:?}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
