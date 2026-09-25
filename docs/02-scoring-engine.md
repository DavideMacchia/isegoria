# Scoring engine — mathematical specification

The engine is **deterministic**: given the same input (node×question ratings,
respondent×question answers), it produces the same output. It is the piece to
implement first; the simulations in `sim/` are its executable spec.

Notation: `u,v` nodes/reviewers; `j` questions (items); `i` respondents; `θ_i` the
respondent's latent competence.

> **Revisions from the working paper (decided 2026-09-24).** The working paper
> (`paper/`, version 0.2) found properties of this specification that `docs/01` D32–D41
> correct; the tasks are `docs/10` Phase 1.1 (T49–T57), the first phase of the roadmap.
> D32 is implemented (T49, 2026-09-24): §A.3 below describes the side-balanced score as
> built, with the amendment (appeal eligibility by the side gap) and the measured limits
> the paper does not have. Until the others land, the text describes the implemented
> behaviour, and the marked sections are superseded by the decisions: §B.3 latent-class
> DIF (D37, D38), §C.2 evaluator score (D33, D35), §C.4 temporal asymmetry and cap (D33,
> D34, D36), anti-collusion (D39, D40). `paper/README.md` lists what changed since the
> paper's snapshot.

---

## Level A — Bridging consensus

### A.1 Model

Matrix factorization with asymmetric regularization. Let `r_uj ∈ [0,1]` be node `u`'s
judgment of question `j`:

```
r̂_uj = μ + b_u + b_j + ⟨f_u , f_j⟩          f ∈ ℝ^d,  d = 1 or 2
```

- `μ` : global mean
- `b_u` : reviewer bias (individual severity/generosity)
- `b_j` : question intercept, reported by the fit; the score the gate reads is the
  side-balanced approval of §A.3 (D32)
- `f_u` : reviewer latent position (ideological axis, discovered from the data)
- `f_j` : how much the question "speaks" to that axis

Objective function, over `Ω` = the set of observed judgments:

```
L = Σ_(u,j)∈Ω (r_uj − r̂_uj)²  +  λ_b (Σ b_u² + Σ b_j²)  +  λ_f (Σ‖f_u‖² + Σ‖f_j‖²)
```

**with `λ_b ≫ λ_f`** — indicatively `λ_b = 0.15`, `λ_f = 0.03`.

### A.2 Why asymmetric regularization is the trick

By penalizing the intercepts heavily, the model is forced to explain approval *first*
through the polarization factors `⟨f_u,f_j⟩`. Only approval that **cannot** be
explained as "my faction likes it" survives in `b_j`.

- A question that one camp likes a lot → large `f_j`: the two sides' predicted approval
  differs and its mean stays low → **discarded**.
- A question approved by reviewers with opposite-sign `f_u` → both sides' predicted
  approval is high → **passes**.

### A.3 Score and threshold

**Bridge score (D32, T49): the side-balanced predicted approval.** After the fit, the
reviewers are split into two sides by a deterministic one-dimensional 2-means on `f_u`,
initialized at its minimum and maximum. For each question the model's predicted ratings
`r̂_uj` — every reviewer's, whether or not they rated it — are averaged within each
side, `A_j` and `B_j`, and the score is

```
S_j = (A_j + B_j) / 2
```

so each side counts once, whatever its size, and the score lives on the scale of the
declared probability. Accepted if `S_j ≥ τ`, with `τ ≈ 0.80` **provisional**: on the
reference simulation the consensus items score 0.83–0.86, the partisan items 0.53–0.56
and the mildly partisan one 0.70 (`sim/bridging_irt_dif.py`). Calibrate on the pilot
(T25).

*Why not the intercept.* `b_j` is relative to its batch (`Σ_j b_j = 0` at every
stationary point, with `μ` unpenalized) and partly majoritarian: its origin is a gauge
fixed only by the penalties, which at the default `λ_b / λ_f` leaves 53–87% of the
camp-size effect in the score (paper §3.3–3.4, `docs/08` BRIDGE-008/009). The
predictions depend on neither. On mirror-image partisan items the side-balanced score
leaks at most 0.1 of the camp-size effect from 200 reviewers up (`AT-BR-08`); with
50–100 reviewers — a handful of minority ratings per item — the fit shrinks `f` and a
residual leak of 0.1–0.2 remains, a fraction of the intercept's. With a minority side of
about ten reviewers the side means are noisy: on the review's dataset at 95/5 one
consensus item in eight fell to 0.78. A floor on the minority side is a calibration
item (T25).

