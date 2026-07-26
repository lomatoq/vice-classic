//! Negative validation tests: every invariant class of spec §12 that M1
//! checks gets a scene that is valid EXCEPT for exactly one broken
//! invariant, and the reject must be the matching typed error — never a
//! panic, never a silent pass.

mod common;

use common::*;
use vice_geom::Pt;
use vice_ir::{
    validate_scene, BoundaryId, ChainNode, CurveChain, Face, FaceId, GraphError, GraphVertex,
    HalfEdgeId, JoinKind, LinearRgb, Paint, PixelFilter, SceneError, Segment, VertexId,
};

fn expect_graph_err(scene: &vice_ir::VectorScene) -> GraphError {
    match validate_scene(scene) {
        Err(SceneError::Graph(e)) => e,
        other => panic!("expected graph reject, got {other:?}"),
    }
}

#[test]
fn valid_scenes_pass() {
    validate_scene(&empty_scene()).unwrap();
    validate_scene(&one_square_scene()).unwrap();
    validate_scene(&build_scene(
        128,
        96,
        &[
            square_with_hole(Pt::new(4.0, 4.0), 24.0, 6.0, red()),
            mixed_island(Pt::new(60.0, 30.0), blue()),
            loop_island(Pt::new(100.0, 70.0), 5.0, red()),
        ],
    ))
    .unwrap();
}

// ---------------------------------------------------------------------------
// Number rules (§5.5)
// ---------------------------------------------------------------------------

#[test]
fn nan_vertex_rejected() {
    let mut s = one_square_scene();
    s.graph.vertices[1].pos.x = f64::NAN;
    match validate_scene(&s) {
        Err(SceneError::NonFinite { location }) => {
            assert!(location.contains("vertices[1].pos.x"), "{location}");
        }
        other => panic!("expected NonFinite, got {other:?}"),
    }
}

#[test]
fn infinite_control_point_rejected() {
    let mut s = build_scene(128, 96, &[mixed_island(Pt::new(20.0, 20.0), red())]);
    // mixed_island boundary 2 is the cubic.
    if let Segment::Cubic { ctrl1, .. } = &mut s.graph.boundaries[2].curve.segments[0] {
        ctrl1.y = f64::INFINITY;
    } else {
        panic!("expected cubic at boundary 2");
    }
    match validate_scene(&s) {
        Err(SceneError::NonFinite { location }) => {
            assert!(location.contains("segments[0].ctrl1.y"), "{location}");
        }
        other => panic!("expected NonFinite, got {other:?}"),
    }
}

#[test]
fn negative_zero_rejected() {
    let mut s = one_square_scene();
    s.graph.vertices[0].pos.y = -0.0;
    match validate_scene(&s) {
        Err(SceneError::NegativeZero { location }) => {
            assert!(location.contains("vertices[0].pos.y"), "{location}");
        }
        other => panic!("expected NegativeZero, got {other:?}"),
    }
}

#[test]
fn nan_paint_rejected() {
    let mut s = one_square_scene();
    s.graph.faces[1].paint = Paint::OpaqueSolid(LinearRgb::new(0.5, f64::NAN, 0.5));
    assert!(matches!(
        validate_scene(&s),
        Err(SceneError::NonFinite { .. })
    ));
}

// ---------------------------------------------------------------------------
// Canvas / formation
// ---------------------------------------------------------------------------

#[test]
fn empty_canvas_rejected() {
    let mut s = empty_scene();
    s.canvas.width_px = 0;
    assert!(matches!(
        validate_scene(&s),
        Err(SceneError::EmptyCanvas { .. })
    ));
}

#[test]
fn nonpositive_gaussian_sigma_rejected() {
    let mut s = empty_scene();
    s.formation.pixel_filter = PixelFilter::Gaussian { sigma_px: 0.0 };
    assert!(matches!(
        validate_scene(&s),
        Err(SceneError::Formation { .. })
    ));
}

// ---------------------------------------------------------------------------
// Face / paint invariants
// ---------------------------------------------------------------------------

#[test]
fn opaque_exterior_rejected() {
    let mut s = one_square_scene();
    s.graph.faces[0].paint = Paint::OpaqueSolid(red());
    assert!(matches!(
        expect_graph_err(&s),
        GraphError::ExteriorNotTransparent(FaceId(0))
    ));
}

#[test]
fn out_of_range_paint_rejected() {
    let mut s = one_square_scene();
    s.graph.faces[1].paint = Paint::OpaqueSolid(LinearRgb::new(1.5, 0.0, 0.0));
    assert!(matches!(
        expect_graph_err(&s),
        GraphError::PaintOutOfRange(FaceId(1))
    ));
}

