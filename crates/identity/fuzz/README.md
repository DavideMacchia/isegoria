# identity fuzz targets (T44)

`cargo fuzz` targets for the entry points of `identity` that take untrusted input. They
live outside the main workspace because libFuzzer needs a nightly toolchain; the same
entry points are covered on every push by `tests/hostile_input.rs` and the in-crate
property tests (stable, proptest). The audit they belong to is recorded in
`docs/12-panic-audit.md`.

Two targets need code that is private in normal builds. cargo-fuzz compiles with
`--cfg fuzzing`, which exposes `ThresholdOprfOracle::fuzz_label_with_quorum` and
`NullifierProof::{to_bytes, from_bytes}` (a test encoding, not a wire format).

| Target | Entry point | Asserted besides "no panic, no abort" |
|---|---|---|
| `oprf_quorum` | the threshold OPRF with an arbitrary committee shape, anchor and claimed quorum | a label comes back exactly for a quorum of at least `t` distinct members of the committee, and equals the label of every other valid quorum |
| `enrollment` | `EnrollmentRegistry::enroll` of arbitrary codice-fiscale strings through the reference, VOPRF and threshold oracles | the same person via the other source is a duplicate |
| `voprf_wire` | the RFC 9497 messages decoded from arbitrary bytes: a blinded element at the server, an evaluation and a proof at the client | no decoded evaluation verifies without the server key |
| `nullifier_proof` | `nullifier::verify` of a proof decoded from arbitrary bytes, or from a genuine proof with a window overwritten | only the untouched genuine proof verifies, for its own role and context |

## Running

```sh
rustup toolchain install nightly
cargo install cargo-fuzz
cd crates/identity
cargo +nightly fuzz run oprf_quorum -- -max_total_time=600
# anchors past the RFC 9497 input limit need longer inputs:
cargo +nightly fuzz run enrollment -- -max_len=70000 -max_total_time=600
```

A crash is saved under `fuzz/artifacts/<target>/`; replay it with
`cargo +nightly fuzz run <target> fuzz/artifacts/<target>/<file>`, and turn it into a
regression test before fixing it. `corpus/`, `artifacts/` and `target/` are git-ignored.