**Polarization.** The gap `|A_j − B_j|` between the two sides is the question's
polarization. It feeds the appeal rule of `docs/05` [5b] — a question rejected with a
wide gap was rejected for polarization, not for a defect — with a provisional threshold
of 0.25, between the reference simulation's consensus items (≤ 0.02) and its mildly
partisan one (0.30). It replaces `|f_j|`, which *falls* as the camps become unequal
(`docs/08` §0-quinquies, BRIDGE-009).

**Uncertainty band (correction from testing).** Scores cluster near the threshold:
questions separated by thousandths end up one inside and one outside for pure noise.
Do not use a hard cut: define a band `[τ−ε, τ+ε]` in which questions go to
supplementary review instead of being decided by the exact value. `ε ≈ 0.02`
provisional, about three times the bootstrap spread of `S_j` on the reference fixtures
(≤ 0.006); to be tuned (T25).

*History.* Until T49 the gate read the intercept `b_j` against `τ ≈ 0.08` with
`ε ≈ 0.008`: the Community Notes reference value (0.40, on binary votes) proved
inadequate at this scale, and the useful value on the intercept was around 0.08.

### A.4 Robustness

- The objective is non-convex and has distinct local minima: a single start can land in
  a worse one, sometimes on the other side of `τ`. Fit from several deterministic
  starts (8 in the reference implementation) and keep the lowest objective; report `f`
  in a canonical sign (it is identified only up to sign).
- Run the fit on `m = 10` bootstrap subsamples (random removal of ~15% of judgments)
  and take the minimum of the side-balanced score over them, `min_s S_j^(s)`
  (pessimistic estimate): a question must pass in all repetitions.
- `d = 2` if society has more than one fracture axis (e.g. right/left + urban/rural).
  `d` chosen empirically by maximizing explained variance on historical data. Note:
  Community Notes essentially bridges on a binary axis; `d=2` is the generalization.
- Nodes with fewer than `n_min = 30` reviews do not contribute to defining the `f`
  space (only to filling it).

### A.5 Optimization

L-BFGS-B with an analytic gradient. The gradient with respect to each parameter block
is in the prototype (`sim/bridging_irt_dif.py`, function `fit`). Attention point for
reproducibility: fix the initialization seed and the iteration order. The transcendental
functions (`exp`, `ln`, `ln_1p`, `cos`, `pow`) come from one pure-Rust implementation
(the `libm` crate, `scoring::fmath`), not from the platform's libm, so the same input
gives the same bits on every platform and in every build profile (INV-7, `docs/08`
AT-BR-04).

---

## Level B — Empirical validation

Here nobody votes. You measure, on the pilot data. **Always in batches of questions,
never on a single item** (see the latent-class DIF section and
`sim/latent_dif_and_capacity.py`).

### B.1 IRT model

3-parameter model (3PL); the 2PL (without `c`) is enough for small batches:

```
P(X_ij = 1 | θ_i) = c_j + (1 − c_j) · [ 1 + exp(−a_j (θ_i − b_j)) ]⁻¹
```

- `a_j` : **discrimination** — how well the question separates those who know from
  those who don't
- `b_j` : **difficulty**
- `c_j` : **pseudo-guessing**

### B.2 Retention criteria

| Statistic | Threshold | Meaning of failure |
|---|---|---|
| `a_j` | ≥ 0.6 | the question distinguishes nothing: it is noise |
| `b_j` | −2.5 ≤ b_j ≤ 2.5 | too easy or too hard to be informative |
| `c_j` | ≤ 0.35 | answer is guessable |
| Infit/Outfit MNSQ | 0.7 – 1.3 | the question is not coherent with the construct |
| `r_pbis` (point-biserial) | ≥ 0.20 | same, classical version |

