# Architecture

How the design in [`docs/`](docs/) maps onto the code. This document is the bridge
between the conceptual specification and the Rust implementation. Read
[`README.md`](README.md) first for the plain-language overview.

## Contents

- [Principles](#principles)
- [Workspace and dependency graph](#workspace-and-dependency-graph)
- [`scoring` — the deterministic engine](#scoring--the-deterministic-engine)
- [`identity` — anonymous enrollment](#identity--anonymous-enrollment)
- [`network` — tamper-evident storage](#network--tamper-evident-storage)
- [`protocol` — lifecycle orchestration](#protocol--lifecycle-orchestration)
- [Invariants and where they are enforced](#invariants-and-where-they-are-enforced)
- [Reproducibility](#reproducibility)
- [Testing strategy](#testing-strategy)
- [Plug points: real vs. placeholder](#plug-points-real-vs-placeholder)
- [Future work](#future-work)

## Principles

Four rules shape every crate:

1. **The scoring engine runs offline.** `scoring` has no dependency on `identity`,
   `network`, or any I/O — the compiler enforces it. It is the piece that must be
   independently re-runnable to catch a dishonest signer.
2. **Determinism is a security property, not an optimization.** Given identical
   input, the engine produces identical output, bit-for-bit (pinned toolchain,
   seeded RNGs, fixed iteration order, one pure-Rust implementation of the
   transcendental functions). See [Reproducibility](#reproducibility).
3. **Prefer mature crypto; roll our own only with a high bar.** Heavy primitives
   enter behind traits; production wires them to mature, audited libraries. Bespoke
   cryptography is allowed when it genuinely serves the design (no suitable library,
   or a needed variant such as a threshold scheme built on a vetted single-party one),
   kept small, built on audited building blocks, and pinned down with known-answer and
   property tests. It is a considered exception, not the default.
4. **The docs are the source of truth for the logic.** Code comments are minimal
   and point back to the relevant `docs/` section; they do not restate the maths.

## Workspace and dependency graph

```
protocol ──► scoring
        ├──► identity
        └──► network

scoring   (no internal deps; only rand, rand_chacha)
identity  (sha2, voprf, curve25519-dalek, bbs_plus, schnorr_pok, arkworks,
           oblivious_transfer_protocols, secret_sharing_and_dkg, dock_crypto_utils)
network   (sha2, ed25519-dalek, reed-solomon-erasure, opentimestamps)
```

`scoring` sits at the bottom on purpose. `protocol` is the only crate that composes
the other three.

## `scoring` — the deterministic engine

The mathematical core (`docs/02`). The Python simulations in `sim/` are its
executable specification; the Rust implementation must reproduce their results.

| Module | Spec | Key items |
|---|---|---|
| `bridging` | §A | `Ratings`, `BridgingParams`, `Fit`, `fit`, `bridge_scores` |
| `irt` | §B.1–B.2, B.4 | `theta_from_anchors`, `point_biserial`, `fit_2pl_item`, `A_MIN`, `R_PBIS_MIN` |
| `dif` | §B.3 | `logistic_dif`, `mantel_haenszel` (`EtsClass`), `mixture_dif`, `BETA2_MAX`, `MIXTURE_DIF_MAX` |
| `validation` | §B.4 | `purify_theta` (iterative purification to a fixed point) |
| `reputation` | §C | `author_score`, `brier_skill_score`, `evaluator_score`, `asymmetric_ema`, `weight_cap`, `dasgupta_ghosh` |
| `collusion` | §Anti-collusion | `correlation_matrix`, `cluster_by_correlation`, `sublinear_group_weight`, `discount_weights` |
| `glm` (private) | — | shared maximum-likelihood logistic regression |
| `optim` (private) | §A.5 | in-house L-BFGS + numerical gradient |

Notes on non-obvious choices:

- **`bridge_scores` warm-starts each bootstrap from the full fit.** The objective is
  non-convex (the bilinear term `⟨f_u, f_j⟩`), so independent random inits would let
  some subsamples land in a different minimum and pollute the bootstrap-min with
  optimizer noise rather than sampling variability.
- **L-BFGS is in-house** (`optim`) rather than a dependency, to keep full control
  over floating-point determinism.
- **The mixture detector uses a numerical gradient**, matching SciPy's gradient-free
  L-BFGS in `sim/latent_dif_and_capacity.py`; its δ is initialized non-zero to break
  the class symmetry.

## `identity` — anonymous enrollment

The state authenticates but does not issue (`docs/03`).

| Module | Spec | Key items |
|---|---|---|
| `nym` | §M3 | `Role`, `Nym`, `derive_nym` = `H(secret, role)` (lightweight address) |
| `nullifier` | §M3 | `NullifierProof`, `prove`, `verify` (ZK nullifier bound to the BBS+ credential) |
| `ratelimit` | §Cost of proposing | `rln_token`, `within_quota`, `SlotLedger` |
| `enrollment` | §M1 | `IdentityDocument` (+ `Cie`, `Spid`), `UniquenessOracle` (`VoprfOracle` real + `ReferenceOracle` test-only), `EnrollmentRegistry` |
| `oprf` | §M1 | `ThresholdOprfOracle` (Shamir + DLEQ), `KeyShare`, `PublicShare`, `PartialEval`, `DleqProof` |
| `credential` | §M2 | `Credential`, `Issuer`, `ThresholdIssuer`, `IssuerPublic`, `IssuanceRequest`, `AnonymousCredential` |
| `hash` (private) | — | domain-separated SHA-256 |

**Real:** role pseudonyms (deterministic → not rotatable → no whitewashing; distinct
per role → unlinkable) with, on top, a Semaphore-style **ZK nullifier** (`nullifier`):
`N = x·H_role` plus a proof that binds it, in zero knowledge, to a valid BBS+
credential over the same secret `x` — a bespoke sigma-protocol composition (the BBS+
proof of knowledge sharing its message blinding with the nullifier's Schnorr proof
under one Fiat–Shamir challenge), so no circom/Groth16 stack is needed. Also real:
rate-limiting tokens (reuse collides and is detected), the uniqueness label, and
credential issuance. The label has two real backends: a
single-server **VOPRF** (`VoprfOracle`, RFC 9497 via `voprf` — oblivious and
verifiable) and a real **threshold** t-of-n OPRF (`oprf::ThresholdOprfOracle`) that
closes the single-holder gap — the key is Shamir-shared, each member proves its
partial evaluation with a Chaum–Pedersen **DLEQ**, and any `t` Lagrange-combine to the
label, so `t-1` members cannot compute it and none can brute-force the codice-fiscale
space alone. It is a bespoke DH-OPRF on the vetted `curve25519-dalek` group (a tested
exception, see the crypto-rule note). Credential issuance is a real **BBS+** blind
signature (via `bbs_plus`, BLS12-381): the holder commits to its secret and proves
knowledge of it (`schnorr_pok`), the issuer verifies that proof and blind-signs
`(secret, label)` learning only the label, and the holder unblinds a verifiable
signature. This too has both a single-issuer backend (`Issuer`) and a real **threshold**
t-of-n one (`ThresholdIssuer`): the signing key is Shamir-shared and a signature is
produced by the DKLS-based MPC of `bbs_plus::threshold` (DKG + base OT + a
multiplication phase), so `t-1` members cannot sign; the aggregate is an ordinary BBS+
signature, so the holder's request and unblinding are unchanged.
**Still modeled:** a real distributed key-generation ceremony and network transport for
both threshold committees (here trusted-dealer keygen + an in-process committee running
every protocol message locally); and selective-disclosure *presentation* of the
credential (the `PoKOfSignature` reveal, tied to the M3 nullifier).

## `network` — tamper-evident storage

Integrity without permissionless consensus (`docs/04`).

| Module | Spec | Key items |
|---|---|---|
| `cid` | §Content-addressed storage | `Cid`, `cid` |
| `merkle` | §Merkle tree | `leaf_hash`, `merkle_root`, `merkle_proof`, `verify_proof` |
| `log` | §Signed append-only logs | `TransparencyLog` (hash-chained; `verify` detects tampering; `checkpoint` + `verify_extends` prove consistency/truncation against a signed prior head, T14) |
| `consortium` | §The consortium as backbone | `Member` (ed25519), `Checkpoint` (net-id + member-set bound, T15), `Consortium::verify` (t-of-n), `CheckpointClient` (monotonic-height, equivocation, T15) |
| `anchoring` | §Anchoring | `Anchor` trait, `OtsAnchor`, `Receipt`, `AnchorState` |
| `erasure` | §Durability | `encode`, `reconstruct`, `reconstruct_verified` (real Reed–Solomon; per-shard manifest, corrupt-shard authentication before decode, T16) |

**Real:** content addressing, Merkle trees, the hash-chained append-only log,
ed25519 consortium checkpoints, erasure coding, and the anchoring proof format —
`OtsAnchor` builds, serialises and verifies real **OpenTimestamps** `.ots` proofs (via
`opentimestamps`): `verify` runs the actual OTS walk (`Op::execute` over the step tree)
and checks a Bitcoin attestation against a block Merkle root. **Still modeled** for
anchoring: the live network parts — POSTing to a calendar server and reading block
roots from a Bitcoin node/SPV; here an injected block source stands in and
`OtsAnchor::upgrade` models the calendar's confirm-and-upgrade with one hashing step.
**Not yet implemented:** gossip/DHT transport (libp2p) and CRDT convergence.

## `protocol` — lifecycle orchestration

Composes the three layers into the question lifecycle (`docs/05`). Deterministic
steps are seeded for reproducibility.

| Module | Stage | Key items | Uses |
|---|---|---|---|
| `admission` | INV-9/ID-008 | `admit`, `NullifierSet` (T6); `QuotaLedger` — per-credential proposal quota (T11) | `identity::nullifier`, `identity::ratelimit` |
| `blueprint` | [8]/L2 | `Blueprint`, `quotas`, `coverage_deviation`, `assemble_test` | — |
| `deposit` | [2] | `Draft`, `deposit`, `deposit_with_identity` (identity-gated) | `admission`, `identity`, `network::{log,cid}` |
| `exposure` | [9] | `ExposureLedger`, `should_retire`, `Template`, `least_exposed_variant` | `network::cid` |
| `randomness` | INV-10 | `Beacon::{from_checkpoint, seed}` — checkpoint-derived seeds for every draw (T8) | `network::consortium` |
| `lottery` | [3] | `admit`, `admit_from_beacon` (checkpoint-seeded) | `randomness` |
| `review` | [4] | `Reviewer`, `assign_reviewers`, `commit`, `reveal`, `submit_review` (identity-gated) | `admission`, `identity`, `network::cid` |
| `gate` | [5]/[5b] | `GateOutcome`, `bridging_gate`, `supplementary_review` (D26 re-decision, T10/T30), `settle_appeal` | `scoring::bridging` |
| `pilot` | [6]/[7] | `stage1_screen`, `stage2_dif`; batch/sample gates `screen`, `dif_batch`, `admit_dif_batch` (INV-8, T9) | `scoring::irt`, `scoring::dif` |
| `honeypot` | Golden items | `inject`, `reviewer_skill`, `HONEYPOT_RATE` | `scoring::reputation` |
| `governance` | Meta-level | `stratified_sortition`, `change_approved` | — |
| `probation` | Cold start / P2 | `status`, `review_weight`, `FounderSet`, `N_PROBATION` | `identity::nym`, `scoring::reputation` |
| `revalidation` | [8] | `revalidate_pool` (multi-axis), `revalidate_pool_latent`, `items_to_retire` | `scoring::dif`, `exposure` |
| `lifecycle` | §9.1 | `State`, `Event`, `step`, `deposit`, `K_MIN` — rejects every invalid transition (T12); `Event::Resolve` re-decides the band (T10/T30) | `gate`, `review`, `exposure`, `identity::nym` |
| `orchestrator` | Epoch glue | `bridging_weights`, `weighted_ratings` (prior-epoch `w_u` → the fit, T5), `run_item`, `ItemVerdicts` (drives the epoch through `step`, T12) | `lifecycle`, `probation`, `scoring::bridging` |

Each module's doc comment names the attack the stage neutralizes (brigading,
information cascades, queue explosion, the true-but-divisive false negative, block
voting).

## Invariants and where they are enforced

The invariants from [`docs/CLAUDE.md`](docs/CLAUDE.md):

| # | Invariant | Where |
|---|---|---|
| 1 | Anonymity is the base; no demographic attributes | No such fields anywhere; DIF runs on latent axes (`scoring::dif`) |
| 2 | Quality is never decided by majority vote | `scoring::bridging` + `scoring::dif`; `protocol::gate` has no vote count |
| 3 | No money as stake | Bond is reputation (`protocol::deposit`, `scoring::reputation`) |
| 4 | Two reputation scores, never combined | `scoring::reputation` (`author_score` vs `evaluator_score`); separate role nyms in `identity::nym` |
| 5 | One role, one deterministic non-rotatable pseudonym | `identity::nym::derive_nym` |
| 6 | The state authenticates, does not issue | `identity::enrollment` (`UniquenessOracle`) separate from `credential::BlindIssuer` |
| 7 | Scoring is deterministic and reproducible | `scoring` (pinned toolchain, seeded RNG); `tests/reproducibility.rs` |
| 8 | Empirical validation happens in batches | `scoring::dif::mixture_dif`; `protocol::pilot::stage2_dif`; test proves a lone biased item is invisible |

## Reproducibility

Invariant #7 is load-bearing: reproducible computation is what unmasks a dishonest
signer. Measures:

- Pinned toolchain (`rust-toolchain.toml`) and a release profile with
  `codegen-units = 1` and no fast-math.
- Explicitly seeded RNGs (`rand_chacha::ChaCha8Rng`) with a fixed consumption order.
- In-house L-BFGS with a fixed iteration/summation order.
- The transcendental functions (`exp`, `ln`, `ln_1p`, `cos`, `pow`) come from the
  pure-Rust `libm` crate through `scoring::fmath`, not from the platform's libm, whose
  last bits differ between glibc, musl, Apple and Microsoft (AT-BR-04).
- `crates/scoring/tests/reproducibility.rs` asserts `fit`, `bridge_scores`, and
  `mixture_dif` are **bit-for-bit** identical across runs (`f64::to_bits`), and CI
  checks the golden bits (`golden.rs`) on linux-gnu (dev and release), linux-musl,
  macOS-aarch64 and Windows-MSVC (`.github/workflows/ci.yml`, job `golden`).

Note: bit-for-bit equality holds **within** the Rust engine — across platforms and
build profiles since AT-BR-04 — not between Python and Rust: SciPy and the in-house
optimizer differ. The Python sims are an oracle of
*behaviour* (within tolerance), not of bits.

## Testing strategy

Eight kinds of test (the per-crate counts change often; `cargo test --workspace` reports them):

1. **Oracle acceptance tests** run the Rust engine on the *same dataset* as the
   Python sims (exported by `sim/export_fixtures.py` into
   `crates/scoring/tests/fixtures/`) and check it reproduces their numbers — e.g.
   bridging recovers the latent axis at |corr| ≈ 0.99; the evaluator BSS matches the
   sim exactly (follows-the-crowd −1.33, expert 0.95).
2. **Property tests** encode the design guarantees: cross-source duplicate
   enrollment is rejected; a tampered log entry breaks `verify`; a k-of-n checkpoint
   needs k valid signatures; erasure recovers from any k of n; a 500-node cartel's
   influence ≈ 22 independents; a lone biased item stays invisible (batch validation).
   A `proptest` suite (`network/tests/properties.rs`, `protocol/tests/properties.rs`)
   fuzzes these over arbitrary inputs: Merkle inclusion + root sensitivity, erasure
   recovery from any survivor set, log append/verify, blueprint apportionment, lottery,
   sortition. **Model-based** suites (T43: `protocol/tests/{lifecycle,orchestrator}_model.rs`,
   `network/tests/checkpoint_model.rs`) drive the three state machines with random event
   sequences, checking every step against a small reference model and their invariants.
3. **Reproducibility tests** assert determinism (above).
4. **End-to-end integration** (`protocol/tests/end_to_end.rs`) walks the ten civic
   items of the oracle fixtures through all four crates in one epoch and asserts each
   item is stopped at the right stage (ESM by DIF not review; the non-discriminating
   item by the pilot screen; a true-but-divisive item recovered via the appeal). The
   bridging uncertainty band is resolved by the **D26 re-decision**
   (`gate::supplementary_review`, T10/T30): re-run the bridging fit and decide the
   side-balanced score against the plain threshold τ — a bridging decision over the
   latent axis, not a weighted vote — and a polarized item that fails it keeps the
   appeal channel (D26 amendment, T59). It re-fits the first panel's ratings; the extra
   reviewers of D26 are roadmap T60.
   `supplementary_redecision.rs` pins the improvement: the partisan fixture items 08/09
   are **not** passed (the retired weighted-mean tie-break carried them), while a genuine
   near-threshold item is. The √k anti-collusion stays covered at the scoring layer
   (`anti_collusion.rs`) and in the T5 bridging weights.
5. **Adversarial scenarios** (`*/tests/adversarial.rs`) compose mechanisms against
   the threat model: a 400-node cartel is detected and √k-discounted below an honest
   majority; a long-con's reputation rises slowly, falls fast, and is capped;
   whitewashing fails because the role pseudonym is deterministic and re-enrollment
   is refused.
6. **Golden outputs** (`scoring/tests/golden.rs`): every value `fit`, `bridge_scores`
   and `mixture_dif` return on the fixtures is pinned bit-for-bit in
   `fixtures/golden_bits.txt`, so a silent change fails even inside an oracle
   tolerance; hand-computed values (`hand_computed.rs`, `exact_outcomes.rs`) pin the
   formulas behind the range and ordering checks.
7. **Mutation testing** (`cargo-mutants`, run on demand, not in CI): measures what the
   suite verifies rather than executes. Every surviving mutant is killed or justified
   as equivalent in `docs/11-mutation-testing.md`.
8. **`#[ignore]` guards**, run on demand: `fixture_drift` regenerates the oracle
   fixtures from the Python sims and diffs them against the committed ones (catches
   sim/fixture drift; needs numpy/scipy); `power` is a Monte-Carlo check of the
   §B.6 sample-size claim (latent-class DIF detection rate at N≈1500 vs 3000).

Run them:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets
```

## Plug points: real vs. placeholder

| Concern | Status | Production backend |
|---|---|---|
| Bridging, IRT, DIF, reputation, anti-collusion | **Real** | — |
| Role pseudonyms; one credential per label; proposal rate limit | **Real** — deterministic role nyms; `IssuanceRegistry` (one credential per label, T11); `QuotaLedger` per-credential proposal quota keyed on the proven id (T11) | cryptographic-grade RLN (ZK `slot < quota`, reuse reveals key) — T20 |
| ZK nullifier (pseudonym ⇐ valid credential) | **Real** (BBS+-bound sigma protocol), wired into the protocol boundary (T6): `admission` verifies it and the deposit/review/respond entry points key on its proven `id`, with an action-context binding (draft cid and epoch) against replay, and a duplicate cid refused before the quota is charged (T64) | External review of the bespoke composition; cryptographic-grade enrollment/quota (T20/T11) |
| Content addressing, Merkle, transparency log, checkpoints, erasure | **Real** | — |
| Uniqueness label | **Real** (single-server VOPRF RFC 9497; **threshold** t-of-n OPRF, Shamir + DLEQ) | Real DKG ceremony + network transport for the committee |
| Credential issuance | **Real** (BBS+ blind; single-issuer **and** threshold t-of-n MPC) | Real DKG ceremony + network transport; selective-disclosure presentation |
| Public-chain anchoring | **Real** (OpenTimestamps proof format + verification) | Live calendar POST + Bitcoin node/SPV block source |
| Gossip/DHT transport, CRDT | Documented, not implemented | libp2p, Automerge/Yjs |

Reference implementations are clearly marked and provide **no** security; they exist
to make the pipeline testable end-to-end.

## Future work

- Integrate the real cryptographic and transport backends into the plug points.
- Reputation now enters `bridging::fit`: `orchestrator::{bridging_weights, weighted_ratings}`
  turn prior-epoch reviewer standing into the per-reviewer `w_u` the weighted objective
  minimizes over (docs/08 BRIDGE-007, roadmap T5 — **done**), and `end_to_end.rs::run_epoch`
  drives each item through the `lifecycle` state machine (T12 — **done**). The borderline
  band is decided by the `docs/01` D26 mechanism — re-run bridging, re-decide the
  side-balanced score against the plain threshold, and keep the appeal open for a
  polarized item that fails (`gate::supplementary_review`, T10/T30/T59 — **done** on the
  first panel's ratings; the extra reviewers are T60), replacing the retired weighted-mean
  tie-break. The open work is ordered in `docs/10` (mathematics → P2P network → the rest).
  Still open here: persistence (roadmap T13); `governance`
  sortition feeding the honeypot / blueprint committees; `revalidation` → `exposure`
  retirement on a schedule. (Reviewer-vote dedup via the M3 ZK nullifier is done at the
  boundary — `admission::NullifierSet` — T6.)
- Optional engine refinements: 3PL IRT (currently 2PL), infit/outfit MNSQ, Bayesian
  Truth Serum.
- Robustness roadmap from `docs/01` D14: anchoring → erasure coding → multiple
  cross-signing consortia → succinct (zk) proofs of the computation.
