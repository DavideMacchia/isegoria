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
| T6 | **The protocol stops trusting bare pseudonyms.** Every entry point requires a verified identity proof (nullifier); reputation and rate limits are keyed on it. **Done:** `protocol::admission::{admit, NullifierSet}` verifies a role `NullifierProof` and returns the proven `NullifierProof::id()` (never `derive_nym`); `deposit_with_identity` and `review::submit_review` are the identity-gated entry points; `nullifier::{prove,verify}` gained an action `context` so a proof cannot be replayed (AT-ID-05); `end_to_end.rs::run_epoch` deposits through the gate. `inv9_nym_proof.rs` covers `AT-PRO-01`, `AT-ID-05` and per-item dedup. *Note:* the cryptographic-grade enrollment/replay hardening and the per-credential quota stay **T20/T11** | PROTO-007 / G-04, INV-9 | `AT-PRO-01`, `AT-ID-05` pass | L |
| T7 | A blind review commits to *who* cast it and *which* item, and only that person can reveal it. **Done:** `review::commit`/`reveal` bind the committer nym and item cid (`H(prob, nonce, committer, item)`); the `lifecycle` `Revealing` state carries the item and the reveal recomputes against the revealer + item, so a copied commitment cannot be opened by anyone else. `inv12_commit_binding.rs` (`AT-BR-06`) | CRYPTO-007 / INV-12 | `AT-BR-06` (commitment-copying blocked) passes | S |
| T8 | The "luck" (lottery, reviewer assignment, honeypot, sortition) comes from the signed checkpoint, so nobody can pick their own reviewers. **Done:** `randomness::Beacon::from_checkpoint` + `seed(purpose, index)` = `H(signed head ‖ height ‖ purpose ‖ index)`, and the `_from_beacon` wrappers in `lottery`/`review`/`honeypot`/`governance` seed each draw from it; reviewer assignment keys on a byte-independent admitted slot, so the panel does not depend on draft bytes. `inv10_checkpoint_seed.rs` (`AT-BR-05`) | D29 / INV-10 / G-05 | `AT-BR-05` (seed grinding blocked) passes | M |
| T9 | Enforce the batch minimum and the pilot sample-size gates (never validate a single item). **Done:** `pilot::{screen, dif_batch, admit_dif_batch}` and `revalidation::revalidate_batch_latent` reject a DIF batch below `K_MIN` items (INV-8) and a sample below its floor (`N1_MIN`=300, `N2_MIN`=1500, `N_LATENT_MIN`=3000, §B.6); the per-item DIF math stays pure. `run_epoch` runs the pilot through the gates. `inv8_batch_min.rs` (`AT-PRO-02`) | INV-8, PROTO-006, G-15 | `AT-PRO-02` (batch of one rejected) passes | S |
| T10 | Borderline items get extra reviewers then a clean re-decision; a failed appeal is a pseudo-observation with escrow. **Done:** `gate::supplementary_review` re-runs bridging over the (expanded) panel and re-decides `b_j` vs the plain threshold; the `lifecycle` `SupplementaryReview` state gains its forward transition (`Event::Resolve` → `Pilot1` or `Rejected(Borderline)`), so a band item reaches a defined terminal (`supplementary_redecision.rs`). Failed-appeal escrow: `gate::settle_appeal` (tested in `lifecycle.rs`); full escrow bookkeeping deferred | D26, D27, G-15 | `AT-PRO-03` (defined outcome) passes | M |
| T30 | **Retire the band tie-break. Done:** the `aggregate` module (`resolve_band`/`aggregate_pass_probability`) and its tests (`review_aggregation.rs`, `composed_gate.rs`) are deleted; the D26 re-decision (`gate::supplementary_review`) replaces it — re-run bridging, decide `b_j` against the plain threshold. `end_to_end.rs::run_epoch` resolves the band through it, `EXPECTED_POOL={0,6}` preserved; `supplementary_redecision.rs` shows the polarized items 07/08 are *not* passed (bridging, not a vote). The √k anti-collusion stays covered at the scoring layer (`anti_collusion.rs`) and in the T5 weights | PROTO-012, D26, D2 | the `documents_limitation_*` tests are deleted; a polarized panel is *not* resolved by the larger camp | M |
| T31 | **Crowd baseline in code.** Implement the D23 baseline (`p̄_j` = weight-adjusted mean of the reviewers' own predictions) and use it for `E_u` everywhere the base rate is used today. **Done:** `reputation::crowd_baseline`; `honeypot::reviewer_skills` normalizes BSS against `p̄_j`; `AT-REP-02` passes (`level_c.rs`). *Residual:* the sim's `levelc_bss` reference still uses the base rate | D23, G-09, REPUTATION-003 | `AT-REP-02` (consensus follower ≈ 0) passes | M |
| T32 | **Gate Variant-1 DIF behind a calibration flag.** `pilot::stage2_dif`, `revalidation::revalidate_pool`, `logistic_dif`/`mantel_haenszel`/`purify_theta` callers in the production path require a `calibration` feature; the e2e epoch uses Variant 2 | D20, G-01, DIF-002 | production build has no code path that accepts a per-respondent `group` | S |
| T11 | Structural rate-limit and one-credential-per-label enforcement (the in-process form; the cryptographic-grade version is T20). **Done:** `credential::{IssuanceRegistry, Issuer::issue_once}` refuse a second credential for a label (`AlreadyIssued`, whatever the secret — `id007_one_credential.rs`); `admission::QuotaLedger` + `deposit_with_identity` reject a proposer over its per-epoch quota (`OverQuota`), keyed on the proven Propose id, with the quota set from `C_a` via `reputation::proposal_rate` (`id008_proposal_quota.rs`) | ID-007, ID-008 | `AT-ID-02/03` pass; over-quota rejected | M |

### P1.3 · Network and runtime layer (audit block 4) — build what is missing

| Task | What it means (plain) | Audit refs | Done when | Size |
|---|---|---|---|---|
| T12 | A real lifecycle **state machine / orchestrator** (today the flow lives only inside a test) that rejects every invalid transition. **Core done:** `protocol::lifecycle` (`State`/`Event`/`step`/`deposit`) rejects every checkable §9.1 invalid case (`orchestrator.rs`); proof-gated preconditions (T6/T7/T8/T11) enter as explicit inputs; dead `Stage` removed. `end_to_end.rs::run_epoch` is now routed through it: every stage-to-stage decision (gate outcome → pilot entry, pilot verdict → pool or reject) goes through `lifecycle::step` via `orchestrator::run_item`/`ItemVerdicts`, and the pool is exactly the items left in `ActivePool`. *Remaining:* persistence (T13) and the `SupplementaryReview` forward transition (T30) | §2.2, §9.1, PC-1 | the "invalid cases" of §9.1 are rejected in code | L |
| T13 | **Persistent state** so a restart recovers (everything is in-memory today) | §10.6 | state survives a process restart | M |
| T14 | **Signed** append-only log + consistency proofs + truncation detection. **Done:** `log::checkpoint()` yields the `Checkpoint{height, head}` the consortium signs, and `log::verify_extends(&prior)` proves the current log consistently extends a trusted signed checkpoint — catching the consistent suffix rewrite (`ForkedHistory`) and truncation (`Truncated`) that `verify()` alone cannot (`log_consistency.rs`, AT-NET-01) | NET-004 / G-14, DS-1 | `AT-NET-01` passes | M |
| T15 | Checkpoint hardening: network id, member-set hash, client monotonic-height rule, fork/equivocation evidence. **Done:** `Checkpoint` gains `network_id` + `member_set_hash` inside the signed message (v2); `Consortium::member_set_hash`; `CheckpointClient` follows one network under a fixed member set — `Rejected(WrongNetwork/WrongMemberSet/InsufficientSignatures)`, `Stale` on a non-monotonic replay, `Forked{trusted,conflicting}` on same-height equivocation (`checkpoint_replay.rs`, AT-NET-03..05). Higher-height fork detection combines with `log::verify_extends` (T14) | NET-006, §9.4, DS-3 | `AT-NET-03..05` pass | M |
| T16 | Authenticate backup shards (detect a corrupted shard before decoding). **Done:** `Encoded` carries a per-shard `manifest` (`erasure::shard_hash`), and `erasure::reconstruct_verified` drops any present-but-wrong shard (failing its manifest hash) before Reed–Solomon, failing with `TooFewAuthenticShards` rather than trusting a bad one (`shard_authentication.rs`, AT-NET-06) | NET-007, DS-4 | `AT-NET-06` passes | S |
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

### P1.5 · Second review — spec-to-code closure and test consolidation

A second, independent review (2026-09-23) re-read the code against `docs/02`, `docs/05`
and `docs/08`. Line coverage is already ~97% (`cargo llvm-cov --workspace --features
calibration --summary-only`), yet every confirmed defect below sits in a file covered at
96–100%: the lines run, but no test asserts that they *reject* the wrong input. So this
block is two things — close the confirmed defects, then add the kinds of test that
measure *verification*, not execution. Only property-based tests exist today in
`network` (4) and `protocol` (3); `scoring` and `identity` have none.

**Confirmed defects (fix first, each with a failing-first regression test):**

| Task | What it means (plain) | Audit refs | Done when | Size |
|---|---|---|---|---|
| T33 | **The review round is checked from the state, not trusted from the caller.** **Done:** distinct panel (`DuplicatePanelist`), one reveal per nym (`AlreadyRevealed`), `Revealing` carries the panel and `Score { outcome }` checks every panelist revealed; `orchestrator::review_round` + `run_item(reviewed, …)`, and the e2e epoch walks a real 9-panel round per item. `AssignReviewers` accepts a panel with a repeated nym (7 slots, 6 people); `Score { all_reveals_in: bool }` takes the caller's word that everyone revealed; the same nym can reveal twice. The panel must be distinct, a reveal is accepted once, and `Score` succeeds only when every panelist has revealed — derived from the state | §9.1, PC-1 | duplicate panel / double reveal / partial reveal each rejected in `orchestrator.rs`; `run_item` walks a real review round | S |
| T34 | **The 2PL item fit reports whether it can be trusted.** **Done:** `Fit2pl { a, b, status }`; `stage1_screen` requires `Converged` (`lifecycle.rs::pilot_stage1_fails_an_item_whose_2pl_fit_is_separated`). `fit_2pl_item` drops the logistic status, so a separated fit's huge slope passes the `a ≥ A_MIN` screen. Return the status; the stage-1 screen fails an item whose fit is not `Converged` | OPT-001, T2 | a perfectly separating item fails the screen | S |
| T35 | **The latent re-check reports the specified quantity and only acts on a trustworthy fit.** **Done:** `MixtureDif::dif = 2|δ|`, `MIXTURE_DIF_MAX = 1.0` (provisional), `revalidation::latent_flags` gates on convergence and BIC (`latent_revalidation.rs`). Report `DIF_j = 2|δ_j|` (the b-gap `docs/02` defines, not the half-gap) and flag nothing when the free fit did not converge or the BIC does not favour two classes. The rejection threshold on the b-gap stays **1.0** (the current behaviour, now stated) — the literature 0.5 applied to this estimator flags all 8 items of the one-biased-item fixture — until T24/T25 calibrate it | DIF-006, DIF-004, D24 | threshold on `DIF_j`; non-converged / BIC ≤ 0 flags nothing; docs/02 and docs/08 record the provisional value | S |
| T36 | **Degenerate inputs have a defined answer.** **Done:** θ ≡ 0 with no spread, `point_biserial` = 0 with no variance, malformed tally never approves, `DuplicateCandidate` (`degenerate_inputs.rs`, `lifecycle.rs`, `properties.rs`). `standardize` on an empty or constant vector returns NaN (IRT-001); `point_biserial` divides by a zero variance; `change_approved` accepts `votes_for > total_eligible`; `stratified_sortition` can seat the same id twice | IRT-001, §6 | each case has a documented, conservative result and a test | S |
| T37 | **The beacon is not grind-free yet (reopens INV-10).** **Decision pending (maintainer):** (a) a unique threshold signature over the epoch number, drand-style — unbiasable, but inherits the trusted-dealer caveat until T19; or (b) commit-reveal among members bound before the deposit window closes — simpler, but the last revealer can abort and bias one bit. The seed is `H(checkpoint head ‖ …)` and the head is a deterministic function of the log content: whoever orders or includes the last deposits before the checkpoint — the publisher, a colluding threshold of signers, or a last depositor who sees the log — can try variants and keep the preferred seed. Separate the randomness from the state commitment (commit-reveal among members, or a threshold signature/VRF over the epoch as the beacon) | INV-10, CRYPTO-008, D29 | a test shows the last depositor cannot choose among seeds; docs/08 status honest meanwhile | M |
| T38 | **A higher checkpoint must extend the trusted one.** **Done:** `CheckpointClient::ingest_with_log` (`checkpoint_fork.rs`). `CheckpointClient::ingest` accepts any threshold-signed checkpoint of greater height; a client that holds the log must also require `log::verify_extends(prior)` (or a consistency proof) before moving its trust | NET-006 residual, T14/T15 | a threshold-signed higher-height fork is reported, not accepted | S |
| T48 | **The bridging fit can stop far from a minimum, and depends on the seed.** Found by the T42 property tests: the relative-progress stall criterion ends a slow Armijo-only descent early and reports `Converged` (on the fixture, seed 5 stops at 4× the objective of the other seeds, with `b_j` up to 1.08); on random data several local minima exist and 8 seeds disagree on some `b_j` by > 0.01 in 57–84% of cases, sometimes across `τ`. **Decided (2026-09-24): both** — a strong-Wolfe line search with a gradient-based stop, so each start really reaches a minimum, *and* a deterministic multi-start keeping the lowest objective, so the choice among genuine minima does not depend on the seed. Changes the golden outputs (regenerate on purpose) and multiplies the fit time by the number of starts. Not started | OPT-001, BRIDGE-001, T40, T45 | the verdict of every item on the fixture is the same for every seed; a fit reported `Converged` has `‖∇‖_∞` near `g_tol` | M |

**Spec features still missing (tracked, not defects of the code that exists):**

| Task | What it means (plain) | Audit refs | Size |
|---|---|---|---|
| T39 | Bridging: `n_min = 30` (reviewers below it do not define the `f` axis) and `d = 2` | BRIDGE-00x, PROTO-003, `docs/02` §A.4 | M |
| T40 | Mixture DIF: multi-start with deterministically derived seeds (keep the best converged likelihood), per-class discrimination (non-uniform DIF), `G > 2` selected by BIC, analytic gradient | DIF-004, §6.6 | M |

**Test consolidation (what gives confidence beyond coverage):**

| Task | What it means (plain) | Done when | Size |
|---|---|---|---|
| T41 | **Mutation testing** (`cargo-mutants`) over `protocol` and `scoring`, then `network`/`identity`: every surviving mutant is either killed by a new test or justified. Run periodically (not on every push: slow). **Done:** 1482 mutants; survivors 220 → 0 real (32 equivalent, each justified); found the bridging tolerance hiding gradient errors and `lbfgs` reporting a failed line search as `Converged` (both fixed). Report: `docs/11-mutation-testing.md` | a report with the survivors triaged | M |
| T42 | **Property tests where there are none:** `scoring` (bit-for-bit determinism, permutation invariance, bootstrap ≤ full fit, no NaN on finite input, monotonicity of `b_j` in agreeing ratings, `f` sign symmetry, zero weight = absent observation) and `identity` (any valid credential verifies, any flipped byte fails, nullifier distinct per role, any `t`-quorum reconstructs the same value, duplicate quorum always refused). **Identity half done:** proptest suites in `credential`, `nullifier`, `oprf` (in-crate, since the byte encodings and the quorum entry point are private) and `tests/{properties,threshold_bbs}.rs` — any issued credential (single and threshold) verifies, any single altered byte of the credential or nullifier-proof encoding is rejected, a proof fails under another role or an altered context, nullifiers/nyms distinct per role and per person, any quorum of ≥ `t` distinct members yields the same label, a quorum with a duplicate, a missing or a foreign index is refused, a shifted partial fails its DLEQ. **Scoring half done:** `scoring/tests/properties.rs` + in-crate `bridging` proptests — bit-for-bit determinism, observation-order invariance, zero weight = absent (bit-for-bit, after fixing the start point), finite outputs, bootstrap ≤ full fit, `f`-sign symmetry of the objective, gradient vs central differences anywhere; `C_a` ∈ (0, 1) and monotone in quality, crowd baseline within the predictions, BSS ≤ 1 and 0 for the baseline, EMA between old and new; correlation matrix well-formed, cluster discount never a boost (INV-14); θ standardized, point-biserial scale-free. *Not a property:* monotonicity of `b_j` in agreeing ratings — it fails for start-point reasons (T48) | proptest suites in both crates | M |
| T43 | **Model-based tests of the state machines** (`lifecycle`, `orchestrator`, `CheckpointClient`): random event sequences checked against a small reference model; invariants — no score without every reveal, no double commit/reveal, trusted height never decreases | proptest state-machine suites | M |
| T44 | **Fuzzing and panic audit** of every byte decoder (`oprf`, `erasure`, `consortium`, and later the network codecs) with `cargo-fuzz`; classify the ~70 `unwrap`/`expect`/`assert` in `src/` as internal invariant vs external input, and turn the latter into errors. **Done for `network` and `identity`** (`docs/12-panic-audit.md`): 35 sites outside tests classified; nine external-input crashes fixed — the `.ots` parser (overlong varint panic, declared-length allocation abort, unbounded `Hexlify` growth, fork amplification: now a bounded pre-scan before the library), erasure layout (count overflow, `orig_len` allocation), `merkle_proof` out-of-range index, VOPRF anchors over `u16::MAX` bytes, fallible transcripts on the verify paths; eight `cargo fuzz` targets in `crates/{network,identity}/fuzz/`, and stable no-panic properties (`hostile_input.rs`) on every push. *Remaining:* `scoring`/`protocol` entry points (their sites are classified; not touched while they were being changed in parallel); fuzz targets for the codecs T18 adds | no panic on arbitrary bytes; the classification is recorded | M |
| T45 | **Wider differential oracles:** random datasets vs SciPy (not only the fixed fixture), analytic vs numerical gradient for the mixture, `lbfgs` on functions with known minima (Rosenbrock, ill-conditioned quadratics). Known from T41: Rosenbrock needs ~670 gradients vs SciPy's ~46 (Armijo-only line search; consider strong-Wolfe), and the relative-progress stall can report `Converged` with `‖g‖_∞ ≫ g_tol` near `f = 0` | oracle suite runs under the pinned sim env | M |
| T46 | **Validated boundary types** (`Probability`, `ValidatedRatings`, a distinct odd `Panel` of 7–11) so whole classes of bad input cannot be constructed | public entry points take the validated types | M |
| T47 | *(optional)* **Bounded model checking (Kani)** on small decisive functions: `bridging_gate`, `change_approved`, panel admission, `lagrange_at_zero` with distinct indices | proofs run in CI | S |

T24 (below) is the statistical counterpart: it is the only way to say the detectors
"work", and it now explicitly includes the **zero-biased** condition (false-positive
rate), unbalanced classes, `δ` below 0.9, and the T35 threshold choice.

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
- T33–T36 are small and independent; do them before T41 (mutation testing), so the
  mutants report measures the fixed code. T35's threshold is final only after T24/T25.
- T37 (beacon) and T38 (higher-height fork) are prerequisites for T18 (transport).
- Nothing in Phase 1 requires the *external* gates; but the README/docs MUST keep
  calling the system a reference/testnet until T23, T26 and T27 are done.
