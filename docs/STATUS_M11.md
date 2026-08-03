# STATUS M11 — implementation complete

Date: 2026-08-03.

M11 implements the complete §28 gradient lane as a bounded standalone model
family, without changing accepted flat-scene identities or admission policy:

- validated compact scenes represent opaque solid, linear and radial paints;
- linear geometry uses two points; radial geometry uses one center and radius;
- ordered stops are bounded to 32, with equal offsets reserved for exactly two
  different colors on the two sides of a real discontinuity;
- deterministic rendering interpolates in canonical linear light and preserves
  hard-stop side semantics;
- observed pixels determine the linear direction, bounded radial-center search,
  stop profiles and discontinuities; proposal and render work are bounded and
  unsupported alpha is a typed refusal;
- solid, linear and radial candidates use the same codec-aware residual plus an
  explicit geometry/stop/model description length;
- flat and one-pixel inputs select Solid, while independent linear, radial and
  hard-step fixtures select the corresponding compact gradient model.

The public API is `inspect_m11_gradients` and `classify_m11_gradient`. The
classification result includes the validated scene, canonical scene JSON,
deterministic rendered bytes, every candidate score and the runner-up margin.
Region-editor/UI/WASM integration and broader product performance policy remain
M12 productization work.

Verification:

```text
cargo test --release --workspace
cargo clippy --workspace --all-targets -- -D warnings
```
