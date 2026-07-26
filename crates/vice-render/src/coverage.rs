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
//! contributes the exact trapezoid `dy · (((x0 − c) + (x1 − c))/2)` to its
//! own column and the full `dy` to every column strictly left of it
//! (folded with a per-row difference array). All quantities are f64
//! arithmetic on linear interpolation — exact area coverage for the fixed
//! polyline tessellation up to f64 rounding.
//!
//! Why the trapezoid is written column-locally (F-0008, REDTEAM_M2
//! F-M2-R2): the differences `x0 − c`, `x1 − c` are EXACT (Sterbenz), so
//! the integration error per piece is a few ulp(1) ABSOLUTE, independent
//! of where the geometry sits in coordinate space. Written the obvious way
//! — `(x0 + x1)/2 − c` — the sum is rounded at magnitude ~2c first, which
//! injects ~c·eps per piece: at c ≈ 2^24 that is ~2e-9 per pixel, above
//! the tolerance the partition checker enforces, and INVISIBLE to it
//! (the error is common-mode: adjacent faces traverse the same shared
//! polyline with opposite `dy` and it cancels bit-for-bit in the sum).
//!
//! The residual magnitude dependence is NOT in the integration but in the
//! geometry itself: an edge position interpolated at coordinate magnitude
//! M is only representable to ulp(M). That is a property of f64, not of
//! this algorithm, and it is what the typed [`crate::domain::NumericDomain`]
//! bounds — see ADR-0013.
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

/// Typed rejection of a coverage request (F-0011).
///
/// The preconditions of this public entry point used to be a
/// `debug_assert` (silent garbage in release: an unclosed square returned
/// 24 instead of 16) and an `assert!` (panic in both profiles on an
/// inverted band). Spec §5.4 wants typed refusals, and a public API's
/// preconditions belong in a type or a typed error — never in a
/// debug-only check.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CoverageError {
    #[error("loop {loop_index} is not closed: first {first:?} != last {last:?}")]
    UnclosedLoop {
        loop_index: usize,
        first: Pt,
        last: Pt,
    },
    #[error("loop {loop_index} has {points} points; a closed loop needs at least 2")]
    DegenerateLoop { loop_index: usize, points: usize },
    #[error("inverted row band: row_start {row_start} > row_end {row_end}")]
    InvertedRowBand { row_start: u32, row_end: u32 },
}

