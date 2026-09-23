# Isegoria — Remediation and Build Roadmap

| | |
|---|---|
| **Purpose** | Turn the gaps found in `docs/08-formal-specification.md` and the decisions in `docs/01` (D17–D31) into an ordered plan of work. |
| **Derived from** | `docs/08` §14 (gaps), §12 (adversarial tests AT-*), §16 (acceptance gate). |
| **Status** | Living plan. Task ids (`T#`) are stable; sizes are S/M/L (relative effort, not dates). |

## Where we start

Already done (branch `fix/audit-concrete-bugs`):
- The six concrete code defects are closed with regression tests (see `docs/08` §0-bis).
- The seventeen open design questions are decided and recorded as `docs/01` **D17–D31**.

Everything below is what remains of the audit: the decisions turned into code, plus
the parts of the system that are specified but not yet built.

## Ordering principle

Make the system **architecturally whole and honest first**, then harden it. Concretely:

1. **Phase 1 (priority):** blocks 1 → 2 → 4, then a consolidation pass. At the end of
   Phase 1 the reference implementation actually *uses* reputation, is Sybil-resistant
   at its protocol boundary, and its nodes can talk and persist — so the README can
   describe the state accurately. It is still a **single-organization testnet**, not
   production.
2. **Phase 2 (later):** blocks 3 → 5 → 6 — the expensive hardening that turns the
   testnet into a production candidate.

Two gates are **outside our control** and block "production" regardless of code:
external cryptographic review (`docs/08` §7.4), external psychometric review (SC-8),
and real-world pilots for every empirical parameter (`docs/08` §19).

---

## Phase 1 — priority

### P1.1 · Quick fixes (audit block 1) — like the bug fixes: small, isolated, tested

| Task | What it means (plain) | Audit refs | Done when | Size |
|---|---|---|---|---|
| T1 | Stop printing the credential secret / label in debug logs (custom `Debug`) | PV-4, §8.3 | a test asserts the secret bytes never appear in `{:?}` output | S |
| T2 | The optimizer and the logistic fit report whether they converged; a "cannot separate" case returns *undetermined* instead of garbage numbers | OPT-001, IQ-1, §6.2–6.3 | callers propagate the status; `AT-DIF-06` passes | M |
| T3 | Fix a canonical order for the engine input, so re-ordering votes cannot change the result | INV-13, REPRO-002 | `AT-BR-03` (permutation invariance) passes | M |
| T4 | Record the exact numpy/scipy/OS used to generate the fixtures; re-enable the drift check in CI under a pinned environment. **Done (2026-09-21):** env pinned in `sim/requirements.txt` (numpy 2.4.4 / scipy 1.17.1, CPython 3.13); `expected_levelA.csv` / `expected_meta.csv` regenerated under it; `fixture_drift` de-ignored (self-skips only without the env) and run for real by CI after `pip install -r sim/requirements.txt` (`fixtures/PROVENANCE.md`, docs/08 §0-ter) | REPRO-003/004 | `AT-PRO-06` runs on every push; `fixture_drift` no longer ignored | S |

### P1.2 · Wiring (audit block 2) — connect pieces that already exist; **highest value**

This is where the decisions of `docs/01` become code. It is what makes "the engine is
complete" honest.

