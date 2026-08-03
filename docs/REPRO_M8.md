# Reproducing M8 generation 7

Generation 5 and generation 6 are historical burned evidence. A release claim
may use only the fresh generation-7 sequence below on clean commits.

Run the author barrier first:

```text
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo test --locked --release --workspace
```

Build `m8-court` from the clean feature commit. The binary embeds that commit
SHA and refuses a dirty checkout or a mismatched build. For each scope, run
exactly four shards with distinct execution IDs:

```text
cargo build --locked --release -p vice-bench --bin m8-court
target/release/m8-court measure --scope calibration --variants 650 --shard-index 0 --shard-count 4 --execution-id m8-g7-cal-0 --out runs/m8-generation-7/cal-0.json
target/release/m8-court measure --scope calibration --variants 650 --shard-index 1 --shard-count 4 --execution-id m8-g7-cal-1 --out runs/m8-generation-7/cal-1.json
target/release/m8-court measure --scope calibration --variants 650 --shard-index 2 --shard-count 4 --execution-id m8-g7-cal-2 --out runs/m8-generation-7/cal-2.json
target/release/m8-court measure --scope calibration --variants 650 --shard-index 3 --shard-count 4 --execution-id m8-g7-cal-3 --out runs/m8-generation-7/cal-3.json
target/release/m8-court merge --inputs runs/m8-generation-7/cal-0.json --inputs runs/m8-generation-7/cal-1.json --inputs runs/m8-generation-7/cal-2.json --inputs runs/m8-generation-7/cal-3.json --out runs/m8-generation-7/calibration-court.json
target/release/m8-court calibrate --report runs/m8-generation-7/calibration-court.json --out runs/m8-generation-7/calibration.json
```

If and only if calibration is green, commit its exact bytes as
`configs/M8_PRODUCTION_CALIBRATION_V2.json` together with
`configs/M8_GATE_PROVENANCE_V2.toml` in one config-only commit. Rebuild the
runner from that clean gate commit, then run the same four-shard sequence with
`--scope sealed-audit` and execution IDs `m8-g7-sealed-0` through
`m8-g7-sealed-3`.

Merge and decide release only on that gate commit:

```text
target/release/m8-court release --report runs/m8-generation-7/sealed-court.json --calibration configs/M8_PRODUCTION_CALIBRATION_V2.json --authority configs/M8_GATE_PROVENANCE_V2.toml --out runs/m8-generation-7/release.json
target/release/m8-court promote --calibration configs/M8_PRODUCTION_CALIBRATION_V2.json --release runs/m8-generation-7/release.json --out configs/M8_PRODUCTION_POLICY_V1.json
```

The release command rejects any non-ancestor or non-config-only delta between
calibration and sealed candidates. Production admission loads the policy with
`load_committed_m8_production_policy`; uncommitted or byte-modified policy
files fail closed.
