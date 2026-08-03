//! vice-render — certified partition renderer of vice-classic (M2).
//!
//! Scope (spec v1.3 §16, §28 M2):
//! - fixed tessellation of a validated scene with certified chord-error
//!   budgets ([`mesh`]; curve math in `vice-geom::flatten`, ADR-0008);
//! - exact signed-area polygon coverage on that fixed tessellation
//!   ([`coverage`]) with the documented out-of-canvas clip policy
//!   (ADR-0009);
//! - per-face + exterior partition coverage, geometric embedding
//!   certification, premultiplied compositing and render digests
//!   ([`partition`], ADR-0010);
//! - ROI rendering with an explicit dependency closure ([`roi`],
//!   ADR-0011);
//! - the M2 seal revalidation skeleton ([`seal`], spec §20.2 in M2 scope).
//!
//! "Exact" means exact area coverage FOR THE FIXED POLYLINE TESSELLATION
//! (spec §16.1); the curve→polyline step carries its own certified budget.
//! Everything is clean-room: no donor code is ported (PORTING_MANIFEST
//! stays at zero units; REVIEW_M0 condition 6). The independent
//! differential court against external rasterizers lives in this crate's
//! integration tests (dev-dependencies only).

#![forbid(unsafe_code)]

pub mod certified;
pub mod coverage;
pub mod domain;
pub mod embedding;
pub mod formation;
pub mod gradient;
pub mod junction;
pub mod mesh;
pub mod partition;
pub mod render_error;
pub mod roi;
pub mod seal;
pub mod stroke;

pub use certified::CertifiedMesh;
pub use coverage::{polygon_coverage, CoverageError};
pub use domain::NumericDomain;
pub use embedding::verify_embedding;
pub use formation::{render_partition_formed, FormationRenderError};
pub use gradient::{
    render_gradient_scene, GradientRender, GradientRenderError, GRADIENT_RENDER_SCHEMA,
};
pub use junction::{
    certify_junction_fractions, JunctionCertificateError, JunctionFractionCertificate,
    JunctionFractionSample, JUNCTION_FRACTION_SCHEMA,
};
pub use mesh::{BoundaryPolyline, LoopPolygon, MeshError, RenderMesh, TessellationBudget};
pub use partition::{
    render_digest_sha256, render_mesh_partition, render_mesh_partition_reusing, render_partition,
    PartitionRender, PartitionRenderWorkspace, PartitionTolerances, RenderOptions,
    MAX_COVERAGE_ELEMENTS, RENDER_DIGEST_SCHEMA,
};
pub use render_error::RenderError;
pub use roi::{render_mesh_partition_roi, render_partition_roi, PixelRect, RoiRender};
pub use seal::{seal_render_cycle, SealError, SealReport};
pub use stroke::{
    render_stroke_scene, StrokeRender, StrokeRenderError, STROKE_RENDER_SCHEMA,
    STROKE_SUPERSAMPLE_SIDE,
};
