//! Property tests for certified flattening (F-0007).
//!
//! The governing property is TOTALITY: for **every** finite input the
//! flatteners must return — never panic, never wrap, never produce a
//! non-finite certificate. The defect that motivated this file
//! (`estimate.ceil() as u32 + 1`, REDTEAM_M2 F-M2-R1 / REVIEW_M2_B
//! M2-B-N1) panicked in the `dev` profile and returned a typed error in
//! `release`: one input, two outcomes.
//!
//! THESE TESTS MUST BE RUN IN BOTH PROFILES. `cargo test --workspace`
//! (dev, `overflow-checks = on`) is the profile in which the defect
//! panicked; `cargo test --release --workspace` is the delivery profile.
//! Both are part of the milestone gate, and a divergence between them is
//! itself a finding.

use proptest::prelude::*;
use vice_geom::flatten::{
    flatten_circular_arc, flatten_cubic, flatten_elliptic_arc, flatten_quad, ChordTolerancePx,
    MAX_SUBDIVISIONS,
};
use vice_geom::Pt;

/// Finite f64 across the whole exponent range, biased towards the
/// magnitudes that break integer conversions: ordinary canvas values,
/// astronomical control points, denormal-adjacent tolerances.
fn wild_coord() -> impl Strategy<Value = f64> {
    prop_oneof![
        4 => -1.0e3f64..1.0e3,
        2 => -1.0e9f64..1.0e9,
        2 => prop::sample::select(vec![
            0.0,
            -0.0,
            1.0,
            -1.0,
            1e-300,
            -1e-300,
            f64::MIN_POSITIVE,
            5e-324,
            1e17,
            4.076_193e17, // measured overflow threshold for quad ctrl
            1e19,
            5e18,
            1e100,
            f64::MAX,
            -f64::MAX,
            f64::MAX / 4.0,
        ]),
        1 => (-300i32..300).prop_map(|e| 2f64.powi(e)),
    ]
}

fn wild_pt() -> impl Strategy<Value = Pt> {
    (wild_coord(), wild_coord()).prop_map(|(x, y)| Pt::new(x, y))
}

/// Tolerances from ordinary to absurd (denormal), all strictly positive.
fn wild_tol() -> impl Strategy<Value = ChordTolerancePx> {
    prop_oneof![
        3 => (1e-6f64..10.0),
        1 => prop::sample::select(vec![
            1.0 / 64.0,
            1e-9,
            1e-18,
            1e-300,
            f64::MIN_POSITIVE,
            5e-324,
            f64::MAX,
        ]),
    ]
    .prop_filter_map("positive finite tolerance", ChordTolerancePx::new)
}

