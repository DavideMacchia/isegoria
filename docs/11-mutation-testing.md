# Isegoria — Mutation testing report (T41)

| | |
|---|---|
| **Purpose** | Measure how much of the code the tests actually *verify*, not just execute, and record every mutant that survives with the reason it is acceptable. |
| **Tool** | `cargo-mutants` 26.0.0 (the newest release that builds on the pinned rustc 1.86). |
| **Date** | 2026-09-24, branch `test/t41-mutation-survivors`. |
| **Status** | Every surviving mutant is either killed or justified below (runs 1–3 for T41, run 4 for the T48 optimizer). |

## Why this was needed

Line coverage was already ~97% (`cargo llvm-cov`), yet the second review (`docs/10`
P1.5) found defects in files covered at 96–100%. Coverage says a line *ran*; a mutant
survives when the line can be changed — a `<` into `<=`, a `+` into `-`, a function
body into `return 0` — and no test notices. The survivors are a map of the checks the
suite does not make.

## How to run

```sh
cargo install cargo-mutants --version 26.0.0 --locked
# whole workspace (~2 h on 16 cores with -j 4)
cargo mutants --workspace --features calibration -j 4
# one file, or selected mutants by name
cargo mutants -p scoring --features calibration -f crates/scoring/src/optim.rs
cargo mutants -p scoring --features calibration --re 'optim.rs:97:'
```

Each mutant runs the tests of the crate that contains it. It is too slow for every
push; run it after changing decision logic, and before a release.

## Results

| Run | Scope | Mutants | Caught | Timeout¹ | Unviable² | Missed |
|---|---|---|---|---|---|---|
| 1 | whole workspace | 1482 | 876 | 5 | 381 | **220** |
| 2 | the 17 files that had survivors, after the new tests | 1088 | 1008 | 3 | 37 | **40** |
| 3 | the 9 real survivors of run 2, after their tests | 9 | 9 | — | — | **0** |

¹ The mutant made a loop never terminate (union-find `find`, the mixture optimizer):
counted as caught. ² The mutant does not compile.

By crate, run 1: `network` and `identity` had **no** survivors; `protocol` 30;
`scoring` 188 (bridging 60, DIF 62, optimizer 23, reputation 19, …). After run 3 the
32 survivors left are all **equivalent** mutants (next section).

### Defects the survivors exposed

The work was not only adding assertions. Two survivors pointed at real problems:

1. **The bridging oracle tolerance hid fit errors.** With the gradient's data or
   regularization term corrupted, the fit still reported `Converged` and moved `b_j` by
   up to 0.009 — inside the 0.03 tolerance against SciPy, but wider than the gate's
   uncertainty band (`ε = 0.008`), so it could flip an item across `τ`. The gradient is
   now pinned against central differences, and the oracle tolerance is 0.004 (Rust and
   SciPy agree to ~0.002).
2. **A failed line search was reported as `Converged`** (`optim::lbfgs`). Two paths:
   the step halved 60 times barely moved `f` and the relative-progress stall test ran
   before the failure check; and once `x + step·d` rounded back to `x`, Armijo passed
   with `f_new == f` and no movement. Both are now `LineSearchFailed`, and a failed
   trial point that raised the cost is not taken (`docs/08` OPT-001).

Also recorded (not fixed here): the Armijo-only line search needs ~670 gradients on
Rosenbrock where SciPy needs ~46 (T45); Mantel–Haenszel with more strata than
respondents makes α infinite and classes the item C (calibration-only).

### What was added

