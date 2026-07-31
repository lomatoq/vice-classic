use super::*;

fn rect_loop(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Pt> {
    // Positive algebraic signed area.
    vec![
        Pt::new(x0, y0),
        Pt::new(x1, y0),
        Pt::new(x1, y1),
        Pt::new(x0, y1),
        Pt::new(x0, y0),
    ]
}

fn cov(loops: &[&[Pt]], w: u32, h: u32) -> Vec<f64> {
    polygon_coverage(loops, w, 0, h).expect("closed loops")
}

#[test]
fn unit_square_covers_its_pixel_exactly() {
    let lp = rect_loop(1.0, 1.0, 2.0, 2.0);
    let c = cov(&[&lp], 4, 4);
    for (i, v) in c.iter().enumerate() {
        let expected = if i == 4 + 1 { 1.0 } else { 0.0 };
        assert_eq!(*v, expected, "pixel {i}");
    }
}

#[test]
fn half_pixel_offsets_are_exact() {
    // [0.5, 2.5] × [0.5, 2.5]: corners 0.25, edges 0.5, center 1 — all
    // EXACT in f64, asserted with equality, not tolerance.
    let lp = rect_loop(0.5, 0.5, 2.5, 2.5);
    let c = cov(&[&lp], 3, 3);
    let expected = [
        0.25, 0.5, 0.25, //
        0.5, 1.0, 0.5, //
        0.25, 0.5, 0.25,
    ];
    assert_eq!(c, expected);
}

#[test]
fn quarter_pixel_offsets_are_exact() {
    let lp = rect_loop(0.25, 0.75, 1.25, 1.75);
    let c = cov(&[&lp], 3, 3);
    let expected = [
        0.75 * 0.25,
        0.25 * 0.25,
        0.0, //
        0.75 * 0.75,
        0.25 * 0.75,
        0.0, //
        0.0,
        0.0,
        0.0,
    ];
    assert_eq!(c, expected);
}

#[test]
fn reversed_loop_negates_coverage() {
    let mut lp = rect_loop(0.5, 0.5, 2.5, 2.5);
    lp.reverse();
    let c = cov(&[&lp], 3, 3);
    assert_eq!(c[4], -1.0);
    assert_eq!(c[0], -0.25);
}

#[test]
fn triangle_covers_half_its_bounding_pixel() {
    // Right triangle over one pixel: hypotenuse from (1,1) to (2,2).
    let lp = vec![
        Pt::new(1.0, 1.0),
        Pt::new(2.0, 1.0),
        Pt::new(2.0, 2.0),
        Pt::new(1.0, 1.0),
    ];
    let c = cov(&[&lp], 4, 4);
    assert!((c[4 + 1] - 0.5).abs() < 1e-15);
    let total: f64 = c.iter().sum();
    assert!((total - 0.5).abs() < 1e-15);
}

#[test]
fn total_coverage_equals_polygon_area_at_arbitrary_offsets() {
    let (x0, y0, x1, y1) = (0.3, 1.7, 5.9, 6.2);
    let lp = rect_loop(x0, y0, x1, y1);
    let c = cov(&[&lp], 8, 8);
    let total: f64 = c.iter().sum();
    let exact = (x1 - x0) * (y1 - y0);
    assert!((total - exact).abs() < 1e-12, "{total} vs {exact}");
    // Every pixel value is a valid area fraction.
    for v in &c {
        assert!(*v >= -1e-15 && *v <= 1.0 + 1e-15);
    }
}

#[test]
fn l_shape_nonconvex_total_area_is_exact() {
    // L-shape: 3x3 square minus 2x2 top-right notch = 9 - 4 = 5.
    let lp = vec![
        Pt::new(1.0, 1.0),
        Pt::new(2.0, 1.0),
        Pt::new(2.0, 3.0),
        Pt::new(4.0, 3.0),
        Pt::new(4.0, 4.0),
        Pt::new(1.0, 4.0),
        Pt::new(1.0, 1.0),
    ];
    let c = cov(&[&lp], 6, 6);
    let total: f64 = c.iter().sum();
    assert!((total - 5.0).abs() < 1e-12);
    // The notch pixel (3,1) is empty; the corner pixel (1,3) is full.
    assert_eq!(c[6 + 3], 0.0);
    assert_eq!(c[3 * 6 + 1], 1.0);
}

#[test]
fn outer_and_hole_loop_compose_a_ring() {
    let outer = rect_loop(1.0, 1.0, 6.0, 6.0);
    let mut hole = rect_loop(3.0, 3.0, 4.0, 4.0);
    hole.reverse(); // negative orientation: a hole
    let c = cov(&[&outer, &hole], 8, 8);
    assert_eq!(c[3 * 8 + 3], 0.0, "hole pixel empty");
    assert_eq!(c[2 * 8 + 2], 1.0, "ring pixel full");
    let total: f64 = c.iter().sum();
    assert!((total - 24.0).abs() < 1e-12);
}

#[test]
fn steep_and_shallow_slopes_conserve_area() {
    // Thin sliver triangle crossing many cells.
    let lp = vec![
        Pt::new(0.2, 0.1),
        Pt::new(7.8, 6.9),
        Pt::new(0.4, 0.1),
        Pt::new(0.2, 0.1),
    ];
    let c = cov(&[&lp], 8, 8);
    let shoelace =
        0.5 * ((0.2 * 6.9 - 7.8 * 0.1) + (7.8 * 0.1 - 0.4 * 6.9) + (0.4 * 0.1 - 0.2 * 0.1));
    let total: f64 = c.iter().sum();
    assert!((total - shoelace).abs() < 1e-12, "{total} vs {shoelace}");
}

// --- canvas clip policy (ADR-0009, M1-N6) ---------------------------

#[test]
fn fully_off_canvas_geometry_contributes_nothing() {
    for lp in [
        rect_loop(100.0, 1.0, 105.0, 3.0), // right of canvas
        rect_loop(-9.0, 1.0, -2.0, 3.0),   // left of canvas
        rect_loop(1.0, -8.0, 3.0, -2.0),   // above
        rect_loop(1.0, 50.0, 3.0, 55.0),   // below
    ] {
        let c = cov(&[&lp], 8, 8);
        assert!(c.iter().all(|v| *v == 0.0), "clipped to nothing");
    }
}

#[test]
fn straddling_geometry_is_clipped_to_the_canvas_window() {
    // Rect [5, 12] × [-2, 3] on an 8×8 canvas: canvas-side part is
    // [5, 8] × [0, 3] = 9 px².
    let lp = rect_loop(5.0, -2.0, 12.0, 3.0);
    let c = cov(&[&lp], 8, 8);
    let total: f64 = c.iter().sum();
    assert!((total - 9.0).abs() < 1e-12);
    assert_eq!(c[6], 1.0, "pixel (6,0) fully covered");
    assert_eq!(c[7], 1.0, "pixel (7,0) fully covered");
    assert_eq!(c[4], 0.0, "pixel (4,0) untouched");
}

#[test]
fn row_band_equals_full_render_rows_bitwise() {
    // The dependency closure of a row band is exact: recomputing only
    // rows 2..5 yields bit-identical values to the full render.
    let lp1 = rect_loop(0.3, 0.7, 6.55, 7.2);
    let lp2 = vec![
        Pt::new(2.1, 1.2),
        Pt::new(7.9, 3.4),
        Pt::new(4.0, 6.6),
        Pt::new(2.1, 1.2),
    ];
    let full = cov(&[&lp1, &lp2], 8, 8);
    let band = polygon_coverage(&[&lp1[..], &lp2[..]], 8, 2, 5).expect("closed loops");
    assert_eq!(band.len(), 3 * 8);
    assert_eq!(&full[2 * 8..5 * 8], &band[..]);
}

/// Dyadic coordinates (n/16): the integer shift and every downstream
/// trapezoid term stay exactly representable, so shifted coverage is
/// BITWISE equal to the original.
#[test]
fn integer_translation_shifts_dyadic_coverage_bitwise() {
    let lp = rect_loop(0.3125, 0.6875, 3.5625, 3.1875);
    let shifted: Vec<Pt> = lp.iter().map(|p| Pt::new(p.x + 3.0, p.y + 2.0)).collect();
    let a = cov(&[&lp], 12, 12);
    let b = cov(&[&shifted], 12, 12);
    for y in 0..10 {
        for x in 0..9 {
            assert_eq!(a[y * 12 + x], b[(y + 2) * 12 + (x + 3)]);
        }
    }
}

/// The OTHER half of the dyadic class, and the reason the claim above
/// is stated for axis-aligned edges only (F-0006 amendment, REDTEAM_M2
/// F-M2-R4).
///
/// A SLOPED dyadic edge is not bitwise translation-invariant: the row
/// walk evaluates `y_of_x(x) = sy + (x - sx) * dy_dx`, and that final
/// addition re-rounds when `sy` moves by an integer, even though every
/// operand is dyadic — a sum can need more mantissa bits than either
/// addend. The red team's counterexample (18 pixels differing at
/// 5.0e-16) is reproduced here, and the honest claim for this class is
/// a typed bound, not bit equality.
#[test]
fn dyadic_sloped_edges_are_translation_invariant_only_within_a_typed_bound() {
    let tri = vec![
        Pt::new(1.25, 1.125),
        Pt::new(9.75, 4.375),
        Pt::new(3.5, 8.625),
        Pt::new(1.25, 1.125),
    ];
    let shifted: Vec<Pt> = tri.iter().map(|p| Pt::new(p.x + 3.0, p.y + 2.0)).collect();
    let a = cov(&[&tri], 16, 16);
    let b = cov(&[&shifted], 16, 16);
    let mut differing = 0usize;
    let mut worst = 0.0f64;
    for y in 0..12 {
        for x in 0..12 {
            let (u, v) = (a[y * 16 + x], b[(y + 2) * 16 + (x + 3)]);
            if u.to_bits() != v.to_bits() {
                differing += 1;
            }
            worst = worst.max((u - v).abs());
        }
    }
    // The class genuinely is NOT bitwise invariant: if this ever
    // becomes 0 the claim above may be widened, but only then.
    assert!(
        differing > 0,
        "the counterexample must stay a counterexample"
    );
    // ...and the honest guarantee for it is the typed bound.
    assert!(
        worst <= 1e-12,
        "sloped dyadic shift worst |delta| {worst:e}"
    );
}

/// Non-dyadic coordinates: integer shifts change the ROUNDING of the
/// per-cell trapezoid terms, so equality is up to a tiny typed bound
/// (few ulps), not bitwise — documented, not hidden.
#[test]
fn integer_translation_shifts_general_coverage_within_float_noise() {
    let lp = rect_loop(0.3, 0.7, 3.55, 3.2);
    let shifted: Vec<Pt> = lp.iter().map(|p| Pt::new(p.x + 3.0, p.y + 2.0)).collect();
    let a = cov(&[&lp], 12, 12);
    let b = cov(&[&shifted], 12, 12);
    for y in 0..10 {
        for x in 0..9 {
            let (u, v) = (a[y * 12 + x], b[(y + 2) * 12 + (x + 3)]);
            assert!((u - v).abs() <= 1e-12, "({x},{y}): {u} vs {v}");
        }
    }
}

#[test]
fn coverage_is_continuous_in_translation() {
    let eps = 1e-6;
    let base = rect_loop(1.3, 1.7, 5.55, 5.2);
    let moved: Vec<Pt> = base.iter().map(|p| Pt::new(p.x + eps, p.y)).collect();
    let a = cov(&[&base], 8, 8);
    let b = cov(&[&moved], 8, 8);
    let max_delta = a
        .iter()
        .zip(&b)
        .map(|(x, y)| (x - y).abs())
        .fold(0.0f64, f64::max);
    // An x-shift of eps changes any pixel's covered area by at most
    // eps (row height 1) per crossing edge; 2 edges here.
    assert!(max_delta <= 2.0 * eps + 1e-15, "max delta {max_delta}");
}
