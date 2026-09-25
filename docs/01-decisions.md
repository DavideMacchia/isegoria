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

> **Superseded for the protocol by D40** (2026-09-24): detected clusters constrain panel
> assignment instead of losing weight; detection itself moves to model residuals (D39).

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

> **Refined by D33** (2026-09-24): the crowd baseline excludes the reviewer being scored, and
> the score is a difference of Brier scores instead of the ratio-form BSS.

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

> **Amendment (decided and implemented 2026-09-25, T59).** A band item that fails the
> re-decision follows the below-band rule of the gate: with a side gap at or above the
> appeal threshold it was rejected for polarization and is `AppealEligible`; otherwise it
> is `Rejected(Borderline)`. Before, `Resolve { passed: false }` ended every failing band
> item in a terminal reject, so a true-but-divisive item that happened to land in the band
> lost the correction channel of `docs/05` [5b] that an item scored clearly below the band
> keeps (third review, `docs/08` PROTO-004).
>
> **Extra round implemented (2026-09-25, T60):** a band item gets `k_extra` reviewers
> drawn from the beacon outside its first panel (`review::assign_extra_from_beacon`,
> `K_EXTRA = 4`, provisional), who commit and reveal on the same item under the first
> round's rules (`Event::AssignExtraReviewers`, then `Commit`/`CloseCommits`/`Reveal` in
> `SupplementaryReview`); `Event::Resolve` is refused until every extra panelist revealed
> (`NoExtraPanel`, `PartialEpoch`), and the re-decision fits the first panel's ratings
> *plus* the extra round's (`orchestrator::{extra_round, expanded_ratings}`,
> `gate::supplementary_review`). Before, the re-decision re-fitted the first panel's
> ratings alone, which relaxed the robust threshold instead of adding evidence
> (`docs/08` PROTO-008). A band item nine reviewers approve just above `τ` is now
> rejected when four extra reviewers disapprove (`supplementary_redecision.rs`).

**Choice.** An item that lands in the uncertainty band at the bridging gate goes to an
additional round of reviewers and is then re-decided against the plain threshold,
without the band.

**Why.** "Supplementary review" was a label with no defined meaning; an item could sit
there forever. Adding reviewers and re-deciding gives borderline items a definite,
evidence-based outcome. Resolves Q-7 / G-15 (supplementary-review part).

---

## D27 — Appeal cost is a pseudo-observation inside the author score

> **Implemented as decided (2026-09-25, T61):** `protocol::appeal::AuthorHistory` holds
> the author's quality observations; `file_appeal` refuses an author whose `C_a` is below
> the stake floor — the prior mean `α₀ / (α₀ + β₀)`, 0.4 with the default prior — and
> otherwise escrows a zero-quality observation at age 0, so `C_a` falls at once;
> `orchestrator::settle_appeal` replaces it with the item's measured quality when the
> item reaches the pool and leaves it standing on any other terminal. `run_item` derives
> the appeal's two checks (window, `C_a ≥ floor`) from `ItemVerdicts` instead of taking
> the caller's word. The ledger form (`gate::settle_appeal`: `+ gain` / `− stake`) is
> retired: **there is no additive gain** — promotion replaces the pseudo-observation with
> a real, good observation, and that is the author's reward for being right against the
> opinion filter (`05` [5b]).

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

> **Superseded by D41** (2026-09-24): the checkpoint head can be ground by whoever orders the
> last deposits (T37); the beacon becomes commit-reveal among members, then a threshold signature.

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

---

## D32 — Bridge score: side-balanced predicted approval on an absolute threshold

> **Implemented** (T49, 2026-09-24): `scoring::bridging::{two_means, side_balanced,
> bridge_scores}` and `protocol::gate::{bridging_gate, supplementary_review}` with the
> provisional constants `TAU = 0.80`, `EPS = 0.02`, `APPEAL_GAP = 0.25`; `sim/`, the
> fixtures and the golden outputs regenerated. One amendment: appeal eligibility reads the
> *side gap* `|A_j − B_j|`, not `|f_j|` — the third review showed `|f_j|` falls as the
> camps become unequal (`docs/08` BRIDGE-009), the gap does not. Measured limits: a
> residual leak of 0.1–0.2 with 50–100 reviewers, and noisy side means with a minority
> side of about ten reviewers (`docs/02` §A.3).

