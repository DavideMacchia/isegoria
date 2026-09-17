# Fixture provenance

These CSVs are the oracle the Rust engine is tested against (`level_a.rs`, `level_b.rs`,
`level_c.rs`, `reproducibility.rs`, `end_to_end.rs`). They were produced by
`sim/export_fixtures.py` (numpy seeds 7 / 0 / 100+s / 200).

| | |
|---|---|
| numpy / scipy versions at generation | **not recorded** (pre-audit) |
| Regeneration check (`tests/fixture_drift.rs`, `#[ignore]`) | **fails** under numpy 2.4.4 / scipy 1.17.1 (run by the docs/08 auditor, 2026-09-17): first mismatch `expected_levelA.csv` token 10, `0.107583` vs `0.107804`; `bj_full` drifts by up to 1.0e-3, `mu_hat` by 1.9e-4; all *data* files (`R`, `mask`, `levelb_*`, `levelc_*`, `mixture_*`, `true_f`) regenerate exactly |
| Cause | the sim's `scipy.optimize.minimize(method="L-BFGS-B")` stopping point changed across SciPy releases (L-BFGS-B was ported from Fortran to C in SciPy 1.15); the oracle is defined only to ≈1e-3 in `b_j` |
| Consequence | `level_a.rs` compares at 0.03; the gate band is `ε = 0.008`; items 02/06/07 sit within 0.004 of `τ`. Verdict-level agreement near the threshold is not established (docs/08 REPRO-003, G-10) |

Until roadmap **T4** pins the Python environment in CI and re-enables the drift guard,
treat `expected_levelA.csv` / `expected_meta.csv` as **environment-dependent to ≈1e-3**
and do not tighten the Level-A tolerances without regenerating them under a recorded
environment. Do not edit these files by hand.
