//! Cost gate for the coverage accumulator (F-0009; REDTEAM_M2 F-M2-R3,
//! REVIEW_M2_A M2-A-N2, REVIEW_M2_B M2-B-N2).
//!
//! Spec §36 names "runtime has uncontrolled growth" as a stop condition
//! and §29 forbids unbounded growth. The accumulator used to step through
//! every column between an edge's endpoints, including columns far outside
//! the canvas, so render time was Θ(coordinate magnitude) rather than
//! Θ(W×H): a single triangle on a 16×16 canvas took 5.4 s at x = 1e9,
//! with perfect linearity (×10 coordinate → ×10 time) and years at legal
//! f64 coordinates.
//!
//! These tests are deliberately wall-clock based with an enormous margin
//! (5+ orders): pre-fix the 1e10 case needs hours, post-fix it is
//! microseconds, so no plausible machine variance can flip the verdict.
//! Correctness of the clamped tails is judged separately against the
//! independent Sutherland–Hodgman reference, not against timing.

mod common;

use std::time::Instant;

use common::sh_clip_coverage;
use vice_geom::Pt;
use vice_render::polygon_coverage;

/// A sloped triangle whose far vertex sits at `far` — the shape class that
/// exercises the column walk (an axis-aligned edge would take the vertical
/// fast path and never walk at all, which is why a rectangle fixture could
/// not have caught this).
fn far_triangle(far: f64) -> Vec<Pt> {
    vec![
        Pt::new(2.0, 1.5),
        Pt::new(far, 7.5),
        Pt::new(3.0, 14.5),
        Pt::new(2.0, 1.5),
    ]
}

#[test]
fn cost_is_bounded_by_the_canvas_not_by_coordinate_magnitude() {
    let (w, h) = (16u32, 16u32);
    for far in [1.0e4, 1.0e6, 1.0e8, 1.0e10] {
        let lp = far_triangle(far);
        let t = Instant::now();
        let cov = polygon_coverage(&[&lp], w, 0, h);
        let elapsed = t.elapsed();
        println!("far = {far:e}: {:?}", elapsed);
        assert!(
            elapsed.as_secs_f64() < 1.0,
            "far = {far:e} took {elapsed:?}; cost must not scale with coordinate magnitude"
        );
        assert!(cov.iter().all(|v| v.is_finite()));
    }
}

/// The clamped tails must be mathematically identical to walking them:
/// judged against the independent per-pixel clipping reference, which has
/// no notion of columns, tails or scanlines at all.
#[test]
fn clamped_tails_agree_with_the_independent_clipping_reference() {
    let (w, h) = (16u32, 16u32);
    for far in [20.0, 100.0, 1.0e4, 1.0e8] {
        let lp = far_triangle(far);
        let ours = polygon_coverage(&[&lp], w, 0, h);
        let reference = sh_clip_coverage(&[&lp], w, h);
        let worst = ours
            .iter()
            .zip(&reference)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f64, f64::max);
        println!("far = {far:e}: worst |ours - SH| = {worst:e}");
        // The reference itself loses precision at extreme magnitudes, so
        // the bound is scaled by the magnitude it has to represent.
        let allowed = 1e-12 * far.max(1.0) / 100.0;
        assert!(
            worst <= allowed.max(1e-12),
            "far = {far:e}: {worst:e} exceeds {:e}",
            allowed.max(1e-12)
        );
    }
}

/// Geometry reaching out on BOTH sides, and the left tail specifically:
/// columns left of 0 contribute nothing (the winding ray looks right), and
/// dropping them in O(1) must not change a single value.
#[test]
fn left_and_right_tails_are_both_exact() {
    let (w, h) = (12u32, 12u32);
    let spanning = vec![
        Pt::new(-1.0e9, 2.25),
        Pt::new(1.0e9, 4.75),
        Pt::new(1.0e9, 9.5),
        Pt::new(-1.0e9, 6.5),
        Pt::new(-1.0e9, 2.25),
    ];
    let t = Instant::now();
    let ours = polygon_coverage(&[&spanning], w, 0, h);
    assert!(t.elapsed().as_secs_f64() < 1.0, "spanning geometry is fast");
    let reference = sh_clip_coverage(&[&spanning], w, h);
    let worst = ours
        .iter()
        .zip(&reference)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0f64, f64::max);
    println!("both-tail worst |ours - SH| = {worst:e}");
    assert!(worst < 1e-5, "worst {worst:e}");
    // Rows fully inside the horizontal span must be fully covered.
    for c in 0..w as usize {
        assert!((ours[7 * w as usize + c] - 1.0).abs() < 1e-9);
    }
}

/// Cost must also be bounded on the full render path, where the numeric
/// domain caps coordinates at 2^16: even at the domain edge the walk is
/// capped by the canvas width, not by the coordinate.
#[test]
fn in_domain_extreme_geometry_renders_promptly() {
    let lp = far_triangle(65000.0);
    let t = Instant::now();
    let cov = polygon_coverage(&[&lp], 16, 0, 16);
    assert!(t.elapsed().as_secs_f64() < 1.0);
    assert!(cov.iter().all(|v| (-1e-9..=1.0 + 1e-9).contains(v)));
}
