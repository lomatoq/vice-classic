//! Certified curve flattening: curve → polyline with a CERTIFIED chord-error
//! bound (spec v1.3 §16.1: "exact" coverage is exact for a FIXED polyline
//! tessellation; the curve→polyline step carries its own certified budget).
//!
//! Every flattener returns a [`FlattenedCurve`]: the polyline points (first
//! and last are the EXACT input endpoints — shared chain parameters are
//! never re-derived, so shared boundaries tessellate crack-free) plus
//! `max_deviation_px`, a certified upper bound on the Hausdorff distance
//! between the true curve and the returned polyline.
//!
//! Certification basis (derivations in docs/ADR/ADR-0008):
//! - Béziers: the chord interpolation bound. For a C² curve `B` on a
//!   parameter interval of length `h`, the linear interpolant `L` through
//!   the interval endpoints satisfies
//!   `max |B - L| <= h²/8 · max |B''|` (classic Rolle/Taylor argument).
//!   Since `L(t)` is a point ON the chord segment, this bounds the distance
//!   from every curve point to the polyline AND from every chord point to
//!   the curve. `B''` is constant (quad) or linear in the control-point
//!   second differences (cubic), so `max |B''|` is exact arithmetic.
//! - Circular arcs: the sagitta. A circular piece spanning angle `φ`
//!   deviates from its chord by exactly `r · (1 - cos(φ/2))`.
//! - Elliptic arcs: flattened in the unit-circle domain and mapped by the
//!   linear map `M = R(φ_axis) · diag(rx, ry)`; segments map to segments
//!   and distances scale by at most the operator norm
//!   `‖M‖ = max(rx, ry)`, so `deviation <= max(rx, ry) · sagitta_unit`.
//! - Floating-point: interior polyline points are EVALUATED in f64, so the
//!   analytic bound is widened by a conservative evaluation guard
//!   ([`eval_guard_px`], 64 ulp of the coordinate magnitude). The analytic
//!   subdivision count additionally never trusts exact real arithmetic:
//!   `n` is computed from an upper-rounded ratio and the certified bound is
//!   RE-computed from the actual `n` (never assumed equal to the request).
//!
//! Budget honesty: `n` per curve is capped ([`MAX_SUBDIVISIONS`]). If the
//! cap binds, the returned `max_deviation_px` is the certified bound AT the
//! cap — larger than requested. Callers that need the requested budget MUST
//! check `max_deviation_px <= requested` and fail typed; nothing here
//! silently pretends the budget was met.

use crate::vec2::{Pt, Vec2};

/// Hard cap on subdivisions per curve segment: keeps candidate cost bounded
/// (spec §29 "no uncontrolled growth"). At 2^14 pieces the certified bound
/// of any reasonable canvas-scale curve is far below any useful tolerance.
pub const MAX_SUBDIVISIONS: u32 = 1 << 14;

/// Typed chord tolerance in pixels (spec §5.4: tolerances typed by units
/// and purpose). Finite and strictly positive by construction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChordTolerancePx(f64);

impl ChordTolerancePx {
    pub fn new(px: f64) -> Option<ChordTolerancePx> {
        (px.is_finite() && px > 0.0).then_some(ChordTolerancePx(px))
    }

    pub fn px(self) -> f64 {
        self.0
    }
}

/// A polyline approximation of one curve with a certified deviation bound.
#[derive(Debug, Clone, PartialEq)]
pub struct FlattenedCurve {
    /// `>= 2` points; `points[0]` and `points[last]` are the EXACT input
    /// endpoints (bitwise), interior points are evaluated.
    pub points: Vec<Pt>,
    /// Certified upper bound (px) on the Hausdorff distance between the
    /// true curve and this polyline (analytic bound + evaluation guard).
    pub max_deviation_px: f64,
}

