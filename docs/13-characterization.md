# Isegoria — Characterization studies (T24)

| | |
|---|---|
| **Purpose** | The specification of T24: the simulation studies that measure how the production detectors and gates behave, how to run them, and what counts as done. |
| **Derived from** | `docs/10` T24 and §1.4; `docs/08` §5.3, §12 (AT-DIF-01..09, AT-BR-02), §16.1 (SC-2, SC-3, SC-5, SC-7); `docs/07` §12–§14; the working paper's designs (`paper/scripts/common.py`). |
| **Status** | Specified and harness built (2026-09-26, `crates/characterization`). The full run is pending: it runs on the owner's machine, and §7 receives its results. The thresholds are set afterwards, from §7, by T25. |

## 1. What T24 measures, and what it does not

Every threshold of the mechanism is provisional until it is measured (`docs/10` §1.4,
`docs/07` §13). T24 measures the **production** estimators and gates on synthetic
populations with a known truth, over the regimes a deployment will meet, and reports rates
with intervals. It sets no threshold: T25 reads the tables of §7 and sets them.

| Study | Question | `docs/08` |
|---|---|---|
| `dif-null` | How often does the latent re-check flag a clean item, by sample size, batch size and anchor reliability? | AT-DIF-01, DIF-008, the KR-20 floor (T53) |
| `dif-power` | How often does it flag a biased item, by shift, number of biased items, class balance, sample and batch size? | AT-DIF-02, SC-2, STAT-001 |
| `dif-misspec` | Does a real ability gap between the classes (impact) or guessing on multiple-choice items create or hide DIF? | `docs/07` §14, D25 |
| `dif-nonuniform` | Is DIF in the discrimination found, and where would a cut on `a_gap` sit? | AT-DIF-03, T40 |
| `dif-two-axes` | Is bias on two independent hidden axes found? | AT-DIF-04 |
| `dif-poisoning` | What share of coordinated respondents creates DIF on a clean item, or hides it on a biased one? | AT-DIF-07, SC-7 |
| `dif-pool-scale` | What does a batch of 32 or 100 items cost? | AT-DIF-09, DIF-009 |
| `dtf-error` | How far is the fitted DTF of a set of contested facts from its true DTF? | DIF-011 (T55) |
| `bridging-sweep` | How well does the side-balanced score recover the design's quality, and how often does the gate pass what it should not? | SC-3, BRIDGE-002, BRIDGE-003 |
| `bridging-capture` | How many boosters from the camp a partisan item disfavours does it take to pass it? | AT-BR-02, BRIDGE-005 |

**Not in T24.** AT-DIF-05 (one rule on one metric) is a consistency fix: T35 settled the
metric, T25 sets the value. AT-DIF-06 (a separated item) and AT-DIF-08 (oscillating
purification, Variant 1, calibration-only) are constructed cases, deterministic tests
rather than studies. The Level C parameters — the CUSUM `k`/`h`, the weight cap, `γ`,
`N_PROBATION`, the exploration rate — get their calibration procedures in T25. Real
response data exist only in pilots (T27): everything here is inside the simulated regime,
and §7's statements say so.

## 2. The harness

`crates/characterization` is a workspace member but not part of a node: it depends on
`scoring` and `protocol` and nothing depends on it. One binary:

```sh
cargo run --release -p characterization -- plan                 # studies, cells, runs
cargo run --release -p characterization -- run --grid smoke     # one tiny cell per study
cargo run --release -p characterization -- run --replicates 20  # a first pass: 10% of the grid
cargo run --release -p characterization -- run                  # the full grid of §4
cargo run --release -p characterization -- summarize            # summary.md and CSV tables
```

| Option | Meaning | Default |
|---|---|---|
| `--grid full\|smoke` | the grid of §4, or one tiny cell per study | `full` |
| `--study NAME[,NAME…]` | the studies to plan or run | all |
| `--replicates R` | the first `R` replicates of each study, at most its own count | the study's |
| `--filter TEXT` | only the cells whose key contains `TEXT` | none |
| `--jobs J` | worker threads | the number of cores |
| `--out DIR` | where the records go | `characterization-results` |
| `--quiet` | no progress lines | off |

**Records.** Each run appends one line to `<out>/<study>/records.csv` as it completes: the
study, the cell's key, the replicate, the seed, the time, and the outcome (§4). Lists
inside a field are `;`-separated; numbers are written in their shortest exact decimal
form, so a record reads back to the same bits. `summarize` writes `<out>/summary.md` and
the CSV tables of §5 beside the records; `run` summarizes when it ends.

