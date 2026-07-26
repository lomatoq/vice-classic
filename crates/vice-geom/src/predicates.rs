//! Robust geometric predicates — thin typed adapter over the `robust` crate.
//!
//! Spec v1.3 §5.4: combinatorial topology decisions never rely on
//! `abs(cross) < eps` in plain f64. This module is the single entry point
//! for exact orientation / incircle / segment-intersection decisions; the
//! result is a typed enum, never a raw float whose sign a caller might
//! misinterpret.
//!
//! Implementation: the external OSS crate `robust` (Rust port of
//! J. R. Shewchuk's adaptive-precision predicates, spec §37 item 11).
//! Donor code from the pinned repositories is NOT used (REVIEW_M0
//! condition 6); see docs/ADR/ADR-0004-robust-predicates.md.
//!
//! Sign convention: [`Orientation::CounterClockwise`] means the ALGEBRAIC
//! cross product `cross(b - a, c - a)` is positive (right-handed axes).
//! In the image frame y points DOWN, so an algebraically counterclockwise
//! triple looks clockwise on screen. Callers must reason in the algebraic
//! convention; visual language is banned from topology code.

use robust::{incircle as robust_incircle, orient2d as robust_orient2d, Coord};

use crate::vec2::Pt;

/// Exact orientation of the ordered triple `(a, b, c)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    /// `cross(b - a, c - a) > 0` exactly (algebraic convention).
    CounterClockwise,
    /// `cross(b - a, c - a) < 0` exactly.
    Clockwise,
    /// The three points are exactly collinear.
    Collinear,
}

fn coord(p: Pt) -> Coord<f64> {
    Coord { x: p.x, y: p.y }
}

/// Exact orientation test (adaptive precision, correct sign for any finite
/// f64 inputs).
pub fn orient2d(a: Pt, b: Pt, c: Pt) -> Orientation {
    let v = robust_orient2d(coord(a), coord(b), coord(c));
    if v > 0.0 {
        Orientation::CounterClockwise
    } else if v < 0.0 {
        Orientation::Clockwise
    } else {
        Orientation::Collinear
    }
}

/// Position of a query point relative to a circle through three points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CirclePosition {
    /// Strictly inside the circumcircle of the triangle.
    Inside,
    /// Strictly outside.
    Outside,
    /// Exactly on the circle.
    OnBoundary,
}

/// Exact incircle test: where does `d` lie relative to the circumcircle of
/// the triangle `(a, b, c)`?
///
/// The result is orientation-normalized (independent of whether `(a, b, c)`
/// is given clockwise or counterclockwise). Returns `None` when `(a, b, c)`
/// are collinear — a degenerate triangle has no circumcircle, and hiding
/// that behind an arbitrary answer would be a silent topology decision.
pub fn incircle(a: Pt, b: Pt, c: Pt, d: Pt) -> Option<CirclePosition> {
    let sign = match orient2d(a, b, c) {
        Orientation::CounterClockwise => 1.0,
        Orientation::Clockwise => -1.0,
        Orientation::Collinear => return None,
    };
    let v = sign * robust_incircle(coord(a), coord(b), coord(c), coord(d));
    Some(if v > 0.0 {
        CirclePosition::Inside
    } else if v < 0.0 {
        CirclePosition::Outside
    } else {
        CirclePosition::OnBoundary
    })
}

/// Exact test: does `p` lie on the CLOSED segment `[a, b]`?
///
/// Degenerate segments (`a == b`) degrade to point equality.
pub fn point_on_closed_segment(p: Pt, a: Pt, b: Pt) -> bool {
    if orient2d(a, b, p) != Orientation::Collinear {
        return false;
    }
    // Collinear: 1D closed-range check on both axes (exact comparisons).
    let (xmin, xmax) = if a.x <= b.x { (a.x, b.x) } else { (b.x, a.x) };
    let (ymin, ymax) = if a.y <= b.y { (a.y, b.y) } else { (b.y, a.y) };
    xmin <= p.x && p.x <= xmax && ymin <= p.y && p.y <= ymax
}