impl FlattenedCurve {
    /// Certified upper bound (px²) on the area between the true curve and
    /// this polyline: each flattened piece stays within `δ` of its chord,
    /// so the region between them lies in the closed `δ`-neighbourhood of
    /// the chord: `area <= Σ (2δ·|chord| + 4δ²)` (4 > π keeps it integer
    /// friendly and conservative).
    pub fn area_error_bound_px2(&self) -> f64 {
        let d = self.max_deviation_px;
        let mut sum = 0.0;
        for w in self.points.windows(2) {
            sum += 2.0 * d * w[0].dist(w[1]) + 4.0 * d * d;
        }
        sum
    }
}

/// Conservative bound on the f64 evaluation error of interior polyline
/// points: 64 ulp at the magnitude of the participating coordinates.
fn eval_guard_px(max_abs_coord: f64) -> f64 {
    64.0 * f64::EPSILON * max_abs_coord.max(1.0)
}

fn max_abs_coord(points: &[Pt]) -> f64 {
    let mut m = 0.0f64;
    for p in points {
        m = m.max(p.x.abs()).max(p.y.abs());
    }
    m
}

/// Subdivision count from the analytic relation `deviation(n) = k / n²`:
/// smallest `n` with `k / n² <= tol`, clamped to `[1, MAX_SUBDIVISIONS]`.
/// The +1 absorbs sqrt/division rounding; the certified bound is always
/// recomputed from the returned `n`.
fn subdivisions_for_quadratic_decay(k: f64, tol: f64) -> u32 {
    if k <= tol {
        return 1;
    }
    let n = (k / tol).sqrt().ceil() as u32 + 1;
    n.clamp(1, MAX_SUBDIVISIONS)
}

// ---------------------------------------------------------------------------
// Quadratic / cubic Bézier
// ---------------------------------------------------------------------------

fn eval_quad(p0: Pt, c: Pt, p1: Pt, t: f64) -> Pt {
    let u = 1.0 - t;
    p0 * (u * u) + c * (2.0 * u * t) + p1 * (t * t)
}

fn eval_cubic(p0: Pt, c1: Pt, c2: Pt, p1: Pt, t: f64) -> Pt {
    let u = 1.0 - t;
    p0 * (u * u * u) + c1 * (3.0 * u * u * t) + c2 * (3.0 * u * t * t) + p1 * (t * t * t)
}

/// Flatten a quadratic Bézier. `B'' = 2(p0 - 2c + p1)` (constant), so the
/// certified per-piece bound is `(1/n)²/8 · 2|B''..| = |p0-2c+p1|/(4n²)`.
pub fn flatten_quad(p0: Pt, c: Pt, p1: Pt, tol: ChordTolerancePx) -> FlattenedCurve {
    let second = p0 - c * 2.0 + p1;
    let k = second.length() / 4.0; // deviation(n) = k / n²
    let n = subdivisions_for_quadratic_decay(k, tol.px());
    let mut points = Vec::with_capacity(n as usize + 1);
    points.push(p0);
    for i in 1..n {
        points.push(eval_quad(p0, c, p1, f64::from(i) / f64::from(n)));
    }
    points.push(p1);
    let analytic = k / (f64::from(n) * f64::from(n));
    FlattenedCurve {
        max_deviation_px: analytic + eval_guard_px(max_abs_coord(&[p0, c, p1])),
        points,
    }
}

/// Flatten a cubic Bézier. `B''(t) = 6[(1-t)a + t b]` with
/// `a = p0 - 2c1 + c2`, `b = c1 - 2c2 + p1`; `max|B''| <= 6·max(|a|,|b|)`,
/// so the certified per-piece bound is `(3/4)·max(|a|,|b|)/n²`.
pub fn flatten_cubic(p0: Pt, c1: Pt, c2: Pt, p1: Pt, tol: ChordTolerancePx) -> FlattenedCurve {
    let a = p0 - c1 * 2.0 + c2;
    let b = c1 - c2 * 2.0 + p1;
    let k = 0.75 * a.length().max(b.length()); // deviation(n) = k / n²
    let n = subdivisions_for_quadratic_decay(k, tol.px());
    let mut points = Vec::with_capacity(n as usize + 1);
    points.push(p0);
    for i in 1..n {
        points.push(eval_cubic(p0, c1, c2, p1, f64::from(i) / f64::from(n)));
    }
    points.push(p1);
    let analytic = k / (f64::from(n) * f64::from(n));
    FlattenedCurve {
        max_deviation_px: analytic + eval_guard_px(max_abs_coord(&[p0, c1, c2, p1])),
        points,
    }
}

