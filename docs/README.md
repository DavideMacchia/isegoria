# A distributed network for producing and validating quiz questions

A network whose nodes — pseudonymous people — **propose** questions, **judge**
other people's questions, and **answer** quizzes to trial them. It exists to
produce banks of questions (true/false or multiple-choice items) in settings where
**no authority is credibly impartial**: control over the questions is distributed
across all nodes instead of belonging to an institution.

Reference use case: civic questions for a possible weighted vote, where you do not
want the government to control the questions. Less contested, more immediate use
cases: professional certifications independent of training bodies, collaborative
fact-checking, licensing exams removed from the profession that has a stake in them.

## The idea in one line

The quality of a question is **not decided by voting** (whoever holds the majority
would control the questions). It is decided by two filters in a row:

1. **Opinion filter** — aggregated peer review *across the bridge* (bridging): what
   matters is where the approval comes from, not how much of it there is. A question
   approved only by the largest camp is discarded.
2. **Evidence filter** — the question is field-trialled on a sample of respondents;
   psychometric statistics (IRT) and bias analysis (DIF) give the final verdict.
   Here nobody's opinion is needed.

The anonymity of the nodes is the non-negotiable base. The uniqueness of a person is
guaranteed by an external enrollment layer (national eID / CIE / SPID / others),
**cryptographically severed** from what the node does.

## Document structure

| File | Contents |
|---|---|
| [`CLAUDE.md`](CLAUDE.md) | Instructions for implementing with Claude Code: work order, invariants not to violate, stack |
| [`00-overview.md`](00-overview.md) | Two-level conceptual architecture, lifecycle of a question |
| [`01-decisions.md`](01-decisions.md) | Every decision taken and every alternative rejected, with rationale (ADR) |
| [`02-scoring-engine.md`](02-scoring-engine.md) | **The mathematical core**: bridging, IRT, DIF, reputation, anti-collusion |
| [`03-identity-enrollment.md`](03-identity-enrollment.md) | Anonymous multi-source enrollment, threshold credentials, role pseudonyms |
| [`04-storage-network.md`](04-storage-network.md) | P2P layer, consortium, erasure coding, anchoring, node types |
| [`05-question-lifecycle.md`](05-question-lifecycle.md) | Step-by-step protocol, appeal to evidence, lottery, honeypot, cold start |
| [`06-threat-model.md`](06-threat-model.md) | Attack surface and countermeasures; what the system does NOT solve |
| [`07-verification-and-assurance.md`](07-verification-and-assurance.md) | Verification, assurance and falsification methodology |
| [`08-formal-specification.md`](08-formal-specification.md) | Independent audit: what is implemented, tested and still open |
| [`10-roadmap.md`](10-roadmap.md) | **Development plan by priority** — mathematics, then P2P network, then the rest (task ids `T#`) |
| [`11-mutation-testing.md`](11-mutation-testing.md) | Mutation-testing report: how much the tests verify, and every accepted survivor |
| [`12-panic-audit.md`](12-panic-audit.md) | Panic audit and fuzzing report: every `unwrap`/`expect`/`assert` classified, every crash on hostile input fixed |
| [`13-characterization.md`](13-characterization.md) | T24: the simulation studies that characterize the detectors and gates, the harness that runs them, and their results |
| [`99-glossary.md`](99-glossary.md) | Every concept explained from scratch, from the problem to the formula |
| [`sim/`](../sim/) | Executable simulations that demonstrate the behavior and the corner cases |
| [`paper/`](../paper/) | Working paper: formal statement and analysis of the scoring mechanism, with proofs, reproducible experiments and open problems |

## Development priorities

The reference implementation exists (see Status below); what remains is ordered in
[`10-roadmap.md`](10-roadmap.md) in three phases:

1. **Mathematics** (`02`, `01` D32–D41) — the two severe defects of the protocol
   boundary (a replayed deposit, respondents without an identity gate) are fixed (T64,
   T65), the engine rejects malformed input (T62) and the gate reads the side-balanced
   score (D32, T49); now correct the rest of the mechanism (D33–D41) and the decisions
   built on it (band, appeal), and characterize every threshold. It is the genuinely new
   piece, and everything else consumes its numbers.
2. **P2P network** (`04`) — persistence, transport and replication, a grind-free
   randomness beacon, live anchoring: a single-organization testnet.
3. **The rest** — the protocol boundary (`05`), distributed identity and the external
   cryptographic review (`03`), statistical privacy (`06`), and the real-world pilots
   that fix the empirical parameters.

The original build order (offline engine first, then identity, network, protocol) is
done as a reference implementation; the closed pilot and the opening to independent
operators come after the three phases.

## Status

A reference implementation exists as a Cargo workspace (`crates/{scoring,identity,
network,protocol}`); see the top-level `README.md` and `ARCHITECTURE.md` for the
current state, and `docs/08-formal-specification.md` for an independent audit of what
is implemented, tested, and still open. The simulations in `sim/` are the executable
specification the engine reproduces, not the deployment target.

## License

EUPL-1.2 (see `LICENSE`).
