# Instructions for Claude Code

This repository contains the specification of a distributed network for producing
and validating quiz questions. You are here to **implement it**. Read
`00-overview.md` first, then `02-scoring-engine.md` (the central piece), then the
rest as needed.

## Invariants never to violate

These constraints are non-negotiable: breaking one destroys the property that makes
the system useful. Before every implementation choice, check you are not violating
them.

1. **Anonymity is the base.** No personal, demographic, or affiliation data ever
   enters the system. Nodes are pseudonymous IDs and nothing else. Do not add
   "optional" fields that collect people's attributes, not even to improve DIF: the
   bias analysis runs on latent axes, not on declared labels (see `02`, §DIF).

2. **Quality is not decided by voting.** Never implement a simple-majority
   aggregation to accept or reject a question. Acceptance goes through bridging
   (opinion filter) + empirical validation (evidence filter). If you find yourself
   writing `if positive_votes > negative_votes`, you have drifted from the design.

3. **No money as stake.** No token, no economic stake, no monetary bond to propose
   or vote. The cost of proposing is reputation and rate limits. A money-based
   mechanism reintroduces wealth-based access, which is exactly the problem to avoid.

4. **Two separate scores, never combined.** The author score (`C`) and the evaluator
   score (`E`) live on **different, unlinkable pseudonyms** of the same person. `E`
   weights the review vote; `C` governs the rate limit on proposals. Do not merge
   them into a single weight and do not have them share the same ID.

5. **One role, one deterministic pseudonym, not rotatable.** Role pseudonyms derive
   deterministically from the credential (`H(secret, context)`). A person can derive
   exactly one author pseudonym, one evaluator pseudonym, one respondent pseudonym.
   Do not allow generating fresh pseudonyms for the same role: it would wipe out
   negative reputation (whitewashing).

6. **The state authenticates, it does not issue.** Whoever verifies identity
   (CIE/SPID) is distinct from whoever issues the anonymous credential (the
   threshold committee). Do not collapse the two roles: the separation is what stops
   the state from linking person and activity.

7. **Score computation is deterministic and reproducible.** The scoring engine,
   given the same input (ratings + answers), produces the same output bit-for-bit.
   No unseeded sources of non-determinism (dictionary iteration order, uncontrolled
   floating point, the platform's libm — transcendental functions go through
   `scoring::fmath` — timestamps in the computation). Reproducibility is the defense
   that unmasks a dishonest signer.

8. **Empirical validation happens in batches, never on a single question.** An
   isolated question does not allow detecting bias on latent axes (see
   `sim/latent_dif_and_capacity.py`). Always validate groups of questions.

## Current priorities

The four crates exist; the work left is ordered in `10-roadmap.md`. Follow its phases in
order — **mathematics** (T64, T65, T62, the side-balanced score T49, the appeal for
band items T59, the appeal stake T61, the anchor-reliability gate T53, the proper
evaluator score T50, the change detector T51, the band's extra round T60, the
coordination detector on residuals T56, panel diversification T57, live outcomes
with exploration T52, the latent DIF target model T54, the reviewer floor T39, the
differential oracles T45 and the contested-facts pool T55 are done; next the
characterization T24/T25 — T24 is specified in `13-characterization.md` and its harness is
`crates/characterization`; the full run happens on the owner's machine), then
the **P2P network**, then **the rest** (protocol boundary, distributed identity,
privacy, pilots) — and inside a phase, fix defects in existing code before adding
features. Every task starts with a test that fails on the current code.

## Original build order (done)

The order the reference implementation was built in — each phase verifiable before the
next:

1. **`scoring/`** — the deterministic engine. Input: a ratings file (node ×
   question) and an answers file (respondent × question). Output: bridging scores,
   IRT parameters, DIF verdicts, reputations. It must run offline, with no network
   or identity. The simulations in `sim/` are the executable spec: your
   implementation must reproduce their results.
2. **Tests against the corner cases** documented in `06` and in `sim/`: coordinated
   cartel, elite consensus, true-but-divisive question, survival rate. These are the
   engine's acceptance tests.
3. **`identity/`** — enrollment adapters, threshold credential, role nullifiers.
   See `03`.
4. **`network/`** — signed append-only logs, gossip, DHT, erasure coding, anchoring.
   See `04`.
5. **`protocol/`** — lifecycle orchestration: random reviewer assignment,
   commit-reveal, bridging gate, two-stage pilot, appeal channel, honeypot. See `05`.

## Suggested stack (not binding)

- **Scoring engine**: Python with NumPy/SciPy for the prototype (the sims already
  are); consider Rust for the production version when bit-for-bit reproducibility and
  performance matter.
- **Identity cryptography**: prefer mature, audited libraries — Semaphore for
  nullifiers, BBS+ schemes for credentials, a threshold OPRF for anchoring. Rolling
  our own is permitted when it genuinely serves the design — no suitable library
  exists, or we need a variant a library does not provide (e.g. a threshold scheme
  built from a vetted single-party one). When we do, keep it small, build it on
  audited primitives (not from first principles), pin it down with known-answer and
  property tests, and mark it clearly as bespoke. The bar is high, not absolute.
- **P2P layer**: consider libp2p (gossipsub + Kademlia DHT). CRDTs with Automerge or
  Yjs where converging state is needed.
- **Anchoring**: OpenTimestamps or equivalent to publish the Merkle root on a public
  chain.

## What NOT to do

- Do not introduce a permissionless blockchain with global consensus. Heavy
  consensus is not needed (writes almost never conflict) and reintroduces cost,
  latency, and public data. See `01` and `04`.
- Do not put voting patterns or questions under review on a public register in the
  clear: they are a vector for statistical deanonymization.
- Do not implement a Kleros-style Schelling point (rewarding those who vote with the
  majority). It is the opposite of what is needed: see the evaluator score in `02`.
- Do not treat an LLM's output as a source of truth in the scoring engine. The
  engine is deterministic and mathematical.

## Code style and docs

- **Comments are budgeted, and the budget is enforced.** The design docs are the source of
  truth for the logic (`ARCHITECTURE.md` principle #4); code comments never restate them.
  `scripts/comment_budget.py` fails a file that exceeds the budget, and it runs in CI and,
  through the hook in `.claude/settings.json`, after every edit Claude Code makes:
  - a `//!` module header of at most 3 lines: what the module is, plus the `docs/` pointer;
  - `///` item docs of at most 3 lines: only the contract the signature does not express
    (shapes, units, preconditions, what an error means), plus the `docs/` pointer;
  - inline `//` runs of at most 2 lines, only for a mechanic the code cannot express;
  - no comment line over 100 characters (a longer line is not a shorter comment);
  - at most 10% comment lines per file (files with 20 comment lines or fewer are exempt);
  - one line per test: the property checked and its AT id.
  Never in a comment: rationale, history (task or decision narratives), measurements,
  rejected alternatives, or a narration of the next statement. A fact worth keeping that is
  not in `docs/` goes into `docs/` (`02`, `08`), not into a comment.
- **Keep the docs in step with the code.** When a change implements or alters behaviour
  the docs describe, update the relevant section in the same change (e.g. the `docs/08`
  §15 status matrix, the `docs/10` roadmap) rather than letting them drift.
