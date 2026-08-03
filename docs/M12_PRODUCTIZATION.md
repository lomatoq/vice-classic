# M12 productization contract

M12 adds adapters and policy around the existing core; it does not fork the
inference implementation.

## Production surfaces

- `vicec vectorize`: native selective Flat2 delivery using embedded,
  digest-pinned production configs by default.
- `vice-wasm::vectorize_flat2`: browser adapter over the same embedded entry
  point; success returns verified SVG/render artifacts and every non-success
  returns only its typed report.
- `web/`: dependency-free upload/preset/preview/download UI source with CSP and
  accessible status output.
- `vicec legacy-vectorize`: explicit SHA-pinned external wrapper, never a
  Classic fallback or Classic success.
- `vicec release-status`: stable structural contract and legal-release state.

The P1 deterministic partition-correction API remains the editor core. M12 does
not introduce an unverified Bézier editor or post-export geometry mutation.

## Cross-platform tiers

Tier A exact floating render artifacts remain same-platform, as documented by
the existing corpus policy. M12 adds Tier B structural compatibility on Linux,
Windows and macOS: binary version, both production config trust anchors, WASM
schema, fallback policy and legal status must exactly match
`M12_CROSS_PLATFORM_VECTORS.json`. CI also uses pinned `wasm-pack` to build the
real browser package and imports its generated module on all three platforms.

## Packaging

The installed CLI no longer depends on its working directory for production
configs. The WASM module has no native filesystem/process calls. `wasm-pack`
generates the browser glue into `web/pkg`; generated package output is a build
artifact and is not committed.

This is a technical release candidate only. See `M12_LEGAL_FTO_REVIEW.md`.
