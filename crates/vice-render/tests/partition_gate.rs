//! M2 gate tests "partition sum" (spec §28 M2) plus the REVIEW_M1 M1-N5
//! hard checks: per-pixel partition invariants, direct triple-junction
//! area fractions, and typed rejection of the B4 (wrong loop orientation)
//! and B2 (wrong nesting) scene classes.

mod common;

use common::*;
use vice_geom::Pt;
use vice_ir::color::PremulRgba;
use vice_ir::{LinearRgb, Paint, PixelFilter, ValidatedScene};
use vice_render::{render_partition, RenderError, RenderOptions, TessellationBudget};

fn opts() -> RenderOptions {
    RenderOptions::default()
}

fn assert_partition_invariants(scene: &ValidatedScene) -> vice_render::PartitionRender {
    let r = render_partition(scene, &opts()).expect("partition renders");
    let n = (r.width_px as usize) * (r.height_px as usize);
    for i in 0..n {
        let mut sum = 0.0f64;
        for cov in &r.face_coverage {
            let v = cov[i];
            assert!(
                (-1e-9..=1.0 + 1e-9).contains(&v),
                "face coverage {v} out of range at {i}"
            );
            sum += v;
        }
        assert!((sum - 1.0).abs() <= 1e-9, "partition sum {sum} at {i}");
    }
    r
}

#[test]
fn partition_sum_is_one_across_scene_zoo() {
    // Empty scene (exterior only).
    let empty = wire_scene(16, 16, &[], &[transparent()], &[]);
    let r = assert_partition_invariants(&empty);
    assert!(r.face_coverage[0].iter().all(|v| *v == 1.0));

    // Single rect at fractional offsets.
    assert_partition_invariants(&rect_scene(32, 32, 3.3, 4.7, 21.6, 27.2, red()));
    // Two faces sharing an edge.
    assert_partition_invariants(&shared_edge_scene(48, 32));
    // Donut with a transparent hole face.
    assert_partition_invariants(&donut_scene(48, 48, 8.0, 8.0, 30.0, 9.0));
    // Triple junction.
    assert_partition_invariants(&triple_junction_scene(49, 49));
    // Curves: circle island from two arc boundaries.
    assert_partition_invariants(&circle_scene(32, 32, 16.0, 16.0, 10.0));
    // Off-canvas and straddling islands (ADR-0009 clip policy).
    assert_partition_invariants(&rect_scene(16, 16, 100.0, 3.0, 110.0, 9.0, red()));
    assert_partition_invariants(&rect_scene(16, 16, 10.0, -5.0, 22.0, 9.0, red()));
}

fn circle_scene(w: u32, h: u32, cx: f64, cy: f64, r: f64) -> ValidatedScene {
    use vice_ir::{CurveChain, Segment};
    let vs = [Pt::new(cx - r, cy), Pt::new(cx + r, cy)];
    let arc = |ccw| {
        CurveChain::single(Segment::CircularArc {
            radius_px: r,
            large_arc: false,
            ccw,
        })
    };
    wire_scene(
        w,
        h,
        &vs,
        &[transparent(), red()],
        &[
            BoundarySpec {
                start: 0,
                end: 1,
                left: 1,
                right: 0,
                chain: arc(true),
            },
            BoundarySpec {
                start: 1,
                end: 0,
                left: 1,
                right: 0,
                chain: arc(true),
            },
        ],
    )
}

