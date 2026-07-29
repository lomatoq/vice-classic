# STATUS_M6 — Typed grammar + k-best DP + joint G1 + explicit MDL

**Author context, M6. The author does NOT self-certify.**

This document reports a milestone that is **not complete**. §28 M6's own six
bullets are **not started**. What is delivered is the obligation M6 inherited
and was forbidden to defer again, plus three defects found by measurement in
the course of delivering it. The reason for stopping where I stopped is in §7,
stated as a fact about this context rather than as a property of the work.

---

## 1. The ladder, measured before anything else

The instruction that dispatched me named M6, and F-0080 is the record of what
it costs to inherit such a name. So it was measured first, against
`VICE_CLASSIC_CORE_AGENT_SPEC_v1.3.md`, SHA-256 verified as the first action:

```
652fd0b6e17c96c38af0173ddcc93a3921eafd60a9aff34c8d848829228d9bb1   (matches)
```

§28's headings, read whole:

```
M0  M1  M2  M3  M3.5  M4  M4.5  M5  M6  M7  M8  P1  M9  M10  M11  M12
```

- **M6 EXISTS** — §28 line 1983, "Typed grammar + k-best DP + joint G1 +
  explicit MDL", six bullets and a `Gate:` line.
- **Its predecessor is M5.**
- **Its successor is M7** — "Exact posterior refinement + selective delivery +
  export materialization". Read from §28, not from the dispatch.

**Positive control**, because F-0080's rule requires one and it costs a line:
the same instrument, counting `^## <name> ` headings in the same file, returns
`M6` 1, `M7` 1, `M4.5` 1 — and `M5.5` 0, `M6.5` 0. An instrument that returned
zero for everything would have "confirmed" the ladder equally well.

## 2. §34, for M6 specifically

§34's text, read directly:

> Review выполняется human или независимым agent context, который … подписывает
> `docs/REVIEW_<M>.md` либо возвращает blockers.
>
> Даже зелёный author gate не даёт агенту право самостоятельно продолжать.
> **M2, M5 и M7** требуют отдельного numerical/topology red-team pass.

So for M6, **§34 requires ONE signature**: an independent review signing
`docs/REVIEW_M6.md`. **§34 does not require a red-team pass for M6** — the
list is M2, M5, M7, and M6 is not in it.

This differs from M5, and the difference should not be inherited silently.
M5 carried three signatures, and `docs/REVIEW_M5_A.md:5` shows where the third
and the second came from: the **governor** required "два независимых cold
review разных модельных семейств плюс отдельный red-team pass", recorded in
this repository as T15. That is a governor standard, not §34's text. §34 alone
gives M6 one. **The gate is the governor's to declare, and if the governor
wants T15's standard for M6 the place to say so is the dispatch, not this
document.** I report what §34 says because I was asked to measure it.

## 3. What was delivered

### 3.1 §28 M5's undelivered bullet: local COMPOUND topology transactions

This was the one carried obligation both M5 reviewers marked as having no
second deferral available (REVIEW_M5_A §A4 "it must not be deferred twice";
REVIEW_M5_B at six separate places, "второй отсрочки нет"). It is delivered,
and delivering it produced the milestone's most important finding.

**The type.** `EditKind` is a point of Z^2 (`crates/vice-topology/src/edit.rs`)
rather than a four-variant enum. F-0048 Q2 asks what the next finding costs: a
fifth variant answers "append a line", a delta answers "the criterion already
covers it". The four unit steps survive as CONSTANTS whose names are the exact
strings the signed artifacts already carry, so no recorded byte moved on their
account.

**The executor.** `apply` adds the declared delta to the base signature in
signed arithmetic and requires the sum to equal the candidate's. Every
multi-step edit is expressible; the four unit steps evaluate exactly as before.

**The provenance.** The declaration used to come from `Dcel::assemble` and was
checked against `Dcel::assemble` — F-0048 Q4, a guard sharing its origin with
the mechanism, so a defect inside `assemble` moved both sides together. It now
comes from `topology::independent::signature_of` (breadth-first flood fill plus
a bit-quad Euler count), which shares no code with the DCEL.

**The population.** See §4.1: widening the type was necessary and **not
sufficient**, and the number that said so is published.

**Measured, full scope, exit 0, all four §28 M5 clauses MET:**

