# STATUS_M7 — exact posterior refinement, selective delivery, and export

Status: **author release candidate; sealed generation 5 and independent reviews
remain the acceptance authority.** This document does not self-certify M7.

Normative scope is `docs/spec/VICE_CLASSIC_CORE_AGENT_SPEC_v1.3.md` §28 M7,
§29–§31, §34–§36. The exhaustive acceptance map is
`docs/M7_OBLIGATIONS.md`; no requirement is narrowed here.

## Release identity

| Item | Frozen identity |
|---|---|
| procedural generation | 5 |
| supported-model universe | `m7-v8` / `bad6c768e289fe624843fb0260bf0c49f89a6a9789689bc515cb7f9e7e84af05` |
| pricing | `beabb5b48b9cef811356bf49f4d178fb1e6cadbb9fec76cb49ca80d2895060c6` |
| corpus | 63 scenes, 2817 renders / `50ebb0eff526d1d036459100ae989ff8b3448932903a063edf0d1681b0200795` |
| Quality production config | `8a950dbe023a95ef3fc075bc6abfd8e415412a5fdcb5392ef63c6c2e27cca8db` |
| Fast production config | `5341ae899610c746582b26afb01e53a29c884d523773d0310db76abffc447bf4` |
| geometry calibration | `1c6bc17e43b066aba27cdb1db40381638c7a7d05b1cb5295278e77623946c1e8` |

The structured authority for these values is
`configs/M7_GATE_PROVENANCE_V1.toml`, whose source commit is `562d96b`.
Production config loaders compare exact bytes against the executable-bound
digests; arbitrary research overrides cannot publish a production success.

## Complete pre-seal measurement

Generation 5 was calibrated over every mandatory calibration row, once per
preset, after an exhaustive classification of all 1116 generation-4 Fast
abstentions. The generic repair converted 706 to verified production
successes and retained 410 typed abstentions. Fresh calibration then completed:

| Preset | Rows | Gate | posterior lower bound | unexplored-mass upper bound | predictive bits/block | support displacement px |
|---|---:|---|---:|---:|---:|---:|
| Quality | 1809/1809 | MET | 0.0005294467469807713 | 1884 | 0.23230730958896773 | 0.7634564581698986 |
| Fast | 1809/1809 | MET | 0.0012460896972730008 | 799 | 0.23230730958896773 | 0.7222824562882783 |

The generation-5 geometry calibration contains 11 complete six-arm rows,
six measured/recovered G20 rows and eleven measured/recovered G30 rows. Gate
values were frozen in config-only commits before any generation-5 sealed row
was inspected.

## Author closure table

`MET` below means executable production code plus a judge exists and the
pre-audit workspace/replay barrier is green. `SEALED` rows are intentionally
not promoted until the untouched audit is measured.

| Obligation | Author state | Evidence owner |
|---|---|---|
| M7-01–05 product/architecture | MET | workspace, `vice-core`, CLI end-to-end and source-role tests |
| M7-06–11 likelihood/posterior | MET | `vice-opt` likelihood/posterior ledgers and equivalence-class tests |
| M7-12–20 optimization/search | MET | trust-region, compound transaction, incremental DCEL, beam and knockout tests |
| M7-21–28 verification/export/abstention | MET | `vice-verify`, `vice-svg`, delivery roundtrip and forced-refusal tests |
| M7-29 selective reliability | PENDING SEALED | generation-5 release verdict |
| M7-30 frozen boundary/catastrophic gates | PENDING SEALED | gate provenance plus release verdict |
| M7-31 internal baseline | PENDING SEALED | paired baseline/blind artifact |
| M7-32 blind court | PENDING SEALED | paired baseline/blind artifact |
| M7-33 refusal/conditioning accounting | MET | complete calibration reports; repeated in sealed verdict |
| M7-34–35 complete oracle/recovery | PENDING SEALED | complete PF/G/O release oracle |
| M7-36 determinism | PENDING RELEASE ARTIFACT | isolated and worker-count replay artifact |
| M7-37 budgets | MET | typed elapsed/memory/evaluation ledgers and exhaustion tests |
| M7-38 runner governance | MET | anchored tool/config/provenance attestation and substitution attacks |
| M7-39 exact replay | PENDING RELEASE ARTIFACT | canonical artifact bound to the final clean SHA |
| M7-40 documentation/debt | MET FOR AUTHOR CANDIDATE | this status, reproducibility, traceability, failure ledger and debt inventory |

## Pre-audit barrier

The author barrier contains format, warning-free release clippy, complete
debug and release workspace suites, all declared ignored courts, exact
production-config loading, documentation claims, same-platform corpus replay,
and exact same-platform replay of the M4, M4.5, M5 and M6 evidence artifacts.
The command list is in `docs/REPRODUCIBILITY_M7.md`.

The generic M7 palette/evidence repair changed inherited M4–M6 measurements.
All four artifacts were regenerated together, every dependent document number
was enumerated, and `doc_claims` passed 7/7. This is recorded as F-0149 rather
than hidden as incidental churn.

## Runtime statement

The 10 s Quality and 1 s Fast wall-clock values are reported research
diagnostics, not release refusals, exactly as `M7_OBLIGATIONS.md` states.
Deterministic elapsed, memory, hypothesis and render caps remain hard. No
runtime miss may truncate the search or alter selected bytes.

## Acceptance still outstanding

The generation-5 seal remains unopened at this author-candidate stage. M7 is
not accepted until the sealed release, baseline/blind, oracle, determinism and
canonical artifacts are green and four independent reviews name the same
immutable SHA: two cold reviews, the mandatory numerical/topology red team,
and the release-quality audit.
