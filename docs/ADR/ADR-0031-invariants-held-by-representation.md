# ADR-0031 — The §12 invariants are held by the REPRESENTATION, and the one that is not says so

Status: accepted (M5, C238)
Spec: v1.3 §12, §5.3, §5.4, §11.4, §28 M5, §32 rules 6 and 7, §4, §4.1

## Context

§12 lists seven invariants for the shared planar graph and then says the thing
that decides the architecture: a wrong state must not be *caught*, it must not
**assemble**. M1 already implements the same seven as a post-hoc validator
(`vice_ir::validate::GraphError`), and it had to: M1's `PlanarGraph` has public
fields and accepts graphs from outside, so validation was the only place the
invariants could live.

M5 is not in that position. It builds the arrangement itself, from a labelling.

The standing hazard is FAILURE_LEDGER F-0048: a mechanism that closes the
presented instance and leaves a place where the next line is appended. Seven
consecutive instances in M4.5. Its first question — *is there a literal in the
mechanism enumerating its subjects?* — answers **yes** for any design where the
seven invariants are seven checks, and the next finding is an eighth check.

## Decision

**1. The invariants are properties of the representation, not entries in a
checker.** Six of the seven have no failure mode to check for:

| §12 invariant | how it is held | can a value violate it? |
|---|---|---|
| every half-edge has a twin | `HalfEdgeId::twin` is `id ^ 1` | no — there is no twin field |
| every interior boundary has two owners | `Boundary::owners: FacePair` | no — two owners is the shape of the record |
| border boundary has an interior and an exterior owner | the padding ring is background, so the exterior is an ordinary face of the ordinary walk | no |
| face cycles closed and oriented | a loop is a `Vec<HalfEdgeId>` traversed modulo its length | no — a cyclic traversal has no open state |
| no dangling cracks | `FacePair::new` returns `None` on equal ids and is the only mint | no |
| non-adjacent boundaries do not intersect | segments are unit steps of an INTEGER lattice | no |
| Euler / cubical signature preserved | `dcel::audit` | **yes**, and that is why it is the audited one |

The answer to F-0048 Q2 ("what happens at the next finding?") is therefore *the
criterion has to change*, because there is no list to extend.

**2. `Dcel::assemble` is total and is the only constructor.** It takes a
labelling and a convention and returns a `Dcel`, not a `Result`. There is no
invalid outcome to represent. Fields are private.

**3. The seventh invariant is an instrument, and its resolving power is
measured — including where it is weak.** `dcel::audit` runs the construction
invariants; the honest question is what it can see. The control is
`Parts::perturbations`, which exhaustively DESTRUCTURES the derived structure
and emits one perturbation per scalar slot of the actual data. A field added
without a site does not compile — the judge is the compiler, which is the form
F-0048 lists as good and the form `TopologyGateConfig::sites` already uses.

Measured on a 13×13 annulus, and the split is published rather than summed,
because summing it would overstate one of the two checks:

```text
slots 312 | caught by audit 86 | caught by assembly-equality 312
```

**`audit` alone catches 86 of 312 — 28 %.** That number is the point of taking
the measurement. Most slots are entries of `face_of_padded_px`, and moving one
pixel's face id inside a large uniform face changes nothing any construction
invariant can see; it is caught by `is_the_assembly_of_its_own_labelling`, which
compares against a fresh assembly. So the two checks are reported separately and
the §28 M5 clause "no dangling/invalid faces" cites both, because a row citing
only the first would claim a resolving power the first does not have.

The other direction is asserted in the same test: the unperturbed value passes
both checks, and the walk must find more than 200 slots. A checker that always
failed, and a walk that found nothing, would each satisfy half of this and
measure nothing (F-0039).

**4. Robust predicates are obtained by choosing the representation (§5.4).**
The combinatorics is decided by comparisons of `u32` lattice coordinates and
`bool` labels. There is no orientation test to make adaptive and no tolerance to
type, because there is no float in the decision path. §5.4's list applies to
M6's fitted curves, and the four §12 isotopy conditions for curve replacement
are a typed refusal naming M6 (`certificate::curve_replacement_isotopy`).

**5. The proof domain is built to cover the application domain, on three named
axes.** F-0054 / F-9 is the class where an instrument proved on hand-written
witnesses decides questions outside them.

- *exhaustive*: every labelling of 4×3 (unit tests, default path) and of 4×4
  (`#[ignore]`, release CI) under both arms — 131 070 arrangements, 11
  topological classes, 41 678 labellings carrying a critical 2×2;
- *size*: 32/64/128 by default, 256 and 512 under `--ignored`, so the DCEL has
  no gap of the F-8 kind;
- *structure*: five fixtures at every size (condition 51).

**6. New crate: none.** §4's target structure names `vice-topology` as "event
trees, hypotheses, RAG, **planar graph**, signatures", and §4.1 forbids crates
created outside that structure. A `vice-dcel` crate is not in §4 and would need
this ADR to argue against the spec rather than from it. The DCEL is
`vice_topology::dcel`, four modules, each under the §4.1 size rule.