**Point-biserial**: correlation between "correct answer to this item" (0/1) and total
score on the rest of the test. If **negative**, the answer key is almost always wrong
(those who know more get it wrong more) — in testing this correctly caught an item
with an inverted key.

### B.3 DIF analysis on latent groups — the neutrality test

Establishes whether a question is politically biased **without knowing anyone's
identity or attributes**.

Idea: two people with the *same* competence `θ` but from different groups must have
the same probability of answering correctly. If they don't, the question is measuring
group membership.

**Variant 1 — logistic regression on continuous `f`** (uses the Level A latent axis):

```
logit P(X_ij = 1) = β₀ + β₁ θ_i + β₂ f_i + β₃ (θ_i · f_i)
```

- `β₂ ≠ 0` → at equal competence, the question favors one end of the axis (uniform DIF)
- `β₃ ≠ 0` → the advantage varies with competence level (non-uniform DIF)

Operational threshold: `|β₂| > 0.40` → reject. In parallel, discretizing `f` into
tertiles, Mantel–Haenszel with ETS classification:

```
Δ_MH = −2.35 · ln(α_MH)
|Δ_MH| < 1.0        → class A   accepted
1.0 ≤ |Δ_MH| < 1.5  → class B   accepted with monitoring
|Δ_MH| ≥ 1.5        → class C   REJECTED
```

**Variant 2 — latent-class IRT mixture** (independent of Level A):

> **Revised by D37 and D38 (T53–T55).** Error in the ability proxy creates spurious latent
> classes (paper §4.5): the re-check runs only when the anchors' KR-20 on the batch's
> respondents is ≥ 0.90 (`irt::KR20_MIN`, enforced by `revalidate_batch_latent` — T53,
> done), and the target model integrates `θ` with the anchors inside the likelihood (T54).
> The differential gap — each item's class shift less the batch's median shift — is
> reported as a diagnostic only (`MixtureDif::differential`): it inverts in a campaign. An
> item whose
> DIF concerns knowledge of a fact established by a primary source becomes a *contested
> fact* in a balanced pool instead of being rejected (D38).

```
P(X_ij = 1 | θ_i, g) = [1 + exp(−a_jg (θ_i − b_jg))]⁻¹
DIF_j = max_{g,h} | b_jg − b_jh |          reject if DIF_j > 1.0  (provisional, see below)
```

The population is a mixture of `G` classes with proportions `π_g`; the classes have no
label and do not need one. Estimation via EM or MCMC; `G` chosen by BIC. **This
variant is what makes DIF compatible with full anonymity**: in testing, with ≥2
distorted questions in a batch, it estimates a difficulty gap `DIF_j` of ~2.0 on the
defective ones and ~0.2 on the clean ones, and reconstructs the hidden axis with
correlation 0.5–0.8 without ever observing it.

*Threshold.* The literature value for `DIF_j` is 0.5 logit. On this estimator it is
not usable yet: with one distorted question in eight, every question's estimated gap
lands in 0.5–1.0, so 0.5 would retire the seven clean ones too. The reference
implementation therefore rejects at **1.0** on the gap, and only when the selected
fit converged and the BIC prefers a mixture (two or more classes) over one class. The value is provisional until the
false-positive / power study (`10` T24/T25) sets it (`08` DIF-006).

*Reference implementation (T40).* `G ∈ {1, …, 4}` and uniform (shared `a_j`) vs
non-uniform (per-class `a_jg`) DIF are chosen together by BIC, each candidate fitted from
several seeded starts with an analytic gradient; a class holding under 5% of the
respondents does not define `DIF_j` (its difficulties are unidentified). The verdict
reads the difficulty gap only, as above: a per-class *discrimination* gap is estimated
and reported (`a_gap`) but has no threshold yet (to be set with the uniform one by the
false-positive / power study).

**Critical requirement: validate in batches.** A single distorted question in
isolation is unidentifiable (in testing: 1 of 8 → invisible; 2 of 8 → detected). The
real threat is a *campaign* to tilt the bank, and it is that which becomes visible in
batches. Periodically re-run the analysis on the whole active pool, where even
scattered distortions add up.

