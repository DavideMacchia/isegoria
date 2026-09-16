# Identity and enrollment

**Design assumption.** Nodes are pseudonymous IDs. No personal, demographic, or
affiliation information ever enters the system. The fact that behind each ID there is
exactly one real person is guaranteed by this layer, which must preserve anonymity at
every stage.

Guiding principle, from which everything else follows: **the state authenticates, but
does not issue.** CIE/SPID (national eID) only serve to prove "I am a real and unique
person"; the credential the node participates with is issued by a different party, and
the two do not talk to each other.

---

## The three properties the layer must guarantee

Without these, the engine's math (`02`) is decorative.

**(P1) Uniqueness.** One person → at most one active ID per role.

**(P2) Non-rotatability.** The ID must be permanent and non-reissuable. It is the most
often forgotten and most damaging requirement: if a person can abandon an ID with a
ruined reputation and get a new one, negative reputation does not exist (the Friedman
& Resnick result on cheap pseudonyms). A necessary operational consequence anyway:

```
probation period:  w_u = 0  for the first n_probation ≈ 200 judgments with known
                   outcome (they contribute to measuring E_u, not to determining
                   outcomes)
```

**(P3) Unlinkability.** The issuer must not be able to link the credential to the
derived IDs, nor the two role IDs of the same person to each other.

---

## Enrollment flow

```
 CIE            SPID           other eID (EUDI, passport)
  │  document     │  federated    │
  │  with NFC     │  identity     │
  └───────┬───────┴────────┬──────┘
          ▼                 ▼
     ┌─────────────────────────┐
     │  common anchor          │   tax code (codice fiscale)
     │  (one per person)       │   — to which both CIE and SPID are bound
     └───────────┬─────────────┘
                 ▼
     ┌─────────────────────────┐
     │  issuing committee      │   heterogeneous, threshold t-of-n
     │  (NOT the state)        │   computes the uniqueness label in a
     └───────────┬─────────────┘   distributed, encrypted form; no single
                 ▼                  actor reads or inverts it
     ┌─────────────────────────┐
     │  anonymous credential   │   severed from the identity
     └───────────┬─────────────┘
       ┌─────────┼─────────┐
       ▼         ▼         ▼
    propose    judge     answer         three DETERMINISTIC pseudonyms
   pseudonym  pseudonym pseudonym       unlinkable to each other or to the person
      1          2          3
```

---

## The three mechanisms, one per problem

### M1 — Multiple sources without double enrollment

**Risk.** The same person enrolls once with CIE and once with SPID.

**Solution.** All sources converge on a **canonical anchor**, in Italy the tax code
(both CIE and SPID are bound to it). From there a **uniqueness label** is derived with
a computation distributed across the committee. Whether you arrive from CIE or SPID
you get the same label; the second enrollment is recognized as a duplicate.

**Beware the brute-forceable space.** The tax code is derivable from name + date +
place of birth + sex: its space is small and enumerable offline. The label must
therefore be computed with a **threshold OPRF** (oblivious PRF with a key distributed
across the committee):

- no issuer learns the tax code (it is blinded in the OPRF)
- no issuer learns the label
- the user gets a deterministic label they **cannot compute alone** (the committee is
  required)
- uniqueness is verified by checking the label is not already in the set (or the user
  proves its freshness in ZK)

Adding a new source = writing an **adapter** that extracts the anchor and feeds it into
the same OPRF → label → credential pipeline. The core does not change. It is
consistent with the pluggable model of eIDAS 2.0 / the EUDI Wallet.

### M2 — The state cannot link

**Solution.** Whoever authenticates (CIE/SPID = the state) is distinct from whoever
issues (the committee). The state knows you enrolled, but:

- does not see the label (computed in distributed, encrypted form)
- does not see the pseudonyms
- does not see the votes

**Blind** issuance (blind signature or BBS+): the committee certifies but cannot
recognize the credential when it meets it in circulation. The threshold committee
requires the agreement of `t` of `n` heterogeneous issuers: no sub-threshold coalition
can link. Same philosophy as the storage consortium (`04`): security comes from
diversity.

### M3 — One role, one pseudonym, non-rotatable

**Solution.** From the credential three pseudonyms are derived with a **per-context
nullifier**:

```
nym_propose   = H(secret, "propose")
nym_judge     = H(secret, "judge")
nym_answer    = H(secret, "answer")
```

Properties of a nullifier, all necessary:

1. **always equal** for the same context → you cannot vote/propose twice as different
   identities
2. **not traceable** to the secret key → nobody links back to you
3. **different for different contexts** → the three pseudonyms are not linkable to each
   other

Every action is accompanied by a zero-knowledge proof that the pseudonym derives from
a valid credential, without revealing which one.

**Closing whitewashing.** The three pseudonyms are deterministic: for you there exists
**only one** possible judge-identity, always the same. You cannot burn its reputation
and derive a fresh one. The uniqueness label (M1) prevents a second credential; the
determinism prevents a second pseudonym per role. Negative reputation becomes
inescapable without sacrificing anonymity.

---

## Pseudonymous does not mean anonymous

An ID that has proposed 200 questions is **deanonymizable statistically** even with
perfect cryptography. Mandatory countermeasures if the threat model includes a
state-level actor:

| Leakage channel | Mitigation |
|---|---|
| Stylometry on the question text | Mandatory structural template + automatic linguistic normalization before publication: every item comes out in the same voice |
| Timing of submission and voting | Batched publication with random delay; queue mixing; no precise timestamp in the public log |
| Topic choice (one ID only on one domain) | Per-author domain quotas; partial assignment of topics by lottery |
| Free metadata, source formatting | No public free-text field; citations only in structured form (act identifier, not an arbitrary URL) |
| Author↔evaluator correlation | M3 — separate role pseudonyms |

---

## Residual gaps (not to be hidden)

**F1 — Enrollment metadata.** SPID tells its identity provider that you authenticated
*with the enrollment service*: the fact that you enrolled is visible (not what you
do). **CIE via NFC is preferable to SPID**: reading the chip verifies the document
against the Ministry's CA without a call revealing *where* you are enrolling. It does
not eliminate the gap, it narrows it. Offer both but push CIE.

**F2 — Cross-source double enrollment for foreigners.** Someone with both an Italian
tax code and a foreign passport could enroll twice, because the two anchors live in
separate spaces. A niche case with imperfect mitigations.

---

## Cost of proposing

The "bond" for proposing is in **reputation and rate limits**, never money (see `01`
D13). The per-person technical limit is a **rate-limiting nullifier**: one token is
generated for each slot of the epoch, `H(key, "propose", epoch, i)` with `i` below the
quota, and you prove in ZK you are within the limit. Reusing a token (exceeding the
quota) reveals the secret key by construction: the limit enforces itself. But the
quota is not the real brake on spam — see the lottery in `05` and `01` D10.

---

## Implementation notes

- Prefer mature, audited building blocks: Semaphore for group nullifiers; BBS+
  schemes for credentials with selective disclosure; a threshold OPRF for anchoring.
  Bespoke cryptography is allowed when it serves the design (e.g. composing a
  threshold scheme from a vetted single-party one), but only kept small, built on
  audited primitives, and tested against known answers — not invented from scratch.
- eIDAS 2.0 mandates every EU state a digital identity wallet by the end of 2026, with
  selective disclosure: it is the infrastructure to lean on in steady state.
- Document verifiers (adapters): CIE via NFC (CieID / chip reading), SPID via
  federated IdPs, EUDI Wallet, ICAO 9303 e-passport. Each is an adapter toward the
  same pipeline.
