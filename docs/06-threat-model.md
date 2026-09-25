# Threat model

## Attack surface and countermeasures

| Attack | Primary countermeasure | Ref. |
|---|---|---|
| Sybil | Enrollment layer: externally-certified uniqueness | `03` P1 |
| Whitewashing (abandon a compromised ID) | Non-rotatable ID + deterministic pseudonyms + zero-weight probation | `03` P2, M3 |
| Statistical deanonymization of the proposer | Text normalization, temporal mixing, domain quotas | `03` |
| Joining the proposals register with the votes register | Unlinkable role pseudonyms | `03` M3 |
| Brigading specific items | Random reviewer assignment; item not searchable before the verdict | `05` [4] |
| Herding / information cascades | Commit-reveal, blind judgments, no visibility of others' votes | `05` [4] |
| Majority capture | Bridging: the majority alone is not enough, cross-cutting approval is needed (on master the score still keeps part of the camp-size effect; corrected by the side-balanced score, D32/T49) | `02` A |
| Coordinated cartel | Sublinear √k discount for correlated clusters (revised: residual-correlation detection and panel diversification, D39/D40) | `02` anti-collusion |
| Long-con (reputation accumulated and spent) | `w_max` cap on the odds scale, where it binds (D33, T50); a CUSUM change detector on the per-item scores that returns the reviewer to probation at the first sustained fall, wiping the hoarded record (D34, T51); continuous honeypots | `02` C.4, `05` |
| Biased item passing peer review | Field-data DIF (does not depend on opinions) | `02` B.3 |
| Vote-buying | Random assignment + no verifiable receipt | `05` [4] |
| Author drawn onto their own item's panel | Accepted residual risk: the author's judge pseudonym is unlinkable by design, so it cannot be excluded; random assignment makes it one vote in 7–11, bridging needs agreement across camps, and honeypots measure reviewers | `05` [4], `08` §9.1 |
| Item leakage (exposure) | Broad pool, rotation, parametric items, retirement on detection | `05` [9] |
| Meta-level capture (parameters, honeypots, consortium) | Stratified sortition, supermajority + delay, freedom to fork | `05`, `04` |
| Score falsification by a signer | Deterministic reproducible computation: anyone redoes the math and unmasks it | `04` |
| Betrayal by the whole consortium | Freedom to fork: the network abandons it with all the data | `04` |
| Rewriting the past | Hourly anchoring to a public chain | `04` |
| Collusion of the issuing committee | Threshold t-of-n issuance with heterogeneous issuers | `03` M2 |

---

## Corner cases verified in testing

Results of the simulations in `sim/`. These are the engine's **acceptance tests**: the
implementation must reproduce them.

### Cost of bipartisan corruption

A 60/40 unbalanced network. A cartel pushing a partisan item:

- with majority voting: ~40 nodes from its own camp are enough → PASSES
- with bridging: 40 nodes from its own camp shift nothing; you need **70 of 80 of the
  opposing camp (87%)** to pass the item

Bridging drastically raises the cost of capture.

### The biased item that passes review

A question with strong DIF (β₂ = 0.72 in the data: item 04 of the reference fixture)
**passes peer review** — side-balanced score `S_j = 0.84` against `τ = 0.80`, both
sides approving it — and is stopped only by Level B. This is the scenario the evidence
filter exists for: human review does not see the bias, the data does.

### Latent axis recovery

Bridging reconstructs the ideological axis with correlation 0.99 against the true one,
with nobody declaring anything. Latent-class DIF reconstructs a hidden axis (education)
with correlation 0.5–0.8 without observing it, provided ≥2 distorted items are in the
batch.

---

## What the system does NOT solve (structural limits, not bugs)

These must be made explicit: they are properties of the design, not implementation
defects.

### L1 — The true-but-divisive false negative

Bridging does not tell a true, polarizing fact from one-sided propaganda: they produce
the same voting pattern, and **no threshold saves it**. This is the most serious
limitation. Partial mitigation: the appeal-to-evidence channel (`05` [5b]).

### L2 — Bias of the pool, not of the single item

You can build a test in which every item passes DIF but the choice of topics is skewed
(only parliamentary procedure, never labor law). Mitigation: a blueprint of fixed
quotas per domain, by sortition (`05` [8]).

### L3 — The elite-consensus blind spot

An item can pass bridging and political DIF yet be distorted on a socio-economic axis.
Mitigation: multi-axis DIF, which must be actively sought.

### L4 — "Competence" is still a construct chosen by someone

Psychometrics verifies that the test coherently measures *one* thing, not that that
thing is the right thing to measure.

### L5 — The consortium's initial selection has no technical solution

It is a human decision about whom to trust. Made bearable — not fatal — by
reproducibility of the computation and freedom to fork (`04`).

### L6 — The political constraint of weighted voting

In the electoral use case, weighting the vote by competence (epistocracy) is
contested: Brennan defends it (*Against Democracy*), Estlund criticizes it with the
demographic objection — even a perfectly neutral test weights participation on a
variable correlated with education, income, and native language. It is also in tension
with universal suffrage. This is not a judgment on the project but a design constraint:
if the goal is electoral, the main obstacle will not be technical.

The same infrastructure has less contested and more immediately achievable
applications (independent certifications, fact-checking, licensing exams), useful also
as a testbed, with the same underlying problem — no authority is credibly impartial —
but without the constitutional friction.