**Multi-axis.** Test on more than one latent axis, including at least one that
captures the socio-economic fracture. Variant 2 surfaces it by itself: in testing a
question neutral on the political axis (DIF ≈ 0) but distorted on education (DIF ≈
0.66) was missed by looking only at the political axis. Bridging protects against
factional capture, not against elite consensus against the general public.

### B.4 Iterative purification (mandatory)

`θ` must be estimated on a set of **anchor items** already certified free of DIF,
otherwise the biased items contaminate the very measure used to judge them:

```
1. estimate θ using all items (or the anchors)
2. identify the items with DIF
3. remove them
4. re-estimate θ using only the clean items (anchors)
5. re-test all items with the new θ
6. repeat until the set of discarded items stops changing
```

In the prototype, `θ` is estimated on 30 anchor items external to the batch under
validation — enough for the reference fixtures, not for the production re-check, which
requires the anchors' KR-20 on the batch's respondents to reach 0.90 (about 40 anchors of
this design; `01` D37, T53): the standardized total is a proxy for `θ` only as reliable as
its anchors, and below that reliability the mixture reads proxy error as a latent class.

### B.5 Upstream admissibility

Psychometrics does not save you from badly conceived questions. Strict taxonomy:

- **Admitted**: verifiable procedural and institutional facts, with a **mandatory
  citation to a primary source** (Official Gazette, parliamentary act, official
  dataset).
- **Rejected by construction**: normative items ("is it right that…"), predictive,
  counterfactual, and items that attribute causes to contested phenomena.

Disputes over an item are resolved by an evidentiary procedure (comparison with the
source), not by a vote.

### B.6 Sample sizes and the minimum viable network

The `~300` and `~1500` respondent counts are not arbitrary; they come from
psychometric sample-size requirements. They are still **calibration targets** to be
confirmed with a formal power analysis before a real pilot — this section states the
reasoning and the constraints, not a proof.

**Where the two pilot sizes come from.**

- **Pilot 1 (~300)** is a cheap classical screen (proportion correct, point-biserial,
  a rough 2PL). Classical item statistics stabilize around 100–200 respondents; a 2PL
  fit wants ~250–500. `~300` is the smallest count that gives a reliable first cut —
  no larger, because respondents are the scarce resource (`01` D11).
- **Pilot 2 (~1500)** is the full IRT + DIF stage, which needs enough people *at every
  competence level and in every latent subgroup* (DIF compares people of equal `θ`):
  3PL wants ~500–1000+, and Mantel–Haenszel / logistic DIF wants ≥200 per group. `1500`
  covers this **when a grouping signal exists** (the observed group, or `f` from Level
  A).

**The anonymity ↔ sample-size tension.** The anonymity-compatible detector is the
latent-class mixture (§B.3, Variant 2), which must estimate class membership *and*
per-class item parameters without observing the group. It is far more data-hungry:
`sim/latent_dif_and_capacity.py` uses **NT = 3000**, not 1500. So:

> **1500 is optimistic for the fully-anonymous variant.** For latent-class,
> multi-axis DIF, budget ~2500–3000+ respondents per batch, rising with the number of
> axes/classes sought. Giving up observed group labels is paid for in sample size.

**Three distinct floors on network size** (person-nodes, `04`), of different natures:

1. **Evidence-filter correctness (the binding one).** Each batch needs ~1500–3000
   *distinct* respondents — not many answers from few people: DIF needs different people
   spread across competence and latent groups. Below ~1500–2000 active answerers in the
   validation window, Level B cannot run as specified. The floors are enforced on
   persons: the protocol counts the `Respond` nullifiers admitted to the batch, never the
   answer rows (T65).
2. **Level A identifiability.** The matrix factorization recovers the axis `f` only with
   enough overlapping judgments; the spec already sets `n_min = 30` reviews per node to
   enter the `f`-space (§A.4) and `k = 7–11` reviewers per item. This needs at least a
   few hundred active reviewers spanning the axis.
3. **Anonymity (a privacy floor, often forgotten).** Anonymity is a form of
   k-anonymity: you hide in the crowd. In a small network, statistical deanonymization
   (`03`: stylometry, timing, topic choice) becomes easy — a few hundred authors is not
   enough to hide 200 questions from one ID. There is a size below which the system
   *functions* but is no longer *anonymous*.

