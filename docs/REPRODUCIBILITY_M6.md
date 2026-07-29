# REPRODUCIBILITY_M6 — Stage G/H and five-arm geometry oracle

This is the reproducibility contract for §28 M6. It supersedes the historical
“M6 not started / 0 of 5 arms” statements in earlier sections of
`STATUS_M6.md`; those sections remain as an audit trail.

## 1. Pinned inputs

- specification: `docs/spec/VICE_CLASSIC_CORE_AGENT_SPEC_v1.3.md`;
- gate file: `configs/GATES_V1.toml`, sections `[geometry_code_table]`,
  `[geometry_pricing]` and `[m6_geometry]`;
- geometry artifact: `docs/gt/GEOMETRY_M6.json`;
- model universe:
  `47903d7374d54683e60c318239d75adabcc2eef5fc80ad9d7822e8176990f097`;
- geometry pricing surface:
  `1060cc132bde90a32043a9a7bca6c6936be241b38ac20523ed2f76bea0dfc691`;
- Stage G/H backend source:
  `f22e03c9b469119b8bdb2f983387539a3f48deeb405d465950698f0674e915a3`;
- recording platform for the Tier-A artifact: `windows-x86_64`.

`GEOMETRY_M6.json.measurements.config` carries all three hashes, the candidate
budget, K and the four-cut closed-loop policy. Therefore a model, price,
materialization or search-policy change changes the §27.6 compatibility key
instead of silently retaining the old fingerprint.

## 2. Build and default verification

From the repository root:

```text
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo test --locked --release --workspace
```

These commands cover the typed input refusals, hierarchical schedules, family
fits, k-best grammar, joint G1 representation, code-length selector,
sample/cut/transform invariances, no-BIC knockout, relation projections,
whole-loop primitive promotion, universe judges, gate consumers and CLI tests.

## 3. Corpus Stage G/H population

```text
cargo test --locked --release -p vice-bench \
  fit::tests::the_candidate_stage_over_the_corpus \
  -- --ignored --nocapture
```

The 2026-07-29 Windows run (one cell per non-sealed scene) measured:

| quantity | measured |
|---|---:|
| analysed arms / without boundary | 41 / 13 |
| sealed-audit source groups skipped | 22 |
| chains / samples | 36 / 1,910 |
| supports / structural bound | 4,065 / 10,187 |
| candidates after cost / before cost | 14,326 / 14,659 |
| normal-line-miss cost refusals | 333 |
| chains with a model / solver emptied k-best | 35 / 1 |
| selected typed chains / whole-loop primitives | 7 / 28 |
| typed selected segments / smooth joins | 18 / 1 |
| path refusals | 43 outside-corridor |
| relation hypotheses considered / promoted | 222 / 1 |
| whole-loop hypotheses considered / promoted among k-best models | 3,920 / 209 |
| worst exact-G1 spread | `3.553e-15 rad` over 1 selected typed node |
| lowering failures | 0 |

The test asserts that every fitted segment family occurs, the schedule stays
within its structural bound, all 35 materialized winners are classified as
either a typed chain or a whole-loop primitive, every smooth join in selected
typed geometry is measured (including the cyclic seam), no lowering failure is
hidden, and only the declared typed path refusals occur. Candidate-generation
statistics still cover every fitted chain; the one chain whose k-best set the
solver emptied is counted explicitly, and a primitive winner cannot skip the
outer chain population.

## 4. Five-arm geometry artifact

Record and gate the raster-derived common population:

```text
cargo run --locked --release --bin gt-corpus -- geometry-m6 \
  --gates configs/GATES_V1.toml \
  --out docs/gt/GEOMETRY_M6.json
```

Replay every boundary and arm on the recording platform:

```text
cargo run --locked --release --bin gt-corpus -- geometry-m6-check \
  --gates configs/GATES_V1.toml \
  --report docs/gt/GEOMETRY_M6.json
```

Expected population and gate witnesses:

| clause | measured | frozen requirement |
|---|---|---|
| common population | 11 of 19 observed closed chains complete all five arms | `>= 6` |
| exact arm set | G00, G10, G01, G11, G20 on every boundary | exactly 5 |
| compatibility | 55 arm rows share key `d5d21071…98f1b` | one identical five-component key |
| raster provenance | 11 rows from independent ExactClip raster → production Stage F | `>= 6` |
| oracle candidate injection | 27 forced-discrete fits | `>= 10` |
| material selector changes | G01/G10/G11 = 3/6/1 geometry hashes | `>= 1/1/1` |
| multi-span / heterogeneous | 11 / 2 rows | `>= 6/2` |
| arc / quad / cubic GT labels | 1 / 1 / 3 rows | `>= 1/1/2` |
| forced joint alternatives / smooth | 3 / 3 rows | `>= 2/2` |
| selected Stage H relations / primitives | 4 / 5 rows | `>= 2/1` |

Aggregate symmetric maximum error:

