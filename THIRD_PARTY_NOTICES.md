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

## 2c. Rust crate dependencies added in M3

Licence verified against the ACTUAL package downloaded from the registry
(the `license` key in the package's own `Cargo.toml` and the licence texts
it ships), not against crates.io metadata — the same method used for §2b.

Two of these become NORMAL (non-dev) dependencies, and only of
`crates/vice-bench`. The reason is spec §27.1: the GT corpus must be
rasterized by several INDEPENDENT engines, and the corpus generator is a
binary rather than a test. The core crates are unaffected — `vice-geom`,
`vice-ir` and `vice-render` do not link them, and `vice-bench` is not a
shipped artifact (`publish = false`).

| Crate | Version (Cargo.lock) | License | Used by | Role |
|---|---|---|---|---|
| tiny-skia | 0.11.4 | BSD-3-Clause (`LICENSE` shipped) | vice-bench (normal), vice-render (dev) | independent corpus rasterizer profile; independence arm of the M2 court |
| tiny-skia-path | 0.11.4 | BSD-3-Clause (`LICENSE` shipped) | transitive | — |
| raqote | 0.8.5 | BSD-3-Clause (`LICENSE.md` shipped) | vice-bench (normal), vice-render (dev) | second independent corpus rasterizer profile |
| sw-composite, euclid, lyon_geom, typed-arena, arrayref, arrayvec, bytemuck, strict-num, libm, log, num-traits, cfg-if, png | per Cargo.lock | as recorded in §2b | transitive | — |

`sw-composite` 0.7.16 still ships no licence text in its package while
declaring BSD-3-Clause; the §2b note stands and is repeated here because
the crate is now in a NORMAL dependency graph rather than a dev-only one:
**re-check before any redistribution.**

### External tournament opponents (spec §27.3)

Declared in `configs/baselines.toml` as installed EXECUTABLES, not as
linked crates. Nothing is vendored, nothing is linked, and nothing this
project ships inherits their licences. Provenance recorded per run: the
resolved path, its sha256 and its `--version` output.

| Tool | Licence | Verified how | How it is used |
|---|---|---|---|
| VTracer | MIT OR Apache-2.0 | `license` key of the actual `vtracer` 1.0.0-alpha.1 package. NOTE: that package ships NO licence text; `visioncortex` 0.9.0, its core, ships both `LICENSE-MIT` and `LICENSE-APACHE` | separate process, `vtracer --input … --output …` |
| Potrace | GPL-2.0-or-later | upstream project licence | separate process ONLY. Never linked, vendored or derived from, so no copyleft obligation attaches here. Not installed on the recording machine: the baseline records a typed `binary_missing` rather than being quietly dropped |

The GPL point is the reason Potrace is an out-of-process opponent and will
stay one: §36 names "commercial implementation requires code or data with
an incompatible licence" as a stop condition, and linking Potrace would
walk into it.

### Corpus provenance

The GT corpus contains no third-party asset. Procedural scenes are
generated by `vice-bench::gt::grammar`; the six hand-authored SVG files
under `tests/fixtures/gt/authored/` were authored in this repository for
this purpose; the adversarial fixtures are constructed in code. Nothing is
taken from the donor pins — `PORTING_MANIFEST.toml` remains at **zero
units** (REVIEW_M0 condition 6, debt D-3), which for a ground-truth corpus
is exactly where the temptation was strongest.

## 2d. Зависимости M4

**Ни одной новой третьесторонней зависимости.** Проверено, а не выведено из
молчания: `git diff <M3.5 HEAD> -- Cargo.lock` показывает ровно три новых
пакета, и все три — крейты этого воркспейса (`vice-image`, `vice-evidence`,
`vice-cli`). Внешний набор не изменился.

Три новых крейта используют только уже записанное выше:

| Крейт | Внешние зависимости | Где записаны |
|---|---|---|
| `vice-image` | `png` 0.17, `serde`, `sha2`, `hex`, `thiserror` | §2 (M0) |
| `vice-evidence` | `serde`, `serde_json`, `sha2`, `hex`, `thiserror` | §2 (M0) |
| `vice-cli` | `clap` 4, `serde_json`; `tempfile` (dev) | §2 (M0), §2c |

`png` перешёл из «зависимость бинаря `gen-smoke`» в «зависимость библиотеки
`vice-image`», то есть в путь, который однажды будет отгружаться. Лицензия
та же (MIT OR Apache-2.0) и уже проверена по фактическому пакету в §2;
отдельного действия это не требует, но переход отмечен здесь, потому что
роль зависимости изменилась.

PORTING_MANIFEST на M4 остаётся при **нуле units**: донорские
`energy.rs` (v-ice) и `coverage_evidence.py` (Vice-) — ближайшие
родственники того, что делает M4, и ни один не открывался. Оба под
`OWNER_CONTROLLED_VERIFY_BEFORE_PUBLIC_RELEASE` (§2), а долг D-3 требует
license/IP review ДО первого `[[unit]]`; смотреть на код донора «чтобы
свериться» — это и есть перенос, только неучтённый.
