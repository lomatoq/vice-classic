//! Fixed tessellation of a validated scene (spec v1.3 §16.1, §16.4).
//!
//! A [`RenderMesh`] is the FIXED polyline tessellation on which coverage is
//! exact. Two structural guarantees make the partition property possible at
//! all:
//!
//! 1. **Each boundary is tessellated exactly once.** Both adjacent faces
//!    reference the same polyline (the reverse side walks it backwards), so
//!    a shared boundary cannot produce cracks or overlaps between its two
//!    faces by construction.
//! 2. **Chain endpoints pass through bitwise.** Flattening keeps the exact
//!    shared graph-vertex / interior-node positions as polyline vertices
//!    (vice-geom::flatten contract), so junctions are watertight.
//!
//! Budget honesty: every flattened segment carries a CERTIFIED deviation
//! bound; if it exceeds the requested [`TessellationBudget`] (possible only
//! at the subdivision cap), mesh construction fails with a typed error —
//! the mesh never silently under-delivers the budget it was asked for.

use vice_geom::flatten::{
    flatten_circular_arc, flatten_cubic, flatten_elliptic_arc, flatten_quad, ArcError,
    ChordTolerancePx, FlattenedCurve,
};
use vice_geom::Pt;
use vice_ir::{BoundaryId, FaceId, HalfEdgeId, Paint, Segment, ValidatedScene};

/// Typed tessellation budget of a render (spec §5.4: typed tolerances).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TessellationBudget {
    pub chord_tolerance: ChordTolerancePx,
}

impl TessellationBudget {
    /// The default M2 judging budget: 1/64 px chord error.
    ///
    /// What that budget actually feeds (corrected per REVIEW_M2_A
    /// M2-A-N3; the previous wording claimed it stays "well below the
    /// partition/coverage tolerances", which is wrong by ~9 orders and
    /// gave the reader a false model of how the budgets relate): the
    /// certified AREA budget of a canvas-scale curve at this tolerance is
    /// ~1.55 px² for a disk of r = 10, versus a partition tolerance of
    /// 1e-9. The two are unrelated quantities. The area budget is the
    /// uncertainty term in loop-orientation CERTIFICATION
    /// (`embedding::verify_embedding`), where it is compared against a
    /// loop's own |signed area| — 1.55 px² against a disk area of 314 px²
    /// is the meaningful comparison. The partition tolerances bound f64
    /// accumulation on a FIXED tessellation and are set by the numeric
    /// domain instead (ADR-0013).
    ///
    /// Consequence worth knowing (red team's observation): since the
    /// bound is Σ(2δ·|chord| + 4δ²), a face is not certifiable at this
    /// budget if its area is ≲ perimeter/32 px². Small detail therefore
    /// needs an explicitly tighter budget — a refusal in the safe
    /// direction, not a silent one.
    pub fn default_m2() -> TessellationBudget {
        TessellationBudget {
            chord_tolerance: ChordTolerancePx::new(1.0 / 64.0).expect("static tolerance"),
        }
    }

    pub fn with_chord_tolerance_px(px: f64) -> Option<TessellationBudget> {
        ChordTolerancePx::new(px).map(|chord_tolerance| TessellationBudget { chord_tolerance })
    }
}

/// Typed mesh-construction failure.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum MeshError {
    #[error(
        "tessellation budget exceeded at boundary {boundary:?} segment {segment}: certified {certified_px} px > requested {requested_px} px (subdivision cap)"
    )]
    BudgetExceeded {
        boundary: BoundaryId,
        segment: usize,
        certified_px: f64,
        requested_px: f64,
    },
    #[error("arc at boundary {boundary:?} segment {segment}: {source}")]
    Arc {
        boundary: BoundaryId,
        segment: usize,
        source: ArcError,
    },
}

/// The shared polyline of one boundary, tessellated once.
#[derive(Debug, Clone, PartialEq)]
pub struct BoundaryPolyline {
    /// `>= 2` points from the boundary's start vertex to its end vertex;
    /// first/last and every interior chain-node position are bitwise the
    /// graph's shared parameters.
    pub points: Vec<Pt>,
    /// Max certified chord deviation (px) over the chain's segments.
    pub max_deviation_px: f64,
    /// Certified bound (px²) on the area between the true chain and this
    /// polyline (summed over segments).
    pub area_error_bound_px2: f64,
}

/// One face loop as a closed polygon over shared boundary polylines.
#[derive(Debug, Clone, PartialEq)]
pub struct LoopPolygon {
    /// Closed: `points[0] == points[last]` (bitwise). Orientation is the
    /// face-on-the-algebraic-left walk of the half-edge cycle.
    pub points: Vec<Pt>,
    /// Sum of the certified area budgets of the traversed boundaries.
    pub area_error_bound_px2: f64,
}

