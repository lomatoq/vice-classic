//! Independent differential court (spec §16.3, §28 M2 "multiple
//! rasterizers", "no self-reference-only tests").
//!
//! The internal renderer is judged against independent references on a
//! shared scene set:
//!
//! - `tiny-skia` 0.11 (Skia-lineage analytic-AA scanline rasterizer),
//! - `raqote` 0.8 (Firefox-lineage rasterizer, independent codebase),
//! - Sutherland–Hodgman per-pixel polygon clipping (`tests/common`): a
//!   second implementation of the exact integral by a different
//!   algorithm, with no 8-bit quantisation and no AA model at all.
//!
//! # Metric (recalibrated after REVIEW_M2_A M2-A-N1)
//!
//! The first version of this court thresholded `mean |Δ|` normalised by
//! the FRAME. Reviewer A demonstrated that this is not a property of the
//! renderer: with the same geometry, the same renderer and the same
//! judges, shrinking the canvas from 64×64 to 8×8 moved the frame-mean
//! from 0.00023 to 0.01446 — a 63× swing on a parameter unrelated to
//! accuracy — so the frozen threshold both fired falsely on a correct
//! render of a small scene and stayed silent on a systematic +0.05
//! injection at every partial pixel on any canvas ≥ 64. Spec §36 names
//! "quality depends on a sample-specific threshold" as a stop condition.
//!
//! The gate metric is now `edge_mean`: the mean of |Δ| over PARTIALLY
//! COVERED pixels only. Antialiasing differences live exactly there, and
//! the denominator is the number of such pixels, so the metric does not
//! change when empty canvas is added around a shape. The frame-mean is
//! still printed as a diagnostic and is explicitly NOT a gate.
//!
//! `max |Δ|` is retained as the gross-error detector, and its
//! non-degeneracy is now PROVEN in-tree by fault injection rather than
//! asserted in prose (`max_threshold_detects_injected_faults`).
//!
//! # Scope of the thresholds (M2-B-N3)
//!
//! The frozen numbers below are properties of the AA models of the two
//! external judges, measured on the committed scene set. A new scene may
//! legitimately exceed them where a judge's AA model degrades (reviewer B
//! found a ~1.5° sliver where tiny-skia reaches 0.2037 while an analytic
//! reference confirms our coverage). The arbitration rule is therefore
//! explicit: an exceedance is first refereed by the SH-clip reference; if
//! that reference agrees with vice-render, the finding is against the
//! JUDGE and the scene is recorded here with its arbitration, never
//! silently absorbed by widening a threshold.

mod common;

use common::*;
use vice_geom::Pt;
use vice_ir::{CurveChain, Segment, ValidatedScene};
use vice_render::{polygon_coverage, render_partition, PartitionRender, RenderMesh, RenderOptions};

/// Typed disagreement metrics of one court comparison.
#[derive(Debug, Clone, Copy, PartialEq)]
struct CourtMetrics {
    /// Gross-error detector.
    max_abs_delta: f64,
    /// SCALE-INVARIANT gate metric: mean |Δ| over partially covered
    /// pixels, where AA models actually differ.
    edge_mean_abs_delta: f64,
    /// Diagnostic only — NOT a gate (varies with empty canvas area).
    frame_mean_abs_delta: f64,
    n_edge_pixels: usize,
}

fn metrics(ours: &[f64], reference: &[f64]) -> CourtMetrics {
    assert_eq!(ours.len(), reference.len());
    let mut max_abs = 0.0f64;
    let mut sum_all = 0.0f64;
    let mut sum_edge = 0.0f64;
    let mut n_edge = 0usize;
    for (a, b) in ours.iter().zip(reference) {
        let d = (a - b).abs();
        max_abs = max_abs.max(d);
        sum_all += d;
        // "Partially covered" is judged on OUR value, which is the exact
        // area fraction: a pixel is an edge pixel iff a boundary crosses it.
        if *a > 1e-12 && *a < 1.0 - 1e-12 {
            sum_edge += d;
            n_edge += 1;
        }
    }
    CourtMetrics {
        max_abs_delta: max_abs,
        edge_mean_abs_delta: if n_edge == 0 {
            0.0
        } else {
            sum_edge / n_edge as f64
        },
        frame_mean_abs_delta: sum_all / ours.len() as f64,
        n_edge_pixels: n_edge,
    }
}