**Choice.** The gate no longer reads the item intercept `b_j`. The weighted fit is
unchanged. After it, the reviewers are split into two sides by a deterministic 1-D
2-means on `f_u` (initialized at the minimum and maximum of `f_u`). For each item, the
model's predicted ratings `r̂_uj` are averaged within each side, and the bridge score is
the mean of the two side averages: each side counts once, whatever its size. The
pessimistic bootstrap-min, the uncertainty band and the D26 re-decision all apply to
this score. The threshold is absolute, on the scale of the declared probability:
provisionally `τ ≈ 0.80`, to be calibrated (T25). Appeal eligibility still reads
`|f_j|`.

**Why.** The intercept has two defects (`paper/`, §3.3–3.4). First, it is relative to
the batch: with `μ` unpenalized, `Σ_j b_j = 0` at every stationary point. Second, its
origin is a gauge fixed only by the penalties, which at the default `λ_b / λ_f = 5`
leaves 53–87% of the camp-size effect in the score for 50–3,200 reviewers. Predictions
avoid both: they do not depend on the gauge and they sit on an absolute scale.

In the paper's tests the side-balanced score brings the leak to between −0.08 and 0.00,
with 60/40 and 80/20 camps and 50–3,200 reviewers. It also keeps the consensus items
within ±0.01 whether they are scored in their batch, alone, or next to ten weak
decoys; on the same test the intercept moves from +0.09 to +0.32.

**Rejected.**
- Keeping the intercept and raising `λ_b / λ_f` with `√(n/S)`: a moving, data-dependent
  penalty.
- Imposing `Σ_j f_j b_j = 0`: the origin would then depend on the lean–quality
  correlation of the batch.
- The minimum of the two sides: it gives each side a veto over items it merely
  tolerates, while Level A is only the upstream filter and Level B decides.

---

## D33 — Evaluator score: leave-one-out difference score, odds-scale weights with shrinkage

> **Implemented (2026-09-25, T50):** `reputation::{loo_baseline, loo_scores, mean_score,
> odds_weight, EvaluatorParams}` (`γ = 35`, `k₀ = 100`, provisional); `honeypot::reviewer_skills`
> returns the mean leave-one-out difference score on the golden items; the review weight is
> `min(w_max, exp(γ·S_u·k_u/(k_u+k₀)))` (`probation::effective_review_weight`,
> `orchestrator::bridging_weights`) with `w_max = 3 × median` over the reviewers who carry
> weight (`orchestrator::epoch_weight_cap`), where it binds. `evaluator_score` (`σ(γ·BSS)`) is
> removed; `brier_skill_score` stays as the sim oracle only. AT-REP-02/04/05 pass
> (`evaluator_score.rs`). The scored items are still the golden ones: D35 is T52.

**Choice.** On every scored item the evaluator score is
`S_uj = (p̄_{−u,j} − o_j)² − (p_uj − o_j)²`, where `p̄_{−u,j}` is the weight-adjusted
mean forecast of the *other* panelists. `S_u` is its mean over the reviewer's scored
items. Weights are on the odds scale, with shrinkage toward zero:

```
w_u = exp( γ · S_u · k_u / (k_u + k_0) ),    k_0 ≈ 100,  γ ≈ 35
```

Here `k_u` is the number of scored items. At `γ ≈ 35`, a reviewer who is reliably 0.02
better than the crowd weighs double. The cap stays `w_max = 3 × median(w)`. The rule
replaces the ratio-form BSS and `E_u = σ(γ·BSS)`. It keeps D23's crowd baseline, minus
the reviewer being scored.

**Why.**
- *The ratio-form BSS is not proper* (`paper/`, Prop. 12). With one scored item the
  best report is `logit p* = logit q + 2 logit b`, so a dissenter is paid to move toward
  the crowd. Example: a reviewer believes 0.30 and the crowd says 0.65. Reporting 0.60
  earns +0.01 in expectation, reporting the truth earns −0.35.
- *The difference score is strictly proper* and gives exactly 0 to a reviewer who copies
  the crowd (Prop. 14).
- *The cap needs an unbounded scale.* It never binds on `E_u ∈ (0,1)` (Prop. 15), and
  relative weights `E_u / median(E)` stay bounded too. Odds weights are unbounded, so the
  cap works.
- *Shrinkage stops luck from buying weight.* With 16 scored items and one standard error
  of luck, a reviewer gets ×2.4 the normal weight without shrinkage and ×1.13 with it.

**Rejected.** Keeping the BSS and waiting for more items: its bias shrinks only as about
`0.23/m`. Relative weights: still bounded.

---

## D34 — Reputation dynamics: long-window mean plus a change detector