| quantity | M5 (`c4fc903`) | M6 |
|---|---|---|
| edit shapes per arm | 1 | **2** |
| transactions attempted | 170 | **960** |
| transactions committed | 170 | **678** |
| rolled back | 0 | 282 |
| **compound transactions** | **0** (not attempted) | **172** |
| **compound committed** | — | **172** |
| max declared steps | 1 | **4** |
| arms excluded as "compound" | 310 | **0** (the key is gone) |
| unrelated chains | 130 | 556, **0 moved** |
| named steps exercised | 2 of 4 | **3 of 4** |

Compound deltas exercised: `(+1,+1)` 118, `(-1,+1)` 46, `(0,+2)` 6,
`(-3,+1)` 2.

### 3.2 Three defects, each found by a mechanism rather than by re-reading

Recorded as **F-0081**, **F-0082**, and the gate-file finding in §4.3.

### 3.3 Smaller closures, taken because they were in the way

- `ArmTransaction::components_before/after` are `Option` rather than
  `.unwrap_or(0)`. Zero is a real component count, so the error path was
  writing a value that elsewhere means the subject was absent — **F-0075, live
  in this struct**, in the milestone that recorded F-0075.
- `refusals_observed` / `refusals_never_observed` are MEASURED. They were two
  hand-written lists with a prose argument for why four of six refusals could
  not fire; two of those four became reachable the moment the filter came out.
  An argument that must be re-derived whenever the harness moves is not
  evidence.
- `TransactionRefusal::name()` is an exhaustive match — the compiler is the
  judge — with `every_refusal_variant_is_in_all_names` constructing one of each
  variant so `ALL_NAMES` cannot silently shrink a report's denominator.
- `continuation::edit_kind` keeps the unit-step restriction, at the call site
  that means ADJACENCY. `TOPOLOGY_M4_5.json` therefore does not move, and
  limitation 32 is untouched and still owned by M6.

## 4. The three defects my own mechanisms found against me

### 4.1 F-0081 — a subclass name that was arithmetically defensible and materially false

Removing the filter, the first run published `transactions_compound = 0` and
`max_declared_steps = 1`. **The 310 arms reported for M5 as excluded-as-compound
are not compound.** All 310 declare the identity delta `(0,0)`, and 282 of them
change no pixel at all — the centred square is already inside the foreground.

The predicate was described correctly: `(0,0)` is not a single `±1`. The NAME
was wrong, and the author, both reviewers, the red team and the dispatch all
read `..._as_compound` as naming the subclass §28 M5 calls compound. Nobody
printed what was in the 310.

The cause is STATUS_M5 limitation 34: **one transaction shape**. A filled
square can only ever yield `(0,0)`, `(-1,0)` or `(+1,0)`. The compound
population came from a second edit shape — a square annulus, compound by
construction on a background centre — not from the type.

**Had `transactions_compound` not been published, this report would have said
"compound transactions delivered, 480 attempted, 0 excluded", and it would have
been false.** The number that caught it is the one F-0039 and F-0059 require.

### 4.2 F-0082 — a denominator and a numerator from different populations

The second shape doubled the transactions. Every clause-3 counter was moved to
a single `all` list except `transactions_attempted`, and the run printed
`480 attempted, 678 committed`. Visible only because the two print on one line;
in M5 they did not.

### 4.3 The frozen gate file's provenance comment, stale by two deltas

`[dcel]`'s comment claims the population its floors were read from. Measured
against `docs/gt/DCEL_M5.json` at `c4fc903`, **six of its eight numbers were
false** (474/30/237/10/167/127/28 against 480/36/240/13/170/130/30). Only
"444 corpus" survived. The population moved at C261 (delta-3); the file was
last edited at C269 (delta-5) and the comment did not follow.

**This is the third occurrence in that one comment block.** RT5-A4 and
REVIEW_M5_A N8b caught it when the text said 159 and the artifact said 167; the
block's own WAS/IS note records that correction; and at the very next delta the
same number drifted again, 167 → 170. Nothing caught it because it is a
COMMENT: `the_full_scope_population_is_what_the_thresholds_were_read_from`
PRINTS the population and asserts none of it, and
`every_frozen_value_agrees_with_the_code_that_uses_it` compares keys against
consumers, not prose against a run. **That is limitation 36's class, in the
file §27.7 exists to keep reviewable.** Corrected in C283; the mechanism that
would prevent a fourth occurrence is not built, and is priced in §6.

### 4.4 A fourth, against the dispatch rather than against the tree

The dispatch told me reviewer A had verified that "`EditKind` не встречается ни
в одном подписанном артефакте". Reviewer A's actual sentence
(`docs/REVIEW_M5_A.md:683`) is scoped to **one file**:

