# STATUS_M5 — Shared DCEL + safe dual/primal transactions

Spec v1.3 §28 M5, §12, §11.4, §5.3, §5.4. Commits **C238–C247**, base
`d624e81`.

> **This report is an author report. It does NOT make M5 green.**
> §32 rule 29 and §34: a milestone requires an independent cold review from a
> clean checkout, and the author does not self-certify. **§34 additionally makes
> a separate numerical/topology red-team pass MANDATORY for M2, M5 and M7.** It
> was optional for M4.5. It is not optional here.
>
> The governor's standing requirement for this milestone is stricter still:
> **two independent cold reviews of DIFFERENT model families, plus a separate
> red-team PASS.**

---

## 1. What was built

### No new crate

§4's target structure names `vice-topology` as "event trees, hypotheses, RAG,
**planar graph**, signatures", and §4.1 forbids structure outside that list. A
`vice-dcel` crate is not in §4 and would have needed an ADR arguing against the
spec rather than from it. The DCEL is `vice_topology::dcel` — seven modules,
each under the §4.1 size rule (largest 718 lines).

*Price of the alternative, named because the decision was made on it:* if a
later milestone needs the arrangement without the event trees — M7's optimizer
is the plausible one — extracting `dcel` into its own crate is a mechanical move
of seven files with no logic change, one commit, and `vice-topology` then
depends on it. Cheaper than carrying a crate §4 does not name for six
milestones.

### Six of §12's seven invariants have no failure mode

§12's demand is not "check them": a wrong state must not ASSEMBLE. `vice-ir`
already checks the same seven after the fact, and it has to — its `PlanarGraph`
has public fields and accepts graphs from outside. M5 constructs, so:

| §12 invariant | how it is held | can a value violate it? |
|---|---|---|
| every half-edge has a twin | `HalfEdgeId::twin` is `id ^ 1` | no — there is no twin field |
| interior boundary has two owners | `Boundary::owners: FacePair` | no — two owners is the shape of the record |
| border boundary has interior + exterior owner | the padding ring is background, so the exterior is an ordinary face of the ordinary walk | no |
| face cycles CLOSED | a loop is a `Vec<HalfEdgeId>` traversed modulo its length | no — a cyclic traversal has no open state |
| face cycles ORIENTED | *(corrected in addendum 3: this was claimed from the row above and does not follow from it — see A3.2)* | **yes**, and it is a computation since delta-3 |
| no dangling cracks | `FacePair::new` refuses equal ids and is the only mint | no |
| non-adjacent boundaries do not intersect | segments are unit steps of an integer lattice | no |
| Euler / cubical signature preserved | `dcel::audit` | **yes** — and it is the audited one |

`Dcel::assemble` is the only constructor and returns no `Result`: every binary
labelling has exactly one cubical arrangement under a given convention, so there
is no invalid outcome to represent.

**The honest boundary, stated where the strength is claimed** (F-0048's last
paragraph). The cheapest bypass is a second constructor added inside
`vice-topology` — one `pub fn` taking pieces. Nothing makes that a compile
error, and a scan for `pub fn -> Dcel` would be a text scan, which F-0048 Q3
calls a habit rather than a judge. What ships instead is a computation:
`is_the_assembly_of_its_own_labelling` re-derives the arrangement and compares
every entity, so a hand-built `Dcel` that is not the assembly of its own input
is caught. The residual is "a hand-built `Dcel` that IS the assembly of its own
input", which is a correct `Dcel` obtained the long way.

### §5.4 robust predicates, obtained by choosing the representation

The combinatorics is decided by comparisons of `u32` lattice coordinates and
`bool` labels. There is no orientation test to make adaptive and no tolerance to
type, because there is no float in the decision path. §5.4's list applies to
M6's fitted curves.

### Transactions (§11.4, §12)

`dcel::transaction::apply` executes three of §11.4's seven steps — the edit, the
DCEL rebuild, the local certificate — and decides acceptance against the
CERTIFICATE. It has no parameter through which a cost, a bound or a score could
arrive, so §32 rule 14 is kept by the signature rather than by prose. Atomicity
is bought by immutability: the base is `&Dcel`, so rollback is dropping a
candidate.

The four §12 isotopy conditions for curve replacement are a typed refusal naming
M6; the first, incidence, is computed and published as the executed half.

---

## 2. What was measured

Every number below is printed by a command in `docs/REPRODUCIBILITY_M5.md`.

```text
gt-corpus dcel --scope full            EXIT 0, four [MET]
  41 scenes, 474 arms (444 corpus, 30 structural), 0 refused,
  22 sealed-audit groups skipped
  classes [(0,0), (1,0), (1,1), (2,0), (2,1), (3,0), (3,1), (5,0)]
  groups 237, classes in 247 out 247, convention-dependent 10
  transactions 167 attempted, 167 committed, 0 rolled back
  unrelated chains 127, moved 0
  audit resolving power: 28 arrangements of 474 arms, 155 160 slots,
    audit 5648, assembly 155 160, neither 0, no-ops 0

exhaustive 4x4    131 072 arrangements, 11 classes, 41 678 with a critical 2x2
exhaustive 4x3      8 192 arrangements, in the DEFAULT test path
structural register five fixtures x five sizes x two arms, classes asserted
```

**The audit alone catches 5648 of 155 160 slots — 3.6 %.** That number is
published rather than summed away, and it is the honest shape of clause 4: most
slots are face ids inside a uniform face, where no construction invariant can
see the change and only the assembly comparison does. Both checks are cited in
the row, because a row citing one would claim a resolving power that one does
not have.

---

## 3. Gate table (author-side; §28 M5)