**A fourth precondition, on the anchors rather than the network.** The latent re-check
also requires the anchors the batch's respondents answered to be reliable: KR-20 ≥ 0.90
(`01` D37, T53; `pilot::admit_anchors`). Below it the batch is refused like a short
sample, because a noisy `θ` proxy is read by the mixture as a latent class (`08` DIF-010).

**Throughput** (a floor on usefulness, not correctness) follows `01` D10:
`validatable_questions/month ≈ (nodes × answers_per_node_month) / answers_per_question`.
With 10,000 nodes × 50 answers ÷ 1500 ≈ 333 questions/month. Halving the network halves
output; it does not break correctness.

| Active person-nodes | What is possible |
|---|---|
| < ~1,500 | Evidence filter not runnable as specified; only a weak Level A |
| ~2,000–3,000 | Minimum viable: one batch at a time, anonymous DIF at the edge, fragile anonymity |
| ~10,000 | ~300 questions/month, stable latent-class DIF, good k-anonymity |
| 100,000+ | Robust on throughput, multi-axis DIF, and privacy |

---

## Level C — Node reputation

Two **separate** scores, on unlinkable pseudonyms (see `01` D5). Never combined.

### C.1 Author score `C_a`

Hierarchical Bayesian model with shrinkage. Let `q_j ∈ [0,1]` be the final quality of
item `j` (a function of the Level B statistics):

```
q_j  ~  Beta( ψ_a · κ , (1 − ψ_a) · κ )
ψ_a  ~  Beta( α₀ , β₀ )              weak prior, e.g. α₀ = 2, β₀ = 3
```

Point estimate (posterior mean, with time decay):

```
          α₀ + Σ_j w_j q_j
C_a  =  ─────────────────────      w_j = exp(−Δt_j / T),  T ≈ 18 months
        α₀ + β₀ + Σ_j w_j
```

Shrinkage is indispensable: a node with 2 of 2 items accepted must not be worth as
much as one with 180 of 200. Example with `Beta(2,3)`: author A (2/2) → 4/7 ≈ 57%;
author B (180/200) → 182/205 ≈ 89%.

**Use.** `C_a` governs the **rate limit on proposals**, not the vote weight:

```
q_a = q_min + (q_max − q_min) · C_a
```

**Appeal stake (`01` D27, `05` [5b]).** An appeal is a pseudo-observation *inside* this
average, not a deduction from it: filing appends `q = 0` at `Δt = 0` (the escrow); the
verdict replaces it with the item's real `q_j` if the pilot promotes the item and leaves
it otherwise. The stake floor is the prior mean `α₀ / (α₀ + β₀)` (0.4 with `Beta(2,3)`):
an author files only while `C_a` covers it, so a failed appeal costs the next one until
the evidence has restored the average. There is no additive gain — the reward for being
right is the good observation itself (`protocol::appeal`, T61). Floor provisional (T25).

### C.2 Evaluator score `S_u` and review weight `w_u`

> **Revised by D33 (T50, done) and D35 (T52).** The ratio-form Brier skill score of the
> first design is not a proper scoring rule (paper Prop. 12): it paid a dissenter to move
> toward the crowd. Since T50 the score is the leave-one-out difference score below and
> the weight lives on the odds scale. D35 (T52) will add the live outcomes and the
> randomized exploration that make the scored items more than the golden ones.

The reviewer does not give a binary judgment: they **declare a probability** `p_uj`
that the item passes Level B empirical validation. On every scored item — a golden item
(`05` §Golden items) or, after T52, a live item whose outcome `o_j ∈ {0,1}` is known —
the forecast is scored with a **strictly proper** rule, which makes honesty the optimal
strategy whatever the crowd says:

```
S_uj = (p̄_{−u,j} − o_j)² − (p_uj − o_j)²      p̄_{−u,j} = Σ_{v≠u} w_v p_vj / Σ_{v≠u} w_v
```

`p̄_{−u,j}` is the weight-adjusted mean forecast of the *other* panelists (`01` D23's
crowd baseline minus the reviewer scored). The score is the reviewer's Brier improvement
over the crowd: positive when they are right where the crowd is wrong, exactly 0 for a
reviewer who reports the crowd's forecast, negative for noise or block voting. Its
expectation is maximized by the true belief, by exactly `Σ_j (p_uj − q_uj)²` over any
other report (`08` AT-REP-05).

