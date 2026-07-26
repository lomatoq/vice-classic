//! Test-support scene builder for the vice-render integration tests.
//!
//! Unlike the vice-ir test builder (disjoint rings only), this one wires
//! ARBITRARY shared-boundary scenes: the caller lists vertices, faces and
//! boundaries (with left/right faces), and `wire_scene` derives half-edges
//! and `next` pointers from the geometric rotation system at each vertex
//! (outgoing directions sorted by angle; `next(h)` = predecessor of
//! `twin(h)` in counterclockwise order — the face-on-the-algebraic-left
//! walk). The result is validated by `vice_ir::ValidatedScene::new`, so a
//! wiring bug fails loudly instead of producing a quietly wrong fixture.
//!
//! Orientation convention exercised here (docs/COORDINATE_CONVENTION.md):
//! a bounded face's outer loop has POSITIVE algebraic signed area; holes
//! and exterior loops are negative.

#![allow(dead_code)] // each integration-test binary uses a subset

use vice_geom::flatten::ChordTolerancePx;
use vice_geom::Pt;
use vice_ir::{
    BlendSpace, Boundary, BoundaryId, Canvas, CurveChain, ExteriorModel, Face, FaceId,
    GlobalFormationHypothesis, GraphVertex, HalfEdge, HalfEdgeId, LinearRgb, Paint, PixelFilter,
    PlanarGraph, QuantizationModel, Segment, ValidatedScene, VectorScene, VertexId,
};

pub struct BoundarySpec {
    pub start: usize,
    pub end: usize,
    /// Face on the algebraic left when traveling start -> end.
    pub left: usize,
    pub right: usize,
    pub chain: CurveChain,
}

pub fn line(start: usize, end: usize, left: usize, right: usize) -> BoundarySpec {
    BoundarySpec {
        start,
        end,
        left,
        right,
        chain: CurveChain::single(Segment::Line),
    }
}

/// Wire a full scene from vertices / paints / boundary specs and validate
/// it. `paints[0]` must be the exterior.
pub fn wire_scene(
    width_px: u32,
    height_px: u32,
    vertices: &[Pt],
    paints: &[Paint],
    bounds: &[BoundarySpec],
) -> ValidatedScene {
    ValidatedScene::new(wire_scene_raw(
        width_px, height_px, vertices, paints, bounds,
    ))
    .expect("test fixture must be a valid scene")
}

