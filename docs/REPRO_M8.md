# Reproducing M8

The committed generation-6 calibration and release verdict are immutable
evidence. Do not reopen or overwrite the sealed court as an ordinary test.

Run formatting and the focused implementation courts:

```text
cargo fmt --all -- --check
cargo test -p vice-evidence --lib multicolor
cargo test -p vice-topology --lib rag
cargo test -p vice-topology --lib multidcel
cargo test -p vice-render --lib junction
cargo test -p vice-opt --lib multiregion
cargo test -p vice-bench --lib oracle::paint
cargo test -p vice-core --lib m8
cargo test -p vice-svg --lib multicolor_faces
cargo test -p vice-verify
```

Run warnings-as-errors and the M7 regression barrier:

```text
cargo clippy -p vice-evidence -p vice-topology -p vice-render -p vice-opt -p vice-core --lib --tests -- -D warnings
cargo clippy -p vice-bench --lib --tests -- -D warnings -A clippy::unnecessary-unwrap
cargo test -p vice-bench --lib
```

The one clippy allowance names a pre-existing M7 diagnostic in
`vice-bench/src/m7/measure.rs`; it does not apply to M8 code.

The release operator is `m8-court`:

```text
cargo build --release -p vice-bench --bin m8-court
target/release/m8-court measure --scope calibration --variants 650 --shard-index 0 --shard-count 4 --out <calibration-shard.json>
target/release/m8-court merge --inputs <each-shard> --out <court.json>
target/release/m8-court calibrate --report <calibration-court.json> --out <calibration.json>
target/release/m8-court release --report <sealed-court.json> --calibration configs/M8_PRODUCTION_CALIBRATION.json --out <release.json>
```

On Windows use `target\release\m8-court.exe`. `--inputs` is repeated once per
shard. Merge refuses missing, duplicate or identity-incompatible shards.

For a non-sealed determinism replay, compare the canonical `rows` arrays from
one-worker and merged two-worker smoke runs after excluding only `runtime_ms`.
The committed result and report hashes are in
`docs/gt/M8_REPLAY_ATTESTATION.json`.