/// The junction pixel of the triple-junction scene has ANALYTIC area
/// fractions 0.375 / 0.375 / 0.25 (junction at the pixel center, two
/// diagonals through the bottom corners). The composite must be their
/// DIRECT premultiplied sum — and must NOT equal painter-style pairwise
/// alpha compositing of the same fractions.
#[test]
fn triple_junction_fractions_sum_directly() {
    let scene = triple_junction_scene(49, 49);
    let r = assert_partition_invariants(&scene);
    let (w, x, y) = (49usize, 24usize, 24usize);
    let i = y * w + x;

    let a = r.face_coverage[1][i];
    let b = r.face_coverage[2][i];
    let c = r.face_coverage[3][i];
    assert!((a - 0.375).abs() < 1e-12, "sector A fraction {a}");
    assert!((b - 0.375).abs() < 1e-12, "sector B fraction {b}");
    assert!((c - 0.25).abs() < 1e-12, "sector C fraction {c}");
    assert!(
        r.face_coverage[0][i].abs() < 1e-12,
        "exterior 0 at junction"
    );

    // Direct premultiplied sum of the three paints.
    let paints = [
        LinearRgb::new(0.8, 0.1, 0.1),
        LinearRgb::new(0.1, 0.7, 0.2),
        LinearRgb::new(0.1, 0.2, 0.9),
    ];
    let expect = PremulRgba {
        r: a * paints[0].r + b * paints[1].r + c * paints[2].r,
        g: a * paints[0].g + b * paints[1].g + c * paints[2].g,
        b: a * paints[0].b + b * paints[1].b + c * paints[2].b,
        a: a + b + c,
    };
    let got = r.composite[i];
    assert!((got.r - expect.r).abs() < 1e-12);
    assert!((got.g - expect.g).abs() < 1e-12);
    assert!((got.b - expect.b).abs() < 1e-12);
    assert!((got.a - expect.a).abs() < 1e-12);

    // Pairwise painter-style compositing of the SAME fractions gives a
    // different red channel: the direct sum is not sequential alpha math.
    let over = |src: (f64, f64), dst: f64| src.0 + dst * (1.0 - src.1);
    let ab_r = over((b * paints[1].r, b), a * paints[0].r);
    let painter_r = over((c * paints[2].r, c), ab_r);
    assert!(
        (painter_r - expect.r).abs() > 1e-3,
        "painter compositing must differ, else this test proves nothing"
    );
}

/// Composite is exactly Σ coverage·premul(paint) everywhere, and the
/// donut's transparent hole shows alpha 0 (transparent faces contribute
/// nothing — RGB under zero alpha is not invented).
#[test]
fn composite_is_direct_weighted_sum_and_holes_are_transparent() {
    let scene = donut_scene(48, 48, 8.0, 8.0, 30.0, 9.0);
    let r = assert_partition_invariants(&scene);
    let w = r.width_px as usize;
    // Deep inside the ring: opaque red.
    let ring = r.composite[12 * w + 12];
    assert!((ring.a - 1.0).abs() < 1e-12 && (ring.r - 0.8).abs() < 1e-12);
    // Center of the hole: fully transparent, all channels zero.
    let hole = r.composite[23 * w + 23];
    assert_eq!((hole.r, hole.g, hole.b, hole.a), (0.0, 0.0, 0.0, 0.0));
    // Far outside: transparent.
    let out = r.composite[2 * w + 44];
    assert_eq!(out.a, 0.0);
}

// ---------------------------------------------------------------------
// M1-N5 hard checks: B4/B2 scene classes are REJECTED, typed.
// ---------------------------------------------------------------------

/// B4 class: ring wired with reversed orientation (island claims the
/// algebraic-left side but the vertex order walks it negatively). All M1
/// combinatorial invariants hold; the M2 embedding certification rejects.
#[test]
fn reversed_ring_orientation_is_rejected_typed() {
    // Vertices in NEGATIVE algebraic order, but faces claim left=island.
    let vs = [
        Pt::new(4.0, 4.0),
        Pt::new(4.0, 12.0),
        Pt::new(12.0, 12.0),
        Pt::new(12.0, 4.0),
    ];
    let scene = vice_ir::ValidatedScene::new(wire_scene_raw(
        16,
        16,
        &vs,
        &[transparent(), red()],
        &[
            line(0, 1, 1, 0),
            line(1, 2, 1, 0),
            line(2, 3, 1, 0),
            line(3, 0, 1, 0),
        ],
    ))
    .expect("combinatorially valid: M1 validation cannot see orientation");
    match render_partition(&scene, &opts()) {
        // Faces are scanned in id order: the exterior's positive loop is
        // detected first (deterministically).
        Err(RenderError::ExteriorPositiveLoop { .. }) => {}
        other => panic!("expected ExteriorPositiveLoop, got {other:?}"),
    }
}

