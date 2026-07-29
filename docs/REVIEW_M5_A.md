# REVIEW_M5_A — independent cold review A of milestone M5

Reviewer: independent cold reviewer A (Claude Opus 5). Not the author. Author's conclusions treated as claims under test. No code was fixed.

> Публикуется **дословно**, как вернулось из холодного контекста. Governor не редактировал и не сокращал. Подписанный артефакт: изменению не подлежит, только addendum. §34 требует для M5 ДВА независимых cold review РАЗНЫХ модельных семейств плюс отдельный red-team pass; это первый из двух.

## §0. Hygiene, at the start

```
$ cd C:/Users/nirrt/Toolset/vice-classic
$ git status --porcelain
                                  # (empty — clean)
$ git rev-parse HEAD
496a256c7b723912f3f4642d5f0e9c768b892138
```

Spec SHA-256, checked as the first action:

```
$ Get-FileHash -Algorithm SHA256 C:\Users\nirrt\Downloads\VICE_CLASSIC_CORE_AGENT_SPEC_v1.3.md
652FD0B6E17C96C38AF0173DDCC93A3921EAFD60A9AFF34C8D848829228D9BB1
```

Matches the required digest `652fd0b6e17c96c38af0173ddcc93a3921eafd60a9aff34c8d848829228d9bb1`.

**Isolation.** The main tree was read only. Two clones, both by-construction-unique names, both `git clone` (no `git worktree`, no `git worktree prune`):

- `…/scratchpad/m5-ra-coldrev7k` — the **measuring clone**. `CARGO_TARGET_DIR=…/scratchpad/tgt-m5-ra-coldrev7k`, verified not to exist before the first command. Every number reported in §1 and §2 comes from this clone, and its `git status --porcelain` was empty before and after every reported run.
- `…/scratchpad/m5-ra-coldrev7k-exp` — the **experiment clone**, deliberately dirtied with my own probes. `CARGO_TARGET_DIR=…/scratchpad/tgt-m5-ra-coldrev7k-exp`. Every number from it is labelled as mine and is never offered as a production measurement.

`git worktree list` on the main repo shows one entry, the main tree itself — no worktree was created or pruned.

Donor sources (`v-ice`, `v-ize`, `v-ice part`) were not opened (D-3).

---

## §1. What was checked, and how

Every number below was produced by a command I ran. Nothing here is copied from a document.

### 1.1 `cargo fmt --all --check`

```
FMT_EXIT=0
```
No output, exit 0.

### 1.2 `cargo clippy --workspace --all-targets -- -D warnings` — CLEAN target dir

The target directory was deleted and its non-existence asserted before the run, because a warm clippy prints nothing and is indistinguishable from not having run it (the standing note from M4.5).

```
=== TARGET DIR (must not exist before): …\tgt-m5-ra-coldrev7k ===
False
…
   Compiling proc-macro2 v1.0.107
   [104 lines: full dependency graph + all nine workspace crates compiled from scratch]
    Checking vice-topology v0.0.1 (…\m5-ra-coldrev7k\crates\vice-topology)
    Checking vice-bench v0.0.1 (…\m5-ra-coldrev7k\crates\vice-bench)
    Checking vice-cli v0.0.1 (…\m5-ra-coldrev7k\crates\vice-cli)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 21.15s
CLIPPY_EXIT=0
```
105 lines of output, all of them compilation progress. Zero diagnostics. This is a real clean-tree clippy run, not a cache hit.

### 1.3 `cargo test --locked --workspace` and `--release`

Summed across all test binaries by script, not by eye:

```
DEBUG_EXIT=0     DEBUG:   sum passed = 530  failed = 0  ignored = 12  (43 test binaries)
RELEASE_EXIT=0   RELEASE: passed = 530      failed = 0  ignored = 12
```

**530 in both profiles — the author's count is confirmed by summation.** The `ignored = 12` is 11 `#[ignore]` tests plus one `ignore`-marked doctest; see N14.

### 1.4 The §28 M5 gate

```
$ git status --porcelain          # clone clean before the run — empty
$ cargo run --locked --release --bin gt-corpus -- dcel --out runs/m5/report.json --scope full
dcel: 41 scenes, 474 arms (444 corpus, 30 structural), 0 refused, 22 sealed-audit groups skipped
  classes [(0, 0), (1, 0), (1, 1), (2, 0), (2, 1), (3, 0), (3, 1), (5, 0)]; groups 237, classes in 247 out 247, convention-dependent 10
  transactions 167 attempted, 167 committed, 0 rolled back; unrelated chains 127, moved 0
  audit resolving power: 28 arrangements of 474 arms, 155160 slots, audit 5648, assembly 155160, neither 0
M5 gate table (all four clauses of the spec):
  [MET] no final-topology claim from proxy: …
  [MET] candidate recall maintained after budget pruning: …
  [MET] no unrelated graph mutation: …
  [MET] no dangling/invalid faces: …
GATE_EXIT=0
$ git status --porcelain          # clone clean after the run — empty
```

Four `[MET]`, exit 0. Every headline count the author reports reproduces exactly.

### 1.5 Artifact reproduction

```
fresh run : 64939C1BC1F5751413B73127370F7D778F02D23BFB015F9F0102B756AB2A03A2
committed : 64939C1BC1F5751413B73127370F7D778F02D23BFB015F9F0102B756AB2A03A2
RESULT: BYTE-IDENTICAL
```

And the Tier A re-run comparison, without `--structural`, on the recording platform (`windows-x86_64`):

```
$ cargo run --locked --release --bin gt-corpus -- dcel-check --report docs/gt/DCEL_M5.json
dcel report reproduced with every metric compared
DCELCHECK_EXIT=0
```

**Byte-reproducibility of `DCEL_M5.json` is confirmed.**

### 1.6 The `#[ignore]`d M5 mechanisms

```
$ cargo test --locked --release --workspace -- --ignored --nocapture
[dcel_harness]        4 passed, 0 failed        (249.20s)
[frozen_calibration]  5 passed, 0 failed        (312.41s)
[dcel_props]          2 passed, 0 failed          (3.34s)
                     11 passed total — matches the author's "11 passed"
4x4 exhaustive: 131072 arrangements, 12 classes
  [(0,0),(1,0),(1,1),(1,2),(2,0),(2,1),(3,0),(4,0),(5,0),(6,0),(7,0),(8,0)],
  41678 labellings with a critical 2x2
test the_audit_holds_on_the_structural_register_at_every_declared_size ... ok
test the_audit_is_green_over_every_labelling_of_a_four_by_four ... ok

test crates\vice-bench\src\gt\legal.rs - gt::legal (line 35) ... FAILED
IGNORED_EXIT=101
```

Two facts here. The three knockouts and both proof-domain axis tests **pass** — including the 256/512 size axis and the full 4×4 sweep. And two things do not match the documents: the sweep reports **12** topological classes where three documents say 11 (N8), and the workspace-wide `-- --ignored` command exits 101 on a documentation snippet (N14, pre-existing, not an M5 defect).

### 1.7 §27.7 for the `[dcel]` thresholds — verified independently

```
$ git log --oneline d624e81..496a256 -- configs/GATES_V1.toml
8dedc55 C241 M5(gate file): the six M5 population floors are frozen, from a run, with no code
52739fb C239 M5(gate file): the four M5 population thresholds are registered, as a placeholder, with no code
$ git show --name-only 52739fb  →  configs/GATES_V1.toml
$ git show --name-only 8dedc55  →  configs/GATES_V1.toml
```