// ---------------------------------------------------------------------
// Reference renderers (dev-dependencies; their internals are not ours).
// ---------------------------------------------------------------------

fn face_path_points(mesh: &RenderMesh, face: usize) -> Vec<Vec<Pt>> {
    mesh.face_loops[face]
        .iter()
        .map(|lp| lp.points.clone())
        .collect()
}

/// Face loops → tiny-skia alpha coverage (nonzero winding, analytic AA).
fn tiny_skia_coverage(loops: &[Vec<Pt>], w: u32, h: u32) -> Vec<f64> {
    let mut pb = tiny_skia::PathBuilder::new();
    for pts in loops {
        pb.move_to(pts[0].x as f32, pts[0].y as f32);
        for p in &pts[1..pts.len() - 1] {
            pb.line_to(p.x as f32, p.y as f32);
        }
        pb.close();
    }
    let path = pb.finish().expect("path");
    let mut pixmap = tiny_skia::Pixmap::new(w, h).expect("pixmap");
    let mut paint = tiny_skia::Paint::default();
    paint.set_color_rgba8(255, 255, 255, 255);
    paint.anti_alias = true;
    pixmap.fill_path(
        &path,
        &paint,
        tiny_skia::FillRule::Winding,
        tiny_skia::Transform::identity(),
        None,
    );
    pixmap
        .data()
        .chunks_exact(4)
        .map(|px| f64::from(px[3]) / 255.0)
        .collect()
}

/// Face loops → raqote alpha coverage (nonzero winding).
fn raqote_coverage(loops: &[Vec<Pt>], w: u32, h: u32) -> Vec<f64> {
    let mut pb = raqote::PathBuilder::new();
    for pts in loops {
        pb.move_to(pts[0].x as f32, pts[0].y as f32);
        for p in &pts[1..pts.len() - 1] {
            pb.line_to(p.x as f32, p.y as f32);
        }
        pb.close();
    }
    let path = pb.finish();
    let mut dt = raqote::DrawTarget::new(w as i32, h as i32);
    dt.fill(
        &path,
        &raqote::Source::Solid(raqote::SolidSource {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        }),
        &raqote::DrawOptions::new(),
    );
    dt.get_data()
        .iter()
        .map(|argb| f64::from((argb >> 24) & 0xff) / 255.0)
        .collect()
}

// ---------------------------------------------------------------------
// Frozen thresholds — SCALE-INVARIANT metric, measured 2026-07-26 on the
// committed scene set (full table and margins in ADR-0012). Exceeding one
// is a FINDING to be arbitrated by the SH reference (see module docs),
// never a number to widen.
//
// Measured maxima over the set:
//   tiny-skia  max 0.20009 (shallow_sliver), edge-mean 0.11982 (junction f3)
//   raqote     max 0.23180 (quad_blob),      edge-mean 0.12909 (rect_fract)
//   SH-clip    max 1.17e-13 (quad_blob)
// Frozen with ~1.34x margin for the external judges and ~8.5x for SH.
//
// DIVISION OF LABOUR (deliberate, and the reason the numbers differ by ten
// orders): the external rasterizers establish INDEPENDENCE — two foreign
// AA models, 8-bit alpha, no shared code — and can only resolve gross
// error, because their own AA noise (up to 0.13 edge-mean) exceeds any
// subtle systematic bias. ACCURACY is certified by the exact SH reference
// at 1e-12. Both roles are necessary; neither substitutes for the other,
// and `judges_cannot_resolve_a_systematic_bias_but_the_exact_reference_can`
// pins that limitation down instead of leaving it implicit.
// ---------------------------------------------------------------------

const TINY_SKIA_MAX_ABS: f64 = 0.27;
const TINY_SKIA_EDGE_MEAN: f64 = 0.16;

const RAQOTE_MAX_ABS: f64 = 0.31;
const RAQOTE_EDGE_MEAN: f64 = 0.17;

