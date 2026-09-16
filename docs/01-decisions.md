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
non-discriminating questions; a second stage (~1500–3000) only for the survivors, for
DIF analysis.

**Why.** DIF analysis on latent axes needs many respondents to have enough people at
each competence level. Wasting them on obviously broken questions is inefficient.
Respondents are the system's scarce resource. The sizes are derived, not arbitrary,
and set a floor on the network itself — see `02` §B.6 (the fully-anonymous
latent-class DIF wants ~3000, not 1500).

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

## D15 — Issuing committee separate from the storage consortium

**Choice.** The committee that issues identity credentials is distinct from the
consortium that signs the data. Two separate bodies, not one. (Confirmed; this
supersedes the earlier "preferred" wording — see D22 for the enrollment binding it
protects.)

**Why.** The same committee is simpler but concentrates two powers (data custody +
identity issuance) in a single body: a single compromised body could then both learn
who you are and manipulate the data. Separation is exactly what protects anonymity,
and is faithful to the separation-of-powers logic that underpins the whole design.

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

---

## D17 — Who may re-check the computation: consortium now, cryptographic proofs later

**Choice.** Verifying the scores requires the votes, and votes are **never published
in the clear** (that would enable statistical de-anonymization). Therefore:
- *Interim:* only the storage consortium — redundant and threshold-signed — re-runs
  the deterministic engine and signs the result. Ordinary participants trust the
  signed output, not their own recomputation.
- *Target:* zero-knowledge proofs of the computation (D14 step 4), so anyone can
  verify the scores are correct **without** seeing any vote — adopted as soon as it
  is computationally feasible.

