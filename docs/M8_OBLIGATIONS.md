# M8 obligation and acceptance map

Status: generation 6 burned by independent review; generation 7 author audit in progress.

Normative authority remains `docs/spec/VICE_CLASSIC_CORE_AGENT_SPEC_v1.3.md`, in
particular sections 1.1, 6.1, 6.4, 9.3, 11.5, 12, 16.1, 17, 19, 27.6 and the
M8 milestone in section 28. This map makes the M8 conjunction executable; it
does not narrow the specification.

M8 is complete only when every row is `MET` on one clean candidate SHA. A
typed refusal is acceptable only outside the declared M8 universe or when a
calibrated ambiguity/unsupported condition is actually present. A unit-only
branch, oracle-only override, copied aggregate, or report counter is not
production evidence.

| ID | Obligation | Owner | Required judge | Release evidence |
|---|---|---|---|---|
| M8-01 | Version and canonically hash the expanded multiregion supported-model universe; stale Flat2 calibration must not authorize multiregion success | `vice-opt`, `vice-core` | universe/config identity and stale-calibration refusals | universe, pricing, backend and config digests |
| M8-02 | Produce a deterministic beam of opaque multi-palette hypotheses from interior-weighted premultiplied evidence; transparent pixels contribute no RGB evidence | `vice-evidence` | palette permutation, transparent-RGB and low-support adversaries | palette beam with intervals, support and code lengths |
| M8-03 | Select palette cardinality through physical code length, spatial coherence and exact rerender evidence; elbow rules and raw unique-colour counts are forbidden | `vice-evidence`, `vice-opt` | cardinality positive/negative controls and exact-rerender knockout | per-cardinality score decomposition |
| M8-04 | Alternate palette estimation with visible partition and per-face paint refinement until a bounded deterministic fixed point or typed exhaustion | `vice-core`, `vice-opt` | stale-partition, stale-paint, iteration-cap and rollback tests | alternation trace and convergence/refusal record |
| M8-05 | Preserve the Flat2 alpha contract: opaque interior faces plus transparent or opaque exterior; true semi-transparent authored interior layers remain unsupported until their milestone | `vice-ir`, `vice-evidence`, `vice-core` | alpha-mixture and hidden-layer adversaries | typed model-boundary decision |
| M8-06 | Build topology evidence for every palette/formation hypothesis without treating pairwise colour mixtures as independent evidence | `vice-evidence`, `vice-topology` | label permutation and double-counting tests | palette/formation provenance on topology candidates |
| M8-07 | Represent a deterministic region-adjacency graph with region identity, palette/paint hypothesis, shared-boundary support and exterior ownership | `vice-topology` | RAG symmetry, connectivity, canonicalization and tamper tests | canonical RAG digest |
| M8-08 | Implement atomic RAG merge, split and paint transactions with affected-scope rebuild/refit, exact ROI posterior, certificates and rollback | `vice-topology`, `vice-opt`, `vice-core` | transaction success, rejection, rollback and incremental/full differential tests | transaction ledger |
| M8-09 | Materialize one shared multicolor DCEL: each neighboring face pair references one boundary object and every face loop/exterior assignment remains valid | `vice-topology`, `vice-core` | multicolor DCEL invariants, junction/border fixtures and corruption attacks | graph certificate and shared-boundary inventory |
| M8-10 | At multi-face junctions compute one local area-fraction simplex, or exact local forward rendering, whose non-negative face fractions sum to one | `vice-render`, `vice-evidence` | triple/quad junction analytic fixtures, permutation invariance and sum-to-one tests | per-pixel junction fraction certificate |
| M8-11 | Certified rendering and ROI dependency closure must preserve partition sum, expose no hidden gap/overlap and remain deterministic for multicolor faces | `vice-render` | whole-scene/ROI differential and partition-sum adversaries | render/coverage certificate |
| M8-12 | Score and optimize one paint per visible face in the common full-resolution likelihood, with physical paint code lengths and quantized serialized acceptance | `vice-opt`, `vice-core` | finite-difference, quantization, face-permutation and paint-block transaction tests | per-face paint likelihood/code ledger |
| M8-13 | Search jointly over palette, RAG, topology, geometry and paint while retaining deterministic diversity and explicit unexplored mass | `vice-opt`, `vice-core` | beam quota, memoization, budget and missed-alternative tests | search-mass certificate |
| M8-14 | Add the factorial paint oracle without fake arms: auto/GT partition and auto/GT paint interventions share backend, config, budget and fixture identity | `vice-bench` | complete paint intervention matrix and incompatible-key refusals | per-arm rows, main effects and interaction |
| M8-15 | Calibrate multiregion success/abstention on a separately frozen clustered split and meet catastrophic-risk plus source/render coverage gates without borrowing Flat2 rows | `vice-bench`, `vice-core` | complete calibration and untouched held-out release court | signed selective reliability artifact |
| M8-16 | Export canonical PurePartition and SeamSafe multicolor SVGs whose independently parsed/rendered bytes agree with the certified scene | `vice-svg`, `vice-verify` | adjacent-colour seams, junctions, gaps and tamper tests | delivery seal and renderer comparisons |
| M8-17 | Keep the complete M7 Flat2 regression barrier green; multiregion support may not widen Flat2 calibration buckets or weaken typed abstention | workspace | M7 focused barrier plus cross-universe negative tests | unchanged Flat2 decisions/config identities or an explicit recalibration |
| M8-18 | Preserve typed budgets, deterministic decisions/bytes and exact population/execution attestations across isolated repeats and supported worker counts | all production crates, `vice-bench` | forced exhaustion, six-role replay and evidence-binding attacks | resource and determinism artifacts |
| M8-19 | Update status, reproducibility, traceability, failure ledger and deferred debt; P1 remains after M8 and is not smuggled into this milestone | docs, governance | document claims and clean replay | M8 documentation set |
| M8-20 | Freeze measured gates in config-only commits and obtain the review protocol required by the specification on one immutable clean SHA | governance | gate-history checker and independent review | exact-SHA verdicts |