// ---------------------------------------------------------------------------
// Circular / elliptic arcs (endpoint parameterization)
// ---------------------------------------------------------------------------

/// Typed failure of the endpoint→center conversion. Validation (vice-ir)
/// rejects unrepresentable arcs before rendering; this is the defensive
/// typed form for direct callers.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ArcError {
    #[error("arc endpoints coincide")]
    DegenerateChord,
    #[error("arc not representable: {reason}")]
    NotRepresentable { reason: String },
}

/// Center parameterization of an endpoint-parameterized circular arc.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArcCenter {
    pub center: Pt,
    /// Angle of `p0` seen from the center (atan2 convention).
    pub theta0_rad: f64,
    /// Signed sweep: positive = algebraically counterclockwise. `p1` sits
    /// at `theta0_rad + sweep_rad`.
    pub sweep_rad: f64,
}

/// Endpoint → center conversion for a circular arc.
///
/// Geometry (clean-room): the center lies on the chord bisector at distance
/// `h = sqrt(r² - (|chord|/2)²)` from the midpoint. Going algebraically
/// counterclockwise along the MINOR arc, the center sits on the side of
/// `rot90_ccw(chord)`; each flipped flag mirrors the side, so the side sign
/// is `+` iff `ccw != large_arc`. The sweep is the angle difference
/// normalized into `(0, 2π)` for ccw and `(-2π, 0)` for cw.
pub fn circular_arc_center(
    p0: Pt,
    p1: Pt,
    radius_px: f64,
    large_arc: bool,
    ccw: bool,
) -> Result<ArcCenter, ArcError> {
    let d = p1 - p0;
    let chord = d.length();
    if chord == 0.0 {
        return Err(ArcError::DegenerateChord);
    }
    let half = chord / 2.0;
    if radius_px < half {
        // Guarded by validation (2r >= chord); tiny float slack is NOT
        // repaired here silently: the caller sees a typed error.
        return Err(ArcError::NotRepresentable {
            reason: format!("2*radius {} < chord {chord}", 2.0 * radius_px),
        });
    }
    let mid = Pt::new((p0.x + p1.x) / 2.0, (p0.y + p1.y) / 2.0);
    let h = (radius_px * radius_px - half * half).max(0.0).sqrt();
    let u = Vec2::new(-d.y, d.x) * (1.0 / chord); // rot90_ccw(chord), unit
    let side = if ccw != large_arc { 1.0 } else { -1.0 };
    let center = mid + u * (side * h);

    let theta0 = (p0.y - center.y).atan2(p0.x - center.x);
    let theta1 = (p1.y - center.y).atan2(p1.x - center.x);
    let tau = std::f64::consts::TAU;
    let mut sweep = theta1 - theta0;
    if ccw {
        while sweep <= 0.0 {
            sweep += tau;
        }
        while sweep > tau {
            sweep -= tau;
        }
    } else {
        while sweep >= 0.0 {
            sweep -= tau;
        }
        while sweep < -tau {
            sweep += tau;
        }
    }
    Ok(ArcCenter {
        center,
        theta0_rad: theta0,
        sweep_rad: sweep,
    })
}