**Why.** Reproducibility (invariant #7) and secrecy of voting patterns are in direct
tension. Publishing every pseudonymous vote so "anyone can redo the math" breaks the
anonymity that is the base of the whole system. Keeping the check inside a signed,
forkable consortium preserves both today; zk proofs remove the trust assumption
tomorrow. Resolves docs/08 §17 Q-1 / G-20.

**Rejected.** Publishing all ratings per pseudonym (contradicts docs/CLAUDE.md and
the anonymity base).

---

## D18 — A lost or stolen secret is not recoverable

**Choice.** If a person loses or has stolen their credential secret, the identity is
lost permanently: no recovery, no re-issuance to the same "self", accumulated
reputation is gone. No revocation list at launch.

**Why.** The non-rotatable pseudonym (invariant #5) is what stops whitewashing —
starting over to shed a bad reputation. Any recovery/revocation path is, by
construction, also a way to obtain a fresh identity, and a revocation list keyed on
identities adds a linkability channel that weakens anonymity. The loss is a real
cost, accepted for now in exchange for simplicity and privacy. Resolves Q-13 / G-17.

---

## D19 — Only CIE/SPID identities at launch; foreign documents deferred

**Choice.** Enrollment accepts only Italian digital identities (CIE/SPID) at launch.
People without one (e.g. foreign passports) cannot enroll until a mechanism that
preserves "one person = one account" across identity systems exists.

**Why.** Uniqueness (Sybil resistance) depends on a single canonical anchor space.
Different national identity systems live in different anchor spaces, so a person
could enroll once per system — a Sybil hole. Better to exclude for now than to open
that hole. A known limitation to revisit, not a permanent exclusion. Resolves Q-14 /
docs/03 F2.

---

## D20 — Bias detection: latent-class method in production, attribute-based only in pilots

**Choice.** The empirical bias test (DIF) runs in production using the **latent-class
(Variant 2)** method only, which needs no declared attribute. The attribute-based
method (Variant 1) is permitted **only in closed calibration pilots** with declared
attributes, never in production.

**Why.** Variant 1 needs a per-respondent group value that the live system cannot
possess without either linking a person's roles (violates P3) or collecting a
declared attribute (violates invariant #1). Only the latent method is compatible with
anonymity. Resolves Q-2 / G-01.

---

## D21 — Honeypot ground truth comes from validated history, not committee opinion

**Choice.** The "known quality" of the golden items used to score evaluators is taken
from the empirical history (items that passed or failed Level-B validation), not from
a committee's judgment of what a good item is.

**Why.** Letting a committee declare an item's quality would reintroduce exactly the
subjective opinion-as-truth that the evidence filter (D3) exists to remove, and hand
the committee a lever over evaluator scores. Resolves Q-9 / G-16.

---

## D22 — Enrollment binds the anonymous label to the state-authenticated identity

**Choice.** The uniqueness label is derived from an input that is cryptographically
bound to the identity the state authenticated (e.g. the identity provider signs a
commitment to the fiscal code, and the holder proves the blinded OPRF input opens to
that signed value). A holder cannot enroll with a made-up identity.

**Why.** Without this binding, anyone could request a label for an invented anchor and
enroll any number of times — Sybil resistance would not be provided by the
cryptography at all. This is the enrollment-side counterpart of D15's separation.
Resolves Q-3 / G-02.

---

## D23 — Evaluator score baseline: the crowd's prediction, not the outcome base rate

**Choice.** The evaluator skill score (BSS) is measured against the crowd's average
predicted probability (a weight-adjusted average of the reviewers' own predictions),
computed by the consortium re-runner — not against the after-the-fact base rate of
outcomes.

**Why.** The design goal (D6) is to reward genuine judgment and give ~0 to someone who
just follows the crowd. That property holds against a crowd baseline; the base-rate
baseline currently in code measures something different and depends on hindsight.
Predictions stay private (D17): the consortium computes the baseline without
publishing them. Resolves Q-4 / G-09.

---

## D24 — One documented threshold for the latent-bias detector

**Choice.** The latent-class bias detector reports a single quantity as the item's
DIF, `DIF_j = 2·|δ̂_j|` (the gap in difficulty between the two hidden groups), and
rejects above one documented value chosen from a false-positive / false-negative
study — not the three different metrics/values currently spread across docs, sim and
code.

**Why.** Today the design doc, the simulation and the code disagree on both what is
measured and the cut-off, so the same data could pass in one place and fail in
another. One metric, one value, one justification. Resolves Q-5 / G-08.

---

## D25 — Declared ability metric; guessing correction for multiple-choice

**Choice.** The document states explicitly which "ability" scale the item thresholds
are expressed in, and adds the guessing correction (3PL) for multiple-choice items,
or documents why it is safe to omit.

**Why.** The thresholds (discrimination, difficulty) were borrowed from the
psychometric literature, which uses a specific ability scale; the code uses a simpler
proxy, so a threshold can mean something different than intended. Multiple-choice is
exactly where guessing matters. Resolves Q-6 / G-07.

---

## D26 — Borderline items: more reviewers, then a clean re-decision

**Choice.** An item that lands in the uncertainty band at the bridging gate goes to an
additional round of reviewers and is then re-decided against the plain threshold,
without the band.

**Why.** "Supplementary review" was a label with no defined meaning; an item could sit
there forever. Adding reviewers and re-deciding gives borderline items a definite,
evidence-based outcome. Resolves Q-7 / G-15 (supplementary-review part).

---

## D27 — Appeal cost is a pseudo-observation inside the author score

**Choice.** The cost of a (failed) appeal is modelled as a negative pseudo-observation
inside the author's reputation score, escrowed when the appeal is filed and replaced
by the real result on verdict — not as a separate deduction from an unrelated ledger.

**Why.** The author score is defined as an average of item-quality observations;
subtracting an arbitrary constant makes it no longer that average, so two parts of the
spec contradict each other. A pseudo-observation keeps the score coherent. Resolves
Q-8 / REPUTATION-007.

---

## D28 — Sortition members act under a dedicated pseudonym

**Choice.** A participant drawn by lottery for a governance role acts under a separate,
dedicated pseudonym, not under their proposing or judging pseudonym.

**Why.** The sortition draws from judging pseudonyms (which carry a position
estimate); if the drawn member then acted under another role, it would link their
pseudonyms and leak. A dedicated pseudonym keeps the roles unlinkable. Resolves Q-10 /
G-19.

---

## D29 — Public randomness comes from the latest signed checkpoint

**Choice.** Every lottery, reviewer assignment, honeypot placement and sortition draws
its randomness from the latest threshold-signed consortium checkpoint head (fixed
after the relevant submissions close), combined with the item id — not from a seed any
participant can choose or grind.

**Why.** If an author could influence the seed (e.g. by editing their draft), they
could select their own reviewers — the brigading random assignment exists to prevent.
A value fixed by the signed checkpoint is public, unpredictable in advance and
unchooseable. Resolves Q-11 / G-05.

---

## D30 — The uniqueness key is not rotated without a dedup-preserving migration

**Choice.** The key behind the uniqueness label is not rotated except through a
documented migration that preserves de-duplication. Routine proactive refresh of the
committee's shares (which does not change the label) is fine; changing the key itself
is not, absent such a migration.

**Why.** Every label is a function of that key. Silently changing it re-computes every
label and re-opens double enrollment for everyone. Resolves Q-12 / ID-006 (INV-11).

---

## D31 — One ideological dimension (d=1) for now

**Choice.** The bridging model uses a single latent axis (`d = 1`). A second dimension
(`d = 2`) is deferred; the reference use case and the simulations do not require it.

**Why.** One axis already captures the dominant fracture the design targets, and it
keeps the model simpler to reason about and reproduce. Adding a dimension is a future
option if a real deployment shows a single axis is insufficient. Resolves Q-16 /
BRIDGE-001.