## Required implementation order

1. Freeze this map and the M8 universe boundary before production success is
   possible.
2. Land multi-palette evidence and canonical RAG types with negative tests.
3. Land atomic RAG/DCEL transactions and junction fraction rendering.
4. Connect per-face paint likelihood, bounded alternation and production
   selection.
5. Add oracle and calibration populations, then freeze gates.
6. Run the M7 regression barrier, M8 author barrier, held-out release and
   exact-SHA review.

No step may manufacture success from a diagnostic/oracle override or reuse an
M7 confidence threshold for the expanded model universe.

## Generation-7 pre-release snapshot (2026-08-03)

| Obligation | State | Current evidence / remaining work |
|---|---|---|
| M8-01 | IMPLEMENTED | distinct M8 universe/config identities; stale M7 reliability is rejected; fresh author barrier pending |
| M8-02 | MET in code | deterministic multicolour palette beam; hidden transparent RGB and permutation/determinism controls pass |
| M8-03 | MET | all supported cardinalities enter the exact rerender court; observable mode count is retained for calibrated admission |
| M8-04 | MET | production alternation performs bounded shared-vertex/refit rounds and records convergence/exhaustion |
| M8-05 | MET in representation | only opaque face paints and transparent exterior are representable; transparent interior paint transactions refuse |
| M8-06 | MET | every palette/exterior/formation seed produces topology evidence; pairwise junction substitution refuses |
| M8-07 | MET in code | canonical component-based RAG, outside-connected exterior, adjacency inventory and digest |
| M8-08 | MET | atomic ledgers plus exact ROI/full-court equality and rollback certificates |
| M8-09 | MET | one shared DCEL is materialized into validated production IR with exact boundary bindings |
| M8-10 | MET in renderer | explicit multi-face simplex certificate, triple-junction analytic and permutation/gap controls |
| M8-11 | MET | multicolour whole/ROI equality, partition certificates and delivery rerenders are green |
| M8-12 | MET | full-resolution likelihood, per-face physical paint pricing and serialized acceptance are connected |
| M8-13 | MET empirically | joint deterministic candidate court and explicit `Unknown` unexplored mass; no theoretical completeness claim |
| M8-14 | MET | actual common-observation PP00/PP10/PP01/PP11 measurement, effects and incompatible-key refusals |
| M8-15 | PENDING EVIDENCE | generation 6 is burned; generation 7 requires fresh calibration, config-only gate freeze and untouched sealed court across procedural/authored/adversarial origins |
| M8-16 | MET | selected M8 scenes produce independently parsed/rendered PurePartition and SeamSafe delivery seals |
| M8-17 | PENDING REPLAY | the prior 239-test barrier was green; it must be rerun on the generation-7 feature candidate |
| M8-18 | IMPLEMENTED, PENDING REPLAY | typed budgets; exact population reconstruction; clean candidate/runner binding; four distinct execution IDs; committed-policy tamper refusal |
| M8-19 | PARTIAL | generation-5/6 failures and generation-7 procedure are documented; final evidence hashes remain pending |
| M8-20 | PENDING | no generation-7 gate commit, sealed verdict, final immutable SHA or passing cold review exists yet |
