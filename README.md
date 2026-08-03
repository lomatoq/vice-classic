# vice-classic

A clean-room classical raster-to-SVG inverse rasterizer. The normative design
is `docs/spec/VICE_CLASSIC_CORE_AGENT_SPEC_v1.3.md`.

Current state: M0–M11 are implemented. M12 provides the technical product
release candidate: installed CLI, embedded digest-pinned production configs,
WASM adapter, browser UI source, explicit legacy wrapper, cross-platform
structural checks and security/resource policy.

This repository is **not authorized for public or commercial release yet**.
The repository license grant, owner-controlled donor attestations and a human
patent/FTO review remain mandatory. `vicec release-status` is the machine
readable authority and deliberately reports both authorizations as false.

## CLI

```bash
cargo run --release --bin vicec -- vectorize input.png \
  --mode flat2 --intent clean --preset quality --out out/sample
```

The installed binary uses production configs embedded at compile time and
bound to the existing SHA-256 trust anchors. `--production-config PATH` is an
explicit fail-closed override; a missing or modified file never falls back.

Successful Classic runs write:

```text
result.svg
result.pure-partition.svg
result.scene.json
result.export-plan.json
result.report.json
result.render.png
result.seal.json
trace/trace.json       # only with --trace
```

Ambiguous, unsupported and failed runs publish only their typed report.

An external legacy engine is never an implicit fallback. It can only be run
through the explicit digest-pinned wrapper:

```bash
vicec legacy-vectorize input.png \
  --engine /absolute/path/to/engine \
  --engine-sha256 <64-hex-digest> \
  --arg --input --arg {input} --arg --output --arg {output} \
  --out out/legacy
```

Its report always contains `classic_success: false`.

## Browser/WASM

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack --locked
wasm-pack build crates/vice-wasm --target web --out-dir ../../web/pkg
python -m http.server --directory web 8080
```

Open `http://localhost:8080`. The UI runs the same embedded production core;
it has no JavaScript reimplementation of inference or thresholds.

## Verification

```bash
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --release --workspace
cargo build --locked --release -p vice-wasm --target wasm32-unknown-unknown
cargo run --locked --release --bin vicec -- release-status \
  --check docs/M12_CROSS_PLATFORM_VECTORS.json
```

See `SECURITY.md`, `docs/M12_PRODUCTIZATION.md`,
`docs/M12_PERFORMANCE.md`, `docs/M12_LEGAL_FTO_REVIEW.md` and milestone status
documents for scope and remaining release blockers.

## Provenance

No donor code is ported: `PORTING_MANIFEST.toml` remains at zero units.
Dependencies and their verified package licenses are recorded in
`THIRD_PARTY_NOTICES.md`; pinned donor roles and hashes are in
`SOURCE_PINS.toml`.