| # | Gate | Статус | Evidence |
|---|---|---|---|
| T1 | **§12: six invariants held by the representation** | PASS | not checked because not representable; the table in §1 names where each lives; `a_face_pair_with_one_owner_is_not_constructible` checks the one mint in both directions |
| T2 | **§12: Euler/cubical signature** — the one invariant that is a computation | PASS | `dcel::audit`; resolving power measured by the mutation walk, not asserted |
| T3 | **§5.3: deterministic ambiguous-saddle branches** | PASS | the pairing is unconditional and the convention decides the FACES; both arms disagree on a critical 2x2, asserted rather than assumed; F-0057 records the rule that was wrong first |
| T4 | **§28 M5 clause: no final-topology claim from proxy** | **MET** | `apply` has no parameter for a score; 237 groups, 247 classes in, 247 out; the row stands on 10 convention-dependent groups, all from the structural register, because the corpus has none (limitation 18) |
| T5 | **§28 M5 clause: candidate recall maintained after budget pruning** | **MET** | 0 of 474 arms disagree with the independent chain; 0 fail `V - B + L = 2C`; 8 distinct classes so the equalities are not about a run of disks |
| T6 | **§28 M5 clause: no unrelated graph mutation** | **MET** | 167 committed, 127 unrelated chains, 0 moved; 56 transactions had no unrelated chain and that number is published because it is where the clause says nothing |
| T7 | **§28 M5 clause: no dangling/invalid faces** | **MET** | 0 of 466 non-empty arms fail the audit; 8 empty arrangements measured and excluded from every clause; mutation walk 155 160 slots, caught-by-neither 0, no-ops 0 |
| T8 | **§27.7 for the M5 thresholds** | PASS | `[dcel]` section, three separate commits (C239 keys, C240 rows, C241 freeze), zero code lines in the two gate commits; `Threshold::from_gates` is the only mint and takes a file, not a number |
| T9 | **Proof domain covers application domain** (F-0054 / F-9) | PASS | three named axes; the sweep's own exclusion was found and closed (F-0058) |
| T10 | **REVIEW_M4_5 condition 51** — structural register by construction | **CLOSED, with a correction** | class `(1,1)` asserted at every size under both arms; the fifth fixture is a diagonal pinch, not a triple junction, and §5 п.1 says why |
| T11 | **M4-N9** (§27.7 in CI when the push base is unavailable) | **CLOSED IN CODE, NOT EXECUTED** | the NOTE is gone, the base is reconstructed, the fallback is `exit 2`; a workflow the author has not run is a claim, and it is recorded as one in §5 п.2 |
| T12 | **F-2** (build separated from measurement in CI) | **CLOSED IN CODE, NOT EXECUTED** | both measuring jobs build first in a step that does nothing else; same caveat as T11 |
| T13 | **M4-N8** (`BoundaryRefusal::Malformed` unreachable by type) | **OPEN, NOT PASSED OFF AS CLOSED** | owner and price in §4 |
| T14 | **A7.1** (Tier B) | **NOT CLOSED, RECORDED** | owner M12; no Tier B instrument exists; the M5 projection is not offered as one |
| T15 | **Independent cold review** (§32 rule 29, §34) | **OPEN — BLOCKS M6** | two reviews of different model families plus a separate red-team PASS; the author does not self-certify |

---

## 4. The inherited obligations: counted, not inherited

REVIEW_M4_5 addendum 6 §7: *"Five items with owner M5 plus A7.1, M4-N8, M4-N9
and limitation 18 — nine open obligations carried forward. That is still all
right; **on the next milestone this must be COUNTED, not inherited.**"* This is
that count. Thirteen items came in. **Five are closed, eight are carried, and
every carried one has a price re-stated below.**