| Area | Tests |
|---|---|
| Bridging | gradient vs central differences; objective by hand; oracle tolerance 0.004 and `Converged`; bootstrap must lower scores; empty input |
| All fits | `golden.rs` + `fixtures/golden_bits.txt`: every output of `fit`, `bridge_scores`, `mixture_dif` pinned bit-for-bit (kills initialization, RNG-order and optimizer-path mutants). Regenerate on an intended change with `ISEGORIA_UPDATE_GOLDEN=1 cargo test -p scoring --test golden` |
| Optimizer | condition-10⁴ quadratic within a SciPy-like budget; Rosenbrock to 1e-6; immediate stop at a stationary point; exact 1-D convergence; Armijo rejects a non-decreasing step; uphill gradient → `LineSearchFailed`; a failed search keeps the better point |
| Level B | hand-computed Mantel–Haenszel (one stratum, group swap, two strata, unequal strata, empty strata, no discordant cells); purification iterations and θ; 2PL `b`; point-biserial by hand; `sigmoid`/`softplus` at ±800 |
| Level C, anti-collusion | `hand_computed.rs`: author decay, weighted crowd baseline, zero weight, even/odd median, Pearson by hand, constant rows, zero-weight cluster |
| Protocol | `exact_outcomes.rs`: seats per stratum, sortition deficit fill, largest-remainder apportionment, zero shares, coverage deviation, exactly-enough items, honeypots inserted, lottery epoch mixing, `K_MIN` batch, `ExposureLimit`, accessors, one reviewer per stratum |

## Surviving mutants — all equivalent

A mutant is *equivalent* when no valid input can tell it from the original. Line
numbers are those of this branch.

| Mutant(s) | Why it is equivalent |
|---|---|
| `dif.rs:264` and `glm.rs:37` `softplus`: `>` → `>=` | At `z = 0` both branches return `ln 2` exactly. |
| `dif.rs:171`, `dif.rs:238` `d[j] * z` → `d[j] / z` | `z ∈ {−1, +1}`, so `d·z = d/z`. |
| `dif.rs:102`, `dif.rs:103` (Mantel–Haenszel) `item > 0.5`, `group > 0.0` → `>=` | Items are 0/1 and groups ±1: neither threshold value occurs. |
| `dif.rs:111` `ns > 0.0` → `>=` | An empty stratum only exists when there are more strata than respondents; then every stratum holds at most one person, which carries no discordant pair, and α is ∞ either way. |
| `dif.rs:119`, `dif.rs:121` ETS class boundaries `<` → `<=` | `Δ = −2.35 ln α` with α a ratio of counts never equals 1.0 or 1.5 exactly. |
| `validation.rs:36`, `revalidation.rs:47` `|β₂| > BETA2_MAX` → `>=`; `glm.rs:76` `max_z > SEPARATION_LOGIT` → `>=` | Equality of a fitted continuous statistic with the threshold has probability zero. |
| `bridging.rs:314` `rng < keep_frac` → `<=` (was 272) | A uniform `f64` draw equal to 0.85 exactly has probability ~2⁻⁵³ per draw. |
| `bridging.rs:328` `bj < b` → `<=` (was 286); `bridging.rs:156` `obj < best` → `<=` in the multi-start | Replacing a minimum by an equal value changes nothing. |
| `optim.rs:110` `sy > 1e-12` → `>=` | Floating equality at a continuous boundary. |
| `bridging.rs:172`, `bridging.rs:174` (`canonical_sign`) `>` → `>=`, `<` → `<=` | A tie between two `|f_j|` or a leading `f_j` of exactly 0 (then every `f` is 0 and negating gives `−0.0`, equal as a number). |
| `optim.rs:193` `i > 0` → `>=` (expansion) | At `i = 0` a value not below the start already fails sufficient decrease. |
| `optim.rs:218`, `optim.rs:219` the bracket-width stop (`hi − lo` → `hi + lo`; `1e-16 ·` → `/`) | The width stop is reached only by a failing search, where `lo` is still the start (`a = 0`): then `hi + lo = hi − lo` and `max(1, 0) = 1`. |
| `optim.rs:230` `dg · (hi − lo)` → `dg / (hi − lo)` | Product and quotient have the same sign. |
| `optim.rs:247` `\|\|` → `&&`, `optim.rs:252` `disc < 0` → `<=`, `==` | Any non-finite bracket end or negative discriminant makes the cubic minimizer NaN, which the final `is_finite` check sends to bisection anyway (a zero discriminant is a measure-zero double root). |
| `collusion.rs:32` `skip(i + 1)` → `skip(i * 1)` | Adds the diagonal pair `(i, i)`; `union(i, i)` is a no-op. |
| `collusion.rs:64` `s > 0.0` → `>=` | At `s = 0` the product is `0 · min(NaN, 1) = 0 · 1 = 0` (`f64::min` ignores NaN): same result. |
| `collusion.rs:81` (×2) `(a[i] − ma)` or `(b[i] − mb)` → `+` in the covariance term | Centring one factor is enough: `Σ(a + ma)(b − mb) = Σ(a − ma)(b − mb) + 2ma·Σ(b − mb)` and `Σ(b − mb) = 0` (same for the other factor). The variance terms, which are not equivalent, are pinned by `hand_computed.rs`. |
| `governance.rs:74` `.max(lo + 1)` → `.max(lo * 1)`; `review.rs:56` same | The guard only matters for an empty stratum, and `strata ≤ seats ≤ n` (resp. `k ≤ n`) rules that out. |
| `governance.rs:85` `count < seats` → `<=` | At equality the fill loop breaks before changing anything. |
| `review.rs:44` `n == 0 \|\| k == 0` → `&&` | Either zero makes `k = min(k, n) = 0`, and the loop draws nothing. |
| `optim.rs:60` `yy > 0` → `>=` (was line 61) | `yy = 0` means `y = 0`, so `sᵀy = 0` and the pair was never stored (`sᵀy > 1e-12`). |
| `optim.rs:85` `gd >= 0` → `<` (second check, after the steepest-descent fallback; was line 86) | The fallback direction is `−g`, whose slope `−‖g‖²` is negative unless `g = 0`, which the gradient test has already stopped on. The fallback itself is unreachable while stored pairs keep the Hessian estimate positive definite. |

