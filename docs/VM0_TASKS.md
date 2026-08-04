# VM0 Creator tasks

There is exactly one current Creator task. Do not begin any later plan item.

## C1 — create `vice-vm-core` and the Circle forward fixture

State: ACTIVE

### Objective

Create the smallest real `vice-vm-core` crate and prove its deterministic
forward-fixture path with a circle. This task creates test evidence only; it
does not attempt inverse vectorization yet.

### Allowed changes

- Add `crates/vice-vm-core` to the workspace.
- Add only the concrete crate/module code needed for the Circle fixture.
- Reuse `vice-geom` and `vice-render`; use `vice-image` only if artifact
  encoding requires it.
- Add focused tests and saved C1 visual artifacts.
- Add a short `docs/VM0_TASK_RESULT.md` containing commands, outputs, artifact
  paths, and the commit SHA.

Do not add placeholder traits or empty future modules. Do not depend on
`vice-fit`, `vice-core`, `vice-opt`, `vice-topology`, or `vice-wasm`.

### Fixture contract

The generator input is explicit and serializable: resolution, center/radius,
foreground/background linear-RGB paints, subpixel phase, blend space, AA
kernel, and seed. The output contains the source raster, analytic ground-truth
circle, and ground-truth boundary samples. Identical inputs must be
byte-identical.

Exercise at minimum:

- resolutions 32 and 64;
- phases `(0,0)` and `(0.37,0.61)`;
- one high-contrast and one moderate-contrast paint pair.

Save a compact contact sheet showing the analytic GT overlay and generated
raster for every exercised case. It must be inspectable without a UI or M8.

### Required checks

```text
cargo test -p vice-vm-core
cargo clippy -p vice-vm-core --all-targets -- -D warnings
```

Tests must prove deterministic replay, dimensions/color validity, closed and
finite GT boundary samples, and that the configured circle is not rasterized
as exported unit-grid geometry.

### Stop condition

Make one small commit, write the task result, and stop. Do not implement paint
estimation, topology, subpixel inversion, fitting, multicolor, UI, M8
integration, or release work. The next task remains unavailable until the
Checker-Fixer returns `ACCEPT` or `FIXED_AND_ACCEPT` for C1.
