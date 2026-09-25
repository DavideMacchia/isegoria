# Overview

## The real problem

If the quality of a question is decided by voting, whoever controls the majority of
votes controls the questions: power shifts from the government to the largest or
best-organized faction. A naive reputation system (whoever gets more upvotes rises)
makes things worse, because it concentrates power in a cohesive group faster than
plain voting does.

The design rests on three pillars, in order of importance:

1. **Sybil resistance** — if creating identities is cheap, no reputation math has
   any value. It is the constraint that holds up all the rest; it is solved by the
   enrollment layer (`03`).
2. **Non-opinion quality signals** — you need at least one source of truth that is
   not anybody's judgment. In our case one exists: psychometrics on real answer data
   (`02`, evidence filter).
3. **Bridging aggregation, not majority** — a question is accepted only if it is
   approved *across* the lines of division, not if it collects more votes than the
   opposing front (`02`, bridging).

## Two-level architecture

| | Level A — Opinion | Level B — Evidence |
|---|---|---|
| Input | Peer reviews from nodes | Real answers from samples |
| Question | "Does this question look good?" | "Does this question behave well?" |
| Attackable? | Yes, with coalitions | Very hard: requires corrupting the sample |
| Role | Upstream filter | Final verdict |

The system's center of gravity is Level B: peer review only decides *which questions
are worth field-testing*. The final decision is made by statistics computed on
answer data, not by votes. This moves the system from "what the network thinks" to
"what the question does", which is much harder to manipulate.

## The three actions of a node

Every node (person) can take three distinct actions, each under a **separate
pseudonym not linkable to the others**:

- **Propose** — write a new question.
- **Judge** — evaluate other people's questions during review.
- **Answer** — act as a sample for the trial: answer quizzes in which the new
  question is mixed with already-validated ones, without the answer counting toward
  one's own score.

Propose and judge were the two roles envisaged from the start. Answering emerged as
a need of the evidence filter: someone has to answer for a question to be trialled.
It is convenient for the same nodes to do it, provided the system does not know it is
the same person.

## Lifecycle of a question

```
 [1] Draft          author fills in item + mandatory primary source
      ↓
 [2] Deposit        reputation bond (not money) + hash on the log
      ↓
 [3] Review         k reviewers assigned AT RANDOM, blind,
     (peer)         commit-reveal, nobody sees others' votes
      ↓
 [4] Bridging       B_j ≥ τ  →  passes;  otherwise discarded
      ↓                 │
      │                 └──→ appeal to evidence (if discarded for polarization)
      ↓
 [5] Pilot 1        cheap screen on ~300 respondents: immediately kills
                    broken and non-discriminating questions
      ↓
 [6] Pilot 2        ~1500–3000 respondents: IRT + DIF on latent axes
                    (in batches, never a single item)
      ↓
 [7] Active pool    usable; periodic re-validation of the whole pool
      ↓
 [8] Retirement     for exposure, drift, obsolescence, or emerging DIF
```

Nobody chooses what to review (prevents brigading). Nobody sees the author (prevents
voting on the person). Nobody sees others' judgments before casting their own
(prevents information cascades).

The **[4]→appeal** branch is the most important correction to emerge from testing: a
true but politically divisive question produces the same voting pattern as one-sided
propaganda, and bridging wrongly discards it. The appeal channel lets the author send
it straight to the pilot, staking their own reputation: a zero-quality observation
enters their average when they file, and the item's real quality replaces it if the
psychometrics promote it (`01` D27). It is the only way to recover the "true but
inconvenient" category, politically the most important. Details in `05`.

## Node types (network layer)

Three infrastructural roles, not to be confused with the three actions of people:

- **Person-nodes** — the pseudonymous participants. Number: unlimited, the more the
  better (security grows with the count).
- **Signer-nodes** (consortium) — a few dozen always-on machines, heterogeneous and
  in different jurisdictions, that keep a full copy and sign the state. Number:
  limited by the coordination cost (~n²). Security comes from the diversity of who
  controls them, not from the number.
- **Light-nodes** — anyone can run one: it keeps only the slice of data it needs and
  verifies signatures without taking part in the agreement. Number: unlimited, slows
  nothing down.

Details in `04`.
