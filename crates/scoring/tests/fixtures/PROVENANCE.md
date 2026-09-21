# Fixture provenance

These CSVs are the oracle the Rust engine is tested against (`level_a.rs`, `level_b.rs`,
`level_c.rs`, `reproducibility.rs`, `end_to_end.rs`). They were produced by
`sim/export_fixtures.py` (numpy seeds 7 / 0 / 100+s / 200).

| | |
|---|---|
| Generating environment (recorded, T4) | CPython 3.13, **numpy 2.4.4 / scipy 1.17.1** (pinned in `sim/requirements.txt`). `expected_levelA.csv` / `expected_meta.csv` were regenerated under this environment on 2026-09-21; the 18 *data* files (`R`, `mask`, `levelb_*`, `levelc_*`, `mixture_*`, `true_f`) are environment-independent and byte-unchanged. |
| Regeneration check (`tests/fixture_drift.rs`) | **passes** under the pinned environment. No longer `#[ignore]`d: it self-skips (prints a notice, does not fail) only when `python3`/numpy/scipy is absent, so `cargo test` stays green without a Python toolchain. CI installs `sim/requirements.txt` and runs the comparison for real on every push (AT-PRO-06). |
| Earlier drift (docs/08 §0-ter, REPRO-003) | against the pre-audit, *unrecorded* fixtures the guard failed under numpy 2.4.4 / scipy 1.17.1 by up to 1.0e-3 in `b_j` and 1.9e-4 in `mu_hat`. Cause: `scipy.optimize.minimize(method="L-BFGS-B")`'s stopping point changed when L-BFGS-B was ported from Fortran to C in SciPy 1.15. Recording and pinning the environment closes it. |
| Consequence | `level_a.rs` compares the engine at 0.03; the gate band is `ε = 0.008`; items 02/06/07 sit within 0.004 of `τ`. Verdict-level agreement near the threshold is still not independently established (docs/08 REPRO-003, G-10). |

The Level-A oracle (`expected_levelA.csv` / `expected_meta.csv`) is defined only to ≈1e-3
in `b_j`: it is reproducible **within** the pinned environment, not across SciPy releases.
Do not tighten the Level-A tolerances, and do not bump the numpy/scipy pins, without
regenerating these two files under the new environment (`python sim/export_fixtures.py
<dir>`) and updating this record. Do not edit the fixture files by hand.
