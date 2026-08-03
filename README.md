# vice-classic

A clean-room classical raster-to-SVG inverse rasterizer. The normative design
is `docs/spec/VICE_CLASSIC_CORE_AGENT_SPEC_v1.3.md`.

M0–M11 are implemented. The product surface routes those lanes through one
typed CLI/WASM/browser contract while preserving each lane's production
admission boundary.

This repository is **not authorized for public or commercial release yet**.
The repository license grant, owner-controlled donor attestations and a human
patent/FTO review remain mandatory. `vicec release-status` is the
machine-readable authority and deliberately reports both authorizations as
false.

## CLI

```bash
cargo run --release --bin vicec -- vectorize input.png \
  --mode auto --intent clean --preset quality --experimental --out out/sample
```

`--mode` accepts `auto`, `flat2`, `multiregion`, `line-art`, and `gradient`.
Auto records the deterministic lane decision and does not stop merely because
Flat2 evidence refused the input. Use `--experimental` to receive inspectable
artifacts from lanes that are not production-admitted.

The installed binary uses production configs embedded at compile time and
bound to the existing SHA-256 trust anchors. `--production-config PATH` is an
explicit fail-closed Flat2 override; a missing or modified file never falls
back. Production Flat2 success remains limited by the existing calibrated
envelope.

Successful production runs write:

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

An inspectable non-production route may write:

```text
result.experimental.svg
result.experimental.pure-partition.svg
result.experimental.scene.json
result.experimental.render.png
result.experimental.export-plan.json
result.experimental.seal.json
result.manifest.json
result.report.json
```

Experimental outputs never become production success: filenames, manifest,
report, and status mark them as non-production manual-inspection artifacts.
Unsupported and failed routes still produce a typed report.

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

Open `http://localhost:8080`. The UI exposes Auto and every explicit lane,
shows the selected lane and route reason, previews SVG or PNG artifacts, and
always exposes the diagnostic report. It runs the same Rust product router;
JavaScript does not reimplement inference or thresholds.

## Verification

```bash
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --release --workspace
cargo build --locked --release -p vice-wasm --target wasm32-unknown-unknown
cargo run --locked --release --bin vicec -- release-status \
  --check docs/M12_CROSS_PLATFORM_VECTORS.json
```

See `SECURITY.md`, `docs/M12_PRODUCTIZATION.md`, `docs/M12_PERFORMANCE.md`,
`docs/M12_LEGAL_FTO_REVIEW.md` and milestone status documents for scope and
remaining release blockers.

## Provenance

No donor code is ported: `PORTING_MANIFEST.toml` remains at zero units.
Dependencies and their verified package licenses are recorded in
`THIRD_PARTY_NOTICES.md`; pinned donor roles and hashes are in
`SOURCE_PINS.toml`.
