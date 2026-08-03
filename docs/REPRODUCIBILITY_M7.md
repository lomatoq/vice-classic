# Reproducing M7

Use a clean checkout of the exact candidate SHA and the pinned Windows
toolchain. Tier-A floating/render artifacts require the same platform.

## Compact author barrier

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p vice-bench --lib calibration_digest_
cargo test --locked -p vice-bench --lib sealed_population_and_execution_are_exact_and_tamper_evident
cargo test --locked -p vice-bench --lib determinism_requires_six_distinct_typed_executions
cargo test --locked -p vice-bench --lib canonical_artifact_binds_every_green_component_and_identity
cargo build --locked --workspace --release
target/release/gt-corpus.exe verify --manifest docs/gt/CORPUS_MANIFEST.json
```

Generation-8 calibration commands and evidence digests are structured fields
in `configs/M7_GATE_PROVENANCE_V1.toml`.

## Strict sealed release path

Create a runner attestation and open a byte-for-byte copy of the sealed record.
Every logical run has one role and run ID shared by all its shards.

```text
target/release/gt-corpus.exe m7-runner-attest --anchor-source reviewer_pinned --event-commit SHA --repository-root ROOT --git-executable GIT --vicec-executable ROOT/target/release/vicec.exe --gates configs/GATES_V1.toml --gate-provenance configs/M7_GATE_PROVENANCE_V1.toml --out RUN/runner-attestation.json
target/release/gt-corpus.exe m7-audit-open --runner-attestation RUN/runner-attestation.json --gate-provenance configs/M7_GATE_PROVENANCE_V1.toml --audit-seal RUN/AUDIT_SEAL_OPENED.json --manifest docs/gt/CORPUS_MANIFEST.json --gates configs/GATES_V1.toml --note SHA
```

Run `m7-audit-measure` for the six exact roles below. Isolated roles use one
worker; parallel roles use two. Sharding is allowed, but every merged report
must reconstruct exactly 800 groups and 2400 unique rows.

```text
fast-primary     fast     1 worker
fast-repeat      fast     1 worker
fast-parallel    fast     2 workers
quality-primary  quality  1 worker
quality-repeat   quality  1 worker
quality-parallel quality  2 workers
```

Each invocation supplies the common governance arguments plus:

```text
--production-config CONFIG --preset PRESET --role ROLE --run-id UNIQUE_ID --workers N --shard-index I --shard-count COUNT --out REPORT
```

Then run release, baseline and oracle on the primary reports. Run determinism
with its six named report options (`--fast-primary`, `--fast-repeat`,
`--fast-parallel`, `--quality-primary`, `--quality-repeat`,
`--quality-parallel`) and the same governance inputs. Finally run
`m7-canonical-artifact`. Every court refuses a partial/substituted population,
stale config, mixed candidate/runner/corpus, duplicate execution, changed row,
or non-green renderer/geometry gate.

## Recorded waiver

The current closure intentionally does not execute the strict path after
`fbd0a41`. The earlier generation-8 run stopped at 5091/14400 checkpointed rows
and predates the evidence-binding repair. Per operator direction, those rows
are neither resumed nor promoted. The reproducible current claim is therefore
the compact author barrier plus generation-8 calibration, not a sealed release
artifact.
