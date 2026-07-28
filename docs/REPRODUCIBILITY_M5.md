# REPRODUCIBILITY_M5 — shared DCEL and topological transactions

Spec v1.3 §34: a reviewer makes a clean checkout and runs documented commands
without the author's caches. This file is that list. It supplements
`docs/REPRODUCIBILITY_M4_5.md`, `_M4`, `_M3_5` and `_M3`, which are still the
instructions for everything M5 did not touch.

Spec SHA-256, checked first:

```text
652fd0b6e17c96c38af0173ddcc93a3921eafd60a9aff34c8d848829228d9bb1
```

## 0. Clean checkout

```bash
git clone <repo> <unique-path>          # unique BY CONSTRUCTION (condition 25)
cd <unique-path>
git rev-parse HEAD
git status --porcelain                  # before AND after every reported run
```

## 1. The whole suite, both profiles

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings   # CLEAN target dir
cargo test --locked --workspace
cargo test --locked --release --workspace
```

Author's counts on `windows-x86_64`, rustc 1.96.0: **530 passed, 0 failed** in
each profile, and **11 passed** under `-- --ignored` in release.

That number was 529 when this file was first written and went stale the moment
C246 added a test. It is corrected here rather than removed, because it is the
one number in this document a reviewer can check in a single command — but the
correction is the point: a count in prose is a COPY (F-0028), and the command
above is the original.

`clippy` on a warm target directory is indistinguishable from not having run it;
the reviewer's own note on this from M4.5 applies unchanged.

## 2. The §28 M5 gate

```bash
cargo run --locked --release --bin gt-corpus -- dcel \
  --out runs/m5/report.json --scope full
```

Exits non-zero when any of the four clauses fails. Author's run: **exit 0, four
`[MET]`**, and the numbers the report carries:

```text
41 scenes, 474 arms (444 corpus, 30 structural), 0 refused,
22 sealed-audit groups skipped
classes [(0,0), (1,0), (1,1), (2,0), (2,1), (3,0), (3,1), (5,0)]
groups 237, classes in 247 out 247, convention-dependent 10
transactions 167 attempted, 167 committed, 0 rolled back
unrelated chains 127, moved 0
audit resolving power: 28 arrangements of 474 arms, 155 160 slots,
  audit 5648, assembly 155 160, neither 0, no-ops 0
```

Then compare against the committed artifact:

```bash
cmp runs/m5/report.json docs/gt/DCEL_M5.json     # byte-identical on the recording platform
cargo run --locked --release --bin gt-corpus -- dcel-check \
  --report docs/gt/DCEL_M5.json                  # WITHOUT --structural
```

On another platform `dcel-check` without `--structural` is a typed refusal, not
a pass: the report carries `platform`. The projection is wide — every count and
every class — because those are integers a different libm cannot move without
moving the topology itself. **It is not Tier B and is not offered as it**; A7.1
remains open with owner M12.

## 3. The knockouts, which are the reason the clauses mean anything

```bash
cargo test --locked --release -p vice-bench --test dcel_harness -- --ignored --nocapture
```

Three tests, each with a positive control in the same body:

- `a_stage_that_picks_a_winner_is_visible_to_clause_one` — production carries
  every topology through; `ProxyKnockout::Select` loses classes;
- `an_edit_reaching_outside_its_roi_is_refused_on_every_arm` — the clean edit
  commits somewhere; `RoiKnockout::Reach` commits nowhere;
- `the_production_run_has_a_population_for_every_clause` — every clause's
  population is non-empty and every asserted zero is zero.

## 4. The proof domain, on all three axes

```bash
cargo test --locked --release -p vice-topology --test dcel_props -- --ignored --nocapture
```

- `the_audit_is_green_over_every_labelling_of_a_four_by_four` — 131 072
  arrangements, 11 topological classes, 41 678 labellings with a critical 2×2;
- `the_audit_holds_on_the_structural_register_at_every_declared_size` — the five
  structural fixtures at 32, 64, 128, 256 and 512 px under both convention arms.

The 4×3 sweep (8192 arrangements) runs in the DEFAULT path, so the exhaustive
axis is never entirely absent from `cargo test --workspace`.

## 5. What a reviewer should try to break

Suggestions, not instructions — the point of a red team is that it chooses.

1. **Add a field to `Parts`** without a site in `Parts::perturbations`. It must
   not compile. That is the mechanism clause 4 stands on.
2. **Edit `dcel::lattice::Arrangement::succ`** so it behaves differently only
   above some size. The size axis is supposed to make that visible; F-8 is the
   record of the last time it was not.
3. **Make `assemble` wrong in a way that agrees with itself** — that is the
   documented blind spot of `is_the_assembly_of_its_own_labelling`, and the
   construction invariants are what is supposed to catch it.
4. **Relax a `[dcel]` threshold and change the code that meets it in one
   commit.** `gates-check` should refuse.
5. **Find an arm where the DCEL and `topology::independent` agree and both are
   wrong.** They share the labelling and the convention; that shared link is
   named in the clause-2 row rather than left to be discovered.

## 6. What could NOT be verified locally, and is therefore not claimed

- **CI.** F-2 (build separated from measurement) and M4-N9 (the frozen-gate step
  refusing instead of printing a NOTE) are properties of a RUNNER. Both are
  written and both are readable in `.github/workflows/ci.yml`; neither has been
  executed by the author, and a workflow that has not run is a claim.
- **Tier B / cross-platform correctness.** Author's platform is
  `windows-x86_64`, the same `(os, arch)` the artifact records. The behaviour of
  `dcel-check --structural` on ubuntu is unverified here. That is A7.1, owner
  M12.
- **Donor sources.** Not opened (D-3).