## Run 4 — the T48 optimizer and multi-start

T48 replaced the Armijo line search with a strong-Wolfe one and made the bridging fit a
multi-start, so `optim.rs` and `bridging.rs` were re-run: 464 mutants, 28 survivors.
Three were real and are killed: a fit with every weight 0 produced NaN through a 0/0
start value; the sufficient-decrease sign (a flat point that *raises* the cost must be
rejected); and the bisection fallback (a cost that is infinite past a wall). The
bracketing branches of `zoom` were not exercised at all by the reference problems (with
`c₂ = 0.9` the first interpolated point is almost always accepted), so
`optim::tests::trajectories_on_reference_problems_are_pinned` pins the exact number of
evaluations and the final point, bit for bit, on problems built to reach them — a
cliff past the minimum, a steep wall, a failing search — together with Rosenbrock and an
ill-conditioned quadratic. The survivors that remain are in the table above, plus four
**path-only** mutants:

| Mutant(s) | Why it is accepted |
|---|---|
| `optim.rs:193` `i > 0` → `<`, `==` | Drops the "value rose during expansion" test (Nocedal & Wright 3.5). The search then brackets on the slope sign or accepts a Wolfe point further out: a different trajectory to a valid step, and no constructed problem made it differ. |
| `optim.rs:230` `hi − lo` → `hi + lo` (×2: `+`, `/`) | Differs only once the bracket is reversed (`hi < lo`) *and* an interior point is too steep — not reached by any reference problem. |
| `optim.rs:258`, `optim.rs:259` (×2) the interpolation margins | Matter only when the cubic minimizer falls outside the bracket, which cannot happen while the slope changes sign inside it. |

## Keeping it this way

- New decision logic gets a hand-computed or exact-outcome test, not only a range or
  an ordering check: those are what the survivors were.
- Re-run on the files you touched; a new survivor is either killed or added to the
  table above with its reason.
- The golden file changes only on purpose. Its diff is part of the review of any
  change to the engine.
