# Isegoria — Verification, Assurance & Falsification Methodology

**Status:** proposed repository methodology  
**Purpose:** make Isegoria auditable even when implementation work is performed with substantial AI assistance.

---

## 1. Purpose

Isegoria is complex across several disciplines: psychometrics, statistics, cryptography, privacy, identity, incentive design, distributed systems, and software engineering.

The central engineering risk is not merely that an AI can introduce a coding bug. A more dangerous failure mode is:

> a plausible implementation is accepted because no one has an independent way to distinguish a correct solution from a persuasive but incorrect one.

Therefore the project must not rely on the competence, confidence, or self-review of any single implementer, human or AI.

The governing principle of this document is:

> **No important property is accepted because an AI says it is correct. It is accepted only when the property has an explicit statement, explicit assumptions, an attack/falsification strategy, executable evidence where possible, and a clearly recorded residual risk.**

This document defines the process used to achieve that.

---

## 2. Scope

This methodology applies to all security-, privacy-, statistical-, protocol-, and correctness-critical properties of Isegoria, especially:

- identity and uniqueness;
- unlinkability and pseudonym derivation;
- anti-Sybil properties;
- reputation and incentives;
- bridging;
- IRT and DIF;
- anti-collusion;
- question admission and lifecycle;
- append-only logs and integrity;
- consortium checkpoints;
- replication and convergence;
- cryptographic integrations;
- network transport;
- adversarial resilience;
- reproducibility;
- simulation validity.

It also applies to any future major subsystem added to the project.

---

## 3. The assurance model

Every important claim should be represented as:

```text
CLAIM
  ↓
ASSUMPTIONS
  ↓
THREAT / FAILURE MODEL
  ↓
ATTACK OR FALSIFICATION TEST
  ↓
IMPLEMENTATION
  ↓
EVIDENCE
  ↓
INDEPENDENT REVIEW
  ↓
RESIDUAL RISK
```

A claim without evidence is a hypothesis.

A claim with only unit tests is not necessarily a system-level guarantee.

A claim involving cryptography is not considered production-secure merely because the implementation passes tests.

---

## 4. The Claim → Evidence model

For each security or scientific property create an entry in a verification matrix.

Recommended schema:

| Field | Meaning |
|---|---|
| ID | Stable identifier, e.g. `ID-UNIQUENESS-001` |
| Claim | Exact property being asserted |
| Scope | Component and lifecycle stage |
| Assumptions | Conditions under which the claim is expected to hold |
| Adversary / failure model | What an attacker, faulty node, or bad dataset may do |
| Required invariant | Precise condition that must remain true |
| Falsification method | How we try to break the claim |
| Automated test | Executable test, property test, simulation, fuzz case, etc. |
| Independent oracle | External implementation, mathematical derivation, reference dataset, or second implementation |
| Human review | Discipline required for external validation |
| Evidence | Test report, benchmark, proof, paper, review, trace, etc. |
| Status | `hypothesis`, `implemented`, `tested`, `independently-checked`, `production-ready` |
| Residual risk | What remains unproven |
| Last reviewed | Date / commit |
| Owner | Responsible maintainer |

The matrix is the project's central epistemic record: it distinguishes what is known from what is merely believed.

---

## 5. Separate specification from implementation

A specification must state **what must be true**, not only how the current code happens to work.

For every subsystem maintain:

### A. Property specification

Example:

```text
P-ID-001:
A single real participant cannot obtain two simultaneously valid
active pseudonyms for the same role within the same protocol epoch.
```

### B. Assumptions

```text
A-ID-001:
The external identity source correctly enforces one-person/one-registration.
The issuer committee contains fewer than the threshold number of malicious members.
The underlying cryptographic primitive is secure under its stated assumptions.
```

### C. Implementation

Only after A and B are frozen should the implementation be evaluated.

This prevents a common AI failure mode:

> the implementation silently changes the property because the original requirement was difficult to implement.

---

## 6. Never use self-review as primary evidence

The same AI instance that proposes a design must not be the only source that validates it.

For important changes, use role separation:

### Designer

Proposes an architecture or implementation.

### Adversarial reviewer