/// Same wiring without the validation wrapper (for negative fixtures).
pub fn wire_scene_raw(
    width_px: u32,
    height_px: u32,
    vertices: &[Pt],
    paints: &[Paint],
    bounds: &[BoundarySpec],
) -> VectorScene {
    let boundaries: Vec<Boundary> = bounds
        .iter()
        .map(|b| Boundary {
            left_face: FaceId(b.left as u32),
            right_face: FaceId(b.right as u32),
            start_vertex: VertexId(b.start as u32),
            end_vertex: VertexId(b.end as u32),
            curve: b.chain.clone(),
        })
        .collect();

    // Half-edges: boundary b -> forward 2b, reverse 2b+1.
    let n_he = 2 * boundaries.len();
    let he_boundary = |h: usize| h / 2;
    let he_forward = |h: usize| h.is_multiple_of(2);
    let he_origin = |h: usize| {
        let b = &bounds[he_boundary(h)];
        if he_forward(h) {
            b.start
        } else {
            b.end
        }
    };
    let he_target = |h: usize| {
        let b = &bounds[he_boundary(h)];
        if he_forward(h) {
            b.end
        } else {
            b.start
        }
    };

    // Initial outgoing direction of each half-edge from its origin, taken
    // from a fine flattening of the chain (adequate for test fixtures).
    let tol = ChordTolerancePx::new(1e-3).unwrap();
    let he_direction = |h: usize| -> (f64, f64) {
        let b = &bounds[he_boundary(h)];
        let p_start = vertices[b.start];
        let p_end = vertices[b.end];
        let mut pts: Vec<Pt> = vec![p_start];
        let nodes = b.chain.node_positions(p_start, p_end);
        for (si, seg) in b.chain.segments.iter().enumerate() {
            let flat = flatten_for_direction(seg, nodes[si], nodes[si + 1], tol);
            pts.extend_from_slice(&flat[1..]);
        }
        let (a, bb) = if he_forward(h) {
            (pts[0], pts[1])
        } else {
            (pts[pts.len() - 1], pts[pts.len() - 2])
        };
        (bb.x - a.x, bb.y - a.y)
    };

    // Rotation system: outgoing half-edges per vertex, sorted by algebraic
    // angle (atan2 is fine for distinct directions in test fixtures).
    let mut outgoing: Vec<Vec<usize>> = vec![Vec::new(); vertices.len()];
    for h in 0..n_he {
        outgoing[he_origin(h)].push(h);
    }
    for list in &mut outgoing {
        list.sort_by(|&a, &b| {
            let (ax, ay) = he_direction(a);
            let (bx, by) = he_direction(b);
            ay.atan2(ax).total_cmp(&by.atan2(bx))
        });
    }

    // next(h) = CCW-predecessor of twin(h) among outgoing at target(h).
    let mut next = vec![0usize; n_he];
    for (h, nx) in next.iter_mut().enumerate() {
        let twin = h ^ 1;
        let at = &outgoing[he_target(h)];
        let pos = at.iter().position(|&x| x == twin).expect("twin outgoing");
        *nx = at[(pos + at.len() - 1) % at.len()];
    }

    let half_edges: Vec<HalfEdge> = (0..n_he)
        .map(|h| {
            let b = &bounds[he_boundary(h)];
            HalfEdge {
                boundary: BoundaryId(he_boundary(h) as u32),
                forward: he_forward(h),
                twin: HalfEdgeId((h ^ 1) as u32),
                next: HalfEdgeId(next[h] as u32),
                face: FaceId(if he_forward(h) { b.left } else { b.right } as u32),
            }
        })
        .collect();

    // Face loops: one representative (minimal half-edge id) per next-cycle.
    let mut faces: Vec<Face> = paints
        .iter()
        .map(|p| Face {
            loops: Vec::new(),
            paint: *p,
        })
        .collect();
    let mut seen = vec![false; n_he];
    for start in 0..n_he {
        if seen[start] {
            continue;
        }
        let mut min_h = start;
        let mut cur = start;
        loop {
            seen[cur] = true;
            min_h = min_h.min(cur);
            cur = next[cur];
            if cur == start {
                break;
            }
        }
        let face = half_edges[start].face;
        faces[face.index()].loops.push(HalfEdgeId(min_h as u32));
    }
    for f in &mut faces {
        f.loops.sort();
    }

    VectorScene {
        canvas: Canvas {
            width_px,
            height_px,
        },
        graph: PlanarGraph {
            exterior: FaceId(0),
            vertices: vertices.iter().map(|&pos| GraphVertex { pos }).collect(),
            boundaries,
            half_edges,
            faces,
        },
        formation: formation(),
    }
}

fn flatten_for_direction(seg: &Segment, p0: Pt, p1: Pt, tol: ChordTolerancePx) -> Vec<Pt> {
    use vice_geom::flatten as fl;
    match *seg {
        Segment::Line => vec![p0, p1],
        Segment::Quad { ctrl } => fl::flatten_quad(p0, ctrl, p1, tol).points,
        Segment::Cubic { ctrl1, ctrl2 } => fl::flatten_cubic(p0, ctrl1, ctrl2, p1, tol).points,
        Segment::CircularArc {
            radius_px,
            large_arc,
            ccw,
        } => {
            fl::flatten_circular_arc(p0, p1, radius_px, large_arc, ccw, tol)
                .expect("test arc representable")
                .points
        }
        Segment::EllipticArc {
            rx_px,
            ry_px,
            x_axis_rotation_rad,
            large_arc,
            ccw,
        } => {
            fl::flatten_elliptic_arc(
                p0,
                p1,
                rx_px,
                ry_px,
                x_axis_rotation_rad,
                large_arc,
                ccw,
                tol,
            )
            .expect("test arc representable")
            .points
        }
    }
}

pub fn formation() -> GlobalFormationHypothesis {
    GlobalFormationHypothesis {
        blend_space: BlendSpace::LinearLight,
        pixel_filter: PixelFilter::Box,
        quantization: QuantizationModel::Uint8,
        exterior: ExteriorModel::Transparent,
    }
}