/// Pieces needed so the per-piece sagitta `r(1 - cos(φ/2))` stays under
/// `tol`: `φ_max = 2·acos(1 - tol/r)`.
fn arc_subdivisions(sweep_abs: f64, r: f64, tol: f64) -> u32 {
    let ratio = 1.0 - tol / r;
    if ratio <= -1.0 {
        return 1; // tolerance beyond the diameter: one chord suffices
    }
    let phi_max = 2.0 * ratio.acos();
    if phi_max <= 0.0 {
        return MAX_SUBDIVISIONS;
    }
    let n = (sweep_abs / phi_max).ceil() as u32 + 1;
    n.clamp(1, MAX_SUBDIVISIONS)
}

fn sagitta(r: f64, phi: f64) -> f64 {
    r * (1.0 - (phi / 2.0).cos())
}

/// Flatten an endpoint-parameterized circular arc.
pub fn flatten_circular_arc(
    p0: Pt,
    p1: Pt,
    radius_px: f64,
    large_arc: bool,
    ccw: bool,
    tol: ChordTolerancePx,
) -> Result<FlattenedCurve, ArcError> {
    let arc = circular_arc_center(p0, p1, radius_px, large_arc, ccw)?;
    let n = arc_subdivisions(arc.sweep_rad.abs(), radius_px, tol.px());
    let mut points = Vec::with_capacity(n as usize + 1);
    points.push(p0);
    for i in 1..n {
        let theta = arc.theta0_rad + arc.sweep_rad * (f64::from(i) / f64::from(n));
        points.push(arc.center + Vec2::new(theta.cos(), theta.sin()) * radius_px);
    }
    points.push(p1);
    let analytic = sagitta(radius_px, arc.sweep_rad.abs() / f64::from(n));
    let guard = eval_guard_px(max_abs_coord(&[p0, p1, arc.center]) + radius_px);
    Ok(FlattenedCurve {
        max_deviation_px: analytic + guard,
        points,
    })
}