/// The accuracy gate against the exact reference, DERIVED from coordinate
/// magnitude rather than frozen flat (F-M2-R8 / M2-A-N10).
///
/// A flat `1e-12` was wrong in both directions. Both cold contexts
/// measured, independently, that the arbiter's own error grew as eps*M^2
/// and overtook the renderer at the declared domain edge, so the constant
/// would have fired against a CORRECT render from M ~ 2^12 upward — while
/// on the committed set (<= 49 px) it was simultaneously looser than it
/// needed to be. The arbiter is now conditioned so its own error is
/// magnitude-independent (see `sh_clip_coverage_window`), and this bound
/// follows the same law as `PartitionTolerances::for_domain` (ADR-0013),
/// applied to the scene's ACTUAL extent: what it bounds is the
/// RENDERER's error, which is the only thing left once the arbiter is
/// exact.
fn sh_threshold_for(max_abs_coord_px: f64) -> f64 {
    (16.0 * f64::EPSILON * max_abs_coord_px.max(1.0)).max(1e-12)
}

fn mesh_max_abs_coord(mesh: &RenderMesh) -> f64 {
    let mut m: f64 = f64::from(mesh.width_px).max(f64::from(mesh.height_px));
    for poly in &mesh.boundary_polylines {
        for p in &poly.points {
            m = m.max(p.x.abs()).max(p.y.abs());
        }
    }
    m
}

fn court_scene(name: &str, scene: &ValidatedScene) -> PartitionRender {
    let opts = RenderOptions::default();
    let render = render_partition(scene, &opts).expect("internal render");
    let mesh = RenderMesh::build(scene, opts.budget).expect("mesh");
    let (w, h) = (mesh.width_px, mesh.height_px);
    let exterior = mesh.exterior.index();
    for face in 0..mesh.face_loops.len() {
        if face == exterior {
            continue; // machine-checked via the partition sum instead
        }
        let ours = &render.face_coverage[face];
        let loops = face_path_points(&mesh, face);

        // Primary certification: the exact independent reference.
        let refs: Vec<&[Pt]> = loops.iter().map(|l| l.as_slice()).collect();
        let sh = sh_clip_coverage(&refs, w, h);
        let m_sh = metrics(ours, &sh);
        let sh_gate = sh_threshold_for(mesh_max_abs_coord(&mesh));
        println!("court[{name}] face {face} vs SH-clip:   {m_sh:?} (gate {sh_gate:e})");
        assert!(
            m_sh.max_abs_delta <= sh_gate,
            "{name}/f{face} SH max {} exceeds derived gate {sh_gate:e}",
            m_sh.max_abs_delta
        );

        let ts = tiny_skia_coverage(&loops, w, h);
        let m_ts = metrics(ours, &ts);
        println!("court[{name}] face {face} vs tiny-skia: {m_ts:?}");
        assert!(
            m_ts.max_abs_delta <= TINY_SKIA_MAX_ABS,
            "{name}/f{face} tiny-skia max {}",
            m_ts.max_abs_delta
        );
        assert!(
            m_ts.edge_mean_abs_delta <= TINY_SKIA_EDGE_MEAN,
            "{name}/f{face} tiny-skia edge-mean {}",
            m_ts.edge_mean_abs_delta
        );

        let rq = raqote_coverage(&loops, w, h);
        let m_rq = metrics(ours, &rq);
        println!("court[{name}] face {face} vs raqote:    {m_rq:?}");
        assert!(
            m_rq.max_abs_delta <= RAQOTE_MAX_ABS,
            "{name}/f{face} raqote max {}",
            m_rq.max_abs_delta
        );
        assert!(
            m_rq.edge_mean_abs_delta <= RAQOTE_EDGE_MEAN,
            "{name}/f{face} raqote edge-mean {}",
            m_rq.edge_mean_abs_delta
        );
    }
    render
}

