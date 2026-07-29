# M7 obligation and acceptance map

Status: active implementation contract for milestone M7.

Normative source: `docs/spec/VICE_CLASSIC_CORE_AGENT_SPEC_v1.3.md`.

This map is not a replacement for the specification. It makes every M7
obligation, inherited debt, executable judge, and release artifact explicit so
that implementation and reviews cannot silently narrow the milestone.

## Acceptance rule

M7 is accepted only when every row below is either `MET` or is an explicit
hard blocker in the failure ledger. A unit-tested branch without a production
call site is not `MET`. A report-only simulation of a production transition is
not `MET`. A claim without an executable judge and replayable artifact is not
`MET`.

The final release-candidate commit must be clean and immutable while it is
examined by:

1. two independent cold reviewers;
2. the mandatory independent M7 numerical/topology reviewer;
3. the release-quality audit.

All four reviews must name the same exact commit SHA. Any implementation or
evidence edit invalidates all prior reviews.

## Product and architecture

| ID | Obligation | Production owner | Executable judge | Required evidence |
|---|---|---|---|---|
| M7-01 | Activate real `vice-opt`, `vice-verify`, `vice-svg`, and `vice-core` responsibilities; no empty placeholder API | workspace | workspace tests plus public API call-site scan | source manifest and test inventory |
| M7-02 | Bind every posterior and confidence result to the current supported-model universe and pricing identities | `vice-opt`, `vice-core` | stale-identity refusal and replay tests | universe, pricing, backend, and config hashes |
| M7-03 | Preserve first-class observed-chain to DCEL-boundary identity through fit, topology edits, local isotopy, verification, and export | `vice-core`, `vice-topology`, `vice-fit` | identity survival and mismatch-refusal tests | identity trace in scene/report |
| M7-04 | Provide the production `vicec vectorize ... --mode flat2` path; it must return a typed success, ambiguous, unsupported, or failed result rather than silently falling back | `vice-cli`, `vice-core` | end-to-end CLI tests | §30 output bundle |
| M7-05 | Keep the ground-truth scene builder and oracle machinery independent from the production builder | `vice-bench`, `vice-core` | dependency and source-role tests | component-role inventory |

## Likelihood, posterior, and supported search

| ID | Obligation | Production owner | Executable judge | Required evidence |
|---|---|---|---|---|
| M7-06 | Score the final serialized/rendered full-resolution image, including observed bytes and quantization behavior | `vice-opt`, `vice-render` | perturbation and serialized-render score tests | full-resolution likelihood trace |
| M7-07 | Use an allowed correlation-aware residual model with residual-model identity, correlation length, conditioning, and calibration diagnostics; iid is diagnostic only | `vice-opt` | correlated-residual calibration tests | residual diagnostics artifact |
| M7-08 | Prevent double counting between topology/geometry evidence and pixel likelihood by declaring disjoint score ownership | `vice-opt`, `vice-core` | term-ownership and duplicate-term rejection tests | decomposed `L_total` ledger |
| M7-09 | Implement `L_total`: pixel NLL plus topology, geometry, paint, relation, and formation description lengths | `vice-opt` | recomposition and finite-score tests | per-term score rows |
| M7-10 | Account for supported-model universe, enumerated search mass, budget-pruned mass, and a conservative unexplored-mass bound | `vice-opt` | exact small-universe and truncation tests | posterior/search-mass certificate |
| M7-11 | Bind confidence to delivery-equivalence classes rather than in-memory hypothesis identity | `vice-opt`, `vice-verify` | equivalent-serialization aggregation tests | equivalence-class posterior table |

## Continuous and discrete optimization