/// Exact test: do the CLOSED segments `[p1, p2]` and `[q1, q2]` share at
/// least one point (proper crossing, T-touch, endpoint touch or collinear
/// overlap all count)?
pub fn closed_segments_intersect(p1: Pt, p2: Pt, q1: Pt, q2: Pt) -> bool {
    let o1 = orient2d(p1, p2, q1);
    let o2 = orient2d(p1, p2, q2);
    let o3 = orient2d(q1, q2, p1);
    let o4 = orient2d(q1, q2, p2);

    // Any endpoint exactly on the other closed segment (covers T-touch,
    // endpoint touch, collinear overlap and degenerate segments).
    if o1 == Orientation::Collinear && point_on_closed_segment(q1, p1, p2) {
        return true;
    }
    if o2 == Orientation::Collinear && point_on_closed_segment(q2, p1, p2) {
        return true;
    }
    if o3 == Orientation::Collinear && point_on_closed_segment(p1, q1, q2) {
        return true;
    }
    if o4 == Orientation::Collinear && point_on_closed_segment(p2, q1, q2) {
        return true;
    }

    // Proper crossing: strictly opposite orientations on both sides.
    let opposite = |x: Orientation, y: Orientation| {
        matches!(
            (x, y),
            (Orientation::CounterClockwise, Orientation::Clockwise)
                | (Orientation::Clockwise, Orientation::CounterClockwise)
        )
    };
    opposite(o1, o2) && opposite(o3, o4)
}