> `EditKind`'s variant names are *not* carried in `docs/gt/TOPOLOGY_M4_5.json`
> (checked: zero occurrences …)

I re-measured both. In `TOPOLOGY_M4_5.json`: **0 occurrences**, reviewer A is
right. In `docs/gt/DCEL_M5.json`: **`bridge_close` and `gap_open` are present**,
through `TransactionReport::declared`. The reviewer measured one file and said
so precisely; the restatement dropped the scope and became a universal.

The conclusion survives — adding to the type moved no existing value, and the
artifact had to be re-recorded anyway because 310 arms gained transactions —
but the stated REASON was false, and a price accepted for a false reason is not
an accepted price. This is F-0049 and F-0080 again: **a coordinator's
restatement is a claim, and it is measured, not inherited.**

## 5. What is NOT delivered — §28 M6 itself

§28 M6's six bullets and its gate are **NOT STARTED**. No `vice-fit` crate
exists. Nothing here is a placeholder for one: §4.1 forbids creating the crate
before it has a real executable responsibility, and §32 rule 7 forbids
placeholder APIs, so what is missing is recorded as data with a price.

| §28 M6 bullet | status | price |
|---|---|---|
| hierarchical span candidates | **NOT STARTED** | `vice-fit` crate (§4 lists it: "line/arc/quad/cubic/primitive candidates + global DP/MDL"), created in this milestone under §4.1; resampling, breakpoint schedule, corner proposals (§14.1–14.2) |
| candidate-generation budgets | **NOT STARTED** | §14.2's hard cap and profile; the O(N²) ban needs a measured candidate count per chain |
| k-best jet-compatible grammar paths | **NOT STARTED** | the DAG over (breakpoint, family, corner state, tangent jet class) and k-shortest paths (§14.3, §24) |
| joint constrained chain refit | **NOT STARTED** | shared node positions and shared tangent variables optimised together (§24) |
| explicit code lengths | **NOT STARTED** | §14.5's `L_total` in physical bits, and it has a named home already: `configs/GATES_V1.toml [geometry_code_table]` is `status = "placeholder"`, `set_by_milestone = "M6"`, with `bits_per_anchor`, `bits_per_segment_family`, `bits_per_relation` all `0.0` |
| primitive/relation hypotheses | **NOT STARTED** | §15 Stage H |
| **Gate**: exact G1 after joint solve; sample/cut/transform invariance; oracle G00–G20; no BIC-only promotion | **NOT STARTED** | the G00–G20 decomposition is a `vice-bench` harness over the corpus (§27); the invariance set is §14.5's six (0.25/0.5/1.0 px samples, duplicate samples, cyclic cut, uniform scale, translation, reflection) |

**The design decision I did reach, recorded because it is the load-bearing one
and a successor should not have to re-derive it:** §14.3 says
`angle < tolerance` is not G1. The honest closure is ADR-0031's idiom —
invariants held by REPRESENTATION. If a chain's segments read their endpoint
tangents from a SHARED node variable rather than storing one each, then G1 at a
smooth join is a property of the type and a corner is the deliberate absence of
sharing. That makes "exact G1 after joint solve" a claim the compiler enforces,
with the residual being the numeric derivation from node tangent to control
points — which needs a witness and a knockout, not a tolerance. This is
untested and unwritten, and is marked as such.

## 6. Open items, counted rather than inherited

Thirteen items entered M6 owned by M6. **Two are closed, eleven are carried**,
and every carried one has a price.