| arm | mean max px | worst max px | interpretation |
|---|---:|---:|---|
| G00 | 0.5984981083 | 1.6679802262 | auto candidates + auto selector |
| G10 | 0.2962620573 | 0.8023976306 | forced candidate union + auto selector |
| G01 | 0.5984981083 | 1.6679802262 | auto set + oracle selector |
| G11 | 0.2960652304 | 0.8023976306 | forced set + oracle selector |
| G20 | 0.2962620573 | 0.8023976306 | forced families/breakpoints + production parameter fit |

All fit inputs are `BoundaryChain`s extracted by the production Stage-F path
from an independently rendered 128 px raster. GT is used only to bind a
Stage-F chain to a face loop, label families/breakpoints for the forced arms,
and build the scoring target. G20 never receives GT parameters; it calls the
typed forced-discrete API and the production joint solver. Selector changes
compare SHA-256 of serialized materialized geometry, not pointers, source
labels or code bits.

The artifact is Tier A because the geometry contains libm-derived floats.
`geometry-m6-check` therefore performs the full comparison only on the
recording `(os, arch)`. The Windows CI job runs that comparison. On another
platform, generate a new report and inspect its gate table; do not compare its
float rows as if they were portable bytes.

## 5. Frozen-gate separation

The relevant sequence is intentionally split:

1. C325/C326 register placeholder gate keys;
2. C327 lands consumers and the first five-arm harness;
3. C328 freezes its first thresholds in a gate-file-only commit;
4. C330 changes the primitive/relation model universe and pricing;
5. C331a changes the universe witness; C331b separately freezes pricing;
6. C339–C348 repair materialization, seam/search semantics and the
   raster-derived oracle;
7. C342 binds the repaired model/search surface and C343 is its config-only
   re-freeze;
8. C348 lands the final row-derived consumers and C349 freezes their thresholds
   in a config-only commit;
9. C351 splits the repaired source under the hygiene bound while extending the
   backend digest to the new modules, and C352 records that candidate;
10. C357–C365 close the final corridor, relation, cyclic-search, totality,
    pricing-control, provenance and aggregate-review blockers without moving a
    frozen threshold;
11. C369 restores the production-module hygiene bound by splitting the
    non-production control, and C370 records the final Tier-A artifact.
12. C373–C377 close the final independent-review findings: observation-bound
    cyclic roots, translation-invariant concentric relations, negative-weight
    refusal, non-negative physical code terms and pre-K cyclic seam ranking;
13. C378 freezes the changed pricing surface alone, C379 adds the
    raster-derived smooth-seam witness, and C380 records the repaired Tier-A
    artifact alone.
14. C382 splits grammar tests below the 800-line production-module bound, and
    C383 records the content-bound backend identity alone.
15. C385 closes public-input totality and relation-saving ownership; C386
    branches corner/smooth seam joins inside K-best and applies the declared
    proposal tie-break at every truncation.
16. C387 freezes that changed pricing/search surface alone, and C388 records
    the resulting 11-boundary, 55-arm Tier-A artifact alone.
17. C390 restores the 800-line production-module bound without changing
    behavior, and C391 separately records its content-bound backend identity.
18. C393 makes both exported grammar stages typed-refuse invalid observation
    mass and finite derived residual-code overflow.
19. C394 removes the unidentifiable adjacent `SharedBaseline` duplicate; C395
    freezes that changed eligibility/pricing surface alone.
20. C396 separately records and replays the resulting 11-boundary, 55-arm
    Tier-A artifact.
21. C399 validates caller-constructible candidate, edge and path structure at
    every exported grammar boundary; C400 gives Stage H the observation's
    open/closed topology and refuses relation projections that change it.
22. C401 publishes the new typed refusal names in corpus telemetry; C402 adds
    both split source modules to the complete backend digest; C403 separately
    records the resulting seam-safe 11-boundary, 55-arm Tier-A artifact.
23. C405 validates every remaining caller-constructible grammar/relation
    composition: whole-path cost accumulation, path-family lookup,
    family/segment materialization and relation-code application. C406 records
    the resulting content identity alone; all row-derived populations and
    geometry errors remain unchanged.

To audit the rule over a commit range, feed `git diff --name-status` rows to:

```text
cargo run --locked --release --bin gt-corpus -- gates-check \
  --stdin --existing-gate configs/GATES_V1.toml
```

CI applies this per commit over the whole pushed range.

## 6. What is and is not claimed

M6 claims a boundary-observation MDL selector and materialized constrained
hypotheses. It does not call `ChainCode::total_bits()` the final pixel
posterior: the correlation-aware full-resolution likelihood, scene-level
compound search, local-isotopy binding, export and post-quantization
verification remain successor work.

Whole-loop promotion is not permission to emit a native SVG primitive. Native
emission still requires the three §15 conditions: exact canonical boundary
identity, shared neighbouring boundary ownership and a green post-quantization
verifier.

The sealed audit is not opened by any M6 command above.