/// Invariants every successful flattening must satisfy regardless of input.
fn assert_flatten_contract(f: &vice_geom::FlattenedCurve, p0: Pt, p1: Pt) {
    assert!(f.points.len() >= 2, "at least a chord");
    assert!(
        f.points.len() <= MAX_SUBDIVISIONS as usize + 1,
        "piece count is capped: {} points",
        f.points.len()
    );
    // Endpoints pass through BITWISE: shared chain parameters are never
    // re-derived (crack-free shared boundaries depend on this).
    assert_eq!(f.points[0].x.to_bits(), p0.x.to_bits());
    assert_eq!(f.points[0].y.to_bits(), p0.y.to_bits());
    assert_eq!(f.points.last().unwrap().x.to_bits(), p1.x.to_bits());
    assert_eq!(f.points.last().unwrap().y.to_bits(), p1.y.to_bits());
    assert!(
        f.max_deviation_px >= 0.0 && !f.max_deviation_px.is_nan(),
        "certified bound is a non-negative number: {}",
        f.max_deviation_px
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2048))]

    /// Quadratic flattening is TOTAL on finite inputs.
    #[test]
    fn flatten_quad_never_panics(
        p0 in wild_pt(), c in wild_pt(), p1 in wild_pt(), tol in wild_tol()
    ) {
        let f = flatten_quad(p0, c, p1, tol);
        assert_flatten_contract(&f, p0, p1);
    }

    /// Cubic flattening is TOTAL on finite inputs.
    #[test]
    fn flatten_cubic_never_panics(
        p0 in wild_pt(), c1 in wild_pt(), c2 in wild_pt(), p1 in wild_pt(),
        tol in wild_tol()
    ) {
        let f = flatten_cubic(p0, c1, c2, p1, tol);
        assert_flatten_contract(&f, p0, p1);
    }

    /// Arc flattening either returns a contract-respecting polyline or a
    /// TYPED error — never a panic (this is the second `as u32 + 1` site).
    #[test]
    fn flatten_circular_arc_never_panics(
        p0 in wild_pt(), p1 in wild_pt(), r in wild_coord(),
        large in any::<bool>(), ccw in any::<bool>(), tol in wild_tol()
    ) {
        if let Ok(f) = flatten_circular_arc(p0, p1, r, large, ccw, tol) {
            assert_flatten_contract(&f, p0, p1);
        }
    }

    /// Elliptic arc flattening is total in the same sense.
    #[test]
    fn flatten_elliptic_arc_never_panics(
        p0 in wild_pt(), p1 in wild_pt(),
        rx in wild_coord(), ry in wild_coord(), phi in -10.0f64..10.0,
        large in any::<bool>(), ccw in any::<bool>(), tol in wild_tol()
    ) {
        if let Ok(f) = flatten_elliptic_arc(p0, p1, rx, ry, phi, large, ccw, tol) {
            assert_flatten_contract(&f, p0, p1);
        }
    }
}

/// The exact reproduction from REDTEAM_M2 F-M2-R1 and REVIEW_M2_B ADV-2c,
/// kept as a named regression: quad control point at 1e19 with the default
/// M2 budget. Before the fix this panicked with `attempt to add with
/// overflow` in the dev profile.
#[test]
fn redteam_quad_ctrl_1e19_is_a_certificate_not_a_panic() {
    let tol = ChordTolerancePx::new(1.0 / 64.0).unwrap();
    for ctrl in [
        Pt::new(1e19, 1e19),
        Pt::new(5e18, 5e18),
        Pt::new(4.076_193e17, 4.076_193e17),
        Pt::new(f64::MAX, f64::MAX),
    ] {
        let f = flatten_quad(Pt::new(0.0, 0.0), ctrl, Pt::new(1.0, 0.0), tol);
        assert_flatten_contract(&f, Pt::new(0.0, 0.0), Pt::new(1.0, 0.0));
        // The budget is honestly reported as unmet rather than pretended.
        assert!(
            f.max_deviation_px > tol.px(),
            "an unreachable budget must be reported as exceeded"
        );
    }
}

/// The latent twin of the same pattern in `arc_subdivisions`
/// (REDTEAM_M2: no reachable input found, fixed as a class anyway).
#[test]
fn extreme_arcs_are_total() {
    let tol = ChordTolerancePx::new(1.0 / 64.0).unwrap();
    for r in [1e17, 1e150, f64::MAX, 1e-9, 5e-324] {
        for (large, ccw) in [(false, true), (true, false)] {
            if let Ok(f) =
                flatten_circular_arc(Pt::new(0.0, 0.0), Pt::new(1.0, 0.0), r, large, ccw, tol)
            {
                assert_flatten_contract(&f, Pt::new(0.0, 0.0), Pt::new(1.0, 0.0));
            }
        }
    }
    // Denormal tolerance: the estimate overflows to +inf; the cap must
    // absorb it instead of the cast.
    let denormal = ChordTolerancePx::new(5e-324).unwrap();
    let f = flatten_circular_arc(
        Pt::new(0.0, 0.0),
        Pt::new(1.0, 0.0),
        10.0,
        false,
        true,
        denormal,
    )
    .unwrap();
    assert_flatten_contract(&f, Pt::new(0.0, 0.0), Pt::new(1.0, 0.0));
}
