# STATUS M8 — accepted

Date: 2026-08-03.

Generation 7 passed its fresh calibration and untouched sealed release court.
The agreed independent cold review accepted exact clean SHA
`9c58d181120af872bbb179c26bc1a35dde505615`; that SHA was pushed to
`origin/main` without a tracked edit after the verdict.

## Generation 7 result

- feature candidate: `ad164c239e8c7c4d3c455d1f2d817c4465b5b67c`;
- config-only sealed candidate: `fa83a2f38f9791bf5b11634f9b11365f645cc1f7`;
- calibration: 604 source groups, 503 admitted (83.278%), zero catastrophes,
  99% catastrophic-risk upper bound 0.009114;
- sealed audit: 600 source groups, 497 admitted (82.833%), zero catastrophes,
  99% catastrophic-risk upper bound 0.009223;
- both courts contain procedural, authored and adversarial sources;
- every court is reconstructed from exact source identities and digests, four
  distinct execution attestations, a clean candidate SHA and runner digest;
- the sealed release has `gate_met=true` and `refusals=[]`;
- production admission loads only the byte-exact policy committed at clean
  `HEAD`; uncommitted, stale or caller-constructed policy cannot authorize
  success.

Committed evidence:

- `docs/gt/M8_G7_CALIBRATION_COURT.json`, SHA-256
  `8f52382f29775b41e2254b394c264acb0fd04ec5056bdc706a3c8170b13fbde4`;
- `docs/gt/M8_G7_SEALED_COURT.json`, SHA-256
  `b7bb42addf98377a8e488a760798dceae3c852153d7978bddf14fba51dcc360a`;
- `docs/gt/M8_G7_RELEASE.json`, SHA-256
  `7208325d0cd6f58e4dc6995a5c7c838dac799e05f63b8a14d830d47b41291cb3`;
- `configs/M8_PRODUCTION_POLICY_V1.json`, SHA-256
  `ef91d9143d8853bafa9362d441e3a5b34090e43fc4fdc589f8b70ad0fbe79e62`.

## Historical failures and boundary

Generation 5 is burned: 67.17% coverage and five frozen row-gate violations.
Generation 6 is burned despite green aggregates because its evidence and
production-admission binding failed independent review. Neither generation is
eligible for promotion.

M8 makes no theoretical posterior-completeness claim; unexplored mass remains
explicitly `Unknown`. Semi-transparent authored interiors, P1 editing,
extended degradation, strokes and gradients remain later milestones.
