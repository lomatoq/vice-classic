# STATUS M8 — generation 7 pre-release audit

Date: 2026-08-03.

M8 is not released yet. Generation 6 produced numerically green calibration
and sealed aggregates, but the required independent cold review blocked it:
the court trusted self-described evidence, production admission had no trusted
success path, the population had only two procedural families, and the
traceability/failure documents made claims broader than their evidence.
Generation 6 is therefore permanently burned.

## Generation 7 repair state

The replacement candidate now contains, before any new sealed opening:

- exact reconstruction of the formal population, shard set, row identities,
  rendered source digests and merged corpus commitment;
- four distinct execution IDs, clean-HEAD candidate identity, runner digest,
  and a runner build-SHA check;
- disjoint procedural, authored and adversarial calibration/release sources,
  with explicit per-origin counts and a 50% minimum coverage floor per origin;
- calibration-file and config-only gate-authority binding;
- a real production-admission path that accepts only a byte-exact policy
  committed at clean `HEAD`; an arbitrary caller policy cannot authorize
  success;
- negative tests for forged population counts, duplicate executions,
  untrusted policy bytes, stale identities and calibrated abstention.

The authored/adversarial preflight reaches exact solve and independent SVG
delivery on both formal splits. The full author barrier, fresh generation-7
calibration, config-only gate freeze, untouched generation-7 sealed court and
one follow-up cold review are still pending. No generation-7 gate or release
metric is claimed in this document yet.

## Historical evidence

Generation 5 failed its first sealed opening: 67.17% coverage and five frozen
row-gate violations. Generation 6 measured 83.78% sealed coverage with zero
reported catastrophes, but its evidence/provenance mechanism failed review;
neither generation is eligible for promotion.

M8 still makes no theoretical posterior-completeness claim. Unexplored mass is
explicitly `Unknown`. Semi-transparent authored interiors, P1 editing,
extended degradation, strokes and gradients remain later milestones.