/// Exact per-pixel winding integral of closed `loops` over the pixel rows
/// `[row_start, row_end)` of a canvas `width_px` wide.
///
/// Returns a row-major buffer of `(row_end - row_start) * width_px`
/// values. Every loop must be closed (`first == last`) — checked, with a
/// typed error otherwise; horizontal edges contribute nothing and
/// zero-length edges are skipped.
pub fn polygon_coverage(
    loops: &[&[Pt]],
    width_px: u32,
    row_start: u32,
    row_end: u32,
) -> Result<Vec<f64>, CoverageError> {
    if row_end < row_start {
        return Err(CoverageError::InvertedRowBand { row_start, row_end });
    }
    for (i, lp) in loops.iter().enumerate() {
        if lp.len() < 2 {
            return Err(CoverageError::DegenerateLoop {
                loop_index: i,
                points: lp.len(),
            });
        }
        if lp[0] != lp[lp.len() - 1] {
            return Err(CoverageError::UnclosedLoop {
                loop_index: i,
                first: lp[0],
                last: lp[lp.len() - 1],
            });
        }
    }
    let w = width_px as usize;
    let n_rows = (row_end - row_start) as usize;
    let stride = w + 1; // extra right-edge bucket per row
    let mut area = vec![0.0f64; n_rows * stride];
    let mut diff = vec![0.0f64; n_rows * stride];

    for lp in loops {
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
    Ok(out)
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
            let cf = c as f64;
            // COLUMN-LOCAL trapezoid (F-0008). Both endpoints of a cell
            // piece lie in `[c, c+1]` by construction of the split below,
            // so `x0 - cf` and `x1 - cf` are EXACT (c = 0 trivially;
            // c >= 1 by Sterbenz, since cf <= x0 <= c+1 <= 2c = 2cf), and
            // both lie in [0, 1]. Their sum is therefore rounded at
            // magnitude <= 2 and the halving is exact, so the integration
            // error per piece is a few ulp(1) ABSOLUTE and independent of
            // the coordinate magnitude. The previous form `(x0 + x1)/2 - cf`
            // rounded the sum at magnitude ~2c FIRST and lost ~c*eps.
            debug_assert!(
                (cf..=cf + 1.0).contains(&x0) && (cf..=cf + 1.0).contains(&x1),
                "cell piece must lie inside its own column"
            );
            area[ci] += dy * (((x0 - cf) + (x1 - cf)) / 2.0);
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
    let w = f64::from(width_px);
    let width_i = i64::from(width_px);
    let bucket = width_px as usize;

    // COST (F-0009): the walk below is clamped to the canvas window, so it
    // performs at most `width_px` steps per row piece no matter how far the
    // geometry extends. Both tails are exact in O(1):
    //   - columns < 0 lie entirely to the LEFT of every canvas pixel, and
    //     the winding ray looks RIGHT, so they contribute nothing at all;
    //   - columns >= width lie to the RIGHT of every canvas pixel, so each
    //     contributes its full `dy` to the same full-cover bucket; their
    //     sum telescopes to one subtraction of the entry/exit y values.
    // Previously the loop stepped through every column between the true
    // endpoints, making render time Θ(coordinate magnitude): 5.4 s for one
    // triangle on a 16x16 canvas at x = 1e9.
    let mut cur_y = sy;
    let mut cur_x = sx;
    if ex > sx {
        // Ascending in x; y grows with x along the piece.
        if ex <= 0.0 {
            return; // entirely left of the canvas: no contribution
        }
        if cur_x < 0.0 {
            // Skip the zero-contribution left tail in one step.
            cur_y = y_of_x(0.0).min(ey).max(cur_y);
            cur_x = 0.0;
        }
        if cur_x >= w {
            // Entirely right of the canvas: one bucket add.
            diff[bucket] += sign * (ey - cur_y);
            return;
        }
        let c_end = ex.floor() as i64;
        let mut c = cur_x.floor() as i64;
        while c < c_end && c < width_i {
            let bnd = (c + 1) as f64;
            let ycross = y_of_x(bnd).min(ey).max(cur_y);
            emit(c, sign * (ycross - cur_y), cur_x, bnd, area, diff);
            cur_y = ycross;
            cur_x = bnd;
            c += 1;
        }
        if c >= width_i {
            // The remaining run is right of the canvas: one bucket add.
            diff[bucket] += sign * (ey - cur_y);
        } else {
            emit(c, sign * (ey - cur_y), cur_x, ex, area, diff);
        }
    } else {
        // Descending in x; y still grows along the piece.
        if sx <= 0.0 {
            return; // entirely left of the canvas: no contribution
        }
        if ex >= w {
            // Entirely right of the canvas (ex < sx, so both are >= w):
            // full cover for every canvas cell, one bucket add. Without
            // this the tail skip below would leave `cur_x = w` and then
            // emit a final piece at column w-1 with `x1 = ex > w`, i.e.
            // outside its own column — the invariant the column-local
            // trapezoid proof rests on (caught by its debug_assert).
            diff[bucket] += sign * (ey - sy);
            return;
        }
        if cur_x > w {
            // Right tail first (x descends into the canvas): one bucket add.
            let ycross = y_of_x(w).min(ey).max(cur_y);
            diff[bucket] += sign * (ycross - cur_y);
            cur_y = ycross;
            cur_x = w;
        }
        let c_end = ex.floor() as i64;
        // `cur_x == w` belongs to column width-1 when walking left.
        let mut c = (cur_x.floor() as i64).min(width_i - 1);
        while c > c_end && c >= 0 {
            let bnd = c as f64;
            let ycross = y_of_x(bnd).min(ey).max(cur_y);
            emit(c, sign * (ycross - cur_y), cur_x, bnd, area, diff);
            cur_y = ycross;
            cur_x = bnd;
            c -= 1;
        }
        if c >= 0 {
            emit(c, sign * (ey - cur_y), cur_x, ex, area, diff);
        }
        // c < 0: the remaining run is left of the canvas — nothing to add.
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
}
