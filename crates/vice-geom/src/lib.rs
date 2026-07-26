//! vice-geom — fixed geometric conventions for vice-classic (M1).
//!
//! Scope (spec v1.3 §5, §28 M1):
//! - the frozen coordinate frame and the ONLY pixel↔canvas transform module
//!   ([`coords`]);
//! - `Vec2`/`Pt` and the basic clean-room vector operations ([`vec2`]);
//! - robust adaptive-precision geometric predicates behind a thin typed
//!   adapter ([`predicates`]), so combinatorial topology decisions never
//!   depend on `abs(cross) < 1e-9` in plain f64.
//!
//! Everything here is clean-room: no code from the pinned donor repositories
//! is ported (PORTING_MANIFEST.toml stays at zero units in M1). The exact
//! predicates come from the external OSS crate `robust` (a Rust port of
//! Shewchuk's public-domain predicates); see THIRD_PARTY_NOTICES.md and
//! docs/ADR/ADR-0004-robust-predicates.md.
//!
//! M2 adds [`flatten`]: certified curve→polyline flattening (spec §16.1 —
//! curve tessellation with a certified chord-error budget; derivations in
//! docs/ADR/ADR-0008). Curves belong to this crate per the target layout
//! (spec §4: vice-geom = "Vec2, robust predicates, curves, intersections").
//!
//! Explicitly NOT here (their milestones have not started): certified
//! curve-curve intersection, coverage rasterization (vice-render), any
//! evidence machinery.

#![forbid(unsafe_code)]

pub mod coords;
pub mod flatten;
pub mod predicates;
pub mod vec2;

pub use coords::Aabb;
pub use flatten::{ChordTolerancePx, FlattenedCurve};
pub use vec2::{is_negative_zero, Pt, Vec2};
