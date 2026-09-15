# Architectural decisions

Each decision states: what was chosen, why, and which alternatives were rejected.
The decisions here are the **final** state; some supersede intermediate choices made
during design.

---

## D1 — Quality is not decided by majority

**Choice.** Accepting a question goes through two filters: bridging (opinion) +
empirical validation (evidence). Never a majority vote count.

**Why.** Voting on quality means handing control of the questions to whoever controls
the majority of votes. That is exactly the power the system must take away from the
government, shifted onto a faction.

**Rejected.** Upvote/downvote with a threshold; reputation that rises with received
upvotes (concentrates power faster than plain voting).

---

## D2 — Bridging instead of vote averaging

**Choice.** The aggregation of peer review is a matrix factorization with asymmetric
regularization (`02`, Level A), in the style of Community Notes. Score = the item
intercept `b_j`.

**Why.** It forces the model to explain approval first as an effect of alignment;
only cross-cutting approval survives in the intercept. Robust to manipulation even
when open source. In testing: to pass a partisan question you must corrupt 87% of the
opposing camp, versus 50% of your own camp with majority voting.

**Rejected.** Simple average; PageRank and variants (less robust to targeted
brigading in this context).

---

## D3 — Final truth comes from data, not from judgments

**Choice.** The final verdict on a question is given by psychometric statistics (IRT)
and bias analysis (DIF) on real answer data, not by peer review.

**Why.** Reviewers' opinion is attackable with coalitions; answer data is not, it
would require corrupting the sample. In testing peer review turned out to be almost
uncorrelated with empirical validity: an evaluator who follows the peer consensus
does worse than guessing.

---

## D4 — DIF on latent axes, not on declared attributes

**Choice.** Bias analysis uses the latent position `f_u` estimated by bridging, or a
latent-class IRT mixture. No demographic attribute is collected.

**Why.** IDs are purely pseudonymous (D9). Self-declared labels would be noisy,
manipulable, and would capture the fracture only if guessed in advance. Testing shows
bias detection works without knowing which axis it is, provided the distortion is
systematic and questions are validated in batches.

**Supersedes.** The initial version planned DIF on observed groups (party, region,
education). Dropped for incompatibility with anonymity.

---

## D5 — Reputation: two separate scores, never combined

**Choice.** Author score `C` and evaluator score `E` live on different, unlinkable
pseudonyms. `E` weights the review vote; `C` governs only the rate limit on
proposals. No combined weight.

**Why.** A single ID accumulating both would allow joining the register of proposals
with that of votes (deanonymization). Collapsing them also creates a channel to
convert success as an author into influence over other people's questions.

**Supersedes.** The initial version had a combined weight `w = C^ρ · E^(1-ρ)`.
Dropped.

---

## D6 — Evaluator score with its own scoring rule, not a Schelling point

**Choice.** The evaluator declares a probability that the question passes validation,
and is scored with a proper scoring rule normalized on the crowd baseline (Brier
Skill Score). You gain reputation only by being right when the crowd is wrong.

**Why.** In a system that must resist majority capture, the correct dissenter must be
rewarded structurally. Rewarding alignment with the majority (like the Kleros
Schelling point) is the opposite of what is needed.

**Rejected.** Schelling point / voting consistent with the majority (Kleros, UMA).

---

## D7 — Sublinear anti-collusion discount

**Choice.** The weight of a correlated group ∝ √(size). Clusters identified by
behavior correlation.

**Why.** An individual cap does not stop a cartel. With the √k discount, 500
coordinated nodes count as 22 independent ones.

**Accepted cost.** It also penalizes genuine agreement between people who think alike
for good reasons. The system is tuned conservatively: pass few questions but solid
ones.

---

## D8 — Appeal-to-evidence channel

**Choice.** A question discarded by bridging *for polarization* (high `|f_j|`) and not
for a defect can skip review and go straight to the pilot, at the cost of the
author's reputation, refunded if the psychometrics promote it.