| Task | What it means (plain) | Audit refs / decision | Done when | Size |
|---|---|---|---|---|
| T5 | **Reputation actually counts.** The score computation consumes per-reviewer weights (reputation, anti-collusion discount, probation = 0), computed from the previous epoch. **Done:** `bridging::fit` minimizes the weighted objective `Σ w_u (r−r̂)²` over `Ratings.weights` (`AT-COL-06`), and `orchestrator::bridging_weights`/`weighted_ratings` now turn prior-epoch reviewer standing (probation = 0, founder = 1, established = `min(w_max, E_u)`) into those `w_u` and hand them to the fit — wired into `end_to_end.rs::run_epoch` (bootstrap epoch = founders, unit weight) and covered by `orchestrator_driver.rs::lower_reputation_moves_the_bridge_score_less` | BRIDGE-007 / G-03, D23 | `AT-COL-06`: a cartel moves a score less than the same number of independents | L |
| T6 | **The protocol stops trusting bare pseudonyms.** Every entry point requires a verified identity proof (nullifier); reputation and rate limits are keyed on it | PROTO-007 / G-04, INV-9 | `AT-PRO-01`, `AT-ID-05` pass | L |
| T7 | A blind review commits to *who* cast it and *which* item, and only that person can reveal it | CRYPTO-007 / INV-12 | `AT-BR-06` (commitment-copying blocked) passes | S |
| T8 | The "luck" (lottery, reviewer assignment, honeypot, sortition) comes from the signed checkpoint, so nobody can pick their own reviewers | D29 / INV-10 / G-05 | `AT-BR-05` (seed grinding blocked) passes | M |
| T9 | Enforce the batch minimum and the pilot sample-size gates (never validate a single item) | INV-8, PROTO-006, G-15 | `AT-PRO-02` (batch of one rejected) passes | S |
| T10 | Borderline items get extra reviewers then a clean re-decision; a failed appeal is a pseudo-observation with escrow | D26, D27, G-15 | `AT-PRO-03` (defined outcome) passes | M |
| T30 | **Retire the band tie-break.** `protocol::aggregate::resolve_band` (a weighted mean of the same ratings vs 0.5) is replaced by the D26 mechanism: add reviewers, re-run bridging, re-decide `b_j` against the plain threshold. Until then it stays documented as provisional (docs/08 PROTO-012) | PROTO-012, D26, D2 | the `documents_limitation_*` tests in `review_aggregation.rs` / `end_to_end.rs` are deleted because the mechanism they pin no longer exists; a polarized 120-vs-80 panel is *not* resolved by the larger camp | M |
| T31 | **Crowd baseline in code.** Implement the D23 baseline (`p̄_j` = weight-adjusted mean of the reviewers' own predictions) and use it for `E_u` everywhere the base rate is used today. **Done:** `reputation::crowd_baseline`; `honeypot::reviewer_skills` and `composed_gate.rs` normalize BSS against `p̄_j`; `AT-REP-02` passes (`level_c.rs`). *Residual:* the sim's `levelc_bss` reference still uses the base rate | D23, G-09, REPUTATION-003 | `AT-REP-02` (consensus follower ≈ 0) passes | M |
| T32 | **Gate Variant-1 DIF behind a calibration flag.** `pilot::stage2_dif`, `revalidation::revalidate_pool`, `logistic_dif`/`mantel_haenszel`/`purify_theta` callers in the production path require a `calibration` feature; the e2e epoch uses Variant 2 | D20, G-01, DIF-002 | production build has no code path that accepts a per-respondent `group` | S |
| T11 | Structural rate-limit and one-credential-per-label enforcement (the in-process form; the cryptographic-grade version is T20) | ID-007, ID-008 | `AT-ID-02/03` pass; over-quota rejected | M |

### P1.3 · Network and runtime layer (audit block 4) — build what is missing

| Task | What it means (plain) | Audit refs | Done when | Size |
|---|---|---|---|---|
| T12 | A real lifecycle **state machine / orchestrator** (today the flow lives only inside a test) that rejects every invalid transition. **Core done:** `protocol::lifecycle` (`State`/`Event`/`step`/`deposit`) rejects every checkable §9.1 invalid case (`orchestrator.rs`); proof-gated preconditions (T6/T7/T8/T11) enter as explicit inputs; dead `Stage` removed. `end_to_end.rs::run_epoch` is now routed through it: every stage-to-stage decision (gate outcome → pilot entry, pilot verdict → pool or reject) goes through `lifecycle::step` via `orchestrator::run_item`/`ItemVerdicts`, and the pool is exactly the items left in `ActivePool`. *Remaining:* persistence (T13) and the `SupplementaryReview` forward transition (T30) | §2.2, §9.1, PC-1 | the "invalid cases" of §9.1 are rejected in code | L |
| T13 | **Persistent state** so a restart recovers (everything is in-memory today) | §10.6 | state survives a process restart | M |
| T14 | **Signed** append-only log + consistency proofs + truncation detection | NET-004 / G-14, DS-1 | `AT-NET-01` passes | M |
| T15 | Checkpoint hardening: network id, member-set hash, client monotonic-height rule, fork/equivocation evidence | NET-006, §9.4, DS-3 | `AT-NET-03..05` pass | M |
| T16 | Authenticate backup shards (detect a corrupted shard before decoding) | NET-007, DS-4 | `AT-NET-06` passes | S |
| T17 | Real anchoring: submit to a calendar, read Bitcoin headers, schedule it, anchor the checkpoint head | NET-009, DS-5 | a checkpoint head is anchored and verified end-to-end | M |
| T18 | **Transport/replication** (gossip + DHT + convergent state / CRDT): let nodes actually talk and agree | NET-010, DS-6, §10.3 | two replicas converge on the same signed set | L |

### P1.4 · Consolidation pass — cleanup, tests, analysis

Run after P1.1–P1.3, before Phase 2.

- **Cleanup:** NaN policy on caller-supplied floats is `f64::total_cmp` (NaN sorts
  last) at every sort site — done (IQ-2, docs/08 §0-ter); keep `clippy -D warnings`
  and `fmt` green (IQ-4), including under newer clippy (1.89 adds
  `cloned_ref_to_slice_refs`, already addressed).
- **Tests:** assemble the adversarial suite added along the way into `docs/08` §13's
  tree; report a coverage figure with the command and date (IQ-5); add the
  cross-platform reproducibility check (`AT-BR-04`, RP-2).
- **Analysis:** re-walk the `docs/08` §15 matrix and raise the status of every claim
  that now has evidence; create `docs/09-verification-matrix.md` from it.

**Milestone M1 — "whole and honest testnet":** reputation is consumed, the protocol
boundary is Sybil-resistant, nodes talk and persist, the docs match the code. Not yet
production (single-org committees, unreviewed bespoke crypto, uncharacterized
parameters).

---

## Phase 2 — later (the expensive hardening)

### P2.1 · Distributed identity and cryptographic review (audit block 3)

| Task | What it means (plain) | Audit refs / decision | Size |
|---|---|---|---|
| T19 | Real distributed key generation and transport for both committees; remove the "trusted dealer"; independent setup randomness | CS-2 | L |
| T20 | Real enrollment: the state-signed identity is bound to the anonymous label with proper client/server messages (no cleartext identity reaching the key holder); cryptographic-grade rate limiting | CS-1, D22, ID-002/004/005 | L |
| T21 | Identity proofs are bound to the specific action (cannot be replayed onto another) | CS-3, AT-ID-05 | M |
| T22 | Key lifecycle (D30) and the "no recovery" revocation policy (D18) implemented and documented | CS-4 | M |
| T23 | **External cryptographic review** of the bespoke constructions, and RFC/library test vectors pass | CS-5/6, §7.4 | *external* |

### P2.2 · Scientific characterization and pilots (audit block 5)

| Task | What it means (plain) | Audit refs / decision | Size |
|---|---|---|---|
| T24 | Simulation studies: bias-detector false-positive/false-negative and power; bridging robustness sweeps; sample-poisoning | SC-2/3/7, AT-DIF-01..09 | L |
| T25 | Declare the ability metric and add the guessing correction (D25); fix the bias threshold from the studies (D24); calibration procedure for every operational threshold (τ, ε, λ, …) | SC-1/4/6 | M |
| T26 | **External psychometric review** of the statistical method | SC-8 | *external* |
| T27 | A closed calibration pilot with declared attributes, then real-world pilots — the only way to establish the empirical parameters | OR-3, §19 | *external* |

### P2.3 · Statistical privacy (audit block 6)

| Task | What it means (plain) | Audit refs | Size |
|---|---|---|---|
| T28 | De-anonymization mitigations: text normalization, batched publication with random delay, no precise timestamps, topic quotas | PRIV-003, PV-3 | M |
| T29 | A small-network anonymity (k-anonymity) model, with the population floor stated as a deployment precondition | PRIV-005, PV-3 | M |

**Milestone M2 — "production candidate":** distributed trust, externally reviewed
crypto, empirically characterized parameters, privacy hardened. Reachable only once
the *external* tasks (T23, T26, T27) are complete.

---

## Dependency notes

- T5 (weights) and T6 (identity proofs) are the backbone of Phase 1; T7–T11 build on
  T6, and T14–T18 give T5’s weighting a durable place to live.
- T30 depends on T5 (a clean re-decision needs the weighted bridging fit) and T10;
  T31 and T32 are independent and small.
- T20 supersedes T11’s in-process rate limiting with the cryptographic form.
- Nothing in Phase 1 requires the *external* gates; but the README/docs MUST keep
  calling the system a reference/testnet until T23, T26 and T27 are done.
