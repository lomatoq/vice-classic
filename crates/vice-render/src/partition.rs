//! Certified partition renderer (spec v1.3 §16.1, §28 M2).
//!
//! For every face — INCLUDING the exterior — coverage is computed
//! independently from that face's own loops (the exterior is `1 + its
//! winding integral`: the whole window plus its negative loops). This
//! independence is what makes the partition checks REAL:
//!
//! - the algebraic identity Σ_faces winding ≡ 0 means a naive
//!   "sum of signed windings" check could never fail — so the renderer
//!   additionally checks each face's coverage stays in `[0, 1]`
//!   (`PartitionRangeViolation` catches overlap/nesting errors like the
//!   REVIEW_M1 B2 scene), and the per-pixel SUM check bounds the f64
//!   accumulation noise and any tessellation inconsistency between the
//!   faces (shared polylines make gaps/overlaps unrepresentable, and the
//!   sum check verifies that claim numerically on every render);
//! - loop-orientation certification (embedding module) rejects the
//!   B4-class scenes before any pixel work.
//!
//! Compositing is premultiplied linear and DIRECT: at every pixel the
//! composite is `Σ_f coverage_f · premul(paint_f)`. At a triple junction
//! the area fractions of all incident faces sum directly — pairwise
//! sequential alpha compositing is NOT a ground-truth partition renderer
//! (spec §16.1) and does not exist in this crate.

use sha2::{Digest, Sha256};
use vice_ir::color::PremulRgba;
use vice_ir::{FaceId, Paint, PixelFilter, ValidatedScene};

use crate::coverage::polygon_coverage;
use crate::domain::NumericDomain;
use crate::embedding::verify_embedding;
use crate::mesh::{RenderMesh, TessellationBudget};
use crate::render_error::RenderError;

/// Typed per-pixel partition tolerances (pixel-area units).
///
/// A tolerance is only meaningful together with the domain it was proven
/// on (F-0008): these values are valid for [`NumericDomain::m2_default`],
/// which the render path ENFORCES with a typed error. Derivation: with the
/// column-local trapezoid the integration error per piece is a few ulp(1)
/// (see `coverage`), and the magnitude-dependent remainder is bounded by
/// `NumericDomain::coverage_error_bound_px` — 2.3e-10 for the default
/// domain. The frozen floor of 1e-9 keeps a 4x margin over that bound and
/// still sits ~9 orders below any REAL geometric violation, which
/// deviates by pixel-area amounts.
///
/// Construct with [`PartitionTolerances::for_domain`] so a widened domain
/// widens the tolerance explicitly instead of silently invalidating it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PartitionTolerances {
    /// Allowed per-pixel deviation of `Σ_f coverage_f` from 1.
    pub sum_abs_tol: f64,
    /// Allowed per-pixel excursion of a face coverage outside `[0, 1]`.
    pub range_tol: f64,
}

impl PartitionTolerances {
    /// The frozen floor: the tolerance for the default M2 domain.
    pub const FROZEN_FLOOR: f64 = 1e-9;

    /// Tolerances for a domain: the frozen floor, or the domain's derived
    /// error bound when a caller deliberately widened the domain past it.
    pub fn for_domain(domain: &NumericDomain) -> PartitionTolerances {
        let t = domain.coverage_error_bound_px().max(Self::FROZEN_FLOOR);
        PartitionTolerances {
            sum_abs_tol: t,
            range_tol: t,
        }
    }
}

impl Default for PartitionTolerances {
    fn default() -> Self {
        PartitionTolerances::for_domain(&NumericDomain::m2_default())
    }
}

/// Options of a partition render.
///
/// `tolerances` and `domain` are PRIVATE and always consistent (F-M2-R9).
/// ADR-0013 states that a tolerance is only meaningful together with the
/// domain it was proven on — but as independent public fields that was a
/// caller's discipline, not a guarantee, and the red team reproduced the
/// original defect straight through the public API by widening the domain
/// while leaving the frozen tolerance in place (1.33e-9 of real error
/// against a declared 1e-9). The pairing is now established by
/// construction: the only way to obtain a `RenderOptions` derives the
/// tolerances from the domain.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderOptions {
    pub budget: TessellationBudget,
    tolerances: PartitionTolerances,
    domain: NumericDomain,
}

impl RenderOptions {
    /// Options for an explicitly chosen numeric domain, with the
    /// tolerances derived from it.
    pub fn for_domain(domain: NumericDomain) -> RenderOptions {
        RenderOptions {
            budget: TessellationBudget::default_m2(),
            tolerances: PartitionTolerances::for_domain(&domain),
            domain,
        }
    }