| ID | Obligation | Production owner | Executable judge | Required evidence |
|---|---|---|---|---|
| M7-12 | Implement scaled trust-region optimization with projected parameter blocks, fixed mesh inner solves, exact backtracking, and deterministic initialization | `vice-opt` | quadratic, projection, and deterministic-replay tests | optimizer trace |
| M7-13 | Recompute each block's current parent after earlier accepted blocks; snapshot the tested scope; compare parent/child under identical cache and approximation state | `vice-opt` | stale-parent regression and cache-identity tests | per-block acceptance trace |
| M7-14 | Use ROI plus halo correctly and perform periodic exact full-scene checks that can reject local false improvements | `vice-opt`, `vice-render` | adversarial outside-ROI tests | local/full score reconciliation |
| M7-15 | Re-solve relation-constrained geometry in the optimizer, including the normal-direction residual/Jacobian contract inherited from M6 | `vice-opt`, `vice-fit` | constrained-solve and finite-difference Jacobian tests | relation solve trace |
| M7-16 | Search complete compound transactions: anchors, split/merge/refit, family/corner/primitive/relation edits, topology edits, and paint/exterior edits | `vice-opt`, `vice-topology` | transaction atomicity and rollback tests | transaction inventory |
| M7-17 | Support scene-level mirror/repetition hypotheses and preserve the M6 formation-stability envelope across topology/formation hypotheses | `vice-opt`, `vice-core` | mirrored/repeated-scene recovery tests | formation posterior rows |
| M7-18 | Replace whole-arrangement-only transaction handling with incremental DCEL rebuilding and prove agreement with a full rebuild on the corpus | `vice-topology`, `vice-opt` | incremental-versus-full differential tests | differential summary |
| M7-19 | Keep a diverse deterministic quality beam with topology/formation quotas, memoized canonical hashes, explicit time/memory/evaluation budgets, and delivery-equivalent-only tie collapse | `vice-opt` | diversity, budget, memo, and tie tests | beam/search trace |
| M7-20 | Thread the M5 shape-knockout variant through real measurements and populate all refusal paths instead of report-only mutation or unit-only coverage | `vice-bench`, `vice-core` | intervention and population tests | refusal population table |

## Verification, quantization, and SVG delivery

| ID | Obligation | Production owner | Executable judge | Required evidence |
|---|---|---|---|---|
| M7-21 | Run pre-seal combinatorial checks: DCEL validity, face/exterior ownership, shared-neighbor identity, formation validity, and model admissibility | `vice-verify` | corrupt-scene refusal suite | pre-seal certificate |
| M7-22 | Run geometric checks: intersections, winding, local isotopy, G1 relations, primitive constraints, and topology invariants | `vice-verify`, `vice-topology` | adversarial geometry suite | geometry certificate |
| M7-23 | Quantize shared geometry exactly once, reconstruct it, and rerun DCEL/intersection/G1/isotopy/native-primitive admissibility checks | `vice-verify`, `vice-svg` | quantization-collapse and neighbor-identity tests | post-quantization certificate |
| M7-24 | Build a canonical export plan and canonical scene/report/SVG bytes; independently parse the serialized SVG, render it, compare it to the certified render, and seal all digests | `vice-svg`, `vice-verify` | round-trip, parser-independence, render-equivalence, and tamper tests | sealed delivery manifest |
| M7-25 | Export both PurePartition and SeamSafe profiles; SeamSafe aprons must use shared-edge ownership, deterministic z-order and width, and must never leak at gaps, exterior boundaries, or junctions | `vice-svg` | profile parser/render tests and seam adversaries | both SVGs and profile diagnostics |
| M7-26 | Compare delivery in independent render paths and refuse the seal when serialized output changes the certified result | `vice-svg`, `vice-verify` | cross-renderer differential tests | renderer comparison rows |

## Confidence, abstention, and release SLOs