#[test]
fn face_without_loops_rejected() {
    let mut s = one_square_scene();
    s.graph.faces.push(Face {
        loops: Vec::new(),
        paint: Paint::OpaqueSolid(blue()),
    });
    assert!(matches!(
        expect_graph_err(&s),
        GraphError::FaceWithoutLoops(FaceId(2))
    ));
}

#[test]
fn exterior_id_out_of_range_rejected() {
    let mut s = one_square_scene();
    s.graph.exterior = FaceId(99);
    assert!(matches!(
        expect_graph_err(&s),
        GraphError::IdOutOfRange { .. }
    ));
}

// ---------------------------------------------------------------------------
// Vertex invariants
// ---------------------------------------------------------------------------

#[test]
fn duplicate_vertex_position_rejected() {
    let mut s = one_square_scene();
    s.graph.vertices[2].pos = s.graph.vertices[0].pos;
    assert!(matches!(
        expect_graph_err(&s),
        GraphError::DuplicateVertexPosition(VertexId(0), VertexId(2))
    ));
}

#[test]
fn isolated_vertex_rejected() {
    let mut s = one_square_scene();
    s.graph.vertices.push(GraphVertex {
        pos: Pt::new(50.0, 50.0),
    });
    assert!(matches!(
        expect_graph_err(&s),
        GraphError::IsolatedVertex(VertexId(4))
    ));
}

// ---------------------------------------------------------------------------
// Boundary / chain invariants
// ---------------------------------------------------------------------------

#[test]
fn dangling_crack_rejected() {
    let mut s = one_square_scene();
    s.graph.boundaries[0].right_face = s.graph.boundaries[0].left_face;
    assert!(matches!(
        expect_graph_err(&s),
        GraphError::DanglingCrack(BoundaryId(0), FaceId(1))
    ));
}

#[test]
fn duplicate_boundary_rejected() {
    let mut s = one_square_scene();
    let dup = s.graph.boundaries[0].clone();
    s.graph.boundaries.push(dup);
    assert!(matches!(
        expect_graph_err(&s),
        GraphError::DuplicateBoundary(BoundaryId(0), BoundaryId(4))
    ));
}

#[test]
fn empty_chain_rejected() {
    let mut s = one_square_scene();
    s.graph.boundaries[1].curve = CurveChain {
        interior_nodes: Vec::new(),
        segments: Vec::new(),
    };
    assert!(matches!(
        expect_graph_err(&s),
        GraphError::EmptyChain(BoundaryId(1))
    ));
}

#[test]
fn chain_arity_mismatch_rejected() {
    let mut s = one_square_scene();
    s.graph.boundaries[0].curve.interior_nodes.push(ChainNode {
        pos: Pt::new(1.0, 1.0),
        join: JoinKind::Corner,
    });
    assert!(matches!(
        expect_graph_err(&s),
        GraphError::ChainArityMismatch { .. }
    ));
}

#[test]
fn degenerate_segment_rejected() {
    let mut s = one_square_scene();
    // Interior node placed exactly on the start vertex -> zero-length span.
    let start = s.graph.vertices[0].pos;
    s.graph.boundaries[0].curve = CurveChain {
        interior_nodes: vec![ChainNode {
            pos: start,
            join: JoinKind::Corner,
        }],
        segments: vec![Segment::Line, Segment::Line],
    };
    assert!(matches!(
        expect_graph_err(&s),
        GraphError::DegenerateSegment {
            boundary: BoundaryId(0),
            segment: 0
        }
    ));
}

#[test]
fn unrepresentable_circular_arc_rejected() {
    let mut s = one_square_scene();
    // Chord is 16 px; radius 4 -> 2r = 8 < 16.
    s.graph.boundaries[0].curve = CurveChain::single(Segment::CircularArc {
        radius_px: 4.0,
        large_arc: false,
        ccw: true,
    });
    match expect_graph_err(&s) {
        GraphError::InvalidSegmentParams { reason, .. } => {
            assert!(reason.contains("not representable"), "{reason}");
        }
        other => panic!("expected InvalidSegmentParams, got {other:?}"),
    }
}

#[test]
fn nonpositive_radius_rejected() {
    let mut s = one_square_scene();
    s.graph.boundaries[0].curve = CurveChain::single(Segment::CircularArc {
        radius_px: 0.0,
        large_arc: false,
        ccw: true,
    });
    assert!(matches!(
        expect_graph_err(&s),
        GraphError::InvalidSegmentParams { .. }
    ));
}