*Price of the alternative, stated so the decision is not free:* if a later
milestone needs the arrangement without the event trees (M7's optimizer is the
plausible one), extracting `dcel` into its own crate is a mechanical move of
four files with no logic change — one commit, and `vice-topology` then depends
on it. That is cheaper than carrying a crate §4 does not name for six
milestones.

## The measurement that changed the design

The first version made the pairing at a critical 2×2 depend on which label is
8-connected — the reading that sounds like the textbook. The exhaustive sweep
rejected it: on the 4×3 labelling `18` under foreground-4, the Euler identity
`V − B + L = 2C` came out `1 − 2 + 4 = 3` against `2`, because a background that
must walk both squares in one loop walked them in two.

The correct rule is unconditional, and the reason is worth recording because it
is a fact about the subject rather than about the code: **the boundary segments
are a function of the labels alone**, so the convention cannot move one. What
the convention decides is which pixels are one region — a statement about
faces. Both arms share one curve system and differ in their face structure, and
`the_two_conventions_disagree_about_a_critical_2x2` asserts that they do differ,
so the convention is not decoration.

A rule that sounds right and a rule that survives the whole input space are
different objects. Only the second is in the tree, and the sweep is what told
them apart.

## Consequences

- `vice_ir::validate` keeps checking the same list, for the graphs it receives
  from outside. The two are not redundant: one validates, the other constructs.
- A transaction is atomic because `Dcel` is immutable — rollback is dropping a
  candidate. The price is a full rebuild per attempt rather than the incremental
  rebuild §11.4 implies; recorded as an M7 obligation with its price in
  `docs/STATUS_M5.md`.
- `crate::continuation`'s plan-level refusals still say the DCEL is absent. They
  describe what the ENVELOPE planner alone can do, their counts are frozen into
  `docs/gt/TOPOLOGY_M4_5.json`, and §27.1 forbids moving a signed artifact in a
  feature commit. The executor is `dcel::transaction`, and the gap is recorded
  in `docs/STATUS_M5.md` with an owner and a price.


---

## Erratum — delta-1 (C252)

Accepted ADRs receive an erratum rather than a silent edit, and three of these
were quoted back at me from three independent cold contexts.

1. **"131 070 arrangements"** (§ the three axes) — it is **131 072**, and 4×3 is
   **8192**, not 8190. C243 stopped skipping the two empty labellings; its
   commit message recomputed the arrangement counts here and this file was never
   touched.
2. **"11 topological classes"** — it is **12**. The twelfth is `(0, 0)`,
   contributed by exactly the labelling C243 stopped skipping. C243 corrected
   two of the four numbers it invalidated; this is one of the other two.
3. **"four modules" / "four files"** (§6) — the `dcel` module tree is **seven**
   files. STATUS_M5 §1 says seven, so the two documents disagreed about the
   price of the very decision they jointly justify, and §32 requires that
   decision to be taken on that price.
4. **The proof-domain claim of §5 was false when written.** "There is no
   subclass left for a defect to hide in" was written while
   `audit_every_labelling` did `if l.count_inside() == 0 { continue }`. F-0058
   records it; the ledger's Status field said it was "corrected in ADR-0031
   there", and C243 never touched this file (REVIEW_M5_B N6). It is corrected
   HERE, which is the first time.
5. **§6 argues only half of §4.** §4 also names `vice-opt` as "continuous
   optimizer + **discrete transactions**", and `dcel::transaction` is a discrete
   transaction. The placement is still defensible — §11.4 is Stage D, §19's
   compound discrete search arrives at M7, and `vice-opt` does not exist — but
   an ADR that exists to argue FROM the spec owed the half of §4 that points
   elsewhere (REVIEW_M5_A N13). The transaction module moves to `vice-opt` when
   `vice-opt` is created, at the same mechanical price as the DCEL.
6. **The residual of the one-constructor claim was mispriced.** §2 says the
   cheapest bypass is "one `pub fn` in `vice-topology` taking pieces". It is
   cheaper: `walk::with_parts` is `pub(crate)`, already exists, and `Parts`
   fields are `pub(crate)` beside it, so three lines in any module of the crate
   build a `Dcel` with no faces at all — and `audit` panicked on that one
   (REVIEW_M5_A N5). The guard is added; the price is restated at what it is.
7. **What delta-1 changed in the substance.** §3's measured split — "audit 86 of
   312, 28 %" — is obsolete. `dcel::crossing` is a third construction of the
   face map, and the audit now rejects **every** perturbed slot: 312 of 312 on
   the annulus, 155 160 of 155 160 on the corpus run. The 28 % was the number
   REDTEAM_M5 RT5-A1 walked through: it was published as an honest weakness and
   what it said is that `face_of_padded_px`, the largest field, was read by no
   predicate at all.