/// Flatten an endpoint-parameterized elliptic arc
/// (`x_axis_rotation_rad = φ`, radii `rx, ry`).
///
/// Clean-room construction: map the plane by `M⁻¹ = diag(1/rx, 1/ry)·R(-φ)`
/// — the arc becomes a UNIT-circle arc with the same flags (the map has
/// positive determinant, so sweep orientation is preserved). Solve the
/// circle case there, then map polyline vertices back through
/// `M = R(φ)·diag(rx, ry)`. Segments map to segments under a linear map, so
/// `deviation_ellipse <= ‖M‖ · deviation_circle = max(rx, ry) · sagitta`.
#[allow(clippy::too_many_arguments)]
pub fn flatten_elliptic_arc(
    p0: Pt,
    p1: Pt,
    rx_px: f64,
    ry_px: f64,
    x_axis_rotation_rad: f64,
    large_arc: bool,
    ccw: bool,
    tol: ChordTolerancePx,
) -> Result<FlattenedCurve, ArcError> {
    if rx_px <= 0.0 || ry_px <= 0.0 {
        return Err(ArcError::NotRepresentable {
            reason: format!("radii must be positive: rx={rx_px} ry={ry_px}"),
        });
    }
    let (sin_phi, cos_phi) = x_axis_rotation_rad.sin_cos();
    let to_circle = |p: Pt| -> Pt {
        // diag(1/rx, 1/ry) · R(-φ) · p
        Pt::new(
            (cos_phi * p.x + sin_phi * p.y) / rx_px,
            (-sin_phi * p.x + cos_phi * p.y) / ry_px,
        )
    };
    let from_circle = |p: Pt| -> Pt {
        // R(φ) · diag(rx, ry) · p
        let (x, y) = (p.x * rx_px, p.y * ry_px);
        Pt::new(cos_phi * x - sin_phi * y, sin_phi * x + cos_phi * y)
    };

    let q0 = to_circle(p0);
    let q1 = to_circle(p1);
    let chord = (q1 - q0).length();
    if chord == 0.0 {
        return Err(ArcError::DegenerateChord);
    }
    if chord > 2.0 {
        // Endpoint lambda > 1 (SVG F.6.6); validation rejects this earlier.
        return Err(ArcError::NotRepresentable {
            reason: format!("unit-circle chord {chord} > 2 (lambda > 1)"),
        });
    }
    // Unit-circle tolerance so that after scaling by ‖M‖ = max(rx, ry) the
    // ellipse-space deviation still meets the request.
    let r_norm = rx_px.max(ry_px);
    let arc = circular_arc_center(q0, q1, 1.0, large_arc, ccw)?;
    let n = arc_subdivisions(arc.sweep_rad.abs(), 1.0, tol.px() / r_norm);

    let mut points = Vec::with_capacity(n as usize + 1);
    points.push(p0);
    for i in 1..n {
        let theta = arc.theta0_rad + arc.sweep_rad * (f64::from(i) / f64::from(n));
        points.push(from_circle(
            arc.center + Vec2::new(theta.cos(), theta.sin()),
        ));
    }
    points.push(p1);
    let analytic = r_norm * sagitta(1.0, arc.sweep_rad.abs() / f64::from(n));
    let guard = eval_guard_px(max_abs_coord(&[p0, p1]) + r_norm);
    Ok(FlattenedCurve {
        max_deviation_px: analytic + guard,
        points,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tol(px: f64) -> ChordTolerancePx {
        ChordTolerancePx::new(px).unwrap()
    }

    /// Brute-force distance from a point to a polyline (independent of the
    /// flattening code: plain point-segment projection).
    fn dist_to_polyline(p: Pt, poly: &[Pt]) -> f64 {
        let mut best = f64::INFINITY;
        for w in poly.windows(2) {
            let (a, b) = (w[0], w[1]);
            let d = b - a;
            let len2 = d.length_sq();
            let t = if len2 == 0.0 {
                0.0
            } else {
                ((p - a).dot(d) / len2).clamp(0.0, 1.0)
            };
            best = best.min(p.dist(a + d * t));
        }
        best
    }

    #[test]
    fn tolerance_type_rejects_junk() {
        assert!(ChordTolerancePx::new(0.0).is_none());
        assert!(ChordTolerancePx::new(-1.0).is_none());
        assert!(ChordTolerancePx::new(f64::NAN).is_none());
        assert!(ChordTolerancePx::new(f64::INFINITY).is_none());
        assert_eq!(ChordTolerancePx::new(0.25).unwrap().px(), 0.25);
    }

    #[test]
    fn quad_flattening_meets_its_certified_bound() {
        let (p0, c, p1) = (Pt::new(0.0, 0.0), Pt::new(20.0, -30.0), Pt::new(40.0, 0.0));
        for t in [1.0, 0.1, 0.01, 0.001] {
            let f = flatten_quad(p0, c, p1, tol(t));
            assert!(f.max_deviation_px <= t * 1.000_001 + 1e-9, "budget honest");
            assert_eq!(f.points[0], p0);
            assert_eq!(*f.points.last().unwrap(), p1);
            for i in 0..=1000 {
                let s = f64::from(i) / 1000.0;
                let d = dist_to_polyline(eval_quad(p0, c, p1, s), &f.points);
                assert!(
                    d <= f.max_deviation_px + 1e-12,
                    "t={t} s={s}: {d} > {}",
                    f.max_deviation_px
                );
            }
        }
    }

    #[test]
    fn cubic_flattening_meets_its_certified_bound() {
        let (p0, c1, c2, p1) = (
            Pt::new(3.0, 7.0),
            Pt::new(-10.0, 40.0),
            Pt::new(55.0, -25.0),
            Pt::new(42.0, 13.0),
        );
        for t in [1.0, 0.1, 0.01] {
            let f = flatten_cubic(p0, c1, c2, p1, tol(t));
            assert!(f.max_deviation_px <= t * 1.000_001 + 1e-9);
            assert_eq!(f.points[0], p0);
            assert_eq!(*f.points.last().unwrap(), p1);
            for i in 0..=2000 {
                let s = f64::from(i) / 2000.0;
                let d = dist_to_polyline(eval_cubic(p0, c1, c2, p1, s), &f.points);
                assert!(d <= f.max_deviation_px + 1e-12, "t={t} s={s}");
            }
        }
    }

    #[test]
    fn refinement_is_monotone_in_point_count() {
        let (p0, c, p1) = (Pt::new(0.0, 0.0), Pt::new(10.0, 10.0), Pt::new(20.0, 0.0));
        let coarse = flatten_quad(p0, c, p1, tol(1.0));
        let fine = flatten_quad(p0, c, p1, tol(0.01));
        assert!(fine.points.len() > coarse.points.len());
        assert!(fine.max_deviation_px < coarse.max_deviation_px);
    }

    #[test]
    fn straight_quad_is_one_chord() {
        // Control point on the chord: B'' = 0, one piece, zero analytic
        // deviation (only the evaluation guard remains).
        let f = flatten_quad(
            Pt::new(0.0, 0.0),
            Pt::new(5.0, 5.0),
            Pt::new(10.0, 10.0),
            tol(0.001),
        );
        assert_eq!(f.points.len(), 2);
        assert!(f.max_deviation_px < 1e-9);
    }

    #[test]
    fn circular_arc_center_all_flag_combinations() {
        // Quarter circle r=5 from (5,0) to (0,5) around the origin.
        let (p0, p1, r) = (Pt::new(5.0, 0.0), Pt::new(0.0, 5.0), 5.0);
        // ccw minor arc: sweep +π/2, center at origin.
        let a = circular_arc_center(p0, p1, r, false, true).unwrap();
        assert!(a.center.dist(Pt::ZERO) < 1e-12);
        assert!((a.sweep_rad - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        // cw large arc: sweep -3π/2, same center.
        let b = circular_arc_center(p0, p1, r, true, false).unwrap();
        assert!(b.center.dist(Pt::ZERO) < 1e-12);
        assert!((b.sweep_rad + 3.0 * std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        // cw minor / ccw large sit on the mirrored center (5,5).
        let c = circular_arc_center(p0, p1, r, false, false).unwrap();
        assert!(c.center.dist(Pt::new(5.0, 5.0)) < 1e-12);
        assert!((c.sweep_rad + std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        let d = circular_arc_center(p0, p1, r, true, true).unwrap();
        assert!(d.center.dist(Pt::new(5.0, 5.0)) < 1e-12);
        assert!((d.sweep_rad - 3.0 * std::f64::consts::FRAC_PI_2).abs() < 1e-12);
    }

    #[test]
    fn circular_arc_flattening_meets_its_certified_bound() {
        let (p0, p1, r) = (Pt::new(5.0, 0.0), Pt::new(0.0, 5.0), 5.0);
        for (large, ccw) in [(false, true), (false, false), (true, true), (true, false)] {
            let f = flatten_circular_arc(p0, p1, r, large, ccw, tol(0.01)).unwrap();
            assert!(f.max_deviation_px <= 0.010_001);
            assert_eq!(f.points[0], p0);
            assert_eq!(*f.points.last().unwrap(), p1);
            // Sample the true arc densely; it must stay near the polyline.
            let arc = circular_arc_center(p0, p1, r, large, ccw).unwrap();
            for i in 0..=1000 {
                let theta = arc.theta0_rad + arc.sweep_rad * f64::from(i) / 1000.0;
                let p = arc.center + Vec2::new(theta.cos(), theta.sin()) * r;
                assert!(
                    dist_to_polyline(p, &f.points) <= f.max_deviation_px + 1e-12,
                    "large={large} ccw={ccw} i={i}"
                );
            }
        }
    }

    #[test]
    fn semicircle_is_representable() {
        // 2r == chord exactly: h = 0, both centers coincide.
        let f = flatten_circular_arc(
            Pt::new(0.0, 0.0),
            Pt::new(10.0, 0.0),
            5.0,
            false,
            true,
            tol(0.05),
        )
        .unwrap();
        assert!(f.points.len() > 3);
        assert!(f.max_deviation_px <= 0.050_001);
    }

    #[test]
    fn too_small_radius_is_a_typed_error() {
        match flatten_circular_arc(
            Pt::new(0.0, 0.0),
            Pt::new(10.0, 0.0),
            4.0,
            false,
            true,
            tol(0.1),
        ) {
            Err(ArcError::NotRepresentable { .. }) => {}
            other => panic!("expected NotRepresentable, got {other:?}"),
        }
    }

    #[test]
    fn elliptic_arc_flattening_meets_its_certified_bound() {
        // Ellipse rx=8, ry=3 rotated 30°, quarter-ish arc: endpoints taken
        // from the parametric form so representability is guaranteed.
        let (rx, ry, phi) = (8.0, 3.0, 0.5235987755982988_f64);
        let (sin_phi, cos_phi) = phi.sin_cos();
        let center = Pt::new(20.0, 15.0);
        let at = |theta: f64| {
            let (x, y) = (rx * theta.cos(), ry * theta.sin());
            center + Vec2::new(cos_phi * x - sin_phi * y, sin_phi * x + cos_phi * y)
        };
        let (p0, p1) = (at(0.3), at(1.9));
        for (large, ccw) in [(false, true), (true, false)] {
            let f = flatten_elliptic_arc(p0, p1, rx, ry, phi, large, ccw, tol(0.02)).unwrap();
            assert!(f.max_deviation_px <= 0.020_001, "budget honest");
            assert_eq!(f.points[0], p0);
            assert_eq!(*f.points.last().unwrap(), p1);
            // Every polyline vertex must lie on the true ellipse (map back
            // to the unit circle: radius 1 within evaluation noise).
            for p in &f.points {
                let q = Pt::new(
                    (cos_phi * (p.x - center.x) + sin_phi * (p.y - center.y)) / rx,
                    (-sin_phi * (p.x - center.x) + cos_phi * (p.y - center.y)) / ry,
                );
                assert!((q.length() - 1.0).abs() < 1e-9);
            }
            // And the dense true arc stays within the certified bound.
            let q0 = Pt::new(
                (cos_phi * (p0.x - center.x) + sin_phi * (p0.y - center.y)) / rx,
                (-sin_phi * (p0.x - center.x) + cos_phi * (p0.y - center.y)) / ry,
            );
            let arc = circular_arc_center(
                q0,
                Pt::new(
                    (cos_phi * (p1.x - center.x) + sin_phi * (p1.y - center.y)) / rx,
                    (-sin_phi * (p1.x - center.x) + cos_phi * (p1.y - center.y)) / ry,
                ),
                1.0,
                large,
                ccw,
            )
            .unwrap();
            for i in 0..=1000 {
                let theta = arc.theta0_rad + arc.sweep_rad * f64::from(i) / 1000.0;
                let unit = arc.center + Vec2::new(theta.cos(), theta.sin());
                let (x, y) = (unit.x * rx, unit.y * ry);
                let p = center + Vec2::new(cos_phi * x - sin_phi * y, sin_phi * x + cos_phi * y);
                assert!(
                    dist_to_polyline(p, &f.points) <= f.max_deviation_px + 1e-9,
                    "large={large} ccw={ccw} i={i}"
                );
            }
        }
    }

    #[test]
    fn area_error_bound_is_positive_and_scales_with_tolerance() {
        let (p0, c, p1) = (Pt::new(0.0, 0.0), Pt::new(20.0, -30.0), Pt::new(40.0, 0.0));
        let coarse = flatten_quad(p0, c, p1, tol(0.5));
        let fine = flatten_quad(p0, c, p1, tol(0.005));
        assert!(fine.area_error_bound_px2() < coarse.area_error_bound_px2());
        assert!(fine.area_error_bound_px2() > 0.0);
    }
}
