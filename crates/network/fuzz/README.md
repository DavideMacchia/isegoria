# network fuzz targets (T44)

`cargo fuzz` targets for the entry points of `network` that take untrusted input. They
live outside the main workspace because libFuzzer needs a nightly toolchain; the same
entry points are covered on every push by `tests/hostile_input.rs` (stable, proptest).
The audit they belong to is recorded in `docs/12-panic-audit.md`.

| Target | Entry point | Asserted besides "no panic, no abort" |
|---|---|---|
| `ots_verify` | `OtsAnchor::verify` on arbitrary receipt bytes (AT-NET-07) | bounded time and memory (libFuzzer's `-rss_limit_mb`, `-timeout`) |
| `erasure` | `reconstruct`, `reconstruct_verified` on hostile shards and layouts, and on genuine encodings with losses and corruptions | a genuine encoding recovers exactly when `data_shards` authentic shards survive, to the original bytes |
| `checkpoint` | `CheckpointClient::ingest` / `ingest_with_log` over sequences of honest and forged checkpoints | trusted height never decreases; only `Accepted` changes the trusted checkpoint; acceptance needs a threshold of distinct member signatures; a threshold of honest signatures is never `InsufficientSignatures` |
| `merkle` | `merkle_proof`, `verify_proof` with arbitrary leaves, indices and proofs | a proof exists exactly for an in-range leaf, and verifies |

## Running

```sh
rustup toolchain install nightly
cargo install cargo-fuzz
cd crates/network
cargo +nightly fuzz run ots_verify -- -max_total_time=600
```

`ots_verify` starts faster from the real proofs in `../tests/fixtures/ots/`:

```sh
mkdir -p fuzz/corpus/ots_verify && cp tests/fixtures/ots/*.ots fuzz/corpus/ots_verify/
```

A crash is saved under `fuzz/artifacts/<target>/`; replay it with
`cargo +nightly fuzz run <target> fuzz/artifacts/<target>/<file>`, and turn it into a
regression test in `tests/` before fixing it. `corpus/`, `artifacts/` and `target/` are
git-ignored.
