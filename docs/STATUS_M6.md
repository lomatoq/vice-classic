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

---

# Addendum 1 — continuation (C287–C292)

The governor confirmed the findings of §4.2 and §4.4 as F-0083, pushed
C282–C285, and directed this same context to continue M6. This is what the
continuation did. **§28 M6's six bullets are still NOT STARTED**, and §7's
reason is unchanged.

## A1.1 The ladder, re-measured

Not inherited from the body above: a STOP line is a claim about the ladder
(F-0080) and this addendum carries one. Same file, hash re-verified
`652fd0b6…9bb1`:

```
§28 ladder .......... M0 M1 M2 M3 M3.5 M4 M4.5 M5 M6 M7 M8 M9 M10 M11 M12
predecessor of M6 ... M5
successor of M6 ..... M7
positive control .... "M6.5" 0, "M7.5" 0, "M5.5" 0, against M6 and M7 present
```

## A1.2 `hole_fill` — the answer was a third thing

The governor asked whether `hole_fill` is unreachable by construction or
whether the population does not carry it, and said the first must be proved
rather than assumed. **Both were false.** Measured on the artifact at
`f559767`:

```
arms whose base arrangement carries a hole ....... 72 of 480
declarations with a negative hole component ...... 0 of 960
```

The corpus carries holes in quantity, and `apply` fills them — a unit test
commits a `HOLE_FILL`. What could not reach them was the **shape**: both
existing shapes sit at the canvas centre, and a hole is wherever the scene put
it. The deficiency was in the shape family, not the corpus and not the
executor.

A third shape — fill the lexicographically first hole, flooded under the
COMPLEMENT connectivity — closed it. **All four named unit steps now occur**,
where M5 had two:

```
identity 310, hole_open 308, gap_open 128, compound(c+1,h+1) 118,
hole_fill 66, compound(c-1,h+1) 46, bridge_close 42,
compound(c+0,h+2) 6, compound(c-1,h-1) 6, compound(c-3,h+1) 2
```

`66 + 6 = 72`, exactly the population, so no arm carrying a hole was dropped.
The 6 are `compound(c-1,h-1)`: arms where filling the hole also merges two
components, so the fill is itself compound there.

**The third shape is arrangement-derived and the first two are not**, and that
is stated rather than smuggled. Limitation 34 warns against a SEARCH over
edits; this is not one. The hole is chosen by scan order, the shape is defined
for every arm that has one, its population is published, and its delta is
declared from the independent chain and checked by `apply` like the others.
What would be forbidden is trying several edits and keeping whichever commits.

## A1.3 The compound floor — four steps, not three

The governor asked for the floor as a separate gate commit. It took four, and
the reason is a mechanism refusing me:

```
gate dcel_compound.gate_min_compound_transactions: section "dcel_compound"
is a PLACEHOLDER (set by M6): it is not a threshold and nothing may gate on it
```

`Threshold::from_gates` will not mint a threshold from a placeholder, so the
consumer cannot precede the freeze; and the freeze cannot precede a claim, or
the keys are unclaimed. The only order with no red commit is C289 (keys, gate
file) → C290 (constants and claims, inert, code) → C291 (freeze, gate file) →
C292 (consumer and row, code).

**Three keys, not one**, because one is bypassable:

| key | frozen | measured | why it exists |
|---|---|---|---|
| `gate_min_compound_transactions` | 100 | 178 | the count |
| `gate_min_distinct_compound_deltas` | 3 | 4 | a count floor is met by ONE delta repeated — 118 copies of `(+1,+1)` clear 100 |
| `gate_min_transaction_shapes` | 3 | 3 | the count is an EFFECT; losing a shape is the cheapest way to empty the population |

The shape floor is set AT the measurement, and the exception is deliberate and
priced: a floor of two would permit exactly the regression it guards. It now
moves whenever the shape set does, which is a gate-file commit — the same
reasoning that raised `gate_min_register_arms_with_a_long_loop` to six
(REVIEW_M5_A D4-N3).

`each_compound_floor_has_a_world_in_which_it_is_false` exercises the three
**separately**, with the count held constant while the deltas collapse, because
three conjuncts that only fail together are one conjunct wearing three names.

## A1.4 Final measurement

Full scope, exit 0, four §28 M5 clauses MET:

```
arms 480 (444 corpus, 36 structural)      edit shapes 3
transactions 1032 attempted, 750 committed, 282 rolled back
compound 178, ALL 178 committed, 4 distinct deltas, max 4 steps
named unit steps exercised 4 of 4
unrelated chains 610, moved 0     slots perturbed 179 253, uncaught 0
refusals observed: EditIsANoOp 282
```

## A1.5 New limitations

54. **The compound knockout mutates the REPORT, not the harness.** It proves
    the row responds to an emptied population, not that the harness would
    notice a deleted shape function. **Owner M7**, price: one `ShapeKnockout`
    variant plus the arm threading it through `measure_arm`, and re-deriving
    `RunKnockouts::one_per_clause`'s one-knockout-per-row invariant — which is
    what stopped me spending it here.

55. **Five of six refusals never fire on this population.** Only `EditIsANoOp`
    does, 282 times. `NotTheDeclaredEdit` never firing now MEANS something —
    the independent chain and the DCEL agree on all 1032 — but the branch is
    exercised only by a unit test. **Owner M7.**

56. **The gate file's provenance comment is still bound to the run by nothing.**
    C283 corrected six false numbers; §4.3 records that this was the third
    occurrence in that block. **The mechanism is not built**, and it is the
    governor's item 2. **Owner M7**, price: the printing test asserts instead
    of printing, which requires the comment's numbers to become a parsed
    structure — that is limitation 36's machinery, not a separate thing.

## A1.6 F-0048 over the mechanisms this addendum added

| mechanism | Q1 literal | Q2 next finding | Q3 judge | Q4 provenance, by CODE | Q5 both ways | verdict |
|---|---|---|---|---|---|---|
| the three edit shapes | **YES** — three functions named at the call site | a fourth class needs a fourth shape | the declared/performed comparison, not the shape | no — declaration from `independent::signature_of`, check from `Dcel::assemble` | red per conjunct; empty covered by the floors | **DOES NOT PASS Q1.** Residual exact: the shape SET is a literal. Bypass price: one class of edit nobody wrote a shape for |
| `first_hole` | no | criterion changes | flood fill under the complement connectivity | no — reads the labelling, not the DCEL | 72 found; none on arms without holes | **PASSES** |
| the three compound floors | no — read from the frozen file | the test fails until the gate file moves | `Threshold`, which has no arithmetic | no — floors from the file, counts from the run | each reddens separately, count held constant for the delta leg | **PASSES** |
| `refusals_never_observed` | no — complement of `ALL_NAMES` | a new variant appears without a line | exhaustive `match` plus a constructing test | no | observed set is measured | **PASSES** |
| `EditKind` / `UNIT_STEPS` | **YES**, and stated | the criterion already covers Z^2 | computed predicate, swept over a box both ways | no | 4 named = 4 computed, exhaustively | **PASSES with the literal named** |

**The row that does not pass is the shape set**, and it is F-0081's shape one
level up: a literal enumerating subjects, whose bypass is a class of edit
nobody wrote a shape for. What is different from M5 is that the bypass is now
VISIBLE — `declared_kinds_exercised` publishes every delta that occurred, so an
absent class shows as an absent name rather than as silence. That is weaker
than a closure, and it is named at its true price rather than above it.

## A1.7 What the governor asked for and did not get

- **Item 4, two mechanisms for the "one door".** Not paid. The finding stands
  as recorded in §6: `REVIEW_M5_A.md:1437` already says the TYPE-axis
  proc-macro will not close the `Dcel`-field axis, so budget two. Still a
  reading of two documents, not a measurement.
- **Item 5, backlog 36 / 48 / 49 / 50 / 51 / 52 / 53.** None closed. 36's price
  rose again (§4.3, limitation 56). 53 remains the cheapest at five lines and I
  did not spend them.
- **§28 M6's six bullets.** Not started, for §7's reason, which this pass did
  not change.

**STOPPED AFTER M6 — the successor under §28 is M7, and M7 IS NOT STARTED.**