| # | What it is | M5 verdict | Price if carried, and owner |
|---|---|---|---|
| **F-1** | trust anchor is `HEAD` on the runner | **CARRIED** | one `run:` line with a shim. **Owner M7.** See §4.1 |
| **F-2** | build and measurement not separated in CI | **CLOSED IN CODE** (C244), not executed | — |
| **F-3** | `git` substituted on `PATH` | **CARRIED** | one shim in `PATH`. **Owner M7.** See §4.1 |
| **F-4** | the seal stops at `Vec<u8>`; the echelon is two literal arrays | **CARRIED** | a new `pub(crate)` product type plus one `pub fn`; 258 048 bytes, 22 sealed-audit groups. **Owner M8** — the milestone that touches the corpus. Closing it needs a call scanner resolving names to definitions, which is a new instrument with no M5 responsibility; M5 added no corpus type and no pipeline function |
| **F-5** | `report`/`audit-status` take the gate file from anywhere | **CARRIED** | the values decide no row; the file enters by hash. **Owner M7.** The named fix is to split the two roles by type (threshold source vs hashed input); M5 did the half that was in its way — `Threshold::from_gates` now takes a FILE and a key rather than a number, so no gate config anywhere can be built from an integer |
| **F-6** | magnitude as a word in the unverified tier | **CARRIED, and narrowed** | one phrase in an evidence column. **Owner M6.** M5's own gate rows publish their quantities positionally; the residual is the pre-existing tiers |
| **F-7** | mechanism-existence check sees a PHRASE, not behaviour | **CLOSED FOR M5's OWN MECHANISMS** (C240) | M5's clauses ship with behavioural knockouts in the tree, each with a positive control, run in CI. The thirty-three M4.5-era attacks are still not in the tree: **carried, owner M6**, price = one harness that applies an edit and requires the measurement to move, per attack |
| **F-8** | the judge's 256/512 gap | **CLOSED FOR THE M5 INSTRUMENTS** | the DCEL's proof domain reaches 512 by construction. The M4.5 judge's own gap is unchanged: **carried, owner M6**, price = a run 16x the size of the 128 px one |
| **F-9** | the class: proof domain smaller than application domain | **CLOSED AS A CLASS FOR M5, AND VIOLATED ONCE HERE** | three named axes; and F-0058 is my own instance of it, found by my own gate, six commits after writing the rule down. Carried as a STANDING class, owner: every milestone |
| **cond. 51** | name the third axis | **CLOSED, with a correction** | see §5 п.1 |
| **oblig. 18** | third topological ambiguity pair in the corpus | **CARRIED** | **owner M8.** Adding a pair re-keys `docs/gt/CORPUS_MANIFEST.json` (1086 digests), moves the split assignment and touches the SEALED AUDIT seal — §27.1's burn policy makes that a reviewed change of its own, and M5 must not open it. What M5 gave the underlying need instead: the structural register carries a convention-dependent fixture at every size, so the M5 clause that would have had a singleton population has ten groups |
| **M4-N8** | `BoundaryRefusal::Malformed` unreachable by type | **CARRIED** | **owner M6.** See §4.2 |
| **M4-N9** | CI prints a NOTE instead of refusing | **CLOSED IN CODE** (C244), not executed | — |
| **A7.1** | Tier B | **NOT MINE, VISIBLE** | owner M12 |
| **10th (red team's own)** | attacks live in the tree as `#[ignore]` tests with positive controls | **DONE FOR M5, CARRIED FOR THE REST** | same as F-7 |

### 4.1 Why F-1, F-3 and F-5 go to M7 and not to M6

All three are properties of a RUNNER, and the honest closure of F-1 and F-3
together is architectural: a separate protected job whose output is attested, or
an anchor taken from the event SHA rather than from a local ref. **No in-process
mechanism defeats an attacker who controls the build environment** — compiling
the gate file in is defeated by a rebuild, and consulting `git` is defeated by
`PATH`. M4.5 froze them because it could not verify CI by a run; that reason is
unchanged for M5, and repeating it is the honest answer rather than a new one.

They are addressed to M7 rather than M6 because M7 is the first milestone whose
gate depends on numbers a runner produces at scale (§28 M7: full-resolution
posterior, selective delivery). Naming M6 would be naming the next milestone
rather than the right one.

### 4.2 M4-N8: what M5 delivered toward it, and what it costs to finish

M4-N8 is "`BoundaryRefusal::Malformed` is unreachable BY TYPE", and REVIEW_M4_5
says it "still requires the DCEL". The DCEL now exists and its chains are closed
or vertex-terminated by construction — that is the capability M4-N8 was waiting
for, and it is delivered.

What is NOT done is the rewiring: §13 step 4 says the chain is BOUND to DCEL
endpoints and junctions, and `vice_evidence::boundary` still extracts its own
chains by marching squares on the pixel-CENTRE lattice. Taking them from the
DCEL instead moves every boundary sample by half a pixel, which moves the
corridor calibration, which moves `[noise_scales] clean_bucket_sigma_codes =
25.57` — a frozen gate value — and `docs/gt/CORRIDOR_M4.json`.

**Price, in full:** one rewiring commit, plus one gate commit re-freezing the
noise scale, plus one re-recording of the corridor artifact, plus the M4 gate
rows re-measured. **Owner M6**, which owns the typed refit of boundaries anyway.

This is not a dodge: §27.7 forbids changing the gate file and production code
together, and re-freezing a measured constant is a reviewed change of its own.
Doing it inside M5 would have been the thing this project punishes.

---

## 5. Known limitations (the honest boundary of M5)

Numbering continues from STATUS_M4_5, which ends at 28.

29. **The fifth structural fixture is not a triple junction, and cannot be.** In
    a binary labelling the degree of a lattice point is the number of
    disagreeing pairs around a 2×2 — a cycle of four — so it is even: 0, 2 or 4,
    never 3. Three regions meeting at a point needs three labels. What is there
    instead is the degree-four critical 2×2 as a diagonal pinch, which is the
    only junction a binary labelling has and the only fixture whose class differs
    between the arms. **The real triple junction is an M8 obligation** (§11.5,
    §28 M8), and condition 51 is reported CLOSED WITH A CORRECTION rather than
    closed, because a substitution made quietly is exactly the "list from which a
    line silently dropped" of M45-N31.

30. **The transaction rebuilds the WHOLE arrangement, not the affected part.**
    §11.4 says "rebuild AFFECTED DCEL". `apply` rebuilds all of it and then
    PROVES that nothing outside the region moved. For a few transactions per
    envelope that is the cheaper engineering; for M7's optimizer inner loop it
    will not be. **Owner M7**, price: an incremental rebuild plus the proof that
    it agrees with the full one on the corpus.

31. **The M5 population is the ORACLE observation, not the estimated-evidence
    path.** Arms are the corpus's scenes digitized by the §5.3 majority rule from
    exact ink coverage. §28 M5's four clauses are about the arrangement and the
    transaction, so that is the right population for them — but it means this
    report says nothing about what the DCEL does to an envelope built from
    ESTIMATED coverage. That question is M4.5's clause and M6's problem.

32. **`continuation.rs` still says the DCEL is absent.** Its plan-level refusal
    for "rebuild affected DCEL" names `shared_dcel` as missing with owner M5,
    and that sentence is now false at the executor level. It is left alone
    because its partial/refused COUNTS are frozen into
    `docs/gt/TOPOLOGY_M4_5.json`, and §27.1 forbids moving a signed artifact in
    a feature commit. **Owner M6**, price: one commit re-freezing
    `TOPOLOGY_M4_5.json` together with the reclassification.

33. **The audit catches 3.6 % of perturbed slots on its own.** The rest is
    caught by assembly equality, which is blind to a systematically wrong
    `assemble`. Both are cited in clause 4 for that reason. The gap is real and
    is the reason the exhaustive sweep exists.

34. **One transaction shape, chosen from the canvas.** Each arm runs the same
    edit — fill a small centred square — so the transaction population is not
    selected by what happens to work, but it is also not a search over edits.
    **Owner M7**, which is where compound discrete search arrives (§19).

35. **56 of 167 committed transactions had no unrelated chain at all.** Their
    region plus halo covered everything, so on those the "no unrelated graph
    mutation" clause is a statement about the empty set. The number is published
    beside the row rather than left to be inferred.

36. **The eighth instance of the F-0048 class, found in my own milestone and
    only partly closed.** `doc_claims.rs` derives the ROWS of a gate table from
    the document's structure — that was M45-N20 and condition 23 — but the SET
    OF DOCUMENTS is still two literals, `CLAUSE_ROWS` and `POSITIONAL_DOCS`. So
    the answer to F-0048 Q2 was *append a line*, and `docs/STATUS_M5.md` was
    exactly that line: written, carrying a gate table with measured quantities
    in it, and checked by nothing.

    What C246 changed is the DEFAULT, not the coverage:
    `every_status_document_is_classified_or_excepted_with_a_reason` walks
    `docs/` — a side the commit writing a gate row does not edit — and a status
    document under no tier and in no reviewed exception fails the test. The
    exception list is a list of DECISIONS with reasons and owners rather than a
    list of subjects, and forgetting a document is now red instead of silent.

    STATUS_M5 is in that exception list. **The full closure is: a `dcel:` key
    prefix in `KEY_PREFIXES`, `DCEL_M5.json` in `artifact_file`, a row-kind
    table in `docs/REPRODUCIBILITY_M5.md`, the declaring-document path derived
    instead of hardcoded to `REPRODUCIBILITY_M4_5.md`, and every M5 gate row
    bound position by position to an artifact key.** **Owner M6.** The cheapest
    bypass while it is open: a number in an M5 gate-row evidence cell that no
    artifact carries — which is F-0028 in the document that reports M5.

    I am naming this rather than reporting the class closed, because
    REVIEW_M4_5's own summary of what M4.5 cost is that seven mechanisms in a
    row were "strictly better than the previous one" and all seven left the next
    line. Strictly better is what C246 is.

---

## 6. Blockers before M6

1. **T15**: **two independent cold reviews of DIFFERENT model families**, each
   from a clean checkout, running the documented commands without the author's
   caches (`docs/REPRODUCIBILITY_M5.md` + `_M4_5` + `_M4` + `_M3`), **plus a
   separate numerical/topology red-team PASS**, which §34 makes mandatory for
   M2, M5 and M7. Until all three sign, M6 is not permitted. The author does not
   self-certify.
2. **CI has not been executed.** F-2 and M4-N9 are closed in code and unverified
   in fact. A reviewer with a runner should look at them first, because they are
   the two items this milestone claims and cannot prove.
3. **A7.1 / Tier B** — open, owner M12, and it must stay visible in every
   following STATUS.
4. **M4-N8** — open, owner M6, price in §4.2.
5. **F-1, F-3, F-5** (M7), **F-4, obligation 18** (M8), **F-6, F-7, F-8,
   limitation 32** (M6) — all open, all priced, none passed off as closed.

---

## 7. Explicit statement of stopping

The author does NOT self-certify M5.

No number here is a statement about the reliability of the system. Four `[MET]`
clauses mean that the arrangement is what the labelling says it is, that a
transaction touches what it declared, and that the instrument saying so has
demonstrated resolving power. They say nothing about whether the right topology
was chosen — M5 chooses nothing, and §32 rule 14 forbids it to.

Three ledger entries (F-0057…F-0059) are my own errors. All three were found by
measurement rather than by re-reading a formulation, and two of them were found
by mechanisms I had built in this same milestone to catch that class in others.
F-0058 is the one worth carrying forward: **the instrument built to close "the
proof domain is smaller than the application domain" excluded a subclass by
construction, six commits after I wrote the rule down.** Knowing a rule is not
executing it, and this project has now recorded that twice.

No M6 code (typed grammar, k-best DP, joint G1, explicit MDL) is started. No
placeholder API for M6+ exists: what is missing is recorded as data with an
owner.

**STOPPED AFTER M5 — M5.5 NOT STARTED.**


---

# Addendum 1 — delta-1 (C249–C253)

The M5 gate was NOT met. Two independent cold reviews returned
`ACCEPT WITH CONDITIONS` and the red team returned **FAIL** with two blockers.
This addendum records what changed; the body above is left as it was signed,
and every correction below names the sentence it corrects.

Three contexts that did not know about each other converged on three defects —
swapped format arguments, a false "runs in CI" claim, 12 classes against a
declared 11. Where they converged there is nothing to argue about.

## A1.1 The two blockers

**RT5-A1 — the largest field of the structure was read by no predicate.** A
ten-line edit rotating every entry of `Parts::face_of_padded_px` above 16 px
passed 530 tests, four `[MET]` clauses, a byte-identical artifact, `dcel-check`,
the exhaustive 4×4 sweep and every knockout. `audit()` never touched that field;
in the whole workspace `face_of_pixel` was read by two assertions on two
hand-picked pixels of one 9×9 disk.

This is the **eighth level of the F-0048 class and the first about the OUTPUT**:
exhausting the input domain and exhausting the CHECKED FIELDS of the value are
independent properties, and the mechanism that made the first compiler-checkable
was taken for one that makes the second checkable. Enumerating all 65 536
labellings of a 4×4 gives nothing to a field no predicate reads.

Closed by `dcel::crossing` — the third construction both cold contexts named
independently, and the one thing REVIEW_M5_A said it had not done. It rebuilds
the pixel-to-face map by walking each row from the background ring and reading
the OWNER on each boundary segment it crosses: it never joins two pixels and
never looks at the stored map. The red team's own edit is now a test and the
clause-4 knockout.

**RT5-A2 — `caught_by_neither == 0` was a theorem.** A perturbed value is by
construction not the assembly of its own labelling, so that conjunct could not
be false and `caught_by_assembly_equality == slots − no_ops` identically. What
clause 4 really asked of the audit was `caught_by_audit > 0` — one slot — and
the red team reduced `audit()` to range guards plus a single check, deleting the
whole seventh §12 invariant, with the gate green and 530 tests passing. F-0035
is quoted at the top of the file three doors away.

Both identities are gone. The row requires `uncaught_by_audit == 0`: every real
perturbation of every derived slot rejected by the audit ALONE. **5648 of
155 160 → 155 160 of 155 160.**

**RT5-A3 — the M5 gate was in no CI step.** Not "CI was not executed": the
workflow did not contain the steps six places in the tree said it contained, and
that is checkable by reading. `dcel-check` never calls `gate_table`, so a run
with four NOT MET clauses passes it. F-7 was reported CLOSED on the ground that
the knockouts "run in CI"; the ground was false, so **that closure was invalid**.
Four steps added, and `every_ignore_that_claims_ci_is_named_by_a_workflow_step`
now derives the claim from the file it is about.

## A1.2 Corrections to the body above

| § | said | is |
|---|---|---|
| §2, §3 T6 | 56 transactions with no unrelated chain | **64** |
| §2 | exhaustive 4×4: 11 classes | **12** — `(0,0)` is contributed by the two empty labellings C243 stopped skipping |
| §3 T4 | (row wording) | see A1.3 |
| §3 T7 | 8 empty arrangements "excluded from every clause" | **true of three clauses of four** until delta-1; clause 3 counted their transactions |
| §4 F-7 | CLOSED FOR M5's OWN MECHANISMS | **invalid closure**, re-claimed in C251 on steps that exist |
| §4 oblig. 18 | (justification) | see A1.3 |
| §5 п.29 | "three regions meeting at a point needs three labels" | **false as stated.** Under fg-4 THREE faces are incident at the pinch with two labels — the author's own fixture refutes it. What is impossible is a degree-**three** vertex, by degree parity, and that is the real justification (REVIEW_M5_B N8) |
| §5 п.35 | 56 | **64** |
| §1, ADR-0031 §2 | cheapest bypass = "one `pub fn`" | **cheaper**: `walk::with_parts` is `pub(crate)` and already exists, with `pub(crate)` `Parts` fields beside it (REVIEW_M5_A N5) |

## A1.3 Two claims restated, because correcting the number was not enough

**T4 / obligation 18.** The clause-1 row asserted that every
convention-dependent group comes from the structural register, because
STATUS_M4_5 limitation 18 says zero of the corpus's 132 arms carry such a class.
M5's population is 444 corpus arms over different cells, and 7 of the 10 groups
are corpus groups — the list printed in the same sentence refuted the sentence.
Honest form, now computed rather than asserted: **the register guarantees the
population BY CONSTRUCTION, at every size and under both arms; the corpus
supplies it IN FACT on this cell set, and would not necessarily on another.**
Carrying a limitation between milestones obliges recomputing its number on the
new population.

**Clause 3's population is selected by outcome.** `transaction_for` drops **307
of 474** arms through `_ => return None`, and the doc comment said "the
population is not selected by what happens to work". Four of `apply`'s six
refusal reasons cannot fire on this population, `NotTheDeclaredEdit` among them,
because `kind` is read off the same comparison `apply` redoes — so "167
attempted, 167 committed, 0 rolled back" is a property of the harness. The
excluded count and the reachable/unreachable refusal sets are published now, and
the excluded subclass is exactly what §28 M5 calls **compound** transactions.
That is F-0058's second instance, in the same milestone.

## A1.4 New limitations

37. **The compound transaction is not attempted.** 307 of 474 arms are excluded
    because the edit's effect on the signature is not a single `±1`. §28 M5 says
    "local **compound** topology transactions". **Owner M6**, price: a harness
    that attempts every arm and classifies the outcome, plus whatever `apply`
    needs to accept a multi-step signature delta.

38. **`UnrelatedGraphMutation` is unreachable on the production path**, and that
    is a theorem rather than an accident: a chain wholly outside the ROI depends
    only on labels step (1) guarantees unchanged. The chain COMPARISON has
    demonstrated resolving power — `the_chain_comparison_detects_a_distant_change_when_it_is_given_one`
    is the world in which it moves, and it found on its first run that the
    comparison was one-directional and blind to a chain that APPEARS. `apply`
    reaching the branch is still not demonstrated, and the row says so.

39. **The M5 population carries only the `Thresholded` saddle reading.**
    `Dcel::assemble` takes no `SaddleResolution`; the harness thresholds at the
    majority level. §32 rule 14 is not violated — M5 does no narrowing, and the
    exhaustive axis covers every 4×4 labelling including join/split ones — but
    limitation 31 declared the estimated-evidence narrowing and not this one
    (REVIEW_M5_A N11). **Owner M6.**

40. **`distinct_classes` counts `(0,0)`,** which is carried by exactly the 8
    empty arms no clause may stand on. Clause 2 conjoins `distinct_classes >= 3`
    (REVIEW_M5_A N12). It is not load-bearing at 8 classes; it is wrong in kind.
    **Owner M6.**

41. **The per-clause knockout run costs ~7 minutes** at full scope, five runs of
    the corpus. It is `#[ignore]`d and wired into CI; if that becomes the
    bottleneck the answer is a smaller scope with the floors scaled, not fewer
    knockouts. **Owner M6.**

## A1.5 What delta-1 did NOT do

- **T11/T12 stay "closed in code, NOT executed".** The four CI steps exist and
  are checked by a test; no runner has run them. REVIEW_M5_B is explicit that
  adding the steps is the substantive answer and that these rows stay unexecuted
  until a real run, and that is what they say.
- **Limitation 36 (M5 gate rows under positional binding) is still open**, owner
  M6, and delta-1 raises its price from desirable to obligatory: both reviewers
  found live instances of exactly the class it predicted, in the gate's own
  output.
- **M4-N8, A7.1, F-1/F-3/F-5, F-4, obligation 18, F-6, F-8, limitation 32** are
  unchanged, with the prices in §4 above, which REVIEW_M5_A explicitly does not
  contest.


## A1.6 F-0048 over every mechanism, after delta-1

The red team's own run found three of eight failing, and all three carried the
four `[MET]` clauses. This is the same procedure re-run, including on the
mechanisms delta-1 added. It is published rather than asserted, and two rows do
not pass.

| mechanism | Q1 literal? | Q2 next finding | Q3 judge | Q4 guard key = mechanism key? | Q5 both directions? | verdict |
|---|---|---|---|---|---|---|
| representation invariants (six of §12) | no | the criterion changes; the violation is unrepresentable | compiler / type system | no | `a_face_pair_with_one_owner_is_not_constructible`, both ways | **PASSES**, residual repriced (`with_parts`, not "one `pub fn`") |
| `Parts::perturbations` | no — exhaustive destructuring | a field without a site does not compile | compiler | no | red, empty and IDLE (`no_ops`) | **PASSES**; the `field: _` bypass is named where the strength is claimed |
| `audit()` | **YES — numbered blocks written by hand** | "append a block" | habit, on its own | — | now bounded: `uncaught_by_audit == 0` over 155 160 slots | **PASSES ONLY BECAUSE IT IS BOUNDED.** The enumeration is still an enumeration; what changed is that a field with no predicate now shows up as an uncaught slot and fails clause 4, so the next missing block is a red gate rather than silence |
| `dcel::crossing` (new) | no | the criterion changes | an independent construction | **no** — owners on crossed segments vs flood fill over pixels | RT5-A1's own edit is red; the clean structure is green | **PASSES** |
| `audit_every_labelling` | no by inputs; **no longer yes by checked values** — every field has a predicate | a field without a predicate fails clause 4 | exhaustive enumeration of the input space | — | class set asserted EXACTLY, not as a floor | **PASSES** |
| `RunKnockouts` | **no longer** — the count is compared against `gate_table()`'s own length | a fifth clause without a knockout fails the test | the gate table | — | each knockout must redden its OWN row, clean run all green | **PASSES** |
| `transaction_for` | **YES — a four-arm `match` with `_ => return None`** | "the fifth kind is dropped" | **the outcome, i.e. the author** | yes — `kind` is read off the comparison `apply` redoes | the excluded count is published now; the branch is still not attempted | **DOES NOT PASS.** Mitigated, not closed: 307 of 474 excluded, published, and named as limitation 37 with owner M6. §28 M5 calls that subclass COMPOUND |
| `DcelGateConfig` / `Threshold::from_gates` | no — one mint, takes a FILE and a key | arithmetic is a type error | compiler + the frozen-value cross-check | no — file and code check each other | yes | **PASSES** |
| "the knockouts run in CI" (F-7) | no — derived from `.github/workflows/*.yml` | a claim without a step fails the test | the workflow file | no | empty walk and workflow-less tree both fail; it found a false positive in itself | **PASSES AS A CLAIM-CHECK.** Whether a runner runs them is still unverified (T11/T12) |
| `doc_claims` document set | exceptions only | forgetting a document is red | the file system | no | stale exceptions fail too | **PASSES AS A DEFAULT**; `STATUS_M5` is still excepted, which is limitation 36, and delta-1 raised its price from desirable to obligatory |

**Two rows do not pass cleanly, and both are named above rather than argued
away.** `audit()` is an enumeration whose completeness is now measured instead of
assumed — that is a weaker claim than the compiler judging it, and it is the
claim I am making. `transaction_for` selects by outcome and still does; what
delta-1 added is the count and the owner, which is F-0058's own rule and not its
closure.

The pattern the reviewer named holds and I have no counter-example to it: where
the judge is the compiler, a type, an exhaustive sweep, or an independently
constructed second answer, the mechanisms held under three cold attacks. Where
the judge is a hand-written enumeration or the outcome itself, the milestone
repeated the class it documents — twice inside mechanisms built during M5 to
close that very class.

**STOPPED AFTER M5 DELTA-1 — M5.5 NOT STARTED.**


---

# Addendum 2 — delta-2 (C255–C258)

`REDTEAM_M5` addendum 1 **FAIL**, `REVIEW_M5_A` addendum 1 **REJECT**,
`REVIEW_M5_B` addendum 1 **ACCEPT WITH CONDITIONS**. Two of three say NOT MET,
so the gate is not met. This addendum records delta-2; the body and addendum 1
stand as signed.

## A2.1 The blocker: one step up the same provenance graph

`Boundary::owners` is computed in `assemble` as
`face_at(&face_of_padded_px, ...)` — a SAMPLE of the field, two pixels per
chain. So delta-1's "third independent construction" sits **downstream of what
it certifies**, and `crossing.rs`'s own independence row — *"would survive a
corrupted `face_of_padded_px` | it never looks at it"* — is true of the FUNCTION
and false of the CHECK.

Reviewer A established the residual class exactly, by publishing a **refuted**
hypothesis first: moving the red team's rotation above the sampling point IS
caught, because a global rotation moves the exterior off id 0 — the
construction's only external bit. A corruption respecting that bit passed
everything, with **529 of 1089 pixels** in a face whose label was not theirs.

The remedy needed no third construction:

```text
for every pixel p:  faces[face_of_pixel(p)].label == labelling.inside()[p]
```

**The labelling is the input.** It is not derived from the map, the owners or
the faces, so it is the one comparison in the audit whose two sides do not share
a provenance — and the one thing no M5 predicate read. Reviewer A's note is the
sharpest statement of the cost: *"Had I judged E2b with the milestone's own
instruments I would have concluded it was correct."*

**This refutes my own F-0048 row for `audit()`.** I wrote that it "PASSES ONLY
BECAUSE IT IS BOUNDED — a field with no predicate shows up as an uncaught slot".
The bound is over *perturbations of a correct `Parts`*. A defect inside
`assemble` produces a self-consistent wrong value that is not a perturbation of
anything and never enters the walk at all. Reviewer B falsified the same formula
from the other side — bounded only on the non-empty branch.

## A2.2 What else delta-2 closed

| finding | what it was | closed by |
|---|---|---|
| **RT5-A9 / M5A-D1-N1** | the cross-check derived from what it checks | the labelling anchor, plus the corruption as a gate knockout with a two-sided control |
| **M5B-N11** (a) | the judge returned before the map check on the empty branch | the anchor runs before every branch; the empty branch executes the comparison |
| **M5B-N11** (b) | clause 4's green rested on arm ORDER — 8 empty arms at 87..96, stride 17 hitting 86 and 103 | the probe takes the first arm of EACH branch deterministically; both counts published |
| **RT5-A10** | 96.34 % of the slots are one family whose catch is guaranteed by the check's shape | `by_family` is in the artifact; the total is no longer offered alone |
| **RT5-A11** | a new field still cost one line `_` | the site count is compared against the scalar-leaf count of the SERIALIZED `Parts`, which `extra: _` cannot move |
| **M5A-D1-N2** | `path[j].1` never perturbed, and delta-1 reported it fixed | perturbed; and the false claim is recorded as F-0067 rather than quietly corrected |
| **M5B-N12** | `let _ = west;` under a comment saying it was checked | implemented, which also gives the rebuild self-standing completeness over the owners |
| **M5A-D1-N3 / M5B-N13** | stride comment, "131 070", limitation-18 premise (both sites), CI-checker bypass price | all four; the gate-file site travelled alone (C256) |

**Found by delta-2's own new mechanism, before any reviewer:** the first full run
with the deterministic probe reported **26 idle slots** in the new left-owner
site — on a two-face arrangement it computed its own input. That is F-0059's
counter firing in the very site F-0059 was written about, one milestone later.

## A2.3 Corrections to addendum 1

| said | is |
|---|---|
| A1.6, `audit()` row: "PASSES ONLY BECAUSE IT IS BOUNDED" | **the bound does not cover the class RT5-A1 belongs to.** It is over perturbations of a correct `Parts`; a defect inside `assemble` is not a perturbation. Falsified from two directions (A: non-perturbative defects; B: the empty branch) |
| A1.6, `dcel::crossing` row: "Q4 — no" | **YES.** The guard reads a quantity sampled off the map it checks. Q4 was the one question of the five I did not ask about that mechanism |
| C250's message: `path[j].1` among the fixes | **it was not fixed.** The edit never landed; see F-0067 |

## A2.4 New limitations

42. **The rebuild is a check on the COPY, not on the value**, and it stays — it
    caught RT5-A1, it is the only check over the owners' geometry, and since
    delta-2 it compares the west owner too. What anchors the arrangement is the
    per-pixel comparison against the labelling. The residual class the rebuild
    reproduces is stated exactly in `crossing.rs`: every permutation of face ids
    fixing the exterior.

43. **The CI checker greps line-wise**, so a COMMENTED-OUT line carrying
    `--test <name>` and `--ignored` satisfies it; it covers only
    `crates/*/tests/`; and it verifies that a step NAMES a target, not that a
    runner executes it. Named in its doc comment. **Owner M6.**

44. **Limitation 37 must not be deferred twice.** Reviewer A checked the price
    and found it honest — `EditKind`'s variants are in no signed artifact, so
    widening the enum does not drag §27.1 in — and said plainly that the
    compound transaction is what §28 M5 names. **Owner M6, and it is the one
    carried item with no second deferral available.**

## A2.5 F-0048 after delta-2, with Q4 re-read as PROVENANCE

Q4 is no longer "does the guard share a KEY with the mechanism" but **"does the
guard share a PROVENANCE with it"** — the re-reading delta-2 was bought with.

| mechanism | Q1 literal | Q2 next finding | Q3 judge | **Q4 provenance shared?** | Q5 both ways | verdict |
|---|---|---|---|---|---|---|
| representation invariants (six of §12) | no | criterion changes | compiler | no | yes | **PASSES** |
| `audit`'s labelling anchor (new) | no | criterion changes | per-pixel comparison | **no — the labelling is the input** | RT5-A9 red, clean green | **PASSES** |
| `crossing::face_map_agrees` | no | criterion changes | an independent traversal | **YES, through `owners`** | RT5-A1 red, RT5-A9 reproduced | **DOES NOT PASS Q4**, residual class stated exactly |
| `Parts::perturbations` | no | the leaf count moves and the test fails | compiler **and** the `Serialize` derive | no | red, empty, idle | **PASSES** |
| `audit()` | **YES — hand-written blocks** | "append a block" | habit, bounded by `uncaught == 0` | — | bounded only over PERTURBATIONS of a correct value | **DOES NOT PASS.** The bound's domain is named now instead of claimed |
| `RunKnockouts` | no — counted against `gate_table()` | a clause without a knockout fails | the gate table | no | each reddens its own row | **PASSES** |
| `transaction_for` | **YES — `_ => return None`** | "the fifth kind is dropped" | the outcome | yes | excluded count published | **DOES NOT PASS.** Limitation 37, owner M6, no second deferral |
| `Threshold::from_gates` | no | type error | compiler | no | yes | **PASSES** |
| CI claim checker | no | a claim without a step fails | the workflow file | no | both, incl. its own false positive | **PASSES**, bypass price named |
| `doc_claims` doc set | exceptions only | forgetting is red | the file system | no | stale exceptions fail | **PASSES as a default** |

**Three rows do not pass, and all three are named rather than argued away.** Two
of them — `crossing` and `audit()` — are what delta-2 was written about, and what
changed is that their limits are measured and stated instead of claimed.
`crossing` is kept because a check on the copy is worth having once you know
that is what it is.

**STOPPED AFTER M5 DELTA-2 — M5.5 NOT STARTED.**


---

# Addendum 3 — delta-3 (C260–C262)

`REDTEAM_M5` addendum 2 **FAIL**, `REVIEW_M5_A` addendum 2 **ACCEPT WITH
CONDITIONS / GATE MET**, `REVIEW_M5_B` addendum 2 **ACCEPT WITH CONDITIONS /
GATE MET**. §34 makes a separate red-team pass mandatory for M5, so one FAIL is
the gate.

## A3.1 The blocker: §12 asks for closed AND oriented; half was held

`target(h) == origin(next(h))` — the property that makes a loop a walk rather
than a bag — was checked nowhere, and `Dcel::target` and `Dcel::origin` were
called by **nothing** in the workspace. Swapping two half-edges inside one loop
violates it on **35 768 of 131 072** 4×4 arrangements, with `audit()` returning
`Err` **zero** times, the exhaustive sweep green, the gate at EXIT 0 and the
artifact byte-identical.

Both delta-2 anchors reproduce it and this is asserted rather than described: the
labelling anchor moves no pixel under a reordering, and `crossing` reads only
`boundaries`. The exhaustive sweep exhausts the INPUT domain, which says nothing
about a property no predicate evaluates — the eighth level of the F-0048 class,
a **third** time, now on `next`.

**Q4 was asked first, as required.** The obvious check —
`target(h) == origin(next(h))` — shares a provenance with what it checks:
`target`/`origin` read `boundaries[].start/end`, `next` reads `site` and
`faces[].loops`, all outputs of the same `assemble`. Writing it would have been
RT5-A9's shape a third time. So the loops are **re-derived from the labelling**
and compared as cyclic lattice walks. Residual, stated in `loops.rs`: this shares
the ALGORITHM with `assemble` and not the DATA, so corruption of the stored loops
is caught and a wrong `succ` rule is not — that is what the sweep and the Euler
identity are for, and they share no algorithm with it.

**And the fixture, because a check whose population cannot exercise it is green
for the old reason.** Loops of ≥3 half-edges did not exist in either M5
population: the corpus averages 1.082 per loop, the structural register had
**zero**. A sixth fixture, `diagonal_staircase`, puts a degree-four vertex
between each consecutive pair of blocks and so carries loops of three or more at
every size and under both arms, BY CONSTRUCTION. The register's derived counts
are now computed from the register rather than written as `5 * 2`, which is the
line the sixth fixture would otherwise have broken.

## A3.2 The §12 claim, corrected

The table in `dcel/mod.rs`, STATUS §1 and ADR-0031 §1 claimed **closed and
oriented** from one argument: "a loop is a `Vec<HalfEdgeId>` traversed modulo its
length". That establishes CLOSED and says nothing about ORIENTED — the right
half-edges in the wrong order are still a cycle. **One of the six invariants
declared unrepresentably-false was representably false in half of what §12
asks.** The table now carries two rows: closed (no failure mode) and oriented
(a computation, and it was unrepresented until delta-3).

## A3.3 RT5-A14: the anchor is invisible to the instrument, and the row says so

Switching the labelling anchor off moves no slot count, no `by_family` entry and
no artifact byte. Reviewer A's diagnosis is exact and I adopt it: the walk is
made of perturbations of a **correct** structure, and the anchor's whole domain
is defects **inside** `assemble`, which are not perturbations of anything
(F-0066). This is a boundary of the instrument, not a defect in the anchor.

**Choice taken: the row says it plainly, and the anchor is guarded by
KNOCKOUTS.** RT5-A1 and RT5-A9 are gate-level controls required to redden clause
4, so the anchor is not unguarded — it is guarded by tests rather than measured
by the walk, and the row now distinguishes those two things instead of letting
one stand for the other.

**Measured here rather than quoted**, because the row cites it: on a 13×13
annulus the rebuild alone catches **153** slots the anchor does not, the anchor
alone catches **3**, and **160** fall to both. So the two are not redundant and
citing both is not citing one twice.

*Price of the alternative I did not take*, named as the governor asked: a second
instrument whose population is defects INSIDE `assemble` means a harness that
edits `assemble`, rebuilds the crate in a temporary clone and re-runs the gate —
the source-rewriting harness the red team costed at a milestone of its own. It
is the honest form and it is not affordable here. **Owner M6**, and until then
the anchor's guarantee is exactly as strong as its two knockouts.

## A3.4 RT5-A12 / M5B-N14: the leaf judge keyed on the serialization

`#[serde(skip)]` plus `extra: _` — **two lines**, clippy clean, everything
green, the field invisible to the walk, to the leaf count and to the artifact.
The judge's key is the `Serialize` derive, an attribute on the same surface the
attacker edits in the same commit. B's rule: "a count the attacker does not
control" is verified by ENUMERATING the ways to control it, because a derive is
a function of attributes on the thing checked rather than a constant of nature.

The ways are enumerated: no serde attribute may appear on a field of `Parts`,
checked over the struct's own source, with a positive control that the derive is
still present. **Residual, at the cheapest known price:** it is a text scan,
which F-0048 Q3 calls a habit rather than a judge — renaming the struct, moving
it, or spelling the attribute differently defeats it. What it closes is the
two-line bypass that was measured. A judge sharing no surface with `Parts` needs
reflection Rust does not have; the nearest thing is a proc-macro deriving both
the sites and the count from one definition, which is a new crate. **Owner M6.**

**Corrections to addendum 2:** A2.2 and A2.5 say `extra: _` "cannot move the
leaf count". That is true of `extra: _` alone and false of the two-line form,
and the two-line form is the one that matters.

## A3.5 M5B-N15: the branch counters are gated, and the branch set is derived

The counters were published and gated by nothing, so a commit deleting the branch
probe and re-recording the artifact — which is **not** under §27.7 — would
silently return clause 4 to stride-dependence. They are conjuncts now: every
branch the judge reported must have been probed, and there must be at least two.

And the branch set was a hand-written dichotomy computed by the CALLER
(`count_inside() == 0`), so a new early return inside `audit` cost one line there
and the probe would never have learned of it. **The judge names its own branch**
in `AuditReport::branch`, and the harness buckets by whatever comes back, so a
third branch is probed the first time it appears.

## A3.6 New limitations

45. **`loops_agree_with_the_labelling` shares an ALGORITHM with `assemble`**, and
    that is its stated residual: a wrong `succ` rule produces the same wrong
    loops on both sides. It shares no DATA, which is the class RT5-A13 lives in.
    The `succ` rule is covered by the exhaustive sweep and the Euler identity,
    which share no algorithm with it. **Owner: none — this is the boundary, not
    a defect.**

46. **The serde-skip ban is a text scan.** Named above with its price. **Owner
    M6.**

47. **No second instrument for defects inside `assemble`.** A3.3. **Owner M6.**

## A3.7 F-0048 after delta-3, Q4 as PROVENANCE, new check first

| mechanism | Q1 literal | Q2 next finding | Q3 judge | **Q4 provenance shared?** | Q5 both ways | verdict |
|---|---|---|---|---|---|---|
| `loops_agree_with_the_labelling` **(new, asked first)** | no | criterion changes | re-derivation from the input | **no data; YES algorithm**, and both are stated | RT5-A13 red, clean green, and the fixture that makes the red possible is asserted at every size | **PASSES on data-provenance; the algorithm share is named** |
| `audit`'s labelling anchor | no | criterion changes | per-pixel comparison | no — the labelling is the input | guarded by two gate knockouts; NOT measured by the walk, and the row says so | **PASSES** |
| `crossing::face_map_agrees` | no | criterion changes | an independent traversal | **YES, through `owners`** | RT5-A1 red; RT5-A9 reproduced; 153 unique slots measured | **DOES NOT PASS Q4**, residual class exact |
| `Parts::perturbations` | no | leaf count moves | compiler + `Serialize` derive + a source scan | **YES — the derive is an attribute on `Parts`** | red, empty, idle | **DOES NOT PASS Q4.** Closed for the measured bypass; the scan's own price is named |
| `audit()` | **YES — hand-written blocks** | "append a block" | habit, bounded over perturbations only | — | domain of the bound is named | **DOES NOT PASS** |
| the branch probe | **no longer** — the judge names its branches | a new branch gets a bucket | the judge's own output | no | gated by clause 4 | **PASSES** |
| `RunKnockouts` | no — counted against `gate_table()` | a clause without a knockout fails | the gate table | no | each reddens its own row | **PASSES** |
| `transaction_for` | **YES — `_ => return None`** | "the fifth kind is dropped" | the outcome | yes | excluded count published | **DOES NOT PASS.** Limitation 37, owner M6, no second deferral |
| representation invariants | no | criterion changes | compiler | no | yes | **PASSES** — for the five that remain after §12's ORIENTED clause left the column |
| `Threshold::from_gates` | no | type error | compiler | no | yes | **PASSES** |
| CI claim checker | no | a claim without a step fails | the workflow file | no | both, incl. its own false positive | **PASSES**, price named |
| `doc_claims` doc set | exceptions only | forgetting is red | the file system | no | stale exceptions fail | **PASSES as a default** |

Four rows do not pass, each with a named residual and an owner. Two of them —
`crossing` and `Parts::perturbations` — share a provenance with what they check
and are kept because their measured contribution is real and now quantified.

**STOPPED AFTER M5 DELTA-3 — M5.5 NOT STARTED.**
