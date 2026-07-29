//! Frozen §28 M6 geometry-gate constants consumed by the executable tests.
//!
//! The source of authority is `configs/GATES_V1.toml [m6_geometry]`. These
//! constants exist in `vice-fit` because this crate cannot depend on
//! `vice-bench` (the bench already depends on it). `vice-bench` cross-checks
//! every value against the gate file, so moving either side alone is red; the
//! §27.7 change-set rule forbids moving both in one commit.

/// Largest accepted angular disagreement at a selected smooth node.
pub const GATE_MAX_G1_SPREAD_RAD: f64 = 1e-9;
/// Minimum population for the exact-G1 clause.
pub const GATE_MIN_G1_NODES: usize = 1;
/// The deliberately malformed IR control must be visibly non-G1.
pub const GATE_MIN_G1_POSITIVE_CONTROL_RAD: f64 = 0.4;
/// Largest movement of a breakpoint as a fraction of chain length.
pub const GATE_MAX_BREAKPOINT_FRACTION_DELTA: f64 = 0.06;
/// A cut experiment must expose at least this much per-cut variation, proving
/// that the rotation leg is not green because every cut is numerically equal.
pub const GATE_MIN_CUT_NONTRIVIAL_SPREAD_BITS: f64 = 1.0;
/// Largest movement of the selected code after rotating the same closed loop.
pub const GATE_MAX_CUT_ROTATION_DELTA_BITS: f64 = 1.0;
/// Largest movement of the selected code under translation.
pub const GATE_MAX_TRANSLATION_DELTA_BITS: f64 = 1e-6;
/// Sample-step, duplicates, cyclic cut, translation, reflection and scale.
pub const GATE_MIN_INVARIANCE_LEGS: usize = 6;
/// The cheap-code knockout must buy at least one additional segment.
pub const GATE_MIN_NO_BIC_EXTRA_SEGMENTS: usize = 1;