/// B2 class (REVIEW_M1 §5, scene B2): island 2 drawn geometrically INSIDE
/// island 1 but wired as bordering the exterior. Loop orientations are all
/// consistent, so only the per-pixel range check can catch it — and must.
#[test]
fn island_inside_island_wired_to_exterior_is_rejected_typed() {
    let vs = [
        // Outer island 1.
        Pt::new(4.0, 4.0),
        Pt::new(20.0, 4.0),
        Pt::new(20.0, 20.0),
        Pt::new(4.0, 20.0),
        // Island 2, geometrically inside island 1.
        Pt::new(9.0, 9.0),
        Pt::new(15.0, 9.0),
        Pt::new(15.0, 15.0),
        Pt::new(9.0, 15.0),
    ];
    let scene = vice_ir::ValidatedScene::new(wire_scene_raw(
        24,
        24,
        &vs,
        &[transparent(), red(), blue()],
        &[
            line(0, 1, 1, 0),
            line(1, 2, 1, 0),
            line(2, 3, 1, 0),
            line(3, 0, 1, 0),
            line(4, 5, 2, 0), // island 2 claims the EXTERIOR outside
            line(5, 6, 2, 0),
            line(6, 7, 2, 0),
            line(7, 4, 2, 0),
        ],
    ))
    .expect("combinatorially valid: M1 validation cannot see nesting");
    match render_partition(&scene, &opts()) {
        Err(RenderError::PartitionRangeViolation { face, coverage, .. }) => {
            assert_eq!(face, vice_ir::FaceId(0), "the exterior goes negative");
            assert!(coverage < -0.5, "gross violation, got {coverage}");
        }
        other => panic!("expected PartitionRangeViolation, got {other:?}"),
    }
}

/// Hole ring attached to the WRONG face (exterior instead of the ring):
/// same B2 mechanism inside a donut — rejected by the range check.
#[test]
fn hole_wired_to_wrong_face_is_rejected_typed() {
    let vs = [
        Pt::new(4.0, 4.0),
        Pt::new(20.0, 4.0),
        Pt::new(20.0, 20.0),
        Pt::new(4.0, 20.0),
        Pt::new(9.0, 9.0),
        Pt::new(15.0, 9.0),
        Pt::new(15.0, 15.0),
        Pt::new(9.0, 15.0),
    ];
    let scene = vice_ir::ValidatedScene::new(wire_scene_raw(
        24,
        24,
        &vs,
        &[transparent(), red(), transparent()],
        &[
            line(0, 1, 1, 0),
            line(1, 2, 1, 0),
            line(2, 3, 1, 0),
            line(3, 0, 1, 0),
            // Inner ring: hole face inside, but EXTERIOR claimed outside
            // (correct would be the ring face 1).
            line(4, 5, 2, 0),
            line(5, 6, 2, 0),
            line(6, 7, 2, 0),
            line(7, 4, 2, 0),
        ],
    ))
    .expect("combinatorially valid");
    match render_partition(&scene, &opts()) {
        Err(RenderError::PartitionRangeViolation { .. }) => {}
        other => panic!("expected PartitionRangeViolation, got {other:?}"),
    }
}

/// Orientation certification is honest about its uncertainty: a tiny
/// circle at a COARSE tessellation budget (tolerance beyond the diameter:
/// each semicircle degenerates to one chord) has |signed area| below the
/// certified area budget — typed reject, not a silent guess; the same
/// scene certifies fine at the default budget.
#[test]
fn uncertifiable_orientation_is_a_typed_reject_not_a_guess() {
    let scene = circle_scene(32, 32, 16.0, 16.0, 0.2);
    let coarse = RenderOptions {
        budget: TessellationBudget::with_chord_tolerance_px(0.5).unwrap(),
        ..RenderOptions::default()
    };
    match render_partition(&scene, &coarse) {
        Err(RenderError::UncertifiableLoopOrientation {
            signed_area_px2,
            certified_bound_px2,
            ..
        }) => {
            assert!(signed_area_px2.abs() <= certified_bound_px2);
        }
        other => panic!("expected UncertifiableLoopOrientation, got {other:?}"),
    }
    render_partition(&scene, &opts()).expect("fine budget certifies");
}

#[test]
fn non_box_pixel_filter_is_a_typed_error() {
    let mut raw = rect_scene(16, 16, 4.0, 4.0, 12.0, 12.0, red()).into_inner();
    raw.formation.pixel_filter = PixelFilter::Gaussian { sigma_px: 0.7 };
    let scene = vice_ir::ValidatedScene::new(raw).unwrap();
    match render_partition(&scene, &opts()) {
        Err(RenderError::UnsupportedPixelFilter { .. }) => {}
        other => panic!("expected UnsupportedPixelFilter, got {other:?}"),
    }
}