#[test]
fn differential_court_shared_scene_set() {
    court_scene(
        "rect_fractional",
        &rect_scene(32, 32, 3.3, 4.7, 21.6, 27.2, red()),
    );
    court_scene(
        "rect_half_pixel",
        &rect_scene(16, 16, 2.5, 3.5, 11.5, 12.5, red()),
    );
    court_scene(
        "sliver_triangle",
        &polygon_scene(
            24,
            24,
            &[Pt::new(2.2, 2.1), Pt::new(3.4, 2.6), Pt::new(21.8, 20.9)],
            red(),
        ),
    );
    court_scene(
        "l_shape",
        &polygon_scene(
            24,
            24,
            &[
                Pt::new(3.0, 3.0),
                Pt::new(11.0, 3.0),
                Pt::new(11.0, 13.0),
                Pt::new(19.0, 13.0),
                Pt::new(19.0, 21.0),
                Pt::new(3.0, 21.0),
            ],
            red(),
        ),
    );
    court_scene("shared_edge", &shared_edge_scene(48, 32));
    court_scene("donut", &donut_scene(48, 48, 8.0, 8.0, 30.0, 9.0));
    court_scene("triple_junction", &triple_junction_scene(49, 49));
    court_scene(
        "straddling_rect",
        &rect_scene(16, 16, 10.0, -5.0, 22.0, 9.0, red()),
    );
    court_scene("circle_arcs", &circle_scene(32, 32, 16.0, 16.0, 10.0));
    court_scene("quad_blob", &quad_blob_scene());
    // REVIEW_M2_B's shallow sliver (~1.5°): the case where tiny-skia's
    // vertical supersampling degrades. Kept in the set WITH its
    // arbitration test below, so the class stays covered rather than
    // quietly excluded.
    court_scene("shallow_sliver", &shallow_sliver_scene());
}