pub fn red() -> Paint {
    Paint::OpaqueSolid(LinearRgb::new(0.8, 0.1, 0.1))
}

pub fn green() -> Paint {
    Paint::OpaqueSolid(LinearRgb::new(0.1, 0.7, 0.2))
}

pub fn blue() -> Paint {
    Paint::OpaqueSolid(LinearRgb::new(0.1, 0.2, 0.9))
}

pub fn transparent() -> Paint {
    Paint::TransparentExterior
}

/// One axis-aligned rectangle island `[x0,x1]×[y0,y1]` on canvas `w×h`.
/// Vertex order gives the interior a POSITIVE algebraic signed area.
pub fn rect_scene(
    w: u32,
    h: u32,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    paint: Paint,
) -> ValidatedScene {
    let vs = [
        Pt::new(x0, y0),
        Pt::new(x1, y0),
        Pt::new(x1, y1),
        Pt::new(x0, y1),
    ];
    wire_scene(
        w,
        h,
        &vs,
        &[transparent(), paint],
        &[
            line(0, 1, 1, 0),
            line(1, 2, 1, 0),
            line(2, 3, 1, 0),
            line(3, 0, 1, 0),
        ],
    )
}

/// Arbitrary simple polygon island (vertices in positive-signed-area
/// order).
pub fn polygon_scene(w: u32, h: u32, vs: &[Pt], paint: Paint) -> ValidatedScene {
    let n = vs.len();
    let bounds: Vec<BoundarySpec> = (0..n).map(|i| line(i, (i + 1) % n, 1, 0)).collect();
    wire_scene(w, h, vs, &[transparent(), paint], &bounds)
}

/// A donut: outer square island with a transparent square hole
/// (three faces: exterior, ring, hole).
pub fn donut_scene(w: u32, h: u32, x0: f64, y0: f64, size: f64, margin: f64) -> ValidatedScene {
    let (ox1, oy1) = (x0 + size, y0 + size);
    let (ix0, iy0) = (x0 + margin, y0 + margin);
    let (ix1, iy1) = (ox1 - margin, oy1 - margin);
    let vs = [
        Pt::new(x0, y0),
        Pt::new(ox1, y0),
        Pt::new(ox1, oy1),
        Pt::new(x0, oy1),
        Pt::new(ix0, iy0),
        Pt::new(ix1, iy0),
        Pt::new(ix1, iy1),
        Pt::new(ix0, iy1),
    ];
    wire_scene(
        w,
        h,
        &vs,
        &[transparent(), red(), transparent()],
        &[
            // Outer ring: ring face (1) inside, exterior (0) outside.
            line(0, 1, 1, 0),
            line(1, 2, 1, 0),
            line(2, 3, 1, 0),
            line(3, 0, 1, 0),
            // Inner ring: hole face (2) inside, ring (1) outside.
            line(4, 5, 2, 1),
            line(5, 6, 2, 1),
            line(6, 7, 2, 1),
            line(7, 4, 2, 1),
        ],
    )
}

/// Two rectangles sharing one vertical edge (three faces: exterior, left
/// rect, right rect; the shared boundary is stored ONCE).
pub fn shared_edge_scene(w: u32, h: u32) -> ValidatedScene {
    let vs = [
        Pt::new(8.0, 8.0),   // 0
        Pt::new(24.0, 8.0),  // 1
        Pt::new(40.0, 8.0),  // 2
        Pt::new(40.0, 24.0), // 3
        Pt::new(24.0, 24.0), // 4
        Pt::new(8.0, 24.0),  // 5
    ];
    wire_scene(
        w,
        h,
        &vs,
        &[transparent(), red(), blue()],
        &[
            line(0, 1, 1, 0), // top left
            line(1, 2, 2, 0), // top right
            line(2, 3, 2, 0), // right
            line(3, 4, 2, 0), // bottom right
            line(4, 5, 1, 0), // bottom left
            line(5, 0, 1, 0), // left
            // SHARED edge, walked downward: the algebraic left of (0,+1)
            // is the -x side, i.e. the LEFT rect (face 1).
            line(1, 4, 1, 2),
        ],
    )
}