Starts from the assumption that the design is wrong and searches for counterexamples.

### Domain reviewer

Checks the design using the conventions of the relevant discipline:

- psychometrics/statistics;
- cryptography/privacy;
- distributed systems;
- security engineering;
- mechanism/incentive design.

### Test author

Turns the property into executable falsification tests.

### Integrator

Checks that the implementation actually satisfies the specification and does not merely satisfy local tests.

AI can perform all of these roles, but they must be treated as **independent review passes**, not as one authority.

---

## 7. "Prove it by trying to break it"

For every important property ask:

> **What experiment would prove this implementation is wrong?**

Examples:

### Identity uniqueness

Try to obtain:

```text
same real identity
→ two valid role credentials
```

### Non-rotatable pseudonyms

Try:

```text
credential A
→ reputation = bad
→ burn A
→ obtain credential B
→ fresh reputation
```

### Unlinkability

Try to correlate:

```text
enrollment transcript
          ↕
public pseudonymous actions
```

using all observable metadata.

### Reproducibility

Try:

```text
same input
+ same protocol version
+ different machine
+ different process order
→ different result
```

### Bridging

Construct synthetic datasets with known latent factions and known neutral items. Verify the estimator behaves as specified.

### DIF

Generate synthetic populations with:

```text
no DIF
small DIF
medium DIF
large DIF
uniform DIF
non-uniform DIF
multi-axis DIF
```

Then measure false-positive and false-negative rates.

### Anti-collusion

Generate colluding groups of different sizes and coordination strengths. Measure effective influence against independent participants.

---

## 8. Known-answer tests

Whenever possible, create datasets for which the expected answer is known in advance.

Examples:

```text
known input
→ expected parameter interval
→ expected classification
→ expected acceptance/rejection
```

These tests should include edge cases and adversarial cases.

Known-answer fixtures should be immutable unless the scientific reason for changing them is documented.

---

## 9. Metamorphic testing

Statistical and distributed systems often lack a single exact expected output. In these cases test transformations whose effects are known.

Examples:

### Permutation invariance

```text
score(dataset) == score(shuffle(dataset))
```

### Duplication invariance where appropriate

If the statistical model is supposed to be invariant to a particular representation change, verify it.

### Symmetry

Where a transformation swaps two mathematically equivalent groups, the result should transform correspondingly.

### Seed determinism

Given the same seed and input:

```text
run_1 == run_2 == run_3
```

### Representation independence

Equivalent serialized representations must produce the same semantic result.

Metamorphic tests are especially important for the deterministic scoring engine.

---

## 10. Differential testing

Whenever an independent reference implementation is available, compare outputs.

Examples:

- Rust scoring engine vs Python simulation;
- Rust numerical routines vs established scientific libraries;
- protocol serialization vs an independent decoder;
- cryptographic integration vs the reference implementation of the selected standard/library.

Agreement between two implementations is not a proof, because both can share a conceptual error, but it materially reduces the chance of an isolated implementation bug.

The repository already uses Python simulations as an executable reference for the scoring engine; this practice should be extended wherever practical.

---

## 11. Property-based testing

Use property-based testing for invariants over large generated input spaces.

Recommended targets:

- score bounds;
- monotonicity where mathematically expected;
- order invariance;
- idempotence where applicable;
- credential validation;
- log append-only properties;
- Merkle verification;
- weight caps;
- anti-collusion bounds;
- malformed input rejection;
- state-machine transitions.

A property test should state **why the property is expected**, not just encode a convenient assertion.

---

## 12. Simulation as executable science

Simulation must not be used only to demonstrate success.

It must be used to determine:

- power;
- false-positive rates;
- false-negative rates;
- sensitivity to parameter choice;
- robustness to misspecification;
- attacker success probability;
- minimum sample size;
- instability regions.

For psychometrics, simulations should include explicit ground truth.

Example:

```text
Population
  ↓
known latent competence
  ↓
known item parameters
  ↓
known DIF injection
  ↓
observed response generation
  ↓
Isegoria estimator
  ↓
compare recovered vs true parameters
```

The simulation report should record confidence intervals or uncertainty ranges where meaningful.