> **Implemented (2026-09-25, T51):** `reputation::{Cusum, CusumParams}` (`k = 0.03`,
> `h = 1.5`, provisional) and `probation::SkillTrack` — the running mean of the per-item
> scores is the weight's `S_u`, the CUSUM reads each score against that mean once the
> reviewer is out of probation, and an alarm restarts the track (probation, weight 0).
> `asymmetric_ema` is removed. AT-REP-07 passes (`change_detector.rs`): at most one alarm
> on a seeded honest stream of 10,000 items; a reviewer who starts flipping 20% of
> forecasts is caught within 100 items on nine of ten seeds (median 25; the tenth after
> 356 — at `k = 0.03` the drift is small and the tail long, a T25 calibration item).

**Choice.** The score used for the weights is a symmetric long-window mean of the
per-item scores. The fast fall of the asymmetric update is replaced by a one-sided CUSUM
on each reviewer's per-item scores, measured against the reviewer's own long-run mean.
On an alarm the reviewer returns to probation (D36). Provisional parameters: `k = 0.03`,
`h = 1.5`, to be calibrated (T25).

**Why.** The asymmetric update penalizes variance, not error. Its stationary level sits
far below the true mean. A cautious reviewer who is better than the crowd (true +0.009)
is held at −0.061. A reviewer who copies the crowd stays at exactly 0 and outranks them.
The update therefore rewards herding, the opposite of D6 (`paper/`, §5.5).

The CUSUM tracks the true mean and reacts only to a sustained drop. In simulation, with
`k = 0.03` and `h = 1.5`, it gives 0.07 false alarms per 1,000 scored items for an honest
reviewer. It catches a long-con reviewer who starts flipping 20% of forecasts after a
median of 36 scored items.

**Rejected.** The asymmetric update (above). A symmetric update without detection: it
reacts slowly to a long con.

---

## D35 — Scored outcomes: live items with randomized exploration; golden items at 5%

