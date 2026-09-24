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
 [5] Bridging       B_j ≥ τ → passes;  B_j in band → supplementary review
      ↓                 │
      │                 └──→ [5b] APPEAL TO EVIDENCE
      ↓                          if discarded for polarization (high |f_j|),
      │                          not for a defect
      ↓
 [6] Pilot 1        ~300 respondents: kills broken and non-discriminating questions
      ↓
 [7] Pilot 2        ~1500–3000 respondents, IN BATCHES: IRT + multi-axis DIF
      ↓
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

> **Revised by D40 and D41 (T57, T37).** A panel will hold at most one member of each
> cluster flagged by the coordination detector (D39), and all draws will use a beacon made
> by commit-reveal among consortium members (a threshold signature after T19).

- **Random assignment** of the `k` reviewers (odd, 7–11), stratified on the position
  `f_u` → the batch mirrors all positions of the axis. Prevents **brigading**: nobody
  chooses what to review, and the item is not searchable before the verdict.
- **Blind**: the reviewer does not see the author → prevents voting on the person.
- **Commit-reveal**: first you publish the hash of your judgment (commitment), then
  once the phase is closed you reveal it → prevents copying others and information
  cascades.
- Vote-buying neutralized: you do not know in advance what you will judge, and there
  is no verifiable receipt.

The reviewer **declares a probability** that the item passes validation (not a
yes/no): input for the evaluator score `E_u` (`02` C.2).

---

## [5b] Appeal to evidence (the key correction)

Bridging does not tell "true but divisive" from "one-sided propaganda": they produce
the same voting pattern, and no threshold saves the former (`02`, limit 1). Without an
appeal the politically most important category is systematically lost.

**Mechanism.** A question discarded at [5] *for polarization* — i.e. with high `|f_j|`
and not for a formal defect — can, at the author's initiative, **skip review and go
straight to the pilot**:

- it costs an amount of the author's reputation (`C_a`)
- if the Level B psychometrics **promote** it, the reputation is refunded (and the
  author gains, having been right against the opinion filter)
- if it **fails**, the reputation is lost

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
pseudonym (`nym_answer`), or a separate panel-style sample.

**Administration**: the question under pilot is mixed with already-validated ones and
the answer does not count toward the respondent's score. Whoever answers does not know
which one is on trial.

---

## [8] Active pool and re-validation

- **Periodic re-validation of the whole pool**: scattered distortions add up over time
  until they become visible; an item accepted today can develop DIF as the context
  changes.
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

> **Extended by D35 (T52).** Golden items stay at 5%, but alone they are too few to learn
> an evaluator's skill (about one every two epochs per reviewer). Evaluators will also be
> scored on every reviewed item that reaches Level B and on a random 5% of gate
> rejections sent to the pilot for measurement only (weighted 1/0.05, never entering the
> pool).

A simple and powerful mechanism, **always active**, not only at startup. A fraction `η
≈ 5%` of the items in the review queue are of known quality (excellent or deliberately
defective: ambiguous, factually wrong, with known DIF), indistinguishable from the
rest. They give a **continuous, direct** measure of `E_u` without waiting for the
empirical cycle, and immediately catch nodes voting at random or in blocks.

**Meta-level defense.** Whoever controls the honeypots controls `E_u`. The golden
items must be produced by a committee drawn by **sortition** (stratified on the
position `f_u`, so it mirrors all positions) and rotated quickly. Sortition is the
defense against meta-level capture: they must not be choosable by anyone.

---

## Cold start (bootstrap)

- **A founder set** publicly declared, deliberately heterogeneous in orientation and
  provenance, all with identical weight `w = 1`.
- No differential weight before `≥ 200` judgments with known outcome per node
  (coincides with the probation period, `03` P2). **Revised by D36 (T50):** 30 scored
  outcomes, then shrinkage of the evaluator score toward zero.
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
