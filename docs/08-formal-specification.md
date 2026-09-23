# Isegoria — Formal Specification and Verification Audit

| | |
|---|---|
| **Status** | DRAFT — independent audit of commit `c4a09b1f778037c9d80dd7a85f3243311a1714ec` (2026-09-16). Not yet reviewed by the maintainers. |
| **Intended path** | `docs/08-formal-specification.md` |
| **Companion** | `docs/07-verification-and-assurance.md` (methodology), `docs/09-verification-matrix.md` (to be created from §15 of this document) |
| **Normative language** | MUST / MUST NOT / SHOULD / SHOULD NOT / MAY as in RFC 2119. |
| **Evidence vocabulary** | HYPOTHESIS, IMPLEMENTED, TESTED, REPRODUCED, INDEPENDENTLY_REVIEWED, SCIENTIFICALLY_CHARACTERIZED, PRODUCTION_CANDIDATE, PRODUCTION_READY; plus **NOT ESTABLISHED** when no evidence exists. A status is never assigned above what the cited evidence supports. |

---

## 0. How this document was produced, and what it did not do

This specification was written by an independent auditor with read access to the full repository. Method:

1. Every file in the repository was read: `README.md`, `ARCHITECTURE.md`, all of `docs/`, all four crates (`crates/{scoring,identity,network,protocol}`), all tests, all fixtures, `sim/`, `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `.github/workflows/ci.yml`.
2. The Python simulations were **executed by the auditor** (numpy 2.4.4, scipy 1.17.1) and their output compared with the documented "expected results" and with the committed fixtures.
3. Small independent probes were run in Python to falsify specific claims (BRIDGE-005, COLLUSION-002, COLLUSION-004, NET-003, REPRO-003).
4. **The Rust test suite was NOT executed by the auditor** (no Rust toolchain was available in the audit environment). Every statement below about what the Rust tests assert is based on reading the test source, not on observing a green run. The repository's CI configuration (`.github/workflows/ci.yml`) claims `cargo test --workspace` on every push; that claim was not independently verified.
5. Dependency source was inspected where a correctness argument depended on library behaviour (`opentimestamps` 0.2.0 step-output computation, NET-008).

Nothing in this document infers correctness from the existing code or documentation. Where the documentation makes a claim, this document states what evidence exists for it, and the status assigned is the lowest status consistent with that evidence.

---

## 0-bis. Remediation log (maintainer, post-audit)

The audit body below is a **snapshot of commit `c4a09b1`** and is left unedited: its findings were real at that commit. This log records the maintainer fixes applied afterwards. It does not re-run the audit; a finding is marked RESOLVED only where the code-level defect is closed and a regression test pins it. Design-level and multi-part findings remain open beyond the specific sub-fix noted.

Fixes landed on branch `fix/audit-concrete-bugs`, commit `289aae3` — six concrete, design-free code defects. Workspace: 149 tests green; `cargo fmt` and `clippy -D warnings` clean.

| Finding | Was | Now | Fix | Regression test |
|---|---|---|---|---|
| COLLUSION-004 (§5.4) | INV-14 VIOLATED — sub-unit weights boosted (`0.25 → 0.5`) | **RESOLVED** | per-node multiplier `s^{α−1}` and `sublinear_group_weight` capped so the transform never increases a weight | `anti_collusion.rs::discount_never_increases_a_weight`, `::group_weight_is_capped_at_its_raw_total` (AT-COL-04) |
| NET-003 (§5.8, §10.2) | DEFECT — duplicate-last-node; `root([x,y,z]) == root([x,y,z,z])` | **RESOLVED** | RFC 6962 promotion of the odd node; `merkle_root`/`merkle_proof` share `next_level` | `integrity.rs::root_commits_to_the_leaf_count`, `::inclusion_proofs_verify_at_every_size` (AT-NET-02) |
| PROTO-011 / G-18 (§5.9) | DEFECT — `Draft::content_id` fields not length-prefixed | **RESOLVED** | each field length-prefixed before hashing | `lifecycle.rs::content_id_is_unambiguous_across_the_field_boundary` (AT-PRO-04) |
| REPUTATION-003 guard (§5.4) | `brier_skill_score` divides by zero on a zero-variance baseline | **RESOLVED (sub-fix)** — the guard only; the baseline choice (crowd vs base-rate) stays open (G-09) | `den == 0.0 → 0.0` (finite, neutral) | `level_c.rs::brier_skill_score_is_finite_when_the_baseline_is_perfect` (AT-REP-03) |
| DIF-003 guard (§6.5) | `mantel_haenszel` panics on NaN θ (`partial_cmp().unwrap()`) | **RESOLVED (sub-fix)** — the panic only; the stratification/significance gaps stay open | incomparable values treated as equal | `level_b.rs::mantel_haenszel_tolerates_a_nan_theta` |
| ID-003(ii) (§5.5) | `label_with_quorum` accepts duplicate indices → wrong label silently | **RESOLVED (sub-fix)** — duplicate-index guard only; DKG/transport/external review stay open | reject non-distinct quorum indices | `oprf.rs::a_quorum_with_duplicate_indices_is_refused` (AT-ID-04) |

Everything else in §14 (gaps), §16 (acceptance gate) and the rest of §15 is unchanged: the wiring gaps (BRIDGE-007, PROTO-007) and the design-open questions (§17) are untouched by these fixes.

## 0-ter. Post-remediation re-check (auditor, 2026-09-17)

Re-audit of `e43f1bf` against `c4a09b1`, by the same auditor. This time the Rust suite **was executed** (rustc 1.89.0 from the Ubuntu archive — the pinned 1.86.0 was not reachable; results are functional, not bit-level): at `e43f1bf`, 149 passed / 2 ignored; on this branch, 154 passed / 2 ignored, `cargo fmt --check` and `clippy --all-targets -D warnings` clean under clippy 1.89 (which adds one lint, `cloned_ref_to_slice_refs`, that 1.86 does not have; fixed in `lifecycle.rs:478`).

**The six fixes of §0-bis are confirmed**, with independent replication of the Merkle and discount corrections (promotion construction: proofs verify for n = 1…39 and `root(L) ≠ root(L ++ [last])`; `discount_weights([0.25]) = 0.25`, 400 clones → 20). Two caveats:

- **DIF-003 guard.** `partial_cmp().unwrap_or(Equal)` avoids the panic but is not a total order (NaN "equals" everything), and Rust ≥ 1.81 sorts are permitted to detect a non-total comparator and panic; the 6-element regression test cannot exercise that path. Replaced on this branch by `f64::total_cmp` (NaN sorts last, deterministically) at all five float-sort sites — `dif.rs`, `governance.rs`, `review.rs`, `reputation.rs::median`, `blueprint.rs` — closing IQ-2. Tests: `level_b.rs::mantel_haenszel_tolerates_nan_theta_at_sort_detection_sizes` (n = 200), `lifecycle.rs::float_sorts_tolerate_nan_positions_without_panicking`.
- **REPRO-003, now with the repository's own guard.** `cargo test -p scoring --test fixture_drift -- --ignored` **fails** in a numpy 2.4.4 / scipy 1.17.1 environment (`expected_levelA.csv` token 10: `0.107583` vs `0.107804`). Recorded in `crates/scoring/tests/fixtures/PROVENANCE.md`; roadmap T4 stands.

**New finding — PROTO-012 (`protocol::aggregate`, merged in `b139acf`, not covered by §0-bis).** `resolve_band(aggregate_pass_probability(p, w), 0.5)` is a weighted arithmetic mean of the *same* ratings the bridging model already consumed, compared to 0.5. That is the "simple average" `docs/01` D2 rejects, applied as the deciding rule for the one class of items (the band) where bridging is undecided; it carries no cross-axis requirement. Evidence, now pinned by `documents_limitation_*` tests: on the oracle fixtures the rule advances **every** item, including 08 and 09 that bridging rejects for polarization (unit-weight means 0.593 and 0.708); a 120-vs-80 polarized panel at 0.9/0.3 with no cartel resolves in favour (0.66); the "√k never flips" scenario holds only for exact-copy cartels with k < 576 (flip at k = 576), and a jittered cartel (σ ≈ 0.05) is not clustered at all (COLLUSION-002), flipping the same panel at k ≈ 65. On the fixtures all three band items (1, 5, 6) have mean ≥ 0.83, so "resolved by review" and "passed by default" are not distinguishable by the e2e test. The module also does **not** address BRIDGE-007: `bridging::fit` remains unweighted, so the discount changes this tie-break and nothing else. Finally, the mechanism contradicts `docs/01` D26 (decided the same day): "more reviewers, then a clean re-decision against the plain threshold". `ARCHITECTURE.md`'s "the review aggregation is **done** and **wired into the epoch**" has been corrected on this branch to "provisional tie-break"; roadmap T5 and T10 correctly remain open, and T30 is added. Status: **IMPLEMENTED (tie-break); INV-2 substantively not provided for band items; D26 NOT IMPLEMENTED.**

**Decisions D17–D31 vs. code.** Consistent with §17 (Q-15, D15's "preferred" separation, remains undecided). Three are *decided but not implemented* and should be tracked as such: D20 (`pilot::stage2_dif` is still on the production e2e path with no calibration gate — T32), D23 (`base_rate_baseline` was the only baseline — now RESOLVED@T31 via `reputation::crowd_baseline`), D26 (above — T30).

**Matrix deltas (§15).** Added PROTO-012; IQ-2 closed; REPRO-003 now carries repository-native failing evidence; ID-003 unchanged beyond §0-bis; everything else as in §0-bis.

---

---

## 1. Scope

This specification covers the system described by `docs/00`–`docs/06` and implemented in the Cargo workspace at the audited commit:

- the **scoring engine** (`crates/scoring`): bridging (Level A), IRT and DIF (Level B), reputation (Level C), anti-collusion;
- the **identity layer** (`crates/identity`): enrollment, uniqueness label (single-server VOPRF and threshold OPRF), BBS+ credential (single and threshold issuer), role pseudonyms, ZK nullifier, rate-limiting tokens;
- the **storage/network layer** (`crates/network`): content addressing, Merkle tree, hash-chained log, consortium checkpoints, erasure coding, OpenTimestamps anchoring;
- the **protocol layer** (`crates/protocol`): deposit, lottery, reviewer assignment, commit–reveal, gate and appeal, two-stage pilot, honeypot, probation, re-validation, exposure, blueprint, governance.

Out of scope: user interfaces, deployment tooling, legal/regulatory analysis of eID use, and the political question of whether competence-weighted voting is desirable (`docs/06` L6). The eID adapters (CIE/SPID) are in scope only as interfaces; no adapter performs real document verification at this commit.

---

## 2. System model

### 2.1 What the system claims to guarantee (as stated by the repository)

The repository makes, in `README.md`, `docs/00`–`06`, and `ARCHITECTURE.md`, the following top-level claims. Each is decomposed into numbered claims in §5.

| # | Top-level claim (paraphrased from the docs) | Where stated |
|---|---|---|
| T1 | A question is accepted only if approved *across* the latent fracture axis (bridging), never by majority. | `docs/00`, `docs/01` D1–D2 |
| T2 | The final verdict on a question comes from psychometric statistics on real answers (IRT + DIF), not from opinions. | `docs/01` D3, `docs/02` §B |
| T3 | Bias is detected on *latent* axes without collecting any demographic attribute. | `docs/01` D4, `docs/02` §B.3 |
| T4 | One real person ↔ at most one active pseudonym per role; pseudonyms are non-rotatable and mutually unlinkable; the state authenticates but does not issue. | `docs/03` P1–P3, M1–M3 |
| T5 | Reputation cannot be whitewashed; a coordinated cartel of `k` nodes has influence ≈ `√k`. | `docs/02` §C, §Anti-collusion, `docs/01` D7 |
| T6 | Nobody can delete or rewrite questions and votes, or falsify scores, without it being visible. | `docs/04` |
| T7 | The scoring computation is deterministic and bit-for-bit reproducible, so a dishonest signer is unmasked by re-running it. | `docs/CLAUDE.md` #7, `ARCHITECTURE.md` |
| T8 | The engine reproduces the Python simulations ("executable specification"). | `README.md`, `ARCHITECTURE.md`, `sim/README.md` |

### 2.2 Reconstructed pipeline (what the code actually composes)

```
                    identity                                   network
   eID doc ──► canonical anchor ──► UniquenessOracle ──► Label ──► EnrollmentRegistry (dedup)
                                     (VOPRF | ThresholdOPRF)          │
                                                                     (no link in code)
   holder secret x ──► Credential ──► request_issuance(label) ──► Issuer | ThresholdIssuer ──► BBS+ sig
        │                                                                       │
        ├──► nym::derive_nym(secret, role)  = SHA-256   ◄── USED BY protocol crate
        └──► nullifier::prove(cred, role)    = x·H_role + ZK proof  ◄── NOT used by protocol crate

   protocol (per epoch, in tests only — there is no runtime orchestrator)
   Draft ──deposit──► TransparencyLog(Cid) ──admit(lottery)──► assign_reviewers(f_u-stratified)
     ──commit/reveal──► Ratings ──scoring::bridging::bridge_scores──► bridging_gate(τ, ε)
     ──► Pass | SupplementaryReview | AppealEligible | Reject
     ──► pilot::stage1_screen(r_pbis, 2PL a) ──► pilot::stage2_dif(logistic β₂ on `group`)
     ──► pool ──► revalidation::{revalidate_pool (axes), revalidate_pool_latent (mixture)} ──► exposure::should_retire
