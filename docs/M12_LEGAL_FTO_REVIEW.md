# M12 license, patent and FTO review record

Status: **HUMAN REVIEW REQUIRED — PUBLIC/COMMERCIAL RELEASE NOT AUTHORIZED**.

This document is an engineering inventory and sign-off checklist, not legal
advice. Codex cannot provide the qualified legal opinion required by spec §37.

## Engineering facts

- `PORTING_MANIFEST.toml` contains zero ported units.
- The core and M10/M11/M12 implementation is clean-room repository code.
- Production and development dependencies are pinned by `Cargo.lock`; notices
  and package-license checks are in `THIRD_PARTY_NOTICES.md`.
- Pinned donor repositories are external references/baselines and are not
  linked or vendored. Two owner-controlled pins still lack a distribution
  license grant.
- The repository itself has no selected LICENSE file.
- Papers in spec §37 are research references only; no paper implementation or
  dataset is copied.

## Required human sign-offs

- [ ] Repository copyright owner selects and installs the distribution license.
- [ ] Owner signs non-use/authorization attestations for `v-ice` and `Vice-`.
- [ ] Distribution bundle receives a complete direct/transitive notice audit.
- [ ] Counsel reviews the implemented techniques against relevant patent
      families and records jurisdictions, claims searched and conclusions.
- [ ] Counsel issues the commercial freedom-to-operate opinion.
- [ ] Release owner changes the machine-readable authorization fields only in
      a dedicated reviewed legal-release commit.

Until every box is signed, `vicec release-status` must keep
`public_release_authorized=false` and `commercial_release_authorized=false`.
