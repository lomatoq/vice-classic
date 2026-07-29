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

Author's counts on `windows-x86_64`, rustc 1.96.0, after delta-6: **549 passed,
0 failed** in each profile.

**The `--ignored` run, and its EXIT CODE** — which is the thing that matters,
and which this file got wrong:

```bash
cargo test --locked --release --workspace -- --ignored
```

It exited **101** before delta-1. `-- --ignored` un-ignores doctests, and
`crates/vice-bench/src/gt/legal.rs` carried a fenced `ignore` block that is a
deliberate illustration of a past defect and cannot compile. This file said
"11 passed" — true of the passing tests, silent about the return code — so a
reviewer following the instruction got red (REVIEW_M5_B N5). The fence is a
plain `text` fence now.

**Reproducibility is a command's exit code, not a number chosen out of its
output.** The number that was chosen was published by C247, a commit devoted
entirely to correcting that very number, and it was still the wrong kind of
statement.

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
  (7 from the CORPUS, 3 from the structural register)
transactions 167 attempted, 167 committed, 0 rolled back
unrelated chains 127, moved 0
audit resolving power: 30 arrangements of 480 arms, 179 253 slots,
  caught by audit 179 253, UNCAUGHT 0, no-ops 0
arms with a loop of >=3 half-edges: 20 (14 corpus, 6 register), longest 8
    vertices              82        boundaries.owners     90
    boundaries.endpoints  90        boundaries.path    10 058
    faces.label           67        faces.loops           90
    face_of_padded_px 150 644       site                 270
```

**Read the decomposition, not the total.** `face_of_padded_px` is 150 644 of the
161 391 slots — 93 % — and for that family a catch is guaranteed by the SHAPE of
the check: the perturbation moves the map and the rebuild reconstructs it from
`boundaries`, which the perturbation does not touch. "161 391 of 161 391" is
true and reads as a coverage it is not (REDTEAM_M5 RT5-A10). The breakdown is in
the artifact so a reviewer does not have to reconstruct it.

The slot count rose from 155 160 because `path[j].1` is perturbed at last
(REVIEW_M5_A D1-N2 — delta-1 reported this fixed and it was not; F-0067) and
because the owners are two sites rather than one, so the site count equals the
count of scalar leaves of the serialized `Parts`. 29 probes rather than 28: the
probe now takes the first arm of EACH judge branch deterministically, because
clause 4's green rested on arm order (REVIEW_M5_B N11).

Two of those lines changed in delta-1, and both changes are the point of it.

The convention-dependent split is **computed from the printed set** rather than
asserted beside it. The row used to say every such group came from the
structural register, carrying STATUS_M4_5 limitation 18 (`zero of 132 arms`)
onto M5's 444-arm population over different cells without recomputing it — and
the list printed in the same sentence refuted it (REVIEW_M5_B N1).

The last line read `audit 5648, assembly 155 160, neither 0`, and **two of those
three numbers were arithmetic**: a perturbed value is by construction not the
assembly of its own labelling, so `caught_by_neither == 0` could not be
otherwise. What the clause really asked of the audit was ONE caught slot, and
REDTEAM_M5 RT5-A2 deleted the entire seventh §12 invariant and kept the gate
green. It now asks that the audit ALONE reject every perturbation — and it does,
after `dcel::crossing` gave a predicate to `face_of_padded_px`, the largest field
of the structure, which until delta-1 no predicate read at all (RT5-A1).

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

Five tests, each with a positive control in the same body:

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
  arrangements, **12** topological classes, 41 678 labellings with a critical
  2×2. It said 11 until delta-1: C243 stopped skipping the two empty labellings
  and the twelfth class, `(0, 0)`, is exactly what they contribute. The test
  asserts the class SET exactly now, because `>= 8` was a floor and a floor
  cannot see a count drift upward (REVIEW_M5_A N8c);
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
3. **Make `assemble` wrong in a way that agrees with itself.** This is the one
   that worked three times — RT5-A1, RT5-A9 and RT5-A13 — and the third did not
   need the map at all: reordering two half-edges inside one loop leaves every
   pixel, every owner and every count where they were. It worked twice before
   delta-3 as: RT5-A1 (a field no predicate read) and RT5-A9 (a
   relabelling that every check reproduced, because the cross-check's input is
   sampled out of what it checks). Try it a third time, and ask the question the
   reviewers wrote: not "does my check look different" but **"what is the
   largest corruption of `assemble` that my check reproduces"**. For
   `crossing::face_map_agrees` that set is stated in its own doc comment —
   every permutation of face ids fixing the exterior — and the labelling anchor
   is what lies outside it.
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