```

Observations that matter for every later section:

- **The orchestrator exists (T12) and the fixture epoch is routed through it.** `protocol::lifecycle` owns per-item `State` and a `step` transition function that rejects every checkable invalid §9.1 transition (`tests/orchestrator.rs`). The dead `protocol::Stage` enum was removed. `end_to_end.rs::run_epoch` now makes every stage-to-stage decision through `lifecycle::step` via `orchestrator::run_item` (RESOLVED@T12). The `SupplementaryReview` forward transition is now defined (`Event::Resolve`, the D26 re-decision — RESOLVED@T10/T30). Still open: persistence (T13).
- **The protocol crate consumes only `identity::nym::Nym` and `network::{cid, log}`.** It never calls `nullifier::{prove,verify}`, `ratelimit::*`, `credential::*`, `consortium::*`, `merkle::*`, `erasure::*`, or `anchoring::*` (verified by `grep` over `crates/protocol/src`). *(RESOLVED@T6: `protocol::admission` now calls `nullifier::verify` and the `deposit_with_identity`/`submit_review` entry points key on `NullifierProof::id`; see PROTO-007.)*
- **`scoring::bridging::fit` takes no reviewer weights.** Everything Level C and anti-collusion computes (`E_u`, `w_max`, probation weight, `discount_weights`) has no consumer in the Level A objective. *(RESOLVED@T5: the fit minimizes `Σ w_u (r−r̂)²`; `orchestrator::bridging_weights` supplies `w_u`; see BRIDGE-007.)*

### 2.3 Design intent vs. implemented vs. tested vs. simulated

| Component | DESIGN INTENT (`docs/`) | IMPLEMENTED BEHAVIOUR | TESTED BEHAVIOUR | SIMULATED BEHAVIOUR (`sim/`) | UNIMPLEMENTED / SCAFFOLD | ASSUMED SECURITY PROPERTY | PROVEN OR EMPIRICALLY SUPPORTED |
|---|---|---|---|---|---|---|---|
| Bridging (A) | MF with asymmetric reg., `d ∈ {1,2}`, `n_min = 30`, bootstrap-min, band `ε`, L-BFGS-B | `d = 1` only; unweighted; in-house L-BFGS (Armijo, no bounds); bootstrap-min warm-started from full fit and including the full fit in the min | Oracle within 0.03 on `b_j`, axis corr > 0.98, bootstrap ≤ full, monotone capture cost | Same dataset, SciPy L-BFGS-B | `d = 2`; `n_min`; reviewer weights; uncertainty-band handling beyond a label | Non-convex objective reaches a "good" minimum from the seeded init | Behaviour on one synthetic dataset (N=200, M=10) |
| IRT (B.1–B.2) | 3PL, `a ≥ 0.6`, `|b| ≤ 2.5`, `c ≤ 0.35`, infit/outfit 0.7–1.3, `r_pbis ≥ 0.20` | `θ` = standardized anchor total; `r_pbis`; per-item 2PL by logistic regression on fixed `θ` | `r_pbis` within 0.02 of oracle; `a` ranks items | `r_pbis` only (no 2PL fit in sim) | 3PL, `c`, infit/outfit, `|b|` bound (constant defined, never used) | — | `r_pbis` and `β₂` numerics on one dataset |
| DIF Variant 1 | logistic on continuous `f_i` from Level A; MH on tertiles | logistic on caller-supplied `group`; MH on `group > 0` dichotomy with `n_strata` | `β₂` within 0.02; MH class A/C on two items | logistic on an *observed* ±1 group | Source of `f_i` for respondents (see DIF-002) | — | Numerics only |
| DIF Variant 2 | latent-class mixture, `G` by BIC, `max|b_g − b_h| > 0.5` | 2-class, fixed-`θ`, numerical-gradient L-BFGS; `|δ| > 0.5` | 3/8 biased detected (BIC>0, axis corr>0.4); 1/8 invisible | 1,2,3,5,8 of 8 × 3 seeds at NT=3000 | `G > 2`; scalable gradient; FP rate at 0 biased items | — | Detection regime at NT=3000, K=8 (auditor re-ran: matches) |
| Purification (B.4) | iterate on anchors until flagged set stable | anchors + currently-clean batch items; fixed point on flagged set; `max_rounds` cap, non-convergence not signalled | ESM flagged, others not, fixed point re-verified | anchors only (single pass) | Convergence signalling | — | One dataset |
| Reputation (C) | Beta-shrinkage `C_a`; log score; BSS vs crowd `p̄_j`; `E_u = σ(γ·BSS)`; asymmetric EMA; `w_max = 3·median` | `C_a` exact; BSS vs **base rate `mean(o)`**; `E_u`; EMA; cap | Matches sim BSS to 1e-6; docs examples | BSS vs base rate | Log score; consumption of `E_u` by bridging | — | Formula-level |
| Anti-collusion | ρ-matrix, spectral clustering or `f_u` distance, `(Σw)^α` | dense Pearson; connected components at `|ρ| ≥ thr`; `(Σw)^α` split pro-rata | identical-vector cartels of 400/500 at thr 0.99, unit weights | none | spectral clustering; sparse handling; consumption by bridging | — | Identical-vector case only |
| Identity M1 | threshold OPRF on anchor; no issuer learns anchor or label | `VoprfOracle` (RFC 9497) and `ThresholdOprfOracle` (2HashDH + Shamir + DLEQ), both run client+server **in one process with the cleartext anchor as argument**; `EnrollmentRegistry` stores labels | dedup across CIE/SPID; blind-independence; DLEQ soundness; t−1 refusal | none | DKG, transport, input binding to an authenticated anchor, label-authenticity check, key rotation | 2HashDH OPRF security; DDH on Ristretto255 | Functional tests |
| Identity M2 | blind BBS+ issuance by threshold committee | `Issuer` and `ThresholdIssuer` (`bbs_plus::threshold` DKLS MPC, trusted-dealer keygen, in-process) | round trip; wrong-issuer rejection; PoK soundness on tampered request | none | DKG, transport, presentation, revocation, link to registry (issuer never checks label freshness) | BBS+ unforgeability (q-SDH), blindness | Functional tests |
| Identity M3 | `H(secret, role)` + ZK proof of valid credential | `derive_nym` (SHA-256, no proof) **and** `nullifier` (`x·H_role` + BBS+-bound sigma proof); the protocol uses the former | determinism, role distinctness, proof verify/reject, wrong issuer | none | unification; protocol-side verification of any proof; revocation | SXDH (DDH in BLS12-381 G1) for cross-role unlinkability | Functional tests |
| Rate limit | RLN token, reuse reveals key, ZK quota proof | `H(secret, role, epoch, slot)`; `SlotLedger` collision detection; `within_quota` arithmetic on a claimed slot | collision detection | none | any verifiability of the token or the slot bound | — | none |
| Log / checkpoints | signed append-only logs; consortium `t`-of-`n` | unsigned hash chain; `Checkpoint{height, head}` ed25519 `t`-of-`n` | tamper of one payload detected; threshold counting; duplicate signer ignored | none | signatures on entries; consistency proofs; equivocation/replay handling; network id | ed25519 EUF-CMA | Functional tests, proptest |
| Merkle | root summarises records | duplicate-last-node tree | inclusion proofs verify; leaf change changes root (proptest) | none | leaf-count commitment | SHA-256 collision resistance | Auditor found duplication collision (NET-003, §10.2) |
| Erasure | (10,30) RS | `reed-solomon-erasure` systematic RS | any-k recovery (proptest), <k fails | none | placement, repair, shard authentication | — | Functional |
| Anchoring | hourly OTS to Bitcoin | real `.ots` build/parse/verify; injected block map; fake `upgrade` | lifecycle, mismatch, garbage | none | calendar POST, Bitcoin block source, scheduling, linkage to checkpoints | SHA-256; Bitcoin PoW | Format-level |
| Transport / CRDT | gossip + DHT + CRDT | none | none | none | all | — | — |
| Protocol stages | `docs/05` [1]–[9] | pure functions per stage + a `lifecycle` state machine (T12) rejecting invalid §9.1 transitions | per-function tests; `orchestrator.rs`; e2e composition in a test | none | route the fixture epoch through the state machine; seed provenance; supplementary review; appeal escrow | — | e2e on the fixture dataset |

---

## 3. Actors and trust boundaries

### 3.1 Actors

| Actor | Holds | Trusted for | Must NOT be trusted for |
|---|---|---|---|
| **Person** (holder) | `Credential.secret` (32 bytes), `AnonymousCredential` (BBS+ signature over `(x, label)`) | Keeping its secret; nothing else | Honesty of ratings, answers, or drafts |
| **Identity source** (CIE/SPID IdP; the state) | Knowledge of `codice fiscale` ↔ person; knowledge that a person enrolled (F1) | Asserting "this is a real, unique person" | Not linking person ↔ label ↔ pseudonyms (design goal, see PRIV-002) |
| **Issuing committee** (`n` members, threshold `t`) | Shamir shares of the OPRF key (`oprf::KeyShare`) and of the BBS+ key (`ThresholdIssuer.key_shares`) | Correct evaluation/signing when `≥ t` honest | Any coalition `≥ t` can brute-force the anchor space (docs/03 M1) and issue arbitrary credentials |
| **Label registry** (unspecified custodian) | `EnrollmentRegistry.used: HashSet<Label>` | Dedup | Unspecified — see ID-005 |
| **Storage consortium** (`n` signers, threshold `t`) | ed25519 `SigningKey` per `consortium::Member` | Signing the true log head | Any coalition `≥ t` can sign a false head or equivocate (NET-006) |
| **Anchoring service** (OTS calendar + Bitcoin) | — | Time-ordering of roots | Availability |
| **Scoring re-runner** (anyone) | Full ratings + answers | Recomputing scores | Receives all voting patterns (PRIV-004) |
| **Sortition committee** (honeypot / blueprint / parameters) | Knowledge of which items are golden | Producing golden items | Reviewing their own golden items (PROTO-009) |
| **Founder set** | Declared `Nym`s with weight 1 | Bootstrap outcomes | Long-term weight without a track record (probation applies at 200) |

### 3.2 Trust boundaries (as they exist in the code)

| Boundary | Design requirement | Status in code |
|---|---|---|
| Identity source ↔ issuing committee | Never communicate; the state does not see the label | No representation. `EnrollmentRegistry::enroll(doc, oracle)` receives the *cleartext* anchor and the oracle in the same call. |
| Holder ↔ committee (OPRF) | Committee sees only the blinded element | `UniquenessOracle::label(&self, anchor: &Anchor)` — the key-holder receives the cleartext anchor. Obliviousness is exercised inside the method, not enforced by the interface. |
| Holder ↔ committee (issuance) | Committee sees only the commitment and the label | Enforced: `Issuer::issue(&IssuanceRequest)` receives `commitment`, `label`, PoK; the secret is never passed. |
| Issuing committee ↔ storage consortium | Preferably distinct (D15) | No representation. |
| Pseudonym ↔ credential | Every action carries a ZK proof | `protocol` accepts a bare `Nym` (32 bytes) with no proof. |
| Signer ↔ verifier | Anyone re-runs the computation | No serialization of the engine input exists; "same input" is not defined (REPRO-002). |

### 3.3 Threat actors assumed by the documentation

`docs/03` D9 requires anonymity to hold **against a state-level actor**. `docs/06` assumes: majority factions, cartels of coordinated real persons, a dishonest signer, a betraying consortium, and a colluding issuing committee below threshold. The documentation does not state assumptions about: the label-registry custodian, the source of protocol randomness, the re-runner of the computation, or a compromised holder device. This specification adds them (§12).

---

## 4. Invariants

The eight invariants of `docs/CLAUDE.md` are restated here as runtime invariants with their actual enforcement point. "Enforced" means a code path makes the violation impossible or detectable; "asserted" means only a comment or a test example states it.

| # | Invariant (normative form) | Enforcement at `c4a09b1` | Status |
|---|---|---|---|
| INV-1 | No personal, demographic, or affiliation attribute MUST enter any data structure processed by the engine. | No such field exists. **However** `dif::logistic_dif`, `pilot::stage2_dif`, `revalidation::revalidate_pool` take an arbitrary caller-supplied `group`/`axes` vector per respondent; nothing prevents a caller from passing a declared attribute. The fixtures do exactly that (`levelb_grp.csv` is an observed ±1 label). | Asserted, not enforced |
| INV-2 | Item acceptance MUST NOT be a majority-vote count. | `gate::bridging_gate` uses `b_j`; no vote count exists. `governance::change_approved` is a 2/3 vote but applies to meta-level changes only. | Enforced for items |
| INV-3 | No monetary stake MUST exist. | No money type exists. `settle_appeal` operates on a reputation float. | Enforced |
| INV-4 | `C_a` and `E_u` MUST live on different pseudonyms and MUST NOT be combined. | Separate functions; `Role::Propose` vs `Role::Judge`. No code combines them. No code *binds* a score to a role either (scores are bare `f64`s in tests). | Enforced by absence |
| INV-5 | One deterministic, non-rotatable pseudonym per role. | `nym::derive_nym` and `nullifier::prove` are both deterministic in `(secret, role)`. Rotation is prevented only if a second credential is impossible (ID-001…ID-005). | Enforced for derivation; depends on identity layer |
| INV-6 | The authenticator (state) MUST be distinct from the issuer (committee). | Distinct types (`IdentityDocument` vs `Issuer`). No protocol message separates them; see §3.2. | Asserted |
| INV-7 | Same input ⇒ bit-identical output. | `tests/reproducibility.rs` (same process, same binary). "Input" has no canonical serialization; see REPRO-001/002. | Tested within a process |
| INV-8 | Level-B validation MUST run on batches, never a single item. | ENFORCED (T9) at the batch-admission gates: `pilot::{admit_dif_batch, dif_batch}` and `revalidation::revalidate_batch_latent` reject a batch below `K_MIN` items and a sample below its §B.6 floor; `run_epoch` runs the pilot through them. The per-item math (`stage2_dif`, `mixture_dif`) stays available for calibration probes. | Enforced |

Additional invariants this specification introduces (not in `docs/CLAUDE.md`), each derived from a gap found in §10:

| # | Invariant | Reason |
|---|---|---|
| INV-9 | Every `Nym` accepted by the protocol MUST be accompanied by a verified `NullifierProof` for the same role, and the protocol MUST key reputation and rate limits on `NullifierProof::nullifier()`, not on `nym::derive_nym`. | ENFORCED at the entry points (T6): `admission::admit` + `deposit_with_identity`/`submit_review` key on `NullifierProof::id()`; PROTO-007 |
| INV-10 | The seed of every lottery, reviewer assignment, honeypot placement, and sortition MUST be derived from public randomness that is fixed *after* the set of candidates is fixed and that no participant can influence. | ENFORCED (T8): `randomness::Beacon` derives every draw's seed from the signed checkpoint head; the `_from_beacon` wrappers are the entry points; CRYPTO-008 |
| INV-11 | The uniqueness-label key (OPRF key) MUST NOT be rotated without a documented migration that preserves dedup; the label MUST be stable for the lifetime of the registry. | ID-006 |
| INV-12 | A commitment in commit–reveal MUST bind the committer's nullifier and the item CID. | ENFORCED (T7): `review::commit = H(prob, nonce, committer, item)`; the reveal recomputes against the revealer + item; CRYPTO-007 |
| INV-13 | The engine input MUST have a canonical serialization (fixed observation order, fixed float encoding) and the reproducibility claim MUST be stated relative to it. | REPRO-002 |
| INV-14 | The `(Σw)^α` group transform MUST NOT increase any node's weight. | COLLUSION-004 |

---

## 5. Formal claims

Each critical claim carries the full block required by `docs/07` §4. Secondary claims appear in compact form and in the matrix (§15). Statuses are the lowest justified by the evidence cited.

### 5.1 Reproducibility

#### REPRO-001 — Bit-for-bit determinism of the engine
- **Claim.** For fixed `Ratings` (same `n`, `m`, and `obs` *in the same order*), fixed `BridgingParams`, fixed toolchain, fixed target triple and libm, `bridging::fit`, `bridging::bridge_scores`, and `dif::mixture_dif` return bit-identical `f64`s across runs.
- **Preconditions.** Same binary, same platform. Same `obs` order.
- **Assumptions.** `f64::{ln, cos, exp, ln_1p, sqrt, powf}` are deterministic on the platform; no SIMD auto-vectorization reorders reductions between builds (`codegen-units = 1`, no fast-math).
- **Invariant.** No `HashMap`/`HashSet` iteration, no timestamps, no unseeded RNG in `crates/scoring` (verified by reading: RNG is `ChaCha8Rng::seed_from_u64` with explicit consumption order; all reductions are sequential loops).
- **Failure condition.** `to_bits()` inequality on any output for identical input on the same platform.
- **Evidence.** `crates/scoring/tests/reproducibility.rs` (3 tests, same process). `Cargo.toml` `[profile.release]` `codegen-units = 1`, `lto = "thin"`; `rust-toolchain.toml` pins 1.86.0.
- **Evidence status.** TESTED (within one process on one platform). **Cross-platform / cross-libm reproducibility: NOT ESTABLISHED.** `ln`, `cos`, `exp`, `ln_1p` lower to the platform libm (glibc vs musl vs macOS differ in last-ulp behaviour); no test runs on two platforms. Note the release profile is not what `cargo test` uses; the reproducibility tests run under the `test` profile with default `codegen-units`.

#### REPRO-002 — Input canonicalization
- **Claim.** The output of `fit` depends only on the *set* of observations, not their order.
- **Failure condition.** Permuting `Ratings.obs` changes any output bit.
- **Evidence.** None. The gradient and cost accumulate in `obs` order; floating-point addition is not associative, so permutation invariance at the bit level is expected to **fail**. `crates/scoring/tests/level_a.rs:152-165` already builds `obs` from a `HashSet` iteration (`for &u in &boosters`), so that test's fit is order-randomized per process (its assertions are inequalities, so it passes regardless).
- **Evidence status.** NOT ESTABLISHED. The reproducibility contract MUST be stated relative to a canonical ordering (e.g. sort `obs` by `(u, j)`) — INV-13 — and a permutation test MUST assert either bit-equality after canonicalization or bounded divergence (`|Δb_j| < 1e-9`) without.

#### REPRO-003 — Oracle equivalence with the Python simulations
- **Claim.** The Rust engine reproduces the simulations' results on the same input.
- **Preconditions.** Fixtures in `crates/scoring/tests/fixtures/` generated by `sim/export_fixtures.py`.
- **Evidence.** `level_a.rs`: `|b_j − b_j^{sim}| < 0.03`, `|μ − 0.7552| < 0.02`, axis corr > 0.98. `level_b.rs`: `r_pbis`, `β₂` within 0.02. `level_c.rs`: BSS within 1e-6. `mixture_*`: qualitative (means, sign of BIC).
- **Audit finding (auditor-executed).** Regenerating the fixtures with numpy 2.4.4 / scipy 1.17.1 changes `bj_full` by up to **1.0e-3** and `mu_hat` by 1.9e-4 relative to the committed `expected_levelA.csv`/`expected_meta.csv` (19 + 1 tokens outside the `fixture_drift` tolerance `1e-4 + 1e-4·|x|`). The data files (`R.csv`, `mask.csv`, all Level-B/C/mixture inputs) regenerate **exactly**. The drift is in SciPy's L-BFGS-B (rewritten in C in SciPy 1.15) stopping point, i.e. the "oracle" is defined only to ≈1e-3 in `b_j`. The ignored test `crates/scoring/tests/fixture_drift.rs::committed_fixtures_match_the_sims` would fail in this environment.
- **Consequence.** With `τ = 0.08` and `ε = 0.008`, three items of the oracle dataset sit within 0.004 of `τ` (`b_j` = 0.0812, 0.0801, 0.0837 for items 02, 06, 07). A 1e-3 oracle uncertainty and a 3e-2 acceptance tolerance mean **verdict-level agreement near the threshold is not tested and not testable at the current tolerances.** `end_to_end.rs` confirms a verdict disagreement: the sim places item 02 in the pool, the Rust pipeline does not (`EXPECTED_POOL = [0, 6]`, comment at lines 30–37), because the Rust screen adds `a ≥ 0.6` on a 2PL fit the sim never performs.
- **Evidence status.** TESTED at the statistic level within loose tolerances; **REPRODUCED: NO** (the auditor could not reproduce the committed oracle values from the sims to the repository's own tolerance); verdict-level equivalence NOT ESTABLISHED.

#### REPRO-004 — Fixture provenance
- **Claim.** Committed fixtures are immutable known-answer data with documented provenance.
- **Evidence.** `sim/export_fixtures.py` (seeds 7, 0, 100+s, 200). No numpy/scipy versions are recorded anywhere; `fixture_drift` is `#[ignore]` and not run in CI.
- **Evidence status.** IMPLEMENTED. The fixtures SHOULD record the exact numpy/scipy versions and SHOULD be regenerated in CI under a pinned environment, or the oracle tolerance SHOULD be widened to the observed drift with an explicit justification.

### 5.2 Bridging (Level A)

#### BRIDGE-001 — Model and objective match the specification
- **Claim.** `bridging::fit_with_init` minimizes `L = Σ_Ω (r_uj − μ − b_u − b_j − f_u f_j)² + λ_b(Σb_u² + Σb_j²) + λ_f(Σf_u² + Σf_j²)` with `d = 1`.
- **Evidence.** Code inspection of `crates/scoring/src/bridging.rs:152-190`: cost and analytic gradient match `sim/bridging_irt_dif.py::fit` term by term (gradient factor 2 included in both). `μ` is unregularized in both.
- **Evidence status.** IMPLEMENTED, TESTED (oracle). `d = 2` (docs/02 §A.4) NOT IMPLEMENTED. `n_min = 30` (nodes with fewer reviews excluded from defining `f`) NOT IMPLEMENTED anywhere in the workspace.

#### BRIDGE-002 — Latent axis recovery
- **Claim.** On the fixture dataset (N=200, 60/40 split, `true_f ~ N(±1, 0.25)`, M=10, 9 ratings/node), `|corr(f_u, true_f)| > 0.98`.
- **Assumptions.** Data generated by the sim's own linear model `R = q + 0.45·f·lean + sev + N(0, 0.07)`, clipped to [0,1]. The recovered axis is the axis the generator planted.
- **Failure condition.** corr ≤ 0.98 on this dataset; or corr materially lower on any dataset with the same generating process and a different seed (untested).
- **Evidence.** `level_a.rs::fit_reproduces_oracle_on_identical_dataset`; auditor re-run of the sim gives 0.990.
- **Evidence status.** TESTED on one seed. **SCIENTIFICALLY_CHARACTERIZED: NO** — no sweep over seeds, split ratios, noise, sparsity, `k`, or model misspecification (non-linear rating behaviour, multi-axis populations, `d=1` fit to a `d=2` world).

#### BRIDGE-003 — Asymmetric regularization discards polarized items
- **Claim.** Items with large `|f_j|` obtain `b_j < τ`; cross-cutting items obtain `b_j ≥ τ`.
- **Evidence.** `level_a.rs::asymmetric_regularization_separates_bridging_from_majority` (items 2,7,8,9 below τ with `|f_j| > 0.4`; items 0,3,4 above τ with `|f_j| < 0.4`).
- **Caveat.** This is a property of the generating process (lean ∈ {0, ±0.75, ±0.8, 0.3, 0.05}) plus the parameter pair (0.15, 0.03) plus `τ = 0.08`, all chosen after looking at the same dataset (`docs/02` §A.3 says τ "proved" to be 0.08 "in testing"). Per `docs/07` §13 this is not evidence of correctness for any other dataset.
- **Evidence status.** TESTED on one dataset. Parameter calibration procedure NOT ESTABLISHED.

#### BRIDGE-004 — Bootstrap-min is pessimistic and stable
- **Claim.** `bridge_scores(·, m=10, keep=0.85)[j] ≤ fit(·).b_j[j]` for all `j`, and the min reflects sampling variability rather than optimizer multimodality.
- **Evidence.** First half: `level_a.rs::bootstrap_min_is_pessimistic` — but it is **true by construction**: `bridge_scores` initializes `best = full.b_j` and only lowers it (`bridging.rs:217, 236-240`), so the assertion cannot fail. This deviates from the sim, which takes the min over the 10 subsample fits only. Second half (warm-start suppresses spurious minima): asserted in `ARCHITECTURE.md`, not tested — no test compares warm-started vs. cold-started bootstrap spread.
- **Evidence status.** IMPLEMENTED; the pessimism test is tautological; multimodality claim HYPOTHESIS.

#### BRIDGE-005 — Cost of bipartisan corruption
- **Claim (docs/06).** With 40 own-camp boosters, a partisan item needs "70 of 80 (87%)" opposing-camp boosters to pass bridging, versus ~40 own-camp under majority vote.
- **Evidence.** `level_a.rs::corner_case_bipartisan_corruption_cost` asserts only monotonicity and `s70 > s0 + 0.2`, not the 87% figure. Auditor re-run of the sim: crossing occurs between 55 and 70 boosters with the sim's random selection; with deterministic first-`n` selection (as the Rust test does) the item passes at **55 of 80 (69%)** (`b_j = +0.081`). Majority vote with 40 own-camp boosters: plain mean 0.624 ≥ 0.60 → passes (auditor-computed; the sim asserts this in text but never computes it).
- **Evidence status.** Qualitative claim (need a large share of the *opposing* camp) TESTED on one dataset. The "87%" figure is sample-dependent and overstated; the correct statement is "≈ 55–70 of 80 depending on which nodes are corrupted". SCIENTIFICALLY_CHARACTERIZED: NO.

#### BRIDGE-006 — Uncertainty band
- **Claim.** Items with `b_j ∈ [τ−ε, τ+ε]` go to supplementary review.
- **Evidence.** `gate::bridging_gate` returns `SupplementaryReview`; the D26 re-decision `gate::supplementary_review` re-runs bridging over the (expanded) panel and decides `b_j` against the plain threshold τ; `lifecycle` resolves the state via `Event::Resolve`. `supplementary_redecision.rs`.
- **Evidence status.** RESOLVED@T10/T30 (was IMPLEMENTED as a label only). Semantics now defined: more reviewers → re-run bridging → decide `b_j` vs τ (a bridging decision, not a vote); polarized items 07/08 are not passed.

#### BRIDGE-007 — Reviewer weights enter the aggregation
- **Claim (docs/02 §C.2, §Anti-collusion, docs/05 §Cold start).** `E_u` weights the review vote; the cartel discount reduces a cartel's influence on `b_j`; probation nodes have weight 0.
- **Evidence.** `bridging::fit` has no weight parameter. `discount_weights`, `capped_weight`, `review_weight` produce numbers no consumer uses. `ARCHITECTURE.md` §Future work acknowledges this.
- **Evidence status.** IMPLEMENTED (T5). `bridging::fit` minimizes the weighted objective `Σ w_u (r−r̂)²` over `Ratings.weights = discount(cap(E_u))` (probation = 0); `anti_collusion.rs::at_col_06_cartel_moves_the_bridge_score_less_than_independents` shows a discounted cartel moves `b_j` less than the same number of independents. Still to wire: per-epoch recomputation from the *previous* epoch's outcomes in a real orchestrator (T12).

#### OPT-001 — Optimizer convergence is observable
- **Claim.** A caller can tell whether `optim::lbfgs` converged.
- **Evidence.** `lbfgs` returns `Vec<f64>` only; hitting `max_iters`, the `step < 1e-20` exit, and the "not a descent direction" exit are indistinguishable from convergence. `fit_logistic` under complete separation (possible for any item with perfect θ-separation, or for the 4-parameter DIF model on small strata) will run to `max_iters` and return finite but arbitrary large coefficients, which then feed `|β₂| > 0.40` decisions.
- **Evidence status.** NOT IMPLEMENTED. `lbfgs` MUST return a status (`Converged{iters}`, `MaxIters`, `LineSearchFailed`) and callers MUST propagate it into verdicts.

### 5.3 IRT and DIF (Level B)

#### IRT-001 — Ability proxy
- **Claim.** `θ_i` is the standardized total score on anchor items (`irt::theta_from_anchors`).
- **Evidence.** Code; matches `th` in both sims.
- **Note.** This is a classical proxy, not an IRT ability estimate. `standardize` divides by the population SD and returns NaN if all totals are equal. All downstream thresholds (`A_MIN`, `BETA2_MAX`, `MIXTURE_DIF_MAX`) are therefore expressed in "logits per SD of anchor total", not in the IRT θ metric from which the literature values were taken.
- **Evidence status.** IMPLEMENTED, TESTED. The metric mismatch is a specification ambiguity (§14 G-07).

#### IRT-002 — Point-biserial catches inverted keys
- **Claim.** An item with an inverted key has `r_pbis < 0`.
- **Evidence.** `level_b.rs::verdicts_match_the_oracle` (item 06: −0.356). Follows from the definition when the key is fully inverted and the item discriminates.
- **Evidence status.** TESTED.

#### IRT-003 — 2PL discrimination screen
- **Claim.** `fit_2pl_item` returns `a` such that `a ≥ 0.6` retains discriminating items.
- **Evidence.** `level_b.rs::irt_2pl_discrimination_ranks_items` (ranking + one item below 0.6). `end_to_end.rs` documents that item 02 (generated as 3PL with guessing floor 0.25 in the sim) fails the screen; the test rationalizes this as 3PL-vs-2PL, but the θ-metric mismatch (IRT-001) is an equally plausible cause and is not separated out.
- **Evidence status.** IMPLEMENTED, TESTED (2 items). Threshold validity NOT ESTABLISHED. 3PL, `c ≤ 0.35`, `|b| ≤ 2.5` (constant `B_ABS_MAX` exists, unused), infit/outfit: NOT IMPLEMENTED.

#### DIF-001 — Logistic DIF numerics
- **Claim.** `dif::logistic_dif` returns the unpenalized MLE of `logit P = β₀ + β₁θ + β₂g + β₃θg`.
- **Evidence.** `level_b.rs::point_biserial_and_logistic_dif_reproduce_oracle` (`|β₂ − β₂^{sim}| < 0.02` on 10 items).
- **Evidence status.** TESTED. No standard errors, no test statistic, no multiple-testing control (§7.4).

#### DIF-002 — Variant 1 has an admissible input under the anonymity invariants
- **Claim (docs/02 §B.3).** The group variable `f_i` for respondent `i` is "the Level A latent axis".
- **Analysis.** Level A estimates `f_u` for **judge** pseudonyms. Respondents answer under the **respond** pseudonym, which is unlinkable to the judge pseudonym by INV-4/INV-5 (P3). Therefore no component can supply `f_i` for a respondent without either (a) linking roles (violates P3) or (b) an observed attribute (violates INV-1). The fixtures use an observed ±1 label (`levelb_grp.csv`); the sims call it `grp` and `edu`, i.e. observed groups.
- **Failure condition.** Any deployment that runs `pilot::stage2_dif` or `revalidate_pool` with a per-respondent group vector obtained without violating P3 or INV-1. None is described.
- **Evidence status.** **NOT ESTABLISHED.** At this commit, Variant 1 (`logistic_dif`, `mantel_haenszel`, `purify_theta`, `pilot::stage2_dif`, `revalidation::revalidate_pool`) is usable only in a pilot with declared attributes. Only Variant 2 is anonymity-compatible. This is the single most consequential gap in the specification (§14 G-01).

#### DIF-003 — Mantel–Haenszel classification
- **Claim.** `mantel_haenszel` computes `α_MH = Σ A_s D_s/N_s ÷ Σ B_s C_s/N_s` over `n_strata` equal-frequency θ strata and classifies by `Δ_MH = −2.35 ln α_MH` with ETS cut-offs 1.0 / 1.5.
- **Evidence.** Code inspection (correct formula); `level_b.rs::mantel_haenszel_classifies_dif` (2 items, 5 strata).
- **Deviations from spec.** Spec says "discretizing `f` into tertiles"; code dichotomizes `group > 0.0`. ETS classification also requires a significance test for B/C (MH χ²); none is computed. Zero cells give `α = ∞` or `0` and `|Δ| = ∞` → class C without warning. `sort_by(partial_cmp().unwrap())` panics on NaN θ.
- **Evidence status.** IMPLEMENTED, TESTED (2 items). Spec/implementation mismatch on stratification.