**Fundamental property.** Someone who replicates the consensus gets `S_u = 0`. You
gain reputation only by being right **when the crowd is wrong**. This is the incentive
needed against majority capture.

`S_u` is the mean of `S_uj` over the reviewer's `k_u` scored items — the symmetric
long-window mean of `01` D34 (the change detector on the per-item scores is T51).

**Use.** `S_u` weights the review vote, on the odds scale and shrunk by the number of
scored items:

```
w_u = exp( γ · S_u · k_u / (k_u + k₀) )       γ ≈ 35,  k₀ ≈ 100   (provisional, T25)
w_u ← min(w_max, w_u),   w_max = 3 × median(w)  over the reviewers who carry weight
```

A crowd-level reviewer weighs 1; one reliably 0.02 better than the crowd weighs about
double; the cap binds on an outlier (it never did on a score in `(0,1)`, `08` G-12).
Shrinkage stops luck from buying weight: with 16 scored items one standard error of luck
(0.025) is worth ×2.4 without it and ×1.13 with it. A new pseudonym has weight 0 until
30 scored outcomes (`01` D36, `03` P2), then the shrinkage takes over.

*History.* Until T50 the score was the Brier skill score against the crowd's mean
forecast, `BSS_u = 1 − Σ_j (p_uj − o_j)² / Σ_j (p̄_j − o_j)²`, squashed by
`E_u = σ(γ·BSS_u)` and used as `w_u = min(w_max, E_u)`. A ratio of two sums is not an
expectation of a score: with one item the optimal report satisfies
`logit p* = logit q + 2 logit b` (paper Prop. 12) — a reviewer who believes 0.30 while
the crowd says 0.65 was best off reporting 0.60. The fixture oracle `levelc_bss.csv`
still reproduces that function (`08` REPUTATION-002); on it the difference score tells
the same story (the expert beats the crowd, the followers do not) and stays proper.

### C.3 Judgments without verifiable truth

For dimensions that never receive an empirical verdict (formal clarity, source
quality, tone), there is no `o_j`. A **peer-prediction** mechanism with correlated
agreement (Dasgupta–Ghosh), incentive-compatible without ground truth. For reviewer
`p` and reference reviewer `q`:

```
Score(p) = 1[ x_p(shared_item) = x_q(shared_item) ]
         − 1[ x_p(t_1) = x_q(t_2) ]          t_1, t_2 non-shared items
```

The second term subtracts baseline agreement (chance or common bias). Reporting the
honest signal is the highest-payoff equilibrium among symmetric strategies.

Richer variant: **Bayesian Truth Serum** (Prelec). Each reviewer declares (a) their
own judgment, (b) the expected distribution of the others. The **surprisingly common**
answer is rewarded — more frequent than the group predicted. It extracts information
from the informed minority.

### C.4 Temporal dynamics and cap

> **Revised by D33, D34 and D36 (T50, T51 — done).** The cap applies on the odds scale,
> where it binds, probation lasts 30 scored outcomes, and the asymmetric update of the
> first design is replaced by the symmetric mean of §C.2 plus a change detector.

- **Change detector (`01` D34).** The weight reads `S_u`, the mean of the reviewer's
  per-item scores (`probation::SkillTrack`). Against that mean, a one-sided CUSUM on the
  per-item scores watches for a sustained *drop* — a reviewer who has built a reputation
  and starts spending it:

  ```
  s ← max(0, s + (S_u − S_uj) − k)        alarm when s > h;   k = 0.03, h = 1.5 (provisional, T25)
  ```

  It runs only once the reviewer is out of probation (a mean over few items is no
  reference). On an alarm the reviewer returns to probation: the mean, the count and the
  statistic restart, so the weight is 0 until 30 new scored outcomes and shrunk again
  afterwards. It reacts to a change, not to variance: a cautious reviewer with noisy
  scores around a good mean raises nothing. In the paper's simulation, `k = 0.03`,
  `h = 1.5` give 0.07 false alarms per 1,000 scored items and catch a reviewer who starts
  flipping 20% of forecasts after a median of 36 items (`08` AT-REP-07).
