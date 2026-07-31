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
//! The residual magnitude dependence is not in the trapezoid but in the
//! POSITION fed to it: `accumulate_edge` computes the row-boundary
//! crossing as `x_at(y) = xlo + (y − ylo)·inv_dy` and stores it in
//! ABSOLUTE coordinates, so it is quantised to ulp(M).
//!
//! Precision of that claim (F-M2-R10; the earlier wording said this was
//! "a property of f64, not of this algorithm", which overstated it): the
//! red team measured the same column-relative rearrangement applied one
//! level up and found it differs by exactly `1.00 × ulp(M)/2` at every
//! magnitude — so the residual is a limit of the CHOSEN REPRESENTATION of
//! the intermediate position, not of f64 as such, and it is reducible
//! again by the same move. It is left in place deliberately: inside the
//! enforced domain the margin is ~50× (4.5e-12 measured against a
//! 2.33e-10 bound), while the change would alter the frozen render digest
//! for no in-domain benefit. Recorded as tracked item D-5 rather than
//! done silently; the bound that makes it safe is the typed
//! [`crate::domain::NumericDomain`] — see ADR-0013.
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
    #[error("loop {loop_index} point {point_index} is not finite: {point:?}")]
    NonFinitePoint {
        loop_index: usize,
        point_index: usize,
        point: Pt,
    },
    #[error(
        "edge {from:?} -> {to:?} cannot be integrated in f64: an intermediate position overflowed to {overflowed}"
    )]
    NonFiniteIntermediate { from: Pt, to: Pt, overflowed: f64 },
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
    let mut output = Vec::new();
    let mut workspace = CoverageWorkspace::default();
    polygon_coverage_into(
        loops,
        width_px,
        row_start,
        row_end,
        &mut output,
        &mut workspace,
    )?;
    Ok(output)
}

#[derive(Debug, Default)]
pub(crate) struct CoverageWorkspace {
    area: Vec<f64>,
    diff: Vec<f64>,
}

