//! The T24 characterization harness (`docs/13`): seeded simulation studies of the
//! production detectors and gates, run in parallel, resumable, summarized with intervals.

pub mod generate;
pub mod grid;
pub mod record;
pub mod run;
pub mod runner;
pub mod stats;
pub mod summary;
