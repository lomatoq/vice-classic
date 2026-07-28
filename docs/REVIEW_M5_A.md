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