pub(crate) fn polygon_coverage_into(
    loops: &[&[Pt]],
    width_px: u32,
    row_start: u32,
    row_end: u32,
    output: &mut Vec<f64>,
    workspace: &mut CoverageWorkspace,
) -> Result<(), CoverageError> {
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
        // Finiteness is the THIRD precondition of this entry point, and it
        // belongs in exactly the same place as the other two (F-0012).
        // Before this it was enforced by nothing in release — a NaN vertex
        // produced a NaN buffer and `Ok` — and, after the column-local
        // trapezoid landed, by a `debug_assert` in `emit`, which turned the
        // same input into a panic under `overflow-checks`. One input, two
        // outcomes, on the public accumulator.
        //
        // It is checked BEFORE closure deliberately: `NaN != NaN`, so a
        // loop whose shared first/last vertex carries a NaN is bitwise
        // closed yet compares unequal, and checking closure first would
        // report `UnclosedLoop` — a true refusal naming the wrong cause.
        // Ordering the checks this way also makes the closure comparison
        // itself sound, since it then only ever sees finite operands.
        for (j, p) in lp.iter().enumerate() {
            if !p.is_finite() {
                return Err(CoverageError::NonFinitePoint {
                    loop_index: i,
                    point_index: j,
                    point: *p,
                });
            }
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
    workspace.area.clear();
    workspace.area.resize(n_rows * stride, 0.0);
    workspace.diff.clear();
    workspace.diff.resize(n_rows * stride, 0.0);

    for lp in loops {
        for e in lp.windows(2) {
            accumulate_edge(
                e[0],
                e[1],
                width_px,
                row_start,
                row_end,
                stride,
                &mut workspace.area,
                &mut workspace.diff,
            )?;
        }
    }

    // Fold: coverage(c) = area(c) + Σ_{c' > c} diff(c'), right-to-left.
    output.clear();
    output.resize(n_rows * w, 0.0);
    for r in 0..n_rows {
        let src = r * stride;
        let dst = r * w;
        let mut suffix = workspace.diff[src + w];
        for c in (0..w).rev() {
            output[dst + c] = workspace.area[src + c] + suffix;
            suffix += workspace.diff[src + c];
        }
    }
    Ok(())
}

/// `(num_hi - num_lo) / (den_hi - den_lo)`, computed so that a difference of
/// two FINITE operands overflowing cannot silently corrupt the result
/// (F-0013).
///
/// The class this closes. A difference of finite coordinates can overflow
/// to `±inf`, and the ratio then goes wrong in one of two directions:
///
/// ```text
/// numerator   overflows -> slope = ±inf -> position = ±inf -> typed refusal
/// denominator overflows -> slope =  0   -> position = const -> Ok, WRONG
/// ```
///
/// Only the first was guarded (C040), because only it produces a
/// non-finite value to notice. The second yields a perfectly finite,
/// in-range, entirely wrong answer: the edge is integrated as if it were
/// a straight line at the start coordinate. Generalising the earlier rule
/// one step: *finiteness of intermediates does not imply CORRECTNESS of
/// intermediates* — a guard that only tests `is_finite` cannot see this.
///
/// Fix: when either difference is not finite, recompute on HALVED
/// operands. Halving is exact for normal values, the mathematical ratio
/// is unchanged, and neither difference can overflow because each operand
/// is then at most `MAX/2`. The fast path is taken whenever the plain
/// differences are finite, so every input that did not overflow keeps its
/// previous bit pattern — frozen digests are untouched by construction.
fn stable_ratio_of_differences(num_lo: f64, num_hi: f64, den_lo: f64, den_hi: f64) -> f64 {
    let num = num_hi - num_lo;
    let den = den_hi - den_lo;
    if num.is_finite() && den.is_finite() {
        return num / den;
    }
    (num_hi * 0.5 - num_lo * 0.5) / (den_hi * 0.5 - den_lo * 0.5)
}

/// Interpolate a position between the two endpoints of an edge (F-0014).
///
/// The CONVEX form `a(1-t) + b t`, not the incremental form
/// `a + t(b - a)` — and that is the whole point. The incremental form
/// cancels catastrophically when the endpoints straddle the exponent
/// range: for an edge from `(1e150, -1e300)` to `(10.02, 0.80)` it
/// computes `1e150 + 1e300·(-1e-150)`, two nearly equal opposite terms,
/// and returns 0 where the true crossing is at 10.02 — a silently wrong
/// answer of 28% of a pixel. The convex form gets it exactly right there,
/// because `1 - t` rounds to 0 and the result is simply `b`.
///
/// This is D-5, and it turned out to be D-6 as well: the class I had
/// documented as "no implementation can be right here" was in fact the
/// conditioning of THIS formulation. Four independent methods — closed
/// form, the clipping reference, supersampling and this very accumulator
/// at benign magnitudes — agree on the answer the incremental form lost.
///
/// The exact cases are taken exactly so that no accuracy is given up
/// where the old form had it: endpoints reproduce `a` and `b` bitwise
/// (better than the incremental form, which only guaranteed `a`), and a
/// degenerate span returns `a`.
fn interpolate_position(a: f64, b: f64, t: f64) -> f64 {
    if a == b || t <= 0.0 {
        return a;
    }
    if t >= 1.0 {
        return b;
    }
    // Interpolate from the NEARER endpoint, so the increment is at most
    // half the span and can never dominate the base it is added to. This
    // keeps the accuracy of the incremental form for ordinary geometry —
    // for `t <= 0.5` it IS the previous expression, bit for bit — while
    // removing its catastrophic case, where the far endpoint's magnitude
    // swamped the near one. The pure convex combination is kept as the
    // fallback for a span so wide that even the mirrored difference
    // overflows.
    let (base, other, step) = if t <= 0.5 { (a, b, t) } else { (b, a, 1.0 - t) };
    let diff = other - base;
    if diff.is_finite() {
        base + step * diff
    } else {
        a * (1.0 - t) + b * t
    }
}

/// FINITENESS OF INPUTS DOES NOT IMPLY FINITENESS OF INTERMEDIATES
/// (F-0012). `xhi - xlo` overflows to ±inf for coordinates near `f64::MAX`,
/// and `0 * inf` then yields a NaN POSITION from two perfectly finite
/// vertices. This is the same class as the NaN arc centre found in F-0007
/// — and the lesson from there was not carried here, which is precisely
/// the generalisation failure the red team flagged. An intermediate that
/// leaves the finite range is REFUSED, never propagated: silently skipping
/// the edge would lose real area instead.
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
) -> Result<(), CoverageError> {
    if a.y == b.y {
        return Ok(()); // horizontal edges: dy = 0
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
        return Ok(());
    }
    // Position along the edge as a CONVEX combination of its endpoints
    // (F-0014). The parameter itself uses the overflow-safe ratio, so
    // both halves of the interpolation are conditioned.
    let x_at = |y: f64| {
        let t = stable_ratio_of_differences(ylo, y, ylo, yhi);
        interpolate_position(xlo, xhi, t)
    };

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
        // Everything downstream is finite by construction once these two
        // are: column boundaries are exact integers, and the y-clamps
        // absorb a non-finite slope.
        if !sx.is_finite() || !ex.is_finite() {
            return Err(CoverageError::NonFiniteIntermediate {
                from: a,
                to: b,
                overflowed: if sx.is_finite() { ex } else { sx },
            });
        }
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
    Ok(())
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

    // The SAME shape as `inv_dy` above, and the same failure: `ex - sx`
    // overflows for a run spanning the exponent range, `dy_dx` collapses
    // to 0, every column crossing lands at `sy`, and the whole row's
    // contribution piles into one column. Found by auditing the class
    // rather than the reported line (F-0013).
    // dy/dx: y is the numerator here, x the denominator — the mirror of
    // `inv_dy` above, and the in-tree slope-conservation test catches the
    // pair being swapped.
    let dy_dx = stable_ratio_of_differences(sy, ey, sx, ex);
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
mod tests;
