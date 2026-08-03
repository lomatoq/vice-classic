# Reproducing the current M8 checkpoint

The checkpoint is a development foundation, not an M8 release court.

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
```

Run warnings-as-errors on the changed production crates:

```text
cargo clippy -p vice-evidence -p vice-topology -p vice-render -p vice-opt --lib --tests -- -D warnings
cargo clippy -p vice-bench --lib --tests -- -D warnings -A clippy::unnecessary-unwrap
```

The single clippy allowance names a pre-existing M7 `unnecessary_unwrap`
diagnostic in `vice-bench/src/m7/measure.rs`; it is outside the M8 change and
is not hidden from the status. Remove the allowance only in a dedicated M7
cleanup change.
