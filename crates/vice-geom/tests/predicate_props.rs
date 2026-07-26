//! Property tests for the robust predicate adapter.
//!
//! Strategy: generate points on an exact dyadic lattice (integer coordinates
//! scaled by a power of two), where f64 representation is exact, and compare
//! every predicate answer against exact i128 integer arithmetic. This is an
//! independent reference implementation, not a self-comparison.

use proptest::prelude::*;
use vice_geom::predicates::{
    closed_segments_intersect, incircle, orient2d, point_on_closed_segment, CirclePosition,
    Orientation,
};
use vice_geom::Pt;

/// Lattice scale 2^-20: `i as f64 * SCALE` is exact for |i| < 2^53.
const SCALE: f64 = 1.0 / (1u64 << 20) as f64;

/// Integer coordinate range: cross products stay far inside i128.
const RANGE: i64 = 1 << 26;

fn to_pt(p: (i64, i64)) -> Pt {
    Pt::new(p.0 as f64 * SCALE, p.1 as f64 * SCALE)
}

fn exact_orient(a: (i64, i64), b: (i64, i64), c: (i64, i64)) -> Orientation {
    let (ax, ay) = (i128::from(a.0), i128::from(a.1));
    let (bx, by) = (i128::from(b.0), i128::from(b.1));
    let (cx, cy) = (i128::from(c.0), i128::from(c.1));
    let cross = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax);
    match cross.signum() {
        1 => Orientation::CounterClockwise,
        -1 => Orientation::Clockwise,
        _ => Orientation::Collinear,
    }
}

/// Exact incircle via the 3x3 determinant with rows (x-dx, y-dy, |..|^2),
/// normalized by triangle orientation. Coordinates <= 2^20 keep every term
/// far inside i128.
fn exact_incircle(
    a: (i64, i64),
    b: (i64, i64),
    c: (i64, i64),
    d: (i64, i64),
) -> Option<CirclePosition> {
    let orient = exact_orient(a, b, c);
    let sign: i128 = match orient {
        Orientation::CounterClockwise => 1,
        Orientation::Clockwise => -1,
        Orientation::Collinear => return None,
    };
    let row = |p: (i64, i64)| {
        let x = i128::from(p.0) - i128::from(d.0);
        let y = i128::from(p.1) - i128::from(d.1);
        (x, y, x * x + y * y)
    };
    let (ax, ay, aq) = row(a);
    let (bx, by, bq) = row(b);
    let (cx, cy, cq) = row(c);
    let det = ax * (by * cq - bq * cy) - ay * (bx * cq - bq * cx) + aq * (bx * cy - by * cx);
    Some(match (sign * det).signum() {
        1 => CirclePosition::Inside,
        -1 => CirclePosition::Outside,
        _ => CirclePosition::OnBoundary,
    })
}

fn lattice_point() -> impl Strategy<Value = (i64, i64)> {
    (-RANGE..=RANGE, -RANGE..=RANGE)
}

/// Small lattice for incircle so the exact determinant stays in range.
fn small_lattice_point() -> impl Strategy<Value = (i64, i64)> {
    const R: i64 = 1 << 20;
    (-R..=R, -R..=R)
}

