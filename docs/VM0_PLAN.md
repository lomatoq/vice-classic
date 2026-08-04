# VM0 implementation plan

## Goal and boundary

Build a new, independent `crates/vice-vm-core` vertical slice for exactly:

```text
one foreground paint + one background paint + one closed anti-aliased shape
-> compact subpixel vector curve
```

The first visible target is a circle that remains a circle across raster
resolution and subpixel phase. VM0 is not accepted by JSON alone: every stage
must leave a runnable visual artifact for the Checker-Fixer.

Not in scope: multicolor, components/holes beyond typed refusal, UI, product
router, M8 integration or repair, M7 calibration, sealed audit, release work,
or a new review campaign.

## Architecture decision

The isolation result is recorded in [VM0_REUSE_DECISION.md](VM0_REUSE_DECISION.md):
the VM0 fit path uses a minimal local rewrite, not `vice-fit`.

| Existing crate/path | VM0 treatment |
| --- | --- |
| `vice-image` | Reuse canonical decode, RGBA, dimensions, source hash, and color assumptions |
| `vice-geom` | Reuse points, vectors, robust predicates, and flattening utilities |
| `vice-render` | Reuse for deterministic forward fixtures and later local rerendering |
| `vice-svg` | Reuse only after compact geometry exists |
| `vice-verify` | Reuse only to verify final VM0 artifacts, never as a generator |
| `vice-fit` | Do not link; rewrite the bounded VM0 fitter locally |
| M7/M8/product paths | Excluded from VM0 |

`vice-vm-core` owns a small set of concrete types, not future-facing traits:
`TwoPaintModel`, `CoarseBoundaryChain`, `VmBoundarySample`, compact curve
geometry, and a typed VM0 report/refusal. A hard mask may establish topology
only. The curve fitter consumes physical subpixel samples.

## Pipeline

```text
decode pixels
-> estimate two paints from stable interiors
-> prove one foreground component / one closed boundary
-> make one maximal coarse chain
-> invert AA to subpixel position + normal + corridor
-> choose compact Line/Arc/Quad/Cubic grammar
-> joint constrained refit
-> local ROI rerender refinement
-> SVG + render + diagnostics
```

No stage may convert the unit pixel lattice directly into final SVG segments.
Ambiguity at 16--24 px is a typed result, not permission to invent geometry.

## Small-commit sequence

Each commit must build, pass its focused tests, save the named visual artifact,
and stop for one fresh-context Checker-Fixer verdict before the next task is
activated.

| Commit | Single responsibility | Required evidence before acceptance |
| --- | --- | --- |
| C1 | Create `vice-vm-core` and a deterministic Circle forward fixture | Seed replay test; source raster + GT overlay at two phases and at least 32/64 px |
| C2 | Estimate two linear-RGB paints from stable interiors | Color/conditioning error tests; interior-selection overlay; typed low-contrast refusal |
| C3 | Prove VM0 topology and collapse it to one maximal closed coarse chain | One-component/one-loop tests; coarse-chain overlay; no duplicate or collinear unit runs |
| C4 | Recover physical subpixel AA boundary samples | Phase/resolution sweep; position p95 and normal checks; sample/corridor overlay |
| C5 | Implement bounded local Line/Arc/Quad/Cubic span solvers | Isolation line/circle/cubic tests using physical samples; no `vice-fit` dependency |
| C6 | Add compact closed-curve DAG/shortest-path grammar | Segment-count and no-staircase tests; triangle and cubic-blob artifacts added here |
| C7 | Jointly refit shared endpoints and G1 nodes inside corridors | Closure/G1/corridor tests; pre/post-refit overlays; reject infeasible models |
| C8 | Refine geometry and paints by local ROI rerendering without topology changes | Objective decrease plus no-regression guard; difference image |
| C9 | Add Circle, Ellipse, and RoundedRectangle sibling hypotheses | Family/parameter stability sweeps; ellipse and rounded-rectangle contact sheets |
| C10 | Add `vicec vm0` and the five developer artifacts | CLI determinism test; `result.svg`, render, report, sample SVG, coarse SVG |
| C11 | Run the complete VM0 benchmark and write the acceptance report | Full shape/resolution/phase contact sheet and machine metrics; Checker verdict |

Synthetic coverage grows with the algorithm that owns it: Circle starts C1;
Triangle and smooth/corner/inflection blobs enter with grammar in C6; Ellipse
and RoundedRectangle enter with primitives in C9. C11 executes the complete
matrix over 16, 24, 32, 48, 64, 128, and 256 px, phases, rotations, scales,
colors, contrast, blend space, and AA kernels. This keeps early commits small
without weakening the final matrix.

## Acceptance gates

- Straight edge: one Line, no staircase, phase/resolution stable.
- Circle: Circle/arcs/at most four cubics; boundary p95 at most 0.25 px on clean
  AA at 32--256 px; segment count independent of perimeter.
- Ellipse: ellipse or compact cubics with phase-stable family.
- Rounded rectangle: four lines plus four arcs or a compact equivalent, with
  stable radii and corners.
- Triangle: three true corners, straight edges, no false rounding.
- Cubic/inflection blobs: few smooth cubics, no grid staircase, resampling
  stability.
- Global: unit-axis-aligned segment ratio near zero and final segment count
  does not scale with raster perimeter.
- Visual: GT, input, coarse chain, subpixel samples, final SVG, rerender, and
  difference contact sheet accepted by the Checker-Fixer.

Only that final verdict may record `VM0_ACCEPTED`. Until then there is no
multicolor, UI, M8 integration, or release task.
