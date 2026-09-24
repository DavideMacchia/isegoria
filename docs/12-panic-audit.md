# Isegoria — Panic audit and fuzzing report (T44)

| | |
|---|---|
| **Task** | T44 in `docs/10-roadmap.md`: fuzz every byte decoder, classify every `unwrap`/`expect`/`assert` in `src/`, and turn the ones that external input can reach into errors |
| **Audited** | master `6c61263`, 2026-09-24; the inventory re-checked after rebasing onto T41 (`dd7a6b6`): unchanged in `protocol` and `scoring`; and after merging T48 (`57bdca1`): one new internal-invariant site in `scoring` (§2.3) |
| **Changed** | `crates/network`, `crates/identity` |
| **Classified only** | `crates/protocol`, `crates/scoring`: being changed in parallel at the time (T41, the scoring half of T42, T48), so their sites are recorded here and not touched |
| **Status of the "done when"** | no panic on arbitrary bytes: met for every decoder of `network` and `identity` (§3, §4); classification recorded: §2 |

## 1. What was looked for, and the classes

Two things. First, every `unwrap()`, `expect(..)`, `assert*!`, `panic!`, `unreachable!`
outside `#[cfg(test)]`. Second — found by fuzzing, or by reading the decoders once the
fuzzer pointed at them — every other way hostile input can stop the process: an
arithmetic overflow (a panic in debug builds), an allocation sized from the input, an
out-of-range index, and unbounded work inside a dependency that parses untrusted bytes.

Each site is in one of four classes:

| Class | Meaning | Treatment |
|---|---|---|
| **Internal invariant** | holds by construction: a failure is a bug in this code | kept; the message states the invariant |
| **Configuration** | a parameter the operator picks at setup (a committee's shape, the layout of one's own erasure code) | kept, documented as `# Panics` |
| **Caller precondition** | the shape of an argument the calling code builds, not one another party sends | kept; listed for T46 (validated boundary types) |
| **External input** | reachable with bytes or values another party controls | turned into an error, or the function made total |

"External" is judged against the protocol, not today's in-process wiring: the anchor
comes from an identity document, receipts, shards, checkpoints and signatures come from
other nodes, and a quorum's answers come from committee members.

## 2. Inventory

35 sites outside `#[cfg(test)]` at `6c61263` (86 counting test modules): identity 28,
network 4, protocol 2, scoring 1. After this audit: 30, none of them reachable from
external input. T48, merged afterwards, adds one internal-invariant site in `scoring`
(§2.3), for 31. Sites are named by function; line numbers drift.

### 2.1 `identity`

| Where | Site | Class | Status |
|---|---|---|---|
| `credential::Credential::request_issuance` | `commit_to_messages(..).expect` | internal: one committed message at constant index 0 < 2 | kept |
| same | `sc.response(..).expect` | internal: two witnesses, two blindings | kept |
| `credential::pok_challenge` | 3 × `serialize_compressed(..).unwrap()` | **external**: the issuer computes it over a request it received (`verify_request`). Writing to a `Vec` cannot fail, but a verification path should not rest on that | **made fallible**; the issuer answers `InvalidProofOfKnowledge`, the holder side keeps one `expect` on its own data |
| `credential::ThresholdIssuer::new` | `assert!(1 <= t <= n)` | configuration | kept, `# Panics` |
| same | `deal_random_secret(..).expect` | internal: follows from the assert | kept |
| `credential::ThresholdIssuer::threshold_sign` | `comm_zeros[..].get(&i).unwrap()` | internal **today**: the committee runs in one process, and every member's zero-sharing commitments cover all the others | kept; **becomes external** when members are remote (DKG/transport, future): must then be an error |
| `credential::setup_base_ot` | 6 × `.unwrap()` on the base-OT rounds | internal **today**: trusted in-process setup | kept; same caveat as above |
| `enrollment::VoprfOracle::new` | `new_from_seed(..).expect` | internal: a 32-byte seed with a fixed info string always derives a key (failure has negligible probability) | kept |
| `enrollment::VoprfOracle::label` | `blind(..).expect`, `finalize(..).expect` | **external**: the anchor's bytes come from the enrollment adapter. RFC 9497 caps an input at `u16::MAX` bytes, and a longer anchor made `finalize` fail — a panic, under a message claiming the proof failed | **fixed (F8)**: the oracle is total; both `expect`s are now internal and say why |
| `nullifier::context_generator` | 2 × `.expect` on hash-to-curve | internal: constant domain tag and curve configuration | kept |
| `nullifier::NullifierProof::id` | `serialize_compressed(..).unwrap()` | internal: writing a verified point to a `Vec` | kept |
| `nullifier::challenge` | `contribute(..).expect` and 3 × `serialize_compressed(..).unwrap()` | **external**: `verify` computes it over a proof it received | **made fallible (F9)**; `verify` answers `false`, `prove` keeps one `expect` on its own proof |
| `nullifier::prove` | `PoKOfSignatureG1Protocol::init(..).expect`, `gen_proof(..).expect` | internal: the holder's own credential, and `IssuerPublic` is opaque and always carries two message generators | kept |
| `oprf::ThresholdOprfOracle::new` | `assert!(1 <= t <= n)` | configuration | kept, `# Panics` |
| `oprf::ThresholdOprfOracle::label` | `label_with_quorum(..).expect` | internal: the first `t` members are the oracle's own honest shares | kept |