Exactly two commits touch the gate file in the M5 range and each touches nothing else. **§27.7 is kept.** (The gate file's *comment* is a separate matter — N8b.)

### 1.8 What I read

Spec §12, §5.3, §5.4, §5.5, §11.3–§11.5, §27.7, §28 M5, §32, §34, §36, §4/§4.1. `STATUS_M5.md`, `REPRODUCIBILITY_M5.md`, ADR-0031, `REQUIREMENTS_TRACEABILITY.md`, `FAILURE_LEDGER.md` (meta-rules M-1…M-4, F-0048, F-0054…F-0060), `STATUS_M4_5.md` §17. Every M5 source file in full: `dcel/{mod,audit,lattice,walk,transaction,certificate,fixtures}.rs`, `vice-bench/src/dcel/{mod,report}.rs`, `bin/gt-corpus/dcel_cmd.rs`, `tests/{dcel_harness,dcel_props,doc_claims,hygiene}.rs`, `.github/workflows/ci.yml`, `configs/GATES_V1.toml`.

---

## §2. Gate table, §28 M5, clause by clause, with my measurement

| Clause | Author | **My verdict** | My measurement, and what it does and does not carry |
|---|---|---|---|
| **no final-topology claim from proxy** | MET | **MET, evidence sound** | Reproduced: 237 groups, 247 classes in, 247 out, 10 convention-dependent groups. I confirmed `apply`'s signature carries no cost/bound/score parameter — the property is held by the signature, not by prose. Knockout `ProxyKnockout::Select` runs and takes the row down. **Caveat (N11):** each group has exactly two members (the two connectivity arms); the three `SaddleResolution` hypotheses `vice_topology::cubical` generates never reach the DCEL, so the set whose size is preserved was already narrowed upstream of the measurement. Not a §32 rule 14 violation — M5 does not do the narrowing — but undeclared. |
| **candidate recall maintained after budget pruning** | MET | **MET** | Reproduced: 0 of 474 arms disagree with `topology::independent`; 0 fail `V − B + L = 2C`; 8 distinct classes. The two conjuncts genuinely share nothing (union-find over pixels vs. loop extraction plus union-find over vertices). The shared link — both read the same labelling and convention — is named in the row rather than hidden. **Caveat (N12):** one of the 8 classes, `(0,0)`, is carried by exactly 8 arms, all `adv/sliver#a`, all with `directed_steps == 0` — arms the report elsewhere says no clause may stand on. |
| **no unrelated graph mutation** | MET | **MET, evidence materially misleading** | Reproduced: 167/167 committed, 127 unrelated chains, 0 moved. The chain comparison is by lattice path rather than id, which is the right call and I could not break it. **But (N2):** the population is selected by outcome. 307 of 474 arms are silently dropped by `transaction_for`'s `_ => return None`; only 2 of 4 `EditKind`s ever occur; and 4 of `apply`'s 6 refusal reasons are unreachable by construction, so "0 rolled back" is not the measurement it reads as. **And (N8a):** the row's own caveat count is published as 56 in STATUS and is 64 in the artifact. |
| **no dangling/invalid faces** | MET | **MET, evidence self-contradictory** | Reproduced: 0 arms fail the audit, 0 are not their own assembly, 155 160 slots perturbed, `caught_by_neither = 0`, `no_ops = 0`. The 4×4 exhaustive sweep and the 32→512 size axis both pass under my run. **But:** the printed row states "8 were not the assembly of their own labelling" — a direct contradiction of a conjunct the same row requires to be zero (N1, swapped format arguments); one of its two conjuncts cannot be false on this population (N7); and the walk is not the one-per-scalar-slot exhaustion it is described as (N4). |

**Summary of §2.** All four clauses are genuinely MET and I could not move any of them. The defects are in the *evidence* of clauses 3 and 4 — in what makes MET mean something — not in the verdicts.

---

## §3. Inherited obligations: are the prices honest?

The author counted thirteen items in, five closed, eight carried. I checked each closure claim against the tree.

| Item | Author's verdict | **Mine** |
|---|---|---|
| **F-2** (build/measurement separated in CI) | CLOSED IN CODE, not executed | **Agreed, and the code is there.** `.github/workflows/ci.yml:47` and `:92` add "Build the measurement binaries (this step does not measure)" to both measuring jobs. Honest price, honestly caveated. Note the `checks` job also measures (`frozen_calibration --ignored`) and has no separate build step, but its `Tests (release)` step builds first, so the exit-code confusion F-2 names cannot arise there. |
| **M4-N9** (CI refuses instead of printing a NOTE) | CLOSED IN CODE, not executed | **Agreed.** `ci.yml:115–139`: the NOTE is gone, the base is reconstructed from the default branch, and the fallback is `exit 2` with a message that says the instrument did not earn a pass. Correctly implements meta-rule M-4. Unexecuted, as stated. |
| **F-7** (mechanism-existence sees a phrase, not behaviour) | CLOSED FOR M5's OWN MECHANISMS | **NOT closed as stated — see N3.** The knockouts exist in the tree and pass when I run them by hand. But the closure claim is "run in CI", and no CI step runs them. A mechanism whose only trigger is a reviewer typing a command is the F-7 condition restated, not its closure. |
| **F-8** (judge's 256/512 gap) | CLOSED FOR THE M5 INSTRUMENTS | **Agreed on the mathematics** — `the_audit_holds_on_the_structural_register_at_every_declared_size` passes at 256 and 512 under both arms in my run. **Qualified by N3:** the test that establishes it is `#[ignore]`d and executed by no automated path. |
| **F-9** (proof domain < application domain) | CLOSED AS A CLASS FOR M5, AND VIOLATED ONCE HERE (F-0058) | **The self-report is accurate and creditable, and there is a second instance the author did not find — N2.** The transaction population excludes 65 % of arms by construction, in the function whose comment denies exactly that. |
| **cond. 51** (third axis / triple junction) | CLOSED, with a correction | **Agreed, and the mathematics is CORRECT — I verified it independently.** See §4 preamble below. Reporting it as "closed with a correction" rather than "closed" is the right call. |
| **F-1, F-3, F-5** | CARRIED, owner M7 | Prices honest. §4.1's architectural argument ("no in-process mechanism defeats an attacker who controls the build environment") is correct and is not a dodge. |
| **F-4** | CARRIED, owner M8 | Price honest; M5 genuinely added no corpus type. |
| **F-6** | CARRIED and narrowed, owner M6 | Agreed. |
| **oblig. 18** (third ambiguity pair) | CARRIED, owner M8 | Price honest — re-keying 1086 digests and touching the sealed-audit seal is a reviewed change of its own under §27.1. The substitute (structural register carrying a convention-dependent fixture at every size) genuinely gives clause 1 a population of 10 rather than a singleton; I verified all 10 in the run. |
| **M4-N8** | OPEN, owner M6, price in §4.2 | Price is stated in unusual and creditable detail (rewiring → half-pixel shift → corridor recalibration → a frozen gate value → `CORRIDOR_M4.json`). I have no reason to doubt it. |
| **A7.1 / Tier B** | NOT CLOSED, visible, owner M12 | Correct. The report explicitly declines to offer the wide integer projection as Tier B. Right call. |
| **limitation 32** (`continuation.rs` says DCEL absent) | CARRIED, owner M6 | Correct; §27.1 does forbid moving `TOPOLOGY_M4_5.json` in a feature commit. |
| **F-0060 / limitation 36** | Partially closed, full price named, owner M6 | The self-diagnosis is right and the mechanism change (invert the default, walk `docs/`) is the right shape. **But the declared blast radius is too small — see N8.** The author predicted one bypass ("a number in an M5 gate-row evidence cell that no artifact carries"). It occurred there *and* in the frozen gate file *and* in an accepted ADR. |

**On "F-2 and M4-N9 are closed in code but not executed":** the author states this plainly and repeatedly, and that is exactly right. I want to separate it sharply from N3, which is a different thing: N3 is not "CI was not run", it is "the workflow file does not contain the steps four documents say it contains", and that is checkable by reading, with no runner.

---

## §4. Findings

**First, what survived.** Before the findings, three claims I attacked hard and could not break, because a review that lists only defects misreports the milestone:

- **Condition 51's mathematics is correct, and I verified it myself rather than accepting it.** Around a lattice point the four incident segments are the four adjacent pairs of the 4-cycle (NW, NE, SE, SW); the number of disagreeing adjacent pairs in a cyclic boolean sequence of length four is even; hence the degree is 0, 2 or 4 and never 3. At the canvas border all out-of-canvas pixels are background, so the parity argument extends unchanged and the `in_lattice` guard in `Arrangement::exists` is redundant there rather than parity-breaking. A triple junction needs three labels. **The author is right, the substitution of `diagonal_pinch` is sound, and the honest thing is that it was reported as a correction rather than slipped in.**
- **The invariants really are held at the crate boundary.** From outside `vice-topology` I could find no way to obtain, build or mutate an invalid `Dcel`: fields are private, there is one constructor, and every accessor is `&`-only. `Boundary` and `Face` are re-exported with public fields, but nothing accepts one.
- **The no-new-crate decision is correct on the spec, not on authority.** §4's target structure names `vice-topology` as carrying "planar graph"; §12 is titled "Stage E — robust shared planar graph"; a `vice-dcel` crate is not in §4, so creating one would require an ADR arguing *against* §4 rather than from it. §4.1's module-size rule is met (largest module 718 lines). **I would have accepted the author's reasoning even if I had been the one who proposed the crate.** One half of the argument is unmade — see N13.

---

### M5A-N1 — MAJOR. The clause-4 gate row misreports two of its own quantities; the arguments are swapped.

**File:line.** `crates/vice-bench/src/dcel/report.rs:429` and `:430` (the `format!` argument list), against the sentence at `:412–414`.

**Reproduction.** From my gate run in §1.4, the printed clause-4 row reads:

> "0 of 466 non-empty arms failed the audit, and **8 were not the assembly of their own labelling**. A further **0** arm(s) carry a valid but EMPTY arrangement…"

Against the artifact:

```
arms_failing_the_audit               = 0
arms_with_a_non_empty_arrangement    = 466
arms_with_an_empty_arrangement       = 8      ← printed as "not their own assembly"
arms_that_are_not_their_own_assembly = 0      ← printed as "empty arrangement"
```

The third and fourth arguments are bound to each other's sentence position.

**Why it matters, precisely.** The row states that 8 arms failed a check that the *same row's* conjunction requires to be zero (`report.rs:308`), and the row nonetheless reads `[MET]`. A reader of the §28 M5 gate output sees a clause that certifies itself while its evidence says the opposite. The booleans read the correct fields, so no verdict is wrong — but under this project's own condition B3 ("what a STATUS or REPRODUCIBILITY presents as measured must be measured"), the evidence column is the artefact of record for a spec clause, and it is false.

**General rule of the class.** When a gate row's evidence is a `format!` with N positional holes and N field expressions, *nothing binds hole i to field i* — not the compiler, not a test, not a reviewer reading either side alone. The project already built the mechanism that fixes exactly this: positional binding of gate-row numbers to artifact keys in `doc_claims.rs`. M5's rows sit in no tier (limitation 36), and this is the first thing that fell through. The rule: **a gate row must publish quantities through a mechanism that binds each number to its name, never through positional formatting.**

---

### M5A-N2 — MAJOR. Clause 3's transaction population is selected by outcome, in the function whose comment denies it. Second instance of F-0058.

**File:line.** `crates/vice-bench/src/dcel/mod.rs:508` (`_ => return None`), against the doc comment at `:462–469`, which reads: *"the population is not selected by what happens to work."*

**Reproduction** (mine, experiment clone, full scope):

```
REV-A FULL: arms=474, arms with size>=16 = 474, arms WITH a transaction = 167
REV-A FULL: arms silently dropped by `_ => return None` = 307
REV-A FULL: declared kinds actually exercised = {"bridge_close": 39, "gap_open": 128}
REV-A FULL: rolled_back = 0
```

Every arm is ≥ 16 px, so the size guard drops none: **all 307 exclusions are the `_ => return None` arm.** `HoleOpen` and `HoleFill` never occur in the production run; two of the four branches of `apply`'s declared-edit check are never exercised on the corpus.

**Why "0 rolled back" is not a measurement.** `transaction_for` derives `kind` from the base-vs-after signature comparison, and `apply` then re-performs that same comparison. Therefore, in the production run:

| refusal | reachable on the corpus? | why not |
|---|---|---|
| `EditLeftTheCanvas` | no | the ROI is derived from the canvas |
| `EditLeftTheRoi` | no | `set` is built from the ROI |
| `EditIsANoOp` | no | a no-op has delta `(0,0)`, which returns `None` before `apply` is called |
| `NotTheDeclaredEdit` | **no** | `kind` is read off the very comparison `apply` redoes |
| `UnrelatedGraphMutation` | yes | this is the clause |
| `CandidateFailedAudit` | yes | |

Four of six cannot fire. "167 attempted, 167 committed, 0 rolled back" is published in the gate row as if the transaction machinery had been exercised and had refused nothing; in fact the population was filtered to the edits that would commit, and the two live refusal paths are the ones the clause asserts to be zero anyway.

**General rule of the class.** This is **F-0058 a second time, in the same milestone, and undeclared**: a mechanism that excludes a subclass by construction and then publishes the surviving subclass's clean result. It is worse than the first instance in one respect — the excluded subclass is precisely what §28 M5 names, *"local **compound** topology transactions"*: every edit whose effect on the signature is not a single clean `±1` is dropped rather than attempted. The rule the author wrote for F-0058 applies verbatim to his own harness: **a filter that decides membership by looking at the answer is not a population, and the count it excludes must be published beside the count it keeps.**

---

### M5A-N3 — MAJOR. Neither the §28 M5 gate nor any M5 knockout is run by CI, while four documents state that they are.

**File:line.** `.github/workflows/ci.yml`. Exhaustive enumeration from the file:

```
$ grep -o "gt-corpus -- [a-z-]*" .github/workflows/ci.yml | sort -u
gates-check  audit-status  build  verify  report  oracle  corridor  corridor-check
oracle-check  topology  topology-check  dcel-check
                                        ^^^^^^^^^^ — the only `dcel` subcommand in the workflow

$ grep -- "--ignored" .github/workflows/ci.yml
33:  run: cargo test --locked --release -p vice-bench --test frozen_calibration -- --ignored --nocapture
                                        ^^^^^^^^^^^^^^^^^^ — the only --ignored step in the workflow

$ grep -rn "dcel_harness\|dcel_props\|dcel --scope" .github/
(no matches)
```

Every previous milestone has a step that **runs** its gate: `report` (M3), `oracle` (M3.5/M4), `corridor` (M4), `topology` (M4.5). **M5 has none.** `dcel-check` (line 298, `tier-a-digests` job) re-runs the harness and compares the artifact, but it never calls `gate_table` — so the four clause verdicts are computed by no CI step, and the six frozen `[dcel]` thresholds are read by no CI step.

**Claims contradicted, all four checkable by reading:**

- `crates/vice-bench/tests/dcel_harness.rs:11–12` — "CI runs them in release beside the other corpus-wide measurements"
- `crates/vice-topology/tests/dcel_props.rs:262–263` and `:280` — "it runs in release in CI"
- `docs/STATUS_M5.md`, F-7 row — "behavioural knockouts in the tree, each with a positive control, **run in CI**"
- `docs/adr/ADR-0031…md:85` — "(`#[ignore]`, release CI)"

**Second consequence, on F-9.** Two of the three declared proof-domain axes — the exhaustive 4×4 sweep and the 256/512 size axis — are `#[ignore]`d and executed by nothing automatic. They exist, they pass when I run them (§1.6), and no push will ever notice if they stop.

**Why this is not covered by "CI has not been executed."** The author states plainly and correctly that he has not run CI, and that F-2 and M4-N9 are therefore claims. This finding is different in kind: the *contents* of the workflow file are readable without a runner, and the steps four documents describe are not in it. Under F-7's own rule — a check on the *presence* of a mechanism sees a phrase, not a behaviour — a status document asserting "run in CI" about a step that does not exist is the F-7 defect turned on the milestone that claimed to close it.

**General rule of the class.** **A sentence asserting where a mechanism runs is a claim about a file, and it must be derived from that file or it will drift.** The cheapest closure is a test that reads `.github/workflows/*.yml` and requires every `#[ignore]`d test whose reason string says "CI" to appear in some step's command.

---

### M5A-N4 — MAJOR. The mutation walk is not "one perturbation per scalar slot"; the compiler is the judge at field granularity only.

**File:line.** `crates/vice-topology/src/dcel/walk.rs:114–233`; the path loop at `:179–187`, which perturbs `pt.0` and never `pt.1`. Compare `vertices` (`:125`, `for k in 0..2`) and `site` (`:218`, `for k in 0..3`).

**Reproduction** (mine, 13×13 annulus — the same fixture ADR-0031 measures):

```
REV-A: enumerated by perturbations() = 312
REV-A: true scalar slots in Parts    = 372
REV-A: OMITTED slots                 = 60
REV-A: path perturbations = 58, path POINTS = 58  (=> 1 coord per point, not 2)
```

58 path *y*-coordinates plus 2 owner slots are never generated. I then perturbed each omitted *y* slot by hand:

```
REV-A: of the path-y slots -> audit 58, assembly 58, NEITHER 0
```

**Honest severity.** The omitted slots are *caught* when tested, so this does not open an uncaught hole today, and `caught_by_neither = 0` stands. What is false is the mechanism's own description, in three places: `walk.rs:17–18`, `audit.rs:33`, and ADR-0031 §3 — *"emits one perturbation per scalar slot of the actual data"*. `slots_perturbed = 155 160` is the size of a hand-written enumeration, not of the data, and it covers 84 % of it.

**General rule of the class.** F-0048 Q1 and Q3, applied one level down — and it is the same shape as the lesson the author himself wrote in F-0060 about nested sets, applied to `Parts` instead of to documents. Exhaustive destructuring makes the compiler the judge of the **field** level; inside each field the enumeration is a hand-written literal, and adding a third element to a path point costs one line and no compile error. **Deriving one level of a nested structure from a property does not close the levels above or below it; each level needs its own Q1.**

---

### M5A-N5 — MAJOR. An invalid `Dcel` compiles inside `vice-topology` using existing crate-visible API, and the audit *panics* on one.

**File:line.** `crates/vice-topology/src/dcel/walk.rs:239` (`pub(crate) fn with_parts`), `audit.rs:62–66` (`Parts` fields `pub(crate)`), and the panic at `audit.rs:344`.

**Reproduction.** In a **new module of the crate** — the exact situation `mod.rs:38–49` describes as costing "one `pub fn`" — three lines, no new constructor, no `unsafe`:

```rust
let d = seed().with_parts(Parts {
    vertices: vec![], boundaries: vec![], faces: vec![],
    face_of_padded_px: vec![], site: vec![],
});
```

It compiles. Output:

```
REV-A: built a Dcel with 0 faces
thread '…' panicked at crates\vice-topology\src\dcel\audit.rs:344:8
```

A second probe — a face with an empty loop list and a one-point boundary — also compiles, and there the audit behaves correctly: `MalformedPath { boundary: 0, "path has 1 point(s)" }`, `is_the_assembly_of_its_own_labelling = false`.

**Two distinct defects.**

1. **The documented residual is understated.** `mod.rs:38–49`, ADR-0031 §2 and STATUS §1 all price the cheapest bypass as "a second constructor — one `pub fn` in `vice-topology` taking pieces and assembling them by hand." It is cheaper than that: **the arbitrary-state setter already exists**, is `pub(crate)`, and is reachable from every module of the crate, with `Parts` fields `pub(crate)` beside it. The honest statement of the boundary — which F-0048's last paragraph requires to be made *at the cheapest known bypass price* — is off by a constructor.

2. **The audit panics where its own comment says it must not.** `audit.rs:313–316` states the rule explicitly: *"an audit that panicked on a structure it was asked to judge would report a broken instrument as a crash. An instrument says what it found (M-4)."* A range guard was added for the `site` index. `audit.rs:344` indexes `faces[0]` with no guard, and that is the line that panicked.

Neither is reachable from outside the crate, and neither is reachable in production (perturbations never delete elements), so this is a discipline defect, not a live crash.

**General rule of the class.** **Meta-rule M-1, verbatim**: the rule was applied to the site where the defect was found (`site`) and not to the class (every unchecked index in a function that promises to return `Err` rather than unwind). And: **when a claim's honesty rests on naming the cheapest bypass, the cheapest bypass must be searched for, not assumed** — here the search would have found a `pub(crate) fn` the author wrote himself, in the same milestone.

---

### M5A-N6 — MAJOR. `incidence_signature` is algebraically `(|V|, 2|B|)` and carries no incidence information.

**File:line.** `crates/vice-topology/src/dcel/certificate.rs:135–142`; the claim at `:60–62`.

**Proof.** Each boundary increments `degree[start]` once and `degree[end]` once (`:138–139`), so `sum(degree) ≡ 2·boundaries.len()` for every arrangement, unconditionally. Measured across four sizes:

```
REV-A: 13x13: incidence=(2,4)  |V|=2 |B|=2 2|B|=4
REV-A:   9x9: incidence=(2,4)  |V|=2 |B|=2 2|B|=4
REV-A: 21x15: incidence=(2,4)  |V|=2 |B|=2 2|B|=4
REV-A: 32x32: incidence=(2,4)  |V|=2 |B|=2 2|B|=4
```

**Why it matters.** This quantity is published as *"§12's FIRST isotopy condition and the only one M5 can evaluate"* (`certificate.rs:61–62`) and as *"the first, incidence, is computed and published as the executed half"* (STATUS §1). It is a restatement of two counts the same `AuditReport` already publishes. It cannot distinguish two arrangements with equal `|V|` and `|B|` but different degree distributions — which is exactly what "junction incidence" means. The doc calls it "the multiset of vertex degrees, summarised as (vertices, sum of degrees)"; that summary is constant given `|B|`. The per-vertex degrees *are* computed at `:137–140` and then discarded.

**Downstream.** `dcel_props.rs:132–135` detects "a junction" as `deg > 2*v`, which reduces to `|B| > |V|`. The assertion `junctions_seen == 6` holds, but it is a counts comparison, not a junction test.

**General rule of the class.** `report.rs`'s own header, applied to the certificate instead of to the gate rows: **"a conjunct implied by its neighbour is a paraphrase, not a second witness" (M45-N8, RT45-A6).** A published field that is an algebraic function of two other published fields is a paraphrase of them, and naming it after the property it does not measure is the part that costs something.

---

### M5A-N7 — MAJOR. Clause 4 carries a conjunct that cannot be false on its population.

**File:line.** `crates/vice-bench/src/dcel/mod.rs:391–394`; the conjunct at `report.rs:308`.

**Argument.** Each arm is built as `d = Dcel::assemble(L, conn)` and then checked with `is_the_assembly_of_its_own_labelling(&d)`, which computes `Dcel::assemble(d.labelling().clone(), d.connectivity()).parts() == d.parts()`. `assemble` is a deterministic pure function — ordered containers throughout, no float in the decision path (which the milestone establishes elsewhere and I confirmed). The comparison is therefore `assemble(L,c) == assemble(L,c)`. Measured: `true` on 474/474, and it can be nothing else.

**What it does and does not measure.** It *does* measure repeat-determinism of `assemble`, which is a genuine §5.5 Tier A property and would catch, say, a hash-order dependence. It does **not** measure what the row says it measures: the row cites it as the check that catches "a value nobody assembled," and among the 474 arms that class has population zero. The check earns its keep in the *mutation walk*, where `Parts` is corrupted — not on the arms.

**General rule of the class.** **F-0035 / M45-N5, quoted at the top of the very file that commits the error**: "a conjunct that cannot be false measures the size of the input." The author applied that rule to three conjuncts of these rows and not to the fourth. The rule to add: **before conjoining a check over a population, ask what member of *that* population could fail it — not what value in general could.**

---

### M5A-N8 — MAJOR. A class of stale and false numbers reaching the frozen gate file and an accepted ADR, wider than limitation 36 declares.

Every row below is verified against a run or against the file system.

| | Site | Says | Actual |
|---|---|---|---|
| **a** | `docs/STATUS_M5.md:124` (row T6) and `:249` (limitation 35) | "56 transactions had no unrelated chain" | **64** (`transactions_with_no_unrelated_population`) |
| **b** | `configs/GATES_V1.toml`, `[dcel]`, comment above `gate_min_transactions` | "Measured **159** of 167 attempted" | **167** of 167 |
| **c** | `docs/STATUS_M5.md:101`, `docs/REPRODUCIBILITY_M5.md:101`, `ADR-0031:85–86` | 4×4 sweep sees "**11** classes" | **12** — my run §1.6: `[(0,0),(1,0),(1,1),(1,2),(2,0),(2,1),(3,0),(4,0),(5,0),(6,0),(7,0),(8,0)]` |
| **d** | `ADR-0031:95` and `:100` | "four modules", "**four** files" | **seven** (`ls crates/vice-topology/src/dcel/`) |
| **e** | `ADR-0031:85`, `crates/vice-topology/tests/dcel_props.rs:257` | "131 **070** arrangements" | **131 072** (the assert at `:268` is `2*(1<<16)`) |
| **f** | `crates/vice-topology/tests/dcel_props.rs:20` | structure axis = "annulus, nested annulus, bridge, two components, **triple junction**" | the fifth fixture is `diagonal_pinch` — the exact silent substitution limitation 29 says must not be made |
| **g** | `crates/vice-bench/src/dcel/mod.rs:330` | "every **twenty-fifth**" | `RESOLVING_POWER_STRIDE = 17` (`:333`) |
| **h** | `docs/REPRODUCIBILITY_M5.md:88` | "**Three** tests" in `dcel_harness` under `--ignored` | **four** (`the_full_scope_population_is_what_the_thresholds_were_read_from` is a fourth) |

**Shared provenance of (a), (b) and (c) — and it is the interesting part.** `docs/gt/DCEL_M5.json` has exactly one version in history, written at **C243**:

```
$ git log --oneline -- docs/gt/DCEL_M5.json
f55c64e C243 M5(F-9 on my own instrument): …
$ git log -S "56 transactions had no unrelated chain" --oneline -- docs/STATUS_M5.md
b2f209c C246 M5: STATUS, reproducibility, traceability …
```

Before C243 the eight `adv/sliver` arms failed the audit: their transactions rolled back — **159 of 167 committed**, and 64 − 8 = **56** with no unrelated chain — and the empty labelling was excluded from the sweep, so 4×4 saw **11** classes rather than 12, the twelfth being `(0,0)`, contributed by exactly the labelling C243 stopped skipping. **All three numbers are pre-C243 measurements carried into artefacts written or frozen after it.** C243's own commit message recomputes the *arrangement* counts (`131 070 → 131 072`) and not the *class* counts, and the ADR received neither correction.

**Why this exceeds what was declared.** Limitation 36 declares one instance and predicts one shape: "a number in an M5 gate-row evidence cell that no artifact carries." The prediction was right and the blast radius was not. The class reached (i) the STATUS gate table, (ii) `configs/GATES_V1.toml` — a **§27.7-governed frozen file** whose comment is its provenance record, and whose C241 commit message says the floors were frozen *"from a run"*, and (iii) an **accepted ADR**, where "four files" understates by nearly 2× the very price §32 requires the no-new-crate decision to be taken on. STATUS states the same price correctly as "seven files", so the two documents disagree about the cost of the decision they jointly justify.

**General rule of the class.** F-0028, and the reason it keeps recurring is visible here: **when a fix changes a measured quantity, the commit must enumerate every site that quotes any quantity the fix moved — not the sites it happens to remember.** C243 corrected two numbers of the four it invalidated. The mechanical version: the set of quoted numbers must be derived from the artifact (which is what positional binding does), and until it is, a commit that re-records an artifact owes a diff of every number in it.

---

### M5A-N9 — MINOR. Knockout 2 has no non-emptiness assertion and would pass on an empty population.

**File:line.** `crates/vice-bench/tests/dcel_harness.rs:93–111`.

Both assertions — `knocked.transactions_committed == 0` and `rolled_back == attempted` — are **vacuously true when `attempted == 0`**, and nothing asserts `knocked.transactions_attempted > 0`. The coupling that makes this live rather than theoretical: `RoiKnockout::Reach` adds a pixel to the set *before* `kind` is derived, so the knockout can move the population.

**Measured (mine, Test scope):** clean attempted 7, knocked attempted 11, committed 0, rolled back 11. **The population does not collapse today** — I checked, and I am reporting a latent hole, not a live one.

**General rule of the class.** F-0039 / RT45-A12, absent from the knockout written to demonstrate that very discipline: **a knockout must assert its own population, because "the knockout produced no violations" and "the knockout produced nothing" are indistinguishable from the assertion's side.**

---

### M5A-N10 — MINOR. The M5 traceability rows cite the wrong gate rows.

**File.** `REQUIREMENTS_TRACEABILITY.md`, M5 section, against `docs/STATUS_M5.md` §3.

- **M5-12** (proof domain covers application domain) cites "STATUS_M5 **T6**"; the proof-domain row is **T9**. T6 is "no unrelated graph mutation".
- **M5-13** (condition 51, structural register) cites "**T6**"; condition 51 is **T10**.
- **M5-9** (§11.4 compound transaction) cites "**T4**"; T4 is the proxy clause, the transaction clause is T6.
- **M5-11** (§12 isotopy condition) cites "**T5**"; T5 is the candidate-recall clause.

§32 rule 23 makes the gate column the last link of `invariant → implementation → tests → milestone gate`. Four of fourteen rows point elsewhere, and `doc_claims.rs` covers only the three M4.5 rows of this file.

---

### M5A-N11 — MINOR. The M5 population never contains a non-`Thresholded` saddle reading, and the narrowing is undeclared.

`Dcel::assemble` takes `(Labelling, ComplementaryConnectivity)` and no `SaddleResolution`; `grep -rn "SaddleResolution" crates/vice-topology/src/dcel/ crates/vice-bench/src/dcel/` returns nothing. The harness builds `inside = ink >= GT_MAJORITY_LEVEL` — plain thresholding. `vice_topology::cubical` generates three readings (`Thresholded`, `JoinDiagonal`, `SplitDiagonal`) and M4.5's envelope carries them.

Consequence: each clause-1 group has exactly two members, the two connectivity arms. "The number of distinct classes per group is the same going in and coming out" is measured on a set already narrower than the envelope produced upstream. **§32 rule 14 is not violated** — M5 does not do the narrowing, and the DCEL's exhaustive axis covers all 4×4 labellings including Join/Split ones. But limitation 31 declares only the estimated-evidence narrowing, not the saddle axis, and the two are different.

---

### M5A-N12 — OBSERVATION. `distinct_classes >= 3` counts a class no clause may stand on.

Class `(0,0)` has exactly 8 arms, all `adv/sliver#a`, all `directed_steps == 0`. The report carefully subtracts empty arms from every population floor and does not subtract them from `distinct_classes`, which clause 2 conjoins.

---

### M5A-N13 — OBSERVATION. ADR-0031 argues §4 for the DCEL and leaves half the argument unmade.

I judge the decision **correct** (see the §4 preamble). The unaddressed half: §4 also names `vice-opt` as *"continuous optimizer + **discrete transactions**"*, and `dcel::transaction` is a discrete transaction executed under §28 M5's "local compound topology transactions". The placement is defensible — §11.4 is Stage D, and §19's compound discrete search, which `vice-opt` serves, arrives at M7 — but ADR-0031 §6 argues only about the DCEL and never mentions the transaction module's §4 home. Since the ADR exists to argue *from* the spec, the half of §4 that points elsewhere belongs in it.

---

### M5A-N14 — OBSERVATION. `cargo test --workspace -- --ignored` exits 101 on a documentation snippet.

```
test crates\vice-bench\src\gt\legal.rs - gt::legal (line 35) ... FAILED
IGNORED_EXIT=101
```

`-- --ignored` un-ignores the ```` ```ignore ```` doctest at `crates/vice-bench/src/gt/legal.rs:35`, which is a deliberate illustration of a past defect and cannot compile. **Pre-existing, not introduced by M5, and not a code defect.** But `REPRODUCIBILITY_M5.md:33` states "**11 passed** under `-- --ignored` in release" without giving the command, and the natural workspace-wide reading of it exits non-zero. The count of 11 is exactly right for the eleven real `#[ignore]` tests — I confirmed it.

---

## §5. F-0048's five questions, applied to every M5 mechanism

| Mechanism | Q1 literal enumerating subjects? | Q2 next finding costs | Q3 who is the judge? | Q4 guard key = mechanism key? | Q5 checked both ways? | Verdict |
|---|---|---|---|---|---|---|
| `Dcel::assemble` as the only constructor | no | change the criterion | privacy + totality | n/a | outside the crate: yes. **Inside: no — N5** | **Strong at the crate boundary; residual mispriced** |
| `FacePair::new` (no dangling cracks) | no | criterion change | the type | n/a | **yes** — `a_face_pair_with_one_owner_is_not_constructible` asserts refusal *and* that a legal pair still constructs | **Clean. The best mechanism in the milestone** |
| `HalfEdgeId::twin` (bit flip) | no | n/a | arithmetic | n/a | yes | **Clean** |
| `Arrangement::succ` / `successor_is_a_permutation` | no | criterion change | exhaustive sweep over the input space | no — the permutation check is independent of the walk it validates | **yes** — refuses an empty step set as loudly as a collision | **Clean, and the sweep genuinely rejected the author's first rule** |
| `audit` | seven checks, but derived from §12 rather than from findings | criterion change | computation | no — Euler vs. flood fill share nothing | red: yes; **empty: partially — N7 for one conjunct**; **panics rather than reports on one shape — N5** | **Sound; two disciplines missing** |
| `is_the_assembly_of_its_own_labelling` | no | — | computation | **yes on the arms population — it re-runs the constructor it judges (Q4 failure)** | **no — cannot be false on this population, N7** | **Meaningful only inside the mutation walk** |
| `Parts::perturbations` / mutation walk | **yes — the per-field loops are hand-written literals (N4)** | **append a line** | compiler at field level, author at slot level | no | red: yes; empty: yes (`slots_perturbed > 0`); **no-op: yes** — this is F-0059's own rule, correctly executed | **Strictly better than a `vec![corrupt_a, …]` and still a literal one level down** |
| `dcel::transaction::apply` | no | criterion change | the signature — no parameter can carry a score | n/a | yes | **Clean. The §32 rule 14 property is genuinely held by the type** |
| `transaction_for` (the harness's population) | **yes — a four-arm `match` with `_ => return None` (N2)** | **the fifth kind is dropped silently** | the outcome, i.e. the author | **yes — kind is derived from the comparison `apply` redoes** | **no — the excluded count is not published** | **F-0058, second instance** |
| Gate rows / `DcelGateConfig` / `Threshold::from_gates` | no — the mint takes a file and a key | criterion change | the type (no arithmetic impls) + the file | no — file and code must agree in both directions | yes | **Clean; the RT45-A10 lesson is well applied** |
| Knockouts (`ProxyKnockout`, `RoiKnockout`) | two named knockouts, but each is a behaviour in the tree | a third clause needs a third knockout | the run | no | **knockout 1: yes. Knockout 2: red only — N9** | **The right shape; one guard missing** |
| `structural_fixtures` (condition 51) | five fixtures, declared with their classes per arm | a sixth is a line | the assertion of the declared class | no — the class is asserted, not observed | yes — classes asserted, convention-dependence counted | **Clean, and F-0045's call-not-copy lesson correctly applied** |
| `curve_replacement_isotopy` / `incidence_signature` | no | — | constructor panics on empty `missing` | — | refusal: yes. **The "evaluated" condition: N6 — it measures nothing** | **The refusal is honest; the executed half is not what it is named** |
| `every_status_document_is_classified_or_excepted_with_a_reason` | **exceptions only, each with reason and owner** | **a red test** | the file system — a side the offending commit does not edit | no | yes — walk non-emptiness and stale-exception both asserted | **The right closure shape. Its own limits are stated, and N1/N8 are what falls outside them** |

**The pattern across the table.** Where the judge is the compiler, the type, or an exhaustive sweep over the input space, the mechanisms are strong and I could not break them. Where the judge is a hand-written enumeration one level below a derived one (`perturbations`), or the outcome itself (`transaction_for`), or positional formatting (`gate_table`), the milestone repeats the class it documents. **Three of the eight instances of F-0048's shape in this milestone are new, and two of those three (N2, N4) are inside mechanisms built during M5 to close that very class.**

---

## §6. What I could not verify

- **CI execution.** I have no runner. I read `.github/workflows/ci.yml` and report N3 as a property of the *file*. **F-2 and M4-N9 remain unverified by execution**, exactly as the author states. I did not attempt to simulate the workflow.
- **Cross-platform / Tier B (A7.1).** My platform is `windows-x86_64`, the same `(os, arch)` the artifact records, so `dcel-check` without `--structural` succeeded. That says nothing about ubuntu, and I make no claim about it. A7.1 remains open, owner M12.
- **`dcel-check --structural` on a foreign platform.** Not reachable from here.
- **Donor sources.** Not opened (D-3).
- **The correctness of `assemble` itself against an external oracle.** The audit's documented blind spot — a systematically wrong `assemble` that agrees with itself — is real. My evidence against it is indirect: agreement with `topology::independent` on 474 arms, the intrinsic Euler identity, and the exhaustive 4×4 sweep. I did not construct a third independent arrangement builder.
- **M0–M4.5 artifacts** beyond what `cargo test --workspace` and the M5 gate exercise. Out of scope for this review.
- **Whether the 307 excluded transaction arms would have passed.** N2 identifies the exclusion; I did not build the widened harness that would measure them, because that is a code change and I do not fix code.

---

## §7. Conditions

Numbering continues from REVIEW_M4_5, which ends at 51.

**Blocking for M6:**

52. **N1.** Correct the swapped arguments at `report.rs:429–430`, and bring the M5 gate rows under the positional binding whose price limitation 36 already names. N1 is the demonstration that the cost of leaving it open is not hypothetical. *Minimum acceptable for M5: the clause-4 row must not state that 8 arms failed a conjunct the same row requires to be zero.*
53. **N8.** Correct all eight sites. Site (b) is in a §27.7-governed frozen file and must be corrected in a gate-file-only commit. Site (c) is the headline number of the F-9 exhaustive axis in three documents. Site (d) makes two documents disagree about the price of the no-new-crate decision. **And record the class rule**: a commit that re-records an artifact owes a diff of every quantity it moved, not the ones it remembers — C243 corrected two of four.
54. **N3.** Either add the missing CI steps — `gt-corpus -- dcel --scope full`, and an `--ignored` step covering `dcel_harness` and `dcel_props` — **or** delete the four claims that say they already run there. A mechanism a document says runs in CI, and which does not, is F-7's own condition inside the milestone that reported F-7 closed for its mechanisms.
55. **N2.** Publish the count of arms excluded by `transaction_for` beside the clause-3 row; state which of `apply`'s refusal reasons are reachable in the production run; and either widen the declared kinds or record the exclusion as a numbered limitation with an owner. **Delete or correct the comment at `mod.rs:465` that asserts the opposite of what the code does.**

**Required, non-blocking, closable in M6:**

56. **N7.** Either drop `arms_that_are_not_their_own_assembly == 0` from clause 4's conjunction, or relabel it in the row as the repeat-determinism check it is on this population.
57. **N6.** Publish the degree multiset, or the count of vertices of degree > 2, rather than the sum — or withdraw the claim that §12's first isotopy condition is evaluated. Fix the junction detection at `dcel_props.rs:132–135`, which currently tests `|B| > |V|`.
58. **N4.** Generate a perturbation for every scalar slot including `path[j].1` — or narrow the claim in `walk.rs`, `audit.rs` and ADR-0031 to what the walk actually does.
59. **N5.** Add the missing range guard at `audit.rs:344` and audit the class, not the site. Re-state the residual of the one-constructor claim at its real price: `with_parts` plus `pub(crate)` `Parts` fields, not "one new `pub fn`".
60. **N9, N10, N11.** Add the non-emptiness assertion to knockout 2; correct the four traceability gate-column references; declare the saddle-axis narrowing as a numbered limitation.

**Standing, carried forward unchanged:** A7.1 (M12), M4-N8 (M6), F-1/F-3/F-5 (M7), F-4 and obligation 18 (M8), F-6/F-7-for-M4.5-attacks/F-8-for-the-M4.5-judge and limitation 32 (M6). Their prices as stated in STATUS_M5 §4 are honest and I do not contest them.

---

## §8. Verdict

**VERDICT: ACCEPT WITH CONDITIONS**

*Reasoning offered so the governor can weigh it against reviewer B and the red team, rather than take the line alone:* all four §28 M5 clauses reproduce MET from a clean clone, the artifact is byte-identical, 530/530 passes in both profiles, clippy is clean on a cold target directory, §27.7 is kept, condition 51's mathematics is correct and I verified it rather than accepted it, and no §36 stop condition fires. The core engineering — invariants held by the representation, the unconditional saddle pairing validated by an exhaustive sweep that rejected the author's first rule, transactions whose acceptance signature cannot receive a score — is genuinely strong, and the F-0058 self-report is the most creditable thing in the milestone. **Every finding above is a defect in evidence or in discipline, not in the measurement.** If the governor's standard is that a false "runs in CI" claim about a gate mechanism (N3) is itself blocking, or that a gate row contradicting its own conjunct (N1) disqualifies the row, then N3 or N1 alone converts this to REJECT; I judged them conditions rather than blockers because neither changes a clause verdict and both are closable without touching the DCEL.

---

## §9. Hygiene, at the end

```
=== MAIN REPO (final) ===
$ git status --porcelain          # (empty — clean)
$ git rev-parse HEAD
496a256c7b723912f3f4642d5f0e9c768b892138

=== MEASURING CLONE (final) ===
$ git status --porcelain          # (empty — clean)
$ git rev-parse HEAD
496a256c7b723912f3f4642d5f0e9c768b892138

=== EXPERIMENT CLONE (deliberately dirty; no number from it is a production measurement) ===
 M crates/vice-bench/tests/dcel_harness.rs
 M crates/vice-topology/src/dcel/walk.rs
496a256c7b723912f3f4642d5f0e9c768b892138

=== worktrees ===
C:/Users/nirrt/Toolset/vice-classic  496a256 [main]
```

Main tree unchanged and clean, at the same HEAD as at the start. The measuring clone was clean before and after every run whose number appears in §1 and §2. The experiment clone carries only my two probe files, and every number taken from it is labelled as mine in §4. No `git worktree` was created and `git worktree prune` was never run.

---

**Reviewer's note on the report itself, per §34.** I reproduced the milestone's documented commands, the gate artifacts and the negative tests, and I reproduced more than one adversarial case: I compiled an invalid `Dcel` from inside the crate, I measured the mutation walk against the structure it claims to exhaust, I measured the knockout's own population, and I re-derived condition 51's parity argument independently. The single most useful thing a second reviewer or the red team could do that I did not: **build a third, independent arrangement constructor and diff it against `assemble` at corpus size.** That is the one blind spot the audit documents and nothing in this milestone closes.

---

# REVIEW_M5_A — addendum 1 (delta-1)

Reviewer A, independent cold review, Opus 5. The signed §0–§9 above is untouched; this is an append.
Object: **`d042bba`**, commits **C249–C253** on top of `2216959`.

## §A0. Hygiene

```
start:  main repo  git status --porcelain → (empty)
        git rev-parse HEAD → d042bba0d53ff5d90836868726dc0aad7b3ccbc0
```

Clone `…/scratchpad/m5a-delta1-rev-qz83k` (unique by construction, condition 25), `CARGO_TARGET_DIR=…/tgt-m5a-delta1-qz83k` asserted **not to exist** before the first command. No `git worktree`; `git worktree list` shows one entry, the main tree. Every cargo call was blocking. A second clone `…-exp-qz83k` carries my probes and is deliberately dirty; no number from it is offered as a production measurement.

```
end:    measuring clone  git status --porcelain → (empty)   HEAD = d042bba
        experiment clone  M dcel/mod.rs, M dcel/walk.rs     HEAD = d042bba
        main repo         M docs/REDTEAM_M5.md              HEAD = d042bba
```

**The main-tree modification is not mine.** It is a 173-line append to `docs/REDTEAM_M5.md` (mtime 22:18, after my session began) — the red team writing its own delta-1 addendum concurrently. I diagnosed it far enough to confirm authorship and stopped at the header rather than read a parallel cold context's findings. My measurements are unaffected: all of them come from my clone, verified clean before and after each reported run.

## §A1. What I reproduced

| command | result |
|---|---|
| `cargo fmt --all --check` | `FMT_EXIT=0`, no output |
| `cargo clippy --workspace --all-targets -- -D warnings`, **cold target dir** | `CLIPPY_EXIT=0`; all nine crates compiled from scratch, zero diagnostics |
| `cargo test --locked --workspace` | `DEBUG_EXIT=0` — **534 passed, 0 failed, 12 ignored** |
| `cargo test --locked --release --workspace` | `RELEASE_EXIT=0` — **534 passed, 0 failed, 12 ignored** |
| `gt-corpus dcel --scope full` | `GATE_EXIT=0`, **four `[MET]`**; 474 arms, 237 groups, 247 in / 247 out, 10 convention-dependent (**7 corpus / 3 register, now computed**), 167/167 committed, `155160 slots, caught by audit 155160, UNCAUGHT 0, no-ops 0` |
| artifact | `7406BF31…CEB11B05` = `7406BF31…CEB11B05` — **byte-identical** |
| `dcel_props -- --ignored` | 2 passed; `4x4 exhaustive: 131072 arrangements, 12 classes, 41678 with a critical 2x2` |
| `dcel_harness -- --ignored` | **5 passed**, 0 failed (380 s), including the new `every_gate_clause_has_a_knockout_that_reddens_it` |
| `--workspace -- --ignored` | **exceeded my 10-minute cap; not verified end-to-end.** The `-101` cause is fixed by inspection (`legal.rs` ```` ```ignore ```` → ```` ```text ````) and both M5 targets pass individually. I do not certify the aggregate exit code. |

## §A2. RT5-A2 — genuinely closed. The control the governor asked for, run.

The question was whether `caught_by_audit == slots` became an identity from the other side. **It did not.** I disabled exactly one check — the third construction — in an experiment clone, changing nothing else, and re-measured the walk per field on a 33×33 ring:

```
BASELINE (crossing check enabled)          E1 (crossing check DISABLED)
slots 1400  caught 1400  UNCAUGHT 0        slots 1400  caught 175  UNCAUGHT 1225
  face_of_padded_px  1225 / 1225 / 0         face_of_padded_px  1225 / 0 / 1225
  boundaries[].path   146 /  146 / 0         boundaries[].path   146 / 146 / 0
  site                 12 /   12 / 0         site                 12 /  12 / 0
  faces[].loops         4 /    4 / 0         (every other field unchanged)
  vertices              4 /    4 / 0
```

`uncaught_by_audit` moves 0 → **1225**, exactly the `face_of_padded_px` slots, exactly the field RT5-A1 targeted, with every other field still fully caught. The predicate is live, attributable, and falsifiable. **The 3.6 % → 100 % jump is the first kind — the audit began reading the structure — not the second.** Both identities are gone from `ResolvingPower`, the clause now stands on the complement, and `gate_min_slots_perturbed = 40000` closes the "a walk that visited one slot satisfied it" hole. This is the strongest work in the delta and it holds.

## §A3. Findings

### M5A-D1-N1 — **BLOCKER.** The third construction is not independent of `flood_faces`, and RT5-A1's class survives one step earlier in the same function.

**The structural fact.** `crossing.rs`'s independence table claims `face_map_from_boundaries` "never looks at" `face_of_padded_px`. That is literally true of the function and materially false of the check, because `Boundary::owners` is *sampled out of* `face_of_padded_px` inside `assemble`:

```rust
let (left_px, right_px) = arr.flanks(ch[0]);
let (lf, rf) = (face_at(&face_of_padded_px, &arr, left_px),
                face_at(&face_of_padded_px, &arr, right_px));
```

So the "third construction" is **downstream of the field it certifies**. Its only genuinely external anchor is one bit: *the padding ring is face 0*.

**E2 — the red team's edit moved before the owners are sampled.** I took RT5-A1's ten lines verbatim and moved them from "just before `Dcel { … }`" to "just after `flood_faces`". **Caught** — 4 tests fail, including `the_boundary_walk_rebuilds_the_face_map`. My hypothesis that it would survive was wrong, and I am reporting the experiment rather than the hypothesis: the exterior anchor fires because a global rotation moves the exterior off id 0.

**E2b — the same corruption, made to respect that one anchor.** A permutation that *fixes* the exterior and swaps ids 1↔2, applied inside `assemble` before the owners are sampled. Nothing else changed:

```
cargo test --locked --release --workspace   →  536 passed, 0 failed
gt-corpus dcel --scope full                 →  four [MET], EXIT 0
artifact SHA-256                            →  7406BF31…CEB11B05  BYTE-IDENTICAL
audit(&d)                                   →  None   (no violation)
face_map_agrees(&d)                         →  true
pixels whose faces[face_of_pixel(p)].label
  contradicts the labelling                 →  529 of 1089   (48.6 %)
```

It is a real defect, not a relabelling: the public `Dcel::face_of_pixel` reports a face whose `label` disagrees with the labelling on nearly half the canvas, and every instrument M5 owns says the arrangement is sound.

**The root cause, stated once.** After delta-1 the audit connects the map to the owners and the owners to the map. It still **never connects either to the labelling.** The `signature` comparison compares *counts of faces*, not per-pixel attribution; `face_of_pixel` is read by no predicate. The loop is closed and its only external anchor is the identity of face 0.

**This refutes the author's own F-0048 self-assessment for `audit()`,** which says the row "PASSES ONLY BECAUSE IT IS BOUNDED — a field with no predicate now shows up as an uncaught slot and fails clause 4." The bound is over *perturbations of a correct `Parts`*. A defect inside `assemble` produces a self-consistent wrong `Parts` that is not a perturbation of anything and never enters the walk. **The bound does not cover the class RT5-A1 belongs to** — E2b is the demonstration, and it is why 155 160/155 160 and a byte-identical artifact coexist with a half-wrong face map.

**General rule of the class.** A cross-check is independent only up to the data it shares with what it checks. Deriving construction B from an intermediate that construction A produced makes B a paraphrase of A however different B's algorithm looks — F-0048's own lesson, applied to the *provenance of the inputs* rather than to the shape of the code. The test is not "does B look different" but **"what is the largest corruption of A that B reproduces"**; here that set is *every permutation of face ids fixing the exterior*.

**The remedy is one line and needs no third construction.** The ground truth is already in the structure and unread:

```
for every pixel p:  faces[face_of_pixel(p)].label == labelling.inside()[p]
```

That catches RT5-A1, E2 and E2b alike, and it anchors the map to the labelling instead of to itself.

### M5A-D1-N2 — MAJOR. `path[j].1` is still never perturbed, is not fixed, and is not declared.

`walk.rs:218-221` is unchanged: `pt.0 = pt.0.wrapping_add(1);`, and `git diff 2216959..d042bba -- walk.rs` adds no `.1` perturbation. Measured on a 33×33 ring: `perturbations() = 1400`; with both path coordinates it would be 1546; the true scalar-slot count of `Parts` is 1548. **146 perturbations for 146 path points — one coordinate per point.** I found no record of it in `STATUS_M5.md`, `REPRODUCIBILITY_M5.md` or `FAILURE_LEDGER.md`.

The governor's summary says the author found this himself. The tree at `d042bba` shows it neither fixed nor declared. It now carries more weight than in my signed §4: clause 4's conjuncts are `uncaught_by_audit == 0` and `gate_min_slots_perturbed = 40000`, both computed over a slot set that omits ~9.6 % of the structure's scalar slots — and the claim "one perturbation per scalar slot" is asserted in `walk.rs`, `audit.rs` and ADR-0031 §3.

### M5A-D1-N3 — MINOR. One of the eight N8 sites is uncorrected.

`crates/vice-bench/src/dcel/mod.rs:423` still reads "every twenty-fifth" against `RESOLVING_POWER_STRIDE = 17` at `:426`. It is in neither errata table.

### M5A-D1-N4 — OBSERVATION. The number corrections are errata, not repairs.

`STATUS_M5.md:101` still reads "11 classes", `:124` and `:249` still read "56", ADR-0031 `:85` still reads "131 070 / 11" and `:95`/`:100` still read "four modules"/"four files". The corrections live in appended errata tables (STATUS `:387-388`, ADR `:146-153`). Defensible under this project's append-only convention for reviewed documents, and every correction I checked is present and correct — but the number a reader meets first is still the wrong one, and none of them is derived. That is limitation 36, owner M6, unchanged.

## §A4. Limitation 37, judged as a claim

The author's own F-0048 table says `transaction_for` **"DOES NOT PASS. Mitigated, not closed"** — 307 of 474 excluded, published, owner M6. I agree with the classification and with the honesty of stating it in his own audit rather than in a footnote.

On the **price** — "a harness that attempts every arm and classifies the outcome, plus whatever `apply` needs to accept a multi-step signature delta" — I looked for the hidden cost this project usually has and did not find it: `EditKind`'s variant names are **not** carried in `docs/gt/TOPOLOGY_M4_5.json` (checked: zero occurrences of `bridge_close`/`gap_open`/`hole_open`/`hole_fill`), so widening the enum does not touch a signed artifact and does not drag §27.1 in. **The price is honest and the deferral to M6 is legitimate.** §28 M5 does name *compound* transactions and the excluded subclass is exactly those, so it must not be deferred twice.

## §A5. Conditions 52–60, verified

| | condition | verdict |
|---|---|---|
| 52 | N1 swapped `format!` args | **CLOSED** — the row now reads "0 … and 0 were not the assembly … A further 8 arm(s) carry a valid but EMPTY arrangement". F-0064 records the class |
| 53 | N8, eight sites | **CLOSED except one** — see D1-N3; gate-file provenance corrected in C249, which touched `configs/GATES_V1.toml` and nothing else (§27.7 kept) |
| 54 | N3 CI | **CLOSED** — four M5 steps added, plus `every_ignore_that_claims_ci_is_named_by_a_workflow_step`, which derives the claim from `ci.yml` rather than trusting it. F-7's closure correctly declared invalid and reissued |
| 55 | N2 transaction population | **MITIGATED, declared** — limitation 37, see §A4 |
| 56 | N7 tautological conjunct | **CLOSED** — `arms_that_are_not_their_own_assembly` removed from `faces_row` |
| 57 | N6 incidence | **CLOSED** — `degree_multiset` / `junction_count` replace `incidence_signature` |
| 58 | N4 path slots | **NOT CLOSED** — see D1-N2 |
| 59 | N5 audit panic / residual price | **CLOSED** — `d.faces().first()` guards both sites; `field: _` bypass named in `walk.rs` where the strength is claimed |
| 60 | N9 / N10 / N11 | **CLOSED** — `knocked.transactions_attempted > 0` at `dcel_harness.rs:116`; traceability M5-12→T9, M5-13→T10 |

## §A6. F-0048 applied to my own method, before I sign

**Q1 — is there a literal enumerating my subjects?** Yes, twice. My delta-1 plan was the governor's three-item list, so the answer to Q2 was "the governor names a fourth" — the same shape I charged the author with. And my per-field classifier in `rev_a_resolving_power_by_field` is eight hand-written buckets with a catch-all; a field I failed to name would have been silently counted as `site`. **The only thing that made it safe is that I checked the buckets sum to `slots` (1400 = 1400) — and I checked it because the sum is a judge and the enumeration is not.**

**Q3 — who was my judge?** For E1, E2 and E2b: the compiler, the test suite and the gate binary. Not my reading of the code. My reading produced a hypothesis (a global rotation before the owners would survive) and the compiler **refuted it** — E2 was caught. I published the refutation rather than quietly replacing it with the version that worked, because the pair E2/E2b is what turns an accusation into a boundary statement: the check has exactly one bit of external anchoring, and I can say which bit.

**Q4 — did my guard share a key with the mechanism?** For E2b, no: the verdict came from a per-pixel comparison against the *labelling*, which is precisely the input no M5 predicate reads. Had I judged E2b with the milestone's own instruments I would have concluded it was correct.

**Q5 — both directions?** E2 red, E2b green, E1 red-on-removal, baseline green. That is what I have; it is also the reason I can state the residual class exactly rather than gesturing at it.

## §A7. What I could not verify

CI execution (the governor's). `cargo test --workspace -- --ignored` end-to-end — exceeded my 10-minute blocking cap; the two M5 targets pass individually and the `-101` cause is fixed by inspection, but I do not certify the aggregate exit code. Cross-platform / A7.1. Donor sources (D-3). Whether the *shipped* `flood_faces` is correct per-pixel: after delta-1 that still rests on inspection plus two assertions on one 9×9 disk, which is the content of D1-N1 rather than a gap in my method.

## §A8. Verdict

**VERDICT (addendum 1): REJECT — one blocker (M5A-D1-N1), one major (M5A-D1-N2).**

Said plainly, because the delta deserves it: RT5-A2 is genuinely and verifiably closed, RT5-A3/N3 is closed with a mechanism that derives the claim instead of asserting it, seven of my nine conditions are closed, and the author's own F-0048 table names two rows that do not pass rather than reporting nine green. This is the strongest delta the project has produced. The blocker is narrow and its remedy is one line — but it is the same class as the blocker it was written to close, at a placement one step earlier in the same function, and it passes with a byte-identical artifact while half the canvas reports the wrong ink.

**GATE §28 M5: NOT MET**

---

# REVIEW_M5_A — addendum 2 (delta-2)

Reviewer A, independent cold review, Opus 5. Signed §0–§9 and addendum 1 untouched; this is an append.
Object: **`2ec7c95`**, commits **C255–C258** on top of `dbe0dea`.

## §B0. Hygiene

```
start:  main repo  git status --porcelain → (empty)   HEAD = 2ec7c9546a10fefdeaac1b314a69d6925d13a094
end:    main repo  (empty)                            HEAD = 2ec7c95
        measuring clone …/m5a-delta2-rev-vt41m  (empty)  HEAD = 2ec7c95
        experiment clone …/m5a-delta2-exp-vt41m  M audit.rs, M walk.rs  (deliberately dirty)
        git worktree list → one entry, the main tree
```

`CARGO_TARGET_DIR=…/tgt-m5a-delta2-vt41m` asserted **not to exist** before the first command. Every cargo call blocking. No `git worktree`. No number below comes from the experiment clone except where labelled as mine.

*On C254: recorded, and the correction is yours to make, not mine to absorb. It cost nothing — I stopped at the header and every measurement in addendum 1 came from my own clone.*

## §B1. What I reproduced

| command | result |
|---|---|
| `cargo fmt --all --check` | `FMT_EXIT=0` |
| `cargo clippy --workspace --all-targets -- -D warnings`, **cold target** | `CLIPPY_EXIT=0`, zero diagnostics |
| `cargo test --locked --workspace` | `DEBUG_EXIT=0` — **537 passed, 0 failed, 13 ignored** |
| `cargo test --locked --release --workspace` | `RELEASE_EXIT=0` — **537 passed, 0 failed, 13 ignored** |
| `gt-corpus dcel --scope full` | `GATE_EXIT=0`, **four `[MET]`**; 29 probes, **161 391 slots, caught 161 391, UNCAUGHT 0, no-ops 0** |
| artifact | `26A08BF0…99A19349` = committed — **byte-identical** |
| `dcel_props -- --ignored` | 2 passed; `131072 arrangements, 12 classes, 41678 with a critical 2x2` |
| `dcel_harness -- --ignored` | **6 passed**, including `the_relabelling_that_survived_delta_one_now_reddens_clause_four` |
| §27.7 | C256 → `configs/GATES_V1.toml` only; C257 → `docs/gt/DCEL_M5.json` only. Kept, and separated as you said |

## §B2. My delta-1 findings, verified closed

- **D1-N1 (blocker) — CLOSED, by exactly the remedy I named.** The per-pixel anchor is in `audit.rs` at `(A)`, placed **before all branches**, which also closes reviewer B's empty-subclass N11(a) in the same movement. The E2b knockout is in the tree, runs in the default path, and — measured — the whole suite goes `534 passed, 1 failed` when I disable the anchor, the failure being `a_relabelling_that_keeps_every_count_defeats_the_rebuild_and_not_the_anchor`. The author's refuted first reproduction mirrors mine; F-0066 generalises it correctly.
- **D1-N2 — CLOSED.** `boundaries.path` now yields **292 slots on a 33×33 ring = 2 × 146**, and the walk's total is **1548**, which is *exactly* the scalar-leaf count I computed by hand in addendum 1 §A3. Workspace figure 155 160 → 161 391. F-0067's rule (each batch replacement asserts its own anchor; the edit list is built from what actually changed) is the right shape, and `assert old in d` catching three mismatched anchors inside this same delta is the kind of evidence that makes a rule credible.
- **D1-N3 — CLOSED.** `mod.rs:455` now records the correction; stride 17 with `first_of_branch ||`.
- **D1-N4 — CLOSED** as errata, with the ADR erratum added.

## §B3. Your two questions, answered with numbers

### B3.1 — What carries the load

Attribution on a 33×33 ring, 1548 slots, one check disabled at a time, nothing else changed:

| configuration | UNCAUGHT | carried uniquely |
|---|---|---|
| shipped (anchor + crossing) | **0** | — |
| **anchor OFF**, crossing on | **0** | the anchor carries **0 slots of the walk** |
| **crossing OFF**, anchor on | **205** | crossing carries **205** uniquely, all in `face_of_padded_px` |
| *(delta-1: crossing off, no anchor)* | *1225* | |

**They carry disjoint loads, and both are real.** `crossing` is not redundant: it uniquely catches **205 of 1548 — 13.2 % of the walk, 16.7 % of the map family**. Those are precisely the slots the anchor cannot see, and the reason is structural: the anchor iterates **canvas pixels only** and compares **labels only**, so a reassignment to a different face with the *same* label, and every entry in the **padding ring**, are outside it. That is the number you asked for, and it means the clause-4 row may keep citing both.

**The converse is the finding.** The anchor contributes **zero to every number the gate publishes**: with it disabled, `slots_perturbed`, `caught_by_audit`, `by_family` and the artifact are unchanged. The check that closed the blocker is **invisible to the instrument clause 4 cites** as "what makes those zeros evidence rather than silence".

It is *not* unguarded — the default-path test above fails on its removal. The precise statement, which belongs in the row: **the anchor is guarded by a test, not measured by the instrument.** `by_family` decomposes the walk, and the walk is by construction made of perturbations of a *correct* value; the anchor's whole domain is defects *inside* `assemble`, which are not perturbations of anything. That is F-0066, and F-0066 implies the walk can never attribute a slot to the anchor.

### B3.2 — The anchor's residual class, stated exactly

The anchor binds, per pixel, `faces[face_of_pixel(p)].label == labelling[p]` — it ties the **labelled partition** to the input. It does **not** tie:

**(a) which face id** — only its label. Two faces carrying the *same* label may exchange ids consistently and the anchor is silent by construction.

**(b) the loop structure and traversal order — verified, E6.** Swap two half-edges inside one face loop and keep `site` the exact inverse, so every internal-consistency check still holds. On a 5×5 labelling (`bits=69`, fg-4, loop of length 4):

```
audit(broken)               → None            (passes)
face_map_agrees(broken)     → true
parts changed?              → true
half-edges whose next(h) does NOT begin where h ends  → 2
```

The labelling, the map, the owners, the paths, the vertices and the loop *count* are untouched, so the anchor is untouched by construction. The constraints that remain on the loop lists are: each boundary used exactly twice, `site` the exact inverse, `next` staying inside the face, and `loop_count()` entering Euler. **None ties the order to the geometry.** Audit step (1) recomputes `successor_is_a_permutation` from the labelling, but only verifies that `succ` *is* a permutation — it never compares the stored loops against the orbits of that permutation. The geometry→loops link is computed once in `assemble` and never re-derived.

**Answering your framing directly: this is not "we moved the single external bit."** Delta-2 added a genuinely *second* external anchor, and it is the right one — the labelling is the only input the audit does not derive. But it anchors the **face map**. The **loop structure** still has no external anchor at all: it has internal consistency and counts. The arrangement now has one anchored half and one unanchored half.

The two functions that would close it already exist in the API and are called by **nothing** in the workspace: `Dcel::origin` (`mod.rs:438`) and `Dcel::target` (`:448`).

## §B4. New finding

### M5A-D2-N1 — MAJOR. §12's "face cycles closed **and oriented**" is half-held, and the §12 table claims the whole.

The milestone's central table (`dcel/mod.rs`, STATUS §1, ADR-0031 §1) says of this invariant: *"a loop is a `Vec<HalfEdgeId>` traversed modulo its length — no: a cyclic traversal has no open state."* That argument establishes **closed**. It does not establish **oriented** — that the cycle *is* the boundary walk of its face — and E6 exhibits a value where it is false, which `audit`, `crossing`, the anchor, the walk and all 537 tests accept.

So one of the six invariants claimed as *unrepresentably-false* is representable-false in half of what §12 asks. The remedy is one line and, by symmetry with the anchor, uses what exists:

```
for every half-edge h:   target(h) == origin(next(h))
```

or, matching the anchor's shape exactly, re-derive `orbits(&arr)` from the labelling and compare against the stored loops.

**Severity, and why it is MAJOR and not the blocker E2b was.** E2b corrupted a *public output* — `face_of_pixel` — on 48.6 % of the canvas with the gate green and the artifact byte-identical. E6 corrupts a *traversal* that no M5 consumer reads: `next` is used inside the audit's own check and nowhere else in the workspace, so no gate number and no artifact byte moves, and the shipped `assemble` builds the loops from `orbits(&arr)` and is correct. What is deficient is resolving power against one class, not any current output. It becomes load-bearing the moment M6 walks those chains for span candidates (§14.2), which is why it is a condition owed before M6 rather than a stop now.

**General rule of the class.** It is the same rule as D1-N1, one level over: *a structure is anchored only in the parts a check ties to an input the structure does not derive.* Delta-2 anchored the partition. The incidence structure — which half-edge follows which — is still certified only against itself. **The question "what is the largest corruption this reproduces" must be asked once per half of the value, not once per mechanism.**

## §B5. Conditions 52–60, by halves, each with its own status (condition 38)

| | half | status |
|---|---|---|
| **52** | (a) swapped `format!` args corrected | **CLOSED** (delta-1, verified) |
| | (b) M5 gate rows bound positionally to artifact keys | **OPEN**, limitation 36, owner M6 — unchanged and correctly priced |
| **53** | (a) the eight stale numbers | **CLOSED** — last one (stride comment) closed in delta-2 |
| | (b) numbers *derived* rather than errata'd | **OPEN**, same owner as 52(b) |
| **54** | (a) four M5 CI steps present | **CLOSED** |
| | (b) the claim derived from `ci.yml` | **CLOSED** — `every_ignore_that_claims_ci_is_named_by_a_workflow_step` |
| | (c) CI observed to pass | **OPEN — yours** |
| **55** | (a) excluded count published | **CLOSED** |
| | (b) compound transactions attempted | **OPEN**, limitation 37, owner M6; price re-checked in addendum 1 §A4 and still honest |
| **56** | tautological conjunct removed | **CLOSED** |
| **57** | (a) `degree_multiset` / `junction_count` replace the sum | **CLOSED** |
| | (b) junction detection no longer `\|B\| > \|V\|` | **CLOSED** |
| **58** | (a) `path[j].1` perturbed | **CLOSED** — 292 slots, total 1548 = scalar leaves |
| | (b) site count judged from outside the walk | **CLOSED** — `every_scalar_leaf_of_parts_has_exactly_one_perturbation` compares against the `Serialize` derive, which `extra: _` does not move. This is the right judge |
| **59** | (a) audit range guards | **CLOSED** |
| | (b) residual priced at `with_parts`, not at a new `pub fn` | **CLOSED** |
| **60** | (a) knockout non-emptiness | **CLOSED** |
| | (b) traceability gate column | **CLOSED** |
| | (c) saddle-axis narrowing declared | **CLOSED** |
| **61** *(new)* | (a) §12 table corrected: "closed" is held, "oriented" is not | **OPEN** |
| | (b) the loop order anchored to the labelling | **OPEN**, owed **before M6 walks chains** |
| | (c) clause-4 row states that the anchor is guarded-not-measured, and keeps citing crossing for its 205 | **OPEN** |

## §B6. F-0048 on my own method, before I sign

**Q1 — literal enumerating my subjects?** Yes, and worse than last time: my plan was *your* two questions, so the answer to Q2 was again "the governor names a third". What I did against it: E6 was not on your list as an experiment — you asked for the residual class and I had to *find* the half that was unanchored, which meant enumerating the value's parts (map / loops / twins / paths) and asking Q1 of each. That enumeration is mine and hand-written, and if the structure had a fifth part I did not name, I would have missed it. **The check that made it safe: `SLOT_FAMILIES` has eight entries and my four-way split had to account for all 1548 slots — it does (4+4+4+292+3+4+1225+12).** A sum is a judge; my list is not.

**Q3 — who was my judge?** The compiler and the test binary for E5a/E5b/E6. My *reading* of `crossing.rs` produced the delta-1 hypothesis that was refuted; my reading this time produced the E6 hypothesis that held. I do not get to count the second as better method than the first — same method, different luck — which is why both are published as runs.

**Q4 — did my guard share a key with the mechanism?** E5a/E5b are differential against the milestone's own instrument, so they share everything; that is fine because they measure *attribution*, not correctness. E6's verdict came from `target`/`origin`, which the milestone computes and never reads — an input to my judgement that no M5 check uses.

**Q5 — both directions?** E5a red-on-nothing (0), E5b red-on-205, anchor-off suite red (1 failed), E6 green-when-it-should-be-red, baseline green. Four directions, and the pair E5a/E5b is what turns "does crossing still matter" from an opinion into 205.

**Where my method was weakest.** I asserted in addendum 1 §A3 that the remedy "catches RT5-A1, E2 and E2b alike". I did not verify the *padding ring* case then, and E5b now shows the anchor does not cover it — crossing does. My one-line remedy was correct for the class I named and incomplete as a replacement, and the author was right to keep both checks rather than swap one for the other.

## §B7. What I could not verify

CI execution (yours). `cargo test --workspace -- --ignored` end-to-end — exceeded my blocking cap again; both M5 targets and `dcel_props` pass individually. Cross-platform / A7.1. Donor sources (D-3). Whether any consumer beyond M5 reads `Dcel::next` — I checked the current workspace and found none, which is the basis for calling D2-N1 major rather than blocking, and it stops being true in M6.

## §B8. Verdict

**VERDICT (addendum 2): ACCEPT WITH CONDITIONS — one major (M5A-D2-N1), no blocker.**

My delta-1 blocker is closed by exactly the check I named, placed better than I specified (before all branches, closing the empty subclass too), with my E2b in the tree as a two-sided knockout that fails when the anchor is removed. D1-N2, N3 and N4 are closed, and 58(b) is closed by a judge — the `Serialize` derive — that sits outside the mechanism it judges, which is the first time in this milestone that a "the compiler is the judge" claim has been true at the level it was claimed. The author's three F-0048 rows declared non-passing are, as far as I can measure, the correct three.

The difference from delta-1 is material and I want it on the record rather than inferred: E2b was a real defect corrupting a public output on half the canvas with a byte-identical artifact; E6 is a defect nothing would catch in a traversal nothing currently reads. Same class, different consequence.

**GATE §28 M5: MET**

---

# REVIEW_M5_A — addendum 3 (delta-3)

Reviewer A, independent cold review, Opus 5. Signed §0–§9 and addenda 1–2 untouched.
Object: **`d8bc9bb`**, commits **C260–C262** on top of `b6a57d3`.

## §C0. Hygiene

```
start / end:  main repo  git status --porcelain → (empty)   HEAD = d8bc9bbffee9198f139a9c97632c9a3323ca1016
              measuring clone …/m5a-delta3-rev-hp62w  (empty)  HEAD = d8bc9bb
              experiment clone …/m5a-delta3-exp-hp62w  M walk.rs  (deliberately dirty)
              git worktree list → one entry, the main tree
```

`CARGO_TARGET_DIR=…/tgt-m5a-delta3-hp62w` asserted **not to exist** before the first command. Every cargo call blocking. No `git worktree`.

## §C1. Reproduced

| command | result |
|---|---|
| `cargo fmt --all --check` | `FMT_EXIT=0` |
| `cargo clippy --workspace --all-targets -- -D warnings`, **cold target** | `CLIPPY_EXIT=0`, zero diagnostics |
| `cargo test --locked --workspace` | `DEBUG_EXIT=0` — **543 passed, 0 failed, 13 ignored** |
| `cargo test --locked --release --workspace` | `RELEASE_EXIT=0` — **543 passed, 0 failed, 13 ignored** |
| `gt-corpus dcel --scope full` | `GATE_EXIT=0`, **four `[MET]`**; 480 arms (444 corpus, 36 register), 240 groups, 253 in/253 out, 13 convention-dependent (**7 corpus / 6 register**), 170/170 committed, 30 probes, **179 253 slots, UNCAUGHT 0, no-ops 0** |
| artifact | `8E008635…B2F8799F` — **byte-identical** |
| `dcel_props -- --ignored` | 2 passed |
| `dcel_harness -- --ignored` | **6 passed** |
| §27.7 | C261 → `docs/gt/DCEL_M5.json` only. Kept |

## §C2. My D2-N1, verified closed — and closed better than I specified

**61(a) — CLOSED.** The §12 claim is corrected in all three places; the argument now establishes **closed** and explicitly disclaims **oriented**.

**61(b) — CLOSED, and the author was right to reject my first option.** I named two variants. He applied Q4 to the first — `target(h) == origin(next(h))` reads `boundaries[].start/end` and `site`, both outputs of the same `assemble`, which is RT5-A9's shape a third time — and took the second. `loops.rs` re-derives the orbits from the **labelling** and compares stored loops as canonical cyclic lattice walks. Its test asserts that **both** delta-2 anchors reproduce my reordering and this one does not, which is my E6 turned into a two-sided control. That is the stronger check and the correct Q4 reasoning.

**61(c) — CLOSED.** The clause-4 row now says in the run's own output that the anchor is **guarded by knockouts rather than measured by the walk** — my formulation, verbatim — and re-publishes the disjointness on a 13×13 annulus as rebuild-only 153, anchor-only 3, both 160. The price of the second instrument is named with owner M6.

## §C3. The governor's asks, answered by execution

### C3.1 — Is there a `succ` corruption both sides share that survives the sweep and Euler? **No — verified.**

I took the one degree of freedom `succ` has and flipped it: at a critical 2×2 (`!al && ar`) take the **right** pairing instead of the left, inside `Arrangement::succ`, which both `assemble` and `loops_agree_with_the_labelling` call. Both sides therefore receive the same wrong loops.

```
E7: cargo test --locked --release --workspace  →  536 passed, 2 failed
  ---- dcel::lattice::tests::a_critical_vertex_has_degree_four_and_one_pairing_under_both_arms
  ---- dcel::sweep::tests::the_audit_is_green_over_a_whole_small_input_space
       panicked: exhaustive audit: "bits=18 conn=fg-Four:
                 half-edge 2 sits in face 1 but its owners put it on face 2"
```

The exhaustive sweep catches it, at **the same 4×3 labelling 18 under foreground-4** that F-0057 records, and it catches it through the owner/site check — a predicate that shares no algorithm with the loop comparison. **The author's residual statement is true by execution, not only by argument.**

### C3.2 — What the new check does **not** bind: the residual class, stated exactly

`loops_agree_with_the_labelling` reads `faces[].loops` and `boundaries[].path`, expands each loop into lattice steps, canonicalises the rotation, and compares as a **multiset** against `orbits()` re-derived from the labelling.

**It binds the ORDER of half-edges within each loop, as a lattice walk.** It does not bind:

**(a) the DECOMPOSITION of that walk into chains — verified, E8c.** `steps_of` *concatenates* chain paths, so any re-split producing the same lattice walk is invisible. I split one maximal chain in two at an interior degree-2 lattice point, keeping vertices, boundaries, loops and `site` all consistent:

```
REV-A E8c boundaries 2 -> 3     vertices 2 -> 3
REV-A E8c audit(broken)      -> None          (accepted)
REV-A E8c loops_agree        -> true
REV-A E8c face_map_agrees    -> true
```

`V` and `B` both rise by one, so `V − B + L = 2C` is preserved. **§12 asks for "maximal shared boundary chains" and the audit checks maximality nowhere.**

**(b) which FACE a loop belongs to** — the comparison is flat across all faces. (Bound elsewhere, by `face_of(h) == fi`.)

**(c) `site`, `owners`, `vertices`** — not read here; bound by other checks.

**(d) the rule generating the walk** — shared algorithm, covered by the sweep (C3.1).

So after delta-3: **map → labelling** (anchor), **loop order → labelling** (loops.rs), **succ rule → exhaustive sweep**, **chain decomposition → nothing.**

**Severity, honestly narrowed — and this is where I was wrong twice.** I predicted a spurious-vertex corruption inside `assemble` would pass everything. It did not. E8 (split point chosen by traversal order) → gate `[NOT MET]`, 2 transactions rolled back. E8b (split point a function of the lattice point alone, so edit-stable) → gate `[NOT MET]`, 8 rolled back. Both fell to **clause 3's** unrelated-chain comparison, which compares chains by path and therefore notices a changed decomposition. I had not modelled that as a maximality guard. Only the reduced claim survives: **the audit accepts non-maximal chains; the gate, on the corpus, does not.** That makes it a residual worth naming and not a hole worth blocking on.

### C3.3 — The fixture, checked as a claim

**What holds.** `diagonal_staircase` is registered; `the_register_carries_loops_long_enough_to_have_an_order` runs in the **default** path (no `#[ignore]`) and asserts `at_least_three > 0` for the staircase at 32/64/128 under both arms, plus `longest >= 3` per size. Register 30 → 36 arms; convention-dependent groups 10 → 13 with 6 now from the register, and `diagonal_staircase@s32/s64/s128` appear in the printed set — the counts are computed from the register as claimed. "Loops ≥3 by construction" is asserted, not hoped.

**What does not.** See D3-N1.

## §C4. New finding

### M5A-D3-N1 — MAJOR. The ORIENTED check has no population floor, and the numbers establishing that its population changed are in no artifact.

**No floor.** Clause 4's conjuncts are `arms_failing_the_audit == 0`, `min_arms`, `min_resolving_power_probes`, `min_slots_perturbed`, `uncaught_by_audit == 0`, `no_ops == 0`, `branches_seen.len() >= 2`, and every branch probed. **None requires a loop of length ≥ 3 to exist in the measured population.** A reordering of a 1- or 2-element cycle is the same cycle, so on a population of short loops the ORIENTED check is green for exactly the reason the *absent* check was green — which `loops.rs:204` states in its own words: *"a check whose population cannot exercise it is green for the reason the old one was."*

This is the position clause 1 was in before `gate_min_convention_dependent_groups` existed, and the milestone fixed that one with a floor. The newest check got a fixture instead. A fixture is asserted by a unit test; a floor is what makes the *gate row* false when the population thins. `report.rs`'s own header requires the second: *"every row that stands on a population also PUBLISHES that population's size, and the row is false when the population is empty."*

**Unmeasured.** `loop_length_profile` is consumed by exactly one site in the workspace — `dcel_props.rs:353`, a test. The figures that justify the whole fixture — *"the corpus averages 1.082 half-edges per loop, at most 55 of 1334 loops of length 3 or more, and the structural register had zero"* — appear in three code comments and ADR-0031 §218, **in no artifact key and under no mechanism**. I checked the artifact directly: no arm carries a longest-loop or loops-≥3 field, and no report-level key does either. That is limitation 36's class, occurring in the delta that closed the check those numbers exist to justify.

**Class rule.** *When a check is added because its population was empty, the population is part of the check.* Closing the gap with a fixture makes the check exercised **today**; closing it with a published floor makes it exercised **tomorrow**. The milestone has now made this exact trade four times and gated it three times — clause 1's convention-dependent groups, clause 3's unrelated chains, clause 4's slots and probes — and the fourth is the one that got a fixture.

**Price.** One `[dcel]` key (`gate_min_loops_of_three_or_more`), one field on `DcelArm` or the report from `loop_length_profile`, one conjunct. Same shape as `gate_min_slots_perturbed`, which delta-2 added for the same reason.

### M5A-D3-N2 — MINOR. Chain maximality is bound by nothing; a gate clause catches it incidentally.

Per C3.2(a) and E8c. §12 names "maximal shared boundary chains" as one of the six representation-held invariants; the representation does not hold it and the audit does not check it. In practice the corpus run catches an `assemble`-level violation through clause 3's path comparison — a mechanism for a different property. Worth a named limitation with the M6 owner, because §14.2's span candidates consume the chain decomposition, which is exactly when "caught by a neighbouring clause" stops being adequate.

## §C5. Conditions, by halves, each with its own status (condition 38)

| | half | status |
|---|---|---|
| **52** | (a) swapped args | **CLOSED** |
| | (b) M5 rows bound positionally | **OPEN**, limitation 36, owner M6 |
| **53** | (a) eight stale numbers | **CLOSED** |
| | (b) numbers derived, not errata'd | **OPEN**, same owner |
| **54** | (a) four M5 CI steps | **CLOSED** |
| | (b) claim derived from `ci.yml` | **CLOSED** |
| | (c) CI observed green | **OPEN — yours** |
| **55** | (a) excluded count published | **CLOSED** |
| | (b) compound transactions attempted | **OPEN**, limitation 37, owner M6 |
| **56** | tautological conjunct removed | **CLOSED** |
| **57** | (a) degree multiset | **CLOSED** |
| | (b) junction detection | **CLOSED** |
| **58** | (a) `path[j].1` perturbed | **CLOSED** |
| | (b) site count judged from outside | **CLOSED** |
| **59** | (a) range guards | **CLOSED** |
| | (b) residual priced at `with_parts` | **CLOSED** |
| **60** | (a) knockout non-emptiness | **CLOSED** |
| | (b) traceability | **CLOSED** |
| | (c) saddle axis declared | **CLOSED** |
| **61** | (a) §12 claim corrected in all three places | **CLOSED** |
| | (b) ORIENTED check anchored to the labelling | **CLOSED**, and by the stronger of my two variants |
| | (c) clause-4 row states guarded-not-measured; disjointness re-measured | **CLOSED** |
| **62** *(new)* | (a) population floor for loops of length ≥3, gated | **OPEN** — D3-N1 |
| | (b) `loop_length_profile` published to the artifact | **OPEN** — D3-N1 |
| | (c) chain maximality: checked, or declared with an owner | **OPEN** — D3-N2, owed **before M6 walks chains** |

## §C6. F-0048 on my own method

**Q1 — literal enumerating my subjects?** Yes: the governor's three asks. My mitigation was to derive the fourth subject rather than receive it — E8c came from asking Q1 of `steps_of`'s *inputs* (it reads `faces[].loops` and `boundaries[].path`, and concatenates), which is reading the provenance graph rather than a list I invented. That is the method I have been charging others with, applied to myself for the first time deliberately.

**Q2 — what happens at the next finding?** Still "the governor names a fourth". Unchanged, and I do not have a fix for it.

**Q3 — who was my judge?** The compiler and the test binary for E7, E8, E8b, E8c. My *reading* produced the E8 hypothesis, and my reading was wrong.

**Q4 — did my guard share a key with the mechanism?** For E8c the verdict is `audit()` returning `Ok` — the mechanism judging itself, which is legitimate here because the question is "does it accept", not "is it right".

**Q5 — both directions?** E7 red, E8/E8b red *unexpectedly*, E8c green-where-it-should-be-red, baseline green.

**Where I was wrong, and it is the useful part.** I predicted E8 would pass everything and it was caught twice, by a mechanism I had not modelled. I refined the corruption twice before I could say anything true, and what survived is much narrower than my first claim: not "non-maximal chains pass everything" but "**the audit accepts them; the gate does not**". In addendum 1 I published a refuted hypothesis and it produced the exact boundary; here the same discipline turned a would-be blocker into a minor. **The first version of a finding is a hypothesis about the tree, and the difference between it and the third version is the whole value of running it.**

## §C7. What I could not verify

CI execution (yours). Cross-platform / A7.1. Donor sources (D-3). The corpus-wide loop-length figures (1.082, 55 of 1334) — I could not check them against anything, because no artifact carries them; that inability *is* D3-N1.

## §C8. Verdict

**VERDICT (addendum 3): ACCEPT WITH CONDITIONS — one major (M5A-D3-N1), one minor (M5A-D3-N2), no blocker.**

D2-N1 is closed by the stronger of the two remedies I offered, with the weaker one correctly rejected on Q4 grounds — the first time in this cycle a reviewer's proposal has been improved rather than implemented. Conditions 61(a,b,c) are closed, everything reproduces, and the succ-residual the author declared is now verified by execution rather than accepted on his word.

The two new findings are of a kind the milestone has repeatedly closed elsewhere: a check without a gated population, and an invariant §12 names that the representation does not hold. Neither is a live defect passing every instrument — the ORIENTED check is exercised today by a fixture a default-path test asserts, and non-maximal chains redden clause 3 on the corpus.

**The date at which this stops being MET is the same one I named in addendum 2 and it is now closer:** M6/§14.2 consumes the chain decomposition, and 62(a–c) are owed before it does.

**GATE §28 M5: MET**

---

# REVIEW_M5_A — addendum 4 (delta-4)

Reviewer A, independent cold review, Opus 5. Signed §0–§9 and addenda 1–3 untouched.
Object: **`2c7bac4`**, commits **C264–C267** on top of `4205d87`.

## §D0. Hygiene

```
start / end:  main repo  git status --porcelain → (empty)   HEAD = 2c7bac48790a1db896f9a3d26003a737b47dc180
              measuring clone …/m5a-delta4-rev-kw39s  (empty)  HEAD = 2c7bac4
              experiment clone …/m5a-delta4-exp-kw39s  M walk.rs  (deliberately dirty)
              git worktree list → one entry, the main tree
```

`CARGO_TARGET_DIR=…/tgt-m5a-delta4-kw39s` asserted **not to exist** before the first command. Every cargo call blocking. No `git worktree`.

**§27.7 verified independently:** C264 → `configs/GATES_V1.toml` only; C266 → `docs/gt/DCEL_M5.json` only. Each on its own commit, as you said.

## §D1. Reproduced

| command | result |
|---|---|
| `cargo fmt --all --check` | `FMT_EXIT=0` |
| `cargo clippy --workspace --all-targets -- -D warnings`, **cold target** | `CLIPPY_EXIT=0`, zero diagnostics |
| `cargo test --locked --workspace` | `DEBUG_EXIT=0` — **544 passed, 0 failed, 15 ignored** |
| `cargo test --locked --release --workspace` | `RELEASE_EXIT=0` — **544 passed, 0 failed, 15 ignored** |
| `gt-corpus dcel --scope full` | `GATE_EXIT=0`, **four `[MET]`**; 480 arms, 240 groups, 13 convention-dependent, 170/170 committed, 30 probes, 179 253 slots, UNCAUGHT 0 |
| artifact | `D6A3CF6E…A026300` — **byte-identical** |
| `dcel_props -- --ignored` | 2 passed |
| `dcel_harness -- --ignored` | **8 passed**; printed `arms with a loop of >=3 half-edges: 20 (14 corpus, 6 register); longest 8; total such loops 28` |

## §D2. Your three questions

### D2.1 — The floor stands on the register share. What it does not bind.

**62(b) is fully closed** and I want that on the record first: `arms_with_a_loop_of_three_or_more`, `…_from_corpus`, `…_from_register`, `longest_loop_seen` and `loops_of_three_or_more_total` are all in the artifact, per arm and in totals. The half of D3-N1 that said "these numbers are under no mechanism" no longer holds anywhere.

And the author corrected a sentence nobody challenged: delta-3 said "neither M5 population had them", which restated the red team's *upper bound* ("at most 55 of 1334") as a measurement of absence. Measured, it is false — the corpus carries **14** such arms. He found that by measuring a number he had previously inherited. That is the F-0028 class caught by its own author before a reviewer reached it.

**What the floor does not bind — and it is narrower than your hypothesis.** Your candidate was: corpus stops carrying long loops, floor stays met, corpus check goes vacuous silently. That is right in form, and the honest statement of it is:

> The floor guarantees the ORIENTED check is **exercised**; it does not guarantee it is exercised on **the population the clause is about**. Clause 4 is measured over 480 arms, 444 of them corpus; the ORIENTED half is floored at ≥4 **register** arms — 0.8 % of the population, all synthetic, all one fixture.

The corpus's 14 are published beside it and floored by nothing, exactly as the gate-file comment says. I accept the author's reason for putting the floor on the register — that is where condition 51's by-construction guarantee lives — and the residual is the price of that choice, correctly stated in the file.

**But the floor does not encode the standard it cites.** Measured: the register's entire long-loop contribution is `diagonal_staircase` at s32/s64/s128 × 2 arms = **6**. The floor is **4**. So one whole size can drop out and the floor still passes. The gate-file comment justifies the placement by "condition 51's standard: every size, both arms" — a floor of 4 encodes *at least two of the three sizes*. Enforcing the stated standard needs 6, or a per-size assertion. **M5A-D4-N3, MINOR.**

### D2.2 — Are `DropLongLoops`'s "red" and "empty" independent? **No.**

`row()` in the harness is `gate_table(&cfg)` filtered to the clause-4 row, i.e. the full conjunction — which now contains `min_register_arms_with_a_long_loop.met_by(count)` with the floor at 4. The knockout asserts:

```rust
assert_eq!(knocked.arms_with_a_loop_of_three_or_more_from_register, 0, ...);
assert!(!row(&knocked), ...);
```

Given a floor > 0, `count == 0` **analytically implies** `!row`. The second assertion carries no information the first does not. What it demonstrates is that the **floor** fires — worth demonstrating, and it is the same thing twice, not two directions.

The third leg the author names — no-op excluded, because the count is computed from real loop lengths rather than from a flag — *is* independent and does hold. So the control has **two** independent legs presented as three. **M5A-D4-N2, MINOR.**

### D2.3 — Q4 on the maximality check, by provenance

**The check reads `d.vertices()` and `d.boundaries()[i].path` — both outputs of the same `assemble`.** The justification in the code says: *"the vertex set is built from lattice degree, which is a function of the labelling, so this does not share a provenance with the chain splitting it judges."* That sentence describes how `assemble` **builds** the set; it does not describe what the check **reads**. The check never consults the labelling. It is a consistency comparison between two stored fields of one constructor — RT5-A9's shape, a fourth time, inside the fix for a finding whose entire content was provenance.

And the consequence is not hypothetical. See D4-N1.

## §D3. Findings

### M5A-D4-N1 — MAJOR. 62(c) is declared closed and is half closed: the check catches under-splitting, and D3-N2 as I ran it is over-splitting.

Non-maximality has two directions:

| | violation | delta-4's criterion (`no interior point of a chain is a vertex`) |
|---|---|---|
| **under-split** | a junction lies *inside* a chain | **catches it** — this is what the new test exercises |
| **over-split** | a chain *endpoint* is not a junction | **does not catch it** — no vertex is interior |

Delta-4's test promotes an interior point to a vertex and leaves the chain whole, so the vertex is interior and the check fires. **My D3-N2 experiment split the chain**, making the split point an endpoint of both halves. Re-run verbatim against `2c7bac4`:

```
REV-A E8c/d4 boundaries 2 -> 3
REV-A E8c/d4 split point (30, 17) has lattice degree 2  (a junction needs != 2)
REV-A E8c/d4 vertices interior to a chain: 0
REV-A E8c/d4 audit(broken) -> None
REV-A E8c/d4 loops_agree   -> true
```

A chain endpoint at a degree-2 lattice point — not a junction — and the audit accepts it. §12's "maximal" is still unbound in the direction my experiment demonstrated.

**Why, in one line:** the check binds the decomposition to the *stored* vertex set. Nothing binds the stored vertex set to the junctions. **Remedy, one comparison, anchored to the input:** re-derive the vertex set from the labelling (`arr.degree(v) != 2`, plus the artificial-vertex rule for junction-free loops) and compare against `d.vertices()`. That binds both directions at once and does not share a provenance with what it judges.

**Severity.** The underlying gap is what I rated MINOR in addendum 3 and I do not raise it: it needs a defect inside `assemble`, and on the corpus clause 3's path comparison catches it (I measured 2 and 8 rolled-back transactions in addendum 3's E8/E8b). What is MAJOR is that it is **recorded as closed**. In this project a closure claim is an instrument like any other, and this one reports green on half its domain.

**Class rule, and I own part of it.** The author reproduced D3-N2 from my *prose* — "splitting a chain at an interior degree-two point" — which is ambiguous between promoting a vertex and splitting the chain. My report did carry the disambiguating numbers (`boundaries 2 -> 3` **and** `vertices 2 -> 3`); his reproduction moved only the second. **A finding must ship as the exact transformation, not as a description of it** — which is F-0067's own rule ("a finding named in a report and absent from the tree"), applied to the reviewer's side of the boundary. Had I shipped the twenty lines instead of the sentence, this delta would have closed both directions.

## §D4. Conditions, by halves (condition 38)

| | half | status |
|---|---|---|
| **52** | (a) swapped args | **CLOSED** |
| | (b) M5 rows bound positionally | **OPEN**, limitation 36, owner M6 |
| **53** | (a) eight stale numbers | **CLOSED** |
| | (b) numbers derived, not errata'd | **OPEN**, same owner |
| **54** | (a) four M5 CI steps | **CLOSED** |
| | (b) claim derived from `ci.yml` | **CLOSED** |
| | (c) CI observed green | **OPEN — yours** |
| **55** | (a) excluded count published | **CLOSED** |
| | (b) compound transactions attempted | **OPEN**, limitation 37, owner M6 |
| **56** | tautological conjunct removed | **CLOSED** |
| **57** | (a) degree multiset · (b) junction detection | **CLOSED · CLOSED** |
| **58** | (a) `path[j].1` · (b) leaf-count judge | **CLOSED · CLOSED** |
| **59** | (a) range guards · (b) residual priced | **CLOSED · CLOSED** |
| **60** | (a) knockout non-emptiness · (b) traceability · (c) saddle axis | **CLOSED · CLOSED · CLOSED** |
| **61** | (a) §12 claim corrected · (b) ORIENTED anchored to labelling · (c) row states guarded-not-measured | **CLOSED · CLOSED · CLOSED** |
| **62** | (a) population floor, gated | **CLOSED**, with D4-N3 on its margin |
| | (b) `loop_length_profile` in the artifact | **CLOSED** |
| | (c) chain maximality | **HALF CLOSED — under-split bound, over-split not.** Reopened as 63 |
| **63** *(new)* | (a) vertex set re-derived from the labelling and compared | **OPEN** — closes both directions; owed **before M6 walks chains** |
| | (b) `DropLongLoops` gains a leg that is not implied by the floor | **OPEN** — D4-N2 |
| | (c) floor raised to 6, or per-size, to match the standard it cites | **OPEN** — D4-N3 |

## §D5. F-0048 on my own method

**Q1 — literal enumerating my subjects?** Your three questions, again. The mitigation that worked was not planning: it was re-running a *previous* experiment against the new tree rather than reasoning about whether the fix covered it. E8c-against-delta-4 took four minutes and settled what an hour of reading the diff would have left ambiguous.

**Q2 — what happens at the next finding?** Unchanged: you name a fourth. I still have no fix, and I now think the fix is not mine to build — it is that a reviewer's findings should be *executable artifacts* in the tree, which is exactly what 63(a) would make of this one.

**Q3 — who was my judge?** The compiler and the test binary for E8c. For D2.1 and D2.2 the judge was reading the conjunction and the artifact — weaker, and I say so: D2.2's conclusion is analytic, derived from `row()` being the full `gate_table` row and the floor being 4. I did not construct a case with `count == 3` to show the row still falls; the implication is sound without it, but that is an argument, not a measurement.

**Q4 — did my guard share a key with the mechanism?** For E8c the verdict is `audit()` returning `Ok`, which is the mechanism judging itself — legitimate, because the question is "does it accept", not "is it right". The independent input was `arr.degree(split_pt) == 2`, computed from the labelling, which is the fact that makes "endpoint that is not a junction" a claim rather than an opinion.

**Where I was wrong, and it is the same shape as the finding.** My addendum 3 wrote the finding as a sentence and the reproduction lost half of it. I have now spent two deltas asking others to ship mechanisms rather than descriptions, and shipped a description. That is the honest reading of why 62(c) is half closed, and it is not primarily the author's error.

## §D6. What I could not verify

CI execution (yours). Cross-platform / A7.1. Donor sources (D-3). Whether a `count == 3` run reddens the row — see Q3 above; the implication is analytic and unmeasured.

## §D7. Verdict

**VERDICT (addendum 4): ACCEPT WITH CONDITIONS — one major (M5A-D4-N1), two minor (D4-N2, D4-N3), no blocker.**

62(a) and 62(b) are genuinely closed, the floor is gated and knocked out, the population is published per arm and by source, and the author corrected two of his own statements — the "neither population had them" sentence and the §12 arithmetic across three documents — without a reviewer asking. The §12 recount in particular is my delta-1 N1 class, found and fixed by its author.

62(c) is recorded as closed and covers one of the two directions of non-maximality; the direction my experiment actually demonstrated still passes the audit, verified by re-running it against this HEAD. The underlying defect keeps the MINOR rating it had — clause 3 catches it on the corpus — so the gate is unaffected. The closure *claim* is what I am rejecting, and half of why it is wrong is that I shipped prose where I should have shipped the transformation.

**The date remains the one I named in addendum 2 and it has not moved:** M6/§14.2 consumes the chain decomposition, and 63(a) is owed before it does.

**GATE §28 M5: MET**

---

# REVIEW_M5_A — addendum 5 (delta-5)

Reviewer A, independent cold review, Opus 5. Signed §0–§9 and addenda 1–4 untouched.
Object: **`66b07d0`**, commits **C269–C272** on top of `120eb2a`.

## §E0. Hygiene

```
start / end:  main repo  git status --porcelain → (empty)   HEAD = 66b07d0fff70d191c9832a36c1b28cad2450d1c2
              measuring clone …/m5a-delta5-rev-tb70x  (empty)  HEAD = 66b07d0
              experiment clone …/m5a-delta5-exp-tb70x  M walk.rs  (deliberately dirty)
              git worktree list → one entry, the main tree
```

`CARGO_TARGET_DIR=…/tgt-m5a-delta5-tb70x` asserted **not to exist** before the first command. Every cargo call blocking. No `git worktree`.

**§27.7 verified:** C269 → `configs/GATES_V1.toml` only; C271 → `docs/gt/DCEL_M5.json` only.

## §E1. Reproduced

| command | result |
|---|---|
| `cargo fmt --all --check` | `FMT_EXIT=0` |
| `cargo clippy --workspace --all-targets -- -D warnings`, **cold target** | `CLIPPY_EXIT=0`, zero diagnostics |
| `cargo test --locked --workspace` | `DEBUG_EXIT=0` — **547 passed, 0 failed, 15 ignored** |
| `cargo test --locked --release --workspace` | `RELEASE_EXIT=0` — **547 passed, 0 failed, 15 ignored** |
| `gt-corpus dcel --scope full` | `GATE_EXIT=0`, **four `[MET]`**; 480 arms, 30 probes, 179 253 slots, UNCAUGHT 0 |
| artifact | `50FB36EA…6C04ECA` — **byte-identical** |
| `dcel_props -- --ignored` | 2 passed |
| `dcel_harness -- --ignored` | **8 passed**; `20 (14 corpus, 6 register); longest 8; total such loops 28` |

## §E2. D4-N1 verified closed — in the direction my experiment demonstrated

I re-ran my over-split verbatim against this HEAD:

```
REV-A E22 over-split: audit -> Some("§12 asks for MAXIMAL shared boundary chains:
  3 stored vertices against 2 the labelling requires; 1 point(s) are vertices only in
  the structure (a chain split where nothing meets: Some((30, 17))) and 0 only in the
  labelling (a junction swallowed inside a chain: None)")
```

Caught, and the message names which of the two directions fired. **62(c) and 63(a) are genuinely closed.** The knockout now performs a real cut with every index consistent and asserts that `face_map_agrees` and `loops_agree` *reproduce* it — i.e. it encodes the blindness that made delta-4's test miss my case, rather than only the case.

I also want to record that the author confirmed **both** directions himself before editing, and wrote the cause as *"these two sentences look the same; therefore Q4 is now asked of the call site."* That is the correct generalisation of what I caught, and it is stronger than the finding.

## §E3. Your three questions

### E3.1 — Q4 on the new vertex check, **by code**

`vertices_of_the_labelling(d)` reads exactly:

| read | provenance |
|---|---|
| `d.labelling().inside()` | **the input** |
| `d.width_px()`, `d.height_px()` | `self.labelling.{width,height}_px()` — **the input** |
| `d.connectivity()` | `self.conn` — the second **input**, stored verbatim from `assemble`'s argument |

It reads **no derived field**. Not `vertices`, not `boundaries`, not `faces`, not the map. `vertices_agree_with_the_labelling` then compares that derived set against `d.vertices()` as sets. **Q4 passes on the data, by code and not by comment.**

What it shares is the **algorithm**: `orbits(&arr)`, `arr.degree()`, and the `min`-of-the-loop rule for junction-free loops are byte-identical to `assemble`'s. That is the same residual `loops.rs` already declares for the loop comparison, and it is declared consistently here. Against that residual stand the exhaustive sweep and the owner/site predicate — whose reach I measured in addendum 3 (`w = 4`) and which the file now states with that bound.

### E3.2 — What the new check does **not** bind: the residual class, stated exactly

The comparison is between **sets**. So it binds *which lattice points are vertices*. It does not bind **which index each one has**. Generalising across the whole structure, and this is the class:

> **Everything anchored to the input is anchored as a SET or as a CYCLIC SEQUENCE. Every remaining freedom is a choice of INDEX or ORDER, and those are bound only to themselves.**

Three instances. Two verified by execution:

**(a) the order of `vertices`, and the `start`/`end` indices that reference it.**
```
REV-A E21 vertices sorted before: true
REV-A E21 vertices sorted after : false
REV-A E21 audit -> None          parts changed? true
```
Swap two entries of `vertices`, remap every `start`/`end` consistently: the set is unchanged, `verts[b.start]` still equals `b.path[0]`, `V` is unchanged so Euler holds — and the audit accepts a structure whose vertex list is no longer in lattice order.

**(b) face ids among faces carrying the same label.**
```
REV-A E21b face labels: [false, true, true]
REV-A E21b audit -> None          parts changed? true
```
The per-pixel anchor compares **labels**, not ids. Swapping two same-label faces consistently through the map, the owners and the loops passes everything. (In addendum 2 the same swap was *caught* — because those two faces had **different** labels. This is the surviving half of that finding, and it survives the anchor by construction.)

**(c) the order of `loops` within a face, and the `site.1` values referencing it** — same shape, **not run**, stated as untested.

**What this costs.** None of the three is a §12 invariant, and none moves a count, so none moves the artifact. What they do contradict is `dcel/mod.rs`'s Determinism paragraph — *"every scan is in a fixed lattice order … every face id comes from raster order of its first pixel … §5.5 Tier A is a byte comparison for this structure"*. That claim holds only if `assemble` emits the canonical order, and **nothing checks that it does**. A defect inside `assemble` that permuted either would be self-consistent, pass every check, and leave the artifact byte-identical — the D1-N1 shape, moved from correctness onto canonicalisation. **M5A-D5-N1, MINOR**, because the property at risk is Tier A determinism rather than topology, and because §5.5's Tier A promise is a same-binary byte comparison that a permuting `assemble` would still satisfy against itself.

### E3.3 — The floor of six, the other side

**Your premise does not hold for this implementation, and I checked it rather than assumed it.** `Threshold::met_by` is `measured >= self.0` (`topology/gate.rs:49-51`). A floor of six therefore reddens on **loss** and passes on **growth**: eight register arms would still be MET. Nothing in the tree asserts equality, so the floor is not brittle upward. The gate-file comment claims only the loss direction — *"lose any size, or any arm, and the row goes NOT MET"* — and that claim is exact.

**The real other side is the one the comment names and nothing enforces.** The floor encodes "every size" *only while the size list has three entries*. Add a fourth size: the register produces eight, the floor stays six, and one whole size can drop out silently — exactly the slack D4-N3 was about, restored. The author writes the cost — *"this floor now moves whenever the register's size list does"* — but the coupling is manual. F-0048 Q2's answer here is **"somebody remembers to bump the number"**, which is the form this project has rejected eight times.

The closure is the project's own pattern, one comparison: **derive the expected count from the register** (fixtures carrying a long loop × sizes × arms) and require the floor to **equal** it, the way `every_frozen_value_agrees_with_the_code_that_uses_it` already ties file to code. That makes the row red on loss *and* on unbumped growth, and the number stops being a hand-maintained transcription of a list. **M5A-D5-N2, MINOR.**

## §E4. Conditions, by halves (condition 38)

| | half | status |
|---|---|---|
| **52** | (a) swapped args · (b) rows bound positionally | **CLOSED** · **OPEN**, limitation 36, M6 |
| **53** | (a) eight stale numbers · (b) numbers derived | **CLOSED** · **OPEN**, M6 |
| **54** | (a) CI steps · (b) claim derived from `ci.yml` · (c) CI observed green | **CLOSED · CLOSED · OPEN — yours** |
| **55** | (a) excluded count published · (b) compound transactions attempted | **CLOSED** · **OPEN**, limitation 37, M6 |
| **56** | tautological conjunct removed | **CLOSED** |
| **57** | (a) degree multiset · (b) junction detection | **CLOSED · CLOSED** |
| **58** | (a) `path[j].1` · (b) leaf-count judge | **CLOSED · CLOSED** |
| **59** | (a) range guards · (b) residual priced | **CLOSED · CLOSED** |
| **60** | (a) knockout non-emptiness · (b) traceability · (c) saddle axis | **CLOSED · CLOSED · CLOSED** |
| **61** | (a) §12 claim · (b) ORIENTED anchored · (c) row states guarded-not-measured | **CLOSED · CLOSED · CLOSED** |
| **62** | (a) population floor gated · (b) profile in the artifact · (c) chain maximality | **CLOSED · CLOSED · CLOSED** (both directions, verified E22) |
| **63** | (a) vertex set re-derived from the labelling | **CLOSED** — Q4 passes by code |
| | (b) `DropLongLoops` third leg | **CLOSED AS NARROWED** — recorded as two legs in STATUS §A5.3 with what a real third would require. Correct response to that finding |
| | (c) floor raised to match its standard | **CLOSED for the current size list**; the coupling is manual — D5-N2 |
| **64** *(new)* | (a) the floor derived from the register instead of transcribed | **OPEN** — D5-N2 |
| | (b) canonical order of `vertices`, face ids among same-label faces, loop order within a face — checked, or the Determinism paragraph narrowed to what is held | **OPEN** — D5-N1 |

## §E5. F-0048 on my own method

**Q1 — literal enumerating my subjects?** Your three questions again, and this time I can name what broke the pattern: I answered E3.3 by *reading `met_by`* instead of accepting the premise in the question. The premise was wrong, and had I taken it I would have written a finding that does not exist. **The reviewer's own brief is an untrusted input, and Q4 applies to it.**

**Q2 — what happens at the next finding?** Unchanged.

**Q3 — who was my judge?** The test binary for E21, E21b, E22. For E3.1 the judge was the function body — I listed every expression it reads, which is the check I invented in addendum 4 after being burned by a comment. For E3.3 the judge was `met_by`'s two lines.

**Q4 — did my guard share a key with the mechanism?** For E21/E21b the verdict is `audit()` returning `Ok`, the mechanism judging itself — legitimate, since the question is "does it accept". The independent facts are `vertices sorted after: false` and `face labels [false, true, true]`, both computed from the value rather than from the audit.

**Q5 — both directions?** E22 red (the closure works), E21/E21b green-where-they-should-be-red (the residual), baseline green.

**What I got right that I did not, last time.** My addendum 4 finding was half my own fault — I shipped prose and the reproduction lost the half prose does not distinguish. This time every claim in §E3.2 arrived with the twenty lines that produce it, and the author has taken that rule into the ledger. That is the only method change I have made across five deltas that I would defend as an improvement rather than as luck.

## §E6. What I could not verify

CI execution (yours). Cross-platform / A7.1. Donor sources (D-3). Residual instance (c) — loop order within a face — named by symmetry and not run. Whether a permuting `assemble` would survive `dcel-check` on a *second machine*; on one machine it agrees with itself by construction.

## §E7. Verdict

**VERDICT (addendum 5): ACCEPT WITH CONDITIONS — two minor (D5-N1, D5-N2), no major, no blocker.**

D4-N1 is closed in the direction my experiment demonstrated, verified by re-running it; the new vertex derivation passes Q4 by code and not by comment; the floor is raised to the value that makes it mean what its comment says; and the D4-N2 overstatement is narrowed rather than papered over. This is the first delta in which nothing I found is a claim that the tree does not keep.

The two minors are both of the same shape and both small: a number that must be bumped by hand, and a canonicalisation claim wider than what is checked. Neither touches a §28 M5 clause, neither moves a count, and neither needs a blocker to be worth closing.

**The M6/§14.2 date I have carried since addendum 2 is now clear.** Chain decomposition is bound in both directions, so the thing M6 consumes is anchored. What remains open against M6 is unchanged and unrelated: limitation 36 (52b/53b), limitation 37 (55b), and CI (54c) — yours.

**GATE §28 M5: MET**

---

# REVIEW_M5_A — addendum 6 (delta-6)

Reviewer A, independent cold review, Opus 5. Signed §0–§9 and addenda 1–5 untouched.
Object: **`bda56f5`**, commits **C274–C276** on top of `a8576cc`.

## §F0. Hygiene

```
start / end:  main repo  git status --porcelain → (empty)   HEAD = bda56f5436945460e4de621bbc35b2cb4c00b2b1
              measuring clone …/m5a-delta6-rev-jx58r  (empty)  HEAD = bda56f5
              experiment clone …/m5a-delta6-exp-jx58r  M mod.rs, M walk.rs  (deliberately dirty)
              git worktree list → one entry, the main tree
```

`CARGO_TARGET_DIR=…/tgt-m5a-delta6-jx58r` asserted **not to exist** before the first command. Every cargo call blocking. No `git worktree`.

**§27.7 verified:** `git log a8576cc..bda56f5 -- configs/GATES_V1.toml` is **empty** — the gate file was not touched. C275 → `docs/gt/DCEL_M5.json` only.

## §F1. Reproduced

| command | result |
|---|---|
| `cargo fmt --all --check` | `FMT_EXIT=0` |
| `cargo clippy --workspace --all-targets -- -D warnings`, **cold target** | `CLIPPY_EXIT=0`, zero diagnostics |
| `cargo test --locked --workspace` | `DEBUG_EXIT=0` — **549 passed, 0 failed, 15 ignored** |
| `cargo test --locked --release --workspace` | `RELEASE_EXIT=0` — **549 passed, 0 failed, 15 ignored** |
| `gt-corpus dcel --scope full` | `GATE_EXIT=0`, **four `[MET]`** |
| artifact | `36875DDC…15CB3496` — **byte-identical** |
| `dcel_props -- --ignored` · `dcel_harness -- --ignored` | 2 passed · **8 passed** |

## §F2. The artifact: shape, not values — verified key by key

I diffed the delta-5 and delta-6 artifacts programmatically rather than reading the 9601-line diff:

```
arms_measured 480/480  corpus 444/444  structural 36/36  refused 0/0
empty 8/8  non_empty 472/472  groups 240/240  classes_in 253/253  classes_out 253/253
convention_dependent 13/13  tx 170/170/0  unrelated 130/0  failing_audit 0/0
long-loop arms 20/20  corpus 14/14  register 6/6  longest 8/8  loops total 28/28
audit_resolving_power identical: True     classes identical: True
value changes among checked keys: 0
```

**Every number from my addendum 5 matches.** The change is per-arm: nine flat fields (`vertices`, `boundaries`, `segments`, `loops`, `faces`, `skeleton_components`, `euler_lhs`, `euler_rhs`, `directed_steps`) replaced by one `audit` field. Measured on the committed artifact: **0 arms with `audit: null`, 8 with `audit.directed_steps == 0`**, all `adv/sliver#a`. "The instrument did not report" and "the subject was absent" are now different states, which is RT5-A21's point.

And a smaller thing I want to record because it is mine: the artifact now carries `reachable_refusals_on_this_population = [UnrelatedGraphMutation, CandidateFailedAudit]` and `unreachable_refusals_on_this_population = [EditLeftTheCanvas, EditLeftTheRoi, EditIsANoOp, NotTheDeclaredEdit]`. That is **addendum 1 §M5A-D1-N2** — four of six refusal reasons unreachable by construction — turned from a limitation into published data.

## §F3. D5-N1: what was done, and all three cases run

The response is a **split**, and it is the right split. Verified by execution against this HEAD:

| my instance | delta-6 | my measurement |
|---|---|---|
| **(a)** order of `vertices` | **CHECKED** — `vertices_agree_with_the_labelling` now compares **sequences**, not sets | E27 → `audit: Some("§12 asks for MAXIMAL shared boundary chains: the vertex SET is right and its ORDER is not: 2 vertices, stored in an order the canonical one does not produce (REVIEW_M5_A D5-N1)")` — **caught** |
| **(b)** order of `loops` within a face | **NARROWED**, owner M6 | E28, *the case I named by symmetry and did not run in addendum 5, now run*: face 1 has 2 loops, swap them, rebuild `site` → `audit: None`, `loops_agree: true`, `parts changed: true` — **not caught, exactly as the narrowing says** |
| **(c)** face ids among same-label faces | **NARROWED**, owner M6 | E29: labels `[false, true, true]`, swap ids 1↔2 through map, owners and loops → `audit: None` — **not caught, exactly as the narrowing says** |

**A narrowing has to be checked for accuracy, not only for honesty, and this one is accurate on both counts it makes.** That is the substantive test of a narrowing and it passes.

**Q4 by code on the new check.** `vertices_agree_with_the_labelling` compares `d.vertices()` (ordered) against `vertices_of_the_labelling(d)`, which reads `d.labelling().inside()`, `d.width_px()`, `d.height_px()` and `d.connectivity()` — the two **inputs** and no derived field. Q4 passes. The residual it shares is the algorithm (`orbits`, `arr.degree`, the `min` rule) — already declared — plus one new and smaller thing: the *canonical order* is now defined as "what the derived `BTreeSet` produces", and `assemble` produces the same order for the same reason. Both sides take "sorted" as canonical, and the tie to §5.5's "fixed lattice order" is prose on both sides. Not a defect; worth one sentence somewhere, and I am not raising it as a finding.

## §F4. Findings

### M5A-D6-N1 — MAJOR. The floor derivation closes D5-N2 in the strong form and reads its own size list from a literal, under a comment saying it does not.

`the_oriented_floor_equals_what_the_register_produces` is exactly the mechanism I asked for: it walks `structural_fixtures(n)` × sizes × arms, counts arms with `at_least_three > 0`, guards against a vacuous `0 == 0`, and asserts **equality** against `cfg.min_register_arms_with_a_long_loop.registered_value()`. Add a size or a long-loop fixture and it fails until the gate file moves — a §27.7 commit by design. The answer to F-0048 Q2 is no longer "someone will remember".

But:

```rust
// Sizes the harness actually builds structural arms at, taken from the run
// rather than from a literal here.
let sizes = [32usize, 64, 128];
```

The production path derives them (`dcel/mod.rs:470-475`): `cells.iter().map(|c| c.size_px)` into a `BTreeSet`. The test hardcodes them **two lines below a comment asserting it does not**. If the cell list gained a size, the register would produce eight arms, the *measured* count would be eight, and this test would compare the frozen six against its own stale literal six — and pass. The mechanism that exists to catch drift is itself keyed on the thing that drifts.

**Class rule.** This is delta-5's own lesson — *"these two sentences look the same; therefore Q4 is now asked of the call site"* — applied to a comment about **provenance of the test's inputs** rather than of the check's. **A derivation is only as derived as its least-derived input, and a comment claiming derivation is not evidence of it.** The fix is three lines: take the sizes from `matrix_v1()`/`TOPOLOGY_CELL_IDS` the way `run()` does.

### M5A-D6-N2 — MAJOR. The clause-4 negative claim is keyed on the field's TYPE; the LOCATION axis is open, and needs no exotic type at all. Verified.

The row says it certifies "the fields the structure has TODAY", and names the bypass as *a field whose type serialises to nothing — a newtype writing `serialize_none()`*, with the proc-macro remedy owned by M6.

That names one axis. I tested the other: an **ordinary `u32`** added to **`Dcel`** rather than to `Parts`, set to `0xDEAD` above 16 px, behind a public accessor. No exotic type, no custom `Serialize`.

```
every_scalar_leaf_of_parts_has_exactly_one_perturbation ... ok
every_slot_family_is_non_empty_and_declared             ... ok   (families unchanged)
every_perturbation_of_every_slot_is_caught_by_the_audit  ... ok   (slots 372 | caught 372)
gt-corpus dcel --scope full                              ... four [MET], EXIT 0
artifact                                                 ... BYTE-IDENTICAL
```

The leaf count is computed over `d.parts()`, so a field on `Dcel` is outside the ruler as well as outside the measurement. **The proc-macro remedy the row names would close the type axis and not this one** — it would still have to be pointed at `Parts`.

*(My experiment did trip `no_production_module_is_over_the_size_rule` — on `walk.rs` at 842 lines, which is my own probe file, not the shadow field. The size rule is not a defence here.)*

**Class rule.** F-0048's last paragraph requires the honest boundary to be stated **at the cheapest known bypass price**. The row states a price — an exotic serialisation — that is higher than the cheapest one, which is a plain field one struct up. This is the second time in this milestone I have found a residual priced above its true cost (addendum 1 §M5A-D1-N5, where `with_parts` already existed and the doc priced the bypass at writing a new `pub fn`). The closure has the shape this project already uses for documents: **take the subject set from the side the attacker does not edit** — every field of `Dcel` reachable by a public accessor is either in `Parts` or declared exempt with a reason.

## §F5. Conditions, by halves (condition 38)

| | half | status |
|---|---|---|
| **52** | (a) swapped args · (b) rows bound positionally | **CLOSED** · **OPEN**, limitation 36, M6 |
| **53** | (a) stale numbers · (b) numbers derived | **CLOSED** · **OPEN**, M6 |
| **54** | (a) CI steps · (b) claim derived from `ci.yml` · (c) CI observed green | **CLOSED · CLOSED · OPEN — yours** |
| **55** | (a) excluded count published · (b) compound transactions attempted | **CLOSED** · **OPEN**, limitation 37, M6 |
| **56–61** | all halves as recorded in addendum 5 | **CLOSED** |
| **62** | (a) floor gated · (b) profile in artifact · (c) maximality both directions | **CLOSED · CLOSED · CLOSED** |
| **63** | (a) vertex set from the labelling · (b) `DropLongLoops` narrowed to two legs · (c) floor matches its standard | **CLOSED · CLOSED · CLOSED** |
| **64** | (a) floor derived from the register, `==` | **CLOSED IN FORM** — the equality is right; its size input is a literal. See D6-N1 |
| | (b) index/order freedoms checked or narrowed | **CLOSED** — (a) checked, (b) and (c) narrowed with owner M6, all three verified by me |
| **65** *(new)* | (a) the floor test's size list taken from the cell matrix | **OPEN** — D6-N1 |
| | (b) the clause-4 negative claim extended to the location axis, or the walk's subject derived | **OPEN** — D6-N2 |

## §F6. F-0048 on my own method

**Q1 — literal enumerating my subjects?** Your asks again, but the two findings this round came from elsewhere: E28 from my own addendum-5 label "*not run*", and E30 from asking Q1 of the negative claim's **key** rather than its content.

**Q3 — who was my judge?** The test binary for E27–E30. For §F2 the judge was a key-by-key programmatic diff of the two artifacts, not reading a 9601-line patch — which is the only way that claim could have been checked honestly.

**A new risk of my own method, and it nearly landed.** In addendum 5 I named instance (c) by symmetry and did not run it. Delta-6 turned that unverified claim into a **declared limitation with an owner**. It happened to be correct — E28 confirms it — but had it been wrong, my untested symmetry would now be a documented property of the tree. **A reviewer's unverified claim can become a milestone's declared limitation, and the label "not run" is the only thing standing between the two.** The label worked here because you read it and made me run it. That is not a mechanism, it is you; and the mechanism version is the one I have been asking the author for all milestone: ship the transformation, not the description.

**Q5 — both directions?** E27 red (the new check works), E28/E29 green-where-unchecked (the narrowing is accurate), E30 green-where-it-should-be-red (the residual), baseline green throughout.

## §F7. What I could not verify

CI execution (yours). Cross-platform / A7.1. Donor sources (D-3). Whether the proc-macro remedy would in fact close the type axis — it is unbuilt and I judged only its stated scope.

## §F8. Verdict

**VERDICT (addendum 6): ACCEPT WITH CONDITIONS — two major (D6-N1, D6-N2), no blocker.**

The artifact re-record is shape-only and I verified it key by key; D5-N1 is answered with one check and two accurate narrowings, all three of which I ran; D5-N2 is closed in the strong form I asked for; and my delta-1 refusal-reachability finding is now published data rather than prose. Both new findings are claim-boundary defects of the same family the milestone has been closing all cycle — a derivation whose own input is a literal, and a residual priced above its cheapest bypass. Neither touches a §28 M5 clause, neither moves a count, and the shipped tree contains no such field and no such drift.

**The M6 ledger against this milestone is unchanged and none of it is topology:** limitation 36 (52b/53b), limitation 37 (55b), the two narrowed index/order freedoms (64b), and CI (54c) — yours.

**GATE §28 M5: MET**

---

# REVIEW_M5_A — addendum 7 (delta-7, confirmatory)

Reviewer A, independent cold review, Opus 5. Signed §0–§9 and addenda 1–6 untouched.
Object: **`bb2ae20`**, commits **C278–C279** on top of `d43b5a4`. Confirmatory pass, not a full review.

## §G0. Hygiene and reproduction

```
main repo  start / end   git status --porcelain → (empty)   HEAD = bb2ae20de615aa373b3f031b3466eaadd72603d0
clone …/m5a-delta7-rev-qc94v   clean before and after the reported run   HEAD = bb2ae20
git worktree list → one entry, the main tree.  No git worktree used.
```

| command | result |
|---|---|
| `cargo fmt --all --check` | `FMT_EXIT=0` |
| `cargo clippy --workspace --all-targets -- -D warnings` | `CLIPPY_EXIT=0`, zero diagnostics, full 31 s compile from an empty target dir |
| `cargo test --locked --release --workspace` | `RELEASE_EXIT=0` — **551 passed, 0 failed, 15 ignored** |
| `gt-corpus dcel --scope full` | `GATE_EXIT=0`, **four `[MET]`**; 480 arms, 30 probes, 179 253 slots, UNCAUGHT 0 |
| artifact | `36875DDC…15CB3496` — byte-identical, **and the same hash as delta-6** |
| `git log d43b5a4..bb2ae20 -- docs/gt/DCEL_M5.json configs/GATES_V1.toml` | **empty** |

Your claim checks out exactly: every edit was in the **judge**, none in the **measurement**. The artifact hash is unchanged from delta-6, which is a stronger statement than "reproduces".

## §G1. My two MAJORs

**D6-N1 — CLOSED, in the named form.** One source: `structural_sizes(scope)` resolves `TOPOLOGY_CELL_IDS` through the degradation matrix to a `BTreeSet` of `size_px`. `run()` calls it (`dcel/mod.rs:352`) and so does `the_oriented_floor_equals_what_the_register_produces` (`dcel_harness.rs:348`). The literal is gone and the comment that described a derivation the code did not perform is corrected.

**D6-N2 — CLOSED, in the named form.** `every_field_of_dcel_is_in_parts_or_declared` takes the subject set from the **struct's own source** and requires every field to be `parts` or an exception with a reason. That is the side an attacker does not silently edit, which is what I asked for.

**Neither closure created a new finding.** I checked both for the defect each was fixing:
- the floor derivation's own inputs are now all derived — no literal survives in it;
- the `Dcel` rule's residual is stated at *its* cheapest price (rename or move the struct — one line), and it is consolidated in `surface.rs` with the two other source scans that share that limit, rather than repeated as three separate half-statements.

## §G2. Q1 on the new exception list, as you asked

`OUTSIDE_PARTS` is a literal of four entries, so **Q1 = YES**. But the question that decides the class is Q2 — *what happens at the next finding?* — and here the answer is **a red test forces a decision**, not *append a line silently*:

- the **subject** set is read from the struct, not from the list;
- a new field on `Dcel` fails the test until somebody classifies it;
- the list may not rot in either direction — an entry naming a field that no longer exists fails, and `why.len() > 30` rejects a reason that does not say anything.

That is the `every_status_document_is_classified_or_excepted_with_a_reason` shape: a list of **decisions**, not of subjects. It is the correct form, and the report that it **failed on its author's own too-short reason for `parts` on the first run** is the best evidence that the guard is real — a mechanism that fires against its creator before it ever sees an attacker is one that was not tuned to pass.

The residual I would name if pressed: nothing checks a reason is *true*, only that it is present and long enough. That is unclosable by a test and correctly left alone.

## §G3. Limitation 53, judged as a claim

**Honest boundary with a correct price.** The obstruction is real and correctly diagnosed: `FAST_SIZES_PX` is a literal inside `vice-topology`'s own test target, the harness's sizes come from `vice-bench`'s degradation cells, and `vice-topology` cannot read `vice-bench` because the dependency runs the other way. The named fix — move the size list into the crate that owns the register — is the right one, and it is right *because* the cheap alternative (have the crate's test read the harness's list) is the one the dependency direction forbids.

Two things I will say plainly rather than inflate:

1. **The price is small** — a `const` moves from a test file into `vice-topology`'s library beside `structural_fixtures`, plus one assertion in `vice-bench` that the cell sizes match it. Deferring five lines to M6 is a judgement call, not a necessity.
2. **The consequence is correctly stated**, and it is the same class I raised as D6-N1: a new harness size would get floor coverage and gate coverage while the crate's own property tests silently went on testing a different set. That is drift with no alarm.

It is named, priced, owned, and found **by the author's own sweep rather than by a reviewer** — which is the part that matters. I record it as a real open item, not as a deferral dressed up, and I would not hold a gate for it.

## §G4. Limitation 52 — my untested case, now measured

The case I labelled *"same shape, not run"* in addendum 5 and ran in addendum 6 (E28) is now run by the author too and holds: permuting two loops of a face leaves every judge silent. It is a measurement rather than my symmetry argument, and the test states what would have to change for the limitation to lift. That closes the specific risk I flagged in addendum 6 §F6 — a reviewer's unverified claim becoming a milestone's declared limitation.

## §G5. Conditions

| | status |
|---|---|
| **52–63** | as recorded in addendum 6 — **CLOSED** |
| **64** (a) floor derived, `==` · (b) index/order freedoms checked or narrowed | **CLOSED · CLOSED** |
| **65** (a) floor test's size list from the cell matrix · (b) clause-4 negative claim extended to the location axis | **CLOSED · CLOSED** |
| **open against M6, none of it topology** | limitation 36 (52b/53b), limitation 37 (55b), limitation 52 (loop/face index order), **limitation 53** (two size declarations), the four F-0048 rows the author declares non-passing, and CI (54c) — **yours** |

## §G6. F-0048 on my own method

Short, because this pass was short. Q1: my subjects were your two questions plus one I added — checking each closure for the defect it was fixing. Q3: the judge was the test binary and the artifact hash, except for §G3, which is an argument about a dependency direction and is labelled as such. Q5: I have no red direction this round — I found nothing, and that is a weaker epistemic position than five deltas of findings, so I say it rather than manufacture a residual to look thorough.

The one thing I would flag about my own record across this milestone: **three of my seven findings were residuals priced above their true cost**, and I found all three by running the cheap bypass rather than by reading the price. That is the only reviewing habit here I would carry to another milestone.

## §G7. Verdict

**VERDICT (addendum 7): ACCEPT — no findings.**

Both of my MAJORs are closed in the exact form I named, neither closure introduced a new one, the new exception list is a list of decisions rather than subjects and has already fired against its author, and the one gap the derivation sweep turned up is named with the correct obstruction and the correct price. The judge changed, the measurement did not, and the artifact hash proves it.

**GATE §28 M5: MET**