proptest! {
    #[test]
    fn orient2d_matches_exact_integer_reference(
        a in lattice_point(), b in lattice_point(), c in lattice_point()
    ) {
        prop_assert_eq!(orient2d(to_pt(a), to_pt(b), to_pt(c)), exact_orient(a, b, c));
    }

    /// Constructed EXACT collinear triples plus one-lattice-step
    /// perturbations: the adapter must call the collinear case exactly and
    /// flip to the exact side for the perturbed points.
    #[test]
    fn orient2d_collinear_and_one_step_perturbations(
        ax in -1000i64..=1000, ay in -1000i64..=1000,
        dx in -1000i64..=1000, dy in -1000i64..=1000,
        t in -1000i64..=1000, s in -1000i64..=1000,
        px in -1i64..=1, py in -1i64..=1,
    ) {
        prop_assume!((dx, dy) != (0, 0));
        let a = (ax, ay);
        let b = (ax + t * dx, ay + t * dy);
        let c = (ax + s * dx, ay + s * dy);
        prop_assert_eq!(orient2d(to_pt(a), to_pt(b), to_pt(c)), Orientation::Collinear);

        let c2 = (c.0 + px, c.1 + py);
        prop_assert_eq!(orient2d(to_pt(a), to_pt(b), to_pt(c2)), exact_orient(a, b, c2));
    }

    #[test]
    fn orient2d_cyclic_and_swap_symmetries(
        a in lattice_point(), b in lattice_point(), c in lattice_point()
    ) {
        let (pa, pb, pc) = (to_pt(a), to_pt(b), to_pt(c));
        let o = orient2d(pa, pb, pc);
        prop_assert_eq!(orient2d(pb, pc, pa), o);
        prop_assert_eq!(orient2d(pc, pa, pb), o);
        let swapped = orient2d(pb, pa, pc);
        match o {
            Orientation::Collinear => prop_assert_eq!(swapped, Orientation::Collinear),
            Orientation::CounterClockwise => prop_assert_eq!(swapped, Orientation::Clockwise),
            Orientation::Clockwise => prop_assert_eq!(swapped, Orientation::CounterClockwise),
        }
    }

    #[test]
    fn incircle_matches_exact_integer_reference(
        a in small_lattice_point(), b in small_lattice_point(),
        c in small_lattice_point(), d in small_lattice_point(),
    ) {
        prop_assert_eq!(
            incircle(to_pt(a), to_pt(b), to_pt(c), to_pt(d)),
            exact_incircle(a, b, c, d)
        );
    }

    /// Segment intersection agrees with an exact integer implementation.
    #[test]
    fn segment_intersection_matches_exact_reference(
        p1 in lattice_point(), p2 in lattice_point(),
        q1 in lattice_point(), q2 in lattice_point(),
    ) {
        let expected = exact_segments_intersect(p1, p2, q1, q2);
        prop_assert_eq!(
            closed_segments_intersect(to_pt(p1), to_pt(p2), to_pt(q1), to_pt(q2)),
            expected
        );
    }

    /// Points constructed exactly on a segment are always reported on it;
    /// one lattice step off (non-collinear) is always reported off it.
    #[test]
    fn point_on_segment_is_exact(
        ax in -1000i64..=1000, ay in -1000i64..=1000,
        dx in -1000i64..=1000, dy in -1000i64..=1000,
        k in 0i64..=1000, n in 1i64..=1000,
    ) {
        prop_assume!((dx, dy) != (0, 0));
        prop_assume!(k <= n);
        let a = (ax, ay);
        let b = (ax + n * dx, ay + n * dy);
        let p = (ax + k * dx, ay + k * dy);
        prop_assert!(point_on_closed_segment(to_pt(p), to_pt(a), to_pt(b)));

        // Perpendicular one-step offset leaves the line exactly.
        let off = (p.0 - dy, p.1 + dx);
        prop_assert!(!point_on_closed_segment(to_pt(off), to_pt(a), to_pt(b)));
    }
}

fn exact_on_segment(p: (i64, i64), a: (i64, i64), b: (i64, i64)) -> bool {
    if exact_orient(a, b, p) != Orientation::Collinear {
        return false;
    }
    p.0 >= a.0.min(b.0) && p.0 <= a.0.max(b.0) && p.1 >= a.1.min(b.1) && p.1 <= a.1.max(b.1)
}

fn exact_segments_intersect(
    p1: (i64, i64),
    p2: (i64, i64),
    q1: (i64, i64),
    q2: (i64, i64),
) -> bool {
    let o1 = exact_orient(p1, p2, q1);
    let o2 = exact_orient(p1, p2, q2);
    let o3 = exact_orient(q1, q2, p1);
    let o4 = exact_orient(q1, q2, p2);
    if o1 == Orientation::Collinear && exact_on_segment(q1, p1, p2) {
        return true;
    }
    if o2 == Orientation::Collinear && exact_on_segment(q2, p1, p2) {
        return true;
    }
    if o3 == Orientation::Collinear && exact_on_segment(p1, q1, q2) {
        return true;
    }
    if o4 == Orientation::Collinear && exact_on_segment(p2, q1, q2) {
        return true;
    }
    let opp = |x: Orientation, y: Orientation| {
        matches!(
            (x, y),
            (Orientation::CounterClockwise, Orientation::Clockwise)
                | (Orientation::Clockwise, Orientation::CounterClockwise)
        )
    };
    opp(o1, o2) && opp(o3, o4)
}
