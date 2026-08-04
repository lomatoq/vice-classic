# VM0 `vice-fit` isolation decision

## Decision

**REWRITE the minimal VM0 fitter inside `vice-vm-core`. Do not link the VM0
geometry path to `vice-fit`.** Reuse its mathematical ideas and the existing
geometric primitives, but not its runtime fitting pipeline.

This decision is scoped only to fitting. The repository and the lower-level
image, geometry, rendering, SVG, and verification infrastructure remain in
place.

## Authority and baseline

- Repository baseline: `bf0b3c8339b66aa6bb363c8ed5b0a0a5767806cd`
- Baseline inspected: exact match (`git rev-parse HEAD`)
- Planner spec SHA-256:
  `78547ccf250ba3018022040774fff5f3d04bc7a516ada08b82ebd8ddaecc6322`
- Spike date: 2026-08-04

## Executable isolation spike

The Planner ran an ignored scratch binary at
`target/vm0-fit-spike/src/main.rs`; it made no production changes. The adapter
was 229 LOC and used only these direct path dependencies:

```text
vice-evidence
vice-fit
vice-geom
vice-ir
```

`cargo tree --edges normal` showed no `vice-core`, `vice-opt`, `vice-topology`,
M7 scene, M8 materializer, browser, router, or release dependency. The only
additional VICE crate in the transitive closure was `vice-image`, through
`vice-evidence`.

Run command:

```text
cd target/vm0-fit-spike
cargo run --release
```

The final command exited 101 at the deliberate cubic compactness assertion;
the summaries above it are the measured evidence, not a compilation failure.

Every sample supplied a physical point, unit normal, positive corridor
halfwidth, arclength weight, confidence, and correlation length. No mask,
M7/M8 scene, or fabricated boundary subtype was supplied.

## Results

The dense chains were run twice in one process. Their complete summaries were
identical, so exact-call determinism passed.

| Fixture | Sampling | Selected output | Final segments | Result |
| --- | ---: | --- | --- | --- |
| Straight line at `x=10.25` | 101 samples | typed chain | 1 Line | PASS |
| Circle `(64.2,63.7), r=30.4` | 192 samples | Circle primitive | 4 arcs | PASS |
| Same circle | 96 samples | Circle primitive | 4 arcs | PASS; final representation stable |
| Known four-cubic blob | 256 samples | typed chain | 5 Cubic + 4 Arc = 9 | FAIL; not compact |
| Same blob | 128 samples | typed chain | 4 Cubic + 1 Arc + 2 Line = 7 | FAIL; changes under resampling |

An exploratory blob corridor of 0.15 px was refused as
`outside_corridor`. Widening it to the repository's existing clean-chain test
scale of 0.35 px made the fit feasible but did not fix compactness or
resampling stability. The decision therefore does not rest on a single overly
narrow corridor.

## PASS/FAIL accounting

| Reuse condition | Verdict |
| --- | --- |
| Adapter at most 300 LOC | PASS (229) |
| No M7/M8 pipeline required | PASS |
| Compact output | FAIL on cubic blob |
| Deterministic on identical input | PASS |
| Uses normal/corridor evidence | PASS |
| Line, circle, and cubic tests all pass | FAIL on cubic blob |

Reuse requires the full conjunction. Two required conditions fail, so wrapping
or tuning `vice-fit` is not authorized for VM0.

## Rewrite boundary

The local implementation is deliberately small:

- consume only `VmBoundarySample { position, normal, ds, halfwidth,
  correlation_length }`;
- fit Line, CircularArc, QuadraticBezier, and CubicBezier spans;
- select a compact closed grammar with an explicit segment penalty;
- jointly refit shared endpoints and G1 nodes against the physical samples;
- reject models outside their uncertainty corridors;
- add Circle, Ellipse, and RoundedRectangle siblings only after the base
  grammar is green.

It must not copy in the M7/M8 pipeline, use a binary mask as the final fitting
target, or grow into a general multicolor fitter. `vice-fit` can remain in the
repository for the existing classic pipeline; VM0 simply does not depend on
it.
