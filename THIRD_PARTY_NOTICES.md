# Third-party notices — vice-classic

Status date: 2026-07-26 (M0).

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

## 3. Prohibited sources

- No code from closed-source Vector Magic, in any form.
- Papers listed in spec §37 are used clean-room (algorithms/formulas only).