> **Implemented (2026-09-25, T52):** `protocol::exploration` — the beacon's draw
> (`explore_from_beacon`, `EXPLORATION_RATE = 0.05`, keyed on the admitted slot) sends a
> gate rejection through `lifecycle::Event::Explore` to `Explored`, the two pilot batches
> and `Measured`, never `ActivePool`; `outcome_of`/`record_outcome` feed every reviewed
> item's terminal into `probation::SkillTrack` — an observed outcome at weight `1/π_j`
> (`record_observed`), an unexplored rejection into the denominator only
> (`record_unobserved`) — and `FalseNegatives` counts the gate's false negatives. The
> change detector reads the unweighted scores. Evidence: `AT-REP-06` exact (the weighted
> score's expectation is `(b − q)² − (p − q)²` under a report-dependent gate; the bare
> observed score pays the paper's dissenter 0.08 to report 0.50 against 0.0045 for the
> truth) and by Monte Carlo (nine reviewers, 100,000 items: the weighted mean within 2.1
> standard errors of the full-information mean, the passed-only mean up to 11.6 off);
> `AT-PRO-07` (the draw reproduces from the beacon, two beacons share 0.24% of their
> draws, a participant's seed is refused). On the fixtures the explored real-health item
> is measured as a pass — a gate false negative — and the pool is unchanged. Grind-free
> with T37; `ε` provisional (T25).

**Choice.** Evaluators are scored on three kinds of item:
1. golden items, still 5% of the review queue;
2. every live item they reviewed that reaches Level B, once its outcome is known;
3. a random 5% of the items the gate rejects, drawn from the public beacon (D41) and
   sent to the pilot for measurement only.

The outcomes of the items in (3) are weighted by `1/0.05 = 20` (inverse probability), so
the expected score equals the score with every outcome observed. An explored item does
not enter the pool because of its pilot result. Entry still requires passing the gate or
a successful appeal (D8).

**Why.** Differences in skill between evaluators are small compared with the noise of a
single item (about 0.01–0.02 against about 0.1). Around 100 scored items per reviewer
are needed before the score means anything. The two options compare as follows at the
design scale:

| Scored items | Per reviewer per month | Time to 100 scored items | CUSUM reaction (D34) |
|---|---|---|---|
| Golden items alone (5%) | ≈ 0.3 | decades | ≈ 10 years |
| Live outcomes + exploration | ≈ 3.5 | ≈ 2.5 years | ≈ 10 months |

Scoring only the items the gate lets through would break properness, because the
forecast then influences whether its own outcome is observed. Randomized exploration
restores properness (Chen, Kash, Ruberry & Shnayder, 2014). Exploration also measures,
for the first time on real data, how many good items the gate rejects. It costs about 5%
of pilot capacity.

**Rejected.**
- Golden items alone: too slow.
- Golden items at 20%: about 5.5 years, +25% reviewer load, four times the golden-item
  supply, and golden items reused so often that they become recognizable.
- Live outcomes without exploration: not proper.

---

## D36 — Probation: 30 scored outcomes, then shrinkage

> **Implemented (2026-09-25, T50):** `probation::N_PROBATION = 30`; after it the odds weight
> of D33 is shrunk by `k_u/(k_u + 100)`.

**Choice.** A new evaluator pseudonym has weight 0 until it has 30 scored outcomes (was
200). After that, the shrinkage of D33 moves its weight away from 1 only as evidence
accumulates. Founders are unchanged.

**Why.** At the design scale, 200 scored outcomes take about five years even with D35.
Probation exists so that a pseudonym with no track record has no influence; 30 outcomes
are enough for that, and shrinkage handles the rest. Whitewashing is prevented by
non-rotatable pseudonyms (invariant #5), not by the length of probation.

**Rejected.** 200: years of zero weight for every newcomer.

---

## D37 — Latent DIF: anchor-reliability precondition; θ inside the likelihood as the target model

> **Precondition implemented (2026-09-25, T53):** `pilot::admit_anchors` refuses the
> latent re-check when the anchors' KR-20 on the batch's respondents (`irt::kr20`) is
> below `KR20_MIN = 0.90`; `revalidate_batch_latent` takes the anchors and computes θ
> itself, so a caller cannot vouch for a proxy the gate has not measured. The differential
> gap is reported as `MixtureDif::differential`, a diagnostic the verdict never reads.
> AT-DIF-11 passes on null batches drawn as the paper's (20 anchors refused, 60 accepted
> with no flag). The target model, θ inside the likelihood, is T54.

**Choice.** The latent-class re-check runs only if the anchors' KR-20, computed on the
batch's respondents, is at least 0.90 (about 40 anchors). Below that the batch is
refused, like the respondent (N) and item (K) floors. The *differential gap* (each
item's class gap relative to the batch's common class shift) is reported as a
diagnostic, never as the verdict.

The target model integrates θ and includes the anchors with class-invariant parameters,
so that a class-wide shift is no longer mistaken for DIF. The provisional 1.0 threshold
is re-derived on that model (T24/T25).

**Why.** Error in the ability proxy creates latent classes that do not exist (`paper/`,
Prop. 10). On null batches at N = 6,000 the production detector behaves as follows:

| Anchors | KR-20 | Result on null batches |
|---|---|---|
| 10 | 0.69 | flags clean items |
| 20 | 0.82 | flags clean items |
| 30 | 0.87 | passes, by a margin of only 0.01–0.06 |
| 60 | 0.93 | no spurious class |

The differential gap removes the artefact when few items are biased, but it inverts the
verdict in a campaign. With 6 of 8 items biased, the biased items score 0.02 and the
clean ones 1.86. It cannot decide.

**Rejected.** The raw gap without a precondition (false positives). The differential gap
as the verdict (inverted in a campaign). A threshold tuned to one anchor set.

---

## D38 — DIF is not bias: contested facts go to a balanced pool

**Choice.** Some items show DIF although their key is established by a primary source:
the dispute concerns knowledge of the fact, not the wording. Such an item is classified
as a *contested fact* instead of being discarded. Its classification follows the
evidentiary procedure on the source (`docs/02` §B.5), not a vote. Contested facts enter
a separate pool. A test draws them only in balanced sets, so that the test as a whole
favours no latent class (differential test functioning ≈ 0). Items that show DIF for
any other reason (wording, framing, nuisance content) are rejected as today.

**Why.** DIF means that the item measures a secondary dimension on which the classes
differ; whether that dimension is a nuisance is a judgment. In a civic bank, a true fact
on which one camp is systematically misinformed shows DIF by construction. For example,
75% versus 45% correct at equal knowledge is a log-odds gap of 1.30. Today such an item
would be discarded, and the appeal channel could never recover it. Balancing at test
level keeps the test neutral without banning contested facts.

**Rejected.** Rejecting every DIF item: a neutral bank that cannot contain contested
facts. Admitting contested facts without scoring them: loses the measurement.

---

## D39 — Coordination is detected on model residuals over long histories

> **Implemented (2026-09-25, T56):** `scoring::collusion::{ResidualHistory,
> coordination_clusters, CoordinationParams}` — residuals `r − r̂` recorded per reviewer
> and item across epochs; a pair read only from 30 shared items; a pair flagged at a
> residual correlation of at least `ρ_min = 0.7` with a permutation p-value ≤ 0.001 (999
> seeded permutations); average-linkage clusters over the flagged pairs. On the paper's
> dataset ported to Rust (`coordination.rs`): honest same-camp residual correlation
> +0.015, cross-camp +0.018, the jittered cartel +0.889; all 45 cartel pairs flagged and
> no honest pair, the cartel one cluster and every honest reviewer a singleton — while the
> raw rule chains the whole majority camp into one cluster. `ρ_min` is 0.7, not 0.5: at
> 0.5 four of the 18,000 honest pairs are flagged by chance. Parameters provisional (T25).
> The clusters constrain panel assignment (D40, T57); the sublinear discount stays in the
> engine for analysis.

**Choice.** Coordination between two reviewers is measured by the correlation of their
residuals (rating minus the bridging model's prediction) on the items both rated. The
histories accumulate across epochs. A pair is considered only once it shares at least 30
items, and its correlation is tested against a permutation null. Clusters use average
linkage, not connected components.

**Why.** Raw correlations cannot tell a cartel from honest reviewers who share a
position. In the paper's test:

| | Honest pairs, same camp | Cartel pairs | Honest pairs flagged when 90% of the cartel is caught |
|---|---|---|---|
| Raw correlation | +0.94 | +0.93 | 50.6% |
| Residual correlation | +0.02 | +0.88 | none |

Within one epoch, two reviewers share less than one item on average at the design scale
(`r²/m`), so detection has to use long histories.

**Rejected.** Pearson correlation on raw ratings with connected components (the current
rule): it flags honest like-minded reviewers, chains unrelated groups together, and is
evaded by jitter of σ = 0.05.

---

## D40 — A detected cluster constrains panel assignment, not weights

> **Implemented (2026-09-25, T57):** `protocol::review::assign_diverse` and the beacon
> entry points `assign_diverse_from_beacon` / `assign_extra_diverse_from_beacon` — the
> stratified draw excludes every cluster already seated, on the first panel and on the
> band's extra round; a stratum the constraint empties is filled by the nearest eligible
> reviewer on the axis. The weights take no cluster input (`bridging_weights`). On the
> paper's population (1,000 reviewers, a cluster of 50, panels of 9; `panel_diversification.rs`,
> 2,000 draws): no diversified panel holds two members of the cluster, every panel still
> spans the axis, while the uniform draw seats two or more on 6.6% of panels (paper 7.0%).

**Choice.** Members of a detected cluster are never assigned together: a panel holds at
most one member of each cluster. The protocol does not apply the sublinear weight
discount. This supersedes D7 for the protocol; the engine keeps the discount function for
analysis.

**Why.** Assignment is the per-epoch defence. Take 1,000 reviewers, a cluster of 50 and
panels of 9: uniform assignment puts two or more members of the cluster on about 7% of
panels, and three or more on about 0.8%. The constraint makes both zero, at no cost to
anyone's weight. The discount would instead cut every member, false positives included,
to 14% of their weight (√50/50). And splitting the cluster to evade detection restores
full weight anyway (`paper/`, Prop. 17).

**Rejected.** Wiring the √k discount into the bridging weights: it can be evaded and it
penalizes honest like-minded reviewers.

---

## D41 — Public randomness: commit-reveal now, a unique threshold signature after the DKG

**Choice.** The beacon feeds the lottery, reviewer assignment, honeypot placement,
exploration (D35) and sortition. It is produced by commit-reveal among consortium
members: each member commits before the deposit window closes and reveals after it, and
the beacon is the hash of all reveals. A member who does not reveal is excluded from the
signing set for that epoch and recorded publicly. Once the committees have a real
distributed key generation (T19), the beacon becomes a unique threshold signature over
the epoch number (drand-style). This supersedes D29.

**Why.** The current seed, derived from the checkpoint head, can be ground by whoever
orders the last deposits (T37).

Commit-reveal is available now, and its residual bias is small and visible. The last
revealer can withhold in order to choose between two outcomes. For example, it can
double a 0.9% chance of landing on a target's panel to 1.8%, once per epoch, at the cost
of a public non-reveal.

A threshold signature cannot be biased. Before T19, however, whoever deals the key can
predict every draw.

**Rejected.** Keeping the checkpoint-derived seed: it can be ground. A threshold
signature before T19: the dealer can predict it.
