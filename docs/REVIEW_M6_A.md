# REVIEW_M6_A — independent cold review A of milestone M6

**Reviewer A. Independent cold context, not the author. The author's conclusions were treated as claims and verified by execution, not by reading. No code was fixed by this review.**

---

## 0. Hygiene, at the start

- **Spec**: `docs/spec/VICE_CLASSIC_CORE_AGENT_SPEC_v1.3.md`, SHA-256 verified as the **first action**, in the main tree and again in the clone:
  `652fd0b6e17c96c38af0173ddcc93a3921eafd60a9aff34c8d848829228d9bb1` — **matches** the pinned hash.
- **HEAD**: `a65f1f7c15b79cc683fc638aa247f2038377d424`, main, clean tree before and after this review. **The main tree received no write operation of any kind from this review.**
- **Fresh clone, unique by construction** (condition 25), never `git worktree`:
  `…\scratchpad\revA-M6-20260729-121402-6625deff` (timestamp + random suffix), `git clone --no-hardlinks` of the local repository; clone HEAD verified identical to main HEAD; spec hash re-verified inside the clone.
- **Own `CARGO_TARGET_DIR`**: `…\scratchpad\revA-M6-target`, used for every cargo invocation; every cargo call was blocking.
- **Donor sources not opened** (D-3).
- Review B and the red team were not read and do not exist in the tree at this HEAD (checked: `docs/` carries no `REVIEW_M6*` or `REDTEAM_M6*`).
- All mutations described below were made in the clone only and each was reverted (`git status --porcelain` empty) before the next; the clone ends clean at `a65f1f7`.

## 1. Scope, and one Q4 note on the dispatch

Scope reviewed: **C282–C311** (`6ca4114^..e652919`), plus C312 (verified: adds exactly one file, `docs/spec/…v1.3.md`, 2337 insertions, nothing else — and the byte-identity is what the hash check above measures). Documents read whole: `docs/STATUS_M6.md` (body + addenda 1–3), `FAILURE_LEDGER.md` F-0080…F-0090, `configs/GATES_V1.toml`, ADR-0031, spec §14, §15, §28, §34, §27.7, §1.5.

**Q4 on the dispatch itself**: the dispatch states the M6 range as "C282–C311 (диапазон `f559767..e652919`)". Measured: `f559767` **is C286**, so that git range contains C287–C311 only; C282–C286 lie before it. The named commit list is right and the git range under it is narrower than the list. I reviewed C282–C311 by ancestry regardless. Second Q4 note: the dispatch's "47 бит" is the ledger's 46.6 (F-0090); rounding, harmless, and the true value 1.303 is what I reproduced. Everything else the dispatch asserted as a number was confirmed by measurement below.

## 2. Reproduction — the documented commands, with exit codes

On the clean clone, per REPRODUCIBILITY_M5 §1 (no M6 reproducibility file exists — see condition M6A-C6):

