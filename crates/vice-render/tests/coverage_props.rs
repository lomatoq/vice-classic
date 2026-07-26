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

mod common;

use common::{sh_clip_coverage, supersampled_winding_pixel};
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

/// The named reproduction from REDTEAM_M2 addendum 2 (F-M2-R11): two
/// finite vertices whose y-span overflows. Before the fix the edge was
/// integrated as a VERTICAL line at `xlo` — finite, in range, `Ok`, and
/// wrong by a full pixel (total area -53.33 against a true -13.33).
#[test]
fn redteam_span_overflow_is_integrated_correctly() {
    // x_at must interpolate to 7.5 at the canvas rows, not stay at xlo.
    let lp = vec![
        Pt::new(2.5, -1.0e308),
        Pt::new(12.5, 1.0e308),
        Pt::new(12.5, -1.0e308),
        Pt::new(2.5, -1.0e308),
    ];
    let cov = polygon_coverage(&[&lp], 16, 0, 8).expect("finite vertices");
    let total: f64 = cov.iter().sum();
    assert!(total.is_finite());

    // The edge crosses the canvas at x = 7.5, so on every row the columns
    // strictly left of 7 are uncovered by that edge's crossing and column
    // 7 is half covered by it. The pre-fix behaviour put the crossing at
    // x = 2.5 instead - a 5 px position error.
    for r in 0..8usize {
        let row = &cov[r * 16..(r + 1) * 16];
        assert!(
            row[3].abs() < 1e-9,
            "row {r}: column 3 must be outside the swept region, got {}",
            row[3]
        );
        assert!(
            (row[7].abs() - 0.5).abs() < 1e-6,
            "row {r}: column 7 must be half swept, got {}",
            row[7]
        );
    }
}

/// The same class in the row walk: a run whose x-span overflows must not
/// collapse its whole contribution into one column.
#[test]
fn row_span_overflow_does_not_collapse_into_one_column() {
    let lp = vec![
        Pt::new(-1.0e308, 2.25),
        Pt::new(1.0e308, 5.75),
        Pt::new(1.0e308, 2.25),
        Pt::new(-1.0e308, 2.25),
    ];
    let cov = polygon_coverage(&[&lp], 12, 0, 8).expect("finite vertices");
    for v in &cov {
        assert!(v.is_finite());
    }
    // Geometry: the hypotenuse runs from (-1e308, 2.25) to (1e308, 5.75),
    // so ACROSS THE CANVAS (x in [0,12], negligible against 1e308) it sits
    // at y = 4.0, and the triangle covers y in [2.25, 4.0] at every canvas
    // column. Row 3 (y in [3,4]) is therefore fully covered everywhere,
    // and row 4 is empty — the true answer, which is what makes this a
    // usable oracle.
    let row3 = &cov[3 * 12..4 * 12];
    for (c, v) in row3.iter().enumerate() {
        assert!(
            (v.abs() - 1.0).abs() < 1e-6,
            "row 3 column {c} must be fully swept, got {v}"
        );
    }
    let row4 = &cov[4 * 12..5 * 12];
    for (c, v) in row4.iter().enumerate() {
        assert!(v.abs() < 1e-6, "row 4 column {c} must be empty, got {v}");
    }
}

// ---------------------------------------------------------------------
// DIFFERENTIAL property: agreement with an independent reference.
//
// Why this exists (the third recurrence of one meta-defect). The two
// properties above check that the accumulator does not PANIC and does not
// return NaN, and `accepted_output_is_bounded_by_winding` checks only
// that |coverage| <= vertex count — a wrong answer of 1.0 passes all
// three trivially. None of them compares against anything. That is
// exactly how a wrong-by-a-full-pixel result (F-0013) survived a delta
// that was specifically about this class: the tests could see crashes but
// not incorrectness.
//
// The instrument that closes the class is differential: random inputs,
// including magnitudes and spans that cross the exponent range, judged
// against an implementation that shares no code with the accumulator
// (per-pixel Sutherland-Hodgman clipping plus shoelace).
// ---------------------------------------------------------------------

