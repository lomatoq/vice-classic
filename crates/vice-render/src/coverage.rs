//! Exact signed-area polygon coverage (spec v1.3 §16.1).
//!
//! For a set of closed polygon loops, [`polygon_coverage`] computes, for
//! every pixel of a row band, the EXACT value of `∫∫_pixel w(x, y) dA`
//! where `w` is the winding number of the loops (algebraic convention:
//! a positive-signed-area loop winds `+1` inside). For a face whose loops
//! satisfy the face-on-the-algebraic-left convention this integral IS the
//! area fraction of the pixel covered by the face — and for an invalid
//! embedding it visibly leaves `[0, 1]`, which the partition checker
//! exploits (ADR-0010).
//!
//! Derivation (clean-room, from the definition): the winding number at a
//! point equals the signed count of edge crossings strictly to the RIGHT
//! of the point along the horizontal ray, so
//! `∫∫_pixel w dA = Σ_edges sign(dy) · ∫_rows clamp(x_edge(y) − c, 0, 1) dy`
//! for pixel column `c`. Every edge is decomposed into row pieces and then
//! into cell pieces where `x(y)` stays within one column; a cell piece
//! contributes the exact trapezoid `dy · ((x0 + x1)/2 − c)` to its own
//! column and the full `dy` to every column strictly left of it (folded
//! with a per-row difference array). All quantities are f64 arithmetic on
//! linear interpolation — exact area coverage for the fixed polyline
//! tessellation up to f64 rounding.
//!
//! Determinism (§5.5): loops, edges, rows and cells are processed in a
//! fixed order; the per-row fold runs right-to-left. No hash maps, no
//! threads. Byte-identical output for identical input on one platform.
//!
//! Canvas clip policy (ADR-0009, REVIEW_M1 M1-N6): geometry outside the
//! canvas is CLIPPED, not rejected — coverage is integrated over canvas
//! pixels only. Rows outside the requested band are skipped exactly
//! (per-row contributions are independent), columns beyond the right edge
//! fold into a full-cover bucket, and geometry left of column 0 or fully
//! off-canvas contributes nothing to canvas pixels. This is mathematically
//! "integrate the true winding over the canvas window".

use vice_geom::Pt;