/// The fixed tessellation of a validated scene.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderMesh {
    pub width_px: u32,
    pub height_px: u32,
    pub exterior: FaceId,
    pub paints: Vec<Paint>,
    /// Indexed by `BoundaryId`.
    pub boundary_polylines: Vec<BoundaryPolyline>,
    /// Indexed by `FaceId`: the face's loops as closed polygons.
    pub face_loops: Vec<Vec<LoopPolygon>>,
    /// The budget this mesh certifiably met.
    pub budget: TessellationBudget,
}

pub(crate) fn flatten_segment(
    seg: &Segment,
    p0: Pt,
    p1: Pt,
    tol: ChordTolerancePx,
) -> Result<FlattenedCurve, ArcError> {
    Ok(match *seg {
        Segment::Line => FlattenedCurve {
            points: vec![p0, p1],
            max_deviation_px: 0.0,
        },
        Segment::Quad { ctrl } => flatten_quad(p0, ctrl, p1, tol),
        Segment::Cubic { ctrl1, ctrl2 } => flatten_cubic(p0, ctrl1, ctrl2, p1, tol),
        Segment::CircularArc {
            radius_px,
            large_arc,
            ccw,
        } => flatten_circular_arc(p0, p1, radius_px, large_arc, ccw, tol)?,
        Segment::EllipticArc {
            rx_px,
            ry_px,
            x_axis_rotation_rad,
            large_arc,
            ccw,
        } => flatten_elliptic_arc(
            p0,
            p1,
            rx_px,
            ry_px,
            x_axis_rotation_rad,
            large_arc,
            ccw,
            tol,
        )?,
    })
}

impl RenderMesh {
    /// Tessellate a validated scene once, deterministically (boundaries in
    /// id order, segments in chain order).
    pub fn build(
        scene: &ValidatedScene,
        budget: TessellationBudget,
    ) -> Result<RenderMesh, MeshError> {
        let g = scene.graph();
        let tol = budget.chord_tolerance;

        let mut boundary_polylines = Vec::with_capacity(g.boundaries.len());
        for (bi, b) in g.boundaries.iter().enumerate() {
            let bid = BoundaryId(bi as u32);
            let nodes = b.curve.node_positions(
                g.vertices[b.start_vertex.index()].pos,
                g.vertices[b.end_vertex.index()].pos,
            );
            let mut points: Vec<Pt> = Vec::new();
            let mut max_dev = 0.0f64;
            let mut area_budget = 0.0f64;
            for (si, seg) in b.curve.segments.iter().enumerate() {
                let flat =
                    flatten_segment(seg, nodes[si], nodes[si + 1], tol).map_err(|source| {
                        MeshError::Arc {
                            boundary: bid,
                            segment: si,
                            source,
                        }
                    })?;
                if flat.max_deviation_px > tol.px() {
                    return Err(MeshError::BudgetExceeded {
                        boundary: bid,
                        segment: si,
                        certified_px: flat.max_deviation_px,
                        requested_px: tol.px(),
                    });
                }
                max_dev = max_dev.max(flat.max_deviation_px);
                area_budget += flat.area_error_bound_px2();
                // Segments share their junction node: drop the duplicate.
                let skip = usize::from(si > 0);
                points.extend_from_slice(&flat.points[skip..]);
            }
            boundary_polylines.push(BoundaryPolyline {
                points,
                max_deviation_px: max_dev,
                area_error_bound_px2: area_budget,
            });
        }

        // Face loops: walk each `next` cycle once, concatenating the shared
        // polylines (reversed on the backward side). Deterministic: faces in
        // id order, loop representatives in the face's stored order.
        let mut face_loops: Vec<Vec<LoopPolygon>> = Vec::with_capacity(g.faces.len());
        for f in &g.faces {
            let mut loops = Vec::with_capacity(f.loops.len());
            for &rep in &f.loops {
                loops.push(walk_loop(g, &boundary_polylines, rep));
            }
            face_loops.push(loops);
        }

        Ok(RenderMesh {
            width_px: scene.scene().canvas.width_px,
            height_px: scene.scene().canvas.height_px,
            exterior: g.exterior,
            paints: g.faces.iter().map(|f| f.paint).collect(),
            boundary_polylines,
            face_loops,
            budget,
        })
    }
}

fn walk_loop(
    g: &vice_ir::PlanarGraph,
    polylines: &[BoundaryPolyline],
    rep: HalfEdgeId,
) -> LoopPolygon {
    let mut points: Vec<Pt> = Vec::new();
    let mut area_budget = 0.0f64;
    let mut cur = rep;
    loop {
        let he = &g.half_edges[cur.index()];
        let poly = &polylines[he.boundary.index()];
        area_budget += poly.area_error_bound_px2;
        // Append this half-edge's traversal WITHOUT its final point (the
        // next half-edge starts there); the loop is closed at the end.
        if he.forward {
            points.extend_from_slice(&poly.points[..poly.points.len() - 1]);
        } else {
            for p in poly.points[1..].iter().rev() {
                points.push(*p);
            }
        }
        cur = he.next;
        if cur == rep {
            break;
        }
    }
    // Close bitwise.
    let first = points[0];
    points.push(first);
    LoopPolygon {
        points,
        area_error_bound_px2: area_budget,
    }
}