**Seeds and reproducibility.** A run's seed is the first eight bytes of SHA-256 over
`isegoria/characterization/v1`, the study's name and the cell's key (each
length-prefixed) and the replicate. The run draws its population from a ChaCha8 stream
on that seed, with the transcendental functions of the pure-Rust `libm` the engine uses
(`scoring::fmath`), and fits it with the engine, whose bits do not depend on the
platform (INV-7, AT-BR-04). The engine's own random starts take a second seed, SHA-256
over `isegoria/characterization/engine/v1` and the run's seed, so they owe nothing to the
population's stream, as in production, where the seed does not depend on the data. So every record reproduces from its study, cell and replicate
on any machine, with any number of workers (`tests/harness.rs`): a surprising cell can be
re-run alone with `--study` and `--filter`.

**Interruptions.** The runs are ordered replicate by replicate, so a partial run covers
every cell evenly. Stopping the process (Ctrl-C, a reboot) loses at most the runs in
flight: at the next `run` a torn last line is cut off, the recorded runs are skipped and
the rest continue. `--replicates 20` followed later by a plain `run` adds replicates 20
to 199 to the first twenty. A run that panics is written to `<out>/errors.log`, which
lists the failures of the latest `run` only, and left unrecorded, so the next `run`
retries it. One process per output directory; a study named twice runs once.

**Fidelity to production.** The DIF studies fit `scoring::latent::latent_dif` with the
production settings and read the verdict with `protocol::revalidation::target_flags`, the
rule of `revalidate_batch_latent`. The gates are recorded, not applied: `admitted` is
`K ≥ K_MIN`, `N ≥ N_LATENT_MIN` and KR-20 ≥ `KR20_MIN`, so the study also measures what
the gates refuse. The rates are over all runs — the engine's behaviour — and every DIF
table gives the share production would admit; `summary.csv` also gives the batch, power
and clean-item rates over the admitted runs alone, which `dif-null` and `dif-misspec`
show (guessing brings the anchors' KR-20 to the floor). `tests/production.rs` checks that an admitted batch gets the flags of
`revalidate_batch_latent` and a refused one is recorded as refused. The bridging studies
compute the gate's robust score as the epoch does (`bridge_scores` with 10 bootstrap
subsamples keeping 85% of the ratings) and read it with `gate::bridging_gate` at the
production `τ`, `ε` and `γ_appeal`.

**Cost.** The DIF studies dominate: a fit is single-threaded and its time grows with the
sample, the batch and the number of classes the BIC tries. Measured in the release
profile on this repository's 4-core development container (one run per cell):

| Cell | Time of one run |
|---|---|
| `dif-null`, N = 1,500, K = 4 | 7 s |
| `dif-null`, N = 3,000, K = 8 | 15 s |
| `dif-null`, N = 6,000, K = 16 | 68 s |
| `dif-power`, N = 3,000, δ = 0.5, two biased items | 20 s |
| `dif-power`, N = 6,000, δ = 0.9, three biased items, π = 0.1 | 81 s |
| `dif-two-axes`, N = 6,000, δ = 0.9 | 148 s |
| `dtf-error`, N = 6,000, δ = 0.9, π = 0.3 | 75 s |
| `bridging-sweep`, 800 reviewers | 2 s |
| `bridging-capture`, item 09 (25 steps) | 5 s |
| `dif-pool-scale`, K = 32 / K = 100, no leaning item | 32 s / 27 s |

The full grid is 51,212 runs and about 400 CPU-hours, more than half of it in `dif-power`
and most of that in its cells at N = 6,000: about a day on 16 cores. A first pass with `--replicates 20` takes a tenth of that and
already gives every cell's point estimates; the intervals of §7 need the full count.

**On the owner's machine.**

1. `git checkout feat/t24-characterization` (or the commit it was merged at), then
   `cargo run --release -p characterization -- run --grid smoke --out smoke-check`: it
   takes under a minute and checks the build and the machine.
2. `cargo run --release -p characterization -- run --replicates 20`, then look at
   `characterization-results/summary.md`.
3. `cargo run --release -p characterization -- run` for the rest; stop and restart it
   freely.
4. Send back `summary.md`, the `summary.csv` files, `thresholds-*.csv`, `tau.csv` and
   `curve.csv`, with the commit (`git rev-parse HEAD`) and the machine. The records stay
   with the owner: every one of them reproduces from its seed.

## 3. The populations

### 3.1 Latent-DIF batches

Every `dif-*` study and `dtf-error` draws a batch of `N` respondents, `A` anchors and `K`
trial items — the paper's `dif_generate` (`paper/scripts/common.py`), extended:

- anchors: `a ~ U(0.9, 1.6)`, `b ~ N(0, 1)`; trial items: `a_j ~ U(1.0, 1.5)`,
  `b_j ~ 0.6 · N(0, 1)`;
- each respondent: a class `z₁ = +1` with probability `π`, else `−1`; an independent
  second class `z₂ = ±1` with probability ½; ability `θ ~ N(0, 1) + impact · [z₁ = +1]`;