    /// Same options with a different tessellation budget. The budget is
    /// independent of the domain, so it stays freely settable.
    pub fn with_budget(mut self, budget: TessellationBudget) -> RenderOptions {
        self.budget = budget;
        self
    }

    pub fn domain(&self) -> &NumericDomain {
        &self.domain
    }

    /// The tolerances derived from [`Self::domain`]. Read-only on purpose:
    /// see the type-level note above.
    pub fn tolerances(&self) -> PartitionTolerances {
        self.tolerances
    }
}

impl Default for RenderOptions {
    fn default() -> Self {
        RenderOptions::for_domain(NumericDomain::m2_default())
    }
}

/// Resource limit: total coverage elements (faces × pixels). 2^27 f64s is
/// ~1 GiB across buffers — the M0-lineage "explicit limits before
/// productization" rule applied to the renderer.
pub const MAX_COVERAGE_ELEMENTS: u64 = 1 << 27;

/// A finished partition render.
#[derive(Debug, Clone, PartialEq)]
pub struct PartitionRender {
    pub width_px: u32,
    pub height_px: u32,
    /// Per-face coverage, indexed by `FaceId`, row-major pixels.
    pub face_coverage: Vec<Vec<f64>>,
    /// Premultiplied linear composite, row-major.
    pub composite: Vec<PremulRgba>,
}

/// Coverage of one face over a row band (exterior gets its `+1` window
/// term). Shared with the ROI path so both compute IDENTICAL values.
pub(crate) fn face_coverage_band(
    mesh: &RenderMesh,
    face: usize,
    row_start: u32,
    row_end: u32,
) -> Result<Vec<f64>, RenderError> {
    let refs: Vec<&[vice_geom::Pt]> = mesh.face_loops[face]
        .iter()
        .map(|lp| lp.points.as_slice())
        .collect();
    // The mesh closes every loop bitwise, so this cannot fail in practice;
    // it is propagated rather than unwrapped so no path to a pixel buffer
    // can panic (F-0011).
    let mut cov = polygon_coverage(&refs, mesh.width_px, row_start, row_end)?;
    if FaceId(face as u32) == mesh.exterior {
        for v in &mut cov {
            *v += 1.0;
        }
    }
    Ok(cov)
}

pub(crate) fn premul_of_paint(paint: Paint) -> PremulRgba {
    match paint {
        Paint::OpaqueSolid(c) => PremulRgba {
            r: c.r,
            g: c.g,
            b: c.b,
            a: 1.0,
        },
        Paint::TransparentExterior => PremulRgba {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        },
    }
}

/// Render the full partition of a validated scene: tessellate, certify the
/// embedding, compute per-face coverage, enforce the partition invariants,
/// composite premultiplied.
pub fn render_partition(
    scene: &ValidatedScene,
    options: &RenderOptions,
) -> Result<PartitionRender, RenderError> {
    let filter = scene.scene().formation.pixel_filter;
    if filter != PixelFilter::Box {
        return Err(RenderError::UnsupportedPixelFilter { got: filter });
    }
    let mesh = RenderMesh::build(scene, options.budget)?;
    render_mesh_partition_in_domain(&mesh, options.tolerances, &options.domain)
}

/// Render from an already-built mesh in the default M2 numeric domain.
pub fn render_mesh_partition(
    mesh: &RenderMesh,
    tolerances: PartitionTolerances,
) -> Result<PartitionRender, RenderError> {
    render_mesh_partition_in_domain(mesh, tolerances, &NumericDomain::m2_default())
}

/// Every mesh point and the canvas must lie inside the numeric domain the
/// tolerances were proven on (F-0008). Outside it the accumulator still
/// returns a number, but its error is not bounded by anything the
/// partition guards can see — so the renderer refuses instead.
pub(crate) fn check_numeric_domain(
    mesh: &RenderMesh,
    domain: &NumericDomain,
) -> Result<(), RenderError> {
    for (dim, value) in [
        ("canvas width", mesh.width_px),
        ("canvas height", mesh.height_px),
    ] {
        if value > domain.max_canvas_dim_px {
            return Err(RenderError::OutsideNumericDomain {
                what: dim,
                value: f64::from(value),
                limit: f64::from(domain.max_canvas_dim_px),
            });
        }
    }
    // BOTH mesh point fields are checked, not just the shared polylines:
    // `face_coverage_band` reads `face_loops`, and a check that does not
    // cover what the consumer actually reads is not a check (red team
    // observation on the delta). `RenderMesh` fields are public, so the
    // two can be desynchronised by a direct constructor even though
    // `RenderMesh::build` derives one from the other.
    let polyline_points = mesh.boundary_polylines.iter().flat_map(|p| p.points.iter());
    let loop_points = mesh
        .face_loops
        .iter()
        .flat_map(|loops| loops.iter())
        .flat_map(|lp| lp.points.iter());
    for p in polyline_points.chain(loop_points) {
        for (what, v) in [("mesh point x", p.x), ("mesh point y", p.y)] {
            if !domain.contains_coord(v) {
                return Err(RenderError::OutsideNumericDomain {
                    what,
                    value: v,
                    limit: domain.max_abs_coord_px,
                });
            }
        }
    }
    Ok(())
}

