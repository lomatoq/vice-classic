# Third-party notices — vice-classic

Status date: 2026-07-26 (M2).

## 1. Pinned donor repositories (NOT vendored, NOT linked in M0)

The three pinned repositories below are executed only as **external black-box
baselines** by the M0 baseline runner. No source code from them is copied,
vendored, or linked into this workspace in M0 (see `PORTING_MANIFEST.toml`,
which has zero units).

| Source | Pin | Declared license status |
|---|---|---|
| `lomatoq/v-ice` | `9211b3213d9b47defdf19ae4d0842af1c3ade45f` | OWNER_CONTROLLED — verify before public release |
| `lomatoq/v-ize` | `95a65194cf34e2d96b41eb299b4769eac624be80` | MIT OR Apache-2.0 (declared in its workspace; re-verify at pin) |
| `lomatoq/Vice-` | `200897ab3e888970e330deeb3bb9e157923cc0aa` | OWNER_CONTROLLED — verify before public release |

Findings verified against the local mirrors on 2026-07-26 (recorded here,
resolution deferred to the pre-release license review):

- `v-ice` @ pin: **no LICENSE/COPYING file and no `license` key** in
  Cargo.toml — legally unlicensed by default; owner-controlled.
- `v-ize` @ pin: `license = "MIT OR Apache-2.0"` is declared in
  `[workspace.package]` and inherited by vize-core/cli/wasm/bench, but
  **no LICENSE-MIT / LICENSE-APACHE text files exist in the repo**, and the
  `vize-gpu` / `bsplat-spike` crates do not inherit the license field at
  the pin (vize-cli depends on vize-gpu).
- `Vice-` @ pin: no license file; only third-party font licenses
  (SIL OFL) are tracked via its `font_license_manifest.py`.

Before any public or commercial release of vice-classic, owner-controlled
repositories require an explicit SPDX/license grant, and a separate license /
patent / freedom-to-operate review is mandatory (spec v1.3 §2.1, §37).

## 2. Rust crate dependencies (M0)

Direct dependencies of `crates/vice-bench`; transitive set is pinned by
`Cargo.lock`. All are used unmodified via cargo.

| Crate | License |
|---|---|
| serde, serde_json | MIT OR Apache-2.0 |
| toml | MIT OR Apache-2.0 |
| sha2 | MIT OR Apache-2.0 |
| thiserror | MIT OR Apache-2.0 |
| clap | MIT OR Apache-2.0 |
| png | MIT OR Apache-2.0 |
| hex | MIT OR Apache-2.0 |

## 2a. Rust crate dependencies added in M1

License verified against the actual package downloaded from the registry
(Cargo.toml `license` key and shipped license texts), not just crates.io
metadata.

| Crate | Version (Cargo.lock) | License | Used by | Role |
|---|---|---|---|---|
| robust | 1.2.0 | MIT OR Apache-2.0 (LICENSE-MIT + LICENSE-APACHE shipped) | vice-geom | Adaptive-precision Shewchuk predicates (orient2d/incircle). Rust port of public-domain C predicates by J. R. Shewchuk; consumed as an external dependency, NOT vendored (PORTING_MANIFEST stays at zero units). |
| proptest | 1.11.0 | MIT OR Apache-2.0 (LICENSE-MIT + LICENSE-APACHE shipped) | vice-geom, vice-ir (dev-dependency only) | Property testing. Not linked into shipped binaries. |

Transitive dependencies are pinned by the committed `Cargo.lock`.

## 2b. Rust crate dependencies added in M2

License verified against the actual packages downloaded from the registry
(Cargo.toml `license` key and shipped license texts), not just crates.io
metadata. The differential-court rasterizers are **dev-dependencies
only** (spec §16.3 independent court); they are never linked into shipped
binaries, and their internal `unsafe` is their own code, not ours
(`#![forbid(unsafe_code)]` applies to vice-classic crates).

| Crate | Version (Cargo.lock) | License | Used by | Role |
|---|---|---|---|---|
| tiny-skia | 0.11.4 | BSD-3-Clause (LICENSE shipped; Google/Reizner copyright) | vice-render (dev) | Differential-court rasterizer #1 (Skia-lineage analytic-AA scanline). |
| tiny-skia-path | 0.11.4 | BSD-3-Clause (LICENSE shipped) | via tiny-skia | Path types for court #1. |
| raqote | 0.8.5 | BSD-3-Clause (LICENSE.md shipped) | vice-render (dev) | Differential-court rasterizer #2 (Firefox-lineage, independent codebase). |
| sw-composite | 0.7.16 | BSD-3-Clause (license key; no text file shipped in package — upstream repo carries it; acceptable for a dev-only dependency, re-check before any redistribution) | via raqote | Compositing kernels of court #2. |
| euclid | 0.22.14 | MIT OR Apache-2.0 (both texts shipped) | via raqote | Geometry types of court #2. |
| lyon_geom | 1.0.19 | MIT OR Apache-2.0 (license key; texts in upstream repo) | via raqote | Curve math of court #2. |
| typed-arena | 2.0.2 | MIT (LICENSE shipped) | via raqote | Arena allocator of court #2. |
| arrayref | 0.3.9 | BSD-2-Clause (LICENSE shipped) | via tiny-skia | Array helpers. |
| arrayvec | 0.7.8 | MIT OR Apache-2.0 (both texts shipped) | via tiny-skia, lyon_geom | Fixed-capacity vectors. |
| bytemuck | 1.25.2 | Zlib OR Apache-2.0 OR MIT (per package license key) | via tiny-skia | Byte casting inside court #1. |
| strict-num | 0.1.1 | MIT (LICENSE shipped) | via tiny-skia | Finite-number newtypes. |
| libm | 0.2.16 | MIT (LICENSE.txt shipped) | via raqote (num-traits) | Math fallbacks. |
| log, num-traits, cfg-if, png (and deps) | per Cargo.lock | MIT OR Apache-2.0 family | transitive | Already present or standard transitive set; pinned by Cargo.lock. |

No production dependency was added in M2 (vice-render's non-dev
dependencies are vice-geom, vice-ir, sha2, hex, thiserror — all already
recorded above).

## 3. Prohibited sources

- No code from closed-source Vector Magic, in any form.
- Papers listed in spec §37 are used clean-room (algorithms/formulas only).
