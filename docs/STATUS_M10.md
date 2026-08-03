# STATUS M10 — implementation complete

Date: 2026-08-03.

M10 implements the complete §28 stroke/line-art scope without changing the
accepted M8/P1 fill pipeline or inventing an additional release gate:

- a validated standalone stroke graph carries centerlines, constant physical
  width, butt/round/square caps, miter/round/bevel joins and explicit branch
  junctions;
- deterministic supersampled rendering covers every style and rejects
  centerline crossings that are not represented by a graph junction; spatial
  indexing plus explicit intersection/sample work budgets refuse adversarial
  valid graphs before unbounded work;
- observed pixels produce an adaptive foreground mask, physical distance
  widths, a thinned centerline and a bounded graph; salient bends become real
  degree-two join vertices;
- only load-bearing cap/join alternatives are enumerated, with canonical unique
  identities;
- fill and stroke candidates are compared with the same codec-aware residual
  likelihood plus an explicit comparable structural description length;
- evidence failures are typed refusals and leave the verified fill witness
  selected.

The public API is `inspect_m10_line_art`, `select_m10_line_art` and
`select_m10_line_art_against_fill`. The first two compose with the existing
pipeline; the explicit-fill entry point supports controlled callers and tests.
M10 returns the selected validated stroke scene and deterministic pixels. SVG,
WASM/UI and product performance policy remain M12 productization work.

Verification:

```text
cargo test --release --workspace
cargo clippy --workspace --all-targets -- -D warnings
```