| ID | Obligation | Production owner | Executable judge | Required evidence |
|---|---|---|---|---|
| M7-27 | Calibrate typed `success`/`ambiguous` confidence against the current universe; truncation and unexplored mass must reduce confidence | `vice-opt`, `vice-core` | calibration, truncation, and stale-universe tests | calibration artifact |
| M7-28 | Make every unsupported condition and verifier failure abstain explicitly; no hidden raster, polygon, iid, renderer, or stale-artifact fallback | `vice-core`, `vice-cli` | forced-capability and forced-failure tests | fallback/refusal matrix |
| M7-29 | Meet the clustered source-level selective catastrophic-risk upper bound and per-source/per-render coverage gates on the untouched sealed Flat2 audit, with at least 459 accepted source groups at 99% confidence | `vice-bench` | reliability harness | signed risk/coverage artifact |
| M7-30 | Freeze measured boundary p95, p99, and max tail gates and meet them without accepted self-intersection, G1, topology, or delivery corruption | `vice-bench`, `configs/GATES_V1.toml` | boundary-tail and catastrophic-defect gates | frozen gate provenance and audit rows |
| M7-31 | Beat the best internal baseline on boundary tails and catastrophic defects without uncontrolled complexity growth | `vice-bench` | paired clustered comparison | baseline comparison artifact |
| M7-32 | Pass the statistical blind court on untouched sealed inputs | `vice-bench` | blind-court runner | sealed verdict artifact |
| M7-33 | Publish refusal counts by family, scale, renderer, and source group, including the M6 cost-refusal histogram and numerical-conditioning diagnostics | `vice-bench`, `vice-opt` | population and accounting tests | refusal/conditioning tables |

## Oracle, recovery, determinism, and governance

| ID | Obligation | Production owner | Executable judge | Required evidence |
|---|---|---|---|---|
| M7-34 | Rerun the complete Flat2 geometry oracle: PF00/PF10/PF01/PF11 and G00/G10/G01/G11/G20/G30 | `vice-bench` | complete oracle matrix | per-cell results and aggregate verdict |
| M7-35 | Pass controlled recovery for both G20 and G30, including recovery mode accounting and refusal integrity | `vice-bench`, `vice-core` | recovery harness | recovery rows |
| M7-36 | Demonstrate deterministic bytes and decisions across repeated runs and supported worker counts | all production crates | byte/trace replay tests | determinism artifact |
| M7-37 | Enforce declared time, memory, hypothesis, and render budgets; budget exhaustion must produce a typed result with honest posterior mass | `vice-core`, `vice-opt` | resource-cap tests | budget ledger |
| M7-38 | Separate runner trust anchor from local HEAD, detect substituted Git/tool paths, split gate-file roles by type, and parse asserted structured provenance rather than trusting comments | `vice-bench`, governance | adversarial runner/governance tests | trust and provenance report |
| M7-39 | Keep every release claim replayable from the exact clean commit with canonical config, artifact, backend, universe, pricing, export, and renderer identities | governance | artifact replay and clean-tree checks | M7 canonical artifact |
| M7-40 | Update traceability, reproducibility, status, failure ledger, and deferred-debt inventory without silently carrying an M5/M6 M7-owned item forward | docs | traceability/debt completeness tests | M7 documentation set |

## Required §30 output bundle

For a successful production invocation:

```text
vicec vectorize input.png --mode flat2 --intent clean --preset quality --out out/sample
```

the sealed output directory must contain at least:

```text
result.svg
result.pure-partition.svg
result.scene.json
result.export-plan.json
result.report.json
result.render.png
result.seal.json
```

Ambiguous, unsupported, failed, or exhausted runs must still write a canonical
report that contains the typed outcome, identities, budget ledger, refusal
reason, and any unsealed diagnostics. They must not publish a success SVG.

## Freeze and review sequence

1. Implement against development and calibration data.
2. Run targeted numerical, topology, quantization, delivery, and fallback
   adversaries.
3. Measure and freeze M7 gates in config-only commits.
4. Create the canonical M7 release artifact in an artifact-only commit.
5. Update documentation in documentation-only commits.
6. Run the final clean-tree workspace and replay matrix.
7. Open the untouched sealed audit once for the release candidate.
8. If the audit fails, record the failed opening, burn/rekey the sealed split,
   fix on development/calibration data, and repeat the release sequence.
9. Obtain all four independent reviews on the same exact clean SHA.