| command | result |
|---|---|
| `cargo fmt --all --check` | exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo test --locked --workspace` (debug) | **623 passed, 0 failed, exit 0** |
| `cargo test --locked --release --workspace` | **623 passed, 0 failed, exit 0** |
| `cargo test --locked --release --workspace -- --ignored` | **17 passed, 0 failed, exit 0** |
| `gt-corpus dcel --scope full` | exit 0, four clauses `[MET]` |
| `cmp` of the produced report vs `docs/gt/DCEL_M5.json` | **byte-identical** (SHA-256 `1992d5db…426f` both sides) |
| `gt-corpus dcel-check --report docs/gt/DCEL_M5.json` | exit 0, "reproduced with every metric compared" |
| `vice-bench --lib fit … --ignored` (the corpus measurement) | **digit-for-digit match with STATUS_M6 A3.5** |
| `vice-fit --test grammar_and_g1` | 10/10, all published numbers reproduced |
| `vice-bench --test dcel_harness -- --ignored` | 9/9 including `each_compound_floor_has_a_world_in_which_it_is_false` |

Reproduced exactly, among others: worst G1 spread **2.776e-15 rad over 8 smooth nodes** (corpus) and **2.220e-16 over 9** (synthetic); positive control **0.7136 rad (40.89°)**; per-cut spread **1.303 bits over 6 cuts**; `worst d_n/d_euclid` **23.8712 at 0.21824 px**; **333** cost refusals (`normal_line_misses`) out of **14659** candidates before cost; 36/36 chains with a model; relations 368/133; frozen table selects **1 arc at 106.837 bits**, cheap table selects **64 quads** with a strictly smaller residual — both directions of the BIC differential asserted.

## 3. Findings

Each finding names the exact transformation, executed in the clone and reverted.

### M6A-N1 (MAJOR). `transaction_shapes` is a typed-in literal, and the shape floor is vacuous against the exact regression its own comment names

`crates/vice-bench/src/dcel/report/mod.rs:276` constructs the report with `transaction_shapes: 3` — a **constant**, not a measurement. The field's doc comment (line 95) calls it "How many distinct edit SHAPES the harness applies per arm". The gate row (line 466) is `cfg.min_transaction_shapes.met_by(self.transaction_shapes)` — the frozen `3` from `[dcel_compound]` compared against a `3` typed into the same code a feature commit edits.

`configs/GATES_V1.toml [dcel_compound]` says: *"The SHAPE floor is the exception and is set AT the measurement, 3 of 3, because losing a shape is precisely the regression it exists to catch."* **Executed transformation:** `hole_fill_transaction_for` in `crates/vice-bench/src/dcel/shapes.rs` made to return `None` unconditionally — the deleted-shape world — then `gt-corpus dcel --scope full`:

```
transactions 960 attempted, 678 committed   (was 1032/750)
compound 172 >= 100   distinct deltas 4 >= 3   transaction_shapes = 3 (the literal)
ALL FOUR CLAUSES [MET], exit 0
```

The gate stays green through the loss of a whole edit shape. The count floor does catch losing the *annulus* (compound would fall to 6 < 100); the hole-fill shape can be lost invisibly, and the floor purpose-built for the cause catches **nothing**, ever, because its subject is not computed from the run. This is F-0048 Q4 in its terminal form: the guard and the mechanism are one number written twice. STATUS_M6 limitation 54 understates this defect — it says the knockout "proves the row responds to an emptied population, not that the harness would notice a deleted shape function", which reads as a limitation of the *test's reach*; the fact is stronger: the production value is not a measurement at all, so the harness **cannot** notice, by construction. (The knockout test at `dcel_harness.rs:479` sets `c.transaction_shapes = 2` on the report — it proves the threshold works and says nothing about the value fed to it.) The artifact would drift on re-record and a byte-diff would show it, but nothing *gates*.

### M6A-N2 (MAJOR). The `[dcel_compound]` provenance comment was false at the moment of freezing: "4 distinct compound deltas" against an artifact that carried five

Measured from `docs/gt/DCEL_M5.json` **as committed at C288** and unchanged since:

```
compound(c+1,h+1) 118   compound(c-1,h+1) 46   compound(c+0,h+2) 6
compound(c-1,h-1) 6     compound(c-3,h+1) 2          -> FIVE distinct, sum 178
```

The freeze commit **C291** wrote into `[dcel_compound]`: *"Measured at C288 on the full scope: 178 compound transactions … 4 distinct compound deltas, 3 edit shapes"* and, on the key itself, *"Measured: four distinct."* The 178 is the three-shape population; the "four distinct" is the **two-shape** population (C282–C285), which the third shape's `compound(c-1,h-1)` had already made five three commits before the freeze. A denominator and a numerator from different populations — **the author's own F-0082 class**, inside the gate file, in the *new* section, at freeze time. STATUS_M6 repeats it: A1.4 says "4 distinct deltas" while A1.2's own list five lines earlier shows five compound names; and A1.3's table row "`gate_min_distinct_compound_deltas` | 3 | 4" carries the same false 4 in its "measured" column. §4.3 of the same document records the third occurrence of the comment-drift class and limitation 56 says "the fourth is a matter of time". **The fourth occurrence was already in the tree when that sentence was written.** The gate itself is unaffected — production computes `distinct_compound` from `declared_kinds_exercised` (report/mod.rs:450–454) and gets 5 ≥ 3 — the prose is what is false.

### M6A-N3 (MODERATE). F-0088's closure has no witness: the guard can be reverted with the entire suite green

**Executed transformation:** in `crates/vice-fit/src/lib.rs` the unit-normal contract check (`(len - 1.0).abs() > UNIT_NORMAL_TOLERANCE`) reverted to finiteness-only — the exact pre-C304 defect. Result: **every vice-fit test passes (60/60, exit 0)**; no test anywhere in the workspace constructs a non-unit or zero normal and asserts `FitRefusal::NonUnitNormal` (grep over `crates/`: the variant appears in `lib.rs` and in `vice-bench`'s name map, nowhere in a test). The ledger closes F-0088 with "Исправлено в C304" naming no instrument — the only one of the four M6 defects whose closure is held by nothing. Contrast: F-0087's closure reddens under attack (N-verification below), F-0089's reddens, F-0090's test guards itself. The corpus cannot supply the witness — F-0089's own text explains why this class is invisible on the real population — so the witness must be a constructed chain. Price: one test, two legs (zero normal → refusal naming the sample; unit normal → no refusal).

### M6A-N4 (MODERATE, governance). Limitation 57's owner migrated silently, and the chain identity is now deferred a second time

STATUS_M6 A2.7, limitation 57: chain identity binding chains to the DCEL — *"Owner: the milestone that closes bullets 3–4."* Addendum 3 **is** the pass that closed bullets 3–4, it did not deliver the chain identity, and A3.7/limitation 65 re-own it to **M7** without recording that this is a reassignment against A2.7's named owner. That is the F-0049 class the author himself applies to the coordinator in §4.4 — a restatement that drops what the original bound. The price itself is unchanged and honestly named (it is what blocks gate clause 3), so this is silent in *owner*, not in *amount* — but the M5 reviewers' "no second deferral" standard now applies to this item at M7, and someone must say so in the tree rather than in this review only.

### M6A-N5 (MINOR). The numbers that decide the three green clauses live in test code, not under §27.7

The clause-1 verdict turns on `worst < 1e-9` (`grammar_and_g1.rs:181` and `fit.rs:431`); clause 2 on `0.06` (breakpoint fraction), `1.0` bit (cut leg two; scale), `1e-6` bits (translation); the G1 positive control on `> 0.4`; the cut test's non-triviality on `> 1.0`. By this repository's own established rule (M45-N6 / RT45-A5: *"a number that decides whether a §28 clause is green is a gate, wherever it is written"*), a certification of M6 cannot stand on constants in test files. The author says plainly that no gate row exists and no threshold is read from the gate file (`fit.rs` module doc), so A3.4's table is a status-document claim, not a scorecard — acceptable for an uncertified milestone, a registration debt for the certified one. Named here so it cannot be inherited silently.

### M6A-N6 (TRIVIAL). Two prose defects

(a) `dcel/report/mod.rs` carries the same "M6 adds the COMPOUND conjuncts…" paragraph twice, back to back (~lines 436–449), in the comment block that justifies the clause-3 conjuncts. (b) `fit.rs`'s assertion message at line 437 contains a run of stray spaces mid-sentence. Neither moves a number.

### Verification of the four defect closures (dispatch claim 5), by mutation

| defect | transformation executed | result |
|---|---|---|
| F-0087 (line-adjacent G1) | `node_dir` reverted to trusting the stored angle beside a `Line` | `exact_g1_holds_on_every_model_the_solver_accepts` **FAILED**, exit 101 — closure held |
| F-0088 (zero normal) | guard reverted to finiteness | **all green** — M6A-N3, closure NOT witnessed |
| F-0089 (duplicate samples) | `dedup_coincident` knocked out at `k_best_boundary_models`' entry | `the_selection_is_invariant_to_duplicate_samples` **FAILED**, exit 101 — closure held |
| F-0090 (instrument through mechanism) | read, not mutated: `models_at_cut` solves one cut without re-cutting, and leg one's `hi - lo > 1.0` assertion makes a silently re-cutting `models_at_cut` fail the test itself | self-guarding; spread 1.303 bits reproduced |

And the frozen-table binding, both directions (dispatch claim 6): gate-file value perturbed (`31.029146 → 31.03`) → `every_frozen_value_agrees_with_the_code_that_uses_it` **FAILED**; code constant perturbed instead → **two** vice-fit derivation tests and the cross-check **FAILED**. Neither side can move alone. `a_table_of_zeros_is_not_a_code_table` exists and rejects zero/NaN tables.

## 4. My own hypotheses that proved false, with refutations

1. **"`distinct_compound` might also be a literal, like `transaction_shapes`."** False: it is derived from `declared_kinds_exercised` (report/mod.rs:450–454), which is measured from the run; in my deleted-shape world it moved 5 → 4. The two conjuncts differ exactly in the way N1 turns on.
2. **"The code-side `GEOMETRY_CODE_TABLE_V1` might be checked only against the gate file, so file and code could move together."** False: the derivation tests in `vice-fit` red independently of the file (attack above, 2 failures), and the derivation lives in `vice-bench` against the universe — three locks, not one.
3. **"The cut-invariance test's leg two might pass vacuously if `models_at_cut` secretly re-cuts."** False by the test's own construction: leg one's `> 1.0`-bit spread assertion fails in that world.
4. **"The author's ladder might again carry a shape literal (F-0086)."** I ran my own instrument — every `## ` heading between the `# 28.` and `# 29.` boundaries, no name predicate: 16 headings, `M0 M1 M2 M3 M3.5 M4 M4.5 M5 M6 M7 M8 P1 M9 M10 M11 M12`; positive control: `P1` present, `M6.5` absent. A3.1's list is exact.

