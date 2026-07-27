//! The truth an extracted boundary is scored against, and how it is found
//! quickly (spec §13.1).
//!
//! Taken from the shared BOUNDARIES of the planar graph rather than from
//! face loops, and boundaries incident to the exterior face are dropped when
//! the scene's exterior is OPAQUE: there the background face covers the
//! canvas, so its outer ring is the canvas edge and not a visible interface.
//! Including it would let a sample near the canvas match the wrong curve and
//! report a distance SMALLER than the truth — an error in the flattering
//! direction, which is the one worth engineering against.
//!
//! Split from the harness at the §4.1 size seam, and it is a real one: this
//! file knows about geometry and nothing about arms.

use vice_geom::Pt;
use vice_ir::ExteriorModel;

use crate::gt::degradation::DegradationCell;
use crate::gt::grammar::AUTHORING_CANVAS_PX;
use crate::gt::GtScene;

/// The true visible interface of a scene, in RENDER space.
pub fn gt_segments(scene: &GtScene, cell: &DegradationCell) -> Vec<(Pt, Pt)> {
    let scale = f64::from(cell.size_px) / f64::from(AUTHORING_CANVAS_PX);
    let tx = |p: Pt| {
        Pt::new(
            p.x * scale + cell.subpixel_dx,
            p.y * scale + cell.subpixel_dy,
        )
    };
    let graph = scene.scene().graph();
    let mesh = scene.certified().mesh();
    let exterior = graph.exterior;
    let opaque = scene.scene().scene().formation.exterior == ExteriorModel::Opaque;
    let mut out = Vec::new();
    for (bid, b) in graph.boundaries.iter().enumerate() {
        if opaque && (b.left_face == exterior || b.right_face == exterior) {
            continue;
        }
        let poly = &mesh.boundary_polylines[bid];
        for w in poly.points.windows(2) {
            out.push((tx(w[0]), tx(w[1])));
        }
    }
    out
}

/// Squared distance from a point to a segment, and the closest point.
fn point_segment(p: Pt, a: Pt, b: Pt) -> (f64, Pt) {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 <= f64::MIN_POSITIVE {
        0.0
    } else {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0)
    };
    let q = Pt::new(a.x + t * dx, a.y + t * dy);
    ((p.x - q.x).powi(2) + (p.y - q.y).powi(2), q)
}

/// A uniform grid over the truth segments, so the nearest-segment query does
/// not become quadratic on a 128 px render with a few hundred segments.
pub struct SegmentIndex {
    segments: Vec<(Pt, Pt)>,
    cell_px: f64,
    origin: (f64, f64),
    dims: (i64, i64),
    buckets: Vec<Vec<u32>>,
}

impl SegmentIndex {
    pub fn new(segments: Vec<(Pt, Pt)>) -> SegmentIndex {
        let cell_px = 4.0;
        let (mut lo_x, mut lo_y, mut hi_x, mut hi_y) = (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        );
        for (a, b) in &segments {
            lo_x = lo_x.min(a.x).min(b.x);
            lo_y = lo_y.min(a.y).min(b.y);
            hi_x = hi_x.max(a.x).max(b.x);
            hi_y = hi_y.max(a.y).max(b.y);
        }
        if !lo_x.is_finite() {
            return SegmentIndex {
                segments,
                cell_px,
                origin: (0.0, 0.0),
                dims: (0, 0),
                buckets: Vec::new(),
            };
        }
        let nx = (((hi_x - lo_x) / cell_px).ceil() as i64 + 1).max(1);
        let ny = (((hi_y - lo_y) / cell_px).ceil() as i64 + 1).max(1);
        let mut buckets = vec![Vec::new(); (nx * ny) as usize];
        for (k, (a, b)) in segments.iter().enumerate() {
            let x0 = (((a.x.min(b.x) - lo_x) / cell_px).floor() as i64).clamp(0, nx - 1);
            let x1 = (((a.x.max(b.x) - lo_x) / cell_px).floor() as i64).clamp(0, nx - 1);
            let y0 = (((a.y.min(b.y) - lo_y) / cell_px).floor() as i64).clamp(0, ny - 1);
            let y1 = (((a.y.max(b.y) - lo_y) / cell_px).floor() as i64).clamp(0, ny - 1);
            for y in y0..=y1 {
                for x in x0..=x1 {
                    buckets[(y * nx + x) as usize].push(k as u32);
                }
            }
        }
        SegmentIndex {
            segments,
            cell_px,
            origin: (lo_x, lo_y),
            dims: (nx, ny),
            buckets,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Nearest point on the truth, and its distance.
    pub fn nearest(&self, p: Pt) -> Option<(f64, Pt)> {
        if self.segments.is_empty() {
            return None;
        }
        let (nx, ny) = self.dims;
        let cx = (((p.x - self.origin.0) / self.cell_px).floor() as i64).clamp(-1, nx);
        let cy = (((p.y - self.origin.1) / self.cell_px).floor() as i64).clamp(-1, ny);
        let mut best = (f64::INFINITY, p);
        let mut ring = 0i64;
        loop {
            let mut touched = false;
            for y in (cy - ring)..=(cy + ring) {
                for x in (cx - ring)..=(cx + ring) {
                    // Only the new ring.
                    if ring > 0 && (x - cx).abs() < ring && (y - cy).abs() < ring {
                        continue;
                    }
                    if x < 0 || y < 0 || x >= nx || y >= ny {
                        continue;
                    }
                    touched = true;
                    for k in &self.buckets[(y * nx + x) as usize] {
                        let (a, b) = self.segments[*k as usize];
                        let (d2, q) = point_segment(p, a, b);
                        if d2 < best.0 {
                            best = (d2, q);
                        }
                    }
                }
            }
            // Everything outside the searched rings is at least
            // `ring * cell` away, so stop once the best is closer.
            let guaranteed = (ring as f64) * self.cell_px;
            if best.0.is_finite() && best.0.sqrt() <= guaranteed {
                break;
            }
            ring += 1;
            if ring > nx.max(ny) + 1 {
                if !touched && best.0.is_infinite() {
                    return None;
                }
                break;
            }
        }
        if best.0.is_infinite() {
            None
        } else {
            Some((best.0.sqrt(), best.1))
        }
    }
}
