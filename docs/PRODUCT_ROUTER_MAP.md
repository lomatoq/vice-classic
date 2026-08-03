# Product router implementation map

This note records the public call graph before the routed product delta.

## Current public routes

- Native CLI `vicec vectorize` accepts only `--mode flat2` and calls
  `vice_core::vectorize_embedded_production()` (or the explicit production
  config variant). Only M7 `SuccessArtifacts` are written.
- WASM exports `vectorize_flat2()` and separately exposes the M11
  `classify_gradient()` diagnostic. There is no unified product result.
- The browser imports and calls only `vectorize_flat2()`.

## Implemented but disconnected lanes

- M8 exposes `solve_multiregion_exact()` and
  `seal_multiregion_delivery()`, including scene, SVG, PNG, seal, and report
  artifacts. It is not reachable from `vicec vectorize`, WASM vectorization,
  or the browser.
- M10 exposes line-art inspection and fill-vs-stroke selection. Its automatic
  selection currently needs an M7 fill witness; that dependency must remain a
  typed public limitation when the witness is unavailable.
- M11 exposes deterministic solid/linear/radial classification, selected scene
  JSON, and a rendered RGBA result. Only its diagnostic WASM function is
  public.

## Artifact constraints

- M7 has a sealed production artifact set and must remain unchanged.
- M8 can materialize SVG and PNG artifacts, but routed non-production output
  must be explicitly marked experimental unless a trusted production policy
  admits it.
- M10 and M11 can expose scene JSON plus an encoded preview PNG. They currently
  have no common SVG exporter, so the product contract must state which
  artifacts are available rather than fabricate an SVG.
- Every routed outcome needs one typed product report, including failures and
  refusals; callers must not infer lane or trust from filenames.
