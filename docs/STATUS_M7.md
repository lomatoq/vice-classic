# STATUS_M7 — exact posterior refinement, selective delivery, and export

Status: **engineering closure under an explicit operator waiver**. The M7
implementation and judges are complete, generation-8 calibration is green,
and the consolidated review repair is tested. This is not represented as a
strict sealed-audit acceptance: the operator stopped the post-repair court and
reduced the final review protocol to one short cold review.

Normative scope remains `docs/spec/VICE_CLASSIC_CORE_AGENT_SPEC_v1.3.md` §28
M7 and `docs/M7_OBLIGATIONS.md`. The waiver changes release procedure, not the
meaning of those requirements.

## Frozen identity

| Item | Identity |
|---|---|
| procedural generation | 8 |
| sealed population | 800 source groups / 2400 rows per run |
| population commitment | `31b6f625a774d34ef9fbc6c2da82af06e0619e3c5cbb61673f1f006f0ba63a7a` |
| model universe | `bad6c768e289fe624843fb0260bf0c49f89a6a9789689bc515cb7f9e7e84af05` |
| pricing | `beabb5b48b9cef811356bf49f4d178fb1e6cadbb9fec76cb49ca80d2895060c6` |
| Quality production config | `60580f10119eed7720909daede4492718eba8362909c448bbd0920d1455677e6` |
| Fast production config | `da2e6414d1ad3cd039969e74df3c514281230d2d4127cfcf0e338a792b3d1594` |
| Quality calibration evidence | `020b0c8e9549fbb0fa32bbdb3acd4ead3b2313b5727290035c0195c6c1fe3482` |
| Fast calibration evidence | `174dadf99e7972298837fc4fae2dc08be53f909218ff8af79059bfcf66e22735` |
| geometry calibration | `1c6bc17e43b066aba27cdb1db40381638c7a7d05b1cb5295278e77623946c1e8` |

`configs/M7_GATE_PROVENANCE_V1.toml` is the structured authority. Production
loaders compare exact config bytes against executable-bound digests.

## Generation-8 calibration

Both complete calibration reports contain 1809/1809 rows and are green. They
were re-analysed under measurement schema v24 after the review repair; the
policy is unchanged, while the production configs now bind the expanded
policy-driving evidence projection.

| Preset | accepted source groups | accepted renders | source coverage | render coverage | catastrophic groups |
|---|---:|---:|---:|---:|---:|
| Quality | 601/601 | 1747/1803 | 100% | 96.894% | 0 |
| Fast | 601/601 | 1751/1803 | 100% | 97.116% | 0 |

The 10 s Quality and 1 s Fast wall-clock values remain non-blocking research
targets. Deterministic work, memory, hypothesis, and render limits remain hard.

## Final review repair

One independent review returned five P1 findings. Commit `fbd0a41` closes them
as one batch:

1. calibration evidence hashes every policy-driving field and excludes only
   scheduling/resource telemetry;
2. all courts reconstruct and require exactly 800 groups and 2400 unique rows
   against the compiled population commitment;
3. determinism requires six distinct typed roles and execution IDs;
4. the complete geometry population and independent renderer gate are both
   release-blocking, and recovery refusals remain in the denominator;
5. reports, journals, commands, shards, configs, candidate, runner, corpus and
   canonical artifact are cryptographically bound and cross-checked.

The new anti-regression tests and warning-free all-target build are green.

## Obligation state

| Obligation | State | Evidence |
|---|---|---|
| M7-01–28 | MET | production path, posterior/search, verifier and delivery suites |
| M7-29–33 | WAIVED AFTER CALIBRATION | generation-8 calibration is green; no complete post-repair sealed verdict |
| M7-34–35 | IMPLEMENTED; POST-REPAIR COURT WAIVED | exact PF/G/O population and renderer gates are enforced in code |
| M7-36 | IMPLEMENTED; POST-REPAIR COURT WAIVED | six-role distinct-execution judge and regression test |
| M7-37–38 | MET | resource ledgers and anchored governance |
| M7-39 | IMPLEMENTED; FINAL ARTIFACT UNMINTED | canonical binding rejects mixed identities; no post-repair court inputs |
| M7-40 | MET WITH WAIVER DISCLOSED | status, reproduction, traceability and failure ledger updated |

## Why no strict acceptance artifact exists

The generation-8 sealed court was stopped after 5091/14400 checkpointed rows
when the independent review found production evidence-binding blockers. Those
checkpoints predate `fbd0a41` and cannot certify the repaired code. By explicit
operator direction, the court is not rerun; only one short final cold review is
performed. Therefore the project may call this **M7 engineering-complete under
waiver**, but must not call it a strict green sealed-audit acceptance.
