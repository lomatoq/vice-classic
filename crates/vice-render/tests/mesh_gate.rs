//! Gate tests for the fixed tessellation (spec §16.1, §28 M2 "bounded
//! curve tessellation"): shared boundaries tessellate once and crack-free,
//! certified budgets are honored or fail typed, loops close bitwise.

mod common;

use common::*;
use vice_geom::Pt;
use vice_ir::{CurveChain, Segment, ValidatedScene};
use vice_render::{MeshError, RenderMesh, TessellationBudget};

fn budget(px: f64) -> TessellationBudget {
    TessellationBudget::with_chord_tolerance_px(px).unwrap()
}

/// Two rectangles sharing a CURVED boundary (quad bulge): the shared
/// polyline exists once and both face loops traverse bit-identical points.
fn curved_shared_scene() -> ValidatedScene {
    let vs = [
        Pt::new(8.0, 8.0),
        Pt::new(24.0, 8.0),
        Pt::new(40.0, 8.0),
        Pt::new(40.0, 24.0),
        Pt::new(24.0, 24.0),
        Pt::new(8.0, 24.0),
    ];
    let mut bounds = vec![
        line(0, 1, 1, 0),
        line(1, 2, 2, 0),
        line(2, 3, 2, 0),
        line(3, 4, 2, 0),
        line(4, 5, 1, 0),
        line(5, 0, 1, 0),
    ];
    bounds.push(BoundarySpec {
        start: 1,
        end: 4,
        left: 1,
        right: 2,
        chain: CurveChain::single(Segment::Quad {
            ctrl: Pt::new(30.0, 16.0),
        }),
    });
    wire_scene(48, 32, &vs, &[transparent(), red(), blue()], &bounds)
}

#[test]
fn shared_curved_boundary_is_tessellated_once_and_crack_free() {
    let scene = curved_shared_scene();
    let mesh = RenderMesh::build(&scene, budget(0.05)).unwrap();

    let shared = &mesh.boundary_polylines[6];
    assert!(shared.points.len() > 3, "quad actually subdivided");
    assert!(shared.max_deviation_px <= 0.05);
    // Endpoints are the exact graph vertices (bitwise).
    assert_eq!(shared.points[0], Pt::new(24.0, 8.0));
    assert_eq!(*shared.points.last().unwrap(), Pt::new(24.0, 24.0));

    // Face 1 (left side of the shared boundary) walks the shared polyline
    // FORWARD, face 2 REVERSED; both use exactly the same evaluated points
    // — cracks are unrepresentable.
    let loop1 = &mesh.face_loops[1][0];
    let loop2 = &mesh.face_loops[2][0];
    let fwd: Vec<Pt> = shared.points.clone();
    let rev: Vec<Pt> = shared.points.iter().rev().copied().collect();
    assert!(
        contains_subsequence(&loop1.points, &fwd),
        "face 1 loop must contain the forward shared polyline"
    );
    assert!(
        contains_subsequence(&loop2.points, &rev),
        "face 2 loop must contain the reversed shared polyline"
    );
}

fn contains_subsequence(haystack: &[Pt], needle: &[Pt]) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }
    (0..=haystack.len() - needle.len()).any(|i| &haystack[i..i + needle.len()] == needle)
}

#[test]
fn loops_close_bitwise_and_use_exact_vertices() {
    let scene = triple_junction_scene(49, 49);
    let mesh = RenderMesh::build(&scene, budget(0.05)).unwrap();
    for (fi, loops) in mesh.face_loops.iter().enumerate() {
        assert!(!loops.is_empty(), "face {fi} has loops");
        for lp in loops {
            assert_eq!(lp.points.first(), lp.points.last(), "closed bitwise");
            assert!(lp.points.len() >= 4);
        }
    }
    // Junction vertex appears bitwise in all three sector loops.
    let c = Pt::new(24.5, 24.5);
    for fi in 1..=3 {
        assert!(
            mesh.face_loops[fi][0].points.contains(&c),
            "sector {fi} touches the junction bitwise"
        );
    }
}

#[test]
fn budget_is_honored_for_all_boundaries() {
    let vs = [Pt::new(10.0, 30.0), Pt::new(50.0, 30.0)];
    let bounds = vec![
        BoundarySpec {
            start: 0,
            end: 1,
            left: 1,
            right: 0,
            chain: CurveChain::single(Segment::CircularArc {
                radius_px: 25.0,
                large_arc: false,
                ccw: true,
            }),
        },
        BoundarySpec {
            start: 1,
            end: 0,
            left: 1,
            right: 0,
            chain: CurveChain::single(Segment::Cubic {
                ctrl1: Pt::new(40.0, 55.0),
                ctrl2: Pt::new(20.0, 55.0),
            }),
        },
    ];
    let scene = wire_scene(64, 64, &vs, &[transparent(), red()], &bounds);
    for tol in [0.5, 0.05, 0.005] {
        let mesh = RenderMesh::build(&scene, budget(tol)).unwrap();
        for bp in &mesh.boundary_polylines {
            assert!(
                bp.max_deviation_px <= tol,
                "certified {} <= {tol}",
                bp.max_deviation_px
            );
            assert!(bp.area_error_bound_px2 > 0.0);
        }
    }
    // Finer budget, more points.
    let coarse = RenderMesh::build(&scene, budget(0.5)).unwrap();
    let fine = RenderMesh::build(&scene, budget(0.005)).unwrap();
    assert!(fine.boundary_polylines[0].points.len() > coarse.boundary_polylines[0].points.len());
}

#[test]
fn unmeetable_budget_is_a_typed_error_not_a_silent_underdelivery() {
    // Second difference ~2000 px with a 1e-9 px tolerance needs ~700k
    // pieces; the 2^14 cap binds and the certified bound honestly exceeds
    // the request -> typed MeshError::BudgetExceeded.
    let vs = [Pt::new(0.0, 500.0), Pt::new(1000.0, 500.0)];
    let bounds = vec![
        BoundarySpec {
            start: 0,
            end: 1,
            left: 1,
            right: 0,
            chain: CurveChain::single(Segment::Quad {
                ctrl: Pt::new(500.0, -500.0),
            }),
        },
        line(1, 0, 1, 0),
    ];
    let scene = wire_scene(1000, 1000, &vs, &[transparent(), red()], &bounds);
    match RenderMesh::build(&scene, budget(1e-9)) {
        Err(MeshError::BudgetExceeded {
            certified_px,
            requested_px,
            ..
        }) => {
            assert!(certified_px > requested_px);
        }
        other => panic!("expected BudgetExceeded, got {other:?}"),
    }
}