/// Exact test for two segments that share exactly one endpoint `shared`:
/// do they overlap along a common line beyond the shared point?
///
/// `e1` and `e2` are the non-shared endpoints. Returns true when the
/// segments are collinear and point in the same direction from `shared`
/// (so a whole sub-segment, not just the endpoint, is common).
pub fn shared_endpoint_segments_overlap(shared: Pt, e1: Pt, e2: Pt) -> bool {
    if orient2d(shared, e1, e2) != Orientation::Collinear {
        return false;
    }
    // Same direction from `shared` <=> the closed segments overlap beyond
    // the point: e1 lies on [shared, e2] or e2 lies on [shared, e1].
    point_on_closed_segment(e1, shared, e2) || point_on_closed_segment(e2, shared, e1)
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: fn(f64, f64) -> Pt = Pt::new;

    #[test]
    fn orient2d_basic_signs() {
        // Algebraic convention: (0,0) -> (1,0) -> (0,1) has positive cross.
        assert_eq!(
            orient2d(P(0.0, 0.0), P(1.0, 0.0), P(0.0, 1.0)),
            Orientation::CounterClockwise
        );
        assert_eq!(
            orient2d(P(0.0, 0.0), P(0.0, 1.0), P(1.0, 0.0)),
            Orientation::Clockwise
        );
        assert_eq!(
            orient2d(P(0.0, 0.0), P(1.0, 1.0), P(2.0, 2.0)),
            Orientation::Collinear
        );
        // Degenerate triples are collinear.
        assert_eq!(
            orient2d(P(3.0, 4.0), P(3.0, 4.0), P(1.0, 2.0)),
            Orientation::Collinear
        );
    }

    #[test]
    fn orient2d_antisymmetry_and_rotation() {
        let (a, b, c) = (P(0.25, 0.75), P(13.5, -2.0), P(-7.0, 41.0));
        let o = orient2d(a, b, c);
        assert_eq!(orient2d(b, c, a), o);
        assert_eq!(orient2d(c, a, b), o);
        let flipped = orient2d(b, a, c);
        assert_ne!(o, flipped);
    }

    /// Classic near-degenerate stress: points 0.5 + i*2^-53 against the line
    /// through (12,12) and (24,24). Naive f64 cross products give unstable
    /// signs here; the exact sign is known from integer arithmetic after
    /// scaling by 2^53 (all coordinates are exactly representable).
    #[test]
    fn orient2d_near_degenerate_matches_exact_integer_sign() {
        let a = P(12.0, 12.0);
        let b = P(24.0, 24.0);
        let ulp = (2.0f64).powi(-53);
        let base: i128 = 1 << 52; // 0.5 scaled by 2^53
        let s: i128 = 12 << 53;
        let t: i128 = 24 << 53;
        for i in -3i128..=3 {
            for j in -3i128..=3 {
                let c = P(0.5 + (i as f64) * ulp, 0.5 + (j as f64) * ulp);
                let (cx, cy) = (base + i, base + j);
                let exact = (t - s) * (cy - s) - (t - s) * (cx - s);
                let expected = match exact.signum() {
                    1 => Orientation::CounterClockwise,
                    -1 => Orientation::Clockwise,
                    _ => Orientation::Collinear,
                };
                assert_eq!(orient2d(a, b, c), expected, "i={i} j={j}");
            }
        }
    }

    #[test]
    fn incircle_rectangle_corners_are_cocircular() {
        // Rectangle corners lie on one circle: exact OnBoundary.
        let (a, b, c, d) = (P(0.0, 0.0), P(4.0, 0.0), P(4.0, 2.0), P(0.0, 2.0));
        assert_eq!(incircle(a, b, c, d), Some(CirclePosition::OnBoundary));
        // Orientation-normalized: reversing the triangle keeps the answer.
        assert_eq!(incircle(a, c, b, d), Some(CirclePosition::OnBoundary));
        // Center (2,1) is strictly inside; (5,2) strictly outside.
        assert_eq!(incircle(a, b, c, P(2.0, 1.0)), Some(CirclePosition::Inside));
        assert_eq!(
            incircle(a, b, c, P(5.0, 2.0)),
            Some(CirclePosition::Outside)
        );
    }

    #[test]
    fn incircle_one_ulp_perturbation_flips_deterministically() {
        let (a, b, c) = (P(0.0, 0.0), P(4.0, 0.0), P(4.0, 2.0));
        let on = P(0.0, 2.0);
        assert_eq!(incircle(a, b, c, on), Some(CirclePosition::OnBoundary));
        // One ulp toward the center -> strictly inside; away -> outside.
        let inward = P(on.x.next_up(), on.y.next_down());
        let outward = P(on.x.next_down(), on.y.next_up());
        assert_eq!(incircle(a, b, c, inward), Some(CirclePosition::Inside));
        assert_eq!(incircle(a, b, c, outward), Some(CirclePosition::Outside));
    }

    #[test]
    fn incircle_rejects_collinear_triangle() {
        assert_eq!(
            incircle(P(0.0, 0.0), P(1.0, 1.0), P(2.0, 2.0), P(0.0, 1.0)),
            None
        );
    }

    #[test]
    fn segment_intersection_cases() {
        // Proper crossing.
        assert!(closed_segments_intersect(
            P(0.0, 0.0),
            P(2.0, 2.0),
            P(0.0, 2.0),
            P(2.0, 0.0)
        ));
        // Disjoint.
        assert!(!closed_segments_intersect(
            P(0.0, 0.0),
            P(1.0, 0.0),
            P(0.0, 1.0),
            P(1.0, 1.0)
        ));
        // T-touch: endpoint of one lies in the interior of the other.
        assert!(closed_segments_intersect(
            P(0.0, 0.0),
            P(2.0, 0.0),
            P(1.0, 0.0),
            P(1.0, 5.0)
        ));
        // Endpoint-endpoint touch counts for CLOSED segments.
        assert!(closed_segments_intersect(
            P(0.0, 0.0),
            P(1.0, 0.0),
            P(1.0, 0.0),
            P(2.0, 5.0)
        ));
        // Collinear overlap.
        assert!(closed_segments_intersect(
            P(0.0, 0.0),
            P(3.0, 0.0),
            P(2.0, 0.0),
            P(5.0, 0.0)
        ));
        // Collinear but disjoint.
        assert!(!closed_segments_intersect(
            P(0.0, 0.0),
            P(1.0, 0.0),
            P(2.0, 0.0),
            P(3.0, 0.0)
        ));
        // Parallel, not collinear.
        assert!(!closed_segments_intersect(
            P(0.0, 0.0),
            P(3.0, 0.0),
            P(0.0, 1.0),
            P(3.0, 1.0)
        ));
        // Degenerate: point on segment.
        assert!(closed_segments_intersect(
            P(1.0, 0.0),
            P(1.0, 0.0),
            P(0.0, 0.0),
            P(2.0, 0.0)
        ));
        // Degenerate: point off segment.
        assert!(!closed_segments_intersect(
            P(1.0, 1.0),
            P(1.0, 1.0),
            P(0.0, 0.0),
            P(2.0, 0.0)
        ));
    }

    #[test]
    fn near_degenerate_segment_touch_is_exact() {
        // q1 is one ulp off the diagonal: naive arithmetic may call it
        // "on the segment"; exact predicates must not.
        let p1 = P(0.0, 0.0);
        let p2 = P(1.0, 1.0);
        let off = P(0.5, 0.5f64.next_up());
        assert!(!point_on_closed_segment(off, p1, p2));
        assert!(point_on_closed_segment(P(0.5, 0.5), p1, p2));
        // A segment from `off` going away from the diagonal never meets it.
        assert!(!closed_segments_intersect(p1, p2, off, P(0.5, 2.0)));
    }

    #[test]
    fn shared_endpoint_overlap_detection() {
        let s = P(1.0, 1.0);
        // Same ray, overlapping run.
        assert!(shared_endpoint_segments_overlap(
            s,
            P(3.0, 3.0),
            P(5.0, 5.0)
        ));
        // Opposite rays along one line: only the shared point is common.
        assert!(!shared_endpoint_segments_overlap(
            s,
            P(3.0, 3.0),
            P(-1.0, -1.0)
        ));
        // Not collinear.
        assert!(!shared_endpoint_segments_overlap(
            s,
            P(3.0, 3.0),
            P(3.0, 4.0)
        ));
        // Nested same-direction run.
        assert!(shared_endpoint_segments_overlap(
            s,
            P(5.0, 5.0),
            P(3.0, 3.0)
        ));
    }
}