**Why.** Bridging does not tell "true but divisive" from "one-sided propaganda": they
produce the same voting pattern. Without an appeal channel, the politically most
important category of questions is systematically lost. Emerged directly from testing
(a clean, factual question was killed by bridging and no threshold saved it).

---

## D9 — Anonymity as the base; uniqueness solved externally

**Choice.** Nodes are pseudonymous IDs. The uniqueness of a person is guaranteed by
an external enrollment layer (national eID / CIE / SPID / others) cryptographically
severed from the activity. See `03`.

**Why.** Explicit design requirement. Anonymity must hold even against a state-level
actor in the threat model.

---

## D10 — Spam control: lottery + rate-limiting nullifier, not a low quota

**Choice.** Anyone can propose; each epoch a randomly drawn subset enters the
pipeline. The per-person technical limit is enforced by a rate-limiting nullifier.

**Why.** The real bottleneck is not reviewers but pilot respondents: validation
capacity is under ~1 proposal/year per node. Even a strict quota (2/month) would
produce 30× more questions than the network can validate, and the queue would
explode. The lottery gives equal access in expected value without letting the queue
grow to infinity.

---

## D11 — Two-stage pilot

**Choice.** A cheap first stage (~300 respondents) that immediately kills broken and
non-discriminating questions; a second stage (~1500) only for the survivors, for DIF
analysis.

**Why.** DIF analysis on latent axes needs many respondents to have enough people at
each competence level. Wasting them on obviously broken questions is inefficient.
Respondents are the system's scarce resource.

---

## D12 — Storage: P2P signed logs, not a permissionless blockchain

**Choice.** A P2P layer (gossip + DHT + append-only log + CRDT) with a consortium of
a few dozen heterogeneous signers as the backbone. No global consensus. See `04`.

**Why.** The property we need — immutability, reproducible computation, absence of a
single administrator — is obtained without the cost, latency, and public exposure of
data of a blockchain. Writes almost never conflict, so heavy consensus is almost
always useless; double-voting is solved with the nullifier, not with consensus.

**Rejected.** Permissionless blockchain (costly, slow, public by construction,
scoring computation too heavy on-chain).

---

## D13 — No money as stake, ever

**Choice.** The cost of proposing is reputation and rate limits. No token, stake, or
monetary bond.

**Why.** A money-based mechanism reintroduces wealth-based access. This also applies
to the storage choice: permissionless consensus with an economic stake
(proof-of-stake) is rejected for the same reason, even though it is the maximum
theoretical distribution.

---

## D14 — "Slower but better" hardenings: adoption order

**Choice.** In order: (1) hourly anchoring of the root to a public chain; (2) erasure
coding across all nodes; (3) multiple consortia that counter-sign each other; (4)
cryptographic proofs of the computation (mature phase).

**Why.** The two best wins — anchoring and erasure coding — raise reliability and
distribution **without** making consensus slower, leaning on already-robust external
structures. Heavier consensus is the last thing to touch.

---

## D15 — Issuing committee separate from the storage consortium (preferred)

**Choice.** The committee that issues identity credentials is, preferably, distinct
from the consortium that signs the data.

**Why.** The same committee is simpler but concentrates two powers (data custody +
identity issuance) in a single body. Separated is more faithful to the
separation-of-powers logic that underpins the whole design. A governance decision,
dependent on how many independent organizations can be involved.

---

## D16 — Consortium selection: no purely technical solution

**Choice.** A combination of: reserved seats per category (including categories of
declaredly opposite orientation) + entry deposit + per-category cap. The ultimate
defenses are the **reproducibility of the computation** (a dishonest signer unmasks
itself) and the **freedom to fork** (if the consortium betrays, the network abandons
it, with all the data). Parameters and composition of the meta-level are managed by
stratified sortition.

**Why.** "Who is in the consortium" is a human decision about whom to trust, and it
is the weakest link. But it becomes a bearable — not fatal — problem if the
computation stays reproducible and exit stays free: the choice becomes revocable
instead of permanent.