#### DIF-004 — Latent-class mixture detects batch-level bias
- **Claim.** With NT = 3000, K = 8, `a ~ U(1, 1.5)`, `b ~ N(0, 0.6)`, planted `δ = 0.9` on ≥ 2 items and a hidden balanced ±1 axis, the 2-class mixture yields `|δ̂| ≈ 1.0` on biased items, ≈ 0.1 on clean, BIC > 0, and `|corr(posterior, axis)| ∈ [0.5, 0.8]`.
- **Assumptions.** Exactly two latent classes, balanced; one common hidden axis for all biased items; θ known up to the anchor proxy; item parameters equal across classes except for the shift `δ_j`; local independence.
- **Failure condition.** On data satisfying the assumptions: `|δ̂|` on biased items < 0.5 or on clean items > 0.5 or BIC ≤ 0 with ≥ 2 biased items.
- **Evidence.** `level_b.rs::mixture_detects_bias_in_a_batch` (3/8, one seed); `end_to_end.rs::pool_revalidation_flags_latent_bias` (allows 1 false positive in 5); auditor re-ran `sim/latent_dif_and_capacity.py`: 1/8 → 0.33 vs 0.35 (invisible), 2/8 → 1.03 vs 0.12 (BIC 100), 3/8 → 1.03 vs 0.09, axis corr 0.50 → 0.80. The ignored `power.rs` uses **5 seeds** per condition (insufficient for a power estimate) and never runs the 0-biased condition (no false-positive rate).
- **Evidence status.** TESTED and REPRODUCED (by the auditor, in the sim) in the tested regime. SCIENTIFICALLY_CHARACTERIZED: NO (no FP rate, no unbalanced classes, no `G ≠ 2`, no `δ < 0.9`, no non-uniform DIF, no multi-axis, no misspecified θ).

#### DIF-005 — A single biased item is unidentifiable (INV-8 rationale)
- **Claim.** With 1/8 biased, the detector cannot separate it.
- **Evidence.** `level_b.rs::mixture_misses_a_single_biased_item` (asserts axis corr < 0.35); sim: `δ̂` = 0.33 on the biased item vs 0.35 on clean items.
- **Note.** The clean-item `δ̂` of 0.35 in this regime is only 0.15 below the Rust rejection threshold; the false-positive margin at small batches is thin and uncharacterized.
- **Evidence status.** TESTED (encodes a limitation as an assertion; a better detector would break this test, which should be inverted into a documentation claim rather than a guard).

#### DIF-006 — Mixture rejection threshold is consistent across docs, sim, and code
- **Analysis.** Model: `logit P = a_j(θ − b_j − δ_j z)`, `z ∈ {−1,+1}` ⇒ class difficulties `b_j ± δ_j` ⇒ `max_{g,h}|b_jg − b_jh| = 2|δ_j|`. `docs/02` rejects at `DIF_j > 0.5` (on the b-gap). `sim/latent_dif_and_capacity.py` declares "DETECTED" at mean `|δ̂| > 0.35` (batch-level, not per item). `scoring::dif::MIXTURE_DIF_MAX = 0.5` is applied to `|δ̂|` in `revalidation::revalidate_pool_latent` (per item) — i.e. 1.0 logit on the b-gap, **twice** the documented cut-off.
- **Evidence status.** INCONSISTENT. The specification MUST fix one metric (recommend: report `2|δ̂|` as `DIF_j` and reject at a documented value chosen by an FP/FN study).

#### DIF-007 — Purification reaches a fixed point
- **Claim.** `validation::purify_theta` returns a flagged set that is a fixed point of the flag→re-estimate map.
- **Evidence.** `level_b.rs::purification_reaches_a_stable_flagged_set` re-runs DIF with the returned θ and checks equality.
- **Deviations.** Spec §B.4 estimates θ on anchors only ("30 anchor items external to the batch"); code adds currently-clean batch items to the total each round, changing the θ metric between rounds and relative to the sim. Non-convergence (oscillation) is returned silently with `iterations == max_rounds`.
- **Evidence status.** TESTED (one dataset). Convergence guarantee NOT ESTABLISHED (the map is not monotone; oscillation is possible in principle).

#### DIF-008 — False-positive / false-negative characterization
- **Evidence status.** NOT ESTABLISHED for every detector (Variant 1 thresholds 0.40, MH 1.5, mixture 0.5). No simulation family in the repository measures FP or FN rates at any sample size. `docs/07` §14 lists this as a minimum expectation.

#### DIF-009 — Whole-pool mixture re-validation is computable
- **Claim.** `revalidate_pool_latent` can run over "the whole active pool".
- **Analysis.** `mixture_dif` uses a central-difference gradient: `2(1+3m)` NLL evaluations per gradient, each `O(NT·m)`, for up to 3000 iterations. For `m = 8, NT = 3000`: ~10⁹ flops per fit (fine). For a pool of `m = 300`: ~10¹⁴ per fit — infeasible. The two-class, one-axis model is also assumed to hold for *all* pool items simultaneously.
- **Evidence status.** NOT ESTABLISHED at pool scale. The specification MUST bound batch size for this detector or require an analytic gradient and a batched design.

#### STAT-001 — Sample sizes 300 / 1500 / 3000 are adequate
- **Claim (docs/02 §B.6).** Pilot 1 ≈ 300, Pilot 2 ≈ 1500 (with group signal) or ≈ 3000 (latent-class).
- **Evidence.** Literature rules of thumb cited in prose; `power.rs` (ignored, 5 seeds, 2 of 8 biased, `δ = 0.9`); the sim uses NT = 1500 for Variant 1 and 3000 for Variant 2. `docs/02` itself says these are "calibration targets … not a proof".
- **Evidence status.** HYPOTHESIS. Required: a power study over (NT, K, n_biased, δ, class balance, a, b) with ≥ 200 replicates per cell, reporting sensitivity and specificity with confidence intervals.

### 5.4 Reputation (Level C) and anti-collusion

#### REPUTATION-001 — Author score
- **Claim.** `C_a = (α₀ + Σ w_j q_j)/(α₀ + β₀ + Σ w_j)`, `w_j = exp(−Δt_j/T)`, `(α₀, β₀, T) = (2, 3, 18 mo)`.
- **Evidence.** `reputation::author_score`; `level_c.rs` reproduces the docs' 4/7 and 182/205 examples and monotone decay.
- **Evidence status.** TESTED. Note `q_j ∈ [0,1]` "a function of the Level B statistics" is never defined; every test uses `q ∈ {0, 1}`.

#### REPUTATION-002 — Evaluator BSS reproduces the oracle
- **Evidence.** `level_c.rs::evaluator_bss_reproduces_the_oracle` to 1e-6 (5 profiles).
- **Evidence status.** TESTED.

#### REPUTATION-003 — "Following the consensus scores ≈ 0"
- **Claim (docs/02 §C.2, docs/01 D6).** BSS is normalized against the crowd baseline `p̄_j`; someone who replicates the consensus gets `BSS ≈ 0`.
- **Analysis.** The sim and `reputation::base_rate_baseline` normalize against the **outcome base rate `mean(o)`** — a constant known only after outcomes, not the crowd's declared probabilities. Under this baseline the "follows the peer average" profile scores **−1.33**, not ≈ 0, and the "always predicts the base rate" profile scores exactly 0. The documented property refers to a baseline that is not implemented; the implemented property ("guessing the base rate scores 0") is different and depends on hindsight.
- **Evidence status.** RESOLVED (T31). Chose (a) the crowd-prediction baseline `p̄_j = Σ_u w_u p_uj / Σ w_u` (D23), matching D6's incentive argument: `reputation::crowd_baseline`, and the protocol's E_u (`honeypot::reviewer_skills`) normalizes BSS against it. `level_c.rs::at_rep_02_a_consensus_follower_scores_zero` (AT-REP-02) pins `BSS = 0` for a follower. The zero-denominator guard is separately RESOLVED (AT-REP-03, §0-ter). Residual: the sim's `levelc_bss` reference still uses the base rate — it reproduces the BSS *function*, not the E_u policy.

#### REPUTATION-004 — Temporal asymmetry
- **Evidence.** `asymmetric_ema` with caller-supplied `(up, down)`; tests use (0.1, 0.8) and (0.05, 0.5). No rates are specified in `docs/02` §C.4 ("rises slowly, falls quickly"). The "long-con is unprofitable" test (`scoring/tests/adversarial.rs::a_long_con_is_unprofitable`) checks arithmetic consequences of chosen rates, not a game-theoretic property.
- **Evidence status.** IMPLEMENTED; parameters NOT ESTABLISHED; the incentive claim is a HYPOTHESIS.

#### REPUTATION-005 — Weight cap `w_max = 3·median(w)`
- **Analysis.** With `E_u ∈ (0,1)` and `w_u = min(w_max, E_u)`: if `median(E) ≥ 1/3` then `w_max ≥ 1 > E_u` and the cap never binds. The tests bind it only with synthetic weights of 2.0 and 5.0, which `evaluator_score` cannot produce.
- **Evidence status.** IMPLEMENTED; the cap is vacuous in the operating range of the score it caps unless the median falls below 1/3. Specification gap G-12.

#### REPUTATION-006 — Probation
- **Evidence.** `probation::{status, review_weight}`; tests. Weight is not consumed (BRIDGE-007).
- **Evidence status.** IMPLEMENTED, TESTED, NOT WIRED.

#### REPUTATION-007 — Appeal stake is coherent with the score model
- **Analysis.** `gate::settle_appeal(reputation, stake, promoted, gain)` adds/subtracts constants from a number that `docs/02` §C.1 defines as a Beta posterior mean of item qualities. There is no escrow (the stake is deducted only on failure), so during the pilot the author's rate limit is unaffected; and a subtracted constant is not expressible as any set of `(q_j, Δt_j)`, so `author_score` and `settle_appeal` cannot both be the definition of `C_a`.
- **Evidence status.** INCONSISTENT. Recommend defining the appeal cost as a pseudo-observation `q = 0` with weight `s` (escrowed at appeal time, replaced by the real `q_j` on verdict), which keeps `C_a` a posterior mean.

#### COLLUSION-001 — Identical-vector cartel is discounted to √k
- **Claim.** 400 or 500 nodes with identical judgment vectors form one cluster at `|ρ| ≥ 0.99` and their summed discounted weight is `√k` (unit weights).
- **Evidence.** `anti_collusion.rs` (500 vs 22), `scoring/tests/adversarial.rs` (400 vs 120).
- **Evidence status.** TESTED for the identical-vector, dense, unit-weight case only.