#[test]
fn elliptic_arc_rotation_out_of_range_rejected() {
    let mut s = one_square_scene();
    s.graph.boundaries[0].curve = CurveChain::single(Segment::EllipticArc {
        rx_px: 20.0,
        ry_px: 20.0,
        x_axis_rotation_rad: 3.5, // >= pi
        large_arc: false,
        ccw: true,
    });
    assert!(matches!(
        expect_graph_err(&s),
        GraphError::InvalidSegmentParams { .. }
    ));
}

#[test]
fn infeasible_elliptic_endpoints_rejected() {
    let mut s = one_square_scene();
    // Chord 16 px but tiny radii: lambda > 1.
    s.graph.boundaries[0].curve = CurveChain::single(Segment::EllipticArc {
        rx_px: 2.0,
        ry_px: 2.0,
        x_axis_rotation_rad: 0.0,
        large_arc: false,
        ccw: true,
    });
    match expect_graph_err(&s) {
        GraphError::InvalidSegmentParams { reason, .. } => {
            assert!(reason.contains("lambda"), "{reason}");
        }
        other => panic!("expected InvalidSegmentParams, got {other:?}"),
    }
}

#[test]
fn tangent_angle_out_of_range_rejected() {
    let mut s = one_square_scene();
    s.graph.boundaries[0].curve = CurveChain {
        interior_nodes: vec![ChainNode {
            pos: Pt::new(16.0, 6.0),
            join: JoinKind::SmoothG1 {
                tangent_angle_rad: 4.0, // > pi
            },
        }],
        segments: vec![Segment::Line, Segment::Line],
    };
    assert!(matches!(
        expect_graph_err(&s),
        GraphError::TangentAngleOutOfRange {
            boundary: BoundaryId(0),
            node: 0
        }
    ));
}

// ---------------------------------------------------------------------------
// Half-edge invariants
// ---------------------------------------------------------------------------

#[test]
fn missing_half_edge_rejected() {
    let mut s = one_square_scene();
    s.graph.half_edges.pop();
    // The dangling twin reference is an id error or a count error depending
    // on which id was popped; both are typed rejects.
    assert!(matches!(
        expect_graph_err(&s),
        GraphError::HalfEdgeCount { .. } | GraphError::IdOutOfRange { .. }
    ));
}

#[test]
fn self_twin_rejected() {
    let mut s = one_square_scene();
    s.graph.half_edges[0].twin = HalfEdgeId(0);
    assert!(matches!(
        expect_graph_err(&s),
        GraphError::TwinViolation(HalfEdgeId(0), _)
    ));
}

#[test]
fn non_involutive_twin_rejected() {
    let mut s = one_square_scene();
    s.graph.half_edges[1].twin = HalfEdgeId(3);
    assert!(matches!(
        expect_graph_err(&s),
        GraphError::TwinViolation(..)
    ));
}

#[test]
fn boundary_cover_violation_rejected() {
    let mut s = build_scene(
        64,
        64,
        &[
            square_island(Pt::new(4.0, 4.0), 8.0, red()),
            square_island(Pt::new(20.0, 20.0), 8.0, blue()),
        ],
    );
    // Point the second boundary's half-edge PAIR (4,5) at boundary 0:
    // twin checks stay green pairwise, but boundary 1 loses its cover.
    s.graph.half_edges[4].boundary = BoundaryId(0);
    s.graph.half_edges[5].boundary = BoundaryId(0);
    assert!(matches!(
        expect_graph_err(&s),
        GraphError::BoundaryHalfEdgeCover(..)
    ));
}

#[test]
fn half_edge_face_mismatch_rejected() {
    let mut s = one_square_scene();
    s.graph.half_edges[0].face = FaceId(0); // forward must carry left = island
    assert!(matches!(
        expect_graph_err(&s),
        GraphError::HalfEdgeFaceMismatch(HalfEdgeId(0))
    ));
}

#[test]
fn next_not_permutation_rejected() {
    let mut s = one_square_scene();
    let target = s.graph.half_edges[0].next;
    s.graph.half_edges[2].next = target; // two predecessors for one half-edge
    assert!(matches!(
        expect_graph_err(&s),
        GraphError::NextNotPermutation(..)
    ));
}

