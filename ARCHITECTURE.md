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
   seeded RNGs, fixed iteration order). See [Reproducibility](#reproducibility).
3. **Never roll our own crypto.** Heavy primitives enter behind traits with
   non-production reference implementations; production wires them to mature
   libraries.
4. **The docs are the source of truth for the logic.** Code comments are minimal
   and point back to the relevant `docs/` section; they do not restate the maths.

## Workspace and dependency graph

```
protocol ──► scoring
        ├──► identity
        └──► network

scoring   (no internal deps; only rand, rand_chacha)
identity  (sha2)
network   (sha2, ed25519-dalek, reed-solomon-erasure)
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
| `nym` | §M3 | `Role`, `Nym`, `derive_nym` = `H(secret, role)` |
| `ratelimit` | §Cost of proposing | `rln_token`, `within_quota`, `SlotLedger` |
| `enrollment` | §M1–M2 | `IdentityDocument` (+ `Cie`, `Spid`), `UniquenessOracle` (+ `ReferenceOracle`), `EnrollmentRegistry` |
| `credential` | §M2 | `Credential`, `BlindIssuer` (+ `ReferenceIssuer`) |
| `hash` (private) | — | domain-separated SHA-256 |

**Real:** role nullifiers (deterministic → not rotatable → no whitewashing; distinct
per role → unlinkable) and rate-limiting tokens (reuse collides and is detected).
**Plug points:** `UniquenessOracle` (threshold OPRF over the anchor) and
`BlindIssuer` (BBS+ blind, t-of-n issuance) — the reference impls exist only to
exercise the pipeline and provide no security on their own.

## `network` — tamper-evident storage

Integrity without permissionless consensus (`docs/04`).

| Module | Spec | Key items |
|---|---|---|
| `cid` | §Content-addressed storage | `Cid`, `cid` |
| `merkle` | §Merkle tree | `leaf_hash`, `merkle_root`, `merkle_proof`, `verify_proof` |
| `log` | §Signed append-only logs | `TransparencyLog` (hash-chained; `verify` detects tampering) |
| `consortium` | §The consortium as backbone | `Member` (ed25519), `Checkpoint`, `Consortium::verify` (t-of-n) |
| `anchoring` | §Anchoring | `Anchor` trait (+ `ReferenceAnchor`) |
| `erasure` | §Durability | `encode`, `reconstruct` (real Reed–Solomon) |

**Real:** content addressing, Merkle trees, the hash-chained append-only log,
ed25519 consortium checkpoints, and erasure coding. **Plug points:** `Anchor`
(OpenTimestamps), and — documented but not yet implemented — gossip/DHT transport
(libp2p) and CRDT convergence.

## `protocol` — lifecycle orchestration

Composes the three layers into the question lifecycle (`docs/05`). Deterministic
steps are seeded for reproducibility.

| Module | Stage | Key items | Uses |
|---|---|---|---|
| `deposit` | [2] | `Draft`, `deposit`, `NoPrimarySource` | `network::log`, `network::cid` |
| `lottery` | [3] | `admit` | — |
| `review` | [4] | `Reviewer`, `assign_reviewers`, `commit`, `reveal` | `identity::nym` |
| `gate` | [5]/[5b] | `GateOutcome`, `bridging_gate`, `settle_appeal` | (scoring outputs) |
| `pilot` | [6]/[7] | `stage1_screen`, `stage2_dif` | `scoring::irt`, `scoring::dif` |
| `honeypot` | Golden items | `inject`, `reviewer_skill`, `HONEYPOT_RATE` | `scoring::reputation` |

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
- `crates/scoring/tests/reproducibility.rs` asserts `fit`, `bridge_scores`, and
  `mixture_dif` are **bit-for-bit** identical across runs (`f64::to_bits`).

Note: bit-for-bit equality holds **within** the Rust engine, not between Python and
Rust — SciPy and the in-house optimizer differ. The Python sims are an oracle of
*behaviour* (within tolerance), not of bits.

## Testing strategy

Three kinds of tests, 61 in total:

1. **Oracle acceptance tests** run the Rust engine on the *same dataset* as the
   Python sims (exported by `sim/export_fixtures.py` into
   `crates/scoring/tests/fixtures/`) and check it reproduces their numbers — e.g.
   bridging recovers the latent axis at |corr| ≈ 0.99; the evaluator BSS matches the
   sim exactly (follows-the-crowd −1.33, expert 0.95).
2. **Property tests** encode the design guarantees: cross-source duplicate
   enrollment is rejected; a tampered log entry breaks `verify`; a k-of-n checkpoint
   needs k valid signatures; erasure recovers from any k of n; a 500-node cartel's
   influence ≈ 22 independents; a lone biased item stays invisible (batch validation).
3. **Reproducibility tests** assert determinism (above).

Run them:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets
```

## Plug points: real vs. placeholder

| Concern | Status | Production backend |
|---|---|---|
| Bridging, IRT, DIF, reputation, anti-collusion | **Real** | — |
| Role nullifiers, rate-limiting tokens | **Real** (hash-based) | Semaphore (ZK nullifiers) |
| Content addressing, Merkle, transparency log, checkpoints, erasure | **Real** | — |
| Uniqueness label | Trait + reference | Threshold OPRF |
| Credential issuance | Trait + reference | BBS+, t-of-n blind |
| Public-chain anchoring | Trait + reference | OpenTimestamps |
| Gossip/DHT transport, CRDT | Documented, not implemented | libp2p, Automerge/Yjs |

Reference implementations are clearly marked and provide **no** security; they exist
to make the pipeline testable end-to-end.

## Future work

- Integrate the real cryptographic and transport backends into the plug points.
- Extract meta-level governance (stratified sortition for the honeypot committee,
  coverage blueprint, and consortium selection) into its own module.
- Optional engine refinements: 3PL IRT (currently 2PL), infit/outfit MNSQ, Bayesian
  Truth Serum.
- Robustness roadmap from `docs/01` D14: anchoring → erasure coding → multiple
  cross-signing consortia → succinct (zk) proofs of the computation.