### 2.2 `network`

| Where | Site | Class | Status |
|---|---|---|---|
| `anchoring::serialize` | `to_writer(..).expect` | internal: a timestamp this module built, written to a `Vec` | kept |
| `erasure::encode` | `ReedSolomon::new(..).expect` | configuration: the encoder picks the layout of its own data | kept, `# Panics` |
| same | `r.encode(..).expect` | internal: equal-length shards built just above | kept |
| `log::TransparencyLog::append` | `last().unwrap()` | internal: an entry was just pushed | kept |

### 2.3 `protocol` and `scoring` (classified only)

| Where | Site | Class | Status |
|---|---|---|---|
| `protocol::randomness::Beacon::seed` | `d[..8].try_into().expect` | internal: SHA-256 yields 32 bytes | — |
| `protocol::review::assign_reviewers` | `stratum.choose(..).unwrap()` | internal: every stratum is non-empty (`lo < n`, `hi >= lo + 1`) | — |
| `scoring::bridging::Ratings::with_weights` | `assert_eq!(weights.len(), self.n)` | caller precondition: the weights are the protocol's own `E_u` | T46 |
| `scoring::bridging::fit` (T48) | `best.expect("at least one start")` | internal: the loop runs `n_starts.max(1)` times, and the first start always sets `best` | — |

Not an `unwrap`, but noted while reading: `scoring::bridging::Ratings::from_dense` indexes
`mask[u][j]` and `r[u][j]` with the width of row 0, so a ragged or short matrix panics.
A caller precondition today; T46's `ValidatedRatings` makes it unrepresentable. The
public entry points of `scoring` and `protocol` were not fuzzed here (§6).

## 3. External-input crashes found, and fixes

Each was reproduced first (stable toolchain, debug build, the way `cargo test` runs),
then fixed, then pinned by a regression test.