---

# Addendum 2 — §28 M6 bullets 1 and 2 (C294–C299)

A fresh author context. The governor directed §28 M6 and forbade M7 and
beyond. **§28 M6's six bullets were NOT STARTED when this pass began**; two of
them are started and finished now, four are not, and the gate has not been
evaluated because none of the code it evaluates exists.

## A2.1 The ladder, measured, with a positive control

Not inherited from the body above or from addendum 1 — a STOP line is a claim
about the ladder and F-0080 is the record of what inheriting one costs. Spec
hash re-verified as the first action of the pass:

```
652fd0b6e17c96c38af0173ddcc93a3921eafd60a9aff34c8d848829228d9bb1   (matches)
```

Headings inside §28 (lines 1895–2045), read whole:

```
M0 M1 M2 M3 M3.5 M4 M4.5 M5 M6 M7 M8 P1 M9 M10 M11 M12
```

- **M6 EXISTS** — "Typed grammar + k-best DP + joint G1 + explicit MDL".
- **Predecessor: M5.** **Successor: M7.**

**Positive control**, same instrument, same line range: `M5` 1, `M6` 1, `M7` 1,
`M3.5` 1, `M4.5` 1, `P1` 1 — against `M5.5` 0, `M6.5` 0, `M7.5` 0. An
instrument returning zero for everything would have "confirmed" the ladder
equally well.

**One correction to addendum 1**, which is why this was re-run rather than
copied: A1.1's ladder line reads `… M8 M9 M10 …` and omits **P1**, which is a
§28 heading ("Partition correction API/editor", after M8). It changes nothing
about M6's successor, and it is the same class of defect F-0080 records — a
ladder transcribed rather than counted.

## A2.2 §34 for M6, re-read

Unchanged from §2 above and re-measured: §34 line 2254 requires a signature on
`docs/REVIEW_<M>.md`; line 2256 names **M2, M5 and M7** for a separate
red-team pass, and M6 is not among them. So §34 requires **one** independent
review for M6. The governor has declared a stricter standard for this
milestone — two cold reviews of different model families plus a red team — and
named it as the governor's own rather than the spec's. **The gate is the
governor's to declare; this document does not self-certify.**

## A2.3 What was delivered

### Scope, chosen from §28 M6 and stated as a choice

§28 M6 is Stage G (§14) plus Stage H (§15). Bullets 1 and 2 are delivered
whole. Bullets 3–6 are not started. The reason is §7's, unchanged, and the
alternative — a partially built grammar whose gate row claims exact G1 — is
what §38 names and §36 forbids.

| §28 M6 bullet | status |
|---|---|
| hierarchical span candidates | **DELIVERED** |
| candidate-generation budgets | **DELIVERED** |
| k-best jet-compatible grammar paths | **NOT STARTED** |
| joint constrained chain refit | **NOT STARTED** |
| explicit code lengths | **NOT STARTED** |
| primitive/relation hypotheses | **NOT STARTED** |
| **Gate** (exact G1 after joint solve; sample/cut/transform invariance; oracle G00–G20; no BIC-only promotion) | **NOT EVALUATED** — none of the four is evaluable while bullets 3–6 do not exist |

### The crate

`vice-fit` exists (C296), created in the milestone that gives it its first
real executable responsibility, per §4.1 and §32 rule 7. It consumes
`vice_evidence::BoundaryChain` — the Stage F observation of §13, already
resampled by physical arclength — and offers, per chain, one fitted primitive
of each family on each interval of a hierarchical schedule, with a proposal
cost (§14.4) to order them.

### The design decision that is the whole of bullets 1 and 2

§14.2 forbids full O(N²) all-pairs AND forbids losing long line/arc support to
`max_candidate_support_px`. The obvious reconciliation — cap the LENGTH of a
support — destroys exactly what the second sentence protects. **There is
deliberately no length cap in this crate.** The budget is on the COUNT, and
the count is bounded by a dyadic schedule that is sparse at coarse scales
rather than short. The whole run is **level 0 of the recursion**, not a
special case appended to satisfy the sentence: F-0048 Q2's distinction, where
"what about scale k" is answered by "it is level log2(k)" rather than by
another line.

### Measured

Synthetic sweep (`vice_fit::schedule`), both ratios printed so the claim is
falsifiable by reading rather than by trusting the bound:

```
n      4   supports/sample 0.250   all-pairs/sample     1.5
n    257   supports/sample 1.953   all-pairs/sample   128.0
n   4096   supports/sample 1.604   all-pairs/sample  2047.5
```

Corpus (`vice_bench::fit`, 1 cell per scene, release, §27.1 respected):

```
arms 41 (13 without a boundary)     sealed-audit groups skipped 22
chains 36                           longest chain 83 samples
chain samples 1910                  supports 3173 (1.661 per sample)
candidates 11157                    refusals none
families present: circular_arc, cubic_bezier, line, quadratic_bezier
min budget headroom 65018
```

Release timing, linear: 4097 samples to 8178 supports to 28 617 candidates in
412 ms.

### The G1 claim of §5, RUN rather than carried

§5 above records a design decision — segments reading endpoint tangents from a
shared node variable make exact G1 "a claim the compiler enforces" — and marks
it **not verified**. It is now run (C294), and it is **half true**:

- **Held.** The tangent is stored once, at the node. Neither segment carries a
  declared tangent of its own.
- **NOT held.** `Segment::Quad`/`Cubic` store absolute control points, and a
  Bezier's endpoint tangent is `ctrl - p0`. Declaration and geometry are two
  independent values, and nothing in the workspace compares them.

Measured on `vice-ir`'s own canonical VALID fixture, not on an adversarial
value I built: arrives −0.24498 rad (−14.04°), leaves 0.00000 rad (0°),
**declared +0.25000 rad (+14.32°)** — a spread of **28.36°** at a node whose
type is named `SmoothG1`, in a scene `valid_scenes_pass` has asserted valid
since M1. Positive control in the same file: `validate` does read the field
and rejects it outside `(−π, π]`, so "nothing rejected the inconsistent value"
is a fact about what the judge checks and not about whether it ran.

The claim in §5 therefore **does not become a declared property of this tree**.
It is narrowed to what was measured, and the closure §14.3 demands is not a
comparison but a representation in which the disagreement cannot be written
down. Owner: bullet 4.

## A2.4 Defects my own mechanisms found against me

Five, all by measurement, none by re-reading.

1. **The cubic missed a chain drawn from an exact cubic by 1.97 px.** Not the
   normal equations — the parameterisation. Chord length ESTIMATES the Bezier
   parameter and is not it. Bounded footpoint refinement, keeping the best
   pass by geometric residual, brings it to 0.56 px at eight passes
   (2.27 / 0.56 / 0.22 / 0.12 / 0.07 at 0 / 8 / 16 / 24 / 40 passes). The
   remaining floor is **named as a property of the candidate stage, not of the
   family**, and the test asserts it at its measured size rather than at a
   zero the code does not deliver.
2. **An assertion that was measuring the ruler.** The arc test required a
   deviation under 0.05 px and the run reported 0.0599. The fit was exact; the
   0.1 px chord tolerance of the flattener was showing through. The bound is
   now the flattener's own certificate, with a second leg asserting the
   certificate does not exceed the tolerance it was asked for — otherwise the
   first bound is vacuous.
3. **A doc comment said the schedule emits "about 1.4" supports per sample.**
   The sweep says 1.6, peaking at 1.953. F-0028's class, in prose, found by
   reading the instrument's own output.
4. **`max_normal_departure_deg` reported exactly 90.000°** — the maximum the
   range allows, which is the shape a SATURATED instrument has, not the shape
   of a measurement. The max was taken over all samples, including ones
   sitting on the curve whose closest-point direction is numerical noise.
   F-0030: the predicate was named for one set and computed over another, in
   an instrument built this milestone to keep me honest about an
   approximation.
5. **After restricting to material deviations it still reports 90.000°, now at
   a deviation of 1.50281 px.** Four times the clean-corridor median
   halfwidth, so finding 4's fix did not paper over finding 5 — it exposed it.
   This one is real: `proposal_cost_px` uses the EUCLIDEAN deviation where
   §14.4 writes `d_n` along the normal, and where the departure is a right
   angle the normal ray does not meet the curve near that sample at all, so
   §14.4's integrand there is unbounded rather than merely larger. The cost
   agrees with §14.4 where a candidate is close to the chain and diverges
   where the candidate is WRONG, **which flatters bad candidates.**