#### COLLUSION-002 — Noisy coordination is detected
- **Failure condition.** A cartel that adds small independent noise to a shared pattern escapes clustering.
- **Auditor probe (Python, m = 24 as in `adversarial.rs`, pattern ~ U(0,1)).** Jitter σ = 0.02: 99.8 % of pairs ≥ 0.99. σ = 0.05: **0.2 %** of pairs ≥ 0.99 → no clustering → no discount. σ = 0.10: 0 %.
- **Evidence status.** **UNSOLVED.** A threshold of 0.99 on Pearson ρ is trivially evaded; the design needs a coordination statistic robust to jitter (e.g. rank agreement over shared items with a permutation null) and a documented FP rate against genuinely like-minded honest reviewers (`docs/01` D7's "accepted cost" is never quantified).

#### COLLUSION-003 — Sparse judgment matrices
- **Analysis.** `correlation_matrix` requires dense rows (`Vec<Vec<f64>>`, equal length, no missing marker). In the design each item has `k = 7–11` reviewers drawn at random; a reviewer with `n_min = 30` reviews shares with another reviewer, in expectation, `n₁n₂/M` items — 0.81 for M = 100, 0.08 for M = 1000. Pearson correlation is undefined or meaningless at that overlap. The docs' alternative ("distance in `f_u`") is not implemented.
- **Evidence status.** NOT IMPLEMENTED for the design's data regime. The current function is applicable only to the dense test fixtures.

#### COLLUSION-004 — The discount never increases a weight
- **Analysis.** `discount_weights` maps a cluster with total `s` to per-node `w_i · s^α / s`. For `s < 1` this is an **increase**: a singleton with `E_u = 0.25` becomes 0.50; 0.5 → 0.707 (auditor-computed). `ARCHITECTURE.md` and the tests only state the `w = 1` case.
- **Evidence status.** INV-14 VIOLATED. Fix: `w_i · min(1, s^{α−1})` or apply the transform to counts rather than to `E_u`-scaled weights; specify which.

#### COLLUSION-005 — Connected-component chaining and griefing
- **Analysis.** Union–find over `|ρ| ≥ thr` is transitive: one honest node correlated ≥ thr with one cartel member joins the cartel cluster and is discounted with it. `|ρ|` also merges *anti*-correlated nodes. Because judgment histories must be public for reproducibility (PRIV-004), an adversary with a few real identities can target an honest reviewer's history to pull it into a cluster. Random assignment slows but does not prevent this over time.
- **Evidence status.** UNSOLVED / not analysed in the repository.

### 5.5 Identity

#### ID-001 — Cross-source duplicate enrollment is rejected
- **Claim.** Two enrollments with the same canonical anchor produce the same label and the second is refused.
- **Preconditions.** Same oracle key; `normalize_cf` (trim + uppercase) maps both documents to the same `Anchor`.
- **Assumptions.** The anchor is correct and authenticated (nothing in code authenticates it: `Cie { codice_fiscale: String }` is a bare string).
- **Evidence.** `identity/tests/properties.rs`, `threshold_oprf.rs`, `protocol/tests/{end_to_end,adversarial}.rs`.
- **Evidence status.** TESTED for the registry logic. **Uniqueness of persons is NOT ESTABLISHED** — see ID-004, ID-005, docs/03 F2 (foreign passports live in a different anchor space).

#### ID-002 — Obliviousness of the label computation
- **Claim (docs/03 M1).** No issuer learns the anchor.
- **Analysis.** `VoprfOracle::label` runs RFC 9497 blind/evaluate/finalize correctly (library `voprf` 0.5.0, Ristretto255-SHA512, `new_from_seed` = DeriveKeyPair with info `isegoria/uniqueness/v1`), but the method signature hands the cleartext anchor to the object that owns the server key. `ThresholdOprfOracle::label_with_quorum` likewise. No client/server message types exist.
- **Evidence status.** The primitive is IMPLEMENTED and TESTED (`voprf_oracle.rs` asserts blind-independence, key separation, wrong-key rejection on the raw API). The **architectural** property is NOT IMPLEMENTED: the `UniquenessOracle` trait MUST be split into `blind(anchor) → (BlindedElement, ClientState)`, `evaluate(BlindedElement) → Evaluation` (server side, no anchor), `finalize(ClientState, Evaluation) → Label`.

#### ID-003 — Threshold: `t−1` members cannot compute the label
- **Claim.** In `oprf::ThresholdOprfOracle`, any `t` verified partials Lagrange-combine to `k·B`; fewer than `t` yield no information about `k`.
- **Assumptions.** Trusted dealer honest and forgets the polynomial; shares distributed to distinct parties; DDH/one-more-DH on Ristretto255; DLEQ challenge domain-separated (`isegoria/oprf/dleq-challenge/v1`, six points compressed).
- **Evidence.** Unit tests in `oprf.rs`: subset agreement (3 quorums), sub-threshold refusal, 2-of-5 interpolation ≠ correct label, lying member caught by DLEQ, key separation.
- **Findings.** (i) The whole committee — all `KeyShare`s — lives in one struct in one process; the security property is modelled, not provided. (ii) `lagrange_at_zero` with duplicate indices in `quorum` divides by zero; `curve25519-dalek` `Scalar::invert` of zero returns zero silently → a wrong label with no error; `label_with_quorum` MUST reject duplicate indices. (iii) `UniquenessOracle::label` always uses the first `t` members; no liveness/fault handling.
- **Evidence status.** IMPLEMENTED, TESTED (functional). Security: standard construction (2HashDH threshold OPRF with Chaum–Pedersen proofs), **INDEPENDENTLY_REVIEWED: NO**; deployment property NOT ESTABLISHED (no DKG, no transport).

#### ID-004 — The OPRF input is bound to the state-authenticated anchor
- **Claim needed by the design.** The blinded element the committee evaluates is a blinding of `H(cf)` for the *same* `cf` the identity source authenticated.
- **Analysis.** In the obliviousness-preserving flow, the holder blinds. Nothing prevents a holder from blinding an arbitrary string, obtaining a label for a fake anchor, and enrolling any number of times. Binding requires, e.g., the IdP to sign `H(cf)` (or a commitment) and the holder to prove in ZK that the blinded element opens to the signed value — or the IdP to perform the blinding, which then gives the IdP the unblinding key. None of this is specified or implemented; the current code sidesteps it only because the *registry* calls the oracle with the cleartext anchor (ID-002).
- **Evidence status.** **NOT ESTABLISHED. This is a design gap, not an implementation gap** (§14 G-02).

#### ID-005 — Label authenticity and registry custody
- **Claim needed by the design.** The label submitted for dedup is the genuine OPRF output, and whoever holds the registry cannot use it to deanonymize.
- **Analysis.** `docs/03` M1 says "no issuer learns the label" and, two lines later, "uniqueness is verified by checking the label is not already in the set (or the user proves its freshness in ZK)". `credential.rs` signs the label as a BBS+ message, so the issuer *does* learn it. Whoever holds both the registry and ≥ t OPRF shares can enumerate the ~10⁸ codice-fiscale space, compute all labels, and read the registry as a list of enrolled persons. A holder who computes its own label can submit a fabricated one unless the issuer re-derives or verifies it.
- **Evidence status.** CONTRADICTORY in docs; NOT IMPLEMENTED beyond an in-memory `HashSet`. The specification MUST state who holds the registry, what the issuer verifies about the label, and whether the label is ever revealed at presentation (it MUST NOT be, or issuer-side unlinkability fails — PRIV-002).

#### ID-006 — Key lifecycle
- **Analysis.** Labels are `F(k, anchor)`. Rotating `k` (compromise, member churn, proactive refresh — `oprf.rs` lists "proactive share refresh" as future work, which does *not* change `k`; a *re-keying* would) changes every label and silently re-opens double enrollment. Issuer-key rotation invalidates all credentials; re-issuance must preserve the holder's `x` or all nullifiers change (whitewashing by design). Neither lifecycle is specified.
- **Evidence status.** NOT ESTABLISHED (INV-11).

#### ID-007 — Non-rotatable pseudonyms / no whitewashing
- **Claim.** A person cannot obtain a second pseudonym for the same role.
- **Evidence.** `derive_nym` and `nullifier::prove` are deterministic in `(x, role)`; `protocol/tests/adversarial.rs::whitewashing_cannot_shed_a_bad_reputation`.
- **Analysis.** The test shows the *same secret* yields the same nym and the *same anchor* is refused a second enrollment. It does not, and cannot, show that a person cannot obtain a second credential with a *different* secret: the issuer never checks the registry, never checks that a label has not already been issued a credential (`Issuer::issue` signs any label with a valid PoK, any number of times), and the label→credential step is unlinked from `EnrollmentRegistry`. Two credentials for one label = two nyms per role.
- **Evidence status.** RESOLVED@T11 for the in-process form (was NOT ESTABLISHED). `credential::IssuanceRegistry` + `Issuer::issue_once` enforce **one credential per label**: a second request for a label already issued is refused with `IssuanceError::AlreadyIssued`, whatever secret it carries — so a person (one label from enrollment) gets one credential, one set of role nyms. Tests: `id007_one_credential.rs` (AT-ID-02, AT-ID-03). Residual: the cryptographic-grade enrollment that binds the label to the request without a trusted registry (ID-004/ID-005) is T20.

#### ID-008 — Rate limiting
- **Claim (docs/03).** One token per slot; reuse reveals the key; the holder proves in ZK that `slot < quota`.
- **Analysis.** `rln_token = H(secret, role, epoch, slot)` is a hash. A verifier cannot check which slot it encodes, that `slot < quota`, or that it derives from a valid secret; `within_quota` checks a *claimed* integer. Reuse yields a duplicate hash (detected) but reveals nothing. The documented Shamir-based RLN (two evaluations of a degree-1 polynomial reveal the secret) is not implemented.
- **Evidence status.** RESOLVED@T11 for the structural in-process form (was NOT ENFORCED). `admission::QuotaLedger` counts proposals per **verified Propose nullifier id** (INV-9) for the epoch and `deposit_with_identity` refuses one over `quota` with `DepositRejected::OverQuota`; the quota is set from the author score `C_a` via `reputation::proposal_rate` (reputation, not money — invariant #3). Tests: `id008_proposal_quota.rs`. Residual: the cryptographic-grade RLN (a ZK proof that `slot < quota` from a valid secret, with reuse revealing the key) is T20 — the in-process ledger trusts the verifier to key on the proven id, which T6 provides.

### 5.6 Cryptography

#### CRYPTO-001 — Single-server VOPRF
- **Primitive.** RFC 9497 VOPRF mode, `Ristretto255-SHA512`; library `voprf` 0.5.0 (`Cargo.lock`); key derivation `VoprfServer::new_from_seed(seed, info)`; blind randomness `rand_core::OsRng`.
- **Evidence.** `voprf_oracle.rs`.
- **Evidence status.** IMPLEMENTED, TESTED. Library security assumed (not audited here). Not wire-compatible with `oprf::ThresholdOprfOracle`; two label spaces coexist.

#### CRYPTO-002 — Threshold OPRF — see ID-003.

#### CRYPTO-003 — BBS+ blind issuance
- **Primitive.** BBS+ over BLS12-381 (`bbs_plus` 0.25.0, `SignatureG1`, params `SignatureParamsG1::new::<Sha256>(b"isegoria/bbs+/v1", 2)`), Pedersen commitment `C = h₀·r + h₁·x`, Schnorr PoK (`schnorr_pok` 0.23.0) with Fiat–Shamir over `(bases, C, t, label)` via `compute_random_oracle_challenge::<Fr, Sha256>`; `new_with_committed_messages` with the label as uncommitted message index 1.
- **Evidence.** `bbs_credential.rs`, unit tests in `credential.rs` (commitment hides bytes of the secret — a weak test; fresh blinding per request; tampered label or swapped commitment rejected).
- **Findings.** The issuer signs any valid request any number of times (no per-label issuance record). The PoK transcript omits a context/domain string beyond the label; a request is replayable to the same issuer (harmless only because the resulting credentials are for the same `x`). `Credential::from_secret` accepts any 32 bytes; `x = 0` gives `N = O` for every role (holder's own loss, but the verifier does not reject the identity point).
- **Evidence status.** IMPLEMENTED, TESTED (functional). Unforgeability/blindness rest on the library's proofs (q-SDH, DL). INDEPENDENTLY_REVIEWED: NO.

#### CRYPTO-004 — Threshold BBS+ issuance
- **Primitive.** `bbs_plus::threshold` (DKLS18/19 OT-based multiplication, `oblivious_transfer_protocols` 0.12.0, `secret_sharing_and_dkg` 0.16.0), `KAPPA = 256`, `STAT = 80`, base-OT key size 128; trusted-dealer Shamir via `deal_random_secret`; base OT bootstrapped in-process from a `StdRng` seeded with the same seed as the key (`credential.rs:340`).
- **Findings.** All shares, all base-OT material, and the dealer seed are in one struct. The base-OT randomness is derived from the *same* seed as the signing key; in a real deployment these MUST be independent. `threshold_sign` always uses members `1..=t`. Signing runs the full MPC per issuance (cost not characterized).
- **Evidence status.** IMPLEMENTED, TESTED (3 functional tests). Security property (no `t−1` coalition signs) is that of the library's protocol and is *modelled* here. INDEPENDENTLY_REVIEWED: NO.

#### CRYPTO-005 — Nullifier is bound to a valid credential
- **Construction.** `N = x·H_role`, `H_role = hash-to-G1(role tag)` with DST `isegoria/nullifier/hash-to-g1/v1` (WB map, `DefaultFieldHasher<Sha256>`); `PoKOfSignatureG1Protocol` with message 0 blinded by a chosen `ρ` (`MessageOrBlinding::BlindMessageWithConcreteBlinding`), message 1 (label) blinded randomly; commitment `t = ρ·H_role`; single challenge `c = H(BBS+ transcript ‖ H_role ‖ N ‖ t)`; verifier checks the BBS+ proof and `s·H_role = t + c·N` where `s` is the proof's response for message 0.
- **Analysis.** This is the standard AND-composition of two sigma protocols sharing a witness (as in BBS pseudonym constructions). Soundness: an accepting proof with the extracted `x` in the signature and `s = ρ + c x` forces `N = x·H_role`. Zero-knowledge: `ρ` fresh per proof (OsRng). Both messages are hidden (`revealed = BTreeMap::new()`), so the label is **not** revealed at presentation — this is correct and contradicts `ARCHITECTURE.md`'s "revealing only the label" description of the future presentation.
- **Evidence.** `nullifier.rs` unit test (swapped nullifier rejected); `identity/tests/nullifier.rs` (verify, determinism, role distinctness, person distinctness, wrong issuer).
- **Evidence status.** IMPLEMENTED, TESTED. **Bespoke composition; INDEPENDENTLY_REVIEWED: NO — external cryptographic review REQUIRED** before any security claim. Not used by the protocol crate (PROTO-007).

#### CRYPTO-006 — Cross-role unlinkability of nullifiers
- **Claim.** Given `(H_p, N_p, H_j, N_j)` with `N_p = x H_p`, `N_j = x H_j`, deciding whether the same `x` is used is hard.
- **Assumption.** DDH in BLS12-381 G1 (the SXDH assumption). In a type-3 pairing there is no efficient map G1→G2, so `e(N_p, H_j) = e(H_p, N_j)` cannot be evaluated with both arguments in G1. The module comment says "(DDH)"; the specification MUST name SXDH explicitly, because DDH is *false* in G1 of a type-1 pairing and a future curve change would break the property silently.
- **Evidence status.** HYPOTHESIS under a standard assumption; no test can establish it.

#### CRYPTO-007 — Commit–reveal binding
- **Analysis.** `review::commit(prob, nonce) = SHA-256("isegoria/commit/v1" ‖ prob_le ‖ nonce)` binds neither the item nor the committer. If commitments are visible before reveal, reviewer B can copy reviewer A's commitment and, after A reveals, reveal the same `(prob, nonce)` — the classic commitment-copying attack, which reintroduces exactly the herding the mechanism exists to prevent. `prob.to_le_bytes()` also makes `0.0` and `−0.0` distinct commitments and admits NaN.
- **Evidence.** `lifecycle.rs::commit_reveal_binds_the_judgment` (value hiding/binding); `protocol/tests/inv12_commit_binding.rs` (AT-BR-06: a copied commitment does not open under another committer, nor for another item).
- **Evidence status.** RESOLVED@T7 (was a specification defect). `review::commit(prob, nonce, committer, item) = SHA-256("isegoria/commit/v2" ‖ prob_le ‖ nonce ‖ committer ‖ item)`; `reveal` recomputes against the revealer's nym and the item, so a commitment opens only for its committer and item (INV-12). The `lifecycle` `Revealing` state carries the item and the reveal is checked against `(reveal nym, item)`; NaN/out-of-range probabilities are already rejected at reveal (`ProbabilityOutOfRange`). Residual (cosmetic, not security): `prob.to_le_bytes()` still distinguishes `0.0`/`−0.0`. **The committer id is the T6 nullifier id**, so the binding is to the verified nullifier.

#### CRYPTO-008 — Randomness for lottery, assignment, honeypot placement, sortition
- **Analysis.** `lottery::admit(base_seed, epoch)`, `review::assign_reviewers(item_seed)`, `honeypot::inject(seed)`, `governance::stratified_sortition(seed)`, `blueprint::assemble_test(seed)` are deterministic in a caller-supplied `u64`. The tests use constants. No document says where the seed comes from. If it is derivable from data an author controls (e.g. the draft CID, which the author can grind by editing whitespace), the author can select its reviewers — the brigading the random assignment is meant to prevent. If it is chosen by an operator, that operator can select reviewers for any item.
- **Evidence status.** RESOLVED@T8 (was NOT ESTABLISHED). `randomness::Beacon::from_checkpoint` takes a consortium-signed `Checkpoint` and derives every draw's seed as `seed(purpose, index) = H(head ‖ height ‖ purpose ‖ index)`, domain-separated per draw. The `_from_beacon` wrappers (`lottery`, `review`, `honeypot`, `governance`) are the sanctioned entry points; the raw `u64`-seeded draws remain for unit tests. Two properties give AT-BR-05: (1) the seed is a function of the signed head, which commits to every deposit and is fixed only once a threshold co-signs — an author cannot influence or predict it before deposits close; (2) reviewer assignment keys `index` on the item's **byte-independent admitted slot**, not the draft CID, so regenerating the draft cannot move the panel. Tests: `inv10_checkpoint_seed.rs`. Residual: the *publisher/timing* of the checkpoint at epoch close is part of the runtime layer (T13–T18); `blueprint::assemble_test` still takes a raw seed (committee-chosen coverage, not an adversarial draw).

### 5.7 Privacy

#### PRIV-001 — Role pseudonyms are mutually unlinkable
- For `nullifier`: see CRYPTO-006. For `derive_nym` (SHA-256): unlinkability holds under preimage resistance *only if the secret has ≥ 128 bits of entropy*; nothing enforces how `secret` is generated (`Credential::from_secret` accepts `[9u8; 32]`). **Evidence status.** HYPOTHESIS (cryptographic); tests only show inequality.

#### PRIV-002 — Issuer-side unlinkability
- **Claim.** The issuing committee cannot link a credential presentation/nullifier to an issuance transcript.
- **Analysis.** Issuance transcript contains `(C, label, PoK)`. Presentation (`nullifier::prove`) hides both messages, so linkability would require breaking BBS+ proof-of-knowledge ZK or DDH. Holds **only if the label is never revealed**; `ARCHITECTURE.md` describes a future presentation "revealing only the label", which would break this claim outright (the issuer saw the label at issuance).
- **Evidence status.** HYPOTHESIS; the design document contradicts the code on whether the label is revealed. The specification MUST state that the label is never disclosed after issuance.

#### PRIV-003 — Statistical deanonymization mitigations
- `docs/03` mandates text normalization, batched publication with random delay, no precise timestamps, domain quotas by lottery, structured citations. **None is implemented**; `Draft` holds free bytes; `log::Entry` has no timestamp (good) but no mixing either. **Evidence status.** HYPOTHESIS / NOT IMPLEMENTED.

#### PRIV-004 — Reproducibility vs. secrecy of voting patterns
- **Analysis.** `docs/04` and `docs/CLAUDE.md`: "do not put voting patterns … on a public register in the clear". `docs/04` and INV-7: "anyone can re-run the computation". Re-running `bridging::fit` requires the full `(u, j, r)` matrix, i.e. every judge-nym's every rating, and the output includes `f_u` — a political-position estimate per pseudonym. `governance::stratified_sortition` and `review::assign_reviewers` consume `f_u` per nym. These requirements are in direct tension. Options: (a) restrict re-running to consortium members and light-node *sampling* (weakens "anyone"); (b) succinct proofs of computation (`docs/01` D14 step 4, "mature phase"); (c) publish only aggregates plus a designated-verifier audit. The repository chooses none.
- **Evidence status.** UNRESOLVED DESIGN TENSION (§17 Q-1).

#### PRIV-005 — Small-network anonymity degradation
- `docs/02` §B.6 acknowledges a privacy floor (~2,000 active nodes). No quantitative k-anonymity model exists. **Evidence status.** HYPOTHESIS.

### 5.8 Network / storage

#### NET-001 — Content addressing
- `cid::cid(bytes) = SHA-256(tag "isegoria/cid/v1", len-prefixed bytes)`. TESTED (`integrity.rs::cid_binds_to_content`). Note `deposit::Draft::content_id` concatenates `item ‖ primary_source` **without** a length prefix before hashing, so `("ab", "c")` and `("a", "bc")` collide (`Template::variant` does prefix). PROTO-011.

#### NET-002 — Merkle inclusion proofs
- TESTED (`integrity.rs`, `properties.rs::merkle_inclusion_always_verifies`, proptest over arbitrary leaf sets). Leaf/node domain separation prevents leaf-as-node confusion.

#### NET-003 — The Merkle root commits to the leaf list
- **Auditor probe (Python replica of `merkle.rs`).** `root([x, y, z]) == root([x, y, z, z])` → **True**. The duplicate-last-node scheme (Bitcoin's CVE-2012-2459 shape) does not commit to the leaf count: a list and the same list with its last element duplicated share a root. Whether this is exploitable depends on what roots are used for (currently: nothing in the protocol crate uses `merkle_root`); it MUST be fixed before roots are anchored or signed (use RFC 6962 hashing — promote the odd node — or include the count in the root).
- **Evidence status.** DEFECT FOUND.

#### NET-004 — Log tamper-evidence
- **Claim (docs/04).** Altering any past entry breaks the chain visibly.
- **Analysis.** `TransparencyLog` is a hash chain with **no signatures** (despite "signed append-only logs"). `verify()` recomputes the chain from `[0;32]`; it detects an inconsistent edit (the test edits one payload without recomputing hashes) but **not a consistent suffix rewrite**: replacing entries `i..` and recomputing all subsequent hashes yields a log that `verify()` accepts. Tamper-evidence therefore exists only relative to an *externally held* prior head (a signed checkpoint or an anchor). No consistency proof (old head ⊑ new head) exists; a light client must re-download the suffix to check extension.
- **Evidence.** `log.rs` unit tests; `integrity.rs::append_only_log_is_tamper_evident`; `properties.rs::log_verifies_and_head_advances`; `log_consistency.rs` (AT-NET-01).
- **Evidence status.** RESOLVED@T14 (was TESTED for the inconsistent edit only). `log::checkpoint()` yields the `Checkpoint{height, head}` the consortium signs (`consortium::Member::sign`, a signature over the head that commits the whole prefix), and `log::verify_extends(&prior)` proves the current log consistently extends a checkpoint the verifier trusts — returning `ForkedHistory` for a consistent suffix rewrite of checkpointed history and `Truncated` for a shorter log, which `verify()` alone accepts. The claim now holds *relative to a signed checkpoint the verifier holds*, exactly as NET-004 required. `log_consistency.rs` co-signs the prior head with a `t`-of-`n` consortium.

#### NET-005 — Checkpoint threshold
- `Consortium::verify` counts distinct valid ed25519 (`ed25519-dalek` 2.2.0) signatures over `SHA-256(tag "isegoria/checkpoint/v1", height_le, head)` and requires `≥ threshold`. TESTED (3-of-5 passes, 2 fails, duplicates ignored, wrong-message signature ignored).

#### NET-006 — Checkpoint replay, equivocation, network binding
- **Analysis.** The signed message has no network/consortium identifier and no epoch/time: a checkpoint is valid forever and, after a fork (`docs/04` "freedom to fork" — the same keys may sign on both sides), on both forks. Two threshold-signed checkpoints with the same `height` and different `head` are both accepted; no equivocation detection, no client-side monotonic-height rule, no accountability record. Member set changes (add/remove/rotate keys) are not representable.
- **Evidence status.** RESOLVED@T15 (was NOT IMPLEMENTED). `Checkpoint` now carries `network_id` and `member_set_hash` **inside the signed message** (`…/checkpoint/v2`), and `consortium::CheckpointClient` is the §9.4 client state machine: it rejects a foreign `network_id` (AT-NET-05) or member set, ignores a non-monotonic `height` as a replay (`Stale`, AT-NET-03), and on two threshold-signed checkpoints at the same height with different heads returns `Forked{trusted, conflicting}` — the equivocation evidence (AT-NET-04). Tests: `checkpoint_replay.rs`. Residual: detecting a higher-height fork whose head does not extend the trusted one combines this with `log::verify_extends` (T14) for a client holding the log; member-set *rotation* in the checkpoint is future (CS-4/T22).

#### NET-007 — Erasure coding
- `reed-solomon-erasure` 6.0.0, GF(2⁸), systematic. TESTED (any-k recovery via proptest; below-k fails). No shard authentication (a corrupted shard is not detected before reconstruction; RS decoding with erasures only assumes shards are either missing or correct — a *wrong* shard yields wrong data silently). No placement, repair, or churn model. **Evidence status.** TESTED for the coding primitive; corrupted-shard case UNSOLVED.

#### NET-008 — Anchoring verification
- `opentimestamps` 0.2.0 parses `.ots`, recomputing each step's output by executing ops from `start_digest` (auditor checked `timestamp.rs::deserialize_step_recurse`), and `OtsAnchor::walk` compares a `Bitcoin{height}` attestation's digest to the injected block root. Sound given a trustworthy block source. TESTED (lifecycle, mismatch, garbage). **Note** `verify` is the only entry that parses untrusted bytes; the recursion limit in the library bounds it, but a fuzz test is absent.

#### NET-009 — Anchoring liveness and linkage
- No calendar submission, no Bitcoin block source, no scheduler ("hourly"), and nothing anchors a consortium checkpoint head (the only caller of `submit` is a test). **Evidence status.** NOT IMPLEMENTED beyond the proof format.

#### NET-010 — Transport, replication, convergence
- Gossip, DHT, CRDT: not implemented; no type in the workspace represents a peer, a message, or a replica. `docs/07` §17's convergence invariant cannot be stated for code that does not exist. **Evidence status.** HYPOTHESIS.

### 5.9 Protocol

#### PROTO-001 — Deposit requires a primary source — TESTED (`lifecycle.rs`). The source is a byte string; "primary source" is not validated (no structured citation type, contrary to `docs/03`).

#### PROTO-002 — Lottery — TESTED (deterministic per `(seed, epoch)`, bounded, no duplicates; proptest). Seed provenance: CRYPTO-008. Equal expected access holds only if the deposited set is not Sybil-inflated (ID-007) and rate limits hold (ID-008).

#### PROTO-003 — Stratified reviewer assignment — TESTED (9 of 200, one per stratum, deterministic). Requires `f_u` per candidate: new reviewers (below `n_min`, which is unimplemented) have no `f_u`; the docs say they "fill" but do not "define" the space — no code path.

#### PROTO-004 — Gate and appeal — TESTED (four outcomes; appeal recovers item 03 in e2e). `appeal_threshold = 0.5` on `|f_j|` appears only in a test constant; not in `docs/02`'s parameter table.

#### PROTO-005 — Pilot stage semantics — Stage 1 = `r_pbis ≥ 0.20 ∧ a ≥ 0.6`; Stage 2 = `|β₂| ≤ 0.40` on a supplied `group`. No `|b| ≤ 2.5`, no `c`, no MH, no mixture in the pilot; the mixture appears only in re-validation. Sample sizes (300/1500/3000) are not parameters of any function. TESTED on synthetic and fixture data.

#### PROTO-006 — Batch enforcement — RESOLVED@T9 (was NOT ENFORCED).
- The DIF stages run through batch-admission gates that refuse a batch below `K_MIN` items (INV-8) and a sample below its §B.6 floor: `pilot::admit_dif_batch`, the wrappers `pilot::{screen, dif_batch}` (Variant 1) and `revalidation::revalidate_batch_latent` (production Variant 2), with floors `N1_MIN`=300 / `N2_MIN`=1500 / `N_LATENT_MIN`=3000. `end_to_end.rs::run_epoch` runs the pilot through the gates, and `lifecycle::step` independently rejects `Pilot2Batch { batch_size < K_MIN }` (T12). AT-PRO-02 passes (`inv8_batch_min.rs`).

#### PROTO-007 — Pseudonym validity is verified by the protocol
- **Analysis.** `review::Reviewer.nym`, `probation::FounderSet`, and reputation maps are keyed on `nym::Nym` = SHA-256 of a secret, presented without proof. Anyone can mint unlimited `Nym`s. Sybil resistance, non-rotatability, and rate limiting are therefore properties of the *identity crate in isolation*, not of the protocol as wired. `lib.rs` of `identity` lists "unifying the protocol pseudonym with the ZK nullifier" as future work.
- **Evidence status.** RESOLVED@T6 (was NOT IMPLEMENTED). `protocol::admission::admit` verifies a role `NullifierProof` (via `nullifier::verify`) and returns `NullifierProof::id()` — a domain-separated hash of the verified nullifier `N = x·H_role`; the entry points `deposit_with_identity` (context = draft cid) and `review::submit_review` (context = item cid + epoch) require it, and `review::NullifierSet` keys per-item dedup on that id, not on `derive_nym`. A bare `Nym` carries no proof and cannot act (`AT-PRO-01`); a proof is bound to its action context and cannot be replayed (`AT-ID-05`); tests in `protocol/tests/inv9_nym_proof.rs`. **Remaining for a full Sybil claim:** the cryptographic-grade enrollment/replay hardening and per-credential quota are T20/T11, and the bespoke nullifier composition is still externally UNREVIEWED (§7.4).

#### PROTO-008 — Supplementary review — RESOLVED@T10/T30 (was NOT SPECIFIED). `gate::supplementary_review` (re-run bridging, decide `b_j` vs τ) + `lifecycle::Event::Resolve` (→ `Pilot1` or `Rejected(Borderline)`); `supplementary_redecision.rs`.

#### PROTO-009 — Honeypot
- `inject` and `reviewer_skill` TESTED. Not specified: how golden items' "known quality" is established without a Level-B run (a committee opinion — which is the thing Level A is not supposed to trust), how committee members are prevented from reviewing their own golden items, and how the base-rate baseline (REPUTATION-003) interacts with a deliberately balanced golden set.

#### PROTO-010 — Governance — `stratified_sortition` TESTED; `change_approved` = 2/3 ∧ ≥ 30 days. The sortition draws from candidates carrying `f_u`, i.e. judge nyms; how a drawn judge nym then *acts* (produces golden items, sets blueprints) without linking to its propose nym is unspecified (PRIV-004).

#### PROTO-011 — Draft serialization — DEFECT (NET-001 note): `Draft::content_id` MUST length-prefix its fields.

---

## 6. Mathematical specification (normative) and audit

Each subsection gives: the model as it MUST be implemented, its parameter constraints, identifiability conditions, numerical failure modes, finite-sample limitations, the implementation's conformance, and what empirical characterization is still required. "Mathematically correct" is asserted nowhere; conformance to a stated equation is.

### 6.1 Bridging factorization

**Model.** For observed set `Ω ⊆ [n]×[m]`, `r_uj ∈ [0,1]`:
```
r̂_uj = μ + b_u + b_j + f_u·f_j            (d = 1; the code implements only d = 1)
L(θ) = Σ_{(u,j)∈Ω} (r_uj − r̂_uj)² + λ_b(Σ_u b_u² + Σ_j b_j²) + λ_f(Σ_u f_u² + Σ_j f_j²)
B_j  = min_{s ∈ {full} ∪ {1..m_boot}} b_j^{(s)}
```
**Parameters.** `λ_b = 0.15`, `λ_f = 0.03`, `m_boot = 10`, `keep = 0.85`, `τ ≈ 0.08`, `ε ≈ 0.008`. MUST satisfy `λ_b > λ_f > 0`. `τ`, `ε` MUST be recalibrated per deployment (docs/02 §A.3); no calibration procedure exists.

**Identifiability.** (i) Sign of `f`: `(f_u, f_j) → (−f_u, −f_j)` leaves `L` invariant; the code does not fix the sign (tests take `|corr|`). Any consumer of `f_u`'s sign across epochs (stratified assignment, sortition, `appeal_threshold` on `|f_j|` is sign-free) MUST canonicalize the sign (e.g. fix the sign of a designated reference item). (ii) `μ` vs `b`: `μ` is unregularized, `b` regularized → identifiable. (iii) Scale of `f_u` vs `f_j`: fixed only through `λ_f` symmetric penalty. (iv) Connectivity: if the bipartite graph `Ω` is disconnected, components have independent `(μ+b)` offsets and `f` signs — the engine does not check connectivity; assignment with `k = 7–11` per item and random draws makes disconnection unlikely but not impossible at small `m`.

**Numerical failure modes.** Non-convex (bilinear); L-BFGS finds a stationary point dependent on the seeded init; the warm-started bootstrap intentionally correlates the subsample solutions with the full solution (this suppresses optimizer noise *and* sampling variability — the pessimistic min therefore underestimates true bootstrap spread; not characterized). No convergence status (OPT-001). `random_init` uses Box–Muller with `ln`/`cos` (platform libm).

**Finite-sample.** Tested at `n = 200, m = 10, |Ω| = 1800`. Behaviour at design scale (`n` in the thousands, `m` in the hundreds per epoch, ~9 ratings per node, `k` per item) is not characterized; `n_min = 30` is unimplemented.

**Conformance.** `bridging.rs` conforms to the equations. Deviations from the sim: warm-start, inclusion of the full fit in the min, different subsample RNG (both documented in `ARCHITECTURE.md` except the full-fit inclusion).

**Required characterization.** Seed sweep (≥ 100 seeds) at the fixture size; sweeps over split ratio (50/50 → 90/10), rating noise, sparsity, `k`; `d = 1` fit on `d = 2` populations; capture-cost curves with confidence bands; sensitivity of verdicts to `(λ_b, λ_f, τ, ε)`.

### 6.2 L-BFGS (`optim::lbfgs`)

Two-loop recursion, history `m_hist`, initial scaling `γ = sᵀy/yᵀy`, Armijo backtracking (`c₁ = 1e-4`, halving, ≤ 60 backtracks, `step ≥ 1e-20`), curvature pairs kept iff `sᵀy > 1e-12`, stop on `‖g‖_∞ ≤ g_tol` or relative progress `≤ 1e-12(1+|f|)` or `max_iters`. No bounds (the sim's `L-BFGS-B` is called without bounds, so this is equivalent in intent). Unit tests: quadratic, Rosenbrock, numerical-gradient check.
**Defects.** No status (OPT-001). Armijo-only line search does not guarantee the strong-Wolfe curvature condition; the `sᵀy > 1e-12` filter compensates for positive-definiteness but the history can starve (no pairs added) and the method degrades to scaled steepest descent silently.

### 6.3 Logistic regression (`glm::fit_logistic`)

Unpenalized MLE via `lbfgs`, `g_tol = 1e-8`, `max_iters` 200 (2PL) / 400 (DIF). Stable `sigmoid`/`softplus`. **Failure mode:** complete or quasi-complete separation → unbounded MLE; returned coefficients are whatever the iteration cap leaves. Any verdict computed from such a fit is undefined. The specification MUST either (a) add a weak ridge penalty (e.g. `1e-4·‖w‖²`) and document it, or (b) detect separation and mark the item "undetermined".

### 6.4 IRT

**Ability.** `θ_i = (T_i − mean T)/sd_pop(T)`, `T_i = Σ_anchor X_ia`. Requires `sd > 0`. Not an IRT ability; downstream thresholds are in this proxy's metric (IRT-001).
**2PL per item.** `logit P(X_ij = 1 | θ_i) = w₁ θ_i + w₀`; `a_j = w₁`, `b_j = −w₀/w₁` (NaN/∞ when `w₁ = 0`). Retention: `a_j ≥ A_MIN = 0.6`. `B_ABS_MAX = 2.5` is defined and never applied.
**Point-biserial.** Pearson between the 0/1 item and `total` (caller passes `θ`, a linear transform of the anchor total ⇒ identical correlation). Retention `≥ 0.20`; negative ⇒ inverted key. The spec's "total score on the rest of the test" is not what is computed (anchor total is used).
**Not implemented.** 3PL (`c_j`), infit/outfit MNSQ, `|b| ≤ 2.5`.
**Required.** Either estimate items on an IRT-scaled `θ` (e.g. EAP under the anchors' 2PL) or re-derive `A_MIN` for the proxy metric by simulation; add 3PL or justify its absence for the item types admitted (multiple choice with ≥ 4 options is exactly where guessing matters; item 02 already fails because of it).

### 6.5 DIF Variant 1

`logit P = β₀ + β₁θ + β₂g + β₃θg`, `g ∈ ℝ` (spec: continuous `f_i`; fixtures: `±1`). Reject if `|β₂| > 0.40`. No SE, no LRT, no Bonferroni/FDR across a batch. MH on `g > 0` with equal-frequency θ strata (`n_strata` caller-chosen; 5 in tests), `Δ_MH = −2.35 ln α_MH`, classes at 1.0/1.5 without significance. Purification: flag → recompute θ on anchors ∪ clean batch → repeat to a flagged-set fixed point (≤ `max_rounds`).
**Admissible input.** NONE under INV-1/INV-4 (DIF-002).

### 6.6 DIF Variant 2 (latent-class mixture)

```
P(X_ij = 1 | θ_i, z_i) = σ( a_j (θ_i − b_j − δ_j z_i) ),   z_i ∈ {−1,+1},  P(z=+1) = π
ℓ(π, a, b, δ) = Σ_i log[ (1−π)·Π_j P(x_ij | z=−1) + π·Π_j P(x_ij | z=+1) ]
LR = 2(ℓ_full − ℓ_null(δ≡0)),   BIC_gain = LR − K·ln(NT)   (> 0 ⇒ two classes)
DIF_j (spec) = |b_j^{+} − b_j^{−}| = 2|δ_j|;  code reports |δ_j| and rejects at 0.5
```
Init: `a = 1, b = 0, δ ~ 0.3·N(0,1)` (seeded), `logit π = 0`. Optimizer: `lbfgs` with central differences `h = 1e-5`, `g_tol = 1e-6`, ≤ 3000 iters.
**Identifiability.** Label switching `(δ, π) ↔ (−δ, 1−π)` resolved by `|δ|`. Under the null, `π` is unidentified and the LR statistic is not χ²_K (boundary + non-identifiability: Self–Liang / mixture-LRT irregularity); BIC comparison is a heuristic, not a calibrated test. `θ` is fixed at the anchor proxy, so measurement error in `θ` is absorbed into `a`, `b`, `δ` (not characterized).
**Numerical.** Numerical gradient on an NLL of magnitude ~`NT·K·ln 2` has cancellation error ~`ε_mach·f/h ≈ 3e-7` per component at `NT = 3000, K = 8`, comparable to `g_tol`; convergence is effectively decided by the progress criterion. Cost scales as `O(NT·K²)` per gradient (DIF-009).
**Regime tested.** NT = 3000, K = 8, balanced classes, `δ = 0.9`, uniform DIF only, one axis. Nothing else.
**Required.** Analytic gradient; `G ∈ {2,3,4}` with a documented selection rule; FP rate at `n_biased = 0`; power surfaces over `(NT, K, n_biased, δ, π)`; non-uniform DIF (`a_j` shift); two simultaneous axes; θ misspecification.

### 6.7 Reputation

`C_a` as in REPUTATION-001. BSS: `1 − Σ(p−o)²/Σ(p̄−o)²` with `p̄ = mean(o)` (REPUTATION-003). `E_u = σ(γ·BSS)`, `γ` unspecified in docs (tests use 2.0). EMA: `E ← E + r·(new − E)`, `r = up` if `new ≥ E` else `down`; `(up, down)` unspecified. Cap `3·median(w)` (REPUTATION-005). Dasgupta–Ghosh: binary agreement minus baseline, per pair; no aggregation over pairs/items specified. Bayesian Truth Serum: not implemented.
**Required.** Specify `γ`, `(up, down)`, the crowd baseline, the definition of `q_j`, and an incentive analysis (at least: is honest reporting a best response under the base-rate baseline when the reviewer knows the batch's approximate base rate?).

### 6.8 Anti-collusion

`ρ_uv` = Pearson over dense rows (undefined rows → 0); clusters = connected components of `{|ρ| ≥ thr}`; `W(G) = (Σ_{u∈G} w_u)^α`, `α = 0.5`, split pro-rata. Defects: COLLUSION-002…005. **Required.** A definition on sparse data (e.g. shared-item rank correlation with a minimum overlap and a permutation null), a clustering rule with a stated FP rate for honest like-minded reviewers, the fix for INV-14, and — above all — a consumer (BRIDGE-007).

### 6.9 Blueprint apportionment and sortition

Hamilton largest-remainder apportionment (deterministic, ties by index); proptest checks sum and ±1 of exact share. Stratified sortition: equal-frequency strata on `f_u`, seats spread by `⌊seats·(s+1)/S⌋ − ⌊seats·s/S⌋`, deficit filled uniformly. Both conform to their doc comments. Neither has a stated randomness source (CRYPTO-008).

### 6.10 Equations in the repository with no implementation

Logarithmic score (`docs/02` C.2); 3PL; infit/outfit; `d = 2`; `n_min`; `w = min(w_max, E_u)` *as consumed by bridging*; BTS; the throughput and availability tables of `docs/02` §B.6 and `docs/04` (the erasure availability numbers 0.973/0.998/0.983/~1.000 are quoted without a derivation or a churn model).

---

## 7. Cryptographic specification

### 7.1 Primitive inventory

| Use | Standard / construction | Library, version (`Cargo.lock`) | Randomness | Domain separation | Status |
|---|---|---|---|---|---|
| Role nym (protocol) | SHA-256 over `(len‖"isegoria/nym/v1", len‖secret, len‖role)` | `sha2` 0.10.9 | none (deterministic) | tag + length prefixes | REAL hash; no proof of validity |
| RLN token | SHA-256 `(…/rln/v1, secret, role, epoch_le, slot_le)` | `sha2` | none | yes | collision detector only (ID-008) |
| Uniqueness label (single) | RFC 9497 VOPRF, Ristretto255-SHA512, DeriveKeyPair(info=`isegoria/uniqueness/v1`); output re-hashed with tag `…/uniqueness/voprf/v1` | `voprf` 0.5.0, `rand_core` 0.6.4 (`OsRng`) | client blind: OsRng; server proof nonce: OsRng | RFC 9497 + tag | REAL primitive, wrong interface (ID-002) |
| Uniqueness label (threshold) | 2HashDH: `W = k·H₁(x)`, `label = H₂(x, W)`; Shamir `f(0)=k`; Chaum–Pedersen DLEQ per partial; Lagrange at 0 | `curve25519-dalek` 4.1.3, `sha2` | blind `r`, DLEQ nonce: OsRng; **key: seed-derived (dealer)** | `…/oprf/{hash-to-group, dleq-challenge, output, keygen}/v1` | REAL math, modelled deployment |
| Credential | BBS+ (BLS12-381, G1 signatures, G2 keys), 2 messages `(x, label)`, blind issuance via Pedersen commitment + Schnorr PoK (FS over `bases‖C‖t‖label`, SHA-256) | `bbs_plus` 0.25.0, `schnorr_pok` 0.23.0, arkworks 0.4.x | OsRng (blinding, PoK); issuer key: seed-derived | `isegoria/bbs+/v1` params label | REAL, single and threshold |
| Threshold credential | `bbs_plus::threshold` (DKLS-style OT multiplication, `κ=256`, `stat=80`, base-OT key 128) | `oblivious_transfer_protocols` 0.12.0, `secret_sharing_and_dkg` 0.16.0, `blake2` 0.10.6, `sha3` 0.10.9 | `StdRng::from_seed(seed)` for dealer **and base OT**; OsRng for signing | `isegoria/bbs+/{gadget,threshold}/v1` | REAL protocol, in-process committee |
| Nullifier | `N = x·H_role`, `H_role = WB hash-to-G1(role, DST …/nullifier/hash-to-g1/v1)`; AND-composed with BBS+ PoK (shared blinding for msg 0, shared FS challenge) | `bbs_plus`, `dock_crypto_utils` 0.23.0, arkworks | `ρ`: OsRng | yes | REAL, bespoke composition, unreviewed, unused by protocol |
| Commit–reveal | SHA-256(`isegoria/commit/v2`‖prob_le‖nonce‖committer‖item) | `sha2` | nonce: caller | tag; fixed-size fields | binds committer + item (CRYPTO-007 RESOLVED@T7) |
| Checkpoint | ed25519 over SHA-256(`…/checkpoint/v1`, height_le, head) | `ed25519-dalek` 2.2.0 | key: seed | tag | REAL; no network id (NET-006) |
| CID / Merkle / log | SHA-256 with tags `…/cid/v1`, `…/merkle/{leaf,node,empty}`, `…/log/entry` | `sha2` | — | yes | REAL; Merkle leaf-count defect (NET-003) |
| Anchoring | OpenTimestamps `.ots` (SHA-256 ops, Bitcoin attestation) | `opentimestamps` 0.2.0 | — | — | REAL format; no network |
| Erasure | Reed–Solomon GF(2⁸) | `reed-solomon-erasure` 6.0.0 | — | — | REAL |
| Engine RNG | ChaCha8 seeded `u64` | `rand_chacha` 0.3.1, `rand` 0.8.8 | seeded | — | deterministic (not cryptographic use) |

Two `rand`/`rand_core` major versions coexist in the lock file (0.8/0.6 and 0.9/0.9); the crates use 0.8/0.6 explicitly. Not a defect; a maintenance hazard.

### 7.2 Threat model per primitive

| Primitive | Adversary | Trust assumptions | Key management | Replay | Revocation | Metadata leakage | Side channels |
|---|---|---|---|---|---|---|---|
| VOPRF / threshold OPRF | malicious committee members `< t`; malicious client | dealer honest (until DKG exists); `t` honest online; DDH/OM-DH | seed in memory; no rotation (INV-11) | a blinded element may be re-submitted; harmless (same label) but rate-limits enrollment attempts nowhere | none | committee learns *that* a label was requested and when | scalar mult in `curve25519-dalek` is constant-time; `Scalar::invert` constant-time; the `find` over shares is not (irrelevant) |
| BBS+ issuance | malicious issuer(s) `< t`; malicious holder | q-SDH, DL on BLS12-381; dealer honest; base-OT seed independent (violated in code) | seed-derived; no rotation | request replay → duplicate credential for same `x` (no harm) but **no per-label issuance limit** (ID-007) | none | issuer learns label, timing | arkworks field ops are not guaranteed constant-time; holder-side secret handling unreviewed |
| Nullifier proof | malicious holder (forge/mis-bind); linking adversary (issuer, operators) | SXDH; ROM for FS; BBS+ PoK ZK | holder secret in `Credential` (plain `[u8;32]`, `Clone`, `Debug` prints it) | a proof is not bound to an action/message: **the same proof can be replayed by anyone to attach `N` to a different action**; the proof MUST include the action's CID/epoch in the challenge | none | `N` is a stable identifier per role (by design) | as above |
| Checkpoints | `≥ t` colluding signers; replay | ed25519 EUF-CMA | seed-derived; no rotation/revocation | yes (NET-006) | none | — | — |
| Log | any writer with the head | SHA-256 | — | consistent rewrite (NET-004) | — | payloads are CIDs only; timing not recorded (good) | — |

### 7.3 Reference implementations vs production

Per `README.md`/`ARCHITECTURE.md`, the following are **explicitly non-production** and this specification concurs: `ReferenceOracle` (keyed hash, test-only), trusted-dealer keygen for both committees, in-process committees, injected Bitcoin block source, `OtsAnchor::upgrade`. This specification adds to that list: `nym::derive_nym` as the protocol's identifier (must be replaced by the nullifier), `ratelimit::*` (no enforcement), `review::commit` (missing bindings), and all seed constants in tests (no randomness source).

### 7.4 External review required

Before any deployment: (1) the threshold OPRF composition and its DLEQ transcript; (2) the nullifier–BBS+ AND-composition (`nullifier.rs`), including the use of `MessageOrBlinding::BlindMessageWithConcreteBlinding` and `get_resp_for_message`; (3) the threshold BBS+ setup randomness; (4) the whole enrollment protocol once ID-004/ID-005 are specified. Passing tests are not evidence for any of these.

---

## 8. Privacy model

### 8.1 Adversaries

| Adversary | Observes | Colludes with | Goal |
|---|---|---|---|
| A-STATE | that person P enrolled (F1); CF of P | IdP; possibly `< t` committee members; possibly the label registry custodian | link P to any nym or action |
| A-ISSUER | issuance transcripts `(C, label, PoK)`; label registry (if custodian) | `< t` peers | link a nullifier/action to an issuance |
| A-OPERATOR (consortium member, re-runner) | full ratings `(u, j, r)`, answers `(i, j, x)`, all `f_u`, `b_u`, all commitments/reveals, log timing | other operators | link nyms across roles; deanonymize by stylometry/timing/topic |
| A-PARTICIPANT | its own assignments and everything public | other participants (cartel) | identify who wrote/judged an item |

### 8.2 Property statements (each must name its adversary)

| ID | Property | Adversary | Holds if | Status |
|---|---|---|---|---|
| PRIV-P1 | A-STATE cannot compute label(P) | committee `< t` colluding with the state; OPRF secure | ID-003 deployment + INV-11 | HYPOTHESIS (modelled) |
| PRIV-P2 | A-ISSUER cannot link `N_role` to an issuance | label never revealed after issuance; BBS+ PoK ZK; SXDH | PRIV-002 | HYPOTHESIS (design contradiction in docs) |
| PRIV-P3 | A-OPERATOR cannot link `N_propose` ↔ `N_judge` ↔ `N_respond` cryptographically | SXDH | CRYPTO-006 | HYPOTHESIS |
| PRIV-P4 | A-OPERATOR cannot link roles *statistically* | text normalization, timing mixing, topic quotas, population ≥ floor | PRIV-003 | NOT IMPLEMENTED |
| PRIV-P5 | A-PARTICIPANT cannot learn who reviews an item before the verdict | assignment private; commitments unlinkable to nyms until reveal | current `commit` has no nym; the assignment list is a plain `Vec<Reviewer>` with nyms | NOT ESTABLISHED |
| PRIV-P6 | Voting patterns are not on a public register in the clear | — | contradicts INV-7 as designed (PRIV-004) | UNRESOLVED |
| PRIV-P7 | Small-crowd degradation is bounded | population ≥ ~2,000 | no model | HYPOTHESIS |

### 8.3 Leakage inventory in the current code

- `Credential` derives `Debug` and prints the secret.
- `EnrollmentRegistry` stores raw labels; `Label` derives `Debug`.
- `review::Reviewer { nym, f_u }` couples a pseudonym with its political-axis estimate in the assignment API.
- `governance::Candidate { id, f_u }` likewise.
- `log::Entry` records `seq` only (no timestamp) — compliant with docs/03's "no precise timestamp in the public log".
- No encryption anywhere: items under review are plaintext `Draft` bytes; the log holds only CIDs, but content distribution is unspecified, so "questions under review stay encrypted until publication" (docs/04) has no implementation.

---

## 9. Protocol state machines

§9.1 is now a real state machine: `protocol::lifecycle` (T12) owns per-item `State`, and `lifecycle::step`/`deposit` reject every checkable "invalid case" row (`tests/orchestrator.rs`). Preconditions that need primitives from other tasks (identity nullifier T6, RLN quota T11, checkpoint seed T8, commit-copy T7) enter as explicit proof inputs the machine checks. §9.2–9.5 below remain the **specification the code MUST be brought to**. `end_to_end.rs::run_epoch` (the fixture walk) is now routed through `lifecycle::step` (via `orchestrator::run_item`), so the flow no longer lives twice (RESOLVED@T12). The `SupplementaryReview` forward transition is defined via `Event::Resolve`, the D26 re-decision (RESOLVED@T10/T30, PROTO-008).

### 9.1 Item lifecycle

| Current state | Event | Preconditions | Next state | Side effects | Invalid cases (MUST be rejected) |
|---|---|---|---|---|---|
| — | `deposit_with_identity(draft, proof)` | `primary_source ≠ ∅`; author presents `NullifierProof(Propose)` bound to the draft cid (INV-9 ✓ T6); valid RLN proof for `(epoch, slot < quota(C_a))` (ID-008) | `Deposited` | `log.append(cid(draft))`; slot consumed | missing source (`NoPrimarySource` ✓); duplicate CID; unproven nym (✓ T6, `DepositRejected::Unproven`); over-quota (✓ T11, `DepositRejected::OverQuota` via `QuotaLedger`) |
| `Deposited` | epoch close → `admit_from_beacon()` | lottery seed = `Beacon::seed("lottery", epoch)` = `H(signed head ‖ height ‖ …)` (INV-10 ✓ T8); capacity fixed by blueprint | `Admitted` or stays `Deposited` (carry-over policy unspecified) | — | seed chosen by a participant (✓ T8: only `_from_beacon` derives it) |
| `Admitted` | `assign_reviewers(k, seed_item)` | `k` odd ∈ [7,11]; candidates = established + founder nyms with `f_u`; probation nyms MAY be assigned at weight 0 | `InReview{commits: ∅}` | private assignment list | author in its own panel (✗ not checked — the author's judge nym is unlinkable, so this cannot be checked; MUST be accepted as residual risk or handled by the honeypot); `k` even |
| `InReview` | `commit(N_judge, cid, prob, nonce)` | `N_judge` in panel; no prior commit by `N_judge` for `cid`; before commit deadline | `InReview` | store `Commit` | commit from non-panel nym; second commit; commitment copied (✗ INV-12 not implemented) |
| `InReview` | commit deadline | — | `Revealing` | publish commitments | — |
| `Revealing` | `reveal(N_judge, cid, prob, nonce)` | `commit(prob,nonce,N_judge,cid)` matches; `prob ∈ [0,1]` | `Revealing` | store rating `r = prob` | mismatch; NaN/out-of-range prob (✗ not checked); reveal by a different nym |
| `Revealing` | reveal deadline | ≥ `k_min` reveals (unspecified) | `Gated` | non-revealers: `E_u` penalty (unspecified) | — |
| `Gated` | epoch scoring: `bridge_scores` → `bridging_gate(B_j, f_j, τ, ε, α_appeal)` | ratings of the whole epoch available; engine run reproducibly | `Pilot1` (Pass) / `SupplementaryReview` / `AppealEligible` / `Rejected` | scores published with checkpoint | scoring on a partial epoch |
| `SupplementaryReview` | D26 re-decision: re-run bridging over the expanded panel, decide `b_j` vs the plain threshold τ (`gate::supplementary_review`, `Event::Resolve`) | band item scored | `Pilot1` if `b_j ≥ τ` else `Rejected(Borderline)` | — | resolved (T10/T30); production "add reviewers" folds them into the re-fit ratings |
| `AppealEligible` | `appeal(N_propose, stake)` | within appeal window; `C_a ≥ stake` | `Pilot1{appealed}` | stake escrowed (REPUTATION-007) | appeal after window; appeal on `Reject` |
| `AppealEligible` | window expires | — | `Rejected` | — | — |
| `Pilot1` | batch of ≥ `N₁` distinct respondents (`≈300`) answered | respondents present `NullifierProof(Respond)`; item mixed with validated items; answers do not count toward respondent score | `Pilot2` if `r_pbis ≥ 0.20 ∧ a ≥ 0.6` (`stage1_screen`) else `Rejected{Screen}` | — | `N₁` not met (✓ T9: `pilot::screen` → `NotEnoughRespondents`); duplicate respondent nullifier (✗ no check) |
| `Pilot2` | batch of ≥ `N₂` respondents **and** ≥ `K_min` items in the batch (INV-8; `K_min` unspecified, ≥ 2 by DIF-005, ≥ 8 by the tested regime) | mixture DIF (Variant 2) run on the batch; Variant 1 only in attributed pilots | `ActivePool` if `DIF_j ≤ cut` else `Rejected{DIF}`; appealed items: stake settled | `q_j` recorded → `author_score`; `o_j` recorded → evaluator BSS | batch of 1 (✓ T9: `revalidate_batch_latent`/`dif_batch` → `BatchTooSmall`); Variant 1 with a linked/declared group in production (✓ T32, gated behind `calibration`) |
| `ActivePool` | administration | blueprint quotas respected; `exposure.record(cid)` | `ActivePool` | exposure++ | — |
| `ActivePool` | periodic re-validation | whole-pool or batched mixture run (DIF-009 bound) | `Retired{EmergingDif}` / stays | — | — |
| `ActivePool` | `exposure ≥ EXPOSURE_LIMIT (2000)` | — | `Retired{Exposure}` | template rotation | — |

### 9.2 Reviewer (judge nym) reputation

| State | Event | Precondition | Next | Effect |
|---|---|---|---|---|
| `Probation{n<200}` | outcome `o_j` known for a reviewed item | — | `Probation{n+1}` or `Established` at 200 | BSS accumulates; weight 0 |
| `Founder` | same | declared at bootstrap | `Established` at 200 | weight 1 until then |
| `Established` | epoch close | — | `Established` | `E ← ema(E, σ(γ·BSS_epoch))`; `w = min(3·median, E)` (vacuous cap, REPUTATION-005); honeypot BSS folded in (rule unspecified) |
| any | detected block voting (cluster) | COLLUSION-002/003 fixed | same | `w ← w·s^{α−1}` (INV-14) |

### 9.3 Enrollment and issuance (target protocol; current code is a single in-process call)

| Step | Party | Message | Precondition | Failure |
|---|---|---|---|---|
| E1 | Holder ↔ IdP | eID authentication; IdP returns an attestation `A = Sig_IdP(commit(cf))` or blinded equivalent (ID-004 — **design open**) | real document | reject |
| E2 | Holder → committee (`t` members) | `B = r·H₁(cf)` + proof that `B` is consistent with `A` (ID-004) | — | reject |
| E3 | Members → holder | `Z_i = k_i·B` + DLEQ_i | member has share `i` | invalid DLEQ → drop member, need another |
| E4 | Holder | `W = r⁻¹·Σ λ_i Z_i`, `label = H₂(cf, W)` | ≥ `t` valid partials | — |
| E5 | Holder → registry/issuer | `label` + (ID-005 — **design open**: proof of correct derivation or committee-side re-derivation) | label ∉ registry | `DuplicateEnrollment` |
| E6 | Holder → issuer(s) | `(C = commit(x), label, PoK)` | one issuance per label (ID-007) | `InvalidProofOfKnowledge`, `Signing`, `AlreadyIssued` |
| E7 | Issuer(s) → holder | blind BBS+ signature (single or MPC-aggregated) | ≥ `t` members | — |
| E8 | Holder | unblind; verify; derive `N_role = x·H_role` on demand with `nullifier::prove` | — | — |

### 9.4 Consortium checkpoint (target)

| State (light client) | Event | Precondition | Next | Invalid |
|---|---|---|---|---|
| `Trusted{h, head, member_set}` | receive `cp{h', head', net_id, member_set_hash}` + sigs | `net_id` matches; `member_set_hash` matches; `≥ t` distinct valid sigs; `h' > h`; a consistency proof or the entries `h..h'` show `head'` extends `head` | `Trusted{h', head'}` | `h' ≤ h` (stale — ignore); `h' > h` with a non-extending head (fork — **alarm**, record both) |
| any | two valid cps with same `h'`, different `head'` | — | `Forked` | equivocation evidence published (accountability rule unspecified) |

Implemented@T15: `Checkpoint` carries `network_id`/`member_set_hash` in its signed message and `consortium::CheckpointClient` is this state machine (net/member-set binding, monotonic-height replay rule, same-height equivocation → `Forked`). The consistency-proof arm of the `h' > h` transition (a higher head that does not extend `head`) is provided by `log::verify_extends` (T14) for a client that also holds the log.

---

## 10. Distributed-systems semantics

### 10.1 Append-only log
- **State.** `Vec<Entry{seq, prev, payload: Cid, hash}>`; `head = last.hash` or `0³²`.
- **Append.** `hash = SHA-256(tag, seq_le, prev, payload)`; `seq = len`. Prior entries are never mutated by the API (`tamper_payload` is `#[cfg(test)]`).
- **Verify.** `verify()` recomputes from genesis (inconsistent edits); `verify_extends(&prior)` (T14) detects consistent suffix rewrites (`ForkedHistory`) and truncation (`Truncated`) against a consortium-signed prior head (NET-004 RESOLVED@T14).
- **Required semantics.** `verify_extends(old_head, old_len) → bool` (consistency); signed heads (per-writer or consortium); a definition of *who* may append (currently anyone holding the `&mut`).

### 10.2 Merkle tree
- Duplicate-last-node construction; inclusion proofs verify; **root does not commit to leaf count** (auditor-verified collision `[x,y,z]` vs `[x,y,z,z]`). Not used by the protocol yet. MUST be replaced (RFC 6962) before roots are anchored.

### 10.3 Checkpoints, replication, convergence
- Replication, gossip, DHT, CRDT: absent. "Writes almost never conflict" (docs/04) is an assumption about workload; the one conflict that matters — two reveals for the same `(nym, item)`, two deposits of the same CID, two checkpoints at one height — has no merge rule anywhere.
- **Required convergence invariant (to be stated when CRDT work starts).** For any two replicas `R₁, R₂` that have received the same set of signed entries in any order, `state(R₁) = state(R₂)`; the state MUST be a function of the *set* of entries, which forces per-writer sequence numbers or a grow-only set with deterministic ordering for scoring input (this also resolves REPRO-002).

### 10.4 Erasure coding
- RS(k, n) on byte shards; systematic. No shard hashing → a corrupted (not missing) shard silently corrupts the output. Required: shard `Cid`s in the manifest, verification before decode, and a repair policy.

### 10.5 Anchoring
- Format-level only. Required: submit `checkpoint.message()` (not the raw log head) hourly; store receipts in the log; verifier reads Bitcoin headers via SPV; define behaviour when the calendar is unavailable (Pending indefinitely).

### 10.6 Crash recovery, partitions, operator compromise
- No persistent state exists (everything is in-memory `Vec`/`HashMap`), so crash recovery is undefined. Partition behaviour is undefined (no network). Operator compromise: a compromised signer with `< t` allies can only refuse or sign truthfully; with `≥ t` it can sign anything (NET-006); the "reproducible computation unmasks it" defense requires re-runners to have the input (PRIV-004) and a published mapping from checkpoint → engine input hash → outputs, which does not exist.

---

## 11. Threat model

The adversary wants Isegoria to accept a partisan item, reject a fair one, deanonymize a participant, or rewrite history. The adversary controls some real persons (each with one genuine credential), possibly some committee members below threshold, possibly some consortium members below threshold, and can read everything a re-runner can read. Unless stated, the adversary cannot break SHA-256, ed25519, DDH on Ristretto255, or SXDH/q-SDH on BLS12-381.

Classification vocabulary: PREVENTED (cannot happen given assumptions), DETECTED (happens but is observable), CONTAINED (bounded impact), PARTIALLY MITIGATED, ACCEPTED RESIDUAL RISK, UNSOLVED. "No trivial exploit found" is never mapped to PREVENTED.

### 11.1 Identity

| Attack | Mechanism in design | Result at this commit | Evidence |
|---|---|---|---|
| Sybil via fake anchors | OPRF input bound to eID | **UNSOLVED** — binding unspecified (ID-004); `Cie{codice_fiscale}` is a free string | none |
| Duplicate enrollment, same person, two sources | canonical anchor → same label | DETECTED in the registry for the *same string*; **UNSOLVED** for foreigners with two anchor spaces (docs/03 F2) and for any holder that can pick its input (ID-004) | `properties.rs` |
| Multiple credentials for one label | issuer checks registry / one-per-label | **UNSOLVED** — issuer signs any label any number of times (ID-007) | none |
| Credential cloning (share the secret) | — | ACCEPTED RESIDUAL RISK by design: sharing `x` shares one identity; the two users collide on every nullifier and gain nothing | — |
| Credential compromise / theft | revocation | **UNSOLVED** — no revocation; non-rotatability makes the loss permanent | none |
| Credential rotation (whitewashing) | deterministic nym, one credential per person | PARTIALLY MITIGATED: derivation is deterministic (TESTED); one-credential-per-person NOT ESTABLISHED (ID-007) | `adversarial.rs` |
| Issuer compromise `< t` | threshold | CONTAINED in the model; NOT ESTABLISHED in deployment (dealer, in-process) | `oprf.rs` tests |
| Issuer compromise `≥ t` | — | ACCEPTED: can issue unlimited credentials (undetectable — issuance is blind) and brute-force labels (docs/03 M1) | — |
| Registry custodian + `≥ t` OPRF shares | — | **UNSOLVED**: enumerates enrolled persons (ID-005) | — |
| Nullifier proof replay onto another action | proof bound to action | **UNSOLVED** — proof not bound to a message (§7.2) | none |
| Rate-limit evasion | RLN | **UNSOLVED** — tokens unverifiable (ID-008) | none |
| Key rotation re-enables everything above | INV-11 | **UNSOLVED** — lifecycle unspecified (ID-006) | none |

### 11.2 Reputation

| Attack | Result | Evidence |
|---|---|---|
| Whitewashing | see above | — |
| Long-con (accumulate then spend) | PARTIALLY MITIGATED by asymmetric EMA + cap; rates unspecified; incentive analysis absent (REPUTATION-004); cap vacuous (REPUTATION-005); and **no weight is consumed** (BRIDGE-007), so at this commit reputation has no effect to spend | `adversarial.rs` arithmetic only |
| Strategic abstention (review only "easy" items) | random assignment; non-reveal penalty | UNSOLVED — no non-reveal rule; abstention after seeing the item is free |
| Score farming via honeypots | sortition-produced golden items | UNSOLVED — committee members know the golden set (PROTO-009) |
| Majority following | BSS vs crowd baseline | PARTIALLY MITIGATED: under the implemented base-rate baseline "follows peers" scores −1.33 on the fixture, but the documented property (≈ 0) is not what is implemented (REPUTATION-003); depends on outcome base rate |
| Deliberate contrarianism | proper scoring rule | CONTAINED: BSS is proper; a contrarian who is wrong loses; a contrarian who is right *should* gain (by design) |
| Cartel scoring (agree on predictions to farm BSS) | anti-collusion | UNSOLVED — BSS is per reviewer against outcomes; coordination does not change BSS but does change bridging (below) |

### 11.3 Bridging

| Attack | Result | Evidence |
|---|---|---|
| Ideological cartel pushing a partisan item (own camp only) | PREVENTED in the tested regime: 40 own-camp boosters leave `b_j < τ` | `level_a.rs`, sim (auditor re-run) |
| Bipartisan corruption | CONTAINED: needs ≈ 55–70 of 80 of the opposing camp on the fixture (BRIDGE-005) — a cost, not a prevention; scales unknown | sim |
| Strategic ratings by a coordinated block with jitter | **UNSOLVED** (COLLUSION-002); and the discount has no effect on `b_j` anyway (BRIDGE-007) | auditor probe |
| Sparse-data manipulation (target items with few reviewers) | `k` fixed per item; `n_min` unimplemented | UNSOLVED — a cartel member landing in a 7-reviewer panel has 1/7 of the raw input; the model's robustness to one extreme rating per panel is uncharacterized |
| Faction impersonation (a cartel member pretends to be of the opposite camp on its history, then "bridges" a partisan item) | bridging estimates `f_u` from history | **UNSOLVED / not analysed**: a patient adversary can build a cross-camp `f_u` cheaply (ratings cost nothing) and then supply "cross-cutting approval" on demand; this is the *designed* trust signal and it is manufacturable at the price of `n_min` sincere-looking ratings |
| Rating inflation/compression (everyone rates 1.0) | `b_u` absorbs severity | CONTAINED for individual bias; global compression destroys the signal (not analysed) |
| Seed grinding for reviewer selection | INV-10 | SOLVED (T8): the seed is the signed checkpoint head; assignment keys on a byte-independent slot (CRYPTO-008) |
| Commitment copying | INV-12 | SOLVED (T7): the commitment binds committer + item (CRYPTO-007) |

### 11.4 Psychometrics

| Attack | Result | Evidence |
|---|---|---|
| Poisoned answer key | `r_pbis < 0` | DETECTED when the item discriminates and the key is fully inverted (`level_b.rs`); partially wrong keys (multiple-choice with two defensible options) not analysed |
| Deliberately ambiguous / non-discriminating item | `r_pbis`, `a` | DETECTED for the tested item; thresholds' FP/FN uncharacterized |
| Sample poisoning (cartel members answer as respondents to steer `θ` or `δ`) | respondents are the "hard to corrupt" resource per docs; anonymity means a cartel of `c` persons controls `c` of `NT` respondents | **UNSOLVED / not analysed**: what fraction `c/NT` of coordinated respondents flips a DIF verdict or masks one? No simulation exists. With NT ≈ 3000 and a mixture detector keyed on class structure, a coordinated block *is* a latent class — it may be detected as DIF or may *create* DIF on clean items |
| Coordinated answering to inject a fake latent class | as above | UNSOLVED |
| DIF camouflage (bias that is non-uniform, or split across two axes, or below `δ = 0.5`) | mixture at `δ = 0.9` only | UNSOLVED — nothing below 0.9 or non-uniform is tested |
| Latent-axis manipulation (bias aligned with `θ` itself) | DIF conditions on `θ` | ACCEPTED by design (docs/06 L4: a `θ`-correlated bias is "competence") |
| Topic-pool manipulation | blueprint quotas | PARTIALLY MITIGATED (apportionment implemented; the quota-setting committee is the attack surface, sortition unspecified in randomness) |
| Anchor-item contamination | purification | PARTIALLY MITIGATED for Variant 1 (DIF-007); anchors for Variant 2 are assumed clean with no purification loop |

### 11.5 Privacy

| Attack | Result |
|---|---|
| Timing correlation (deposit/commit/reveal times ↔ enrollment or other roles) | UNSOLVED — no mixing implemented (PRIV-003); the log has no timestamps but transport will |
| Stylometry on drafts | UNSOLVED — no normalization; `Draft.item` is free bytes |
| Topic correlation | UNSOLVED — no per-author domain quota logic |
| Participation intersection (which nyms were active in which epochs) | UNSOLVED — assignment lists and reveals are per nym per epoch; intersection across roles is a statistical linkage channel |
| Small-crowd deanonymization | ACCEPTED RESIDUAL RISK per docs/02 §B.6, unquantified |
| Operator + issuer collusion | see ID-005; label must never be revealed (PRIV-002) |
| Re-runner learns every judge's political position | UNRESOLVED design tension (PRIV-004) |

### 11.6 Network

| Attack | Result |
|---|---|
| Replay of an old checkpoint | SOLVED (T15): the client's monotonic-height rule ignores it (`Stale`); cross-network replay rejected by `network_id` binding |
| Equivocation (two heads at one height, `≥ t` sigs) | UNSOLVED — accepted twice, no detection |
| Fork by consistent suffix rewrite of a log | DETECTED against a consortium-signed prior head (T14, `verify_extends`); a client without a prior checkpoint still cannot judge history in isolation (inherent) |
| Partition | undefined (no network) |
| Stale-state injection to light clients | UNSOLVED — no client state |
| Malicious checkpoint with `≥ t` signers | ACCEPTED (docs: "freedom to fork"); detection via reproducible recomputation NOT IMPLEMENTED (no checkpoint→input→output mapping) |
| Corrupted shard | UNSOLVED (NET-007) |
| Gossip poisoning | undefined (no gossip) |
| Merkle leaf duplication | DEFECT (NET-003) — currently unexploitable only because nothing uses the root |
| OTS proof parsing of hostile bytes | PARTIALLY MITIGATED (library recursion limit; no fuzzing) |

---

## 12. Adversarial tests (required, keyed to attacks)

Each entry names the test that MUST exist, its oracle, and the claim it falsifies. Tests marked ✓ exist at this commit (with the caveats noted in §5); all others are absent.

| Test ID | Attack / property | Construction | Pass criterion | Falsifies |
|---|---|---|---|---|
| AT-ID-01 | fake-anchor Sybil | holder submits `B = r·H₁(random)` in the target protocol (§9.3 E2) without a valid attestation | rejected | ID-004 |
| AT-ID-02 | double credential | `issuer.issue(req₁); issuer.issue(req₂)` for the same label | second refused (`AlreadyIssued`) | ID-007 |
| AT-ID-03 | whitewashing via new secret | enroll once, request credential with `x₁`, then with `x₂` | second refused | ID-007 |
| AT-ID-04 | duplicate quorum indices | `label_with_quorum(input, &[1,1,2])` | `None`/error, never a label | ID-003 (ii) |
| AT-ID-05 | nullifier proof replay | take `NullifierProof` from action A, attach to action B | verifier rejects (requires message binding) | §7.2 |
| AT-ID-06 | rate-limit forgery | present `quota+1` distinct tokens in one epoch | rejected | ID-008 |
| AT-ID-07 ✓ | cross-source dedup | CIE then SPID same CF | `DuplicateEnrollment` | ID-001 |
| AT-ID-08 ✓ | DLEQ soundness | member with `k_i + 1` | partial rejected | ID-003 |
| AT-REP-01 | long-con, game-theoretic | agent maximizing Σ_t influence·(betrayal payoff) under EMA `(up, down)` and cap, with influence actually consumed by bridging | best response is honesty | REPUTATION-004 |
| AT-REP-02 | consensus follower | `p_uj := p̄_j` for all j | BSS ≈ 0 under the *specified* baseline | REPUTATION-003 |
| AT-REP-03 | denominator zero | all `o_j` equal | finite, defined result | REPUTATION-003 |
| AT-REP-04 | cap binds | weights = `E_u ∈ (0,1)` with median > 1/3 | cap has an effect or the spec is changed | REPUTATION-005 |
| AT-COL-01 ✓ | identical cartel | 400/500 identical rows | Σw = √k | COLLUSION-001 |
| AT-COL-02 | jittered cartel | shared pattern + `N(0, σ)`, σ ∈ {0.02, 0.05, 0.1} | discounted to within 10 % of √k | COLLUSION-002 |
| AT-COL-03 | sparse cartel | design regime: M = 500 items, 9 ratings/node, cartel votes identically *on shared items only* | detected | COLLUSION-003 |
| AT-COL-04 | sub-unit boost | singleton `w = 0.25` | discounted weight ≤ 0.25 | INV-14 |
| AT-COL-05 | griefing | attacker mimics honest node H's history to pull H into a cluster | H's weight unchanged or the effect bounded and documented | COLLUSION-005 |
| AT-COL-06 | influence, not weight | cartel of 400 vs 120 honest, weights *consumed by bridging* | `b_j` of a targeted item moves less than with 22 independents | BRIDGE-007 |
| AT-BR-01 ✓ | own-camp boost | 40 own-camp boosters | `b_j < τ` | BRIDGE-003 |
| AT-BR-02 | crossing curve | boosters 0..80 in steps of 5, ≥ 50 random selections each | crossing distribution reported with CI; docs updated | BRIDGE-005 |
| AT-BR-03 | permutation invariance | shuffle `obs` | bit-equal after canonicalization; `|Δb_j| < 1e-9` without | REPRO-002 |
| AT-BR-04 | cross-platform determinism | same input on linux-gnu, linux-musl, macOS-aarch64 | bit-equal, or documented divergence with tolerance | REPRO-001 |
| AT-BR-05 | seed grinding | author regenerates draft whitespace 1000× to select a panel | panel independent of draft bytes | CRYPTO-008 |
| AT-BR-06 | commitment copying | B copies A's commitment, reveals A's opening after A | B's reveal rejected | CRYPTO-007 |
| AT-BR-07 | faction impersonation | adversary builds `f_u` on the opposite side over `n_min` sincere ratings, then boosts | cost curve reported (this cannot be prevented; must be quantified) | §11.3 |
| AT-DIF-01 | FP rate | `n_biased = 0`, NT ∈ {1500, 3000}, K ∈ {4, 8, 16}, ≥ 200 seeds | FP per item ≤ documented α | DIF-008 |
| AT-DIF-02 | power surface | `δ ∈ {0.3, 0.5, 0.7, 0.9}`, `n_biased ∈ {1,2,3}`, `π ∈ {0.5, 0.3, 0.1}` | sensitivity table with CI; docs' "1500/3000" replaced by the table | STAT-001 |
| AT-DIF-03 | non-uniform DIF | class-specific `a_j` | detected or documented as out of scope | DIF-004 |
| AT-DIF-04 | two axes | biased items split across two independent hidden axes | detected or documented | DIF-004 |
| AT-DIF-05 | metric consistency | same data through docs' `2|δ|`, sim's 0.35, code's 0.5 | one rule | DIF-006 |
| AT-DIF-06 | separation | item perfectly predicted by θ | fit reports separation; verdict "undetermined" | §6.3 |
| AT-DIF-07 | sample poisoning | `c` coordinated respondents (c/NT ∈ {1,2,5,10 %}) answering to mask a real DIF / to create DIF on a clean item | fraction needed reported | §11.4 |
| AT-DIF-08 | purification oscillation | adversarial batch constructed so flags alternate | non-convergence signalled | DIF-007 |
| AT-DIF-09 | pool-scale mixture | K = 100, NT = 3000 | completes within a stated budget | DIF-009 |
| AT-DIF-10 ✓ | single item invisible | 1/8 | (documentation claim, not a guard) | DIF-005 |
| AT-NET-01 | consistent rewrite | rewrite entries `i..`, recompute hashes | detected against a stored prior head | NET-004 |
| AT-NET-02 | leaf duplication | `[x,y,z]` vs `[x,y,z,z]` | distinct roots | NET-003 |
| AT-NET-03 | checkpoint replay | old valid checkpoint to a client at height `h' < h` | ignored | NET-006 |
| AT-NET-04 | equivocation | two cps, same height, `≥ t` sigs | fork alarm + evidence | NET-006 |
| AT-NET-05 | cross-network replay | cp signed for net A presented on net B | rejected | NET-006 |
| AT-NET-06 | corrupted shard | flip bytes in one shard, decode | detected before/at decode | NET-007 |
| AT-NET-07 | hostile `.ots` | fuzz `verify` | no panic, bounded time | NET-008 |
| AT-NET-08 ✓ | threshold counting | 2-of-5, duplicates | rejected | NET-005 |
| AT-PRO-01 | unproven nym | submit a review with a random 32-byte `Nym` | rejected | PROTO-007 |
| AT-PRO-02 | batch of one | `stage2` / mixture with 1 item | rejected | INV-8 |
| AT-PRO-03 | supplementary review | band item | defined outcome | PROTO-008 |
| AT-PRO-04 | Draft CID ambiguity | `("ab","c")` vs `("a","bc")` | distinct CIDs | PROTO-011 |
| AT-PRO-05 | honeypot self-review | committee member assigned its own golden item | excluded or accepted-and-documented | PROTO-009 |
| AT-PRO-06 | oracle version pin | regenerate fixtures under pinned numpy/scipy in CI | matches | REPRO-003/004 |

---

## 13. Verification architecture

The tree below realizes `docs/07` §24. Existing tests are listed where they should move; new tests reference §12.

```
verification/
├── invariants/                 INV-1…INV-14 as executable checks where possible
│   ├── inv8_batch_min.rs       (AT-PRO-02)   inv9_nym_proof.rs (AT-PRO-01)   inv12_commit_binding.rs (AT-BR-06)
│   ├── inv13_canonical_input.rs (AT-BR-03)   inv14_discount_monotone.rs (AT-COL-04)
├── known_answers/
│   ├── fixtures/               = crates/scoring/tests/fixtures, plus PROVENANCE.md (numpy/scipy/OS versions, seeds)
│   ├── level_a.rs level_b.rs level_c.rs   (existing; tolerances justified per REPRO-003)
│   └── verdict_agreement.rs    per-item pass/fail agreement with the sim, or a documented list of divergences
├── property_tests/             existing network/protocol proptests +
│   ├── merkle_leaf_count.rs (AT-NET-02)   log_consistency.rs (AT-NET-01)   checkpoint_replay.rs (AT-NET-03..05)
│   ├── bss_bounds.rs (AT-REP-03)          quotas.rs (existing)
├── metamorphic/
│   ├── bridging_permutation.rs (AT-BR-03)   bridging_sign_flip.rs (f → −f gives same b_j)
│   ├── dif_group_relabel.rs    (g → −g gives −β₂; classes swap gives same |δ|)
│   └── scale_invariance.rs     (θ standardized ⇒ a, b transform predictably)
├── differential/
│   ├── rust_vs_python.rs       (existing oracle tests, re-homed)
│   ├── logistic_vs_statsmodels.rs   (β with SE; separation cases)
│   └── crypto_vs_reference.rs  (VOPRF against RFC 9497 test vectors; BBS+ against the library's vectors)
├── adversarial/                §12 AT-ID, AT-REP, AT-COL, AT-BR, AT-DIF-07, AT-NET, AT-PRO
├── simulations/
│   ├── bridging_sweeps.py      (BRIDGE-002/003/005 characterization)
│   ├── dif_power.py            (AT-DIF-01..04; replaces power.rs's 5 seeds)
│   ├── collusion_regimes.py    (AT-COL-02/03/05)
│   ├── sample_poisoning.py     (AT-DIF-07)
│   └── reports/                CSV + CI summaries, versioned with the fixture provenance
├── reproducibility/
│   ├── cross_platform.yml      (AT-BR-04: three runners, compare `to_bits`)
│   └── fixture_drift.yml       (AT-PRO-06: pinned Python env, run on every push)
└── reports/
    └── 09-verification-matrix.md   (§15, regenerated from test metadata)
```

CI MUST run everything except `simulations/` on every push, and `simulations/` + `cross_platform` on a schedule, with results committed to `reports/`.

Test authorship rules (from `docs/07` §11): each property test states *why* the property holds; a test that encodes a limitation (DIF-005) is labelled `documents_limitation` and not counted as a guarantee; tolerances cite the analysis that justifies them.

---

## 14. Critical gaps and ambiguities

Only gaps supported by evidence in the repository are listed. Each gives: location, the ambiguity, why it matters, the minimum specification that removes it, and the test that validates the fix.

**G-01 — DIF Variant 1 has no anonymity-compatible input.**
*Location.* `docs/02` §B.3 ("uses the Level A latent axis … `f_i`"), `crates/scoring/src/dif.rs::logistic_dif`, `crates/protocol/src/pilot.rs::stage2_dif`, `crates/protocol/src/revalidation.rs::revalidate_pool`, fixtures `levelb_grp.csv`.
*Ambiguity.* `f` is estimated per judge nym; respondents act under an unlinkable respond nym. The docs never say where a respondent's `f_i` comes from.
*Why it matters.* Pilot 2 as implemented (`stage2_dif`) and the "1500 with a group signal" sample size rest on a variable the system cannot possess without breaking P3 or INV-1.
*Minimum spec.* State explicitly: "In production, Level B DIF is Variant 2 only. Variant 1 is permitted only in attributed calibration pilots (`docs/README` step 2). Pilot 2 sample size is the Variant-2 size." Or: specify a linkage-free source of `f_i` (none is known to the auditor).
*Test.* AT-PRO-02 extended: the production pipeline MUST NOT accept a `group` vector; `stage2_dif` MUST be gated behind a `calibration` feature flag.

**G-02 — OPRF input is not bound to the authenticated anchor; label authenticity and registry custody are unspecified.**
*Location.* `docs/03` §M1 (contradictory sentences on who learns the label), `crates/identity/src/enrollment.rs::{UniquenessOracle, EnrollmentRegistry::enroll}`, `oprf.rs::label_with_quorum`.
*Why.* Without binding, Sybil resistance (pillar 1 of `docs/00`) is not provided by the cryptography; without custody rules, the registry is a deanonymization oracle.
*Minimum spec.* §9.3 E1–E5 with a chosen binding mechanism; a statement of who stores labels, who verifies the derivation, and that the label is never disclosed after issuance.
*Test.* AT-ID-01, AT-ID-02, AT-ID-03.

**G-03 — Reputation weights and collusion discounts are not consumed by bridging.**
*Location.* `crates/scoring/src/bridging.rs::fit` (no weights), `ARCHITECTURE.md` §Future work.
*Why.* Every claim of the form "E_u weights the vote", "500 coordinated count as 22", "probation = weight 0" is currently about numbers nobody reads. `README.md` calls the engine "Complete".
*Minimum spec.* Weighted objective `Σ w_u (r_uj − r̂_uj)²` with `w_u = discount(cap(E_u))` recomputed per epoch from the *previous* epoch's outcomes (state the lag), plus the honeypot contribution rule.
*Test.* AT-COL-06.
*Status.* RESOLVED (T5) — `bridging::fit` consumes `Ratings.weights`; AT-COL-06 passes. The per-epoch, previous-epoch-lag recomputation belongs to the real orchestrator (T12).

**G-04 — Protocol accepts unproven pseudonyms and no rate limit is enforced.**
*Location.* `crates/protocol/src/{review,probation}.rs` (`Nym`), `crates/identity/src/{nym,ratelimit}.rs`, `identity/src/lib.rs` ("unifying … future work").
*Minimum spec.* INV-9; every protocol entry point takes `&NullifierProof` and a verifier context; RLN with Shamir-share slashing or a ZK range proof on the slot.
*Test.* AT-PRO-01, AT-ID-05, AT-ID-06.

**G-05 — Randomness source for lottery / assignment / honeypot / sortition.**
*Location.* `lottery.rs`, `review.rs`, `honeypot.rs`, `governance.rs`, `blueprint.rs` (all take a `u64` seed; tests pass constants).
*Minimum spec.* INV-10 with an exact derivation and publisher.
*Test.* AT-BR-05.

**G-06 — Commit–reveal binds neither committer nor item.**
*Location.* `review.rs::commit`.
*Minimum spec.* INV-12.
*Test.* AT-BR-06.

**G-07 — θ metric and IRT thresholds.**
*Location.* `irt.rs` (`theta_from_anchors`, `A_MIN`, unused `B_ABS_MAX`), `docs/02` §B.2 table, `end_to_end.rs:175-179` rationale for item 02.
*Ambiguity.* Whether `a ≥ 0.6`, `|b| ≤ 2.5` refer to the IRT metric or the standardized-total metric.
*Minimum spec.* Declare the metric; either re-derive thresholds for it or estimate an IRT-scaled θ. Decide 3PL.
*Test.* AT-DIF-02 extended with a 3PL generator; verdict_agreement.rs.

**G-08 — Mixture DIF cut-off inconsistent (docs 0.5 on b-gap; sim 0.35 on |δ|; code 0.5 on |δ|).**
*Location.* `docs/02` §B.3, `sim/latent_dif_and_capacity.py:63`, `dif.rs::MIXTURE_DIF_MAX`, `revalidation.rs:58`.
*Minimum spec.* One metric, one value, chosen from AT-DIF-01/02.
*Test.* AT-DIF-05.

**G-09 — BSS baseline (crowd `p̄_j` vs outcome base rate).**
*Location.* `docs/02` §C.2, `reputation.rs::base_rate_baseline`, `sim/bridging_irt_dif.py` (`base = out.mean()`), `levelc_bss.csv`.
*Minimum spec.* Choose; if base rate, re-derive the "correct dissenter is rewarded" argument and specify how `E_u` is computed *before* an epoch's outcomes are known (the base rate is hindsight).
*Test.* AT-REP-02, AT-REP-03.
*Status.* RESOLVED (T31) — chose the crowd baseline (D23): `reputation::crowd_baseline`, and the protocol's E_u (`honeypot::reviewer_skills`) normalizes BSS against `p̄_j`; AT-REP-02 passes. The sim's reference `levelc_bss` still uses the base rate (it reproduces the BSS *function*, not the E_u policy).

**G-10 — Oracle precision, acceptance tolerance, and verdict divergence.**
*Location.* `level_a.rs` (tol 0.03), `fixture_drift.rs` (tol 1e-4, ignored), `end_to_end.rs::EXPECTED_POOL`, auditor regeneration (drift ≤ 1e-3).
*Minimum spec.* Record fixture provenance; pin the Python environment in CI; state that verdict agreement is *not* claimed near τ, or tighten the optimizer tolerances on both sides until it can be.
*Test.* AT-PRO-06, verdict_agreement.rs.

**G-11 — "Same input" is undefined; permutation and platform sensitivity untested.**
*Location.* `bridging.rs` (accumulation order), `level_a.rs:152-165` (HashSet order), `reproducibility.rs` (same process).
*Minimum spec.* INV-13; canonical serialization (`obs` sorted by `(u, j)`, `f64` as IEEE-754 LE bits, CSV/CBOR schema versioned).
*Test.* AT-BR-03, AT-BR-04.

**G-12 — `w_max = 3·median` cannot bind on `E_u ∈ (0,1)` when the median ≥ 1/3.**
*Location.* `reputation.rs::{weight_cap, capped_weight}`, tests using 2.0 and 5.0.
*Minimum spec.* Define the weight scale (is `w = E_u`, or `w = E_u / median(E)`?) so the cap is meaningful.
*Test.* AT-REP-04.

**G-13 — Sub-unit weights are boosted by the discount; sparse data unsupported; jitter evades clustering.**
*Location.* `collusion.rs`.
*Minimum spec.* INV-14; a sparse-aware coordination statistic; a threshold chosen from a FP/FN study.
*Test.* AT-COL-02..05.

**G-14 — Log is unsigned and detects only inconsistent edits; Merkle root does not commit to leaf count; checkpoints lack network id and replay/equivocation handling.**
*Location.* `log.rs`, `merkle.rs`, `consortium.rs`, `docs/04` ("signed append-only logs", "altering one breaks the chain visibly").
*Minimum spec.* §9.4, §10.1–10.2.
*Test.* AT-NET-01..05.

**G-15 — Supplementary review, appeal escrow, non-reveal handling, minimum batch, sample-size gating are unspecified.**
*Location.* `gate.rs` (`SupplementaryReview` label only; `settle_appeal`), `end_to_end.rs:129` ("assumed to pass"), `pilot.rs` (no N or K checks).
*Minimum spec.* §9.1 rows for `SupplementaryReview`, `Revealing` deadline, `Pilot1/2` preconditions.
*Test.* AT-PRO-02, AT-PRO-03.

**G-16 — Honeypot committee self-dealing and golden-item ground truth.**
*Location.* `honeypot.rs`, `docs/05` §Golden items.
*Minimum spec.* Golden "known quality" MUST come from Level-B history (retired validated items and items that failed Level B), not from committee opinion; committee members MUST be excluded from panels containing their golden items (requires a linkage the design forbids — state the residual risk).
*Test.* AT-PRO-05.

**G-17 — Key lifecycle (OPRF, issuer, consortium) and credential revocation.**
*Location.* nowhere (absent from docs and code).
*Minimum spec.* INV-11; re-issuance preserving `x`; consortium member-set changes in the checkpoint message; a revocation list keyed on nullifiers (with the anonymity cost stated).
*Test.* AT-ID-05 extension; AT-NET-05.

**G-18 — Draft CID concatenation without length prefix.**
*Location.* `deposit.rs::Draft::content_id`.
*Minimum spec.* Length-prefix all fields (as `Template::variant` does).
*Test.* AT-PRO-04.

**G-19 — Sortition candidates carry `f_u`; acting roles link pseudonyms.**
*Location.* `governance.rs::Candidate`, `docs/05` §Meta-level governance.
*Minimum spec.* State under which pseudonym a drawn member acts and accept/mitigate the linkage; or draw from a separate "governance" nym with its own `f` estimate.
*Test.* none automatable; design review.

**G-20 — Reproducibility requires publishing all ratings; docs forbid a public register of voting patterns.**
*Location.* `docs/04`, `docs/CLAUDE.md` "What NOT to do", INV-7.
*Minimum spec.* Choose an option from PRIV-004 and record it in `docs/01` as a decision.
*Test.* none; design decision.

---

## 15. Claim / evidence matrix

Status is the lowest justified. "Missing evidence" names what would raise it one level.

| ID | Claim | Evidence (files) | Current status | Missing evidence | Required action |
|---|---|---|---|---|---|
| REPRO-001 | bit-for-bit within platform | `scoring/tests/reproducibility.rs` | TESTED (one process) | cross-platform run; release-profile run | AT-BR-04 |
| REPRO-002 | order-independent input | `bridging.rs` (`Ratings::canonical`), `canonical_input.rs` | TESTED (T3) — canonical `(u,j)` order; a permutation gives bit-equal `b_j` (AT-BR-03) | — | INV-13 |
| REPRO-003 | engine = sims | `level_{a,b,c}.rs`, fixtures; `fixture_drift.rs` **fails** under numpy 2.4.4/scipy 1.17.1 (§0-ter, `fixtures/PROVENANCE.md`) | TESTED (statistic level); REPRODUCED: NO (repository guard fails) | pinned-env regeneration; verdict agreement | G-10, T4 |
| REPRO-004 | fixture provenance | `sim/export_fixtures.py` | IMPLEMENTED | versions recorded, CI regen | AT-PRO-06 |
| BRIDGE-001 | model = spec (d=1) | `bridging.rs`, `level_a.rs` | TESTED | d=2, n_min | implement or descope in docs |
| BRIDGE-002 | axis recovery | `level_a.rs`, sim | TESTED (1 seed) | seed/parameter sweeps | bridging_sweeps.py |
| BRIDGE-003 | polarized items rejected | `level_a.rs` | TESTED (1 dataset) | calibration procedure for τ, λ | docs/07 §13 |
| BRIDGE-004 | bootstrap-min pessimistic & stable | `level_a.rs` (tautological) | IMPLEMENTED | warm vs cold comparison | new test |
| BRIDGE-005 | capture cost ≈ 87 % | `level_a.rs` (monotone only), sim | TESTED (qualitative) | crossing distribution | AT-BR-02; fix docs/06 figure |
| BRIDGE-006 | band → supplementary review | `gate.rs` (`bridging_gate`, `supplementary_review`) | IMPLEMENTED + semantics defined (T10/T30): D26 re-decision | — | G-15 |
| BRIDGE-007 | weights consumed | `bridging.rs` (`Ratings.weights`), `anti_collusion.rs` (AT-COL-06), `orchestrator.rs` (`bridging_weights`/`weighted_ratings`, `orchestrator_driver.rs`) | IMPLEMENTED (T5) — weighted objective `Σ w_u (r−r̂)²`; prior-epoch standing → `w_u` is computed by the orchestrator and consumed in `run_epoch`; a discounted cartel moves `b_j` less than the same number of independents | — | G-03 |
| OPT-001 | convergence observable | `optim.rs`, `glm.rs` (+ tests) | IMPLEMENTED (T2) — `lbfgs`/`fit_logistic` return status; separation detected | — | — |
| IRT-001 | θ proxy | `irt.rs`, `level_b.rs` | TESTED | metric declaration | G-07 |
| IRT-002 | inverted key caught | `level_b.rs` | TESTED | partial-key cases | AT-DIF-02 ext. |
| IRT-003 | 2PL screen | `level_b.rs`, `end_to_end.rs` | TESTED (2 items) | threshold validity; 3PL | G-07 |
| DIF-001 | logistic numerics | `level_b.rs` | TESTED | SE/LRT/multiplicity | §6.5 |
| DIF-002 | Variant 1 admissible input | `dif.rs`, `pilot.rs` (`calibration` feature) | RESOLVED (T32) — Variant 1 is calibration-only; production compiles no per-respondent `group` (D20) | — | G-01 |
| DIF-003 | MH classification | `level_b.rs` (incl. NaN at n = 200) | TESTED (2 items); NaN policy RESOLVED via `total_cmp` (§0-ter) | significance; tertile spec | §6.5 |
| DIF-004 | mixture detects ≥2/8 @3000 | `level_b.rs`, `end_to_end.rs`, sim (auditor re-run) | TESTED, REPRODUCED (sim) | FP rate, power surface | AT-DIF-01/02 |
| DIF-005 | 1/8 invisible | `level_b.rs`, sim | TESTED (limitation) | — | relabel as documentation |
| DIF-006 | threshold consistent | — | INCONSISTENT | — | G-08 |
| DIF-007 | purification fixed point | `level_b.rs` | TESTED (1 dataset) | convergence signalling | AT-DIF-08 |
| DIF-008 | FP/FN characterized | — | NOT ESTABLISHED | simulations | AT-DIF-01..04 |
| DIF-009 | pool-scale feasibility | — | NOT ESTABLISHED | analytic gradient, benchmark | AT-DIF-09 |
| STAT-001 | 300/1500/3000 adequate | `power.rs` (ignored, 5 seeds) | HYPOTHESIS | power study | AT-DIF-02 |
| REPUTATION-001 | author score | `level_c.rs` | TESTED | definition of q_j | docs |
| REPUTATION-002 | BSS = oracle | `level_c.rs` | TESTED | — | — |
| REPUTATION-003 | consensus ≈ 0 | `reputation::crowd_baseline`; `level_c.rs` (AT-REP-02) | RESOLVED (T31) — E_u normalizes BSS against the crowd baseline `p̄_j` (D23); a consensus follower scores BSS 0. Sim's `levelc_bss` reference still base-rate | sim BSS → crowd | G-09 |
| REPUTATION-004 | asymmetry deters long-con | `scoring/tests/adversarial.rs` | IMPLEMENTED; claim HYPOTHESIS | rates; game analysis | AT-REP-01 |
| REPUTATION-005 | cap limits a node | `level_c.rs` (synthetic weights) | IMPLEMENTED (vacuous) | — | G-12 |
| REPUTATION-006 | probation | `probation.rs`, `orchestrator.rs` (`bridging_weights`) | WIRED (T5) — probation → `w_u = 0`, so a probationer's ratings do not move `b_j`; `orchestrator_driver.rs` | — | G-03 |
| REPUTATION-007 | appeal stake coherent | `lifecycle.rs` | INCONSISTENT | — | redefine as pseudo-observation |
| COLLUSION-001 | identical cartel → √k | `anti_collusion.rs`, `adversarial.rs` | TESTED (identical, dense, unit) | — | — |
| COLLUSION-002 | jittered cartel detected | auditor probe (fails at σ=0.05) | UNSOLVED | robust statistic | AT-COL-02 |
| COLLUSION-003 | sparse data | — | NOT IMPLEMENTED | — | AT-COL-03 |
| COLLUSION-004 | discount never boosts | auditor probe (0.25→0.5); `anti_collusion.rs` (AT-COL-04) | RESOLVED @289aae3 (was INV-14 VIOLATED) — see §0-bis | — | — |
| COLLUSION-005 | chaining/griefing | — | UNSOLVED | analysis | AT-COL-05 |
| ID-001 | dedup same anchor | `identity/tests/*` | TESTED (registry logic) | authenticated anchor | G-02 |
| ID-002 | obliviousness | `voprf_oracle.rs` (primitive) | primitive TESTED; interface NOT IMPLEMENTED | split API | G-02 |
| ID-003 | t−1 cannot compute | `oprf.rs` tests (incl. AT-ID-04) | TESTED (functional, in-process); duplicate-index guard RESOLVED @289aae3 | DKG, transport, external review | §7.4 |
| ID-004 | input bound to eID | — | NOT ESTABLISHED (design) | — | G-02 |
| ID-005 | label authenticity/custody | — | CONTRADICTORY / NOT IMPLEMENTED | — | G-02 |
| ID-006 | key lifecycle | — | NOT ESTABLISHED | — | G-17 |
| ID-007 | no whitewashing | `credential.rs` (`IssuanceRegistry`, `issue_once`), `id007_one_credential.rs` | ENFORCED (T11) — one credential per label, `AlreadyIssued` on a repeat; AT-ID-02/03 pass | trusted-registry-free binding (ID-004/005, T20) | AT-ID-02/03 |
| ID-008 | rate limit enforced | `admission.rs` (`QuotaLedger`), `deposit.rs`, `id008_proposal_quota.rs` | ENFORCED (T11, structural) — per-credential epoch quota keyed on the proven id; over quota → `OverQuota` | ZK/RLN cryptographic-grade (T20) | AT-ID-06 |
| CRYPTO-001 | VOPRF RFC 9497 | `voprf_oracle.rs` | TESTED | RFC test vectors | differential/ |
| CRYPTO-003 | BBS+ blind issuance | `bbs_credential.rs`, unit tests | TESTED | per-label limit; external review | §7.4 |
| CRYPTO-004 | threshold BBS+ | `threshold_bbs.rs` | TESTED (in-process) | independent base-OT seed; DKG; review | §7.4 |
| CRYPTO-005 | nullifier bound to credential | `nullifier.rs`, `tests/nullifier.rs`, `protocol/tests/inv9_nym_proof.rs` | TESTED; message binding now present (T6) — `prove`/`verify` take an action `context` folded into the Fiat–Shamir challenge, so a proof does not verify under another context (AT-ID-05) | external review (§7.4) | AT-ID-05; §7.4 |
| CRYPTO-006 | cross-role unlinkability | — | HYPOTHESIS (SXDH) | name the assumption | docs |
| CRYPTO-007 | commit binding | `review.rs` (`commit`/`reveal`), `lifecycle.rs` (item in `Revealing`), `inv12_commit_binding.rs` | RESOLVED@T7 — binds committer + item; a copied commitment does not open (AT-BR-06) | — | INV-12 |
| CRYPTO-008 | randomness source | `randomness.rs` (`Beacon`), `_from_beacon` wrappers, `inv10_checkpoint_seed.rs` | RESOLVED@T8 — every draw seeds from the signed checkpoint head; assignment keys on a byte-independent slot (AT-BR-05) | checkpoint publisher/timing at epoch close (T13–T18) | INV-10 |
| PRIV-001 | role nyms unlinkable | inequality tests | HYPOTHESIS | secret entropy rule | docs |
| PRIV-002 | issuer unlinkability | — | HYPOTHESIS (docs contradict) | "label never revealed" | docs |
| PRIV-003 | stat. deanonymization mitigations | — | NOT IMPLEMENTED | — | roadmap |
| PRIV-004 | repro vs secrecy | — | UNRESOLVED | decision | G-20 |
| PRIV-005 | small-crowd bound | — | HYPOTHESIS | model | research |
| NET-001 | CID | `integrity.rs` | TESTED; Draft prefix fix RESOLVED @289aae3 (PROTO-011) | — | — |
| NET-002 | Merkle inclusion | proptest | TESTED | — | — |
| NET-003 | root commits to leaves | auditor probe (collision); `integrity.rs` (AT-NET-02) | RESOLVED @289aae3 (was DEFECT) — RFC 6962, see §0-bis | — | — |
| NET-004 | log tamper-evident | `log.rs` (`checkpoint`, `verify_extends`), `log_consistency.rs` | RESOLVED@T14 — consistency proof + truncation detection against a consortium-signed prior head (AT-NET-01) | Merkle-style compact consistency proof for light clients (re-download-free) | G-14 |
| NET-005 | checkpoint threshold | `integrity.rs` | TESTED | — | — |
| NET-006 | replay/equivocation/net id | `consortium.rs` (`Checkpoint` v2, `CheckpointClient`, `member_set_hash`), `checkpoint_replay.rs` | RESOLVED@T15 — net/member-set binding in the signed message; client monotonic-height rule; same-height equivocation evidence (AT-NET-03..05) | member-set rotation (T22); higher-height fork via `verify_extends` | §9.4 |
| NET-007 | erasure | proptest | TESTED | shard auth | AT-NET-06 |
| NET-008 | OTS verify | `anchoring.rs`, `integrity.rs` | TESTED (format) | fuzz | AT-NET-07 |
| NET-009 | anchoring liveness | — | NOT IMPLEMENTED | — | roadmap |
| NET-010 | transport/CRDT | — | HYPOTHESIS | — | roadmap |
| PROTO-001 | deposit needs source | `lifecycle.rs` | TESTED | structured citation | docs/03 |
| PROTO-002 | lottery | `lifecycle.rs`, proptest | TESTED | seed source | G-05 |
| PROTO-003 | stratified assignment | `lifecycle.rs` | TESTED | new-reviewer path | docs |
| PROTO-004 | gate + appeal | `lifecycle.rs`, `end_to_end.rs` | TESTED | escrow semantics | G-15 |
| PROTO-005 | pilot stages | `pilot.rs`, `lifecycle.rs`, `end_to_end.rs`, `inv8_batch_min.rs` | TESTED; N/K gating enforced (T9) | — | G-15 |
| PROTO-006 | batch enforced | `pilot.rs` (`admit_dif_batch`, `screen`, `dif_batch`), `revalidation.rs` (`revalidate_batch_latent`), `lifecycle.rs`, `inv8_batch_min.rs` | ENFORCED (T9) — the DIF gates reject a batch < `K_MIN` items and a sample below its §B.6 floor; `run_epoch` runs the pilot through them; the state machine also rejects `Pilot2Batch` of one (T12) | — | AT-PRO-02 |
| PROTO-007 | nym proof verified | `admission.rs`, `deposit.rs`/`review.rs` (entry points), `inv9_nym_proof.rs` | IMPLEMENTED (T6) — entry points verify a role `NullifierProof` and key on `NullifierProof::id()`; AT-PRO-01/AT-ID-05 pass | cryptographic-grade enrollment/replay (T20), per-credential quota (T11), external review of the nullifier (§7.4) | G-04 |
| PROTO-008 | supplementary review | `gate.rs` (`supplementary_review`), `lifecycle.rs` (`Resolve`), `supplementary_redecision.rs` | RESOLVED@T10/T30 — D26 re-decision (re-run bridging, `b_j` vs τ); defined terminal | — | G-15 |
| PROTO-009 | honeypot | `lifecycle.rs` | TESTED (mechanics) | ground truth; self-review | G-16 |
| PROTO-010 | governance | `lifecycle.rs`, proptest | TESTED (mechanics) | acting-role linkage | G-19 |
| PROTO-011 | Draft CID unambiguous | `lifecycle.rs` (AT-PRO-04) | RESOLVED @289aae3 (was DEFECT) — see §0-bis | — | — |
| PROTO-012 | band resolution is a bridging decision (INV-2/D2/D26) | `gate.rs` (`supplementary_review`), `supplementary_redecision.rs` | RESOLVED@T30 — the weighted-mean tie-break (`aggregate` module + `review_aggregation.rs`/`composed_gate.rs`) is deleted; the band is re-decided by re-running bridging vs the plain threshold, so a polarized panel is not carried by the larger camp | — | D26, D2 |

No claim in this matrix is at INDEPENDENTLY_REVIEWED, SCIENTIFICALLY_CHARACTERIZED, PRODUCTION_CANDIDATE, or PRODUCTION_READY. The auditor's re-execution of the Python simulations counts as REPRODUCED for DIF-004 and BRIDGE-002 *at the simulation level only*, and explicitly *fails* REPRODUCED for REPRO-003.

---

## 16. Acceptance criteria (completion gate)

Completion is **not** "all TODOs removed". Isegoria MAY claim completeness only when every critical claim has proportionate evidence, every threat assumption is explicit, every unimplemented production dependency is named, and every residual risk is documented. The gate is split by discipline; each criterion names the minimum evidence level.

### 16.1 Scientific correctness
- SC-1 Every threshold in `docs/02`'s parameter table has: reason, calibration procedure, sensitivity analysis, failure description, versioned location (`docs/07` §13). *Today: none has all five.*
- SC-2 DIF Variant 2 has FP and power tables (AT-DIF-01/02) with CIs, covering unbalanced classes, δ ≥ 0.5, K ∈ {4..16}, NT ∈ {1500, 3000, 6000}; verdict thresholds are chosen from those tables. Status target: SCIENTIFICALLY_CHARACTERIZED.
- SC-3 Bridging: seed and parameter sweeps (BRIDGE-002/003/005) with reported variance; capture-cost curves replace the single "87 %" figure.
- SC-4 The θ metric is declared and thresholds re-derived (G-07); 3PL decision documented.
- SC-5 Sample sizes (STAT-001) derived from SC-2, not from prose.
- SC-6 The mixture metric is unified (G-08); BSS baseline chosen and the incentive argument re-derived (G-09).
- SC-7 Sample-poisoning curves exist (AT-DIF-07).
- SC-8 External psychometric review of §6.4–6.6 by a qualified reviewer, recorded in `reports/`.

### 16.2 Cryptographic security
- CS-1 The enrollment protocol (§9.3) is fully specified including ID-004 binding and ID-005 custody, and implemented with client/server message types (no cleartext anchor crosses to the key holder).
- CS-2 Both committees have a real DKG and transport; the trusted dealer is gone; base-OT randomness independent of keys.
- CS-3 Nullifier proofs bind the action message; one credential per label enforced; duplicate-index guard in `combine`.
- CS-4 Key lifecycle documented (INV-11) and revocation decision recorded.
- CS-5 External cryptographic review of §7.4 items, recorded.
- CS-6 RFC 9497 and BBS+ test vectors pass (differential/).

### 16.3 Privacy
- PV-1 Every PRIV-P property names its adversary and its evidence; PRIV-P6 (G-20) has a recorded decision.
- PV-2 The label is provably never disclosed after issuance (code + docs agree).
- PV-3 Statistical-deanonymization mitigations (PRIV-003) are implemented or explicitly deferred with the population floor stated as a deployment precondition.
- PV-4 No `Debug` derivation prints secrets or labels.

### 16.4 Protocol correctness
- PC-1 A state machine (§9.1) exists in code with rejection of every "invalid case" row; INV-8, INV-9, INV-10, INV-12 enforced.
- PC-2 Supplementary review, appeal escrow, non-reveal, and batch/sample gating specified and tested (G-15).
- PC-3 Weights consumed by bridging (G-03) with the lag rule stated.

### 16.5 Distributed-systems correctness
- DS-1 Log: signatures, consistency proofs, truncation detection (AT-NET-01).
- DS-2 Merkle: leaf-count-committing construction (AT-NET-02).
- DS-3 Checkpoints: network id, member-set hash, client monotonicity, equivocation evidence (AT-NET-03..05).
- DS-4 Erasure: shard authentication (AT-NET-06).
- DS-5 Anchoring: live calendar + SPV block source, or explicitly deferred; checkpoint→anchor linkage.
- DS-6 Transport/CRDT: convergence invariant stated (§10.3) before implementation begins.

### 16.6 Implementation quality
- IQ-1 `lbfgs` and `fit_logistic` return status; separation detected.
- IQ-2 No `partial_cmp().unwrap()` on caller data; NaN policy stated.
- IQ-3 `Draft::content_id` length-prefixed; `discount_weights` monotone; `brier_skill_score` guarded.
- IQ-4 Clippy `-D warnings` and fmt already enforced (CI) — retain.
- IQ-5 Coverage figure ("~97 %") reported with the command that produced it and its date.

### 16.7 Reproducibility
- RP-1 Canonical input serialization (INV-13) and a published `input_hash → outputs` record per checkpoint.
- RP-2 Cross-platform bit-equality (AT-BR-04) or a documented tolerance.
- RP-3 Fixture provenance and pinned regeneration in CI (AT-PRO-06); oracle tolerance justified.
- RP-4 Release-profile reproducibility test (the current test runs in the test profile).

### 16.8 Operational readiness
- OR-1 Deployment preconditions: population floor, committee sizes and `t`, consortium composition rule, anchoring cadence.
- OR-2 Incident procedures: committee member compromise, signer compromise, holder secret loss.
- OR-3 The closed calibration pilot (`docs/README` step 2) executed with declared attributes, and its results used for SC-1.
- OR-4 A clear public statement of what the system does not prove (§18–19).

---

## 17. Unresolved questions

Each question preserves an ambiguity found in the repository and offers precise alternatives. None is answered here.

- **Q-1 (PRIV-004 / G-20).** Who may re-run the scoring computation? (a) anyone, with full ratings public per nym; (b) consortium members only, with light nodes verifying signatures; (c) anyone, from a zk proof of computation. (a) contradicts `docs/CLAUDE.md`; (b) weakens D16's "anyone redoes the math"; (c) is D14 step 4.
- **Q-2 (G-01).** Is Variant 1 DIF production or calibration-only? If production, name the source of `f_i`.
- **Q-3 (G-02).** Binding mechanism: IdP-signed commitment + ZK opening; IdP-side blinding; or committee-side evaluation on a cleartext anchor (giving up obliviousness against the committee). Each has a different trust table (§3.1).
- **Q-4 (G-09).** BSS baseline: crowd prediction or outcome base rate?
- **Q-5 (G-08).** Mixture DIF metric: `|δ|` or `2|δ|`; value?
- **Q-6 (G-07).** θ metric: standardized total, or IRT EAP? 2PL or 3PL?
- **Q-7 (G-15).** Supplementary review: (a) `k` more reviewers then re-gate at `τ` without band; (b) escalate to the pilot directly; (c) hold until next epoch.
- **Q-8 (REPUTATION-007).** Appeal cost representation: pseudo-observation with escrow, or a separate ledger outside `C_a`?
- **Q-9 (G-16).** Golden-item ground truth: Level-B history only, or committee judgment?
- **Q-10 (G-19).** Under which pseudonym does a sortition member act?
- **Q-11 (G-05).** Randomness: checkpoint-head-derived, VRF from consortium, or external beacon (drand)?
- **Q-12 (ID-006).** Is the OPRF key ever rotated? If compromise forces it, is the network re-enrolled from scratch?
- **Q-13 (G-17).** Revocation: none (accept permanent loss on secret compromise), or a nullifier blacklist (accept the linkability cost)?
- **Q-14 (docs/03 F2).** Foreign anchors: separate label space accepted as a known Sybil channel, or a cross-space dedup mechanism?
- **Q-15 (D15).** Same or distinct bodies for issuing committee and storage consortium?
- **Q-16 (BRIDGE-001).** Is `d = 2` required for the reference use case? The sims never exercise it.
- **Q-17 (docs/README §Status vs README.md §Status).** `docs/README.md` still says "No production code written yet" and "license to be decided"; `README.md` says the engine is complete and the license is EUPL-1.2. Which is authoritative? (Recommendation: delete the stale paragraph.)

---

## 18. Residual risks (accepted or currently unmitigated)

| Risk | Nature | Owner decision required |
|---|---|---|
| Issuing committee `≥ t` colludes: unlimited undetectable credentials | structural | accept; mitigate by committee composition (D16-style) — no technical fix in the design |
| Storage consortium `≥ t` colludes: signs any state | structural; docs propose "fork" | accept; define fork procedure and evidence format |
| True-but-divisive false negatives (L1) | structural; appeal mitigates at author cost | accept; quantify survival with and without appeal |
| Pool-level topic bias (L2) | structural; blueprint mitigates | accept; sortition randomness open |
| Elite-consensus blind spot (L3) | structural; Variant 2 mitigates in the tested regime only | accept with the FP/FN caveat |
| Competence construct choice (L4) | philosophical | accept |
| Faction impersonation over time (§11.3) | not analysed | quantify (AT-BR-07) |
| Sample poisoning by coordinated respondents (§11.4) | not analysed | quantify (AT-DIF-07) |
| Small-population anonymity loss | acknowledged, unquantified | accept with a floor |
| Foreign-anchor double enrollment (F2) | acknowledged | accept or design |
| Permanent identity loss on secret compromise | consequence of non-rotatability | decide (Q-13) |
| Oracle drift across SciPy versions | measurement | accept with pinned env |
| Bespoke crypto compositions unreviewed | until §7.4 | do not deploy before review |

---

## 19. Claims not established

The following cannot be established from the repository, from simulation, or from code alone. They are listed so that no reader mistakes their absence from the gap list for evidence.

- **Requires real-world pilots.** τ, ε, λ_b/λ_f, k, `A_MIN`, `BETA2_MAX`, mixture cut-off, `N_PROBATION`, `EXPOSURE_LIMIT = 2000`, `HONEYPOT_RATE = 0.05`, `(up, down)`, `γ` — every operational parameter. The ~30 % survival rate. The claim that human review is "almost uncorrelated" with empirical validity (D3) — observed in a simulation whose generator *defines* the two as independent for most items.
- **Requires population scale.** Level-A identifiability at design sparsity; anonymity floor; throughput; Variant-2 DIF at NT ≥ 3000 with real answer distributions (guessing, non-monotone items, speededness).
- **Requires external cryptographic review.** Threshold OPRF composition; nullifier–BBS+ AND-composition; threshold BBS+ setup; the enrollment binding protocol once designed.
- **Requires external psychometric validation.** The latent-class DIF procedure as a bias test (its FP/FN behaviour, its reliance on a single hidden dichotomy, and the anchor-purification assumptions); the use of a standardized-total proxy for θ in place of an IRT ability.
- **Requires institutional trust assumptions.** Committee and consortium composition; eID correctness; the claim that CIE-NFC verification narrows F1; freedom to fork as a deterrent.
- **Cannot be established from code alone.** That a non-convex factorization "recovers the ideological axis" of a real population rather than the dominant axis of rating variance, whatever it is; that `f_u`-stratified panels "mirror all positions" when `f_u` is itself an estimate with unquantified error; that reproducibility deters signer dishonesty when re-runners must hold data the design says must not be public.

---

## 20. Implementation mapping

| Concept | Spec section | Code | Tests | Sims |
|---|---|---|---|---|
| Bridging model, bootstrap-min | §6.1 | `crates/scoring/src/bridging.rs`: `Obs`, `Ratings`, `BridgingParams`, `Fit`, `fit`, `fit_with_init`, `bridge_scores`, `random_init`, `normal` | `scoring/tests/level_a.rs`, `reproducibility.rs` | `sim/bridging_irt_dif.py::fit`, `sim/export_fixtures.py` |
| Optimizer | §6.2 | `crates/scoring/src/optim.rs`: `lbfgs`, `numerical_gradient` | unit tests in file | SciPy L-BFGS-B |
| Logistic MLE | §6.3 | `crates/scoring/src/glm.rs`: `fit_logistic`, `sigmoid` | unit tests | `dif()` in sims |
| θ, `r_pbis`, 2PL | §6.4 | `crates/scoring/src/irt.rs`: `theta_from_anchors`, `standardize`, `point_biserial`, `fit_2pl_item`, `A_MIN`, `B_ABS_MAX`, `R_PBIS_MIN` | `level_b.rs` | `th`, `np.corrcoef` |
| DIF Variant 1, MH | §6.5 | `crates/scoring/src/dif.rs`: `logistic_dif`, `DifCoefs`, `mantel_haenszel`, `MhResult`, `EtsClass`, `BETA2_MAX`, `MH_DELTA_B/C` | `level_b.rs` | `dif()` |
| Purification | §6.5 | `crates/scoring/src/validation.rs`: `purify_theta`, `Purified` | `level_b.rs` | — |
| DIF Variant 2 | §6.6 | `dif.rs`: `mixture_nll`, `mixture_dif`, `MixtureDif`, `MIXTURE_DIF_MAX`, `logsumexp2` | `level_b.rs`, `reproducibility.rs`, `power.rs` (ignored) | `sim/latent_dif_and_capacity.py::run` |
| Reputation | §6.7 | `crates/scoring/src/reputation.rs`: `AuthorPrior`, `author_score`, `proposal_rate`, `brier_skill_score`, `base_rate_baseline`, `evaluator_score`, `asymmetric_ema`, `weight_cap`, `capped_weight`, `dasgupta_ghosh` | `level_c.rs`, `scoring/tests/adversarial.rs` | VALUTATORI block |
| Anti-collusion | §6.8 | `crates/scoring/src/collusion.rs`: `correlation_matrix`, `cluster_by_correlation`, `sublinear_group_weight`, `discount_weights`, `ALPHA` | `anti_collusion.rs`, `adversarial.rs` | — |
| Role nym | §7.1 | `crates/identity/src/nym.rs`: `Role`, `Nym`, `derive_nym`; `hash.rs::tagged` | `identity/tests/properties.rs` | — |
| Rate limit | §7.1, ID-008 | `crates/identity/src/ratelimit.rs`: `rln_token`, `within_quota`, `SlotLedger`, `DoubleSpend` | `properties.rs` | — |
| Enrollment, VOPRF | §7.1, ID-001/002 | `crates/identity/src/enrollment.rs`: `Anchor`, `IdentityDocument`, `Cie`, `Spid`, `Label`, `UniquenessOracle`, `ReferenceOracle`, `VoprfOracle`, `EnrollmentRegistry` | `properties.rs`, `voprf_oracle.rs` | — |
| Threshold OPRF | §7.1, ID-003 | `crates/identity/src/oprf.rs`: `KeyShare`, `PublicShare`, `DleqProof`, `PartialEval`, `ThresholdOprfOracle`, `hash_to_group`, `dleq_challenge`, `lagrange_at_zero`, `combine`, `finalize` | unit tests, `threshold_oprf.rs` | — |
| Credential | §7.1, CRYPTO-003/004 | `crates/identity/src/credential.rs`: `Credential`, `IssuanceRequest`, `PendingIssuance`, `BlindSignature`, `AnonymousCredential`, `Issuer`, `ThresholdIssuer`, `IssuerPublic`, `setup_base_ot`, `verify_request` | `bbs_credential.rs`, `threshold_bbs.rs`, unit tests | — |
| Nullifier | §7.1, CRYPTO-005/006 | `crates/identity/src/nullifier.rs`: `NullifierProof`, `prove`, `verify`, `context_generator` | `tests/nullifier.rs`, unit test | — |
| CID, Merkle, log | §10.1–10.2 | `crates/network/src/{cid,merkle,log}.rs`: `Cid`, `cid`, `leaf_hash`, `merkle_root`, `merkle_proof`, `verify_proof`, `TransparencyLog`, `Entry` | `integrity.rs`, `properties.rs` | — |
| Checkpoints | §9.4 | `crates/network/src/consortium.rs`: `Checkpoint`, `Member`, `Consortium` | `integrity.rs` | — |
| Erasure | §10.4 | `crates/network/src/erasure.rs`: `encode`, `reconstruct`, `Encoded` | `integrity.rs`, `properties.rs` | — |
| Anchoring | §10.5 | `crates/network/src/anchoring.rs`: `Anchor`, `OtsAnchor`, `Receipt`, `AnchorState` | `integrity.rs`, unit tests | — |
| Lifecycle stages | §9.1 | `crates/protocol/src/{deposit,lottery,review,gate,pilot,exposure,revalidation}.rs`; `lib.rs::Stage` | `lifecycle.rs`, `end_to_end.rs`, `properties.rs` | — |
| Honeypot, probation, blueprint, governance | §9.2, §6.9 | `crates/protocol/src/{honeypot,probation,blueprint,governance}.rs` | `lifecycle.rs`, `properties.rs` | — |
| Build/CI | §16.6–16.7 | `Cargo.toml` (release profile), `rust-toolchain.toml` (1.86.0), `.github/workflows/ci.yml` (fmt, clippy `-D warnings`, test, llvm-cov) | — | — |

Serialization formats: none defined for the engine input/output; fixtures are ad-hoc CSV (`%.10f` floats, `%d` ints, header rows on `expected_*`/`levelb_expected`/`levelc_bss`/`*_meta`); `.ots` is the only wire format; `Cid`, `Nym`, `Label`, `Token` are raw `[u8; 32]`.

---

## 21. References to repository files

- Design: `README.md`, `ARCHITECTURE.md`, `docs/README.md`, `docs/CLAUDE.md`, `docs/00-overview.md`, `docs/01-decisions.md`, `docs/02-scoring-engine.md`, `docs/03-identity-enrollment.md`, `docs/04-storage-network.md`, `docs/05-question-lifecycle.md`, `docs/06-threat-model.md`, `docs/07-verification-and-assurance.md`, `docs/99-glossary.md`.
- Simulations: `sim/README.md`, `sim/bridging_irt_dif.py`, `sim/latent_dif_and_capacity.py`, `sim/export_fixtures.py`.
- Scoring: `crates/scoring/{Cargo.toml, src/{lib,bridging,optim,glm,irt,dif,validation,reputation,collusion}.rs, tests/{level_a,level_b,level_c,adversarial,anti_collusion,reproducibility,power,fixture_drift}.rs, tests/fixtures/*.csv}`.
- Identity: `crates/identity/{Cargo.toml, src/{lib,hash,nym,ratelimit,enrollment,oprf,credential,nullifier}.rs, tests/{properties,voprf_oracle,threshold_oprf,bbs_credential,threshold_bbs,nullifier}.rs}`.
- Network: `crates/network/{Cargo.toml, src/{lib,hash,cid,merkle,log,consortium,erasure,anchoring}.rs, tests/{integrity,properties}.rs}`.
- Protocol: `crates/protocol/{Cargo.toml, src/{lib,deposit,lottery,review,gate,pilot,honeypot,probation,revalidation,exposure,blueprint,governance}.rs, tests/{lifecycle,end_to_end,adversarial,properties}.rs}`.
- Build: `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `.github/workflows/ci.yml`, `.gitignore`, `LICENSE`.
- External sources consulted by the auditor: `opentimestamps` 0.2.0 `src/timestamp.rs` (step-output recomputation on parse); RFC 9497 (VOPRF); ETS DIF classification (Zieky 1993 conventions as used in `docs/02`); RFC 6962 (Merkle construction recommended in NET-003).

*End of document.*
