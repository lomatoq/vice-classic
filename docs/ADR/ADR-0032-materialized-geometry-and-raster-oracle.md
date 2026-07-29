# ADR-0032 — M6 judges the geometry it delivers, and its oracle fits raster evidence

Status: accepted (M6 final delta, C339–C352)  
Spec: v1.3 §14, §15, §27.6, §27.7, §28 M6, §32 rules 9, 12, 23 and 29

## Context

The first M6 completion candidate had two coupled provenance failures.

First, the pipeline could solve a constrained chain, keep a residual from
before the solve, accept a relation or primitive as a code-length delta, and
still expose the unconstrained chain as the selected geometry. Ranking,
verification and delivery therefore did not necessarily refer to one value.

Second, the five-arm oracle fitted samples flattened directly from canonical
ground-truth curves. It exercised fitting APIs but bypassed raster decode,
Flat2 analysis and Stage-F boundary extraction. Its 205 rows could not
establish that the production inverse path supplied any of them.

These are the same architectural error: evidence, score and delivered value
were allowed to have different provenance while sharing one row.

## Decision

### 1. A selected boundary model owns one materialized geometry

`BoundaryModel.geometry: SelectedBoundaryGeometry` is the value judged and
delivered. It is exactly one of:

- a jointly refitted typed chain; or
- the canonical geometry of a selected whole-loop primitive.

A selected relation produces a constrained typed-chain sibling. A primitive
produces a primitive sibling. Code savings are metadata on those values; they
cannot select a sibling that is absent from the model.

Residual, symmetric evidence-corridor feasibility, oracle error, geometry hash
and downstream flattening are all computed after the final constraint from
that same value.

### 2. Closed-loop semantics are part of the model

A closed chain has an explicit closure seam and `closure_smooth` constraint.
Automatic fitting evaluates at most four deterministic openings ranked by
persistent corner evidence. All openings spend one candidate budget belonging
to the physical loop. Forced fitting remains closed and does not turn the seam
into a weighted span.

The opening policy, K, budget, model universe, pricing surface and complete
Stage G/H backend source digest are compatibility inputs.

### 3. The five-arm observation comes from production Stage F

The M6 oracle uses an independent ExactClip raster cell, canonical decode,
production Flat2 analysis and production boundary observation. A row exists
only when an observed closed Stage-F chain can be bound to a canonical face
loop within the declared 2 px matching tolerance and all five arms complete.

Ground truth may supply:

- the face-loop identity used for binding;
- family and breakpoint labels for G10/G11/G20;
- the independent geometry-scoring target.

Ground-truth points may not become the `BoundaryChain` fitted by any arm.

Four explicit development witnesses add the requirement families that the
broad development split does not happen to contain in eligible Stage-F loops:
four circular arcs, mixed quadratic/cubic spans, and a cornered
line/cubic/line chain, plus a smooth four-cubic cyclic seam. They traverse the
same raster and Stage-F path and are identified as witnesses in the population
string.

### 4. A selector change means material geometry changed

Each arm stores SHA-256 of serialized `SelectedBoundaryGeometry`. G01, G10 and
G11 positive-control counts compare those hashes. Pointer identity, source
labels, family labels, code bits and error equality are insufficient.

The gate derives every aggregate and coverage count from primary rows, checks
the full compatibility key on each arm, and has one negative knockout for each
clause.

## Consequences

- The common population is smaller (7 rows, 35 arms) and the measured error is
  larger than the direct-GT artifact. This is accepted because every row now
  proves the production inverse path and every claimed family/joint is
  load-bearing.
- Stage-H selection is observable as geometry and can be deleted only by
  changing the artifact and its gates.
- The artifact is Tier A: same-platform replay compares the complete report;
  cross-platform consumers must not treat libm-derived floats as portable
  bytes.
- M6 still does not claim scene-level posterior selection, local-isotopy export
  authorization, optimizer convergence, selective delivery or
  post-quantization verification.