A sixth, against my own prose: the module doc said a fit needing an iteration
is "REFUSED rather than approximated". Finding 1 added an iteration and the
sentence stayed true-sounding and false (F-0015). Corrected in place.

Three of the workspace's own judges also caught this pass, and each was right
to: `every_crate_forbids_unsafe_code`, and both sealed-population lists, which
require every module able to see a corpus fixture or call the render pipeline
to be a REVIEWED entry rather than an assumed one. `vice-bench/src/fit.rs` is
declared in both with its reason, which is the act those tests exist to force.

## A2.5 F-0048 over the mechanisms this addendum added

| mechanism | Q1 literal | Q2 next finding | Q3 judge | Q4 provenance, by CODE | Q5 both ways | verdict |
|---|---|---|---|---|---|---|
| the dyadic schedule | no — spans are a recursion, not a list | the criterion covers every scale by construction | a sweep over chain lengths comparing against the all-pairs count | n/a — the schedule reads only `n` | bound asserted AND a floor on density, so returning nothing fails | **PASSES** |
| the candidate budget | no — one integer, and it REFUSES rather than truncating | a chain that binds it produces a refusal naming four numbers | `FitBudget`, minted only by a checked constructor, with no arithmetic | cap from the const, counts from the schedule | tested binding and not binding; headroom published | **PASSES** |
| `FITTED_FAMILIES` | **YES** | append a family | the `vice-bench` judge, which inverts the default to "must have a fitter" | universe names from `vice-bench`, fitters from `vice-fit` — neither derives the other | knockout RUN: emptying the excuse list reddens | **DOES NOT PASS Q1.** Residual exact. Bypass price: **one family nobody wrote a fitter for AND nobody declared** — now red, so the true price is one fitter or one written reason |
| `max_normal_departure_deg` | no | the threshold's meaning changes | the run itself, over the corpus | departure from the sample normal, deviation from the flattened candidate | saturated at 90° twice; both published | **PASSES, and it caught itself twice** |
| the family fits | no | a family needs its own solve | chains drawn from a known shape, each family required to FAIL on another's | fits from least squares, deviation from the certified flattener | each family checked against a shape it should not reproduce | **PASSES** |
| `STRUCTURAL_SIZES_PX` (limitation 53) | **YES** — one literal, reduced from two | the harness grows a cell and the judge reddens | an equality in `vice-bench` against the frozen matrix | declaration in `vice-topology`, cells in `vice-bench` | knockout RUN: `[32, 64]` fails with both lists printed | **DOES NOT PASS Q1**, and says so: the literal is reduced to one, not removed |

## A2.6 The backlog — what I took and what I left, explicitly

The governor asked for this decision to be made and stated rather than
inherited.

| # | decision |
|---|---|
| **53** | **TAKEN and CLOSED** (C295). Five lines, as reviewer A priced it, and I verified the price myself rather than inheriting it: the direction is forced because `vice-topology` does not depend on `vice-bench`. One declaration, one judge, knockout run |
| **36** | **NOT CLOSED, and NOT GROWN.** It is on my path — I emit numbers over a corpus population — so M-1 applies to the CLASS rather than to M5's rows: this pass adds **no gate row and no frozen key**, so it contributes no new positionally-bound row. Full closure is unchanged and unchanged in price. **Owner: still open** |
| **56** | **NOT CLOSED, and NOT GROWN.** Same reasoning: `configs/GATES_V1.toml` is **untouched** by this pass, so no new provenance comment was added to the block whose comment is bound to nothing |
| **48 / 49 / 50** | **LEFT.** Not on §28 M6's path: the proc-macro is about `vice-topology`'s leaf judges. The red team's "one door, three locks" warning stands, and the previous context's reading of `REVIEW_M5_A.md:1437` is unchanged — budget **two** mechanisms, not one. I did not re-measure it, so it remains a reading of two documents |
| **51 / 52** | **LEFT.** Both are `vice-topology` internals. Reviewer A owed 52 "before M6 walks chains" — and this pass **does not walk DCEL chains**: Stage G consumes `vice_evidence::BoundaryChain`, produced by marching squares over the coverage field, not `Boundary::path`. That is why 52 did not bind here, and it is also limitation 57 below |

## A2.7 New limitations

57. **The chains Stage G fits are not bound to the DCEL.** §13 step 4 requires
    a chain be tied to its DCEL endpoints and junctions; `BoundaryChain`
    carries no boundary or vertex id, and `vice-fit` invents none. So a
    candidate is a candidate for a curve on a CONTOUR OF THE COVERAGE FIELD,
    not yet for a boundary of the arrangement, and
    `curve_replacement_isotopy` still refuses with `fitted_curve` as its first
    missing capability. **Owner: the milestone that closes bullets 3–4**;
    price: a chain identity carried through `observe_boundaries` and matched
    against `Dcel::boundaries` — which is where limitation 52 (loop and face
    order) starts binding.
58. **The proposal cost is a LOWER BOUND on §14.4's, and the bound is loose.**
    Measured 90.000° departure at 1.503 px on the corpus. **Owner: bullet 3.**
    Price: deviation along the sample's normal ray, plus a decision about what
    a ray that misses the curve means — which is a statement about evidence
    and belongs with the stage that consumes the cost.
59. **The Bezier fits leave a 0.56 px parameterisation floor** on a chain drawn
    from an exact cubic. **Owner: bullet 4.** Price: the joint refit, or a
    Newton footpoint solve in place of the projection.
60. **The corpus measurement is one cell per scene**, 41 arms, and it is a
    COUNT, not a gate: no threshold is read and no clause is evaluated. A gate
    row over this population needs §28 M6's gate to be evaluable, which needs
    bullets 3–6. Price of the full-matrix population: `measure(matrix_v1()
    .len())` — the same code at more cells.
