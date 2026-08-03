# STATUS M8 — release court green, review pending

Date: 2026-08-03.

M8 production code, calibration, delivery, M7 regression barrier and the
untouched generation-6 sealed court are green. Promotion waits only for the
single independent clean-checkout review required by the project protocol.

## Frozen candidate and evidence

- feature SHA: `0f4ecd36e7a34d2c8b64269ba78acb5adad6f562`;
- gate-only SHA: `2eb6082`;
- calibration: 599 eligible groups, 498 admitted, 83.14% coverage, zero
  catastrophes, 99% one-sided upper risk 0.9205%;
- sealed release: 592 eligible groups, 496 admitted, 83.78% coverage, zero
  catastrophes, 99% one-sided upper risk 0.9242%;
- frozen boundary row ceilings: p95 1.25 px, p99 1.5 px, max 1.75 px;
- serialized paint delta ceiling: 0 codes;
- runtime p95 observed in calibration: 5174 ms;
- one-worker versus two-worker replay: eight identical decision/artifact rows
  after excluding runtime telemetry.

The machine-readable authorities are
`configs/M8_PRODUCTION_CALIBRATION.json`,
`configs/M8_GATE_PROVENANCE_V1.toml`,
`docs/gt/M8_RELEASE_VERDICT.json`, and
`docs/gt/M8_REPLAY_ATTESTATION.json`.

## Delivered scope

- deterministic multicolour palette evidence and observable mode support;
- canonical RAG, atomic transactions, shared multicolour DCEL and junction
  simplex rendering;
- production `VectorScene` materialization, per-face paint refit, bounded
  exact rerender alternation and explicit unknown unexplored search mass;
- exact affected-ROI transaction certificates;
- complete measured partition × paint factorial with no fake arms;
- canonical PurePartition and SeamSafe export, independent parse/render and
  delivery seals;
- M8-only selective calibration: no M7 confidence or Flat2 population is
  reused;
- generation-8 M7 Flat2 corpus and the full `vice-bench` regression suite stay
  green (239 passed, 0 failed, 4 explicit long corpus walks ignored).

## Audit history and boundaries

Generation 5 failed after its first sealed opening (67.17% coverage and five
frozen-gate violations). It is permanently burned. Its identities and failure
counts are preserved in `configs/M8_GATE_PROVENANCE_V1.toml`. Generation 6 was
re-keyed before calibration and opened once only after the feature and gate
commits were immutable.

M8 does not claim theoretical posterior completeness: unexplored mass remains
explicitly `Unknown`, and production success is the separately measured
empirical selective claim above. True semi-transparent authored interiors,
the P1 correction editor, broader degradation, strokes and gradients remain
owned by their later milestones.

## Remaining terminal step

Run one independent clean-checkout cold review on the final immutable SHA. A
blocking finding requires a fix, a new candidate SHA, and the audit treatment
mandated by the specification; otherwise M8 may be marked complete and pushed.