/// Is the answer DETERMINED by the input, measured rather than modelled
/// (F-M2-R13)?
///
/// Both previous attempts at this boundary were analytic error models, and
/// both were wrong in the same direction. `eps*mag_x` was the conditioning
/// of the old `x_at`, not a limit of f64 (F-0014). Narrowing to
/// `eps*mag_y*|slope|` does not help either: on the red team fixture the
/// two terms are EQUAL (2.22e134 each - the addendum figure of 2.2e-166
/// for the second is an arithmetic slip), while the crossing position is
/// in fact determined exactly. Both are upper bounds the geometry does not
/// attain, so either one excludes cases the accumulator must get right.
///
/// So the criterion is no longer a model. The real question is whether the
/// f64 input determines the answer: perturb every coordinate by one ulp
/// and ask the INDEPENDENT reference how far its own answer moves. If the
/// reference is stable, the geometry is determined and our accumulator is
/// obliged to agree. If a one-ulp change swings the answer, nothing can be
/// obliged - and that is a property of the input, established by
/// measurement, on a party that is not the one under test.
///
/// This also supplies the agreement bound, so boundary and bound come from
/// ONE measurement instead of two different models (F-M2-R13 item 4b: the
/// old bound came from raw magnitude and handed out 0.25 where the
/// achievable accuracy was ~4e-15).
fn reference_sensitivity(loops: &[Vec<Pt>], w: u32, h: u32) -> f64 {
    let refs: Vec<&[Pt]> = loops.iter().map(|l| l.as_slice()).collect();
    let base = sh_clip_coverage(&refs, w, h);

    let nudged: Vec<Vec<Pt>> = loops
        .iter()
        .map(|lp| {
            let mut v: Vec<Pt> = lp
                .iter()
                .map(|p| Pt::new(next_ulp(p.x), next_ulp(p.y)))
                .collect();
            let first = v[0];
            let n = v.len();
            v[n - 1] = first; // keep the loop closed
            v
        })
        .collect();
    let nudged_refs: Vec<&[Pt]> = nudged.iter().map(|l| l.as_slice()).collect();
    let moved = sh_clip_coverage(&nudged_refs, w, h);

    let mut worst = 0.0f64;
    for (a, b) in base.iter().zip(&moved) {
        if !a.is_finite() || !b.is_finite() {
            return f64::INFINITY;
        }
        worst = worst.max((a - b).abs());
    }
    worst
}

fn next_ulp(v: f64) -> f64 {
    if v == 0.0 {
        f64::MIN_POSITIVE
    } else if v > 0.0 {
        v.next_up()
    } else {
        v.next_down()
    }
}

/// Bound on a legitimate disagreement: what the geometry itself can
/// resolve, times a factor for the several edges that may cross one pixel,
/// floored so ordinary scenes get a fixed 1e-12.
fn agreement_bound(sensitivity: f64) -> f64 {
    (16.0 * sensitivity).clamp(1e-12, 0.25)
}