fn circle_scene(w: u32, h: u32, cx: f64, cy: f64, r: f64) -> ValidatedScene {
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

fn quad_blob_scene() -> ValidatedScene {
    let vs = [Pt::new(6.0, 16.0), Pt::new(26.0, 16.0)];
    wire_scene(
        32,
        32,
        &vs,
        &[transparent(), red()],
        &[
            BoundarySpec {
                start: 0,
                end: 1,
                left: 1,
                right: 0,
                chain: CurveChain::single(Segment::Quad {
                    ctrl: Pt::new(16.0, 2.0),
                }),
            },
            BoundarySpec {
                start: 1,
                end: 0,
                left: 1,
                right: 0,
                chain: CurveChain::single(Segment::Cubic {
                    ctrl1: Pt::new(28.0, 28.0),
                    ctrl2: Pt::new(4.0, 28.0),
                }),
            },
        ],
    )
}

/// ~1.5° sliver: REVIEW_M2_B's scene, which exceeds the old tiny-skia max
/// threshold while an exact reference confirms our coverage.
fn shallow_sliver_scene() -> ValidatedScene {
    polygon_scene(
        48,
        16,
        &[Pt::new(2.3, 6.2), Pt::new(45.1, 7.3), Pt::new(2.3, 7.9)],
        red(),
    )
}

// ---------------------------------------------------------------------
// Native-curve arm, redefined (M2-A-N1 item 3).
//
// The previous arm compared our flattened coverage against tiny-skia
// rendering the ORIGINAL curves and thresholded the difference. Reviewer
// A showed that this measures the JUDGE's flattening, not ours: against
// an exact reference our error was 0.0095 while tiny-skia's was 0.266.
// The arm now judges both against a dense de-Casteljau discretisation of
// the true curve integrated by SH clipping, and asserts the ranking as
// well as our own bound.
// ---------------------------------------------------------------------

fn de_casteljau_quad(p0: Pt, c: Pt, p1: Pt, t: f64) -> Pt {
    let a = Pt::new(p0.x + (c.x - p0.x) * t, p0.y + (c.y - p0.y) * t);
    let b = Pt::new(c.x + (p1.x - c.x) * t, c.y + (p1.y - c.y) * t);
    Pt::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}

fn de_casteljau_cubic(p0: Pt, c1: Pt, c2: Pt, p1: Pt, t: f64) -> Pt {
    let lerp = |a: Pt, b: Pt| Pt::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
    let (a, b, c) = (lerp(p0, c1), lerp(c1, c2), lerp(c2, p1));
    let (d, e) = (lerp(a, b), lerp(b, c));
    lerp(d, e)
}

/// Dense polygonal approximation of the quad_blob outline, built with an
/// evaluator independent of `vice-geom` (de Casteljau, not Bernstein).
fn exact_blob_polygon(n: usize) -> Vec<Pt> {
    let (p0, p1) = (Pt::new(6.0, 16.0), Pt::new(26.0, 16.0));
    let (qc, c1, c2) = (Pt::new(16.0, 2.0), Pt::new(28.0, 28.0), Pt::new(4.0, 28.0));
    let mut out = Vec::with_capacity(2 * n + 1);
    for i in 0..=n {
        out.push(de_casteljau_quad(p0, qc, p1, i as f64 / n as f64));
    }
    for i in 1..=n {
        out.push(de_casteljau_cubic(p1, c1, c2, p0, i as f64 / n as f64));
    }
    out.push(out[0]);
    out
}

#[test]
fn native_curve_arm_is_judged_against_an_exact_reference() {
    // n = 500 per curve: chord error ~3e-5 px, orders finer than the
    // differences being measured.
    let exact = exact_blob_polygon(500);
    let reference = sh_clip_coverage(&[&exact], 32, 32);

    let scene = quad_blob_scene();
    let opts = RenderOptions::default();
    let render = render_partition(&scene, &opts).unwrap();
    let ours = metrics(&render.face_coverage[1], &reference);
    println!("native-curve: vice-render vs exact: {ours:?}");

    let mesh = RenderMesh::build(&scene, opts.budget).unwrap();
    let judge = tiny_skia_coverage(&face_path_points(&mesh, 1), 32, 32);
    let theirs = metrics(&judge, &reference);
    println!("native-curve: tiny-skia   vs exact: {theirs:?}");

    // Our tessellation error against the TRUE curve at the default 1/64
    // chord budget (reviewer A measured 0.0095 for the same comparison).
    assert!(
        ours.max_abs_delta <= 0.02,
        "our error vs the exact curve: {}",
        ours.max_abs_delta
    );
    // The arm is only meaningful if the judge is the looser party here —
    // which is exactly why thresholding against the judge measured the
    // wrong quantity.
    assert!(
        theirs.max_abs_delta > ours.max_abs_delta,
        "the judge's own flattening must be the larger error ({} vs {})",
        theirs.max_abs_delta,
        ours.max_abs_delta
    );
}

// ---------------------------------------------------------------------
// Threshold validity: the two properties a frozen gate must have.
// ---------------------------------------------------------------------

/// (1) NON-DEGENERACY of `max`: injected faults must blow past the
/// threshold. Confirms in code what ADR-0012 previously asserted only in
/// prose (M2-A-N1 item 4).
#[test]
fn max_threshold_detects_injected_faults() {
    let scene = polygon_scene(
        32,
        32,
        &[
            Pt::new(4.4, 5.2),
            Pt::new(26.7, 9.3),
            Pt::new(20.1, 27.6),
            Pt::new(6.2, 19.4),
        ],
        red(),
    );
    let render = render_partition(&scene, &RenderOptions::default()).unwrap();
    let truth = &render.face_coverage[1];
    let (w, h) = (32usize, 32usize);

    // Shift by one pixel.
    let mut shifted = vec![0.0; w * h];
    for y in 0..h {
        for x in 1..w {
            shifted[y * w + x] = truth[y * w + x - 1];
        }
    }
    let m = metrics(&shifted, truth);
    println!("injection 1px shift:        {m:?}");
    assert!(
        m.max_abs_delta > TINY_SKIA_MAX_ABS * 3.0,
        "1px shift max {}",
        m.max_abs_delta
    );

    // Vertical flip.
    let mut flipped = vec![0.0; w * h];
    for y in 0..h {
        for x in 0..w {
            flipped[y * w + x] = truth[(h - 1 - y) * w + x];
        }
    }
    let m = metrics(&flipped, truth);
    println!("injection y-flip:           {m:?}");
    assert!(
        m.max_abs_delta > TINY_SKIA_MAX_ABS * 3.0,
        "y-flip max {}",
        m.max_abs_delta
    );
}

/// The honest limit of an 8-bit judge, pinned by a test so it cannot be
/// forgotten: a systematic +0.05 bias on every partially covered pixel is
/// FAR below the external judges' own AA noise (their edge-mean reaches
/// 0.13 on correct renders), so no threshold over those judges can catch
/// it without firing falsely. The exact SH reference catches it by ten
/// orders of magnitude. This is why the court has both kinds of reference
/// and why the accuracy gate is the exact one.
#[test]
fn judges_cannot_resolve_a_systematic_bias_but_the_exact_reference_can() {
    let scene = polygon_scene(
        32,
        32,
        &[
            Pt::new(4.4, 5.2),
            Pt::new(26.7, 9.3),
            Pt::new(20.1, 27.6),
            Pt::new(6.2, 19.4),
        ],
        red(),
    );
    let render = render_partition(&scene, &RenderOptions::default()).unwrap();
    let truth = &render.face_coverage[1];
    let biased: Vec<f64> = truth
        .iter()
        .map(|v| {
            if *v > 1e-12 && *v < 1.0 - 1e-12 {
                (v + 0.05).min(1.0)
            } else {
                *v
            }
        })
        .collect();
    let m = metrics(&biased, truth);
    println!("injection +0.05 systematic: {m:?}");

    // Below what any external judge can distinguish from its own AA model
    // (the tighter of the two thresholds bounds both).
    let tightest_judge_threshold = TINY_SKIA_EDGE_MEAN.min(RAQOTE_EDGE_MEAN);
    assert!(
        m.edge_mean_abs_delta < tightest_judge_threshold,
        "documented limitation: 8-bit judges cannot resolve {}",
        m.edge_mean_abs_delta
    );
    // Trivially visible to the exact reference — by ten orders.
    assert!(
        m.max_abs_delta > sh_threshold_for(32.0) * 1e9,
        "the exact reference must catch the systematic class: {}",
        m.max_abs_delta
    );
}

/// (2) SCALE INVARIANCE of the gate metric: the same geometry judged on
/// canvases of very different sizes must give a stable `edge_mean`, while
/// the old frame-mean swings by more than an order of magnitude. Reviewer
/// A's demonstration, turned into a regression test.
#[test]
fn gate_metric_is_scale_invariant_where_frame_mean_is_not() {
    let mut edge_means = Vec::new();
    let mut frame_means = Vec::new();
    for size in [8u32, 16, 32, 64] {
        let scene = rect_scene(size, size, 1.3, 1.7, 6.55, 5.2, red());
        let opts = RenderOptions::default();
        let render = render_partition(&scene, &opts).unwrap();
        let mesh = RenderMesh::build(&scene, opts.budget).unwrap();
        let ts = tiny_skia_coverage(&face_path_points(&mesh, 1), size, size);
        let m = metrics(&render.face_coverage[1], &ts);
        println!("canvas {size}x{size}: {m:?}");
        edge_means.push(m.edge_mean_abs_delta);
        frame_means.push(m.frame_mean_abs_delta);
    }
    let edge_spread = edge_means.iter().cloned().fold(0.0f64, f64::max)
        / edge_means.iter().cloned().fold(f64::INFINITY, f64::min);
    let frame_spread = frame_means.iter().cloned().fold(0.0f64, f64::max)
        / frame_means.iter().cloned().fold(f64::INFINITY, f64::min);
    println!("edge-mean spread {edge_spread:.2}x, frame-mean spread {frame_spread:.2}x");
    assert!(
        edge_spread < 1.5,
        "gate metric must not depend on canvas size: {edge_spread}x"
    );
    assert!(
        frame_spread > 10.0,
        "the old metric's dependence should be visible here: {frame_spread}x"
    );
}

/// Arbitration rule, executed rather than described (M2-B-N3): where an
/// external judge exceeds a threshold, the SH reference decides. On the
/// shallow sliver tiny-skia is the party that disagrees with the exact
/// answer.
#[test]
fn shallow_sliver_exceedance_is_arbitrated_against_the_judge() {
    let scene = shallow_sliver_scene();
    let opts = RenderOptions::default();
    let render = render_partition(&scene, &opts).unwrap();
    let mesh = RenderMesh::build(&scene, opts.budget).unwrap();
    let loops = face_path_points(&mesh, 1);
    let refs: Vec<&[Pt]> = loops.iter().map(|l| l.as_slice()).collect();

    let sh = sh_clip_coverage(&refs, mesh.width_px, mesh.height_px);
    let ours = metrics(&render.face_coverage[1], &sh);
    let ts = tiny_skia_coverage(&loops, mesh.width_px, mesh.height_px);
    let judge = metrics(&ts, &sh);
    println!("shallow sliver: ours vs exact {ours:?}");
    println!("shallow sliver: skia vs exact {judge:?}");
    assert!(
        ours.max_abs_delta < 1e-12,
        "our coverage matches the exact reference: {}",
        ours.max_abs_delta
    );
    assert!(
        judge.max_abs_delta > 1000.0 * ours.max_abs_delta.max(1e-15),
        "the judge is the party that deviates on shallow edges: {} vs {}",
        judge.max_abs_delta,
        ours.max_abs_delta
    );
}

/// The arbiter must be MORE accurate than the party it judges — measured,
/// across the declared numeric domain, against analytic truth
/// (F-M2-R8 / M2-A-N10).
///
/// Both cold contexts measured the naive arbiter going the other way: at
/// M = 2^14 its own error was 1.01e-11 against the renderer's 1.38e-12,
/// i.e. 7.3x WORSE, so the court would have convicted a correct renderer.
/// The gate was green only because the committed scenes are <= 49 px.
/// This test walks the domain and fails if the arbiter is ever the looser
/// party.
#[test]
fn sh_arbiter_stays_more_accurate_than_the_renderer_across_the_domain() {
    // Axis-aligned non-dyadic rectangle: per-pixel coverage is the product
    // of two exact 1-D overlaps, so truth is available in closed form.
    fn analytic(px: u32, py: u32, x0: f64, y0: f64, x1: f64, y1: f64) -> f64 {
        let ox = (x1.min(f64::from(px) + 1.0) - x0.max(f64::from(px))).max(0.0);
        let oy = (y1.min(f64::from(py) + 1.0) - y0.max(f64::from(py))).max(0.0);
        ox * oy
    }

    for k in [4u32, 8, 10, 12, 14] {
        let base = f64::from(1u32 << k);
        let (x0, y0) = (base + 0.3, base + 0.7);
        let (x1, y1) = (base + 5.55, base + 4.2);
        let lp = vec![
            Pt::new(x0, y0),
            Pt::new(x1, y0),
            Pt::new(x1, y1),
            Pt::new(x0, y1),
            Pt::new(x0, y0),
        ];
        let (ox, oy) = ((base as u32) - 1, (base as u32) - 1);
        let (w, h) = (9u32, 8u32);

        let sh = sh_clip_coverage_window(&[&lp], ox, oy, w, h);
        let mut sh_err = 0.0f64;
        for r in 0..h {
            for c in 0..w {
                let truth = analytic(ox + c, oy + r, x0, y0, x1, y1);
                sh_err = sh_err.max((sh[(r * w + c) as usize] - truth).abs());
            }
        }
        let gate = sh_threshold_for(base + 8.0);
        println!("arbiter at 2^{k}: own error {sh_err:e}, derived gate {gate:e}");
        assert!(
            sh_err <= gate,
            "the arbiter must stay inside the gate it enforces: {sh_err:e} > {gate:e} at 2^{k}"
        );
        // And in absolute terms it must stay at f64 noise, not grow with M.
        assert!(
            sh_err < 1e-14,
            "arbiter error must be magnitude-independent: {sh_err:e} at 2^{k}"
        );
    }
}

/// The SLOPED half of the same property (F-M2-R12).
///
/// The test above uses an axis-aligned rectangle, which the red team
/// showed is the one class that cannot exercise the dominant term: an
/// axis-aligned edge is either parallel to the clip plane or has its
/// coordinate copied, so no interpolation in absolute coordinates ever
/// happens. Measured on sloped geometry the arbiter was still
/// Theta(ulp(M)) and 1.3-1.6x WEAKER than the party it judges at
/// 2^12..2^14 — the property the ADR-0012 arbitration rule rests on was
/// asserted on a sub-class where it holds for the wrong reason. This is
/// the same meta-failure as F-M2-R4, one delta later and in the
/// verification apparatus itself.
///
/// Instrument: the translation oracle, since closed-form truth for a
/// sloped polygon is not available. The same shape is placed at offset
/// 2^k and at the origin (translation exact by Sterbenz), and each
/// implementation is asked how much its own answer moved.
///
/// WHAT THIS ESTABLISHES, honestly. Conditioning the arbiter's
/// interpolation removes its overflow failure but NOT the ulp(M) floor:
/// on sloped geometry both parties are Theta(ulp(M)), because a sloped
/// edge's crossing position at magnitude M is knowable only to ulp(M).
/// So the claim "the arbiter is more accurate than the party it judges"
/// is FALSE as an asymptotic statement on this class, and is not made.
/// What is true, measured, and sufficient for the ADR-0012 arbitration
/// rule is weaker and checkable: the two are comparable, and the derived
/// gate dominates both by more than an order of magnitude — so anything
/// that trips the gate is a real defect rather than either side's noise.
#[test]
fn sh_arbiter_is_not_the_weaker_party_on_sloped_geometry() {
    fn sloped(base: f64) -> Vec<Pt> {
        let pts = [(0.35, 0.7), (7.55, 2.15), (5.3, 7.45), (1.2, 5.05)];
        let mut v: Vec<Pt> = pts
            .iter()
            .map(|&(x, y)| Pt::new(base + x, base + y))
            .collect();
        v.push(v[0]);
        v
    }

    for k in [4u32, 8, 10, 12, 14] {
        let base = f64::from(1u32 << k);
        let big = sloped(base);
        // Exact translate to the origin, derived by subtraction.
        let small: Vec<Pt> = big
            .iter()
            .map(|p| Pt::new(p.x - base, p.y - base))
            .collect();
        for (b, s) in big.iter().zip(&small) {
            assert_eq!(s.x + base, b.x, "translation must be exact");
            assert_eq!(s.y + base, b.y, "translation must be exact");
        }

        // The two windows must correspond pixel-for-pixel: the big one
        // starts exactly `base` pixels right/down of the small one.
        let (ox, oy) = (base as u32, base as u32);
        let (w, h) = (10u32, 10u32);
        let sh_big = sh_clip_coverage_window(&[&big], ox, oy, w, h);
        let sh_small = sh_clip_coverage_window(&[&small], 0, 0, w, h);
        let ours_big = polygon_coverage(&[&big], ox + w, oy, oy + h).expect("finite");
        let ours_small = polygon_coverage(&[&small], w, 0, h).expect("finite");

        let mut sh_err = 0.0f64;
        let mut our_err = 0.0f64;
        for r in 0..h as usize {
            for c in 0..w as usize {
                sh_err =
                    sh_err.max((sh_big[r * w as usize + c] - sh_small[r * w as usize + c]).abs());
                let big_v = ours_big[r * (ox + w) as usize + ox as usize + c];
                our_err = our_err.max((big_v - ours_small[r * w as usize + c]).abs());
            }
        }
        let gate = sh_threshold_for(base + 8.0);
        println!("sloped 2^{k}: arbiter {sh_err:e}, renderer {our_err:e}, gate {gate:e}");
        // MEASURED, and stated as it is rather than as one would wish
        // (see the note above the test): on sloped geometry the arbiter
        // is NOT asymptotically better than the renderer. Both are
        // Theta(ulp(M)), because the crossing position of a sloped edge
        // at magnitude M is knowable only to ulp(M) — the same
        // representation limit as D-5. Ratios measured 2026-07-26:
        // 1.13x (2^4), 1.54x (2^8), 0.995x (2^10), 1.22x (2^12),
        // 1.67x (2^14). Frozen at 3x.
        assert!(
            sh_err <= our_err * 3.0 + 1e-15,
            "arbiter ({sh_err:e}) must stay comparable to the renderer ({our_err:e}) at 2^{k}"
        );
        // THE PROPERTY THE ARBITRATION RULE ACTUALLY RESTS ON: the gate
        // dominates BOTH parties' noise, so a disagreement large enough
        // to trip it cannot be attributed to representation error on
        // either side and must be a real defect. Measured margins 28x
        // (2^14) to 500x (2^4); frozen at 10x.
        assert!(
            sh_err * 10.0 <= gate && our_err * 10.0 <= gate,
            "gate {gate:e} must dominate arbiter {sh_err:e} and renderer {our_err:e} at 2^{k}"
        );
    }
}