/// Render from an already-built mesh (the seal path re-renders the same
/// fixed tessellation) with an explicit numeric domain.
pub fn render_mesh_partition_in_domain(
    mesh: &RenderMesh,
    tolerances: PartitionTolerances,
    domain: &NumericDomain,
) -> Result<PartitionRender, RenderError> {
    let (w, h) = (mesh.width_px, mesh.height_px);
    let faces = mesh.face_loops.len();
    let elements = u64::from(w) * u64::from(h) * faces as u64;
    if elements > MAX_COVERAGE_ELEMENTS {
        return Err(RenderError::CanvasTooLarge {
            width_px: w,
            height_px: h,
            faces,
            limit_elements: MAX_COVERAGE_ELEMENTS,
        });
    }
    check_numeric_domain(mesh, domain)?;

    // M1-N5 hard gate: loop orientation certification on EVERY render.
    verify_embedding(mesh)?;

    let n_px = (w as usize) * (h as usize);
    let mut face_coverage = Vec::with_capacity(faces);
    for f in 0..faces {
        let cov = face_coverage_band(mesh, f, 0, h)?;
        // Range check: an independently-computed face coverage outside
        // [0,1] means the faces do NOT tile the window (overlap / wrong
        // nesting / self-winding). First offending pixel, deterministic.
        for (i, v) in cov.iter().enumerate() {
            if *v < -tolerances.range_tol || *v > 1.0 + tolerances.range_tol {
                return Err(RenderError::PartitionRangeViolation {
                    face: FaceId(f as u32),
                    x: (i % w as usize) as u32,
                    y: (i / w as usize) as u32,
                    coverage: *v,
                    tolerance: tolerances.range_tol,
                });
            }
        }
        face_coverage.push(cov);
    }

    // Sum check: per pixel, Σ_f coverage = 1 within the typed bound.
    for i in 0..n_px {
        let mut sum = 0.0f64;
        for cov in &face_coverage {
            sum += cov[i];
        }
        if (sum - 1.0).abs() > tolerances.sum_abs_tol {
            return Err(RenderError::PartitionSumViolation {
                x: (i % w as usize) as u32,
                y: (i / w as usize) as u32,
                sum,
                tolerance: tolerances.sum_abs_tol,
            });
        }
    }

    // Premultiplied DIRECT compositing: Σ_f coverage_f · premul(paint_f);
    // face-major fixed order (deterministic reduction, §5.5).
    let paints: Vec<PremulRgba> = mesh.paints.iter().map(|p| premul_of_paint(*p)).collect();
    let mut composite = vec![
        PremulRgba {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0
        };
        n_px
    ];
    for (cov, paint) in face_coverage.iter().zip(&paints) {
        if paint.a == 0.0 && paint.r == 0.0 && paint.g == 0.0 && paint.b == 0.0 {
            continue; // transparent faces contribute nothing
        }
        for (px, c) in composite.iter_mut().zip(cov) {
            px.r += c * paint.r;
            px.g += c * paint.g;
            px.b += c * paint.b;
            px.a += c * paint.a;
        }
    }

    Ok(PartitionRender {
        width_px: w,
        height_px: h,
        face_coverage,
        composite,
    })
}

/// Version tag of the render digest byte layout. Changing the layout or
/// this tag invalidates recorded digests: reviewed change only.
pub const RENDER_DIGEST_SCHEMA: &str = "vice-classic/render/v1";

/// Deterministic digest of the composite bytes: schema tag, dimensions,
/// then row-major premultiplied RGBA as little-endian f64 bit patterns.
pub fn render_digest_sha256(render: &PartitionRender) -> String {
    let mut hasher = Sha256::new();
    hasher.update(RENDER_DIGEST_SCHEMA.as_bytes());
    hasher.update(render.width_px.to_le_bytes());
    hasher.update(render.height_px.to_le_bytes());
    for px in &render.composite {
        for ch in [px.r, px.g, px.b, px.a] {
            hasher.update(ch.to_bits().to_le_bytes());
        }
    }
    hex::encode(hasher.finalize())
}