| # | Input | Effect | Where the fault is | Fix | Regression test |
|---|---|---|---|---|---|
| F1 | an `.ots` varint of more than ten bytes (e.g. as the version) | **panic** in debug builds: `attempt to shift left with overflow` | `opentimestamps` 0.2.0 `ser.rs::read_uint` | bounded pre-scan in `anchoring` refuses varints longer than 9 bytes before the library parses | `anchoring::tests::an_overlong_varint_is_refused` |
| F2 | an unknown attestation declaring 2⁶² bytes | **process abort**: `memory allocation of 4611686018427387904 bytes failed` (uncatchable) | library `read_fixed_bytes(len)` allocates before reading | pre-scan requires the declared length to fit in the proof | `…::an_attestation_longer_than_the_proof_is_refused` |
| F3 | a chain of `Hexlify` operations | message grows ×2 per input byte: 20 bytes of input → 32 MiB in 0.7 s; ~30 → out of memory | library executes operations while parsing, with no cap on a result | pre-scan caps every operation's result at 4096 bytes — the limit of python-opentimestamps, the reference implementation | `…::a_message_longer_than_4096_bytes_is_refused` |
| F4 | many fork branches over a large message | each branch clones the message and every step stores its output: memory hundreds of times the input, even with results capped | library design | pre-scan charges every clone and stored output against a 1 MiB budget | `…::fork_amplification_is_bounded` |
| F5 | an erasure layout with `data_shards + parity_shards` overflowing | **panic** in debug builds inside `ReedSolomon::new` | `reed-solomon-erasure` 6.0.0 sums the counts unchecked | `erasure::check_layout` validates the claimed counts first → `RecoverError::InvalidLayout` | `shard_authentication.rs::an_impossible_layout_is_refused_not_sized` |
| F6 | an `orig_len` of `usize::MAX` | **panic**: `capacity overflow` (smaller huge values: abort) | `erasure::reconstruct` sized its buffer from the claim | the claim must fit in the recovered shards → `InvalidLayout`; nothing is allocated from it before that | same test |
| F7 | `merkle_proof` with an index ≥ the leaf count | **panic** (index out of bounds), or for some indices a silent proof of no leaf | `merkle::merkle_proof` | returns `Option`; `None` for an out-of-range index | `integrity.rs::merkle_inclusion_proof_verifies` |
| F8 | an anchor longer than 65535 bytes | **panic** in `VoprfOracle::label` | RFC 9497's input limit, reached through an `expect` | a longer anchor is hashed to 64 bytes first and its label carries its own tag, so the oracle is total and cannot collide a long anchor with a short one; anchors up to the limit get the same label as before | `voprf_oracle.rs::an_anchor_beyond_the_rfc_9497_limit_still_gets_a_stable_label` |
| F9 | a received nullifier proof or issuance request | would panic if serializing the transcript ever failed (it cannot today) | `nullifier::challenge`, `credential::pok_challenge` | fallible; the verifier answers `false` / `InvalidProofOfKnowledge` | covered by the no-panic properties (§5) |