| # | item | verdict | price and owner |
|---|---|---|---|
| **37 / 44** | compound transactions, "no second deferral" | **CLOSED** | delivered; 172 compound, all committed |
| **F-0075 in `ArmTransaction`** | error path writing 0 | **CLOSED** | `Option` |
| **36** | M5 gate rows bound positionally | **CARRIED** | unchanged, and its price ROSE again: §4.3 is its third live instance. Full closure is a `dcel:` key prefix, `DCEL_M5.json` in `artifact_file`, a row-kind table in `REPRODUCIBILITY_M5.md`, the declaring-document path derived, and every gate row bound position-by-position to an artifact key. **Owner M6 — still open, and I did not close it** |
| **48 / 49 / 50** | leaf-judge TYPE axis, serde scan, branch-label scan | **CARRIED** | proc-macro, new crate under §4.1. See the note below |
| **51** | register's predicate blindness to `degree` | **CARRIED** | owner M6 |
| **52** | face numbering and loop order | **CARRIED** | owner M6 |
| **53** | two declarations of the size list | **CARRIED** | a `const` moves from a test file into `vice-topology` beside `structural_fixtures`, plus one assertion in `vice-bench` that the cell sizes agree with it. Reviewer A priced it at five lines and I did not spend them |
| **32** | `continuation.rs` says the DCEL is absent | **CARRIED** | one commit re-freezing `TOPOLOGY_M4_5.json` with the reclassification. Untouched: `edit_kind`'s predicate is unchanged, so the artifact did not move |
| **M4-N8** | `BoundaryRefusal::Malformed` unreachable by type | **CARRIED** | rewiring + re-freezing `clean_bucket_sigma_codes` + re-recording `CORRIDOR_M4.json` + re-measuring the M4 rows |
| **F-6 / F-7 / F-8** | magnitude as a word; 33 M4.5-era attacks not in the tree; the M4.5 judge's 256/512 gap | **CARRIED** | as priced in STATUS_M5 §4 |
| **new** | `gate_min_compound_transactions` | **OPEN** | without it a harness that lost the annulus shape drops `transactions_compound` to zero with every row MET. Three commits under §27.7: key as placeholder, consumer, freeze |
| **new** | a mechanism binding the gate file's provenance comment to the run | **OPEN** | §4.3 is the third occurrence; the fourth is a matter of time |
| **new** | `hole_fill` still never occurs | **OPEN** | one of four named steps unexercised on the corpus; a third edit shape, or an arm-derived edit |

**On the "one door under three locks" (REDTEAM_M5:1205).** The red team's
instruction to M6 was that when M6 pays for the proc-macro it should check
whether it closes the serde scan (49), the branch-label judge (50) and
`OUTSIDE_PARTS` **at once**. I did not pay for it, so I did not answer it by
construction — but I did check the cross-reference the red team asked for, and
it does not hold: **reviewer A already recorded at `docs/REVIEW_M5_A.md:1437`
that the proc-macro closing the TYPE axis would NOT close the `Dcel`-field
axis** that `OUTSIDE_PARTS` guards. The three locks are on the same door and
at least one needs a different key. Whoever pays should budget for two
mechanisms, not one. **This is a reading of two documents, not a measurement.**

## 7. Why this milestone stops here

§28 M6 is a stage-scale milestone: Stage G (§14) and Stage H (§15), a new
crate, a k-best DP, a joint constrained optimiser, an explicit code table in
physical bits, six invariance properties and a five-arm oracle decomposition.
M5 — a smaller scope — cost seven deltas, six consecutive red-team FAILs and
three signatures.

I delivered the inherited obligation because it was mandatory and because the
project's rules rank it first, and I stopped rather than open Stage G with the
budget left, because the alternative was a partially-built typed grammar whose
gate row would claim exact G1 without the mechanism to hold it. §38 names that
outcome specifically ("не trace-and-smooth demo"), and §36 says a blocker
report beats a new fallback layer. **A milestone reported as delivered with a
grammar that cannot meet its own gate would be worth less than this document.**

What a successor context needs in order to start Stage G on this tree: the
ladder is confirmed, §34's requirement for M6 is one signature, the compound
obligation is discharged and will not need re-opening, the chain decomposition
M6 consumes is anchored in both directions (reviewer A's conditions 62–63,
closed in M5 delta-6), and §5's G1 design decision is recorded in §5 above.

## 8. Explicit statement of stopping

The author does NOT self-certify M6.

No number in this document is a statement about the reliability of the system.
The four `[MET]` clauses are M5's, re-measured on a population 5.6× larger in
transactions, and they say what they said before: that an arrangement is what
its labelling says, that a transaction touches what it declared, and that the
instrument saying so has demonstrated resolving power. **§28 M6's gate has not
been evaluated, because none of the code it evaluates exists.**

Three of my own errors are in the ledger (F-0081, F-0082, and the gate-comment
finding). All three were found by measurement rather than by re-reading a
formulation, and the first was found by a number I published because F-0039
requires it — which is the only reason this report does not contain a false
claim about compound transactions.

**Verification of the fourth claim I could not make:** I did not run CI. T11/T12
remain "closed in code, not executed", unchanged since M5. I did not re-run the
M4.5 or M4 harnesses, so their artifacts are untouched and their numbers here
are quoted from their own records rather than re-measured.

**STOPPED AFTER M6 — the successor under §28 is M7, and M7 IS NOT STARTED.**