/// Loops whose magnitudes are ordinary, extreme, or deliberately span the
/// exponent range — the last class is what produced F-0013.
fn differential_loop() -> impl Strategy<Value = Vec<Pt>> {
    // The extreme class is CONSTRUCTED, not filtered (the M1-N8 lesson:
    // repair the generator rather than reject from it). Its shape is the
    // one that matters: modest x with an astronomical y-span, so the
    // slope is tiny, `yhi - ylo` overflows, and the position stays
    // perfectly resolvable — exactly the configuration in which the
    // accumulator returned a confident wrong answer (F-0013).
    let ordinary = prop::collection::vec((-40.0f64..40.0, -40.0f64..40.0), 3..7);
    let huge_y = prop::sample::select(vec![
        1e150f64, -1e150, 1e300, -1e300, 1e308, -1e308, 5e307, -5e307,
    ]);
    let spanning = prop::collection::vec(
        (
            -40.0f64..40.0,
            prop_oneof![2 => -40.0f64..40.0, 3 => huge_y],
        ),
        3..6,
    );
    // The class the OLD predicate excluded (F-M2-R13): a vertex whose x is
    // astronomically large paired with an even larger y, so the slope stays
    // tiny and the position is perfectly resolvable — the shape on which
    // the accumulator used to return 0 instead of 0.278. It is generated
    // deliberately and heavily, because it is the class that was taken out
    // from under this very property.
    let far_corner = prop::collection::vec(
        prop_oneof![
            3 => (-40.0f64..40.0, -40.0f64..40.0),
            2 => prop::sample::select(vec![
                (1e150f64, -1e300f64),
                (-1e150, 1e300),
                (1e75, -1e150),
                (1e100, 1e200),
                (-1e100, -1e200),
                (1e40, 1e80),
            ]),
        ],
        3..6,
    );
    prop_oneof![2 => ordinary, 3 => spanning, 3 => far_corner].prop_map(|v| {
        let mut pts: Vec<Pt> = v.into_iter().map(|(x, y)| Pt::new(x, y)).collect();
        // Drop consecutive duplicates. A doubled vertex is a zero-area spur
        // traversed in both directions; our accumulator handles it exactly
        // at every magnitude (the two traversals cancel structurally), but
        // the clipping ORACLE cannot resolve it past ~1e50 — see
        // `degenerate_spur_is_correct_where_the_oracle_is_not`, which
        // measures which party is wrong rather than assuming. Excluding it
        // here removes an oracle limitation from the comparison, not a
        // subject class from testing.
        pts.dedup_by(|a, b| a.x == b.x && a.y == b.y);
        if pts.len() < 3 {
            pts = vec![Pt::new(0.0, 0.0), Pt::new(3.0, 0.5), Pt::new(1.0, 2.5)];
        }
        let first = pts[0];
        pts.push(first);
        pts
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(3072))]

    /// Whatever the accumulator ACCEPTS must agree with the independent
    /// reference within the derived bound. A typed refusal is always an
    /// acceptable answer; a confident wrong number is not.
    #[test]
    fn accepted_coverage_agrees_with_the_independent_reference(
        loops in prop::collection::vec(differential_loop(), 1..3),
        w in 4u32..12,
        h in 4u32..12,
    ) {
        let refs: Vec<&[Pt]> = loops.iter().map(|l| l.as_slice()).collect();
        let Ok(ours) = polygon_coverage(&refs, w, 0, h) else {
            return Ok(()); // refused, and refusing is always allowed
        };
        // Skip only what a one-ulp input change makes undecidable.
        let sensitivity = reference_sensitivity(&loops, w, h);
        prop_assume!(sensitivity <= 0.25);
        let reference = sh_clip_coverage(&refs, w, h);
        let bound = agreement_bound(sensitivity);
        for (i, (a, b)) in ours.iter().zip(&reference).enumerate() {
            // The reference may itself fail to be finite on inputs the
            // accumulator accepted; that is the reference's limit, not a
            // disagreement, and it is reported rather than silently
            // skipped by requiring the accumulator's own value finite.
            prop_assert!(a.is_finite(), "accumulator returned non-finite at {}", i);
            if !b.is_finite() {
                continue;
            }
            if (a - b).abs() <= bound {
                continue;
            }
            // The two parties disagree. Neither is privileged — both have
            // been the wrong one — so a THIRD, independent method decides
            // (the same arbitration the differential court uses). Ours is
            // at fault only if it also disagrees with the arbiter.
            let (px, py) = ((i % w as usize) as u32, (i / w as usize) as u32);
            let c = supersampled_winding_pixel(&refs, px, py, 24);
            prop_assert!(
                (a - c).abs() <= 0.1,
                "pixel {}: ours {} vs reference {} (bound {:e}); arbiter says {} — ours is the outlier",
                i, a, b, bound, c
            );
        }
    }
}