61. **The elliptic arc is admissible and unfitted**, by declared decision with
    a reason (§14.2's "targeted evidence"). RED if the reason is deleted.
62. **`FITTED_FAMILIES` and M5's edit-shape set are the same open class**: a
    literal enumerating subjects, whose bypass is visible rather than closed.

## A2.8 What §28 M6 still costs

| not delivered | price |
|---|---|
| k-best jet-compatible grammar paths | the DAG over (breakpoint, family, corner state, tangent jet class) and k-shortest paths (§14.3, §24). Corner proposals (§14.1) do not exist either — no `corner`, `jet` or `breakpoint` anywhere in the tree |
| joint constrained chain refit | shared node positions and shared tangent variables solved together. This is also the only honest closure for exact G1 (§14.3 forbids `angle < tolerance`), and C294 measured that the current types do not hold it |
| explicit code lengths | §14.5's `L_total` in physical bits. Home already named and still a placeholder: `[geometry_code_table]`, `set_by_milestone = "M6"`, three values `0.0`, and `Threshold::from_gates` refuses to mint a threshold from a placeholder. Four commits under §27.7 |
| primitive/relation hypotheses | §15 Stage H. **Verified price, not estimated**: the six relation families in `vice_bench::universe` are `Family::planned("M6", …)`, and promoting any to `admissible` moves `model_universe_hash`, which is frozen by a test and is a §1.5 model-version change requiring recalibration — not a routine edit |
| the gate | exact G1 after joint solve (needs bullet 4); sample/cut/transform invariance (the sample-step leg is measured for the PROPOSAL COST only; the cut and transform legs are about the SELECTION, and there is no selection); oracle G00–G20 (a `vice-bench` harness over §27); no BIC-only promotion (nothing promotes anything yet) |

## A2.9 What I could not verify

- **CI.** Not run. T11/T12 remain "closed in code, not executed", unchanged
  since M5.
- **The M4.5, M4 and M5 harness artifacts.** Untouched and not re-run; every
  number quoted from them here is quoted from their records rather than
  re-measured.
- **The "one door, three locks" cross-reference.** Still a reading of two
  documents, not a measurement. I did not pay for the proc-macro.
- **Whether the 90° departure is one candidate or many.** The run publishes the
  worst and the deviation at which it occurred; the DISTRIBUTION is not
  measured, and limitation 58's price does not include measuring it.

## A2.10 Explicit statement of stopping

The author does NOT self-certify M6.

No number here is a statement about the reliability of the system. **§28 M6's
gate has not been evaluated**, because four of its six bullets do not exist and
none of its four clauses is evaluable without them. What is delivered is two
bullets, measured on the corpus and on shapes whose truth is known, with six
defects found against me by my own mechanisms, and every remaining gap named
with a price and an owner.

**STOPPED AFTER M6 BULLETS 1 AND 2 — the successor under §28 is M7, measured
in A2.1, and M7 IS NOT STARTED. §28 M6 ITSELF IS NOT COMPLETE.**

---

# Addendum 3 — §28 M6 bullets 3, 4, 5, 6 and the first evaluation of the gate (C302–C310)

A fresh author context. The governor directed the remainder of §28 M6 and
forbade M7 and beyond. **When this pass began, bullets 1 and 2 were delivered
and bullets 3–6 were NOT STARTED**; all four are delivered now, and §28 M6's
gate is **evaluated for the first time**: three of its four clauses are green
and the fourth is NOT MET, with its price and its owner named.

## A3.1 The ladder, measured from the section BOUNDARY

Not inherited from the body above or from either addendum. Spec hash re-verified
as the first action of the pass:

```
652fd0b6e17c96c38af0173ddcc93a3921eafd60a9aff34c8d848829228d9bb1   (matches)
```

The subject set is taken from the **range** §28 occupies — line 1895
(`# 28. Milestones и жёсткие гейты`) to line 2046 (`# 29.`) — by listing every
`## ` heading inside it. Not by a predicate on the names, which is what F-0086
records: an `awk` filtering `/M[0-9]/` carried a literal about the SHAPE of the
answer, and `P1` was unfindable by construction.

```
M0 M1 M2 M3 M3.5 M4 M4.5 M5 M6 M7 M8 P1 M9 M10 M11 M12
```

**Positive control on an element whose name does NOT satisfy the pattern the
predicate would have used:** `## P1 — Partition correction API/editor` is at
line 2021, inside the range, and is present in the list above. An instrument
that filtered on `M[0-9]` would have returned fifteen headings and looked
complete. That is the control F-0086 requires, on the exact element that exposed
the defect.

- **M6 EXISTS**, line 1983, six bullets and a `Gate:` line at 1992.
- **Predecessor: M5. Successor: M7** — "Exact posterior refinement + selective
  delivery + export materialization".

## A3.2 §34 for M6, and the governor's standard

Read directly, and both line numbers verified:

- line **2254**: review "подписывает `docs/REVIEW_<M>.md` либо возвращает
  blockers";
- line **2256**: "**M2, M5 и M7** требуют отдельного numerical/topology red-team
  pass."

So **§34 requires ONE independent review for M6** and does not require a red
team. The governor has declared a stricter standard — two cold reviews of
different model families plus a separate red team — and named it as the
governor's own.

**The governor invited me to say if that standard is disproportionate. It is
not, and the evidence is this pass's own findings.** M6 is the first milestone
with an inference stage, and three of the four defects below were invisible to
reading: a G1 violation of 3.01° in models the solver had ACCEPTED, an invariance
clause of §14.5 that simply FAILED, and a DAG that silently lost its source node
and returned no path with no refusal. All three were found by instruments, and
two of the three only because a SECOND instrument disagreed with the first. A
single reviewer reading a diff would have found none of them. The standard is
proportionate to the class of defect this stage produces.

The author does not certify the milestone.

## A3.3 What was delivered

| §28 M6 bullet | status | where |
|---|---|---|
| hierarchical span candidates | **DELIVERED** (addendum 2) | `vice-fit/src/schedule.rs`, `span.rs` |
| candidate-generation budgets | **DELIVERED** (addendum 2) | `schedule::FitBudget` |
| k-best jet-compatible grammar paths | **DELIVERED** | `corner.rs`, `grammar.rs` |
| joint constrained chain refit | **DELIVERED** | `refit.rs`, `solve.rs` |
| explicit code lengths | **DELIVERED** | `code.rs`, `[geometry_code_table]` FROZEN |
| primitive/relation hypotheses | **DELIVERED in part**, stated | `relation.rs` — four of §15's six relation families; the two scene-level ones retargeted to M7 with a reason. §15's whole-loop primitives NOT started, priced in A3.7 |

### The G1 decision the governor asked me to make, and both prices

The governor's measured fact: `vice-ir`'s own canonical VALID fixture has a node
typed `SmoothG1` arriving at −14.04°, leaving at 0.00° and declaring +14.32° — a
spread of **28.36°**, valid since M1. Hold G1 by REPRESENTATION or by CHECK, and
name the price of each.

**Decision: by REPRESENTATION, in the producer.** §14.3 forbids
`angle < tolerance`, so a check needs either a tolerance (forbidden) or exact
equality of two independently rounded floats (unreachable). `refit::RefitChain`
stores the tangent once: `Handle::Shared` carries a LENGTH and the direction
comes from the node; `ArcAnchor::FromHeadTangent` stores no radius at all,
because a circle through two points tangent to a prescribed direction at one of
them is determined. A corner is the deliberate absence of sharing.

**Price of the choice TAKEN.** `vice_ir::CurveChain` is unchanged, so it can
still express an inconsistent chain, and anything constructing one outside
`RefitChain::lower` can still write the disagreement down. What is closed is the
PRODUCER. The residual is the floating-point round trip through absolute control
points, and it is measured rather than assumed: **2.776e-15 rad** on the corpus.

**Price of the choice NOT taken** — `vice_ir::Segment` storing handle lengths
instead of absolute control points, which would give the IR itself the property:
every serialized scene, the M1 canonical serialization and its digests, the
renderer, the validator and `model_universe_hash` all move, and every signed
artifact in the repository is re-recorded. That is a §1.5 model-version change
with full recalibration, and it is a milestone of its own.

### What §1.5 meant for the order of commits

The governor asked what "a bullet that moves `model_universe_hash` is a
model-version change" means operationally. Measured, and it means three things:

1. **The universe change is its own commit** (C306) and touches nothing else, so
   a reviewer sees the model-version change without a diff around it.
2. **The generator has to land first.** C305 delivered `vice_fit::relation`;
   C306 promoted the four families it generates hypotheses for. The reverse order
   would have left the universe wider than anything that produces it for the
   length of one commit, which is the silent widening §1.5 forbids.
3. **The recalibration debt has to be named where the hash is.** It is recorded
   on `FROZEN_V1_HASH`: this tree has no confidence threshold and both
   search-mass bounds are `Unknown`, so nothing must be re-measured NOW — the
   obligation is that **M7 calibrates against `e9e7f7e6…`, not `fed2af86…`**, and
   a calibration inherited across this hash is a calibration for a smaller
   grammar.

And the interaction with §27.7 that this pass found the hard way:
`[geometry_code_table]`'s freeze **could not happen before Stage H existed**,
because `bits_per_relation` is one of its three keys and
`every_frozen_value_agrees_with_the_code_that_uses_it` fails on a frozen key
nothing consumes. The order was therefore forced: C305 generator → C306 universe
→ C307 constants and claim (code, inert while the section is a placeholder) →
C308 freeze (gate file alone) → C309 code → C310 artifact alone.

## A3.4 **The gate, evaluated**

§28 M6: *"Gate: exact G1 after joint solve; sample/cut/transform invariance;
oracle G00–G20 decomposition; no BIC-only promotion."*

**It is evaluable now and it was not before**, because all four clauses are about
code that did not exist. Three are green, one is NOT MET.

| clause | verdict | measured by | number |
|---|---|---|---|
| **exact G1 after joint solve** | **GREEN**, with a positive control | `refit::g1_readings` on every smooth node of every SELECTED model | corpus **2.776e-15 rad** over 8 nodes; synthetic **2.220e-16 rad** over 9 nodes; **positive control 0.7136 rad (40.89°)** through the same instrument |
| **sample / cut / transform invariance** | **GREEN**, six of six | `crates/vice-fit/tests/grammar_and_g1.rs` | sample step 0.25/0.5/1.0 px: identical families and join kinds, breakpoints within 6 % of the chain; duplicate samples: identical; cyclic cut: the pipeline's answer identical to the last printed digit under loop rotation, per-cut spread **1.303 bits** published; translation: identical to 1e-6 bits; reflection: identical; uniform scale ×1/×2/×4: identical selection, deciding residual within **1 bit** |
| **oracle G00–G20 decomposition** | **NOT MET** | `oracle::design` | **0 of 5 arms producible**; price in A3.7 |
| **no BIC-only promotion** | **GREEN**, as a differential with a knockout | `crates/vice-fit/tests/grammar_and_g1.rs` | frozen table selects **1 arc**; the same admissible DAG ranked by the finite non-negative §14.4 proposal residual alone selects **64 arcs** at `0.000 px`. No negative pseudo-bits and no `k log n` exist anywhere in `vice-fit` |

**What the green clauses do NOT say.** The G1 clause was evaluated over **8
smooth nodes** on the corpus — the corpus is largely polygonal and most joins the
DP selects are corners. Eight is not zero and it is not many, and the clause's
strength on this population is bounded by that count. The invariance clause
asserts the SELECTION and the deciding part of the code; the bit totals move
under scale for a reason that is stated (limitation 71). The BIC clause is a
differential on one shape.

## A3.5 Measured, over the corpus, release, one cell per scene

```
arms 41 (13 without a boundary)      sealed-audit groups skipped 22
chains 36                            longest chain 83 samples
chain samples 1910                   supports 4065 (2.128/sample, bound 10187)
corner anchors 219                   max §14.1 saliency 0.4617
candidates 14659 offered             333 refused by the §14.4 cost
worst d_n / d_euclid 23.8712         at a Euclidean deviation of 0.21824 px

chains with a model 36               chains with none 0
selected segments 93                 smooth joins 8
families selected                    circular_arc, cubic_bezier, line, quadratic_bezier
path refusals                        degenerate_span 18, non_finite 10, outside_corridor 28
relations 368 considered             133 accepted
worst selected |d_n| 1.55103 px
WORST G1 SPREAD 2.776e-15 rad over 8 smooth nodes
```

The 56 refused paths are §24's stated reason for k-best — "лучший coarse DP path
может стать infeasible после exact constraints" — as a number rather than as a
justification.

## A3.6 Defects my own mechanisms found against me

Four, all by measurement, none by re-reading a formulation. Recorded as
**F-0087** … **F-0090**.

1. **The representation held G1 everywhere except where a LINE was incident, and
   the models were already ACCEPTED.** A line reads no tangent — its direction is
   its chord — so a smooth node beside one stored an angle that was not a
   parameter. `g1_readings` measured **0.0525 rad (3.01°)** on models the solver
   had returned as valid. The grammar had ALREADY priced a line's tangent as
   determined (`free_scalars(Line, …) = 0`, `tangent_is_free(Line) = false`); the
   code length and the representation disagreed about what a line-adjacent smooth
   node is, and the representation was wrong. `node_dir` now derives the angle
   from the incident line, and a smooth join between two lines is refused
   outright — their chords are G1 only when collinear, and two collinear lines
   are one line.

2. **A zero-length normal passed the finiteness guard and silently emptied the
   DAG.** `BoundarySample::normal` is documented as a unit normal; a zero one is
   finite. It then made `normal_deviation` return `None` at that sample for every
   candidate, so every support touching it disappeared. Measured: **1417
   candidates, none of them starting at sample 0, and `k_best_paths` returned NO
   PATH with no refusal to explain it.** A malformed input must be refused where
   it is malformed, not where its consequences surface:
   `FitRefusal::NonUnitNormal`.

3. **§14.5's duplicate-sample invariance FAILED.** A chain with every third
   sample repeated selected `[Cubic]` where the same boundary selected
   `[CircularArc, Line]`. The residual code was already duplicate-invariant — it
   counts `weight_ds / corr_length_px` — but the SCHEDULE is over sample INDICES,
   so repeats move every dyadic support to a different place on the boundary and
   change which candidates exist at all. `dedup_coincident` collapses them at the
   entry. **The defect is invisible on the population the pipeline actually
   sees**, because `observe_boundaries` resamples by arclength; §14.5 asks for the
   property anyway, and asking is what found it.

4. **The cut-invariance test measured the pipeline twice and I nearly wrote the
   result down as a property of cutting.** It called the whole pipeline on an
   already-rotated chain, which RE-CUT it, and reported a **47-bit** spread. The
   real single-cut spread is **1.303 bits**. `models_at_cut` exists because a test
   of cut invariance may not itself go through the mechanism that hides the cut —
   the same shape as F-0044 and F-0048 Q4, one level up: **an instrument that
   reaches its subject through the mechanism under test measures the mechanism.**

A fifth, caught before it landed and therefore not in the ledger: I computed
`bits_per_anchor = 2 log2(16384 / 0.35)` by hand as `31.029496`. The true value
is `31.029146`. The derivation is a TEST rather than a comment, so it was red
within a minute. That is the whole argument for C307's shape.

## A3.7 What §28 M6 still costs, exactly

| not delivered | price |
|---|---|
| **oracle G00–G20 decomposition** (gate clause 3) | Two capabilities, now named in `oracle::design` rather than described here. `GeometryPipelineArm`: a path from a fixture's GT partition into Stage G — `vice_fit` consumes `BoundaryChain`, which `observe_boundaries` builds from the ANALYSED image — plus a geometry error against GT geometry. `OracleInjection`: expressing a GT boundary as a candidate over an observed chain's samples (G10/G11), a selector ranking by GT agreement (G01), forced GT families and breakpoints (G20). Both need the chain identity limitation 57 prices. **Owner: M7** |
| §15 `mirror_symmetry`, `repetition` | Both are properties of a SCENE; Stage G is handed one chain. Needs §26's scene-level compound search. **Owner: M7**, and the universe now says so |
| §15 whole-loop primitives — circle/ellipse, rect/rotated rect, rounded rect/capsule, regular polygon, against the free typed chain | **NOT STARTED.** The MDL comparison machinery exists and is the same one relations use: a whole-loop model is a `RefitChain` with a constrained parameterisation scored by the same `ChainCode`. Price: one module plus a closed-chain fitter per family, and the §15 admissibility conditions on emitting a native SVG primitive (canonical boundary matches exactly, neighbours share the boundary object, post-quantization verification green) — the last of which is M7's verifier |
| a constrained re-solve for relations | The optimiser has no constraint machinery. Acceptance is SOUND and rejection is CONSERVATIVE without it (limitation 66); closing it is M7's trust-region work |
| §14.1's "stability по topology/formation hypotheses" | `BoundaryChain` is ONE hypothesis. Needs the M4.5 envelope threaded to Stage G (limitation 63) |
| local isotopy (§24's `exact_g1_and_local_isotopy`, second half) | limitation 57's class, unchanged |

## A3.8 New limitations

63. **§14.1's fourth ingredient is not computed.** Corner saliency uses
    multiscale signed turning, curvature persistence and line-intersection
    support; "stability по topology/formation hypotheses" needs the envelope, and
    Stage G is handed one chosen evidence set. **Owner: the milestone that
    threads the M4.5 envelope into Stage G.**
64. **333 of 14 659 corpus candidates are refused by the §14.4 cost** because
    some sample's normal line misses them. The refusal is correct and the
    DISTRIBUTION of what it removes is not characterised — which families, at
    which scales. **Owner M7**, price: a histogram over the refusal, whose data
    the run already carries.
65. **Local isotopy is still unchecked.** §24's line is
    `exact_g1_and_local_isotopy`; only the first half is held.
    `curve_replacement_isotopy` still names `fitted_curve` as its first missing
    capability, and a chain still carries no DCEL identity. **Owner M7.**
66. **Relations are evaluated at PROJECTED parameters, not re-solved under the
    constraint.** Acceptance is sound, rejection is conservative. **Owner M7**,
    price: constraints in the optimiser.
67. **The joint solve minimises the EUCLIDEAN residual and is judged by `d_n`.**
    Two quantities, one optimised and one gating, and the second is the spec's.
    They agree where a fit is good, which is where the solve ends up, but the
    disagreement is not bounded. Price: a normal-direction residual in the
    Jacobian, which is discontinuous where the normal ray leaves the curve —
    which is why it was not taken.
68. **§28 M6's gate clause 3 is NOT MET**, 0 of 5 G arms producible. Priced in
    A3.7.
69. **The G1 clause is evaluated over 8 smooth nodes on the corpus.** Not zero
    and not many. The corpus is largely polygonal and the DP selects corners.
    Price of a stronger population: curved-boundary fixtures, or the full
    degradation matrix instead of one cell per scene — `measure(matrix_v1().len())`,
    the same code at more cells.
70. **`ChainCode::total_bits` is not §14.5's `L_total`.** The pixel-byte
    likelihood is M7's, and `L_paint`/`L_formation` are scene properties. Named
    on the type rather than left to be assumed.
71. **The residual code's quantization normaliser is not scale-invariant**:
    measured 100.756 / 176.756 / 252.755 bits at scales 1 / 2 / 4, because the
    residual precision is a frozen px calibration and a similarity does not move
    it. It is identical for every path over one chain, so it cannot change a
    selection — but a reported bit TOTAL is not comparable across scales.
72. **`FEASIBLE_HALFWIDTHS = 3.0` is a threshold**, and what it hides is stated on
    the constant: a chain whose worst `d_n` is between one and three corridors is
    accepted and pays in the residual code. It bounds GROSS infeasibility, not
    quality.
73. **`JET_CLASSES = 32` with adjacency admits joins within about 11°.** §14.3
    calls that "coarse tangent compatibility" and it is named as a tolerance where
    it is defined. It prunes discrete paths; it decides nothing.

## A3.9 F-0048 over every mechanism this addendum added

Q4 read from the CODE, not from the prose.

| mechanism | Q1 literal | Q2 next finding | Q3 judge | Q4 provenance, by CODE | Q5 both ways | verdict |
|---|---|---|---|---|---|---|
| `normal_deviation` (§14.4's `d_n`) | no — a line/segment intersection, no window and no cap | the criterion is "does the normal line meet it", which does not grow rows | exact-zero determinant, no epsilon | deviation from the flattened candidate, normal from the M4 sample | a miss returns `None`, with a positive control on the same polyline | **PASSES** |
| corner saliency and anchors | no — persistence is a min over a GENERATED dyadic ladder | "what about scale k" is answered by the ladder | strict local maximum, a structural criterion | turning from positions, attenuation from the corridor halfwidth | corner vs arc of the same total turn, both printed | **PASSES** |
| `anchored_schedule` | no | a scale is a level, not a row | an exact structural size bound, asserted | schedule from `n` and the anchors; anchors from the saliency | the plain schedule is a subset; all-pairs printed beside it | **PASSES** |
| `GeometryCodeTable` | **the three frozen values ARE literals** | a fourth code term needs a fourth key and a §27.7 commit | `every_frozen_value_agrees_with_the_code_that_uses_it` walks the CLASS of frozen keys | values in `vice-fit`, derivation in `vice-bench` from the universe and `[identifiability]` — neither side derives the other | a table of zeros is not constructible; the cheap-table knockout is RUN | **DOES NOT PASS Q1**, residual exact: three numbers in a file. Bypass price: a §27.7 commit a reviewer reads |
| `RefitChain` / `Handle::Shared` | no — there is no second tangent to enumerate | a new family needs a lowering, not a row | the TYPE: a G1 violation is unwritable | declaration and geometry both from one `dir(angle)`; the instrument reads the lowered control points | positive control 0.7136 rad through the same function | **PASSES**, and it caught its own line-adjacent hole |
| `k_best_paths` state key | no | a new state dimension changes the key, not a list | the exactness argument: the suffix depends on the prefix only through the key | costs from the frozen table, structure from the DAG | k=8 with 56 refusals published; a k doing nothing would show as zero | **PASSES** |
| `RelationKind` | **YES** — four variants | a fifth family is a variant AND a universe change | `every_admissible_relation_family_has_a_hypothesis_generator`, both directions | universe names in `vice-bench`, generators in `vice-fit` | on-axis 2 kept, off-axis 0 kept, same instrument | **DOES NOT PASS Q1.** Bypass price: **one family admissible with no generator — now RED**, so the true price is a generator or a universe commit |
| `MissingCapability` | **YES** — the variants are a literal | a capability is DELETED when delivered, which is a rewrite not a row | the arm ladder asserts the id SET whole | arms declare what they need; the enum declares who owns it | `g30_is_still_the_only_producible_geometry_arm…` asserts the owner AND the id set | **DOES NOT PASS Q1**, and the mitigation is the deletion discipline rather than a closure |
| `dedup_coincident` | no | a different degeneracy needs a different collapse | the invariance test, which FAILED before it existed | positions from the chain; the test compares two samplings of one boundary | duplicated and plain both measured | **PASSES** |

## A3.10 What I could not verify

- **CI.** Not run. T11/T12 remain "closed in code, not executed", unchanged since
  M5. The governor waits for CI.
- **The M4.5, M4 and M5 harness artifacts.** Untouched and not re-run. Numbers
  quoted from them here are quoted from their records.
- **`docs/gt/SCORECARD_M3.json` carries the OLD `model_universe_hash`**
  (`fed2af86…`). It is the record of an M3 run and records the universe that run
  was made under; nothing re-derives it and nothing compares it against the live
  universe. Whether it SHOULD be compared is a question I did not answer, and it
  is the kind of question the third occurrence of limitation 36 was.
- **Whether the corpus's 8 smooth joins are representative.** They are what this
  corpus produces at one cell per scene. I did not run the full matrix.
- **The "one door, three locks" cross-reference** (backlog 48/49/50). Untouched
  this pass, still a reading of two documents.
- **Backlog 36, 51, 52, 54, 55, 56.** None closed. 36 and 56 did NOT grow: this
  pass added one frozen section whose values are all consumed and cross-checked,
  and whose provenance comment states DERIVATIONS rather than a population, so it
  cannot drift with a run the way `[dcel]`'s did.

## A3.11 Explicit statement of stopping

The author does NOT self-certify M6.

No number here is a statement about the reliability of the system. **§28 M6's
gate is evaluated for the first time: three clauses green, one NOT MET.** The
green ones are properties of the geometry and of the selection, each with a
positive control or a knockout that was RUN; the fourth is refused with two named
capabilities and an owner.

Four of my own defects are in the ledger (F-0087 … F-0090). Three of the four
were invisible to reading, and two of those three were found only because a
second instrument disagreed with the first.

**STOPPED AFTER M6 — the successor under §28 is M7, measured in A3.1 from the
section boundary with a positive control on `P1`, and M7 IS NOT STARTED.**

# Addendum 4 — M6 completion candidate after independent-review delta (C314–C335)

This addendum supersedes every earlier present-tense statement that M6 is “not
started”, “delivered in part”, has “0 of 5 arms”, or that its geometry harness
is owned by M7. Those statements remain above because they are the record of
the tree at C282–C313; treating them as the current state after C327 would turn
an audit trail into misinformation.

The author still does not self-certify. The tree described here is the
candidate submitted to the governor’s stricter requirement: two independent
cold reviews and a separate red-team pass.

## A4.1 Current §28 M6 scope

| §28 M6 bullet | current state | implementation/evidence |
|---|---|---|
| hierarchical span candidates | **DELIVERED** | `vice_fit::schedule`, `corner`, `span`; physical-arclength observations, dyadic full-run schedule plus persistent corner anchors |
| candidate-generation budgets | **DELIVERED** | typed `FitBudget`; an over-budget chain is refused rather than generation-order truncated; corpus 4,065 supports against a 10,187 structural bound |
| k-best jet-compatible grammar paths | **DELIVERED** | finite jet-state DAG, exact reachability checks and eight physical-bit-ranked paths |
| joint constrained chain refit | **DELIVERED** | one parameter vector for the whole chain; exact G1 by shared representation; every unreadable smooth end is refused |
| explicit code lengths | **DELIVERED** | frozen `GeometryCodeTable`, complete pricing-surface digest and proposal-integral tie-break; production entry points cannot inject a replacement table |
| primitive/relation hypotheses | **DELIVERED for boundary-local Stage H** | seven whole-loop families compete against the free typed chain through the same code; equal-radius, concentric, parallel/perpendicular and shared-baseline constrained siblings; scene-level mirror/repeated-transform hypotheses enter with M7’s compound scene search |

Whole-loop families are exactly circle, ellipse, rect, rotated rect, rounded
rect, capsule and regular polygon (3–12 sides). Every rejected and accepted
hypothesis carries primitive/free structure bits, residual change, corridor
witness and net saving. A primitive and its implied relations are competing
siblings; their savings cannot be added twice. Overlapping relation projections
also cannot add the same scalar twice.

This does **not** authorize native SVG primitive emission. The promoted
hypothesis is stored with its canonical parameters, while M7 must still prove
the §15 boundary-identity, shared-neighbour and post-quantization conditions.

The universe/pricing change is explicit:

- model universe `e9e7f7e6…2cfb` → `fdcd283a…7359`;
- pricing surface `01a8b63c…7b0f` → `4d90681d…1d42`;
- C330 changes code and the declared universe;
- gate-only C331 freezes the two resulting identities.

No calibrated confidence threshold existed under the old universe, so there is
no threshold silently inherited across this change. M7’s first confidence
calibration must use `fdcd283a…7359`.

## A4.2 Gate, re-evaluated

The §28 M6 gate is now **MET as a completion candidate**:

| clause | result | non-vacuity witness |
|---|---|---|
| exact G1 after joint solve | **MET** | `6.661e-15 rad` worst over 18 selected smooth joins; 18/18 measured, zero lowering failures; the inconsistent IR fixture reads above the `0.4 rad` positive-control floor |
| sample/cut/transform invariance | **MET** | all six registered legs; non-zero single-cut spread `1.303 bits`; rotations stay within `1 bit`; translation below `1e-6 bit`; duplicate and near-duplicate legs exercise the declared physical equivalence |
| oracle G00–G20 decomposition | **MET** | 205/205 development boundaries, five arms each, 205 forced candidate injections, 14 oracle-selector changes, zero exclusions |
| no BIC-only promotion | **MET** | frozen physical code selects the compact family while the non-negative proposal-residual-only knockout adds at least one segment |

`docs/gt/GEOMETRY_M6.json` is the five-arm artifact. All 1,025 arm rows share
compatibility fingerprint `c74a63c…e2888`. C333 made this key depend on both the
model-universe and pricing hashes; before that fix, the M6 model version changed
while the fingerprint remained `8f5a39d…`, which was not a valid compatibility
claim. C334 is the artifact recorded after that binding.

The legacy `ORACLE_M4.json` remains an M4 snapshot: its G00–G20 refusals name
M6 as the milestone that later supplied the separate geometry artifact. C332
removed the stale claim that this already-delivered harness belonged to M7 and
replayed the full 1,162-arm M4 report.

## A4.3 Current measurements

The Stage G/H corpus walk, one cell per non-sealed scene:

- 36 chains / 1,910 samples; all 36 produced a model;
- 14,326 candidates after 333 named normal-line-miss refusals;
- all four fitted span families appeared in candidates and selected models;
- 561 relation siblings considered, 6 promoted;
- 3,504 whole-loop siblings considered, 188 promoted among accepted k-best
  models;
- 105 segments and 18 smooth joins in the best chain models;
- zero entry-contract refusals and zero hidden lowering failures.

The geometry oracle aggregate (symmetric maximum error):

| arm | mean max px | worst max px |
|---|---:|---:|
| G00 | 0.0059325734 | 0.1520221988 |
| G10 | 0.0059325734 | 0.1520221988 |
| G01 | 0.0004072413 | 0.0104374370 |
| G11 | 0.0004072413 | 0.0104374370 |
| G20 | 0.0004072413 | 0.0104374370 |

G10 equalling G00 does not mean the injection is inert as a mechanism: 205
forced candidates were inserted and the automatic selector declined them.
G01’s 14 changed choices are the selector positive control. G20 forces only
families and breakpoints; it still fits the observations with the production
joint solver and frozen selector.

Exact commands, expected rows, artifact tier and gate sequence are in
`docs/REPRODUCIBILITY_M6.md`.

## A4.4 Review-delta closures

- C314–C323 close the original two cold reviews and red-team findings:
  unread smooth ends, whole input contract, measured gate populations, absence
  sentinels, near-duplicate identity, pricing surface, proposal tie-break,
  production table injection, enum-derived judges and corpus refusal
  aggregation.
- C324–C329 deliver typed G20, the five-arm common-population harness, frozen
  geometry floors and its first committed artifact.
- C330–C334 close the remaining §15 whole-loop/spec-relation gap, universe
  governance, historical oracle provenance and compatibility-key binding.
- C335 connects the full Stage G/H population and five-arm gate to Ubuntu CI
  and the committed Tier-A replay to Windows CI.

The new defect classes are F-0091–F-0107 in `FAILURE_LEDGER.md`.

## A4.5 Boundaries of the claim

Still not claimed by M6:

1. final full-resolution pixel likelihood or a final posterior;
2. a constrained trust-region re-solve (M6 relation rejection remains
   conservative);
3. DCEL-local isotopy and canonical shared-boundary export;
4. scene-level mirror/repeated-transform search;
5. selective-delivery confidence, native SVG emission or post-quantization
   verification;
6. cross-platform equality of Tier-A floating geometry rows.

Those are M7 responsibilities, not hidden M6 passes. The sealed audit remains
closed.

CI configuration now runs the complete M6 evidence path, but this local author
run is not evidence that GitHub’s remote jobs have executed. Remote CI status
must be reported by the publisher/reviewer, not inferred from the YAML.

# Addendum 5 — repaired M6 candidate after final cold-review/red-team blockers

This addendum supersedes Addendum 4’s current measurements, hashes and
five-arm provenance. The older text remains an audit record of the candidate
that the final independent reviewers blocked.

The author still does not self-certify. The exact candidate described here is
submitted to two independent cold reviews and one separate red-team review.

## A5.1 What changed after the blocked candidate

The repaired Stage G/H path now:

1. ranks and validates the geometry produced by the joint refit, never a stale
   pre-refit residual;
2. stores the selected constrained relation or whole-loop primitive as the
   actual `SelectedBoundaryGeometry`, so export and error measurement cannot
   flatten a losing sibling;
3. represents the closed-loop seam explicitly, including its G1 constraint;
4. filters unmaterializable transitions before they can consume one of the
   frozen K paths;
5. checks the evidence corridor symmetrically, observation-to-model and
   model-to-observation;
6. preserves closed-loop semantics in forced G20 fits and budgets all canonical
   openings together, with at most four persistent-corner openings;
7. searches and materializes mirror/repeated-transform relation siblings;
8. prices angles by their actual angle domain rather than a coordinate range.
9. binds cyclic roots to every observation attribute consumed by fitting;
10. derives concentric-relation tolerances in a translation-invariant frame;
11. refuses negative physical arclength weights at every public entry;
12. keeps every accepted physical code component finite and non-negative;
13. ranks smooth cyclic-seam feasibility and scalar sharing before K truncation.

C339–C347 carry these changes and their direct negative witnesses. The repaired
model universe, pricing and search constants are bound by C342 and frozen by
the config-only C343.

## A5.2 Raster-derived five-arm oracle

The previous 205-row artifact fitted samples derived from canonical GT
geometry. Its large population was therefore not evidence that Stage F could
produce the inputs, and nearly all rows were single-span. C348 replaces that
instrument.

The final oracle renders one scene from every independent development source
group plus four certified M6 family/joint/seam witnesses through an independent
ExactClip/Box/linear/no-resize cell at 128 px. It then runs canonical decode,
the production Flat2 analysis and production boundary observation. GT is used
only to:

- match a closed Stage-F chain to a face loop within 2 px;
- label forced families and breakpoints;
- construct the independent scoring target.

No authored curve sample enters a fitter. Every arm stores the complete
five-part compatibility key at construction time and the SHA-256 of its
materialized selected geometry. Selector positive controls compare those
geometry hashes.

The common population is deliberately exact rather than padded:

| quantity | measured |
|---|---:|
| source groups / scenes | 26 / 26 |
| Stage-F closed chains attempted | 19 |
| rows completing all five arms | 11 |
| exclusions, including pre-fit Stage-F/binding refusals | 17 |
| arm rows | 55 |
| forced candidate injections | 27 |
| material geometry changes G01 / G10 / G11 | 3 / 6 / 1 |
| multi-span / heterogeneous-family rows | 11 / 2 |
| arc / quadratic / cubic-labelled rows | 1 / 1 / 3 |
| G20 multi-candidate / selected-smooth rows | 3 / 3 |
| rows selecting Stage-H relation / primitive geometry | 4 / 5 |

All published counts are re-derived from the rows by the gate. Each of the 18
gate clauses has a negative knockout. Losing a family, joint, Stage-H
selection, raster witness, compatibility component or selector geometry turns
its own clause red.

Aggregate symmetric maximum error:

| arm | mean max px | worst max px |
|---|---:|---:|
| G00 | 0.6008265288 | 1.6679802262 |
| G10 | 0.2962620573 | 0.8023976306 |
| G01 | 0.6008265288 | 1.6679802262 |
| G11 | 0.2960652304 | 0.8023976306 |
| G20 | 0.2962620573 | 0.8023976306 |

These errors are larger than Addendum 4’s numbers because they measure fitted
Stage-F raster evidence rather than samples manufactured from the scoring
geometry. That increase is evidence that the inverse path is now present, not
a regression hidden by changing the metric.

The current identities are:

- model universe `47903d73…0f097`;
- pricing surface `1060cc13…0dfc691`;
- backend source `56f3be2b…a6bfd7f`;
- five-arm compatibility fingerprint `ea52f89f…db13e7a`.

C349 is the config-only freeze of the measured floors. C351 keeps every
production module under 800 lines. C363 extends the source digest over every
fitting/intervention Rust source and mechanically guards the manifest; C369
splits the residual-only control under that bound. C373–C377 close the next
independent-review findings; C378 is their config-only pricing freeze, C379
adds the independent smooth-seam raster witness, and C380 is the current
Tier-A geometry record. C382 restores the module-size bound by splitting tests,
and C383 records the split backend. C385/C386 close the final physical-code and
cyclic-search blockers, C387 is their config-only pricing freeze, and C388 is
the first 11-boundary artifact. C390 restores the production-module size gate,
and C391 records that content-bound Tier-A artifact. C393 closes the exported
grammar validation/derived-overflow gap; C394 excludes the duplicate adjacent
baseline relation, C395 freezes that pricing change alone, and C396 is the
current Tier-A artifact, reproduced byte-for-byte on `windows-x86_64`.

## A5.3 Materialized broad-corpus population

The final ignored Stage G/H corpus walk classifies the geometry actually
delivered by all 35 winners and names the one chain whose k-best set the solver
emptied:

| quantity | measured |
|---|---:|
| chains with a model / solver emptied k-best | 35 / 1 |
| selected typed chains / whole-loop primitives | 7 / 28 |
| typed selected segments / smooth joins | 18 / 1 |
| relation hypotheses considered / promoted | 498 / 4 |
| whole-loop hypotheses considered / promoted among k-best models | 3,920 / 209 |
| path refusals | 43 outside-corridor |
| worst exact-G1 spread | `3.553e-15 rad` over 1 selected typed node |
| lowering failures | 0 |

The instrument also retains all-chain generation totals: 36 chains, 1,910
samples, 4,065 supports under a 10,187 structural bound, and 14,326 candidates
after 333 cost refusals. C355 fixed the population walk so a primitive winner
cannot bypass those outer-chain totals and so G1 is read only from selected
typed geometry, including an explicit closure seam. The old Addendum 4 counts
of 105 free-chain segments and 18 joins were not a claim about materialized
winners. After the cyclic cut repair, `7 + 28 = 35`; the remaining chain is the
explicitly reported emptied-k-best refusal, not an unclassified winner.

## A5.4 Gate and governance

The §28 M6 gate is **MET as a completion candidate**:

| clause group | result | non-vacuity |
|---|---|---|
| exact G1 after joint solve | **MET** | every selected smooth node, including the closure seam, is read from materialized geometry; inconsistent controls fail |
| sample/cut/transform invariance | **MET** | six executable function-pointer-registered legs, cyclicly stable bounded cuts and an explicit closed seam |
| G00/G10/G01/G11/G20 decomposition | **MET** | 11 raster-derived common rows, all five arms, geometry-hash changes 3/6/1 |
| no BIC-only promotion | **MET** | physical-code winner changes under the registered finite non-negative proposal-residual-only knockout |

The earlier mixed C331 was a real §27.7 defect. History now contains C331a
code-only and C331b config-only; the project’s own per-commit `gates-check`
passes every one of the 85 commits from `origin/main` through this candidate.

`REQUIREMENTS_TRACEABILITY.md` now contains M6-1…M6-12. Defect classes
F-0108…F-0130 record the final repair rather than letting Addendum 4’s invalid
claim disappear.

## A5.5 Boundary of the stop

M6 does not claim scene-level posterior selection, full-resolution final
likelihood, compound topology/geometry search, optimizer convergence,
selective-delivery confidence, sealed export or post-quantization SVG
verification. Those are successor milestones.

The sealed audit remains unopened. M7 is not started until the required
independent verdicts accept this exact HEAD.

# Addendum 6 — second exact-SHA review delta

Two independent cold reviews and the separate safety/correctness audit all
blocked exact SHA `1d299477525a8610e12d5538234b03fc9c4e0864`. They agreed
that the earlier public-input closure omitted the exported grammar stages; the
separate audit also found an independent Stage-H undercoding case. No M7 work
began.

## A6.1 Exported grammar refusal is now typed

`build_edges` and `k_best_paths` now return `Result` and run the same complete
sample validation as the high-level model APIs. Invalid negative mass and a
finite-positive ratio that overflows are refusals, not an absent edge or a
zero-evidence source transition. A second derived guard checks the weighted
per-observation residual and its running sum; finite inputs whose product
overflows return `FitRefusal::NonFiniteResidualCode` with the sample, weight,
one-observation bits and accumulated bits.

The direct regression matrix calls both exported functions with negative
weight, `f64::MAX / f64::MIN_POSITIVE`, and `f64::MAX / 1`. It also requires
the high-level model path to surface the same typed class instead of reporting
`candidates > 0, edges = 0, paths = 0, refused = []`.

## A6.2 Adjacent shared baseline is not a second relation

For adjacent line spans the second line already starts at the first line's
endpoint. `SharedBaseline` therefore materialized exactly the same constrained
chain as `Parallel` while claiming two determined scalars instead of one.
Stage H could select that duplicate label on a nonexistent extra saving.

C394 makes pair topology part of relation eligibility: adjacent
`SharedBaseline` is not formed, while the non-adjacent family remains present.
The canonical pricing surface serializes this eligibility bit. C395 freezes
the resulting hash in a config-only commit, and C396 records the artifact
alone.

## A6.3 Current exact measurements and identities

The row-derived geometry measurements did not move: 11 common boundaries,
55 arms, 27 injections, selector changes 3/6/1, four relation-selected rows,
five primitive-selected rows, and all 18 clauses MET. Exact replay succeeds on
`windows-x86_64`.

- model universe `47903d7374d54683e60c318239d75adabcc2eef5fc80ad9d7822e8176990f097`;
- pricing surface `1060cc132bde90a32043a9a7bca6c6936be241b38ac20523ed2f76bea0dfc691`;
- backend source `56f3be2b4fb057d2702004ed9d803cf80a53569480ae3055fe3221074a6bfd7f`;
- five-arm compatibility fingerprint
  `ea52f89f00c32dd4e35d7d275dfc0c253da37165a4486d7733ef509cddb13e7a`.

This remains an M6 completion candidate, not a self-certification. Fresh
two-cold-review plus separate safety/correctness verdicts are required on one
unchanged final SHA before M7 starts.
