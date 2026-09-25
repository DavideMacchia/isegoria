# Question lifecycle and protocol

Orchestration of the whole process. Each stage indicates what it prevents (the attack
it neutralizes).

```
 [1] Draft          author fills in item + mandatory primary source
      ↓
 [2] Deposit        reputation bond (not money) + hash on append-only log
      ↓
 [3] Admission      LOTTERY: from the deposited drafts, a randomly drawn subset
                    enters the pipeline (the queue does not explode)
      ↓
 [4] Review         k reviewers assigned AT RANDOM, blind, commit-reveal
      ↓
 [5] Bridging       S_j ≥ τ → passes;  S_j in band → k_extra more reviewers, re-decision
      ↓                 │
      │                 └──→ [5b] APPEAL TO EVIDENCE
      ↓                          if discarded for polarization (wide side gap), not for
      │                          a defect — below the band or failing the re-decision;
      ↓                          the author stakes reputation (`01` D27)
 [6] Pilot 1        ~300 respondents: kills broken and non-discriminating questions
      ↓
 [7] Pilot 2        ~1500–3000 respondents, IN BATCHES: IRT + multi-axis DIF
      ↓                 │
      │                 └──→ [7b] CONTESTED FACT
      │                          DIF, but the cited primary source establishes the key
      │                          (`02` §B.5): a separate pool, drawn into a test only in
      ↓                          balanced sets (`01` D38)
 [8] Active pool    usable; periodic re-validation of the whole pool
      ↓
 [9] Retirement     exposure, drift, obsolescence, emerging DIF
```

---

## [2] Deposit and [3] admission by lottery

The cost of proposing is reputation and rate limits (rate-limiting nullifier, `03`),
never money.

**Why a lottery and not a low quota** (`01` D10). The bottleneck is validation
capacity, under ~1 proposal/year per node:

- 10,000 nodes × 50 answers/month = 500,000 available answers
- ~1,500 answers per question → ~333 validatable questions/month
- a quota of 2/month would produce 20,000 questions/month: 30× the capacity, an
  infinite queue

The lottery gives equal access in expected value and keeps the queue bounded. Anyone
can propose as much as they want; each epoch a drawn subset enters the pipeline.

---

## [4] Review: random, blind, commit-reveal assignment

> **Revised by D40 (done, T57) and D41 (T37).** A panel holds at most one member of each
> cluster flagged by the coordination detector (D39, `review::assign_diverse`), the band's
> extra round included; all draws will use a beacon made by commit-reveal among consortium
> members (a threshold signature after T19).

- **Random assignment** of the `k` reviewers (odd, 7–11), stratified on the position
  `f_u` → the batch mirrors all positions of the axis. Prevents **brigading**: nobody
  chooses what to review, and the item is not searchable before the verdict. A newcomer
  — fewer than `n_min = 30` reviews on record (`02` §A.4, T39) — has the position the
  fit projects for it on the axis it does not define (the origin, with no ratings yet)
  and is a candidate like any other, at weight 0 until it is established (`01` D36): it
  fills the panel and builds its record without defining the axis.
- **Blind**: the reviewer does not see the author → prevents voting on the person.
- **Commit-reveal**: first you publish the hash of your judgment (commitment), then
  once the phase is closed you reveal it → prevents copying others and information
  cascades.
- Vote-buying neutralized: you do not know in advance what you will judge, and there
  is no verifiable receipt.

The reviewer **declares a probability** that the item passes validation (not a
yes/no): input for the evaluator score `S_u` (`02` C.2).

---

## [5b] Appeal to evidence (the key correction)

Bridging does not tell "true but divisive" from "one-sided propaganda": they produce
the same voting pattern, and no threshold saves the former (`02`, limit 1). Without an
appeal the politically most important category is systematically lost.

**Mechanism.** A question discarded at [5] *for polarization* — a wide gap between the
two sides' predicted approval (`02` §A.3), not a formal defect — whether it was scored
below the band or failed the band's re-decision (`01` D26, amended by T59), can, at the
author's initiative, **skip review and go straight to the pilot**:

- filing escrows a zero-quality pseudo-observation in the author's history, so `C_a`
  falls at once; an author whose `C_a` is below the stake floor — the prior mean of
  `02` §C.1 — cannot file (`01` D27, T61)
- if the Level B psychometrics **promote** it — to the active pool, or to the contested
  pool when its DIF concerns a sourced fact ([7b]) — the pseudo-observation is replaced
  by the item's real quality: the author gains a real, good observation, having been
  right against the opinion filter — there is no additive bonus
- if it **fails**, the zero stands — it is the item's real result — and it costs the
  next appeal until the author's average has recovered

It is the only way to recover "true but inconvenient", moving the decision from peer
judgment to the data.

---

## [6]-[7] Two-stage pilot

**Stage 1 (~300 respondents).** Cheap screen: immediately kills questions with
insufficient discrimination (`a < 0.6`, `r_pbis < 0.20`) and those with a wrong key
(negative `r_pbis`). Costs little.

**Stage 2 (~1500–3000 respondents), only for survivors.** The large sample is needed
because **latent-class DIF** (`02` B.3) requires enough people at each competence
level. 1500 suffices with a group signal; the fully-anonymous latent-class variant
wants ~3000 (see `02` §B.6 for the derivation and the minimum network size).
Requirements:

- **In batches, never a single item**: an isolated distorted question is
  unidentifiable (in testing: 1/8 invisible, 2/8 detected). Validate groups.
- **Multi-axis**: look for bias on more than one latent axis, including a
  socio-economic one (a question neutral on the political axis can be distorted on
  education).

Respondents are the scarce resource: they can be the same nodes under the third
pseudonym (`nym_answer`), or a separate panel-style sample. Each respondent proves that
pseudonym for the batch and the epoch before its answers count, and the sample floors
count those pseudonyms, not answer sheets: one person cannot fill a sample.

**Administration**: the question under pilot is mixed with already-validated ones and
the answer does not count toward the respondent's score. Whoever answers does not know
which one is on trial.

---

## [7b] Contested facts (`01` D38)

DIF does not always mean a biased item. When one camp is systematically misinformed
about a true fact, an item stating that fact shows DIF by construction. Rejecting it
would bar every fact a camp disputes from the bank, and the appeal of [5b] could never
recover it: Level B would reject it for the same reason Level A did.

**Classification, not a vote.** An item the batch flags for DIF (`02` §B.3) goes through
the source check (`02` §B.5): four steps anyone can redo from the citation committed at
deposit. If the cited primary source establishes the key, the item is a *contested
fact* and enters the contested pool; otherwise it is rejected, as before. The same rule
applies at re-validation ([8]): an active item whose DIF emerges moves to the contested
pool if its source passes the check, and retires otherwise.

**Use: only in balanced sets.** A test draws its contested facts from the beacon among
the selections whose differential test functioning stays within the tolerance (`02`
§B.7): facts leaning one way are drawn only with facts leaning the other way that cancel
them, measured in the same fit — the classes of different batches are not comparable,
so across batches their DTF only adds up. The test as a whole favours no latent class. A
contested fact is administered, counts exposure and retires at the exposure limit like
any pool item.

**Re-measurement.** The contested pool is re-validated in batches like the active pool.
A fact whose DIF has gone returns to the active pool; one still showing DIF stays,
measured on the new fit — which gives contested facts from different batches one set of
classes, where they can balance each other; one whose source no longer passes the check
(the act was amended or repealed) retires.

**Scores.** A contested fact is admitted to the bank: the reviewers who judged it are
scored as for an item that reaches the pool (`o_j = 1`, `01` D35), and an appeal that
ends in the contested pool is promoted ([5b], `01` D27).

---

## [8] Active pool and re-validation

- **Periodic re-validation of the whole pool**, the contested pool included ([7b]):
  scattered distortions add up over time until they become visible; an item accepted
  today can develop DIF as the context changes.
- **Topic coverage**: DIF removes the bias of a single item, not that of the *pool*.
  You can build a test in which every item passes DIF but the choice of topics is
  skewed. Constrain coverage upstream with a **blueprint of fixed quotas per domain**,
  itself defined by stratified sortition, not by voting.

---

## [9] Retirement and item leakage (exposure)

An item used a lot gets memorized and circulates: it loses value. Countermeasures:

- a broad pool and rotation
- **parametric** items generated from templates (same structure, different values)
- automatic retirement on exposure detection

---

## Golden items (honeypot)

> **Extended by D35 (T52, done).** Golden items stay at 5%, but alone they are too few to
> learn an evaluator's skill (about one every two epochs per reviewer). Evaluators are
> also scored on every reviewed item that reaches Level B and on a random 5% of gate
> rejections the beacon draws for the pilot, for measurement only (`protocol::exploration`:
> `Rejected → Explored → Measured`, never the pool; the outcome weighted `1/0.05` in the
> score, `02` §C.2). The explored items also measure the gate's false-negative rate.

A simple and powerful mechanism, **always active**, not only at startup. A fraction `η
≈ 5%` of the items in the review queue are of known quality (excellent or deliberately
defective: ambiguous, factually wrong, with known DIF), indistinguishable from the
rest. They give a **continuous, direct** measure of `S_u` without waiting for the
empirical cycle, and immediately catch nodes voting at random or in blocks.

**Meta-level defense.** Whoever controls the honeypots controls `S_u`. The golden
items must be produced by a committee drawn by **sortition** (stratified on the
position `f_u`, so it mirrors all positions) and rotated quickly. Sortition is the
defense against meta-level capture: they must not be choosable by anyone.

---

## Cold start (bootstrap)

- **A founder set** publicly declared, deliberately heterogeneous in orientation and
  provenance, all with identical weight `w = 1`.
- No differential weight before 30 scored outcomes per node (the probation period,
  `03` P2; `01` D36, T50 — was 200), then the shrinkage of the odds weight toward 1 as
  evidence accumulates (`02` §C.2).
- Start from a **low-political-temperature domain** (e.g. verifiable administrative
  procedures) to calibrate `τ`, `λ`, `k`, `ε` on real data before tackling hot
  questions.

---

## Meta-level governance

Everything that controls the system itself — scoring parameters, honeypot committee
composition, coverage blueprint, consortium composition — is decided by **stratified
sortition** and not by voting, with a qualified supermajority and a time delay for
changes (e.g. 2/3 + 30 days). Sortition is the recurring defense against capture by
whoever controls the rules: whoever can *choose* who tunes the system, controls the
system.
