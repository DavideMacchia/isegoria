# scoring fuzz targets (T62)

`cargo fuzz` targets for the entry points of `scoring` that take input built elsewhere.
They live outside the main workspace because libFuzzer needs a nightly toolchain; the
same entry points are covered on every push by `tests/malformed_ratings.rs` (stable,
proptest). The audit they belong to is recorded in `docs/12-panic-audit.md`.

| Target | Entry point | Asserted besides "no panic, no abort" |
|---|---|---|
| `bridging` | `bridging::fit`, `bridging::bridge_scores` on a `Ratings` assembled field by field: indices past `n`/`m`, any rating, any number of weights of any value | an error exactly when `Ratings::validate` refuses the input; a validated input always fits |

## Running

```sh
rustup toolchain install nightly
cargo install cargo-fuzz
cd crates/scoring
cargo +nightly fuzz run bridging -- -max_total_time=600
```

A crash is saved under `fuzz/artifacts/<target>/`; replay it with
`cargo +nightly fuzz run <target> fuzz/artifacts/<target>/<file>`, and turn it into a
regression test in `tests/` before fixing it. `corpus/`, `artifacts/` and `target/` are
git-ignored.
