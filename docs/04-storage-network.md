# Storage and network

The property we need is not "distributed" in the abstract, it is: **nobody can delete
or rewrite questions and votes, and nobody can falsify the scores without it being
visible**. This is what makes the network independent of a government. The question is
which architecture achieves it at the lowest cost.

**Final choice.** A P2P network of signed logs (gossip + DHT + append-only + CRDT),
with a consortium of a few dozen heterogeneous signers as the backbone, erasure coding
for durability, anchoring to a public chain for long-term immutability. **No global
permissionless consensus.**

---

## Why not a permissionless blockchain

- **Cost and latency.** Every write costs and has seconds/minutes of latency.
- **Public by construction.** Everything is visible to everyone forever: questions
  under review must stay encrypted until publication, and voting patterns on a public
  chain would be a gift to anyone wanting to deanonymize.
- **Computation too heavy.** Factorization and IRT do not run on-chain; you would end
  up keeping the data on the chain and doing the math elsewhere.
- **Consensus is almost unnecessary.** Writes almost never conflict: two different
  proposals get added, two votes get summed. The only delicate case, double-voting, is
  solved with the nullifier (`03`), not with consensus. Removing global consensus
  removes the bulk of the cost and slowness.

Anti-censorship comes from the **diversity of operators** and from the
**reproducibility of the computation**, not from permissionless consensus.

---

## The four P2P structures (non-blockchain)

They combine, they do not exclude each other. They cleanly separate *transporting
data* from *agreeing on an order* (consensus, the expensive part we avoid here).

| Structure | Purpose |
|---|---|
| **Gossip / epidemic** | propagate questions and votes: each node repeats to its neighbors until everyone knows |
| **DHT** (distributed hash table) | find content: given the hash, which node has the object (like BitTorrent) |
| **Signed append-only logs** | immutability: an add-only register, each row containing the previous one's fingerprint; altering one breaks the chain visibly |
| **CRDT** | converge copies edited in parallel, with no arbiter (collaborative editors) |

How they fit together: questions and votes are objects identified by their hash. The
DHT is the hash→node registry. Gossip is the courier. The signed log is what makes it
all immutable — it is the "transparency log", which lives distributed on every node,
not on a server.

Real projects already doing this: the Bluesky protocol (per-user signed logs +
rebuildable indexes), Secure Scuttlebutt (a signed-log, gossip social network that
works offline), Automerge/Yjs (mature CRDTs). None is a blockchain.

---

## The consortium as backbone

A pure P2P network has an Achilles' heel: a poorly-replicated datum can vanish when
the node holding it goes offline. The consortium solves this: a few dozen operators
**heterogeneous, always-on, in different jurisdictions** each keep a full copy and
periodically sign the state. They are the stable backbone; the citizens' nodes hook on
around them.

### Why few signers

The machines that sign the state must agree, and BFT agreement costs ~n² messages:

| Signers | Messages/agreement | Assessment |
|---|---|---|
| 4–50 | 16–2,500 | efficient |
| 100 | 10,000 | still manageable |
| 300 | 90,000 | slow |
| 1,000 | 1,000,000 | impractical |
| 1,000,000 | 10¹² | impossible |

Beyond a hundred, coordination becomes slow. **But few signers does not mean a small
or poorly-distributed network**: security comes from the diversity of who controls the
machines, not from the number. 30 operators chosen for being diverse (universities,
NGOs, opposing outlets, in different countries) are harder to corrupt all at once than
10,000 servers in the same data center. To censor, a government would have to compel
entities under different laws simultaneously, and one that refuses is enough for the
tampering to become visible.

### Consortium selection and differentiation

**Differentiation (easy, it is math).** You need not declare that an operator is
different: you measure it from behavior correlation (the same tool as bridging). Two
operators that always sign the same things at the same moments are a single block even
if on paper they are two entities. A consortium with two overly-correlated operators
has a problem visible to everyone.

**Selection (hard, no purely technical solution).** A combination of:
- reserved seats per category, including categories of **declaredly opposite**
  orientation (capturing one is not enough)
- an entry deposit against fake identities
- a per-category cap against buying seats

The **two ultimate defenses** matter more than any entry rule:
1. **Reproducibility of the computation** — a signer signs the result of a public
   deterministic computation; if it cheats, anyone redoes the math and unmasks it. It
   need not be chosen well to be caught.
2. **Freedom to fork** — if the consortium betrays, the data lives on every node and
   the code is open: the community takes the whole history, abandons that consortium,
   and restarts with a different set of signers. Not the barrier to entry, but the
   freedom to leave, keeps the consortium honest.

System parameters and meta-level composition: **stratified sortition** (see `05`), not
voting.

---

## Durability: erasure coding

The intuition "more reliable = more copies = more expensive" is wrong. With erasure
coding each datum is split into fragments scattered across many nodes, and only a
fraction is needed to reconstruct it. At equal storage it beats replication. With 30%
churn (a node unreachable 30% of the time):

| Method | Storage | Availability |
|---|---|---|
| replication ×3 | 3× | 0.973 |
| replication ×5 | 5× | 0.998 |
| erasure (10,20) | 2× | 0.983 |
| erasure (10,30) | 3× | ~1.000 |

`erasure (10,30)` uses 3× and is safer than 12 full copies. And durability no longer
lives only on the signers but on **all** nodes, even the light ones that come and go.
Slower to write (encoding + distribution) and read (fragment gathering), but
**simultaneously more distributed and more reliable**.

---

## Long-term immutability: anchoring to a public chain

Nothing runs on a blockchain, but every so often (an hour, a day) a single fingerprint
summarizing the whole network state is published on Bitcoin/Ethereum. It costs a few
cents each time. The gain: to rewrite the network's history an attacker would have to
rewrite *also* the most expensive public chain in the world. It turns "nobody can
falsify the past" from a consortium promise into a fact anchored to a chain the
consortium does not control. Technique: OpenTimestamps or equivalent. **It is the
addition with the best value/slowness ratio.**

---

## Node types

| | Person-nodes | Signer-nodes (consortium) | Light-nodes |
|---|---|---|---|
| What they are | pseudonymous participants | always-on servers | any client |
| What they do | propose, judge, answer | full copy + sign the state | slice of data + verify signatures |
| How many | unlimited | a few dozen (cap ~n²) | unlimited |
| More of them = | safer | slower | slows nothing |
| Security from | quantity + independence | diversity of who controls them | — |

Analogy: few stable trackers, millions of clients (BitTorrent). The roles are in
layers, not in competition.

---

## Future hardenings (order)

See `01` D14. In short:

1. **Anchoring** (above) — integrity, low cost.
2. **Erasure coding** (above) — durability + distribution.
3. **Multiple consortia counter-signing each other** in different jurisdictions —
   distribution of trust, slower cross-coordination.
4. **Cryptographic proofs of the computation** (zk) — the epoch produces a succinct
   proof that the scores are correct; anyone verifies it in an instant without
   downloading the data or re-running it. A mature phase, technically demanding.

**Do NOT adopt:** permissionless consensus with an economic stake (proof-of-stake). It
is the maximum theoretical distribution but the stake is money/tokens: it reintroduces
wealth-based access, the problem to avoid.