/// Triple junction: a square split into three sectors meeting at the
/// center (four faces: exterior + three sectors). The junction point is
/// strictly inside a pixel so area fractions at the junction pixel are
/// properly fractional.
pub fn triple_junction_scene(w: u32, h: u32) -> ValidatedScene {
    let c = Pt::new(24.5, 24.5);
    let vs = [
        Pt::new(8.0, 8.0),   // 0 top-left
        Pt::new(41.0, 8.0),  // 1 top-right
        Pt::new(41.0, 41.0), // 2 bottom-right
        Pt::new(8.0, 41.0),  // 3 bottom-left
        c,                   // 4 center
        Pt::new(24.5, 8.0),  // 5 top-mid
    ];
    // Sectors: A (face 1) top-left region, B (face 2) top-right region,
    // C (face 3) bottom region.
    wire_scene(
        w,
        h,
        &vs,
        &[transparent(), red(), green(), blue()],
        &[
            line(0, 5, 1, 0), // top edge, left half
            line(5, 1, 2, 0), // top edge, right half
            line(1, 2, 2, 0), // right edge (sector B)
            line(2, 3, 3, 0), // bottom edge (sector C)
            line(3, 0, 1, 0), // left edge (sector A)
            line(5, 4, 1, 2), // spoke top-mid -> center (A left, B right)
            line(4, 2, 3, 2), // spoke center -> bottom-right (C left, B right)
            line(4, 3, 1, 3), // spoke center -> bottom-left (A left, C right)
        ],
    )
}

// ---------------------------------------------------------------------
// Independent exact coverage reference (Sutherland–Hodgman clipping).
//
// A SECOND implementation of the same mathematical quantity by a
// different algorithm: instead of a scanline sweep with trapezoids and a
// per-row difference fold, each closed loop is clipped against each pixel
// box by four half-planes and the shoelace area of the clipped polygon is
// taken. For a closed loop `signed_area(L ∩ box) = ∫∫_box w_L dA`, so the
// sum over loops is exactly what the accumulator claims to compute — with
// no scanline, no trapezoid, no fold, and no shared code.
//
// Used as the arbiter wherever an external rasterizer is too coarse to
// judge us (8-bit alpha) or is itself the thing under suspicion.
// ---------------------------------------------------------------------

/// Clip `poly` (closed, first == last not required) to the half-plane
/// selected by `keep`, Sutherland–Hodgman.
fn clip_half_plane(
    poly: &[Pt],
    keep: impl Fn(Pt) -> bool,
    isect: impl Fn(Pt, Pt) -> Pt,
) -> Vec<Pt> {
    let mut out = Vec::with_capacity(poly.len() + 4);
    if poly.is_empty() {
        return out;
    }
    let mut prev = poly[poly.len() - 1];
    let mut prev_in = keep(prev);
    for &cur in poly {
        let cur_in = keep(cur);
        if cur_in {
            if !prev_in {
                out.push(isect(prev, cur));
            }
            out.push(cur);
        } else if prev_in {
            out.push(isect(prev, cur));
        }
        prev = cur;
        prev_in = cur_in;
    }
    out
}

/// Interpolation parameter, conditioned the same way the renderer's slope
/// is (F-0013): the plain differences are used whenever both are finite —
/// so ordinary scenes are unchanged — and halved operands otherwise, which
/// is exact and cannot overflow.
fn clip_parameter(value: f64, from: f64, to: f64) -> f64 {
    let num = value - from;
    let den = to - from;
    if num.is_finite() && den.is_finite() {
        if den == 0.0 {
            return 0.0;
        }
        return num / den;
    }
    let den_half = to * 0.5 - from * 0.5;
    if den_half == 0.0 {
        return 0.0;
    }
    (value * 0.5 - from * 0.5) / den_half
}

/// Interpolate between `a` and `b` (F-M2-R12).
///
/// The exact cases are taken exactly, which matters more than it looks:
/// for an AXIS-ALIGNED edge clipped in the other axis `a == b`, and the
/// difference form `a + t(b - a)` returns `a` exactly while a convex
/// combination `a(1-t) + b t` rounds each product at magnitude `|a|` and
/// loses ulps. Measuring only that class is what hid F-M2-R12 in the
/// first place, so both classes are handled deliberately:
///
/// - `a == b`, or an endpoint parameter: exact by construction;
/// - `b - a` finite: the difference form, best conditioned when the two
///   are close, and identical to what this reference did before;
/// - otherwise (operands spanning the exponent range, where `b - a`
///   overflows): the convex form, whose every term is bounded by
///   `max(|a|, |b|)`.
fn mix(a: f64, b: f64, t: f64) -> f64 {
    if a == b || t <= 0.0 {
        return a;
    }
    if t >= 1.0 {
        return b;
    }
    let diff = b - a;
    if diff.is_finite() {
        a + t * diff
    } else {
        a * (1.0 - t) + b * t
    }
}

