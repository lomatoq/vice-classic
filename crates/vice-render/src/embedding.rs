//! Geometric embedding certification of face loops (REVIEW_M1 M1-N5).
//!
//! M1 validated the COMBINATORIAL side of §12 only; the reviewer's B4/B2
//! scenes proved that a combinatorially perfect graph can still fail to be
//! a planar embedding. M2 closes this with two complementary HARD checks
//! on the render path:
//!
//! 1. **Loop orientation (this module).** With the face on the algebraic
//!    left of every half-edge, a bounded face has EXACTLY ONE loop of
//!    positive signed area (its outer boundary) and only negative loops
//!    otherwise (holes); the exterior face has only negative loops. The
//!    sign is CERTIFIED: the polyline shoelace area must exceed the
//!    certified tessellation area budget plus an f64 shoelace rounding
//!    bound, otherwise the scene is rejected as uncertifiable rather than
//!    silently classified.
//! 2. **Region consistency (partition module).** Loop orientation cannot
//!    see nesting errors (an island drawn inside another island but wired
//!    to the exterior — reviewer scene B2 — has perfectly oriented loops).
//!    Those violate per-pixel coverage bounds instead: some face's
//!    independently-computed coverage leaves `[0, 1]`, which
//!    `render_partition` rejects (`PartitionRangeViolation`). See
//!    ADR-0010 for why the pair of checks is the right split.
//!
//! Both checks run on EVERY render and on the seal path — a hard gate,
//! not an advisory report.

use vice_ir::FaceId;

use crate::mesh::{LoopPolygon, RenderMesh};
use crate::render_error::RenderError;

/// Signed area (shoelace) of a closed polyline plus a conservative bound
/// on its own f64 rounding error.
pub(crate) fn signed_area_with_rounding_bound(lp: &LoopPolygon) -> (f64, f64) {
    let pts = &lp.points;
    let mut sum = 0.0f64;
    let mut max_mag = 1.0f64;
    for w in pts.windows(2) {
        sum += w[0].x * w[1].y - w[1].x * w[0].y;
        max_mag = max_mag
            .max(w[0].x.abs())
            .max(w[0].y.abs())
            .max(w[1].x.abs())
            .max(w[1].y.abs());
    }
    let n = pts.len() as f64;
    // Each term is two products (relative error <= 2^-52 each) plus one
    // subtraction and one accumulation; 8·eps·max² per term is a
    // comfortable upper bound on the total absolute error.
    let rounding = 8.0 * f64::EPSILON * n * max_mag * max_mag;
    (0.5 * sum, 0.5 * rounding)
}

/// Certify the loop-orientation structure of every face of the mesh.
pub fn verify_embedding(mesh: &RenderMesh) -> Result<(), RenderError> {
    for (fi, loops) in mesh.face_loops.iter().enumerate() {
        let face = FaceId(fi as u32);
        let is_exterior = face == mesh.exterior;
        let mut positive_loops = 0usize;
        for (li, lp) in loops.iter().enumerate() {
            let (area, rounding) = signed_area_with_rounding_bound(lp);
            // The polyline area may differ from the true curved-loop area
            // by at most the certified tessellation budget.
            let certified = lp.area_error_bound_px2 + rounding;
            if area.abs() <= certified {
                return Err(RenderError::UncertifiableLoopOrientation {
                    face,
                    loop_index: li,
                    signed_area_px2: area,
                    certified_bound_px2: certified,
                });
            }
            if area > 0.0 {
                if is_exterior {
                    return Err(RenderError::ExteriorPositiveLoop {
                        loop_index: li,
                        signed_area_px2: area,
                    });
                }
                positive_loops += 1;
            }
        }
        if !is_exterior && positive_loops != 1 {
            return Err(RenderError::FaceLoopStructure {
                face,
                positive_loops,
                total_loops: loops.len(),
            });
        }
    }
    Ok(())
}