/// F-0008: geometry outside the proven numeric domain is REFUSED, not
/// silently approximated. Before this, an island at 1e12 rendered and
/// returned numbers whose per-pixel error was bounded by nothing the
/// partition guards could observe (the error is common-mode across faces,
/// so the sum check cancels it bit-for-bit).
#[test]
fn geometry_outside_the_numeric_domain_is_a_typed_refusal() {
    for far in [1.0e7, 1.0e12, 1.0e15] {
        let scene = rect_scene(16, 16, far, 3.0, far + 8.0, 9.0, red());
        match render_partition(&scene, &opts()) {
            Err(RenderError::OutsideNumericDomain { value, limit, .. }) => {
                assert!(value.abs() > limit, "{value} must exceed {limit}");
            }
            other => panic!("expected OutsideNumericDomain at {far}, got {other:?}"),
        }
    }
    // The ROI path enforces the same domain: a certificate must not be
    // obtainable by asking for a window instead of the whole canvas.
    let scene = rect_scene(16, 16, 1.0e12, 3.0, 1.0e12 + 8.0, 9.0, red());
    match vice_render::render_partition_roi(
        &scene,
        &opts(),
        vice_render::PixelRect {
            x0: 0,
            y0: 0,
            x1: 8,
            y1: 8,
        },
    ) {
        Err(RenderError::OutsideNumericDomain { .. }) => {}
        other => panic!("expected OutsideNumericDomain on the ROI path, got {other:?}"),
    }
    // Inside the domain the same shape renders normally.
    let near = rect_scene(16, 16, 40000.0, 3.0, 40008.0, 9.0, red());
    render_partition(&near, &opts()).expect("in-domain geometry renders");
}

/// A caller may widen the domain deliberately — and then gets a WIDER
/// tolerance, derived from the domain rather than silently reused. The
/// trade is explicit in both directions.
///
/// Note the second, independent honesty mechanism this test had to be
/// written around: at astronomical coordinates the shoelace rounding
/// bound exceeds the loop's own area, so `verify_embedding` refuses to
/// certify orientation no matter how wide the numeric domain is. Widening
/// the domain buys a documented tolerance, never a suspended certificate.
#[test]
fn widening_the_domain_widens_the_tolerance_explicitly() {
    let wide = vice_render::NumericDomain::with_max_abs_coord_px(1.0e7).unwrap();
    let opts_wide = RenderOptions {
        domain: wide,
        tolerances: vice_render::PartitionTolerances::for_domain(&wide),
        ..RenderOptions::default()
    };
    assert!(
        opts_wide.tolerances.sum_abs_tol > vice_render::PartitionTolerances::FROZEN_FLOOR,
        "a wider domain must carry a wider, derived tolerance"
    );
    // 1e6 is outside the default domain (65536) but inside the widened one.
    let scene = rect_scene(16, 16, 1.0e6, 3.0, 1.0e6 + 8.0, 9.0, red());
    assert!(matches!(
        render_partition(&scene, &opts()),
        Err(RenderError::OutsideNumericDomain { .. })
    ));
    render_partition(&scene, &opts_wide).expect("explicitly widened domain renders");
}

#[test]
fn oversized_canvas_is_a_typed_resource_error() {
    let scene = rect_scene(1 << 14, 1 << 14, 4.0, 4.0, 12.0, 12.0, red());
    match render_partition(&scene, &opts()) {
        Err(RenderError::CanvasTooLarge { .. }) => {}
        other => panic!("expected CanvasTooLarge, got {other:?}"),
    }
}

#[test]
fn render_digest_is_deterministic_across_repeats() {
    let scene = shared_edge_scene(48, 32);
    let a = render_partition(&scene, &opts()).unwrap();
    let b = render_partition(&scene, &opts()).unwrap();
    assert_eq!(a, b);
    assert_eq!(
        vice_render::render_digest_sha256(&a),
        vice_render::render_digest_sha256(&b)
    );
}

/// Transparent paints must not leak color: a scene whose only island is
/// transparent composites to all-zero even where coverage is 1.
#[test]
fn transparent_interior_face_contributes_no_color() {
    let scene = rect_scene(16, 16, 4.0, 4.0, 12.0, 12.0, Paint::TransparentExterior);
    let r = assert_partition_invariants(&scene);
    let w = r.width_px as usize;
    let inside = r.composite[8 * w + 8];
    assert_eq!(
        (inside.r, inside.g, inside.b, inside.a),
        (0.0, 0.0, 0.0, 0.0)
    );
    assert_eq!(r.face_coverage[1][8 * w + 8], 1.0);
}
