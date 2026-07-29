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
  `fdcd283a01c3987aa58caa5698e5dd17fab36f595bf55e18e25613713c107359`;
- geometry pricing surface:
  `4d90681d4b77129d9017ddfe5809ee249611e9c3dc7f8a56f908e40ca2b81d42`;
- recording platform for the Tier-A artifact: `windows-x86_64`.

`GEOMETRY_M6.json.measurements.config` carries both hashes. Therefore a grammar
or pricing change changes the §27.6 compatibility key instead of silently
retaining the old fingerprint.

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
| chains with a model | 36 |
| selected segments / smooth joins | 105 / 18 |
| path refusals | 18 degenerate-span; 28 outside-corridor |
| relation hypotheses considered / promoted | 561 / 6 |
| whole-loop hypotheses considered / promoted among k-best models | 3,504 / 188 |
| worst exact-G1 spread | `6.661e-15 rad` over 18 nodes |
| lowering failures | 0 |

The test asserts that every fitted segment family occurs, the schedule stays
within its structural bound, every selected smooth join is measured, no
lowering failure is hidden, and only the expected short-chain entry refusal may
occur.

## 4. Five-arm geometry artifact

Record and gate the full development population:

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
| common population | 205/205 boundaries; 0 exclusions | `>= 100` |
| exact arm set | G00, G10, G01, G11, G20 on every boundary | exactly 5 |
| compatibility | 1,025 arm rows share key `c74a63c…e2888` | one identical five-component key |
| oracle candidate injection | 205 forced-discrete fits | `>= 100` |
| oracle selector positive control | 14 G01 choices differ from G00 | `>= 1` |

Aggregate symmetric maximum error:

| arm | mean max px | worst max px | interpretation |
|---|---:|---:|---|
| G00 | 0.0059325734 | 0.1520221988 | auto candidates + auto selector |
| G10 | 0.0059325734 | 0.1520221988 | forced candidate union + auto selector |
| G01 | 0.0004072413 | 0.0104374370 | auto set + oracle selector |
| G11 | 0.0004072413 | 0.0104374370 | forced set + oracle selector |
| G20 | 0.0004072413 | 0.0104374370 | forced families/breakpoints + production parameter fit |

G10 matching G00 is a measured result, not a vacuous arm: 205 forced candidates
were injected. G01 is the selector positive control. G20 never receives GT
parameters; it calls the typed forced-discrete API and the production joint
solver.

The artifact is Tier A because the geometry contains libm-derived floats.
`geometry-m6-check` therefore performs the full comparison only on the
recording `(os, arch)`. The Windows CI job runs that comparison. On another
platform, generate a new report and inspect its gate table; do not compare its
float rows as if they were portable bytes.

## 5. Frozen-gate separation

The relevant sequence is intentionally split:

1. C325/C326 register placeholder gate keys;
2. C327 lands consumers and the five-arm harness;
3. C328 freezes measured thresholds in a gate-file-only commit;
4. C330 changes the primitive/relation model universe and pricing;
5. C331 freezes the resulting universe/pricing hashes in a gate-only commit;
6. C333 binds both hashes into the intervention config;
7. C334 records the artifact produced by that exact config.

To audit the rule over a commit range, feed `git diff --name-status` rows to:

```text
cargo run --locked --release --bin gt-corpus -- gates-check \
  --stdin --existing-gate configs/GATES_V1.toml
```

CI applies this per commit over the whole pushed range.

## 6. What is and is not claimed

M6 claims a boundary-observation MDL selector and constrained hypotheses. It
does not call `ChainCode::total_bits()` the final pixel posterior: the
correlation-aware full-resolution likelihood, scene-level compound search,
trust-region constrained re-solve, local-isotopy binding, export and
post-quantization verification are M7 work.

Whole-loop promotion is not permission to emit a native SVG primitive. Native
emission still requires the three §15 conditions: exact canonical boundary
identity, shared neighbouring boundary ownership and a green post-quantization
verifier.

The sealed audit is not opened by any M6 command above.