#[test]
fn next_endpoint_mismatch_rejected() {
    let mut s = one_square_scene();
    // Forward loop is 0 -> 2 -> 4 -> 6 -> 0. Swap the next-targets of
    // half-edges 0 and 4: still a permutation, but the walk teleports.
    let n0 = s.graph.half_edges[0].next;
    let n4 = s.graph.half_edges[4].next;
    s.graph.half_edges[0].next = n4;
    s.graph.half_edges[4].next = n0;
    assert!(matches!(
        expect_graph_err(&s),
        GraphError::NextEndpointMismatch { .. }
    ));
}

#[test]
fn cycle_mixing_faces_rejected() {
    // Two-vertex, two-boundary island: rewire next so one cycle walks both
    // the island side and the exterior side. Endpoints stay consistent, so
    // only the cycle-face check can catch it.
    let island = Island {
        vertices: vec![Pt::new(10.0, 10.0), Pt::new(20.0, 10.0)],
        chains: vec![
            CurveChain::single(Segment::Quad {
                ctrl: Pt::new(15.0, 4.0),
            }),
            CurveChain::single(Segment::Quad {
                ctrl: Pt::new(15.0, 16.0),
            }),
        ],
        color: red(),
        hole: None,
    };
    let mut s = build_scene(32, 32, &[island]);
    // Half-edges: f0=0, r0=1, f1=2, r1=3. Original cycles: (0 2) and (1 3).
    // New wiring: 0 -> 1 -> 3 -> 2 -> 0 keeps every endpoint contact.
    s.graph.half_edges[0].next = HalfEdgeId(1);
    s.graph.half_edges[1].next = HalfEdgeId(3);
    s.graph.half_edges[3].next = HalfEdgeId(2);
    s.graph.half_edges[2].next = HalfEdgeId(0);
    assert!(matches!(
        expect_graph_err(&s),
        GraphError::CycleFaceMismatch(..)
    ));
}

#[test]
fn face_loops_mismatch_rejected() {
    let mut s = build_scene(
        64,
        64,
        &[
            square_island(Pt::new(4.0, 4.0), 8.0, red()),
            square_island(Pt::new(20.0, 20.0), 8.0, blue()),
        ],
    );
    // Drop one of the exterior's two loops.
    s.graph.faces[0].loops.pop();
    assert!(matches!(
        expect_graph_err(&s),
        GraphError::FaceLoopsMismatch(FaceId(0), _)
    ));
}

// ---------------------------------------------------------------------------
// Euler (planarity of the rotation system)
// ---------------------------------------------------------------------------

#[test]
fn torus_rotation_system_rejected_by_euler() {
    // One vertex, three self-loop boundaries, wired as two triangle walks
    // (fa fb fc) and (ra rc rb): every local invariant holds, but this is a
    // genus-1 (torus) embedding: V - E + F = 1 - 3 + 2 = 0 != 1 + C = 2.
    use vice_ir::{Boundary, HalfEdge};
    let v = Pt::new(10.0, 10.0);
    let mk_chain = |dy: f64| CurveChain {
        interior_nodes: vec![ChainNode {
            pos: Pt::new(14.0, 10.0 + dy),
            join: JoinKind::Corner,
        }],
        segments: vec![
            Segment::Quad {
                ctrl: Pt::new(12.0, 8.0 + dy),
            },
            Segment::Quad {
                ctrl: Pt::new(12.0, 12.0 + dy),
            },
        ],
    };
    let mut s = empty_scene();
    let g = &mut s.graph;
    g.vertices.push(GraphVertex { pos: v });
    let p = FaceId(1);
    g.faces = vec![
        Face {
            loops: vec![HalfEdgeId(0)],
            paint: Paint::TransparentExterior,
        },
        Face {
            loops: vec![HalfEdgeId(1)],
            paint: Paint::OpaqueSolid(red()),
        },
    ];
    g.exterior = FaceId(0);
    for i in 0..3u32 {
        g.boundaries.push(Boundary {
            left_face: FaceId(0),
            right_face: p,
            start_vertex: VertexId(0),
            end_vertex: VertexId(0),
            curve: mk_chain(f64::from(i) * 9.0),
        });
    }
    // forward = 2i, reverse = 2i+1. Walks: (f0 f1 f2) on face 0 and
    // (r0 r2 r1) on face 1.
    for i in 0..3u32 {
        g.half_edges.push(HalfEdge {
            boundary: BoundaryId(i),
            forward: true,
            twin: HalfEdgeId(2 * i + 1),
            next: HalfEdgeId(2 * ((i + 1) % 3)),
            face: FaceId(0),
        });
        g.half_edges.push(HalfEdge {
            boundary: BoundaryId(i),
            forward: false,
            twin: HalfEdgeId(2 * i),
            next: HalfEdgeId(2 * ((i + 2) % 3) + 1),
            face: p,
        });
    }
    assert!(matches!(
        expect_graph_err(&s),
        GraphError::EulerMismatch {
            v: 1,
            e: 3,
            f: 2,
            c: 1
        }
    ));
}