- an anchor is answered correctly with probability `c + (1 − c) σ(a(θ − b))`, `c` the
  guessing floor;
- trial item `j` has signs `s₁ⱼ, s₂ⱼ ∈ {−1, 0, +1}` on the two axes; with the lean
  `ℓ = s₁ⱼ z₁ + s₂ⱼ z₂`, it is answered correctly with probability
  `c + (1 − c) σ((a_j + α/2 · ℓ)(θ − b_j − δ ℓ))`: a difficulty gap `2δ` and a
  discrimination gap `α` between the classes of a leaning item.

The layout sets the signs: a *campaign* of `n` items (`s₁ = +1` on the first `n`), a
*mirror* (`+ − + −` on the first four), or *two axes* (`n` items on each). Coordinated
respondents are the last `round(fraction · N)` of the sample: *injecting* ones answer the
last `targets` trial items wrong and the rest honestly; *masking* ones answer the leaning
items as if they leaned on nothing. Each record carries the items' roles: `+` and `-`
lean on the first axis, `2` on the second, `t` is a clean target of an injection, `c` is
clean. With `π = 0.5` and the extensions at zero the population is the paper's; its
random stream is not (the order of draws is fixed by `generate::dif_batch`).

### 3.2 The Level A mirror design

`bridging-sweep` draws the paper's mirror design (`mirror_design`) with the consensus
items spread across the threshold: `n` reviewers in two camps, the majority's share
`share`, positions `±1 + 0.25 · N(0, 1)`, a severity `0.06 · N(0, 1)` each; ten consensus
items of quality `q ~ U(0.70, 0.95)` and no lean, ten partisan items of quality 0.55 in
mirror pairs leaning `±0.8`; each reviewer rates `per_reviewer` items at random, the
rating `clamp(q + 0.45 · position · lean + severity + noise · N(0, 1), 0, 1)`. An item's
*truth* is what the side-balanced score estimates: the mean over the two camps of each
camp's mean expected rating, the clamp included (`E[clamp(X, 0, 1)]` for a normal `X`,
in closed form). It is `q` for a consensus item away from the bounds and a little below
it near the top (0.91 at `q = 0.95` with noise 0.15); a partisan item's is about 0.55.
An item should pass when its truth is above `τ` and fail when it is below.

### 3.3 The capture design

`bridging-capture` uses the reference simulation's dataset (the fixtures of
`crates/scoring/tests/fixtures`, 200 reviewers in camps of 80 and 120, ten items). On a
partisan item (08 favours the majority camp, 09 the minority one), `own` reviewers drawn
from the camp the item favours and then the reviewers of the other camp, in a drawn
order, rate it 1.0; the gate's score is read at every `step` of the opposing count. The
item *passes* at the first opposing count at which its robust score reaches `τ + ε`, and
*reaches the band* — supplementary review — at the first at which it reaches `τ − ε`.

## 4. The studies

Two hundred replicates per cell, three for `dif-pool-scale`; 51,212 runs. Unless a row
says otherwise a DIF batch has `N = 3,000`, 60 anchors (KR-20 ≈ 0.93), `K = 8`, `π = 0.5`,
no impact, no guessing, no attack.

| Study | Cells | Factors |
|---|---|---|
| `dif-null` | 27 | `N ∈ {1500, 3000, 6000}` × `K ∈ {4, 8, 16}` × anchors `∈ {20, 40, 60}`; no leaning item |
| `dif-power` | 132 | at `K = 8`: `N ∈ {1500, 3000, 6000}` × `δ ∈ {0.3, 0.5, 0.7, 0.9}` × biased items `∈ {1, 2, 3}` × `π ∈ {0.5, 0.3, 0.1}`; at `N = 3000`: `K ∈ {4, 16}` × `δ ∈ {0.5, 0.9}` × biased `∈ {1, 2, 3}` × `π ∈ {0.5, 0.3}` |
| `dif-misspec` | 22 | impact `∈ {0, 0.5, 1.0}` × guessing `∈ {0, 0.2}` × biased items `∈ {0, 2}` (`δ = 0.9`) × `π ∈ {0.5, 0.2}`, `π` varied only where it matters |
| `dif-nonuniform` | 8 | `N ∈ {3000, 6000}` × `α ∈ {0.4, 0.8}` × `δ ∈ {0, 0.5}`, two leaning items |
| `dif-two-axes` | 4 | `N ∈ {3000, 6000}` × `δ ∈ {0.5, 0.9}`, two items on each axis |
| `dif-poisoning` | 15 | injecting: `fraction ∈ {0, 1, 2, 5, 10%}` × targets `∈ {1, 2}` on a clean batch; masking: `fraction ∈ {0, 1, 2, 5, 10%}` on two items with `δ = 0.9` |
| `dif-pool-scale` | 4 | `K ∈ {32, 100}`, no leaning item or a tenth of the items leaning with `δ = 0.9` |
| `dtf-error` | 8 | `N ∈ {3000, 6000}` × `δ ∈ {0.5, 0.9}` × `π ∈ {0.5, 0.3}`, the mirror layout |
| `bridging-sweep` | 36 | `n ∈ {100, 200, 800}` × `share ∈ {0.5, 0.6, 0.8}` × `per_reviewer ∈ {5, 9}` × `noise ∈ {0.07, 0.15}` |
| `bridging-capture` | 4 | item `∈ {08, 09}` (keys `item=7`, `item=8`, counted from 0) × `own ∈ {0, 40}`, opposing boosters in steps of 5 |

