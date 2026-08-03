//! vice-ir — canonical visible-scene IR of vice-classic (M1).
//!
//! Scope (spec v1.3 §6, §28 M1):
//! - canonical color vocabulary and the color-space conventions
//!   ([`color`]);
//! - the complementary digital-connectivity convention ([`connectivity`]);
//! - typed curve grammar: segments, joins with shared tangent parameters,
//!   curve chains ([`curve`]);
//! - the shared planar graph with the exterior as a REAL `FaceId`
//!   ([`graph`]);
//! - canvas / minimal global formation hypothesis / scene ([`scene`]);
//! - typed validation of the §12 invariants — an invalid graph is a typed
//!   reject, never a panic ([`validate`]);
//! - canonical serialization + sha256 digests, the M1 seal skeleton
//!   ([`canonical`]).
//!
//! Deliberate M1 boundaries (documented, spec §32 rule 7 — no API without a
//! call site):
//! - no `SceneHypothesis`/posterior fields — there is no posterior producer
//!   until M5+;
//! - no `EvidenceRef`/`SupportRef`/`SceneProvenance` — no evidence system
//!   until M4;
//! - geometric non-intersection of NON-LINE segment pairs is certified only
//!   via conservative bounding boxes; full certified curve-curve
//!   intersection machinery is M2+ (see docs/ADR/ADR-0005).
//!
//! Everything is clean-room; PORTING_MANIFEST.toml stays at zero units.

#![forbid(unsafe_code)]

pub mod canonical;
pub mod color;
pub mod connectivity;
pub mod curve;
pub mod graph;
pub mod interference;
pub mod scene;
pub mod validate;

pub use canonical::{
    canonical_scene_bytes, canonicalize_graph, parse_scene, scene_digest_sha256, ParseError,
    SCENE_SCHEMA,
};
pub use color::{BlendSpace, LinearRgb, LinearRgba, PremulRgba};
// Re-exported at the root in M4.5, which is the first consumer outside this
// crate: the topology stage takes the complementary pair as a TYPE (§5.3) so
// that a non-complementary convention stays unrepresentable across the crate
// boundary too, not only inside this one.
pub use connectivity::{ComplementaryConnectivity, PixelConnectivity};
pub use curve::{ChainNode, CurveChain, JoinKind, Segment};
pub use graph::{
    Boundary, BoundaryId, Face, FaceId, GraphVertex, HalfEdge, HalfEdgeId, Paint, PlanarGraph,
    VertexId,
};
pub use interference::{uncertified_interference_pairs, UncertifiedPair};
pub use scene::{
    Canvas, CodecResidualModel, ExteriorModel, GlobalFormationHypothesis, PixelFilter,
    QuantizationModel, ResizeChain, VectorScene,
};
pub use validate::{
    validate_graph, validate_scene, ChainPointRef, GraphError, SceneError, ValidatedScene,
};