// ---------------------------------------------------------------------------
// Geometric interference (exact line-line; conservative boxes otherwise)
// ---------------------------------------------------------------------------

#[test]
fn crossing_line_boundaries_rejected() {
    let mut s = build_scene(
        64,
        64,
        &[
            square_island(Pt::new(4.0, 4.0), 8.0, red()),
            square_island(Pt::new(20.0, 20.0), 8.0, blue()),
        ],
    );
    // Drag the second square's first vertex across the first square.
    s.graph.vertices[4].pos = Pt::new(6.0, 6.0);
    assert!(matches!(
        expect_graph_err(&s),
        GraphError::BoundariesIntersect { .. }
    ));
}

#[test]
fn touching_line_boundaries_rejected() {
    let mut s = build_scene(
        64,
        64,
        &[
            square_island(Pt::new(4.0, 4.0), 8.0, red()),
            square_island(Pt::new(20.0, 20.0), 8.0, blue()),
        ],
    );
    // T-touch: a vertex of the second square lands ON an edge of the first.
    s.graph.vertices[4].pos = Pt::new(8.0, 12.0);
    assert!(matches!(
        expect_graph_err(&s),
        GraphError::BoundariesIntersect { .. }
    ));
}

#[test]
fn collinear_overlap_within_chain_rejected() {
    // A chain that doubles back along its own line: consecutive segments
    // share their interior node and overlap collinearly beyond it. Caught
    // by the exact shared-endpoint overlap predicate.
    let mut s = one_square_scene();
    s.graph.boundaries[0].curve = CurveChain {
        interior_nodes: vec![ChainNode {
            pos: Pt::new(30.0, 8.0),
            join: JoinKind::Corner,
        }],
        segments: vec![Segment::Line, Segment::Line],
    };
    assert!(matches!(
        expect_graph_err(&s),
        GraphError::CollinearOverlap { .. }
    ));
}

#[test]
fn collinear_overlapping_islands_rejected() {
    // Distinct vertices, no shared endpoints: two islands whose bottom
    // edges overlap along one line. Exact line-line test must reject.
    let a = Island {
        vertices: vec![Pt::new(0.0, 0.0), Pt::new(10.0, 0.0), Pt::new(5.0, 5.0)],
        chains: vec![
            CurveChain::single(Segment::Line),
            CurveChain::single(Segment::Line),
            CurveChain::single(Segment::Line),
        ],
        color: red(),
        hole: None,
    };
    let b = Island {
        vertices: vec![Pt::new(4.0, 0.0), Pt::new(14.0, 0.0), Pt::new(9.0, -6.0)],
        chains: vec![
            CurveChain::single(Segment::Line),
            CurveChain::single(Segment::Line),
            CurveChain::single(Segment::Line),
        ],
        color: blue(),
        hole: None,
    };
    let s = build_scene(64, 64, &[a, b]);
    assert!(matches!(
        expect_graph_err(&s),
        GraphError::BoundariesIntersect { .. }
    ));
}

#[test]
fn uncertified_pairs_are_reported_not_hidden() {
    // Two islands whose curved segments have overlapping conservative
    // boxes: M1 cannot certify them; the pair must be REPORTED as
    // undetermined (M2+ worklist), while validation still passes.
    let s = build_scene(
        128,
        128,
        &[
            mixed_island(Pt::new(20.0, 20.0), red()),
            mixed_island(Pt::new(36.0, 20.0), blue()),
        ],
    );
    validate_scene(&s).unwrap();
    let pairs = vice_ir::uncertified_interference_pairs(&s.graph);
    assert!(
        !pairs.is_empty(),
        "expected undetermined curve pairs to be reported"
    );
    // And a well-separated pair of squares (lines only) has none.
    let clean = build_scene(
        64,
        64,
        &[
            square_island(Pt::new(4.0, 4.0), 8.0, red()),
            square_island(Pt::new(40.0, 40.0), 8.0, blue()),
        ],
    );
    validate_scene(&clean).unwrap();
    assert!(vice_ir::uncertified_interference_pairs(&clean.graph).is_empty());
}