**What a run records.** A DIF run: the anchors' KR-20, `admitted`, the selected number
of classes, uniform or not, convergence, the BIC gain, the class shares and means, per
item `DIF_j`, `a_gap` and the production flag, and the roles. A `dtf-error` run: the fit's
classes, convergence and flags, and for eight item sets of the mirror layout — each
leaner of the first pair, the two mirror pairs, a same-side pair, the four leaners, the
four clean items, a pair with two clean items — the DTF of the fitted curves
(`ClassCurves::of`) and of the true ones on the same 41-node grid. A sweep run: the axis
recovery `|corr(f_u, true position)|`, convergence, and per item `q`, the lean, the
truth, the full and robust scores, the side gap and the gate's outcome. A capture run: per step the opposing count, the full
and robust scores, and the item's plain mean rating.

## 5. Statistics

- **Over runs** — batches with a clean item flagged, all biased items flagged, items that
  never pass — a proportion with its 95% Wilson interval; runs are independent.
- **Over items** — clean items flagged, power per biased item — the pooled rate with a
  Wilson interval on the effective sample size `p(1 − p) / Var(p̂)`, the variance of the
  pooled rate estimated from the batches (the ratio estimator over batch totals), kept
  between `(Σm)² / Σm²` — every batch counted once, the fewest — and `Σm`, every item
  counted once; the fewest when the rate is 0 or 1 or there is one batch. The items of one
  batch share one fit, so they are not independent trials.
- **Batches with a clean item flagged** counts the items marked `c` only: a leaner or an
  injection's target does not make a batch a false positive.
- **Engine and production.** Every DIF table gives the rates over all runs; the
  admitted column and the admitted-batch rate restrict them to the runs the gates admit.
- **Threshold tables.** The flags at other cuts follow the production rule — a converged
  fit with two or more classes, a gap above the cut: `thresholds-dif-cut.csv` gives, for
  cuts 0.5–1.5 on `DIF_j`, the clean-item rate on admitted null batches and the power of
  every `dif-power` cell with two biased items of eight, `π = 0.5`, `N ≥ 3000`;
  `thresholds-a-gap.csv` the same for cuts 0.2–1.0 on `a_gap`, with the pure non-uniform
  cells of `dif-nonuniform`; `bridging-sweep/tau.csv`, for `τ` from 0.70 to 0.90, the
  share of the items whose truth is below `τ − 0.05` that the robust score passes and of
  those whose truth is above `τ + 0.05` that it fails, per reviewer count.
- **DTF.** Over the runs whose fit converged with two or more classes — the fits whose
  flags could put facts in the pool — the error is the fitted minus the true DTF. A
  set is falsely admitted when its fitted DTF is within `DTF_MAX` and its true one is
  not: the tables give that share of the fitted runs, and the share of the sets truly
  over the tolerance that the fitted value admits.
- **Quantiles** are type 7 (linear); an item that never passes counts as infinite, and a
  quantile that reaches it reads "never".

## 6. Done when

1. The full grid ran on one commit of the harness — every one of the 51,212 runs
   recorded, the last `run` with no `errors.log` — and `summarize` ran on the records.
2. §7 holds the summary's tables with the commit, the date, the machine and the wall
   time; the CSV tables are committed under `verification/reports/t24/` (`docs/07` §24);
   the records stay with whoever ran them, reproducible from their seeds.
3. Each study's result is stated in the form `docs/07` §14 asks for — under these
   assumptions, on these populations, this sensitivity and this specificity, and outside
   them nothing established — and `docs/08` is updated: AT-DIF-01, 02, 03, 04, 07, 09 and
   AT-BR-02 in §12; DIF-008, STAT-001, BRIDGE-002, BRIDGE-003, BRIDGE-005 and DIF-011 in
   §15, raised to SCIENTIFICALLY CHARACTERIZED where the tables support it and inside
   their regime only.
4. T25 is then unblocked: it sets the DIF cut, a cut on `a_gap` or none, `KR20_MIN`, the
   sample floors, `DTF_MAX`, `τ` and `ε` from §7, with the calibration procedure
   `docs/07` §13 requires.

## 7. Results

Pending the full run (§2, "On the owner's machine").