## 5. The dispatch's six claims, judged

1. **"Gate evaluated for the first time, 3 of 4 green."** Reproduced. With the stated boundaries: clause 1 is green over **8 corpus nodes** (the author names the thinness himself, limitation 69) plus 9 synthetic, with a real positive control; clause 2 is six of six, with the scale totals honestly non-comparable (limitation 71, reproduced: 142.598/220.217/298.102 bits); clause 4 is a differential with a run knockout, and no `k log n` exists in `vice-fit` (checked by grep as well). All three verdicts are test-held, not row-held (M6A-N5).
2. **"Clause 3 NOT MET — legitimate split or a gate un-taken by construction?"** Judged: **legitimate, with one governance defect.** The M4.5 standard is "a freeze is as legitimate as its prices are honest". The prices here are honest in *amount*: C309 deletes the three delivered capabilities rather than renaming them (verified: enum id set is exactly `{geometry_pipeline_arm, oracle_injection}` plus the M4.5 one, asserted whole by the ladder test), names the true blocker, and the author does not certify — so the gate is not dodged, it is failed out loud, which is what §36 asks for. The defect is M6A-N4: the blocking capability's owner moved silently, and it is now a **second** deferral of the same named price. The split is legitimate only as long as M6 remains uncertified and M7 carries the chain identity as a first-class obligation.
3. **"Limitation 58 closed; 23.87×; 333 of 14659 refused, not saturated."** Confirmed by execution, digit for digit. The refusal path holds M5's discipline: a normal-line miss is `CostRefusal::NormalLineMisses` carrying the sample index — a typed refusal, counted per family (`no_costs`), never a saturation value, never a zero; `normal_deviation` returns `Option` with an exact-zero determinant test and no epsilon; the miss test carries a positive control on the same polyline. The F-0085 ratio guard (`MATERIAL_DEVIATION_FRACTION`) guards a *reported ratio*, not the cost — the cost integrates every sample with no threshold. Verified in code and by the corpus run.
4. **"§1.5 discipline."** Confirmed: C306 touches only `universe/` (275/338-line refactor+tests, no other file); C305 (generator) precedes it; the recalibration debt is written on `FROZEN_V1_HASH` itself ("M7 must calibrate against `e9e7f7e6…`, not `fed2af86…`"), with both search-mass bounds asserted `Unknown` two tests above; the §27.7 interleaving C305→C306→C307→C308→C309→C310 is real (verified by per-commit file footprints: C289/C291/C308 touch only the gate file; C284/C288/C310 touch only `docs/gt`).
5. **"Four defects against himself, closed by execution."** Three of four closures are held by instruments that redden under mutation; the fourth (F-0088) is closed in code and held by nothing — M6A-N3.
6. **"F-0048 non-passing rows, honest prices."** Verified. `GeometryCodeTable`: the literal is three numbers in the gate file, the bypass is a §27.7 commit a reviewer reads — and my attacks confirm no cheaper bypass exists. `RelationKind`: the both-directions judge is real and RED on an admissible family without a generator; the residual the author does not name is that the test's `generated` list is itself a literal enumerating the enum — a fifth variant with a generator, never admitted, is invisible to it until admitted; this is inside the Q1 verdict the author already gives the row, so the price is honest at its stated grain. `MissingCapability`: the deletion discipline is real (C309) and the ladder test asserts the id set whole.

