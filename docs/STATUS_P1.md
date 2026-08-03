# STATUS P1 — green

Date: 2026-08-03.

P1 implements the exact §28 scope without adding a graphical or Bézier editor:

- `inspect_multiregion_partition` returns a source-, seed-, scene- and
  RAG-bound region snapshot suitable for an editor;
- `PartitionEditScript` is versioned JSON with deterministic
  merge/split/assign/protect/restore operations over stable labels;
- the whole script fails closed: a stale base, disconnected split, protected
  mutation or invalid restore returns no partial scene;
- restore includes both partition and protection state;
- a successful correction rebuilds the RAG and shared multicolor DCEL,
  materializes the edited scene, prices the explicit paints, and reruns the
  exact M8 render/likelihood court;
- an explicit paint assignment is scored as written rather than silently
  refitted away;
- the correction report records every step, affected pixels, all before/after
  digests, protected labels and the exact core report;
- manual correction does not self-declare calibrated automatic production
  admission.

Targeted reproduction:

```text
cargo test --release -p vice-topology partition_script -- --nocapture
cargo test --release -p vice-core p1 -- --nocapture
cargo clippy -p vice-topology -p vice-opt -p vice-core --all-targets -- -D warnings
```

Final barrier: 30 vice-core, 23 vice-opt, 100 vice-topology, 7 topology
integration, 7 documentation and 13 hygiene tests passed in release mode;
clippy is warning-free for all three affected crates. No numerical gate or new
calibration is introduced by P1. M9 is the next milestone.