**The OTS pre-scan.** `anchoring::within_bounds` walks the same grammar as the library's
parser — header, digest, step tree, attestations, no trailing bytes — without executing
anything, and tracks the message length the library would build at each step. It is
deliberately a byte-for-byte mirror: refusing is always safe (such a proof is `Invalid`),
and a proof it accepts makes the library read exactly the same bytes, with its depth
(256, the library's own limit), every operation's result (≤ 4096 bytes), every declared
length (inside the proof), the total memory it materializes (≤ 1 MiB) and the proof itself
(≤ 64 KiB) bounded. The two real proofs from the library's own test vectors
(`network/tests/fixtures/ots/`, including a full Bitcoin-attested one) pass it. Earlier
audits (docs/08 NET-008) took the library's recursion limit as the bound; it bounds depth
only, not lengths, allocations or work. `opentimestamps` 0.2.0 is the latest release;
F1–F4 are worth reporting upstream.

## 4. Fuzz targets

Eight `cargo fuzz` targets, in `crates/{network,identity}/fuzz/` (outside the workspace:
libFuzzer needs nightly). Each README says how to run them. Besides "no panic, no abort,
bounded memory", each asserts a property of its entry point:

| Target | Entry point | Also asserted |
|---|---|---|
| `network/ots_verify` | `OtsAnchor::verify` on arbitrary receipt bytes (AT-NET-07) | — |
| `network/erasure` | `reconstruct`, `reconstruct_verified`: hostile shards and layouts; genuine encodings with losses and corruptions | a genuine encoding recovers exactly when `data_shards` authentic shards survive, to the original bytes |
| `network/checkpoint` | `CheckpointClient::ingest` / `ingest_with_log` over sequences of honest and forged checkpoints | the trusted height never decreases; only `Accepted` changes the trusted checkpoint; acceptance needs a threshold of distinct member signatures |
| `network/merkle` | `merkle_proof`, `verify_proof` | a proof exists exactly for an in-range leaf, and verifies |
| `identity/oprf_quorum` | the threshold OPRF with any committee shape, anchor and claimed quorum | a label exactly for `≥ t` distinct committee members, equal for every valid quorum |
| `identity/enrollment` | `EnrollmentRegistry::enroll` through the reference, VOPRF and threshold oracles | the same person via the other source is a duplicate |
| `identity/voprf_wire` | RFC 9497 messages decoded from arbitrary bytes, at the server and at the client | no decoded evaluation verifies without the server key |
| `identity/nullifier_proof` | `nullifier::verify` of proofs decoded from arbitrary bytes or spliced into a genuine one | only the untouched genuine proof verifies, for its own role and context |

Two targets reach private code through `--cfg fuzzing` (set by cargo-fuzz):
`ThresholdOprfOracle::fuzz_label_with_quorum` and `NullifierProof::{to_bytes, from_bytes}`,
a test encoding and not a wire format. Normal builds do not contain them.

**Runs recorded for this audit** (nightly `cargo-fuzz` 0.13.2, libFuzzer with
AddressSanitizer, 4 cores, 2 GiB RSS limit):

| Target | 15 min (executions) | 3 min on the committed code (executions) | Crashes |
|---|---:|---:|---:|
| `network/ots_verify` (seeded with the two fixture proofs) | 10 426 667 | 2 753 350 | 0 |
| `network/erasure` | 1 603 278 | 352 562 | 0 |
| `network/checkpoint` | 307 814 | 47 438 | 0 |
| `network/merkle` | 12 924 232 | 2 396 733 | 0 |
| `identity/oprf_quorum` | 1 245 236 | 234 811 | 0 |
| `identity/enrollment` (`-max_len=70000`) | 77 016 | 16 198 | 0 |
| `identity/voprf_wire` | 5 529 289 | 1 224 479 | 0 |
| `identity/nullifier_proof` | 91 962 | 18 663 | 0 |

The 15-minute runs predate two final edits — `pok_challenge` made fallible, and the
`merkle` harness's input turned into a named struct — so every target was run again on
the committed code. Peak memory was about 1 GB at most (`ots_verify`, ASan included).
The corpora are not committed; `ots_verify` restarts from the fixtures.

**Is the harness able to find these bugs?** With the pre-scan disabled, `ots_verify`
reached F2 (an out-of-memory `calloc` of ~40 GB) after 29 executions, starting from the
two fixture proofs. The only failures while the targets were written (60-second smoke
runs) were harness bugs, not findings: `erasure` counted a shard as tampered after two
flips of one byte had restored it, and `checkpoint` expected `ingest_with_log` to compare
the very first checkpoint with the log (it trusts it on first use, as specified; later
ones are checked).

## 5. Regression net on every push

The fuzz targets run on demand; CI runs the stable property tests that cover the same
entry points:

- `network/tests/hostile_input.rs`: arbitrary bytes and edited genuine proofs through
  `verify` (4096 cases each: with the pre-scan disabled, most runs abort); hostile shard
  sets and layouts; recovery exactly when enough authentic shards survive; forged
  checkpoint signatures and indices; Merkle proofs for any index.
- `identity/tests/hostile_input.rs`: arbitrary codice-fiscale strings (realistic, arbitrary
  Unicode, whitespace, and past the RFC 9497 limit) through every oracle; the RFC 9497
  messages decoded from arbitrary bytes.
- `identity::nullifier::proptests::hostile_proof_bytes_never_verify` and the T42
  byte-flip properties in `credential` and `nullifier`.
- One unit test per finding (§3).

## 6. Left open

- **`scoring` and `protocol`**: fuzz their public entry points and act on §2.3 (T46 covers
  the caller preconditions). Not done here because those crates were being changed in
  parallel.
- **Network codecs**: none exist yet. Transport (T18) will add wire formats for
  checkpoints, signatures, receipts, shards, nullifier proofs and OPRF partials; each needs
  a fuzz target of the same kind, and the `credential` sites marked "becomes external"
  above turn into errors when the committee goes remote.
- **Fuzzing in CI**: the targets are not built by CI (they need nightly). A scheduled job
  running each for a few minutes would keep them from rotting.
- **Noticed, not panics**: `Consortium::new` accepts a threshold of 0, which accepts any
  checkpoint with no signature (a configuration footgun); `ingest_with_log` trusts the first
  checkpoint on first use, so a local log that does not extend it is reported only at the
  next checkpoint, as `LocalLogDiverged`.
