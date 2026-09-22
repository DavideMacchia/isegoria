# Fixture provenance

These CSVs are the oracle the Rust engine is tested against (`level_a.rs`, `level_b.rs`,
`level_c.rs`, `reproducibility.rs`, `end_to_end.rs`). They were produced by
`sim/export_fixtures.py` (numpy seeds 7 / 0 / 100+s / 200).

| | |
|---|---|
| Generating environment (recorded, T4) | CPython 3.13, **numpy 2.4.4 / scipy 1.17.1** (pinned in `sim/requirements.txt`), on **Windows/MSVC**. `expected_levelA.csv` / `expected_meta.csv` were regenerated under this environment on 2026-09-21; the 18 *data* files (`R`, `mask`, `levelb_*`, `levelc_*`, `mixture_*`, `true_f`) are environment-independent and byte-unchanged. |
| Regeneration check (`tests/fixture_drift.rs`) | Runs the sim and compares. Data files must match exactly (tol 1e-4); the optimizer outputs (`*expected*`) are compared at tol **2e-3**, because their `b_j` varies by ~1e-3 across OS/BLAS and scipy releases. No longer `#[ignore]`d: it self-skips (a notice, not a failure) when `python3`/numpy/scipy is absent. CI installs `sim/requirements.txt` and runs it on every push (AT-PRO-06). |
| Why the loose tol (docs/08 §0-ter, REPRO-003) | `scipy.optimize.minimize(method="L-BFGS-B")`'s stopping point depends on both the scipy build (Fortran→C in 1.15) and the platform's BLAS/libm: with the *same* pinned numpy/scipy, `expected_levelA.csv` token 10 = 0.107583 (pre-audit), 0.107804 (Linux), 0.108018 (Windows). The oracle is defined only to ≈1e-3 in `b_j`. |
| Consequence | `level_a.rs` compares the engine at 0.03; the gate band is `ε = 0.008`; items 02/06/07 sit within 0.004 of `τ`. Verdict-level agreement near the threshold is still not independently established (docs/08 REPRO-003, G-10). |

The Level-A oracle (`expected_levelA.csv` / `expected_meta.csv`) is reproducible only to
≈1e-3 in `b_j` — within the same OS + scipy build, not across them; the Rust engine's own
`level_a` comparison (tol 0.03) and the drift guard's `2e-3` both absorb this. Do not
tighten those tolerances, and do not bump the numpy/scipy pins, without regenerating the
two `expected_*` files and updating this record. Do not edit the fixture files by hand.
