//! ROI rendering with an explicit dependency closure (spec §16.1 "ROI с
//! dependency closure", §28 M2; design in ADR-0011).
//!
//! Dependency structure of the coverage accumulator: the value of pixel
//! `(c, r)` is assembled ONLY from edge pieces with `y ∈ [r, r+1]` and
//! column `>= c` (the winding ray looks to the right). The closure of an
//! ROI window is therefore its ROW BAND (rows `y0..y1`, any column); the
//! implementation computes that band exactly as the full render computes
//! those rows — same pieces, same order, same f64 operations — and then
//! slices the columns. Consequence: an ROI render equals the full render
//! restricted to the window BIT-FOR-BIT, which the gate test asserts with
//! exact equality, not a tolerance.
//!
//! Certificate scope (documented, not hidden): an ROI render certifies
//! the loop-orientation embedding GLOBALLY (it is cheap and geometric)
//! but the per-pixel partition range/sum invariants WITHIN THE WINDOW
//! ONLY. A nesting violation whose pixels lie outside the window is not
//! observable from that window; full-scene certification is the full
//! render's job (spec §18.3 runs periodic full checks for exactly this
//! reason).

use vice_ir::color::PremulRgba;
use vice_ir::{FaceId, PixelFilter, ValidatedScene};

use crate::domain::NumericDomain;
use crate::embedding::verify_embedding;
use crate::mesh::RenderMesh;
use crate::partition::{face_coverage_band, premul_of_paint, PartitionTolerances, RenderOptions};
use crate::render_error::RenderError;

/// Half-open pixel window `[x0, x1) × [y0, y1)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelRect {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
}

impl PixelRect {
    pub fn width(&self) -> u32 {
        self.x1 - self.x0
    }

    pub fn height(&self) -> u32 {
        self.y1 - self.y0
    }
}

/// A finished ROI render.
#[derive(Debug, Clone, PartialEq)]
pub struct RoiRender {
    pub rect: PixelRect,
    /// Per-face coverage of the window, indexed by `FaceId`, row-major
    /// within the window.
    pub face_coverage: Vec<Vec<f64>>,
    /// Premultiplied linear composite of the window, row-major.
    pub composite: Vec<PremulRgba>,
}

/// Render one ROI window of a validated scene.
pub fn render_partition_roi(
    scene: &ValidatedScene,
    options: &RenderOptions,
    roi: PixelRect,
) -> Result<RoiRender, RenderError> {
    let filter = scene.scene().formation.pixel_filter;
    if filter != PixelFilter::Box {
        return Err(RenderError::UnsupportedPixelFilter { got: filter });
    }
    let mesh = RenderMesh::build(scene, options.budget)?;
    render_mesh_partition_roi_in_domain(&mesh, options.tolerances, &options.domain, roi)
}

/// ROI render from an already-built mesh, in the default M2 domain.
pub fn render_mesh_partition_roi(
    mesh: &RenderMesh,
    tolerances: PartitionTolerances,
    roi: PixelRect,
) -> Result<RoiRender, RenderError> {
    render_mesh_partition_roi_in_domain(mesh, tolerances, &NumericDomain::m2_default(), roi)
}

/// ROI render from an already-built mesh with an explicit numeric domain.
pub fn render_mesh_partition_roi_in_domain(
    mesh: &RenderMesh,
    tolerances: PartitionTolerances,
    domain: &NumericDomain,
    roi: PixelRect,
) -> Result<RoiRender, RenderError> {
    let (w, h) = (mesh.width_px, mesh.height_px);
    if roi.x0 >= roi.x1 || roi.y0 >= roi.y1 || roi.x1 > w || roi.y1 > h {
        return Err(RenderError::RoiInvalid {
            x0: roi.x0,
            y0: roi.y0,
            x1: roi.x1,
            y1: roi.y1,
            width_px: w,
            height_px: h,
        });
    }
    let faces = mesh.face_loops.len();
    let elements = u64::from(w) * u64::from(roi.height()) * faces as u64;
    if elements > crate::partition::MAX_COVERAGE_ELEMENTS {
        return Err(RenderError::CanvasTooLarge {
            width_px: w,
            height_px: roi.height(),
            faces,
            limit_elements: crate::partition::MAX_COVERAGE_ELEMENTS,
        });
    }

    // The numeric domain and the global geometric certification stay on:
    // neither M1-N5 nor F-0008 is optional on the ROI path.
    crate::partition::check_numeric_domain(mesh, domain)?;
    verify_embedding(mesh)?;

    let (rw, rh) = (roi.width() as usize, roi.height() as usize);
    let full_w = w as usize;
    let mut face_coverage: Vec<Vec<f64>> = Vec::with_capacity(faces);
    for f in 0..faces {
        // Dependency closure: the row band at FULL width, computed exactly
        // like the corresponding rows of the full render.
        let band = face_coverage_band(mesh, f, roi.y0, roi.y1);
        // Column slice of the window.
        let mut cov = Vec::with_capacity(rw * rh);
        for r in 0..rh {
            let row = &band[r * full_w..(r + 1) * full_w];
            cov.extend_from_slice(&row[roi.x0 as usize..roi.x1 as usize]);
        }
        // Range check within the window.
        for (i, v) in cov.iter().enumerate() {
            if *v < -tolerances.range_tol || *v > 1.0 + tolerances.range_tol {
                return Err(RenderError::PartitionRangeViolation {
                    face: FaceId(f as u32),
                    x: roi.x0 + (i % rw) as u32,
                    y: roi.y0 + (i / rw) as u32,
                    coverage: *v,
                    tolerance: tolerances.range_tol,
                });
            }
        }
        face_coverage.push(cov);
    }

    // Sum check within the window.
    for i in 0..rw * rh {
        let mut sum = 0.0f64;
        for cov in &face_coverage {
            sum += cov[i];
        }
        if (sum - 1.0).abs() > tolerances.sum_abs_tol {
            return Err(RenderError::PartitionSumViolation {
                x: roi.x0 + (i % rw) as u32,
                y: roi.y0 + (i / rw) as u32,
                sum,
                tolerance: tolerances.sum_abs_tol,
            });
        }
    }

    // Same face-major direct compositing as the full render.
    let paints: Vec<PremulRgba> = mesh.paints.iter().map(|p| premul_of_paint(*p)).collect();
    let mut composite = vec![
        PremulRgba {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0
        };
        rw * rh
    ];
    for (cov, paint) in face_coverage.iter().zip(&paints) {
        if paint.a == 0.0 && paint.r == 0.0 && paint.g == 0.0 && paint.b == 0.0 {
            continue;
        }
        for (px, c) in composite.iter_mut().zip(cov) {
            px.r += c * paint.r;
            px.g += c * paint.g;
            px.b += c * paint.b;
            px.a += c * paint.a;
        }
    }

    Ok(RoiRender {
        rect: roi,
        face_coverage,
        composite,
    })
}
