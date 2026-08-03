# STATUS M9 — implementation complete

Date: 2026-08-03.

M9 implements the complete §28 scope without changing the accepted M8/P1
scene identities or automatic admission policy:

- generic production decode accepts PNG, JPEG and lossless/lossy WebP with
  dimension and memory limits;
- one global formation universe contains 3 resize chains and 8 filters (Box,
  Triangle and Gaussian σ 0.35–2.0), for 48 deterministic hypotheses per
  exterior/blend universe;
- the renderer re-rasterizes geometry at the work resolution, applies one
  global PSF, resizes partitions and preserves the per-pixel partition sum;
- kernel estimation is global and refuses unresolved evidence; there is no
  per-edge kernel parameter;
- JPEG uses 8×8 DCT residuals and lossy WebP uses 4×4 transform residuals;
- development-population calibration is recorded in the structured
  `docs/gt/M9_FORMATION_CALIBRATION_V1.json` and enforced by codec/kernel
  constants and tests. The old scalar degraded-bucket placeholder is not
  misrepresented as a codec-specific release gate.

The public production API is `inspect_m9_formation` plus
`score_m9_formation_calibrated`. Clean PNG behavior remains routed through the
existing accepted pipeline; M9 does not silently widen automatic admission.

Verification:

```text
cargo test --release --workspace
cargo clippy --workspace --all-targets -- -D warnings
```