fn lerp_x(a: Pt, b: Pt, x: f64) -> Pt {
    let t = clip_parameter(x, a.x, b.x);
    Pt::new(x, mix(a.y, b.y, t))
}

fn lerp_y(a: Pt, b: Pt, y: f64) -> Pt {
    let t = clip_parameter(y, a.y, b.y);
    Pt::new(mix(a.x, b.x, t), y)
}

fn shoelace(poly: &[Pt]) -> f64 {
    if poly.len() < 3 {
        return 0.0;
    }
    let mut s = 0.0;
    let mut prev = poly[poly.len() - 1];
    for &p in poly {
        s += prev.x * p.y - p.x * prev.y;
        prev = p;
    }
    0.5 * s
}

/// Exact per-pixel winding integral of `loops` over a `w x h` canvas,
/// computed by per-pixel polygon clipping. Row-major, like the renderer.
pub fn sh_clip_coverage(loops: &[&[Pt]], w: u32, h: u32) -> Vec<f64> {
    sh_clip_coverage_window(loops, 0, 0, w, h)
}

/// The same reference over an arbitrary pixel WINDOW `[ox, ox+w) x
/// [oy, oy+h)`, so the arbiter can be exercised at large pixel
/// coordinates without paying for the whole canvas.
///
/// CONDITIONING (F-M2-R8 / M2-A-N10). The arbiter must be more accurate
/// than the thing it judges, and the naive form is not: taking the
/// shoelace in ABSOLUTE coordinates squares the magnitude (terms ~M^2),
/// so its own error grows as eps*M^2 and overtakes the renderer's — the
/// red team measured their own reference at 1e-7 for |x| = 1e9 before
/// fixing it the same way, and both cold contexts independently measured
/// this one going 7.3x WORSE than the renderer at the declared domain
/// edge. Two properties restore it:
///   1. a clipped coordinate is SET to the clip value, never interpolated
///      (already true of `lerp_x`/`lerp_y` below);
///   2. the clipped polygon is translated to the pixel origin before the
///      shoelace. Every clipped point lies in the pixel box, so for pixel
///      index >= 1 the subtraction is exact by Sterbenz and for index 0 it
///      is trivially exact; the shoelace then runs entirely on operands in
///      [0, 1] and its error is a few ulp(1) regardless of where the pixel
///      sits.
pub fn sh_clip_coverage_window(loops: &[&[Pt]], ox: u32, oy: u32, w: u32, h: u32) -> Vec<f64> {
    let mut out = vec![0.0f64; (w as usize) * (h as usize)];
    for lp in loops {
        // Drop the duplicated closing vertex if present.
        let poly: Vec<Pt> = if lp.len() >= 2 && lp[0] == lp[lp.len() - 1] {
            lp[..lp.len() - 1].to_vec()
        } else {
            lp.to_vec()
        };
        for py in 0..h {
            let (y0, y1) = (f64::from(oy + py), f64::from(oy + py) + 1.0);
            for px in 0..w {
                let (x0, x1) = (f64::from(ox + px), f64::from(ox + px) + 1.0);
                let c = clip_half_plane(&poly, |p| p.x >= x0, |a, b| lerp_x(a, b, x0));
                let c = clip_half_plane(&c, |p| p.x <= x1, |a, b| lerp_x(a, b, x1));
                let c = clip_half_plane(&c, |p| p.y >= y0, |a, b| lerp_y(a, b, y0));
                let c = clip_half_plane(&c, |p| p.y <= y1, |a, b| lerp_y(a, b, y1));
                // Translate to the pixel origin (exact) before the shoelace.
                let local: Vec<Pt> = c.iter().map(|p| Pt::new(p.x - x0, p.y - y0)).collect();
                out[(py as usize) * (w as usize) + px as usize] += shoelace(&local);
            }
        }
    }
    out
}
