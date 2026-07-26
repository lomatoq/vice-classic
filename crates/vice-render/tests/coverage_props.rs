//! Totality property tests for the coverage accumulator (F-0012;
//! REDTEAM_M2 addendum F-M2-R7).
//!
//! The governing property, stated the same way as for `vice-geom::flatten`
//! (F-0007): for **every** input the public accumulator either returns a
//! TYPED error or returns finite values — it never panics, never returns
//! `Ok` carrying NaN, and never behaves differently in the two profiles.
//!
//! Why this file exists: the flatten totality test found a second defect
//! of its class the moment it was written, but `coverage` had no analogue,
//! so a third instance of the same class survived a whole delta — a NaN
//! vertex gave `Ok` with a NaN buffer in release and, after the
//! column-local trapezoid added a `debug_assert`, a panic in debug.
//!
//! RUN IN BOTH PROFILES. `cargo test --workspace` is the profile where
//! the defect panicked; `--release` is the delivery profile. A divergence
//! between them is itself a finding.

use proptest::prelude::*;
use vice_geom::Pt;
use vice_render::{polygon_coverage, CoverageError};

/// Coordinates across the whole exponent range, including the values that
/// break integer conversions and the ones that are not numbers at all.
fn wild_coord() -> impl Strategy<Value = f64> {
    prop_oneof![
        6 => -50.0f64..50.0,
        2 => -1.0e12f64..1.0e12,
        3 => prop::sample::select(vec![
            0.0,
            -0.0,
            1.0,
            -1.0,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::MAX,
            -f64::MAX,
            f64::MIN_POSITIVE,
            5e-324,
            1e300,
            -1e300,
            8.0,
            16.0,
        ]),
    ]
}

fn wild_pt() -> impl Strategy<Value = Pt> {
    (wild_coord(), wild_coord()).prop_map(|(x, y)| Pt::new(x, y))
}

/// A loop that is closed by construction (so closure is not what the test
/// is exercising), of arbitrary length and wildness.
fn wild_closed_loop() -> impl Strategy<Value = Vec<Pt>> {
    prop::collection::vec(wild_pt(), 2..12).prop_map(|mut v| {
        let first = v[0];
        v.push(first);
        v
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2048))]

    /// TOTALITY: typed error, or finite output. Never a panic, never
    /// `Ok(NaN)`.
    #[test]
    fn coverage_is_total_on_any_input(
        loops in prop::collection::vec(wild_closed_loop(), 1..4),
        width in 1u32..24,
        rows in 1u32..24,
    ) {
        let refs: Vec<&[Pt]> = loops.iter().map(|l| l.as_slice()).collect();
        match polygon_coverage(&refs, width, 0, rows) {
            Err(_) => {} // a typed refusal is always an acceptable answer
            Ok(cov) => {
                prop_assert_eq!(cov.len(), (width as usize) * (rows as usize));
                for v in &cov {
                    prop_assert!(
                        v.is_finite(),
                        "Ok must never carry a non-finite value: {}",
                        v
                    );
                }
            }
        }
    }

    /// A loop carrying any non-finite coordinate must be refused by NAME,
    /// not merely survived.
    #[test]
    fn non_finite_vertices_are_refused_by_name(
        mut lp in wild_closed_loop(),
        idx in 0usize..8,
        bad in prop::sample::select(vec![f64::NAN, f64::INFINITY, f64::NEG_INFINITY]),
        on_x in any::<bool>(),
    ) {
        // Poison one interior vertex (never the shared first/last pair, so
        // the loop stays closed and the closure check cannot pre-empt).
        prop_assume!(lp.len() > 2);
        let i = 1 + idx % (lp.len() - 2);
        lp[i] = if on_x { Pt::new(bad, lp[i].y) } else { Pt::new(lp[i].x, bad) };

        let refs: Vec<&[Pt]> = vec![lp.as_slice()];
        match polygon_coverage(&refs, 8, 0, 8) {
            Err(CoverageError::NonFinitePoint { point_index, point, .. }) => {
                // The generator may itself have produced a non-finite
                // vertex earlier in the loop, so the claim is not "index
                // i is reported" but "the reported point is genuinely the
                // offender" — the error must never accuse a healthy point.
                prop_assert!(!point.is_finite(), "reported point must be the offender");
                prop_assert!(point_index < lp.len());
                // Compared BITWISE: the reported point contains a NaN, and
                // NaN != NaN under PartialEq, so `==` would be unsound here.
                prop_assert_eq!(lp[point_index].x.to_bits(), point.x.to_bits());
                prop_assert_eq!(lp[point_index].y.to_bits(), point.y.to_bits());
            }
            other => prop_assert!(false, "expected NonFinitePoint, got {:?}", other),
        }
    }

    /// Any input the accumulator ACCEPTS must produce coverage values that
    /// are valid area fractions of a pixel in magnitude: a single closed
    /// loop cannot wind more than its own vertex count.
    #[test]
    fn accepted_output_is_bounded_by_winding(
        lp in wild_closed_loop(),
        width in 1u32..16,
    ) {
        let refs: Vec<&[Pt]> = vec![lp.as_slice()];
        if let Ok(cov) = polygon_coverage(&refs, width, 0, 16) {
            let limit = lp.len() as f64;
            for v in &cov {
                prop_assert!(
                    v.abs() <= limit,
                    "|coverage| {} exceeds the winding bound {}",
                    v,
                    limit
                );
            }
        }
    }
}

/// The exact reproduction from REDTEAM_M2 F-M2-R7, kept as a named
/// regression: release returned `Ok` with NaN pixels and no error, debug
/// panicked at `coverage.rs:226`.
#[test]
fn redteam_non_finite_vertex_is_a_typed_error_in_both_profiles() {
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let lp = vec![
            Pt::new(2.0, 2.0),
            Pt::new(bad, 4.0),
            Pt::new(6.0, 6.0),
            Pt::new(2.0, 2.0),
        ];
        match polygon_coverage(&[&lp], 8, 0, 8) {
            Err(CoverageError::NonFinitePoint {
                loop_index,
                point_index,
                ..
            }) => {
                assert_eq!((loop_index, point_index), (0, 1));
            }
            other => panic!("expected NonFinitePoint for {bad}, got {other:?}"),
        }
        // And the same poison on the y component.
        let lp = vec![
            Pt::new(2.0, 2.0),
            Pt::new(4.0, bad),
            Pt::new(6.0, 6.0),
            Pt::new(2.0, 2.0),
        ];
        assert!(matches!(
            polygon_coverage(&[&lp], 8, 0, 8),
            Err(CoverageError::NonFinitePoint { .. })
        ));
    }

    // The healthy neighbour of the same shape still renders exactly.
    let good = vec![
        Pt::new(2.0, 2.0),
        Pt::new(6.0, 2.0),
        Pt::new(6.0, 6.0),
        Pt::new(2.0, 6.0),
        Pt::new(2.0, 2.0),
    ];
    let cov = polygon_coverage(&[&good], 8, 0, 8).expect("finite closed loop");
    let total: f64 = cov.iter().sum();
    assert_eq!(total, 16.0);
}
