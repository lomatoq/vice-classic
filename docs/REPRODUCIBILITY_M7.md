# Reproducing M7

Run from a clean checkout of the exact release-candidate SHA on the pinned
toolchain. M7 Tier-A floating/render artifacts require an exact same-platform
comparison; `--structural` is only the declared cross-platform CI court.

## Author barrier

```text
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets --release -- -D warnings
cargo test --locked --workspace
cargo test --locked --workspace --release
cargo test --locked --release -p vice-bench --test frozen_calibration -- --ignored
cargo test --locked --release -p vice-bench --test dcel_harness -- --ignored
cargo test --locked --release -p vice-topology --test dcel_props -- --ignored
cargo test --locked --release -p vice-bench fit::tests::the_candidate_stage_over_the_corpus -- --ignored
cargo test --locked --release -p vice-bench --test doc_claims
target/release/gt-corpus.exe verify --manifest docs/gt/CORPUS_MANIFEST.json
target/release/gt-corpus.exe corridor-check --report docs/gt/CORRIDOR_M4.json
target/release/gt-corpus.exe topology-check --report docs/gt/TOPOLOGY_M4_5.json
target/release/gt-corpus.exe dcel-check --report docs/gt/DCEL_M5.json
target/release/gt-corpus.exe geometry-m6-check --gates configs/GATES_V1.toml --report docs/gt/GEOMETRY_M6.json
```

The complete generation-5 calibration and geometry commands, together with
their output digests, are frozen as structured fields in
`configs/M7_GATE_PROVENANCE_V1.toml`. They are not copied into prose as an
independent source of truth.

## Anchored sealed release

Let `SHA` be the exact clean candidate, `ROOT` the canonical repository root,
and `GIT` the canonical Git executable outside the repository. Build release
`gt-corpus` and `vicec`, then create the ignored runner evidence:

```text
target/release/gt-corpus.exe m7-runner-attest --anchor-source reviewer_pinned --event-commit SHA --repository-root ROOT --git-executable GIT --vicec-executable ROOT/target/release/vicec.exe --gates configs/GATES_V1.toml --gate-provenance configs/M7_GATE_PROVENANCE_V1.toml --out runs/m7/final/runner-attestation.json
```

Copy `docs/gt/AUDIT_SEAL.json` byte-for-byte to
`runs/m7/final/AUDIT_SEAL_OPENED.json` before opening it. The tracked sealed
record remains immutable; the opened copy is the release evidence.

```text
target/release/gt-corpus.exe m7-audit-open --runner-attestation runs/m7/final/runner-attestation.json --gate-provenance configs/M7_GATE_PROVENANCE_V1.toml --audit-seal runs/m7/final/AUDIT_SEAL_OPENED.json --manifest docs/gt/CORPUS_MANIFEST.json --gates configs/GATES_V1.toml --note SHA
target/release/gt-corpus.exe m7-audit-measure --runner-attestation runs/m7/final/runner-attestation.json --gate-provenance configs/M7_GATE_PROVENANCE_V1.toml --audit-seal runs/m7/final/AUDIT_SEAL_OPENED.json --manifest docs/gt/CORPUS_MANIFEST.json --gates configs/GATES_V1.toml --production-config configs/M7_PRODUCTION_QUALITY.json --preset quality --workers 1 --out runs/m7/final/quality.json
target/release/gt-corpus.exe m7-audit-measure --runner-attestation runs/m7/final/runner-attestation.json --gate-provenance configs/M7_GATE_PROVENANCE_V1.toml --audit-seal runs/m7/final/AUDIT_SEAL_OPENED.json --manifest docs/gt/CORPUS_MANIFEST.json --gates configs/GATES_V1.toml --production-config configs/M7_PRODUCTION_FAST.json --preset fast --workers 1 --out runs/m7/final/fast.json
```

Analyze the two complete reports with `m7-audit-analyze`, then run
`m7-baseline-court`, `m7-oracle`, and `m7-determinism`; finally bind those four
artifacts with `m7-canonical-artifact`. Each command exits nonzero on a failed
gate. Exact options are printed by `target/release/gt-corpus.exe <command>
--help`; all governance-consuming commands use the same attestation, opened
seal, manifest, gates and provenance paths above.

## Review protocol

Create four independent temporary worktrees at the exact candidate SHA. Each
reviewer starts cold, runs documented commands without author caches, checks a
negative/adversarial case, and records the SHA and verdict. Required roles are
two cold reviewers from different model families, one numerical/topology red
team, and one release-quality audit. Any implementation or evidence edit
invalidates all four verdicts.
