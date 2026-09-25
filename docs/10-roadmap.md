# Isegoria — Development Roadmap

| | |
|---|---|
| **Purpose** | The ordered plan of work: what is left to fix or build, in priority order, and what each step must show to count as done. |
| **Derived from** | `docs/08` §14 (gaps), §12 (adversarial tests AT-*), §16 (acceptance gate); `docs/01` D17–D41; the working paper in `paper/`; three reviews — the `docs/08` audit, the second review (2026-09-23), the third review (2026-09-24, `docs/08` §0-quinquies). |
| **Status** | Living plan, reordered on 2026-09-24 by priority: mathematics → P2P network → the rest; the order of execution inside Phase 1 and its milestone split were fixed the same day ([1.5](#15--order-of-execution-and-milestones)). Task ids (`T#`) are stable across reorderings; sizes are S/M/L (relative effort, not dates). Completed tasks are in [Completed work](#completed-work). |

## Priorities

Three phases, in this order:

1. **[Phase 1 — Mathematics](#phase-1--mathematics).** The two severe defects of the
   protocol boundary (T64, T65) are done; then the scoring mechanism and the decisions
   built on it. The mechanism is the new part of the system, and on master it does not
   yet do all it claims: the bridge score was batch-relative and partly majoritarian
   (D32, fixed by T49); the evaluator score is
   improper (D33), the latent DIF check raises false flags when the ability proxy is
   noisy (D37), and two of the paths that follow the gate (the band and the appeal) are
   thinner than specified. Every other layer consumes these numbers, and fixing them
   changes the golden outputs, the fixtures and `sim/` on purpose: building the rest
   first would mean validating it twice.
2. **[Phase 2 — P2P network](#phase-2--p2p-network).** Nodes that persist, talk, agree,
   and draw randomness nobody can grind. Today every node runs in one process and the
   state lives in memory.
3. **[Phase 3 — The rest](#phase-3--the-rest).** The protocol boundary (every entry point
   checks, nothing is taken on the caller's word), distributed identity and the
   cryptographic review, statistical privacy, real-world pilots.

**What falls where.** Phase 1 holds everything that *computes a verdict from the data*:
the `scoring` crate and the `protocol` rules that turn its numbers into outcomes (gate,
band re-decision, appeal, reputation weights, panel composition). It opens with two
severe defects of the protocol boundary (T64, T65, both done), moved ahead of their
natural place for their severity. Phase 2 holds the `network` crate and the randomness
beacon. Phase 3 holds the rest of *who may act* (panels, no-shows, honeypot, validated
types) and
everything that needs people outside the project.

**Inside a phase,** defects in code that exists come before new features, and each task
starts with a test that fails on the current code: see it fail, fix, see it pass. The
docs that describe the behaviour (`docs/08` §15, this roadmap) change in the same commit.

**External gates.** Three things are outside our control and block "production"
regardless of code: external cryptographic review (T23, `docs/08` §7.4), external
psychometric review (T26, SC-8), and real-world pilots for every empirical parameter
(T27, `docs/08` §19). Until they are done, the README and the docs call the system a
reference implementation / testnet.

## Where we stand

- **Done:** the audit's six concrete defects (`docs/08` §0-bis); the decisions D17–D41;
  the wiring of reputation, identity proofs, commit binding, beacon seeding and batch
  gates into the protocol; signed-log consistency, checkpoint hardening and shard
  authentication; the second review's defects, mutation testing, property and
  model-based suites, and the fuzzing of `network` and `identity`; the deposit replay
  (T64), the respondent gate (T65), the engine's input validation (T62), the
  side-balanced bridge score (T49), cross-platform reproducibility (AT-BR-04), the
  appeal channel for polarized band items (T59), the appeal stake as a
  pseudo-observation inside the author's average (D27, T61), the anchor-reliability
  gate of the latent re-check (D37, T53), the proper evaluator score with odds weights
  and the short probation (D33, D36, T50), the change detector on the per-item scores
  (D34, T51), the band's extra round of reviewers (D26, T60), the coordination
  detector on model residuals (D39, T56), panel diversification (D40, T57), live
  outcomes with randomized exploration (D35, T52) and the latent DIF target model with
  θ inside the likelihood (D37, T54). Details and evidence:
  [Completed work](#completed-work).
- **Open:** the design revisions D38 and D41 (D32 is done, T49; D33 and D36, T50; D34, T51; D35, T52; D37, T53 and T54; D39 and D40, T56 and T57); the defects found by the
  third review (2026-09-24), all but T64, T65, T62, T59 and T61; the network runtime
  (persistence, transport, live anchoring); distributed identity; privacy hardening;
  everything external.

---

## Phase 1 — Mathematics

### 1.0 · The severe defects first

The most severe defects the third review confirmed, done before the mechanism work: a
replay that drained a proposer's quota (T64) and a Level B sample that one person could
fill (T65, the evidence filter counted rows, not persons). **Both done** (2026-09-24):
see [Completed work](#completed-work).

### 1.1 · Correct the mechanism (D32–D41)

The working paper (`paper/`) found properties of the scoring mechanism that the
specification did not anticipate: a batch-relative, partly majoritarian bridge score,
spurious latent classes from error in the ability proxy, an improper evaluator score, a
weight cap that never binds, and a coordination detector with almost no data per
epoch. The decisions D32–D41 correct them (D32: T49; D33 and D36: T50; D34: T51; D37's precondition: T53; D39: T56). Several change the
specified metrics, so the simulations in `sim/`, the oracle fixtures and the golden
outputs change with them — on purpose, as in T48.

| Task | What it means (plain) | Decision / refs | Done when | Size |
|---|---|---|---|---|
| T55 | **Contested-facts pool.** An item with DIF whose key a primary source establishes (evidence procedure of `docs/02` §B.5) becomes a *contested fact*: separate pool, drawn only in balanced sets so that differential test functioning stays within a tolerance; define the DTF statistic and the procedure in `docs/02`/`docs/05` first — a docs task inside this phase; the implementation is the last item of the phase (Milestone 1b), since it adds a pool and a policy rather than correcting a number | D38; paper §4.7 | `AT-PRO-08`: every assembled test has DTF within tolerance; a DIF item without a verified source is rejected as before | L |

### 1.2 · The decisions built on the score: band and appeal

The gate has four outcomes (`docs/02` §A.3–A.4, `docs/05` [5]/[5b], D26, D27). The third
review found that the paths after two of them are thinner than specified, and one took
the caller's word (the appeal, T61). All three are done — T59, T61 and the extra round of
the band re-decision, T60 — and sit in [Completed work](#phase-12--the-decisions-built-on-the-score-band-and-appeal).


### 1.3 · Engine robustness and the rest of the specification

| Task | What it means (plain) | Refs | Done when | Size |
|---|---|---|---|---|
| T39 | Bridging: `n_min = 30` (reviewers below it do not define the `f` axis) and `d = 2`. *Recommended (2026-09-24):* descope `d = 2` — D31 already keeps `d = 1` — and record it in `docs/02` §A.4; only `n_min` remains, size S | BRIDGE-001, PROTO-003, `docs/02` §A.4, D31 | the new-reviewer path of PROTO-003 exists; `d = 2` implemented or descoped in the docs | M (S if descoped) |
| T45 | **Wider differential oracles:** random datasets vs SciPy (not only the fixed fixture), analytic vs numerical gradient for the mixture, `lbfgs` on functions with known minima (Rosenbrock, ill-conditioned quadratics). Known from T41: Rosenbrock needed ~670 gradients vs SciPy's ~46 (fixed by the strong-Wolfe search of T48: ~51), and the relative-progress stall can report `Converged` with `‖g‖_∞ ≫ g_tol` near `f = 0` | OPT-001, REPRO-003 | the oracle suite runs under the pinned sim env | M |

### 1.4 · Characterize the parameters

Every value set above is provisional until it is measured: τ ≈ 0.80 (T49), the latent
DIF cut (T35, T54), `γ` and the cap (T50), the CUSUM `k`/`h` (T51), the KR-20 floor
(T53), `N_PROBATION`, and the sample floors of `docs/02` §B.6.

| Task | What it means (plain) | Refs | Size |
|---|---|---|---|
| T24 | Simulation studies: bias-detector false-positive/false-negative rate and power; bridging robustness sweeps; sample poisoning. The only way to say the detectors "work": includes the **zero-biased** condition (false-positive rate), unbalanced classes, `δ` below 0.9, the T35/T54 threshold choice, and the discrimination-gap threshold left open by T40 | SC-2/3/7, AT-DIF-01..09, STAT-001 | L |
| T25 | Declare the ability metric and add the guessing correction (D25); fix the bias threshold from the studies (D24); a calibration procedure for every operational threshold (τ, ε, λ, …) | SC-1/4/6 | M |
| T26 | **External psychometric review** of the statistical method | SC-8 | *external* |

### 1.5 · Order of execution and milestones

Fixed on 2026-09-24, after the reordering. The sections above are thematic; this is the
order the work is done in. Sizes are the ones in the rows.

1. T65 (M) — done (2026-09-24), as T64 before it.
2. T62 (S) — done (2026-09-24): `fit` and `bridge_scores` return a `Result`; T49 is
   written against that API.
3. T49 (M) — done (2026-09-24): the side-balanced score; sim, fixtures, golden outputs
   and the README warning changed in the same commit. `AT-BR-04` (S) — done
   (2026-09-25): the golden bits checked on four platforms and in the release profile
   in CI.
4. The D26 amendment, then T59 (M) — done (2026-09-25).
5. The D27 decision, then T61 (S) — done (2026-09-25).
6. T53 (S) — the anchor-reliability gate — done (2026-09-25).
7. T50 (M), then T51 (M) — the proper evaluator score, then the change detector — both done (2026-09-25).
8. T60 (L) — the supplementary review with real extra reviewers — done (2026-09-25).
9. T56 (M), then T57 (M) — coordination on residuals, then panel diversification — both done (2026-09-25).
10. T52 (L) — live outcomes with exploration — done (2026-09-25).
11. T54 (L) — latent DIF with θ inside the likelihood — done (2026-09-25).
12. T39 (S if descoped), T45 (M).
13. T55 (L) — contested facts; its specification is written earlier, during the phase.
14. T24 (L), then T25 (M); T26 is external.

**Two streams.** Steps 2–5 and 8 touch `bridging`, `gate`, `lifecycle` and the
orchestrator; steps 6, 7 and 9–11 touch `reputation`, `dif`, `collusion`, `honeypot` and
`probation`. The two sets of files are disjoint, so with two people (or two sessions)
the streams run in parallel and meet only in `docs/08` §15 and this roadmap. Mutation
testing (`docs/11`) is re-run after step 3 and after step 7, the two blocks that change
tested logic most.

**Milestone 1a — "the defects are fixed".** Steps 1–7: every task S or M, every one a
defect in code that exists. At this point the README no longer needs its "partly
majoritarian" warning, the band and appeal paths are decided by the gate, and the engine
returns errors instead of panicking.

**Milestone 1b — "the mechanism is complete".** Steps 8–14: the features the decisions
add (a second panel, exploration, the target DIF model, panel diversification, the
contested-facts pool) and the characterization of every threshold.

**Milestone 1 — "the mechanism is sound"** is 1a and 1b together. Every verdict is
computed as D32–D41 specify; the band and appeal paths are decided by the gate, not by
the caller; the engine returns errors instead of panicking on malformed input; `sim/`,
the fixtures and the golden outputs are regenerated on purpose; every threshold is either
characterized (T24/T25) or marked provisional.

---

## Phase 2 — P2P network

At the end of this phase the reference implementation runs as a **single-organization
testnet**: several processes, one organization holding the committee keys (a trusted
dealer until T19).

### 2.1 · Fix what exists

| Task | What it means (plain) | Refs | Done when | Size |
|---|---|---|---|---|
| T63 | **The consortium validates its threshold and the member set it verifies against.** `Consortium::new(members, 0)` accepts a checkpoint with no signature, `t > n` can never verify (a silent liveness failure), and duplicate member keys are accepted. `Consortium::verify` signs over a message that includes `member_set_hash` but does not compare it with `self.member_set_hash()`, so `t` real members who sign a checkpoint declaring another member set pass `verify` — and `Beacon::from_checkpoint` documents `verify` as the check to run first (`CheckpointClient` does compare it). `new` requires `1 ≤ t ≤ n` and distinct keys (a panic with a message, as `ThresholdOprfOracle::new`: this is operator configuration); `verify` rejects a foreign `member_set_hash` | NET-005, NET-006, `docs/12` §6 | threshold 0, `t > n` and duplicate keys are refused at construction; a checkpoint declaring another member set fails `verify`; `checkpoint_model.rs` unchanged | S |

### 2.2 · Randomness nobody can grind

| Task | What it means (plain) | Refs | Done when | Size |
|---|---|---|---|---|
| T37 | **The beacon is not grind-free yet (reopens INV-10).** The seed is `H(checkpoint head ‖ …)` and the head is a deterministic function of the log content: whoever orders or includes the last deposits before the checkpoint — the publisher, a colluding threshold of signers, or a last depositor who sees the log — can try variants and keep the preferred seed. The third review adds that `lottery::admit` works in the order the caller passes the deposits (the log order), which the publisher controls. **Decided (D41):** commit-reveal among consortium members now, bound before the deposit window closes, with a public penalty for not revealing; a unique threshold signature over the epoch number (drand-style) once T19 gives the committees a real DKG | INV-10, CRYPTO-008, D29, D41 | a test shows the last depositor cannot choose among seeds; a member who withholds its reveal is excluded and recorded; `docs/08` status honest meanwhile | M |

### 2.3 · Build the runtime

| Task | What it means (plain) | Refs | Done when | Size |
|---|---|---|---|---|
| T13 | **Persistent state** so a restart recovers (everything is in memory today); the rest of T12 | §10.6 | state survives a process restart | M |
| T18 | **Transport and replication** (gossip + DHT + convergent state / CRDT): let nodes actually talk and agree. Every new wire codec (checkpoints, signatures, receipts, shards, nullifier proofs, OPRF partials) gets a fuzz target, as T44 did for the existing decoders | NET-010, DS-6, §10.3, `docs/12` §6 | two replicas converge on the same signed set; each codec is fuzzed | L |
| T17 | **Real anchoring:** submit to a calendar, read Bitcoin headers, schedule it, anchor the checkpoint head | NET-009, DS-5 | a checkpoint head is anchored and verified end to end | M |

**Milestone 2 — "nodes talk and agree".** Several processes persist, gossip, replicate
the signed log and converge on the same checkpoint; the beacon is grind-free; checkpoint
heads are anchored end to end.

---

## Phase 3 — The rest

### 3.1 · The protocol boundary: nothing on the caller's word

| Task | What it means (plain) | Refs | Done when | Size |
|---|---|---|---|---|
| T58 | **A panelist who never reveals must not freeze the item.** *(Numbered T49 in commit `4b15ac6`; renumbered because T49 is the side-balanced score.)* Since T33 a round is scored only when every panelist has revealed, and nothing handles a panelist who never commits or never reveals: the item stays in `Revealing` forever (found by the T43 model tests). It is also an attack — being drawn and then vanishing blocks an item. **Decided (2026-09-24):** at the reveal deadline each missing panelist is *replaced* by a beacon-drawn reviewer from the same `f_u` stratum and the round reopens for them; after the second deadline the round is scored on the reveals present if at least **7** panelists revealed, otherwise the item returns to the admission queue (never stuck, never decided by a rump panel). A no-show loses reputation (vote weight), and since reputation does not affect the draw (`docs/05` [4]: stratified on `f_u` only), a reviewer with repeated recent no-shows is also *suspended from the draw* for a number of epochs, growing with repetition — it removes proven no-shows, the draw among the rest stays random. The quorum, the penalty and the suspension schedule are parameters to calibrate | §9.1, G-15, T33, T43, INV-10 | a missing reveal leads to replacement, then quorum or re-queue; no state is absorbing except the terminals; an adversary who vanishes delays an item by one round at most; model tests (T43) extended to the new transitions | M |
| T66 | **An empty or invalid panel cannot be scored.** The `(Revealing, Score)` arm checks only that every panelist revealed; with an empty panel that is vacuously true and the state moves to `Pilot1`. Reachable only by building the state by hand (`AssignReviewers` refuses k ∉ [7, 11]), so it is a hole in the type, not in the transitions. Minimal fix: the `AssignReviewers` guard (odd `k` in [7, 11]) in the `Score` arm as well, or at least the T58 quorum of reveals; the real fix is T46's `Panel` | §9.1, T33, T46, T58 | a hand-built `Revealing` with an empty or even panel refuses `Score`; the `lifecycle_model.rs` reference model is updated | S |
| T67 | **The honeypot draws a random subset of the golden items.** `honeypot::inject` takes `golden.iter().take(n_golden)`: always the *first* golden items, so the same traps return every epoch and reviewers can learn to behave well only on them. `golden.choose_multiple(&mut rng, n_golden)` (deterministic from the seed, so reproducibility holds); also return the golden positions, so the caller need not infer them | PROTO-009, G-16 | over many seeds every golden item is drawn; the same seed gives the same draw | S |
| T46 | **Validated boundary types** (`Probability`, `ValidatedRatings` — the type form of T62 —, a distinct odd `Panel` of 7–11 — the real fix of T66), so whole classes of bad input cannot be constructed. Then fuzz the public entry points of `protocol` (the `protocol` part of T44) | `docs/12` §2.3 | public entry points take the validated types | M |
| T47 | *(optional)* **Bounded model checking (Kani)** on small decisive functions: `bridging_gate`, `change_approved`, panel admission, `lagrange_at_zero` with distinct indices | — | proofs run in CI | S |

Accepted residual risk, recorded rather than fixed: an author can be drawn onto the
panel of their own item, since `assign_reviewers` cannot exclude an author whose role
pseudonyms are unlinkable by design (`docs/08` §9.1 `Admitted` row, AT-PRO-05); the
honeypot is the mitigation.

### 3.2 · Distributed identity and cryptographic review

| Task | What it means (plain) | Refs / decision | Size |
|---|---|---|---|
| T19 | Real distributed key generation and transport for both committees; remove the "trusted dealer"; independent setup randomness. Unlocks the threshold-signature beacon of T37 | CS-2 | L |
| T20 | Real enrollment: the state-signed identity is bound to the anonymous label with proper client/server messages (no cleartext identity reaching the key holder); cryptographic-grade rate limiting, which supersedes T11's in-process form | CS-1, D22, ID-002/004/005 | L |
| T22 | Key lifecycle (D30) and the "no recovery" revocation policy (D18) implemented and documented | CS-4 | M |
| T23 | **External cryptographic review** of the bespoke constructions, and RFC/library test vectors pass | CS-5/6, §7.4 | *external* |

### 3.3 · Statistical privacy

| Task | What it means (plain) | Refs | Size |
|---|---|---|---|
| T28 | De-anonymization mitigations: text normalization, batched publication with random delay, no precise timestamps, topic quotas | PRIV-003, PV-3 | M |
| T29 | A small-network anonymity (k-anonymity) model, with the population floor stated as a deployment precondition | PRIV-005, PV-3 | M |

### 3.4 · Real-world pilots

| Task | What it means (plain) | Refs | Size |
|---|---|---|---|
| T27 | A closed calibration pilot with declared attributes, then real-world pilots — the only way to establish the empirical parameters | OR-3, §19 | *external* |

**Milestone 3 — "production candidate".** Distributed trust, externally reviewed
cryptography, a reviewed statistical method, empirically characterized parameters,
privacy hardened. Reachable only once the external tasks (T23, T26, T27) are complete.

---

## Continuous work

Not a phase; done alongside every task.

- Keep `clippy -D warnings` and `fmt` green (IQ-4), including under newer clippy; the
  NaN policy on caller-supplied floats stays `f64::total_cmp` at every sort site (IQ-2).
- Re-run mutation testing after each block of changes (`docs/11`); every new survivor is
  killed or justified.
- Report a coverage figure with the command and date (IQ-5); assemble the adversarial
  tests into `docs/08` §13's tree.
- Re-walk the `docs/08` §15 matrix, raise the status of every claim that now has
  evidence, and derive `docs/09-verification-matrix.md` from it.
- The README and the docs keep calling the system a reference implementation / testnet
  until T23, T26 and T27 are done.

## Dependency notes

- **Phase 1.** T64 and T65, the two severe defects, are done, and so are T62 and T49
  (the side-balanced score, which changed the golden outputs, the fixtures and `sim/` on
  purpose), and `AT-BR-04` right after it. T59 followed T49 (it measures polarization
  by T49's side gap) and is done, and so is T60, the band's extra round. T61
  followed the D27 decision and is done. T50, T51 and T52 are done. T53 and T54 are done. T55
  needs its evidence procedure specified first and comes last. T56 and T57 are done. T24/T25
  close the phase: the thresholds of T35, T49–T54 are final only after them. The full
  order and the milestone split: [1.5](#15--order-of-execution-and-milestones).
- **Phase 1 → 2.** T52's exploration draw (done) works on today's beacon and becomes
  grind-free with T37.
- **Phase 2.** T63 and T37 before T18 (as T38, done, was). T13 and T18 give T5's weights
  and the reputation histories of T50–T51 a durable place to live.
- **Phase 3.** T66 is the minimal form of T46's `Panel`. T58 builds on T33/T43 and
  draws replacements from the beacon (after T37). T20 supersedes T11. T37's
  threshold-signature variant needs T19.
- **External.** Nothing in Phases 1–2 needs the external gates.

---

## Completed work

Grouped as they were first planned (P1.1–P1.5, then by phase), with their evidence;
other documents and commit messages refer to these ids and block names.

### Phase 1.0 · The severe defects first

| Task | What it means (plain) | Refs | Done when | Size |
|---|---|---|---|---|
| T64 | **A deposit is accepted once. Done (2026-09-24):** `TransparencyLog::contains` (a set kept by `append`); `deposit` and `deposit_with_identity` return `DepositRejected::DuplicateCid` **before** `admit` and the quota charge, so a replay appends nothing and costs the author nothing; the `Propose` proof is bound to the draft *and* the epoch (`deposit::deposit_context(cid, epoch)`, as `review::review_context`), so a proof of epoch `e` is refused in `e + 1`. `proto007_deposit_replay.rs`: a byte replay and a fresh re-proof of the same draft → `DuplicateCid`, `log.len() == 1`, one quota unit used, the author's next draft still accepted; a proof presented in the next epoch → `Unproven(BadProof)` with nothing appended or charged. *Was:* `deposit_with_identity` appended and charged every copy, so whoever saw a proposal in transit could drain its author's quota. `lifecycle::deposit` keeps its `fresh_cid` input (the machine takes the check's result, the entry points now compute it) | `docs/08` §9.1 row 1, PROTO-007, ID-008 | a second deposit of the same pair → `DuplicateCid`, `log.len() == 1`, one quota unit used; a `Propose` proof of epoch e is refused in e+1 | S |
| T65 | **Respondents pass the identity gate; the pilot counts persons, not rows. Done (2026-09-24):** `pilot::submit_response(proof, issuer, batch, epoch, &mut NullifierSet)` admits a `Respond` proof bound to the batch (`pilot::batch_id`, the content id of the sorted item cids) and the epoch (`pilot::response_context`), refusing a second sheet by the same person (`ResponseRejected::Duplicate`); `screen`, `dif_batch` and `revalidate_batch_latent` take the respondent count from that set (`NullifierSet::len`) and refuse rows that are not the admitted respondents one to one (`PilotError::RowCountMismatch`); `end_to_end.rs::run_epoch` issues one credential per fixture row and admits it through the gate (1,500 persons, ≈14 ms each with `identity` optimized in the dev profile, `Cargo.toml`). `proto013_respondent_gate.rs` (AT-PRO-09): the same nullifier twice on one batch → `Duplicate`; 300 rows (3,000 for the latent re-check) from one respondent → `NotEnoughRespondents { have: 1 }`; a proof for batch A on batch B, or in another epoch → `Unproven(BadProof)`; a `Judge` proof → `WrongRole`; one row more than the admitted respondents → `RowCountMismatch`. *Was:* `Role::Respond` appeared nowhere in `protocol/src` and the floors counted `theta.len()` rows, so 300 rows from one person satisfied `N1_MIN` | `docs/08` §9.1 `Pilot1` row, INV-9, `docs/02` §B.6 | `AT-PRO-09`: the same nullifier twice on one batch → `Duplicate`; 300 rows from one respondent → `NotEnoughRespondents`; a proof for batch A is refused on batch B | M |

### Phase 1.1 · Correct the mechanism (D32–D41)

| Task | What it means (plain) | Decision / refs | Done when | Size |
|---|---|---|---|---|
| T49 | **Side-balanced bridge score. Done (2026-09-24):** `bridging::two_means` (deterministic 1-D 2-means on `f_u`, initialized at its extremes, as the paper's script), `bridging::side_balanced(&Fit) -> SideScores` (per-side mean predictions, `score = (A + B)/2`, `gap = \|A − B\|`), `bridge_scores -> BridgeScores { robust: bootstrap-min of the score, full }`; `gate::bridging_gate(score, gap, τ, ε, appeal_gap)` with the provisional constants `TAU = 0.80`, `EPS = 0.02`, `APPEAL_GAP = 0.25` (`docs/02` §A.3); `supplementary_review` re-decides on the full fit's `S_j`. `sim/bridging_irt_dif.py` and `sim/export_fixtures.py` compute the same score; `expected_levelA.csv` gained `side_a_full, side_b_full, side_full, gap_full`, `expected_meta.csv` has `tau = 0.80`, `levelc_p/bss.csv` follow the new verdicts, `golden_bits.txt` regenerated; the README warning is gone. *Evidence:* `level_a.rs` matches the oracle (`S_j` to 0.004, the gap to 0.008, sides of 80 and 120); `side_balanced.rs` — AT-BR-08: leak ≤ 0.1 with 60/40 and 80/20 camps from 200 to 3,200 reviewers where the intercept leaks 0.5–0.9, and on the review's mirror-item dataset the camp-size effect on `S_j` stays within 0.1 at every ratio from 50/50 to 95/5 in both orientations while the intercept's is 0.7–0.9, neither mirror item passes and both stay polarized; AT-BR-09: ten decoys move no item by more than 0.02 and change no verdict, the consensus items alone stay within 0.02, the intercept moves by more than 0.1. `end_to_end.rs`: the pool is unchanged ({01, 07}: item 02 passes Level A on `S_j` and dies in the pilot screen with a 2PL slope of 0.47), and the appeal still recovers the true-but-divisive item on its gap (0.67). *Measured limits, recorded in `docs/02` §A.3:* a residual leak of 0.1–0.2 with 50–100 reviewers (a fraction of the intercept's), and noisy side means with a minority side of about ten reviewers (at 95/5 one consensus item in eight fell to 0.78). *Deviation from this row's target:* the leak bound holds from 200 reviewers, not from 50. *Pending:* the mutation-testing re-run the plan schedules after this step (`docs/11`, [1.5](#15--order-of-execution-and-milestones)) | D32; paper §3.3–3.4, §7.1; BRIDGE-008/009 | `AT-BR-08` and `AT-BR-09` pass — both fail on the intercept | M |
| T53 | **Anchor-reliability gate for latent DIF. Done (2026-09-25):** `irt::kr20(anchors)` — KR-20 of the anchor total on the batch's respondents (population variances, as the paper's `revisions_dif.py`; 0, not NaN, with fewer than two anchors or no spread) — and `irt::KR20_MIN = 0.90` (provisional, T25); `pilot::admit_anchors` refuses below the floor (`PilotError::UnreliableAnchors { kr20, need }`, NaN-safe); `revalidate_batch_latent(respondents, anchors, responses, seed)` takes the anchors instead of θ, checks items, respondents, rows and anchors in that order, and computes θ itself (`theta_from_anchors`), so a caller cannot vouch for a proxy the gate has not measured; `MixtureDif::differential` reports the differential gap — the signed shift between the two most populous counted classes less its median over the batch — as a diagnostic only, pinned in the golden bits, never read by `latent_flags`. `anchor_reliability.rs` (AT-DIF-11): null batches drawn as the paper's `dif_generate` (N = 6,000, K = 8) give KR-20 within 0.03 of the paper's table for 10, 20, 30 and 60 anchors; 20 anchors → `UnreliableAnchors` before any fit, while the bare engine on the same batch selects a spurious two-class model; 60 anchors → accepted, no flag; an undefined reliability (one anchor, no spread, a NaN) is refused; anchor rows are the respondents' (`RowCountMismatch`). `latent_classes.rs`: with 6 of 8 items shifted the same way the raw gap flags exactly the shifted items (θ known) and the differential gap inverts. *Note:* the reference fixtures' 30 anchors (KR-20 ≈ 0.87) are below the floor; they exercise the engine (`revalidate_pool_latent`), not the gate | D37; paper Prop 10, Table 6, §7.3; DIF-010 | `AT-DIF-11`: the paper's null batches with 20 anchors are refused, those with 60 are accepted and raise no flag | S |
| T50 | **Proper evaluator score, odds weights, short probation. Done (2026-09-25):** `reputation::difference_score` (`(p̄_{−u,j} − o_j)² − (p_uj − o_j)²`), `loo_baseline` (the weight-adjusted mean of the *other* panelists; a reviewer with no weighted others is scored against their own forecast, 0), `loo_scores`, `mean_score` (`S_u`), `odds_weight(S_u, k_u)` = `exp(γ·S_u·k_u/(k_u+k₀))` with `EvaluatorParams { gamma: 35, k0: 100 }` (provisional, T25); `evaluator_score` (`σ(γ·BSS)`) removed, `brier_skill_score` kept as the sim oracle; `honeypot::reviewer_skills` returns `S_u` on the golden items; `probation::N_PROBATION` 200 → 30 (D36), `effective_review_weight(is_founder, k_u, skill, w_max)` = the capped odds weight once established; `ReviewerStanding.skill` replaces `e_u`; `orchestrator::epoch_weight_cap` = `3 × median` over the reviewers who carry weight (founders and established, not probationers), `+∞` with none. `evaluator_score.rs`: AT-REP-05 — for random beliefs and baselines (`m ≤ 4`, all `2^m` outcomes enumerated) the truthful report beats any other by exactly `Σ_j (p_j − q_j)²`; the retired ratio-form score reproduces the paper's dissenter (belief 0.30, crowd 0.65 → report 0.60); AT-REP-02 — a copier of the others scores exactly 0 and weighs exactly 1 (also on the fixture, `level_c.rs`); AT-REP-04 — a reviewer reliably 0.1 better than nine crowd-level ones weighs 16 uncapped, 3 capped; D33's arithmetic (×2 for a reliable 0.02; luck after 16 items ×2.4 → ×1.13). `properties.rs`: scores bounded by 1, the copier at 0, the weight finite and positive, 1 with nothing scored. `lifecycle.rs`, `orchestrator_driver.rs` follow the new scale (the discounted bloc of the T5 test is now `S_u = −0.5`, weight ≈ 0.02) | D33, D36; paper Props 12, 14, 15; REPUTATION-005/008, G-12 | `AT-REP-05`, `AT-REP-02` and `AT-REP-04` pass; probation ends at 30 scored outcomes | M |
| T51 | **Change detector instead of the asymmetric update. Done (2026-09-25):** `reputation::{Cusum, CusumParams}` — a one-sided CUSUM on a reviewer's per-item scores against their own mean, `s ← max(0, s + (S_u − S_uj) − k)`, alarm at `s > h`, then restart (`k = 0.03`, `h = 1.5`, provisional, T25); `probation::SkillTrack` — the running mean (the weight's `S_u`), the count (`k_u`, which decides probation) and the statistic; the detector runs only out of probation (a mean over few items is no reference) and an alarm restarts the track, so the reviewer is on probation (weight 0) until 30 new scored outcomes; `standing()` hands the next epoch's `ReviewerStanding` to `bridging_weights`. `asymmetric_ema` removed with its tests; `adversarial.rs::a_long_con_is_unprofitable` reworked on the detector. `change_detector.rs` (AT-REP-07): honest per-item scores drawn as the paper's `panel_bias` (10,000 items, seed 7) raise at most one alarm; a reviewer honest for 300 items who then flips 20% of forecasts is caught within 100 items on nine of ten seeds — median 25 (the paper's 36), the tenth after 356: at `k = 0.03` the drift is about 0.01 per item, detection is noise-driven and the tail is long (a T25 calibration item; the paper's `k = 0.02`, `h = 1.0` halves the median at 2.3 false alarms per 1,000) — is on probation with weight 0, and is established again after 30 honest outcomes; no alarm during probation. `level_c.rs`: alternating ±0.3 never alarms, a drop of 0.1 alarms at item 22; `properties.rs`: the statistic stays in `[0, h]` and a score at or above the reference never alarms | D34; paper §5.5, §7.2; REPUTATION-004 | `AT-REP-07` passes (the game-theoretic `AT-REP-01` stays open) | M |
| T56 | **Coordination detector on residuals. Done (2026-09-25):** `collusion::ResidualHistory` (residuals `r − r̂` per reviewer and item, keyed by a global item id, accumulated across epochs; `record_epoch(ratings, fit, item_ids)`), `MIN_SHARED_ITEMS = 30` (a pair is read only from there), `permutation_p_value` (seeded, one-sided), `CoordinationParams { min_shared: 30, rho_min: 0.7, p_max: 0.001, permutations: 999 }` (provisional, T25), `coordination_clusters` → `CoordinationReport { flagged, clusters }`: a pair is flagged at `ρ ≥ ρ_min` with `p ≤ p_max`, clusters by average linkage over the flagged pairs (a merge needs the mean over all cross pairs, an unflagged pair counting 0, to reach `ρ_min`), everyone else a singleton whose id is its own index. The raw rule (`correlation_matrix`, `cluster_by_correlation`) and the discount stay in the engine as reference and analysis (D40). *Evidence:* `coordination.rs` — the paper's dataset (`revisions_collusion.py`) ported with ChaCha8: raw same-camp +0.93 vs cartel +0.92 (indistinguishable), residual same-camp +0.015, cross-camp +0.018, cartel +0.89; all 45 cartel pairs flagged, none of the 18,000 honest pairs, the cartel one cluster, every honest reviewer a singleton (AT-COL-07, AT-COL-02); the raw rule at the paper's threshold chains the whole majority camp into one cluster; a pair below 30 shared items is never read; the history accumulates across epochs in the sparse regime and reads a coordinated pair once it reaches 30 shared items; the permutation null is seeded and discriminates; a mimic of an honest reviewer forms a pair with them and chains nobody else (AT-COL-05). *Measured:* `ρ_min = 0.5` flags four honest pairs by chance; 0.7 none | D39; paper Prop 18; COLLUSION-002/003/005/006 | `AT-COL-02` (a cartel jittered by σ = 0.05 is detected) and `AT-COL-07` (honest reviewers of the same camp are not flagged; opposite camps are not joined; pairs with fewer than 30 shared items never are) pass | M |
| T57 | **Panel diversification. Done (2026-09-25):** `review::assign_diverse(reviewers, clusters, taken, k, seed)` — the stratified draw of `assign_reviewers` that excludes the `taken` nyms and every cluster already seated (one cluster id per reviewer, as `coordination_clusters` returns them; a mismatched length yields no panel); a stratum the constraint empties is filled by the eligible reviewer nearest the stratum centre; `assign_diverse_from_beacon` (first panel) and `assign_extra_diverse_from_beacon` (the band's extra round, outside the first panel and its clusters) are the beacon entry points. The protocol applies no weight discount: `bridging_weights` takes no cluster input. *Evidence:* `panel_diversification.rs` — the paper's population (1,000 reviewers, a cluster of 50, panels of 9), 2,000 draws: no panel holds two members of the cluster, nine distinct reviewers, every panel spans the axis, while the uniform draw seats two or more on 6.6% of panels (paper 7.0%); the extra round avoids the first panel and its clusters over 200 beacon slots; a stratum emptied by the constraint is filled from the axis and singleton clusters reproduce the plain draw; a flagged and an honest reviewer with the same standing weigh the same | D40; paper Prop 17 | `AT-BR-10` passes: over randomized draws no panel holds two members of one flagged cluster, and no honest reviewer's weight changes | M |
| T52 | **Live outcomes with exploration. Done (2026-09-25):** `protocol::exploration` — `EXPLORATION_RATE = 0.05` (provisional, T25), `explore(seed, rate)`, `explore_from_beacon(beacon, slot, rate)` (`randomness::EXPLORATION`, keyed on the admitted slot, never the draft), `Observation { outcome, inclusion }`, `Scored::{Observed, Unobserved, Pending}`, `outcome_of(state, rate)`, `record_outcome(track, state, rate, score, params)`, `FalseNegatives { explored, passed }::rate`; `lifecycle`: `Event::Explore { seed_from_checkpoint }` takes a gate rejection (`RejectReason::at_the_gate`) to `State::Explored { reason, screened }`, the two pilot batches under the pilot's own floors, then `State::Measured { reason, passed }` — terminal, never `ActivePool`; `ItemVerdicts.explored` and `run_item` walk it; `probation::SkillTrack::{record_observed(score, inclusion), record_unobserved, reference, statistic, reviewed}` — the mean at `1/π_j` over every reviewed item, the CUSUM on the unweighted observed scores; `reputation::inverse_probability_mean`. *Evidence:* `exploration_weights.rs` (AT-REP-06): under a gate that observes a report of at least 0.5 with certainty and the rest at 5%, the weighted score's exact expectation is `(b − q)² − (p − q)²` for 2,000 random cases — the true belief its unique optimum — while the bare observed score pays the paper's dissenter (0.40 against a crowd at 0.70) 0.08 to report 0.50 against 0.0045 for the truth (0 without exploration); nine reviewers with noise 0.05–0.30 on 100,000 items, the gate on the panel's mean forecast: the weighted mean within 2.1 standard errors of the full-information mean for every reviewer, the passed-only mean 11.6 off for the noisiest; `exploration.rs` (AT-PRO-07): the draw reproduces from the beacon, 5.0% and 4.9% of 20,000 slots under two beacons with 0.24% in common, a participant's seed refused, every explored path ends `Measured`, a pilot rejection cannot be explored, the track weighs an explored −0.5 at −10 in the mean and reads −0.5 to the detector (no alarm); the model suites cover `Explored`, `Measured` and every measured reason; `end_to_end.rs`: the real-health item, polarized and unappealed, explored on the fixtures is measured as a pass — a gate false negative, rate 1/1 — with the pool and the author's standing unchanged | D35; paper §7.2, "Exploration restores properness" | `AT-PRO-07` (an explored item never enters the pool; the draw is reproducible from the beacon and cannot be chosen) and `AT-REP-06` (on synthetic data the weighted score's expectation equals the full-information score, and truthful reporting stays optimal) pass | L |
| T54 | **Latent DIF with θ inside the likelihood. Done (2026-09-25):** `scoring::latent::{latent_dif, latent_dif_with, LatentParams { max_classes: 4, n_starts: 4, nodes: 41, theta_max: 5.0 }, LatentDif { classes, non_uniform, pi, eta, anchor_a, anchor_b, item_a, item_b, dif, a_gap, posterior, bic_gain, candidates, status }::flags(max_gap)}` — the paper's full mixture: the anchors in the likelihood with class-invariant parameters, a class ability mean `η_g` per class (`η_0 = 0`), θ integrated on a fixed grid, the number of classes and uniform vs non-uniform DIF by BIC from seeded starts, a fused NLL and analytic gradient from the EM artificial data (every transcendental function per node and item; per respondent two sums per class and one `exp` per class and node), the objective per respondent so the tolerances are scale-free; `revalidation::revalidate_batch_latent` runs it on the anchors it admits (`target_flags`); the proxy-θ `mixture_dif`/`revalidate_pool_latent` are retired from the production path and kept for the fixtures (golden bits unchanged; the target model pinned on a seeded batch, `latent.*` rows). *Evidence:* `latent_target_model.rs` — the paper's null batches at N = 6,000: 1 class at 10, 20, 30 and 60 anchors (KR-20 0.68–0.94), no item flagged, where the proxy model selects two classes at 10, 20 and 30 anchors and flags 8, 2 and 0 clean items; AT-DIF-12: 2, 4 and 6 of 8 items shifted by 0.9 at N = 6,000 with 30 anchors: exactly the shifted items flagged, their gaps 1.7–1.9 (the true 2δ = 1.8; 3.6 and 1.1 in the two-item case) and the clean items' at most 0.13; AT-DIF-01 (`calibration`): 0 of 120 clean items flagged and no batch with a mixture over 15 null batches — 20, 40 and 60 anchors (KR-20 0.79–0.94), four seeds at N = 3,000 and one at N = 12,000; runtime: on one core (dev profile, `scoring` at opt-level 3) 8–32 s per 8-item null batch at N = 3,000, 90–110 s at N = 12,000, 15–25 s at N = 6,000, and 75–90 s for a campaign batch at N = 6,000 whose BIC search reaches three classes. The verdict threshold stays 1.0 on the gap, provisional (T24/T25): the null gaps are 0 and the campaign gaps 1.7–1.9 | D37; DIF-006/008/010 | `AT-DIF-01` holds on the cells run (item-level false-positive rate ≤ 5% on null batches, anchor KR-20 0.82–0.94, N = 3,000 and 12,000) and `AT-DIF-12` passes; runtime per batch recorded | L |

### Phase 1.2 · The decisions built on the score: band and appeal

| Task | What it means (plain) | Decision / refs | Done when | Size |
|---|---|---|---|---|
| T59 | **A polarized band item keeps the appeal channel. Done (2026-09-25),** after the D26 amendment (the below-band rule, decided the same day): `gate::supplementary_review(ratings, params, item, τ, appeal_gap)` returns `Pass` at or above the plain threshold and, below it, `AppealEligible` when the side gap is at least `appeal_gap` and `Reject` otherwise — never a second band; `Event::Resolve { outcome: GateOutcome }` moves `SupplementaryReview` to `Pilot1`, `AppealEligible` or `Rejected(Borderline)` and refuses `SupplementaryReview`; `ItemVerdicts::band_outcome` replaces `band_advances` and `run_item` walks the appeal from a band-derived `AppealEligible` as from any other. Reference models updated (`lifecycle_model.rs`, `orchestrator_model.rs`, with the fourth outcome generated so the refusal is exercised); `supplementary_redecision.rs` shows the rule on the fixture plus one leaning item (polarized → appeal, lukewarm on both sides → borderline, approved → pass) and that the partisan fixture items, re-decided, keep the appeal; the driver shows the band-derived appeal reaching the pool and, un-appealed, a polarization reject. *Was:* `Resolve { passed: false }` ended every failing band item in a terminal reject | D26 (amendment), `docs/05` [5b], PROTO-004 | a polarized band item that fails the re-decision is `AppealEligible`, a non-polarized one `Rejected(Borderline)`; model tests extended | M |
| T61 | **An appeal checks the stake, and the escrow is settled. Done (2026-09-25),** after the D27 decision (the pseudo-observation form, confirmed the same day): `protocol::appeal::AuthorHistory` holds the author's quality observations and their ages (what `reputation::author_score` averages); `file_appeal` refuses an author whose `C_a` is below `appeal_floor` — the prior mean `α₀/(α₀+β₀)`, 0.4 with the default prior — and otherwise escrows a zero-quality observation at age 0, so `C_a` falls at once; `orchestrator::settle_appeal(author, escrow, terminal, quality)` replaces the zero with the item's measured quality when the terminal is `ActivePool` and leaves it on any other; `ItemVerdicts` gains `appeal_within_window`, `author_reputation`, `appeal_floor` and `run_item` derives `Event::Appeal`'s two flags from them; `gate::settle_appeal` (the ledger: `+ gain` / `− stake`) is removed — there is no additive gain, promotion replaces the pseudo-observation with a real, good observation. `appeal_stake.rs`: filing moves a fresh author from 0.4 to 1/3; a failed appeal leaves the zero and refuses the next filing until a good item restores the average; promotion replaces the zero; `run_item` returns `AppealWindowClosed` and `InsufficientReputation` from the verdicts (exactly at the floor covers); the terminal state settles the escrow; `orchestrator_model.rs` carries the two checks; `end_to_end.rs::run_epoch` files and settles the appeal of the true-but-divisive item and the author's reputation ends higher than without it. *Was:* `run_item` sent `Appeal { within_window: true, reputation_covers_stake: true }` hard-wired and the escrow was never settled | D27, REPUTATION-007, `docs/05` [5b] | an appeal with `C_a` below the floor → `Invalid::InsufficientReputation` (and nothing escrowed); promoted → the pseudo-observation replaced by the item's quality; failed → the zero stands; the ledger form retired | S |
| T60 | **The supplementary review really adds reviewers. Done (2026-09-25):** `lifecycle::State::SupplementaryReview { item, panel, extra_panel, commits, commits_closed, reveals }` carries the D26 extra round; `Event::AssignExtraReviewers { panel }` (one to `K_EXTRA_MAX = 11` distinct nyms, none from the first panel: `PanelSizeInvalid`/`DuplicatePanelist`), then the first round's `Commit`/`CloseCommits`/`Reveal` rules on the same item and binding; `Event::Resolve` is refused before the assignment (`Invalid::NoExtraPanel`) and before every extra panelist revealed (`PartialEpoch`). `orchestrator::ExtraRound`, `extra_round` (walks the round through `step`), `expanded_ratings(base, rows, item, reveals, weight_of_new)` (one observation of the item per extra reveal, from the reviewer's row or a new one), and `run_item(reviewed, verdicts, extra, redecide)`: `ItemVerdicts::band_outcome` is gone — the re-decision is a function the orchestrator calls on the reveals the machine recorded, once the round is complete (the caller folds them into the ratings and runs `gate::supplementary_review`). `review::assign_extra_from_beacon(reviewers, first_panel, k_extra, beacon, slot)` draws the extra panel outside the first one on its own domain (`randomness::EXTRA_REVIEW`), `K_EXTRA = 4` (provisional, T25). *Evidence:* `supplementary_redecision.rs` — on the fixture plus an item nine reviewers from both camps rate just above τ, the first panel alone passes (S_j ≥ τ) and four extra reviewers at 0.1 turn the re-decision into `Rejected(Borderline)`, four at 0.95 into the pool, end to end through `run_item`; twenty extra reviewers at 0.2 on the 180-rater item do the same at the fit level; a newcomer gets a new row at the caller's weight. `orchestrator_driver.rs` — the closure receives exactly the round's reveals; no round → `NoExtraPanel`; an overlapping panel → `DuplicatePanelist`; a partial round → `PartialEpoch`. `tests/orchestrator.rs` — every rule of the extra round. The reference models of `lifecycle_model.rs` (a `Band` phase with the extra round, its detours generated) and `orchestrator_model.rs` (`ExtraSpec`, `model_extra_round`; the fixed sample enlarged to 2,048 rounds so every way the extra round can fail is reached) carry the rules. *Was:* the re-decision re-fitted the first panel's ratings alone, which relaxed the robust threshold instead of adding evidence | D26, PROTO-008, BRIDGE-006, G-15 | the re-decision fits ratings from the first panel plus `k_extra` distinct new reviewers; a band item whose extra reviewers disapprove is rejected even though its first-panel full-fit `S_j ≥ τ` | L |

### Phase 1.3 · Engine robustness

| Task | What it means (plain) | Refs | Done when | Size |
|---|---|---|---|---|
| T62 | **The engine rejects malformed ratings instead of panicking. Done (2026-09-24):** `Ratings::validate() -> Result<(), RatingsError>` — an observation with `u ≥ n` or `j ≥ m` (`IndexOutOfRange`), `weights.len() ≠ n` (`WeightCount`), a non-finite rating (`NonFiniteRating`), a non-finite or negative weight (`BadWeight`), a duplicate `(u, j)` pair (`DuplicateObservation`) — runs first in `fit` and `bridge_scores`, which return `Result` (the direction of T46); `gate::supplementary_review` propagates it and refuses an item index past the batch (`ItemOutOfRange`); `orchestrator::weighted_ratings` returns `Result` (a standing count that differs from the rows is `WeightCount`; the `assert` in `with_weights` is gone). `scoring/tests/malformed_ratings.rs`: each case returns its error from both entry points, and a property over arbitrary `Ratings` (indices past `n`/`m`, NaN and infinite values, negative weights, any weight count, duplicates) never panics — `fit` and `bridge_scores` succeed exactly when `validate` accepts. `crates/scoring/fuzz/bridging` (cargo-fuzz; not yet run, no nightly in the session) explores the same entry points. *Was:* `Ratings` has public fields and an out-of-range observation panicked on a bounds check inside the objective; duplicates counted twice. *Left to T46* (`docs/12` §2.3): `Ratings::from_dense` on a ragged matrix, and the slice-taking entry points of `irt`, `dif`, `collusion` and `reputation`, which index out of bounds on mismatched lengths or ragged matrices — caller preconditions, probed and recorded | `docs/12` §2.3, T44, T46 | out-of-range index, wrong weight count, non-finite or negative value and duplicate pair each return an error, no panic; a property test: arbitrary `Ratings` never panics | S |
| — | **Cross-platform reproducibility (`AT-BR-04`). Done (2026-09-25):** the engine's transcendental functions (`exp`, `ln`, `ln_1p`, `cos`, `pow`) come from the pure-Rust `libm` crate through `scoring::fmath`, not from the platform's libm, whose last bits differ between glibc, musl, Apple and Microsoft and which an iterative fit amplifies into a different stopping point; `golden_bits.txt` regenerated on purpose (last-bit changes only, every oracle test unchanged); `libm` optimized in the dev profile (`Cargo.toml`). CI job `golden` (`.github/workflows/ci.yml`) checks the golden bits on linux-gnu in the release profile, linux-musl, macOS-aarch64 and Windows-MSVC on every push, next to the main job's linux-gnu dev run. Verified on 2026-09-25: linux-gnu dev, linux-gnu release and linux-musl agree bit for bit locally, and the first CI run of the job (master `c6ac1fa`, run 38) is green on macOS-aarch64 and Windows-MSVC as well — and the control shows why it was needed: the previous code, on the platform libm, matched its own golden bits in the release profile but moved 96 of the 118 values on musl (glibc vs musl `exp`/`log`/`cos`) | REPRO-001, INV-7 | `AT-BR-04` runs in CI | S |

### P1.1 · Quick fixes (audit block 1)

| Task | What it means (plain) | Audit refs | Done when | Size |
|---|---|---|---|---|
| T1 | Stop printing the credential secret / label in debug logs (custom `Debug`) **Done:** identity types carry a hand-written redacting `Debug` (`identity/tests/redaction.rs`, PV-4). | PV-4, §8.3 | a test asserts the secret bytes never appear in `{:?}` output | S |
| T2 | The optimizer and the logistic fit report whether they converged; a "cannot separate" case returns *undetermined* instead of garbage numbers **Done:** `lbfgs` returns `Convergence` and `fit_logistic` returns `LogisticFit` (`Separated` → undetermined); callers propagate it (`pilot::stage2_dif`, `Fit2pl`, `latent_flags`); `AT-DIF-06` in `lifecycle.rs`. Hardened by T41/T48. | OPT-001, IQ-1, §6.2–6.3 | callers propagate the status; `AT-DIF-06` passes | M |
| T3 | Fix a canonical order for the engine input, so re-ordering votes cannot change the result **Done:** `Ratings::canonical` sorts observations before every fit (`scoring/tests/canonical_input.rs`, `AT-BR-03`); property-tested in T42. | INV-13, REPRO-002 | `AT-BR-03` (permutation invariance) passes | M |
| T4 | Record the exact numpy/scipy/OS used to generate the fixtures; re-enable the drift check in CI under a pinned environment. **Done (2026-09-21):** env pinned in `sim/requirements.txt` (numpy 2.4.4 / scipy 1.17.1, CPython 3.13); `expected_levelA.csv` / `expected_meta.csv` regenerated under it; `fixture_drift` de-ignored (self-skips only without the env) and run for real by CI after `pip install -r sim/requirements.txt` (`fixtures/PROVENANCE.md`, docs/08 §0-ter) | REPRO-003/004 | `AT-PRO-06` runs on every push; `fixture_drift` no longer ignored | S |

### P1.2 · Wiring (audit block 2)

| Task | What it means (plain) | Audit refs / decision | Done when | Size |
|---|---|---|---|---|
| T5 | **Reputation actually counts.** The score computation consumes per-reviewer weights (reputation, anti-collusion discount, probation = 0), computed from the previous epoch. **Done:** `bridging::fit` minimizes the weighted objective `Σ w_u (r−r̂)²` over `Ratings.weights` (`AT-COL-06`), and `orchestrator::bridging_weights`/`weighted_ratings` now turn prior-epoch reviewer standing (probation = 0, founder = 1, established = `min(w_max, E_u)`) into those `w_u` and hand them to the fit — wired into `end_to_end.rs::run_epoch` (bootstrap epoch = founders, unit weight) and covered by `orchestrator_driver.rs::lower_reputation_moves_the_bridge_score_less` | BRIDGE-007 / G-03, D23 | `AT-COL-06`: a cartel moves a score less than the same number of independents | L |
| T6 | **The protocol stops trusting bare pseudonyms.** Every entry point requires a verified identity proof (nullifier); reputation and rate limits are keyed on it. **Done:** `protocol::admission::{admit, NullifierSet}` verifies a role `NullifierProof` and returns the proven `NullifierProof::id()` (never `derive_nym`); `deposit_with_identity` and `review::submit_review` are the identity-gated entry points; `nullifier::{prove,verify}` gained an action `context` so a proof cannot be replayed (AT-ID-05); `end_to_end.rs::run_epoch` deposits through the gate. `inv9_nym_proof.rs` covers `AT-PRO-01`, `AT-ID-05` and per-item dedup. *Note:* the cryptographic-grade enrollment/replay hardening and the per-credential quota stay **T20/T11**. *Third review:* the `Respond` entry point was never gated (T65), and a deposit can be replayed (T64) | PROTO-007 / G-04, INV-9 | `AT-PRO-01`, `AT-ID-05` pass | L |
| T7 | A blind review commits to *who* cast it and *which* item, and only that person can reveal it. **Done:** `review::commit`/`reveal` bind the committer nym and item cid (`H(prob, nonce, committer, item)`); the `lifecycle` `Revealing` state carries the item and the reveal recomputes against the revealer + item, so a copied commitment cannot be opened by anyone else. `inv12_commit_binding.rs` (`AT-BR-06`) | CRYPTO-007 / INV-12 | `AT-BR-06` (commitment-copying blocked) passes | S |
| T8 | The "luck" (lottery, reviewer assignment, honeypot, sortition) comes from the signed checkpoint, so nobody can pick their own reviewers. **Done:** `randomness::Beacon::from_checkpoint` + `seed(purpose, index)` = `H(signed head ‖ height ‖ purpose ‖ index)`, and the `_from_beacon` wrappers in `lottery`/`review`/`honeypot`/`governance` seed each draw from it; reviewer assignment keys on a byte-independent admitted slot, so the panel does not depend on draft bytes. `inv10_checkpoint_seed.rs` (`AT-BR-05`). Reopened by the second review: T37 | D29 / INV-10 / G-05 | `AT-BR-05` (seed grinding blocked) passes | M |
| T9 | Enforce the batch minimum and the pilot sample-size gates (never validate a single item). **Done:** `pilot::{screen, dif_batch, admit_dif_batch}` and `revalidation::revalidate_batch_latent` reject a DIF batch below `K_MIN` items (INV-8) and a sample below its floor (`N1_MIN`=300, `N2_MIN`=1500, `N_LATENT_MIN`=3000, §B.6); the per-item DIF math stays pure. `run_epoch` runs the pilot through the gates. `inv8_batch_min.rs` (`AT-PRO-02`). *Third review:* the floors count rows, not distinct respondents (T65) | INV-8, PROTO-006, G-15 | `AT-PRO-02` (batch of one rejected) passes | S |
| T10 | Borderline items get extra reviewers then a clean re-decision; a failed appeal is a pseudo-observation with escrow. **Partial.** Done: `gate::supplementary_review` re-runs bridging and re-decides `b_j` vs the plain threshold; the `lifecycle` `SupplementaryReview` state gains its forward transition (`Event::Resolve` → `Pilot1` or `Rejected(Borderline)`), so a band item reaches a defined terminal (`supplementary_redecision.rs`); the appeal's stake is a pseudo-observation inside `C_a`, escrowed at filing and settled on the terminal (`protocol::appeal`, **T61**, done). *Third review:* the re-decision re-fitted the same panel's ratings, so it relaxed the robust threshold instead of adding reviewers — the expanded panel is **T60**, done | D26, D27, G-15 | `AT-PRO-03` (defined outcome) passes | M |
| T30 | **Retire the band tie-break. Done:** the `aggregate` module (`resolve_band`/`aggregate_pass_probability`) and its tests (`review_aggregation.rs`, `composed_gate.rs`) are deleted; the D26 re-decision (`gate::supplementary_review`) replaces it — re-run bridging, decide `b_j` against the plain threshold. `end_to_end.rs::run_epoch` resolves the band through it, `EXPECTED_POOL={0,6}` preserved; `supplementary_redecision.rs` shows the polarized items 07/08 are *not* passed (bridging, not a vote). The √k anti-collusion stays covered at the scoring layer (`anti_collusion.rs`) and in the T5 weights | PROTO-012, D26, D2 | the `documents_limitation_*` tests are deleted; a polarized panel is *not* resolved by the larger camp | M |
| T31 | **Crowd baseline in code.** Implement the D23 baseline (`p̄_j` = weight-adjusted mean of the reviewers' own predictions) and use it for `E_u` everywhere the base rate is used today. **Done:** `reputation::crowd_baseline`; `honeypot::reviewer_skills` normalizes BSS against `p̄_j`; `AT-REP-02` passes (`level_c.rs`). *Residual:* the sim's `levelc_bss` reference still uses the base rate | D23, G-09, REPUTATION-003 | `AT-REP-02` (consensus follower ≈ 0) passes | M |
| T32 | **Gate Variant-1 DIF behind a calibration flag.** `pilot::stage2_dif`, `revalidation::revalidate_pool`, `logistic_dif`/`mantel_haenszel`/`purify_theta` callers in the production path require a `calibration` feature; the e2e epoch uses Variant 2 **Done:** the `calibration` feature (`scoring`, `protocol`) gates every `group`-taking path; the default build has none (`docs/08` DIF-002 RESOLVED). | D20, G-01, DIF-002 | production build has no code path that accepts a per-respondent `group` | S |
| T11 | Structural rate-limit and one-credential-per-label enforcement (the in-process form; the cryptographic-grade version is T20). **Done:** `credential::{IssuanceRegistry, Issuer::issue_once}` refuse a second credential for a label (`AlreadyIssued`, whatever the secret — `id007_one_credential.rs`); `admission::QuotaLedger` + `deposit_with_identity` reject a proposer over its per-epoch quota (`OverQuota`), keyed on the proven Propose id, with the quota set from `C_a` via `reputation::proposal_rate` (`id008_proposal_quota.rs`) | ID-007, ID-008 | `AT-ID-02/03` pass; over-quota rejected | M |
| T21 | Identity proofs are bound to the specific action (cannot be replayed onto another). Planned for distributed identity; **done in T6:** `nullifier::{prove, verify}` take an action `context` and the protocol binds it to the item and epoch (`protocol/tests/inv9_nym_proof.rs`, `AT-ID-05`). The cryptographic-grade enrollment it sits on is T20. | CS-3, AT-ID-05 | — | M |

### P1.3 · Network and runtime layer (audit block 4)

| Task | What it means (plain) | Audit refs | Done when | Size |
|---|---|---|---|---|
| T12 | A real lifecycle **state machine / orchestrator** (today the flow lives only inside a test) that rejects every invalid transition. **Core done:** `protocol::lifecycle` (`State`/`Event`/`step`/`deposit`) rejects every checkable §9.1 invalid case (`orchestrator.rs`); proof-gated preconditions (T6/T7/T8/T11) enter as explicit inputs; dead `Stage` removed. `end_to_end.rs::run_epoch` is now routed through it: every stage-to-stage decision (gate outcome → pilot entry, pilot verdict → pool or reject) goes through `lifecycle::step` via `orchestrator::run_item`/`ItemVerdicts`, and the pool is exactly the items left in `ActivePool`. *Remaining:* persistence (T13); the `SupplementaryReview` forward transition was done by T30 | §2.2, §9.1, PC-1 | the "invalid cases" of §9.1 are rejected in code | L |
| T14 | **Signed** append-only log + consistency proofs + truncation detection. **Done:** `log::checkpoint()` yields the `Checkpoint{height, head}` the consortium signs, and `log::verify_extends(&prior)` proves the current log consistently extends a trusted signed checkpoint — catching the consistent suffix rewrite (`ForkedHistory`) and truncation (`Truncated`) that `verify()` alone cannot (`log_consistency.rs`, AT-NET-01) | NET-004 / G-14, DS-1 | `AT-NET-01` passes | M |
| T15 | Checkpoint hardening: network id, member-set hash, client monotonic-height rule, fork/equivocation evidence. **Done:** `Checkpoint` gains `network_id` + `member_set_hash` inside the signed message (v2); `Consortium::member_set_hash`; `CheckpointClient` follows one network under a fixed member set — `Rejected(WrongNetwork/WrongMemberSet/InsufficientSignatures)`, `Stale` on a non-monotonic replay, `Forked{trusted,conflicting}` on same-height equivocation (`checkpoint_replay.rs`, AT-NET-03..05). Higher-height fork detection combines with `log::verify_extends` (T14). *Third review:* `Consortium::verify` itself does not compare the member-set hash, and the threshold is unvalidated (T63) | NET-006, §9.4, DS-3 | `AT-NET-03..05` pass | M |
| T16 | Authenticate backup shards (detect a corrupted shard before decoding). **Done:** `Encoded` carries a per-shard `manifest` (`erasure::shard_hash`), and `erasure::reconstruct_verified` drops any present-but-wrong shard (failing its manifest hash) before Reed–Solomon, failing with `TooFewAuthenticShards` rather than trusting a bad one (`shard_authentication.rs`, AT-NET-06) | NET-007, DS-4 | `AT-NET-06` passes | S |

### P1.4 · Consolidation pass

- **Done:** the NaN policy on caller-supplied floats is `f64::total_cmp` (NaN sorts last)
  at every sort site (IQ-2, `docs/08` §0-ter); `clippy -D warnings` and `fmt` green,
  including clippy 1.89's `cloned_ref_to_slice_refs`.
- **Moved:** the adversarial-suite assembly, the coverage figure and the §15 re-walk to
  [Continuous work](#continuous-work); the cross-platform check `AT-BR-04` to Phase 1.3.

### P1.5 · Second review — spec-to-code closure and test consolidation

A second, independent review (2026-09-23) re-read the code against `docs/02`, `docs/05`
and `docs/08`. Line coverage was already ~97% (`cargo llvm-cov --workspace --features
calibration --summary-only`), yet every confirmed defect sat in a file covered at
96–100%: the lines ran, but no test asserted that they *reject* the wrong input. So this
block was two things — close the confirmed defects, then add the kinds of test that
measure *verification*, not execution. Its open tasks moved into the phases: T37 (Phase
2.2), T58 (Phase 3.1), T39 and T45 (Phase 1.3), T46 and T47 (Phase 3.1).

**Confirmed defects:**

| Task | What it means (plain) | Audit refs | Done when | Size |
|---|---|---|---|---|
| T33 | **The review round is checked from the state, not trusted from the caller.** **Done:** distinct panel (`DuplicatePanelist`), one reveal per nym (`AlreadyRevealed`), `Revealing` carries the panel and `Score { outcome }` checks every panelist revealed; `orchestrator::review_round` + `run_item(reviewed, …)`, and the e2e epoch walks a real 9-panel round per item. `AssignReviewers` accepted a panel with a repeated nym (7 slots, 6 people); `Score { all_reveals_in: bool }` took the caller's word that everyone revealed; the same nym could reveal twice. The panel must be distinct, a reveal is accepted once, and `Score` succeeds only when every panelist has revealed — derived from the state. *Third review:* an empty panel, built by hand, is vacuously "all revealed" (T66) | §9.1, PC-1 | duplicate panel / double reveal / partial reveal each rejected in `orchestrator.rs`; `run_item` walks a real review round | S |
| T34 | **The 2PL item fit reports whether it can be trusted.** **Done:** `Fit2pl { a, b, status }`; `stage1_screen` requires `Converged` (`lifecycle.rs::pilot_stage1_fails_an_item_whose_2pl_fit_is_separated`). `fit_2pl_item` dropped the logistic status, so a separated fit's huge slope passed the `a ≥ A_MIN` screen | OPT-001, T2 | a perfectly separating item fails the screen | S |
| T35 | **The latent re-check reports the specified quantity and only acts on a trustworthy fit.** **Done:** `MixtureDif::dif = 2\|δ\|`, `MIXTURE_DIF_MAX = 1.0` (provisional), `revalidation::latent_flags` gates on convergence and BIC (`latent_revalidation.rs`). Reports `DIF_j = 2\|δ_j\|` (the b-gap `docs/02` defines, not the half-gap) and flags nothing when the free fit did not converge or the BIC does not favour two classes. The rejection threshold on the b-gap stays **1.0** (the literature 0.5 applied to this estimator flags all 8 items of the one-biased-item fixture) until T24/T25 calibrate it | DIF-006, DIF-004, D24 | threshold on `DIF_j`; non-converged / BIC ≤ 0 flags nothing; docs/02 and docs/08 record the provisional value | S |
| T36 | **Degenerate inputs have a defined answer.** **Done:** θ ≡ 0 with no spread, `point_biserial` = 0 with no variance, malformed tally never approves, `DuplicateCandidate` (`degenerate_inputs.rs`, `lifecycle.rs`, `properties.rs`). `standardize` on an empty or constant vector returned NaN (IRT-001); `point_biserial` divided by a zero variance; `change_approved` accepted `votes_for > total_eligible`; `stratified_sortition` could seat the same id twice | IRT-001, §6 | each case has a documented, conservative result and a test | S |
| T38 | **A higher checkpoint must extend the trusted one.** **Done:** `CheckpointClient::ingest_with_log` (`checkpoint_fork.rs`). `CheckpointClient::ingest` accepted any threshold-signed checkpoint of greater height; a client that holds the log must also require `log::verify_extends(prior)` before moving its trust | NET-006 residual, T14/T15 | a threshold-signed higher-height fork is reported, not accepted | S |
| T48 | **The bridging fit can stop far from a minimum, and depends on the seed.** Found by the T42 property tests: the relative-progress stall criterion ended a slow Armijo-only descent early and reported `Converged` (on the fixture, seed 5 stopped at 4× the objective of the other seeds, with `b_j` up to 1.08); on random data several local minima exist and 8 seeds disagreed on some `b_j` by > 0.01 in 57–84% of cases, sometimes across `τ`. **Decided (2026-09-24): both** — a strong-Wolfe line search with a gradient-based stop, so each start really reaches a minimum, *and* a deterministic multi-start keeping the lowest objective, so the choice among genuine minima does not depend on the seed. Changed the golden outputs (regenerated on purpose) and multiplies the fit time by the number of starts. **Done:** strong-Wolfe `lbfgs`; `fit` with `n_starts = 8` and a canonical sign of `f`; disjoint seed sets disagree in 2/300 random cases (was 105/300), the fixture agrees across 8 base seeds to 1e-4; Rosenbrock 670 → 51 gradients. The progress-stall stop is kept (SciPy has an equivalent `ftol`): the measurements showed the seed-dependence came from distinct minima, not from it | OPT-001, BRIDGE-001, T40, T45 | the verdict of every item on the fixture is the same for every seed; a fit reported `Converged` has `‖∇‖_∞` near `g_tol` | M |

**Spec features:**

| Task | What it means (plain) | Audit refs | Size |
|---|---|---|---|
| T40 | Mixture DIF: multi-start with deterministically derived seeds (keep the best converged likelihood), per-class discrimination (non-uniform DIF), `G > 2` selected by BIC, analytic gradient **Done:** the specified model `σ(a_jg(θ − b_jg))` with `G ∈ {1..4}` and uniform/non-uniform chosen by BIC (staged), 4 seeded starts per candidate, analytic gradient, classes < 5% excluded from the gap; `latent_flags` requires ≥ 2 classes. ~4 s per 3000×8 batch; `scoring` is built optimized in dev/test (same bits). *Open:* a threshold for the discrimination gap (`a_gap`, reported, not yet a verdict) — T24/T25 | DIF-004, §6.6 | M |

**Test consolidation:**

| Task | What it means (plain) | Done when | Size |
|---|---|---|---|
| T41 | **Mutation testing** (`cargo-mutants`) over `protocol` and `scoring`, then `network`/`identity`: every surviving mutant is either killed by a new test or justified. Run periodically (not on every push: slow). **Done:** 1482 mutants; survivors 220 → 0 real (32 equivalent, each justified); found the bridging tolerance hiding gradient errors and `lbfgs` reporting a failed line search as `Converged` (both fixed). Report: `docs/11-mutation-testing.md` | a report with the survivors triaged | M |
| T42 | **Property tests where there were none:** `scoring` and `identity`. **Identity half done:** proptest suites in `credential`, `nullifier`, `oprf` (in-crate, since the byte encodings and the quorum entry point are private) and `tests/{properties,threshold_bbs}.rs` — any issued credential (single and threshold) verifies, any single altered byte of the credential or nullifier-proof encoding is rejected, a proof fails under another role or an altered context, nullifiers/nyms distinct per role and per person, any quorum of ≥ `t` distinct members yields the same label, a quorum with a duplicate, a missing or a foreign index is refused, a shifted partial fails its DLEQ. **Scoring half done:** `scoring/tests/properties.rs` + in-crate `bridging` proptests — bit-for-bit determinism, observation-order invariance, zero weight = absent (bit-for-bit, after fixing the start point), finite outputs, bootstrap ≤ full fit, `f`-sign symmetry of the objective, gradient vs central differences anywhere; `C_a` ∈ (0, 1) and monotone in quality, crowd baseline within the predictions, BSS ≤ 1 and 0 for the baseline, EMA between old and new; correlation matrix well-formed, cluster discount never a boost (INV-14); θ standardized, point-biserial scale-free. *Not a property:* monotonicity of `b_j` in agreeing ratings — it fails for start-point reasons (T48) | proptest suites in both crates | M |
| T43 | **Model-based tests of the state machines** (`lifecycle`, `orchestrator`, `CheckpointClient`): random event sequences checked against a small reference model. **Done:** three proptest suites, each with a reference model written from `docs/08` §9.1/§9.4 rather than from the code, checked at every step (the exact next state, or the exact `Invalid`/`CheckpointUpdate`), and a fixed-seed test that the walks reach every state and every rejection. `protocol/tests/lifecycle_model.rs`: walks of `deposit`/`step` that mix the expected event with out-of-order, outsider, repeated, copied/lifted/opaque-commitment and out-of-range ones. Invariants: `Score` only once every panelist has revealed; no double commit or reveal; a distinct panel of odd size in [7, 11]; a rejected event changes nothing (the accepted events alone replay the walk); `Rejected`/`Retired` are never left; `ActivePool` only from `Pilot2`, after `Pilot1`. `protocol/tests/orchestrator_model.rs`: `review_round` + `run_item` against a whole-round model, for complete, partial, empty, outsider, double-judge, bad-probability and bad-panel rounds, from any start state. `network/tests/checkpoint_model.rs`: `ingest`/`ingest_with_log` against a model in which a log extends a checkpoint iff its payloads are a prefix. Invariants: the trusted height never decreases; a wrong network or member set, or too few signatures (down to a whole quorum over another message), never changes the state; two heads at one height always give `Forked`; with the log, a higher checkpoint is accepted iff the log extends both. On their own, the suites kill every viable cargo-mutants mutant of `lifecycle.rs` (38) and `consortium.rs` (24), and 20 of 21 hand mutations of `review_round`/`run_item` and of the `ingest_with_log` closure (the 21st is equivalent). **Found and fixed:** `ingest_with_log` reported a local log shorter than the *trusted* checkpoint as `LocalLogDiverged` instead of `LogBehind` (`checkpoint_fork.rs::a_local_log_behind_the_trusted_checkpoint_is_behind_not_diverged`, NET-006). **Pinned, still open:** a round in which a panelist never commits or never reveals can never be scored (T58) | proptest state-machine suites | M |
| T44 | **Fuzzing and panic audit** of every byte decoder with `cargo-fuzz`; classify the `unwrap`/`expect`/`assert` in `src/` as internal invariant vs external input, and turn the latter into errors. **Done for `network` and `identity`** (`docs/12-panic-audit.md`): 35 sites outside tests classified; nine external-input crashes fixed — the `.ots` parser (overlong varint panic, declared-length allocation abort, unbounded `Hexlify` growth, fork amplification: now a bounded pre-scan before the library), erasure layout (count overflow, `orig_len` allocation), `merkle_proof` out-of-range index, VOPRF anchors over `u16::MAX` bytes, fallible transcripts on the verify paths; eight `cargo fuzz` targets in `crates/{network,identity}/fuzz/`, and stable no-panic properties (`hostile_input.rs`) on every push. *Remaining, now inside the phases:* `scoring` entry points with T62 (Phase 1.3), the codecs T18 adds (Phase 2.3), `protocol` entry points with T46 (Phase 3.1) | no panic on arbitrary bytes; the classification is recorded | M |