---

## 13. Parameter governance

Parameters must not become accidental constants.

Any threshold such as:

- bridging threshold;
- uncertainty band;
- minimum discrimination;
- DIF cutoff;
- cluster discount exponent;
- reputation decay;
- sample size;

must have:

1. a reason for its existence;
2. a documented calibration procedure;
3. sensitivity analysis;
4. an explanation of what breaks if it moves;
5. a versioned location in the specification.

A test that passes only because a threshold was chosen after looking at the answer is not evidence of correctness.

---

## 14. Statistical validation requirements

For the psychometric layer:

### Minimum expectations

- reference mathematical formulation;
- independent derivation or literature basis;
- synthetic ground-truth simulations;
- parameter recovery tests;
- power analysis;
- sensitivity to sample size;
- sensitivity to prevalence/group imbalance;
- robustness to model misspecification;
- false-positive / false-negative characterization;
- multi-axis testing;
- anchor-item contamination analysis;
- numerical stability analysis.

A final statement such as:

> "the DIF detector works"

is insufficient.

The acceptable statement is closer to:

> "Under assumptions A–F, on simulated populations G–L, the procedure achieved X sensitivity and Y specificity in the tested regime; outside those regimes the behaviour is not established."

---

## 15. Cryptographic validation requirements

Do not treat custom cryptography as production-ready.

For every cryptographic construction record:

- exact primitive;
- exact protocol / standard;
- implementation/library version;
- security assumptions;
- trust assumptions;
- key lifecycle;
- serialization rules;
- domain separation;
- randomness requirements;
- side-channel considerations;
- replay resistance;
- compromise model;
- recovery/rotation procedure.

The project rule is:

> **Do not roll your own cryptographic primitive.**

Reference implementations may be used for scaffolding, but production integration requires mature, reviewed implementations and an explicit security review.

---

## 16. Privacy validation requirements

"Anonymous" is not a sufficient claim.

Evaluate separately:

- cryptographic unlinkability;
- issuer-side unlinkability;
- operator-side unlinkability;
- timing correlation;
- batch-size leakage;
- topic leakage;
- stylometry;
- network metadata;
- intersection attacks;
- participation-frequency attacks;
- small-population degradation;
- collusion between previously separated actors.

A privacy claim must specify the adversary.

Example:

```text
"issuer cannot link action X to identity Y"
```

is incomplete until the allowed observations and colluding parties are defined.

---

## 17. Distributed-systems validation requirements

For every state transition define:

- valid state;
- invalid state;
- transition rule;
- concurrent operation behaviour;
- replay behaviour;
- duplicate behaviour;
- partition behaviour;
- merge behaviour;
- Byzantine behaviour;
- recovery behaviour.

For CRDT or convergent state, specify the convergence invariant explicitly.

For append-only logs, verify:

```text
append
→ no mutation of prior state
→ verifiable history
→ detectable truncation
→ detectable fork
```

For consortium checkpoints, specify the exact threshold and consequences of conflicting signatures.

---

## 18. Adversarial testing catalogue

Maintain an explicit attack catalogue.

At minimum:

### Identity
- Sybil creation
- duplicate enrollment
- credential replay
- credential cloning
- credential rotation
- issuer compromise
- threshold compromise

### Reputation
- whitewashing
- long-con attack
- strategic abstention
- score farming
- majority following
- deliberate contrarianism
- collusive scoring

### Bridging
- ideological cartel
- strategic reviewer behaviour
- rating inflation
- rating compression
- sparse-matrix attacks
- synthetic reviewer profiles

### Psychometrics
- answer-key poisoning
- item parameter manipulation
- sample poisoning
- subgroup imbalance
- correlated response patterns
- DIF evasion
- adversarial item construction

### Privacy
- timing correlation
- stylometry
- topic correlation
- intersection attacks
- small-crowd deanonymization
- operator collusion

### Network
- equivocation
- rollback
- replay
- partition
- stale-state acceptance
- malicious checkpoint
- erasure-code corruption
- gossip poisoning

Every attack should have one of:

```text
MITIGATED
DETECTED
CONTAINED
ACCEPTED RISK
UNSOLVED
```

