//! M2 gate tests "area / translation / half-pixel" (spec §28 M2) on the
//! real mesh path, judged against ANALYTIC truths (π·r², exact rectangle
//! areas) — not against the renderer itself.

mod common;

use common::*;
use vice_geom::flatten::{flatten_circular_arc, ChordTolerancePx};
use vice_geom::Pt;
use vice_render::{polygon_coverage, RenderMesh, TessellationBudget};

fn budget(px: f64) -> TessellationBudget {
    TessellationBudget::with_chord_tolerance_px(px).unwrap()
}

/// Circle → π·r²: the flattened-circle coverage total must match the
/// ANALYTIC disk area within the CERTIFIED tessellation area budget (plus
/// f64 accumulation slack) — and the budget must shrink with tolerance.
#[test]
fn circle_area_matches_pi_r_squared_within_certified_budget() {
    let (cx, cy, r) = (16.0, 16.0, 10.0);
    let (p0, p1) = (Pt::new(cx - r, cy), Pt::new(cx + r, cy));
    let mut previous_error = f64::INFINITY;
    for tol_px in [0.5, 0.05, 0.005] {
        let tol = ChordTolerancePx::new(tol_px).unwrap();
        let half1 = flatten_circular_arc(p0, p1, r, false, true, tol).unwrap();
        let half2 = flatten_circular_arc(p1, p0, r, false, true, tol).unwrap();
        let mut lp = half1.points.clone();
        lp.extend_from_slice(&half2.points[1..]); // closes bitwise at p0
        let area_budget = half1.area_error_bound_px2() + half2.area_error_bound_px2();

        let cov = polygon_coverage(&[&lp], 32, 0, 32);
        let total: f64 = cov.iter().sum();
        let exact = std::f64::consts::PI * r * r;
        let error = (total - exact).abs();
        assert!(
            error <= area_budget + 1e-9,
            "tol {tol_px}: |{total} - {exact}| = {error} > certified budget {area_budget}"
        );
        assert!(error <= previous_error + 1e-12, "error shrinks with tol");
        previous_error = error;

        // Coverage values are honest area fractions.
        for v in &cov {
            assert!(*v >= -1e-12 && *v <= 1.0 + 1e-12);
        }
    }
}

/// Rectangles at half-pixel offsets through the FULL mesh path: interior
/// pixels exactly 1.0, edge pixels exactly 0.5, corners exactly 0.25.
#[test]
fn half_pixel_rect_is_exact_through_the_mesh_path() {
    let scene = rect_scene(8, 8, 1.5, 1.5, 4.5, 4.5, red());
    let mesh = RenderMesh::build(&scene, budget(0.05)).unwrap();
    let lp = &mesh.face_loops[1][0];
    let cov = polygon_coverage(&[&lp.points], 8, 0, 8);
    let px = |x: usize, y: usize| cov[y * 8 + x];
    assert_eq!(px(1, 1), 0.25);
    assert_eq!(px(2, 1), 0.5);
    assert_eq!(px(4, 1), 0.25);
    assert_eq!(px(1, 2), 0.5);
    assert_eq!(px(2, 2), 1.0);
    assert_eq!(px(3, 3), 1.0);
    assert_eq!(px(4, 4), 0.25);
    let total: f64 = cov.iter().sum();
    assert_eq!(total, 9.0);
}

/// Exact rectangle area at ARBITRARY subpixel offsets, and translation
/// continuity: a tiny shift changes per-pixel coverage by at most the
/// shift (per crossing edge).
#[test]
fn rect_area_exact_and_translation_continuous_through_the_mesh_path() {
    let (x0, y0, x1, y1) = (2.3, 1.7, 6.55, 5.2);
    let scene = rect_scene(10, 10, x0, y0, x1, y1, red());
    let mesh = RenderMesh::build(&scene, budget(0.05)).unwrap();
    let cov = polygon_coverage(&[&mesh.face_loops[1][0].points], 10, 0, 10);
    let total: f64 = cov.iter().sum();
    assert!((total - (x1 - x0) * (y1 - y0)).abs() < 1e-12);

    let eps = 1e-7;
    let scene2 = rect_scene(10, 10, x0 + eps, y0, x1 + eps, y1, red());
    let mesh2 = RenderMesh::build(&scene2, budget(0.05)).unwrap();
    let cov2 = polygon_coverage(&[&mesh2.face_loops[1][0].points], 10, 0, 10);
    let max_delta = cov
        .iter()
        .zip(&cov2)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0f64, f64::max);
    assert!(max_delta <= 2.0 * eps + 1e-15, "max delta {max_delta}");
}

/// Subpixel phase sweep: total area is conserved for every phase (no
/// staircase bias anywhere in the accumulator).
#[test]
fn subpixel_phase_sweep_conserves_area() {
    for i in 0..16 {
        let phase = f64::from(i) / 16.0;
        let (x0, y0) = (2.0 + phase, 3.0 + phase * 0.5);
        let (x1, y1) = (x0 + 3.25, y0 + 2.75);
        let scene = rect_scene(12, 12, x0, y0, x1, y1, red());
        let mesh = RenderMesh::build(&scene, budget(0.05)).unwrap();
        let cov = polygon_coverage(&[&mesh.face_loops[1][0].points], 12, 0, 12);
        let total: f64 = cov.iter().sum();
        assert!(
            (total - 3.25 * 2.75).abs() < 1e-12,
            "phase {phase}: {total}"
        );
    }
}