## 6. F-0048 over this review's own instruments

The byte-compare of the reproduced artifact used `SHA-256`, independent of the harness under test. The ladder instrument used section boundaries, not name predicates, with a positive control on the exact element that broke F-0086's predicate. Every mutation attack had its positive control in the same run: the same suite green at HEAD immediately before and after (clean-tree checks between attacks). The one instrument of mine with a literal is the list of attack sites itself — six chosen transformations; what it cannot see is stated: I did not attack the M4/M4.5 harnesses (their artifacts are quoted, not re-run, exactly as STATUS_M6 A3.10 declares), and I did not run CI (T11/T12 remain "closed in code, not executed" — unchanged, and the author says so).

## 7. What this review could not verify

- **CI.** Not run here either; the workflow file was not executed by anyone yet.
- **Cross-platform.** All numbers on `windows-x86_64`, the artifact's own recording platform; `dcel-check` passed in full projection here, which says nothing about ubuntu (A7.1, owner M12, unchanged).
- **The full degradation matrix.** Like the author, I measured the corpus at one cell per scene; whether the 8 smooth joins are representative is unmeasured (limitation 69's price stands).
- **The "one door, three locks" cross-reference** (backlog 48/49/50) — untouched by M6, still a reading of two documents; not re-measured here.

## 8. Conditions

Numbered locally (M6A-C*) — I do not continue reviewer A's M5 sequence, whose last number I decline to inherit without measuring it.

1. **M6A-C1** (from N1): `transaction_shapes` must be **derived from the run** (e.g., the count of shape functions that produced ≥1 attempted transaction), not typed in; the deleted-shape world executed above must make the clause-3 row NOT MET before the [dcel_compound] section's own comment is true. Limitation 54's `ShapeKnockout` is *additional* to this, not a substitute — a knockout of a measurement is worth something, a knockout of a literal is not.
2. **M6A-C2** (from N2): correct "4 distinct" to the measured five in `[dcel_compound]`'s comment (a gate-file-only commit under §27.7) and in STATUS_M6 A1.3/A1.4 (an erratum, not an edit); record the fourth occurrence of the comment-drift class in the ledger — it strengthens, not weakens, limitation 56's case and its M7 price.
3. **M6A-C3** (from N3): a witness test for `FitRefusal::NonUnitNormal`, two legs, before F-0088 is cited as closed anywhere again.
4. **M6A-C4** (from N4): the reassignment of limitation 57's owner recorded explicitly, and the chain identity carried into M7's obligations with "second deferral" named on it.
5. **M6A-C5** (from N5): before any certification of M6, the clause-deciding constants (1e-9 rad, 0.06, 1.0 bit, 1e-6 bits) are registered under §27.7 or exempted row by row with a written reason.
6. **M6A-C6**: a `REPRODUCIBILITY_M6.md` exists before certification; this review had to borrow M5's §1 and reconstruct the M6 commands from module docs.

## 9. Verdict

The delivered work is real and its numbers reproduce to the digit; the defect record is honest and, in three of four cases, mechanically enforced; the §27.7 and §1.5 discipline held under commit-level inspection; and the one place the author's own standard was violated (N1/N2 — a gate row standing on a literal, and a provenance comment false at freeze) is precisely the class his own ledger predicts, found here by the method his own milestone taught.

**VERDICT: ACCEPT WITH CONDITIONS** — the conditions above, M6A-C1 through M6A-C6, with C1–C3 blocking any future claim that the compound obligation or F-0088 is "closed".

**GATE §28 M6: NOT MET** — by clauses: **exact G1 after joint solve** — measured green (2.776e-15 rad over 8 corpus + 9 synthetic nodes, positive control 0.7136 rad through the same instrument), held by representation with the line-adjacent hole closed and its closure verified by mutation; **sample/cut/transform invariance** — six of six reproduced, duplicate-sample and cut closures verified by mutation and by the test's own two-leg construction; **oracle G00–G20 decomposition** — **NOT MET**, 0 of 5 G arms producible, refused with two named capabilities and owner M7, verified in `oracle::design` and its ladder test; **no BIC-only promotion** — differential green with the cheap-table knockout run in both directions. A gate is a conjunction; one clause refused makes the gate NOT MET, and the author has said the same. The three green clauses are, additionally, test-held rather than §27.7-registered (M6A-N5), so even a future green on clause 3 does not certify M6 until the registration debt is paid.

## 10. Hygiene, at the end

Clone `revA-M6-20260729-121402-6625deff`: `git status --porcelain` empty, HEAD `a65f1f7c15b79cc683fc638aa247f2038377d424`; every mutation reverted and re-verified clean between attacks. Main tree `C:\Users\nirrt\Toolset\vice-classic`: untouched by any write, `git status --porcelain` empty at HEAD `a65f1f7` at review end. Spec SHA-256 re-verified in the clone at review end: `652fd0b6…9bb1`, matches. All cargo work confined to `…\scratchpad\revA-M6-target`. Donor sources not opened.

*Signed: Reviewer A, independent cold review, M6.*