/// Exact per-pixel winding integral of closed `loops` over the pixel rows
/// `[row_start, row_end)` of a canvas `width_px` wide.
///
/// Returns a row-major buffer of `(row_end - row_start) * width_px`
/// values. Every loop must be closed (`first == last`); horizontal edges
/// contribute nothing and zero-length edges are skipped.
pub fn polygon_coverage(loops: &[&[Pt]], width_px: u32, row_start: u32, row_end: u32) -> Vec<f64> {
    assert!(row_end >= row_start, "inverted row band");
    let w = width_px as usize;
    let n_rows = (row_end - row_start) as usize;
    let stride = w + 1; // extra right-edge bucket per row
    let mut area = vec![0.0f64; n_rows * stride];
    let mut diff = vec![0.0f64; n_rows * stride];

    for lp in loops {
        debug_assert!(
            lp.len() >= 2 && lp.first() == lp.last(),
            "loops must be closed"
        );
        for e in lp.windows(2) {
            accumulate_edge(
                e[0], e[1], width_px, row_start, row_end, stride, &mut area, &mut diff,
            );
        }
    }

    // Fold: coverage(c) = area(c) + Σ_{c' > c} diff(c'), right-to-left.
    let mut out = vec![0.0f64; n_rows * w];
    for r in 0..n_rows {
        let src = r * stride;
        let dst = r * w;
        let mut suffix = diff[src + w];
        for c in (0..w).rev() {
            out[dst + c] = area[src + c] + suffix;
            suffix += diff[src + c];
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn accumulate_edge(
    a: Pt,
    b: Pt,
    width_px: u32,
    row_start: u32,
    row_end: u32,
    stride: usize,
    area: &mut [f64],
    diff: &mut [f64],
) {
    if a.y == b.y {
        return; // horizontal edges: dy = 0
    }
    let sign = if b.y > a.y { 1.0 } else { -1.0 };
    let (ylo, xlo, yhi, xhi) = if b.y > a.y {
        (a.y, a.x, b.y, b.x)
    } else {
        (b.y, b.x, a.y, a.x)
    };

    // Clip the y-span to the requested row band (canvas clip policy:
    // per-row contributions are independent, so dropping rows is exact).
    let band_lo = f64::from(row_start);
    let band_hi = f64::from(row_end);
    let y_from = ylo.max(band_lo);
    let y_to = yhi.min(band_hi);
    if y_to <= y_from {
        return;
    }
    let inv_dy = (xhi - xlo) / (yhi - ylo);
    let x_at = |y: f64| xlo + (y - ylo) * inv_dy;

    let r0 = y_from.floor().max(band_lo) as u32;
    // Last row index touched (y_to on a row boundary belongs to the row
    // below it, which is empty — the per-row emptiness check handles it).
    let r1 = (y_to.ceil().min(band_hi) as u32).saturating_sub(1).max(r0);

    for r in r0..=r1.min(row_end.saturating_sub(1)) {
        let row_off = (r - row_start) as usize * stride;
        let sy = y_from.max(f64::from(r));
        let ey = y_to.min(f64::from(r) + 1.0);
        if ey <= sy {
            continue;
        }
        // Exact endpoints where unclipped; interpolated where clipped.
        let sx = if sy == ylo { xlo } else { x_at(sy) };
        let ex = if ey == yhi { xhi } else { x_at(ey) };
        emit_row_pieces(
            sign,
            sy,
            ey,
            sx,
            ex,
            width_px,
            &mut area[row_off..row_off + stride],
            &mut diff[row_off..row_off + stride],
        );
    }
}

/// Split one row sub-segment (x from `sx` to `ex` over y in `[sy, ey]`)
/// into cell pieces and accumulate them.
#[allow(clippy::too_many_arguments)]
fn emit_row_pieces(
    sign: f64,
    sy: f64,
    ey: f64,
    sx: f64,
    ex: f64,
    width_px: u32,
    area: &mut [f64],
    diff: &mut [f64],
) {
    let emit = |c: i64, dy: f64, x0: f64, x1: f64, area: &mut [f64], diff: &mut [f64]| {
        let w = width_px as i64;
        if c >= w {
            // Fully right of the canvas: full cover for every canvas cell.
            diff[width_px as usize] += dy;
        } else if c >= 0 {
            let ci = c as usize;
            area[ci] += dy * ((x0 + x1) / 2.0 - c as f64);
            diff[ci] += dy;
        }
        // c < 0: contributes nothing to canvas cells (they are to the
        // RIGHT of the piece; the ray-to-the-right sees no crossing).
    };

    if sx == ex {
        emit(sx.floor() as i64, sign * (ey - sy), sx, sx, area, diff);
        return;
    }

    let dy_dx = (ey - sy) / (ex - sx);
    let y_of_x = |x: f64| sy + (x - sx) * dy_dx;

    let mut c = sx.floor() as i64;
    let c_end = ex.floor() as i64;
    let mut cur_y = sy;
    let mut cur_x = sx;
    if ex > sx {
        while c < c_end {
            let bnd = (c + 1) as f64;
            let ycross = y_of_x(bnd).min(ey).max(cur_y);
            emit(c, sign * (ycross - cur_y), cur_x, bnd, area, diff);
            cur_y = ycross;
            cur_x = bnd;
            c += 1;
        }
        emit(c, sign * (ey - cur_y), cur_x, ex, area, diff);
    } else {
        while c > c_end {
            let bnd = c as f64;
            let ycross = y_of_x(bnd).min(ey).max(cur_y);
            emit(c, sign * (ycross - cur_y), cur_x, bnd, area, diff);
            cur_y = ycross;
            cur_x = bnd;
            c -= 1;
        }
        emit(c, sign * (ey - cur_y), cur_x, ex, area, diff);
    }
}

#[cfg(test)]
mod tests {
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
        polygon_coverage(loops, w, 0, h)
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
        let band = polygon_coverage(&[&lp1[..], &lp2[..]], 8, 2, 5);
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
}
