//! The grids of the T24 studies are the ones `docs/13` §4 specifies, every run has its own
//! seed, and a larger replicate count extends a smaller one (`docs/13` §2).

use characterization::grid::{cells, replicates, tasks, Cell, Grid, Study, STUDIES};
use std::collections::BTreeSet;

/// The full grid has the cells and replicates of the `docs/13` §4 tables.
#[test]
fn the_full_grid_has_the_cells_the_specification_lists() {
    let expect = [
        (Study::DifNull, 27, 200),
        (Study::DifPower, 132, 200),
        (Study::DifMisspec, 22, 200),
        (Study::DifNonuniform, 8, 200),
        (Study::DifTwoAxes, 4, 200),
        (Study::DifPoisoning, 15, 200),
        (Study::DifPoolScale, 4, 3),
        (Study::DtfError, 8, 200),
        (Study::BridgingSweep, 36, 200),
        (Study::BridgingCapture, 4, 200),
    ];
    assert_eq!(expect.len(), STUDIES.len());
    for (study, n_cells, reps) in expect {
        assert_eq!(cells(study, Grid::Full).len(), n_cells, "{}", study.name());
        assert_eq!(replicates(study, Grid::Full), reps, "{}", study.name());
        assert!(!cells(study, Grid::Smoke).is_empty(), "{}", study.name());
    }
    let runs = tasks(&STUDIES, Grid::Full, None, None).len();
    assert_eq!(runs, 51_212);
}

/// A cell's key names it uniquely and parses back to it, in both grids.
#[test]
fn every_cell_key_parses_back_to_its_cell() {
    for study in STUDIES {
        assert_eq!(Study::parse(study.name()), Some(study));
        for grid in [Grid::Full, Grid::Smoke] {
            let all = cells(study, grid);
            let keys: BTreeSet<String> = all.iter().map(Cell::key).collect();
            assert_eq!(
                keys.len(),
                all.len(),
                "{} {grid:?}: a repeated key",
                study.name()
            );
            for cell in all {
                assert_eq!(
                    Cell::parse(study, &cell.key()),
                    Some(cell),
                    "{}",
                    cell.key()
                );
            }
        }
    }
    assert_eq!(Cell::parse(Study::DifNull, "n=3000 nonsense"), None);
}

/// Every run of the full grid has its own seed, and runs come replicate by replicate.
#[test]
fn every_run_has_its_own_seed_and_runs_come_replicate_first() {
    let all = tasks(&STUDIES, Grid::Full, None, None);
    let seeds: BTreeSet<u64> = all.iter().map(|t| t.seed()).collect();
    assert_eq!(seeds.len(), all.len());
    assert!(all.windows(2).all(|w| w[0].replicate <= w[1].replicate));
}

/// Twenty replicates are the first twenty of two hundred, run for run and seed for seed.
#[test]
fn a_larger_replicate_count_extends_a_smaller_one() {
    let few = tasks(&[Study::DifPower], Grid::Full, Some(20), None);
    let many = tasks(&[Study::DifPower], Grid::Full, Some(200), None);
    assert_eq!(few.len(), 132 * 20);
    let many: BTreeSet<(String, u32, u64)> = many
        .iter()
        .map(|t| (t.cell.key(), t.replicate, t.seed()))
        .collect();
    assert!(few
        .iter()
        .all(|t| many.contains(&(t.cell.key(), t.replicate, t.seed()))));
}

/// A filter keeps the cells whose key contains it.
#[test]
fn a_filter_keeps_the_matching_cells() {
    let kept = tasks(&[Study::DifNull], Grid::Full, Some(1), Some("n=6000"));
    assert_eq!(kept.len(), 9);
    assert!(kept.iter().all(|t| t.cell.key().contains("n=6000")));
}