---

## 19. Residual risk is a first-class output

A mature verification report must be allowed to end with:

```text
UNSOLVED
```

or:

```text
NOT ESTABLISHED
```

This is not failure. It is an accurate statement of knowledge.

Do not convert:

```text
no exploit found
```

into:

```text
secure
```

Do not convert:

```text
simulation succeeded
```

into:

```text
mathematically proven
```

Do not convert:

```text
library API exists
```

into:

```text
production-ready cryptography
```

---

## 20. AI-assisted development protocol

When AI is used to modify Isegoria:

1. The issue states the intended property.
2. The AI explains assumptions before coding.
3. The AI proposes an implementation.
4. A separate AI/reviewer attempts to falsify it.
5. Tests are added before or with the change.
6. The implementation is compared against the specification.
7. Any changed invariant is explicitly documented.
8. The commit references the relevant claim IDs.
9. Residual risks are recorded.

A useful commit relationship is:

```text
CLAIM-XYZ
  ↳ implementation
  ↳ tests
  ↳ attack simulation
  ↳ review
```

---

## 21. Definition of evidence levels

Use the following vocabulary consistently:

### HYPOTHESIS

The claim is proposed but not implemented or tested.

### IMPLEMENTED

The code exists.

### TESTED

Automated tests exercise the intended behaviour.

### REPRODUCED

An independent execution reproduces the result.

### INDEPENDENTLY REVIEWED

A separate reviewer or implementation has examined the property.

### SCIENTIFICALLY CHARACTERIZED

The statistical/scientific behaviour has been measured over a defined operating regime.

### PRODUCTION-CANDIDATE

All relevant dependencies, failure modes, operational procedures, and external reviews required for deployment have been addressed.

### PRODUCTION-READY

Use only when the project has explicitly defined and met its deployment/security criteria.

---

## 22. What this methodology deliberately prevents

This methodology is intended to stop the following failure patterns:

### "The AI explained it, therefore it is correct."

Rejected.

### "The tests pass, therefore the design is correct."

Rejected.

### "Two AIs agreed, therefore it is correct."

Rejected.

### "A paper exists describing the technique, therefore our implementation is correct."

Rejected.

### "It works on our simulation."

Rejected as a general claim; acceptable only as evidence inside the simulation's defined regime.

### "No one has found an attack."

Not equivalent to "no attack exists."

---

## 23. Minimum gate before claiming a completed system

Before calling Isegoria complete, the project should have:

- a stable formal specification;
- a claim/evidence matrix;
- executable acceptance tests;
- property-based tests;
- adversarial simulations;
- reproducibility checks;
- documented assumptions;
- explicit residual risks;
- external review for cryptography/privacy;
- external review for the psychometric methodology;
- independent build/run instructions;
- versioned scientific fixtures;
- a clear statement of what the system does **not** prove.

Completion is therefore not defined as:

```text
all TODOs removed
```

but as:

```text
all critical claims have proportionate evidence
and all remaining uncertainty is explicit.
```

---

## 24. Repository integration

Recommended files:

```text
docs/
├── 00-overview.md
├── 01-decisions.md
├── 02-scoring-engine.md
├── 03-identity-enrollment.md
├── 04-storage-network.md
├── 05-question-lifecycle.md
├── 06-threat-model.md
├── 07-verification-and-assurance.md      ← this document
├── 08-formal-specification.md            ← generated/reviewed formal spec
├── 09-verification-matrix.md             ← living claim/evidence matrix
└── 99-glossary.md

verification/
├── invariants/
├── known_answers/
├── property_tests/
├── metamorphic/
├── differential/
├── simulations/
├── adversarial/
└── reports/
```

The exact naming may change, but the separation between **design**, **specification**, and **evidence** should remain.

---

## 25. Final principle

Isegoria should be built so that its strongest argument is not:

> "the author and the AI believe the protocol is correct."

It should be:

> **"Here are the properties we claim. Here are the assumptions under which they should hold. Here is how we tried to falsify them. Here is the independent evidence. Here is what remains uncertain."**

That is the standard that allows a small, AI-assisted project to become externally auditable.