- `w_max = 3 × median(w)`, a hard cap recomputed each epoch over the reviewers who
  carry weight (founders at 1 and established reviewers at their odds weight, not
  probationers at 0; `orchestrator::epoch_weight_cap`). Limits the damage of a single
  event.

*History.* Until T51 `E_u` rose slowly and fell fast (an asymmetric moving average),
meant to make the long-con attack unprofitable. It penalized variance, not error: its
stationary level sat far below the true mean, and a cautious reviewer who beat the crowd
(true +0.009) was held at −0.061 while a crowd copier stayed at 0 (paper §5.5).

---

## Anti-collusion (sublinear discount)

> **Superseded for the protocol by D39 and D40 (T56, T57).** Within an epoch two reviewers
> share under one item (paper §6.3), and raw correlations cannot separate a cartel from
> like-minded honest reviewers. Detection will use the correlation of model residuals over
> long histories (≥ 30 shared items, permutation null), and a detected cluster will limit
> panel assignment (at most one member per panel) instead of losing weight.

The individual cap does not stop a cartel of coordinated nodes. Correlation is
penalized.

```
1. correlation matrix ρ_uv between the historical judgment vectors
2. clustering (spectral on 1−|ρ|, or distance in f_u)
3. sublinear group weight:   W(G) = ( Σ_{u∈G} w_u )^α,   α ≈ 0.5
```

A coalition of `k` nodes voting identically counts as `√k`: 500 coordinated ≈ 22
independent. Genuinely independent nodes end up as singletons and are not discounted.

Accepted cost: it also penalizes genuine agreement. The system is tuned
conservatively.

---

## Initial parameters

| Parameter | Value | Notes |
|---|---|---|
| `λ_b / λ_f` | 0.15 / 0.03 | ratio ≈ 5:1, recalibrate |
| `τ` (bridging threshold, on `S_j`) | ~0.80 (provisional, D32) | absolute, on the probability scale; **calibrate on the pilot**, not fixed |
| `ε` (uncertainty band) | ~0.02 (provisional) | questions in the band → supplementary review; ≈ 3× the bootstrap spread of `S_j` |
| `γ_appeal` (side gap for appeal) | 0.25 (provisional) | a rejected question with a wider gap was rejected for polarization: appealable (`05` [5b]) |
| `d` (factors) | 1 → 2 | start from 1 |
| `k` (reviewers/item) | 7–11 | odd, random assignment stratified on `f_u` |
| `N` pilot stage 1 | ~300 | cheap classical screen (see §B.6) |
| `N` pilot stage 2 | ~1500–3000 | 1500 with a group signal; ≥3000 for latent-class DIF (§B.6) |
| `a_min` | 0.6 | minimum discrimination |
| `\|β₂\|` max DIF | 0.40 | logistic regression |
| `DIF_j` max (latent classes) | 1.0 logit (provisional; literature 0.5) | IRT mixture; see §B.3 |
| `Δ_MH` max | 1.5 | ETS class C = reject |
| `α` (cluster discount) | 0.5 | square root |
| `w_max` | 3× median | individual cap |
| `T` (reputation half-life) | 18 months | |
| `η` (honeypot rate) | 5% | see `05` |

---

## What the engine does NOT solve (structural limits from testing)

1. **The true-but-divisive false negative.** Bridging does not tell a true,
   polarizing fact from one-sided propaganda: they produce the same voting pattern,
   and **no threshold saves it**. This is the most serious limitation. Mitigation: the
   appeal-to-evidence channel (`05`), not a change to the engine.
2. **The elite-consensus blind spot.** A question can pass bridging and political DIF
   yet be strongly distorted on a socio-economic axis. Mitigation: multi-axis DIF
   (B.3), which must be actively sought.
3. **~30% survival rate.** In testing, 3 of 10 questions reach the pool. Write ~3× the
   items you need.
4. **The threshold is a blade.** See the uncertainty band (A.3).
5. **Human judgment predicts validity poorly.** Level A is in effect anti-spam against
   partisan questions, not a quality indicator. The real verdict is Level B.
