# Simulations

Research prototypes that demonstrate the behavior of the scoring engine and the
corner cases. **They are not reference implementations** — they are the executable
spec: the implementation in `scoring/` must reproduce their results.

## Requirements

```
pip install numpy scipy
```

## Files

### `bridging_irt_dif.py`

End-to-end simulation over 10 civic questions, 200 reviewers (a 60/40 unbalanced
network), 1500 respondents with 30 anchor items. Covers:

- **Level A** — bridging with asymmetric regularization; the side-balanced score `S_j`
  (two sides by 2-means on `f_u`, the model's predicted approval averaged per side, the
  two averaged again; `docs/02` §A.3, D32) with its bootstrap-min and side gap; latent
  axis recovery
- **Level B** — IRT, point-biserial, DIF via logistic regression, purified via
  anchors
- **Combined verdict** of the two filters
- **Evaluators** — the leave-one-out difference score and odds weight (`docs/02` C.2,
  D33) for various profiles (follows-the-crowd, expert, partisan…), each scored against
  the mean forecast of the others
- **Corner case 1** — elite consensus (an item neutral on the political axis,
  distorted on education)
- **Corner case 2** — cost of bipartisan corruption
- **Corner case 3** — threshold sweep (the false-positive / partisan-item trade-off)

Parameters editable at the top of the file: the majority camp's share (`share_B`), the
threshold (`TAU`), the item leans.

Expected results (indicative, seed-dependent):
- latent axis recovered with correlation ~0.99
- the consensus items score `S_j` ≈ 0.83–0.86 and pass at τ = 0.80; the partisan items
  (0.53–0.56) and the mildly partisan one (0.70) drop; the intercept `b_j` is printed
  alongside for comparison
- the MES item passes bridging but is stopped by DIF (β₂ ≈ 0.7)
- "psychometric expert" → the largest difference score (+0.21, odds weight 1.9 over its
  10 scored items); "follows the peer average" (−0.36) and "partisan" (−0.26) → weights
  below 1
- elite consensus: political DIF ≈ 0, education DIF ≈ 0.66
- bipartisan corruption: ~55/80 nodes of the opposing camp are needed to pass the item
  (`S_j` 0.76 with 40, 0.83 with 55)

### `latent_dif_and_capacity.py`

Two experiments:

1. **Detecting bias without knowing which axis to look on** — latent-class IRT mixture,
   the distorting axis never observed. Shows that with ≥2 distorted items in a batch the
   model estimates delta ~1.0 on the defective ones and ~0.1 on the clean ones, and
   reconstructs the hidden axis (correlation 0.5–0.8). With a single distorted item:
   invisible. **Conclusion: validate in batches.**
2. **Sustainable proposal quota** — shows the bottleneck is pilot respondents, not
   reviewers, and that the sustainable quota is under ~1 proposal/year per node.
   Motivates the choice of a lottery (`docs/01` D10).

### `export_fixtures.py`

Regenerates the oracle fixtures consumed by the Rust acceptance tests
(`crates/scoring/tests/fixtures/`). It mirrors the two simulations above on the same
seeds and dumps the datasets and the full-fit results as CSV. Run it as:

```
python export_fixtures.py <output_dir>
```

## What these prototypes demonstrate

The structural limits documented in `docs/06-threat-model.md`:
- the true-but-divisive false negative (no threshold saves a polarizing fact) —
  motivates the appeal channel
- the elite-consensus blind spot — motivates multi-axis DIF
- the ~30% survival rate — motivates "write 3× the items you need"
- the threshold as a blade — motivates the uncertainty band