/// WITHDRAWN CLAIM, kept as a record rather than deleted.
///
/// This test used to assert that the fixture below was an unresolvable
/// regime where "no implementation is right", citing a three-way
/// disagreement of 0 / +0.278 / -0.517. That justification was wrong in
/// two of its three points (F-0014): +0.278 is correct, 0 was our own
/// defect, and -0.517 was my supersampler overflowing. The fixture is in
/// fact decidable — a one-ulp perturbation moves the reference by 1.7e-16
/// — and the accumulator now computes it correctly, which
/// `redteam_excluded_fixture_is_computed_correctly` asserts.
///
/// What replaces it is the limitation actually found while rebuilding the
/// instrument, and it points the other way: on a near-degenerate spur at
/// extreme magnitude the CLIPPING ORACLE is the wrong party, and our
/// accumulator is exactly right at every magnitude. Measured totals over a
/// 4x4 canvas for the zero-area loop [A, B, B, A]:
///
/// ```text
/// A = (1e1 , 2e1 ) : accumulator 0    reference  0
/// A = (1e10, 1e20) : accumulator 0    reference  6.0e-16
/// A = (1e50, 1e100): accumulator 0    reference -16      <- oracle fails
/// A = (1e100,1e200): accumulator 0    reference -16      <- oracle fails
/// ```
///
/// The accumulator is right here structurally: the spur is traversed in
/// both directions and the two contributions cancel whatever the computed
/// positions are. This is why the differential property arbitrates
/// disagreements with a third method instead of trusting either party.
#[test]
fn degenerate_spur_is_correct_where_the_oracle_is_not() {
    for (ax, ay) in [(1e1, 2e1), (1e10, 1e20), (1e50, 1e100), (1e100, 1e200)] {
        let a = Pt::new(ax, ay);
        let b = Pt::new(0.0, 0.0);
        let lp = vec![a, b, b, a];
        let cov = polygon_coverage(&[&lp], 4, 0, 4).expect("finite vertices");
        let total: f64 = cov.iter().sum();
        assert!(
            total.abs() < 1e-12,
            "zero-area spur must contribute nothing at ({ax:e},{ay:e}), got {total}"
        );
        for v in &cov {
            assert!(v.is_finite());
        }
    }
}

/// REDTEAM_M2 addendum 3, F-M2-R13: the fixture my own resolvability
/// predicate excluded — and on which the accumulator was silently wrong.
///
/// Pixel-scale geometry is unambiguous here: edge B->C has slope
/// dx/dy = -1e-150, so within canvas rows it stands at x ~ 0; edge C->A
/// likewise stands at x ~ A.x. The only boundary crossing pixel (3,0) is
/// therefore the segment A->B through the origin, and the coverage is
/// (A.y / A.x) * (4^2 - 3^2) / 2 = 0.278144151...
///
/// Four independent methods agree on that value (closed form, the
/// conditioned clipping reference, 64x64 supersampling, and this
/// accumulator itself at every benign magnitude of the identical
/// pixel-scale geometry). Before F-0014 the accumulator returned exactly
/// 0.0 here.
#[test]
fn redteam_excluded_fixture_is_computed_correctly() {
    let a = Pt::new(10.018579925919843, 0.7961741172608049);
    let b = Pt::new(0.0, 0.0);
    let c = Pt::new(1e150, -1e300);
    let lp = vec![a, b, c, a];

    let cov = polygon_coverage(&[&lp], 4, 0, 4).expect("finite vertices");
    let got = cov[3].abs(); // pixel (3, 0)
    let expected = (a.y / a.x) * 3.5;
    assert!(
        (got - expected).abs() < 1e-12,
        "pixel (3,0): got {got}, closed form {expected}"
    );

    // Magnitude convergence: the same pixel-scale geometry with the far
    // vertex pulled in must give the same answer all the way out. This is
    // what proves the value is a property of the geometry and not of the
    // magnitude — the argument that showed the exclusion was unjustified.
    for (fx, fy) in [
        (1e0, -1e1),
        (1e6, -1e12),
        (1e10, -1e20),
        (1e75, -1e150),
        (1e150, -1e300),
    ] {
        let lp = vec![a, b, Pt::new(fx, fy), a];
        let cov = polygon_coverage(&[&lp], 4, 0, 4).expect("finite vertices");
        assert!(
            (cov[3].abs() - expected).abs() < 1e-9,
            "far ({fx:e},{fy:e}): got {}, expected {expected}",
            cov[3].abs()
        );
    }
}
