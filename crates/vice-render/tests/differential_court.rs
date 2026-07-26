//! Independent differential court (spec §16.3, §28 M2 "multiple
//! rasterizers", "no self-reference-only tests").
//!
//! The internal renderer is judged against TWO independent external
//! rasterizers on a shared scene set:
//!
//! - `tiny-skia` 0.11 (Skia-lineage analytic-AA scanline rasterizer),
//! - `raqote` 0.8 (Firefox-lineage rasterizer, independent codebase),
//!
//! plus a THIRD, fully math-only reference for the disk scene: dense
//! binary supersampling of the analytic disk indicator (no rasterizer
//! code involved at all).
//!
//! Court protocol (ADR-0012): every BOUNDED face of a scene is rendered
//! alone as a white-on-transparent nonzero-winding path built from the
//! SAME fixed tessellation the internal renderer judges (the court
//! compares polygon-coverage claims; the curve→polyline budget is judged
//! separately by the native-curve arm below and by the analytic-area gate
//! in coverage_gate.rs). The reference's 8-bit alpha is compared per
//! pixel; disagreement metrics are TYPED (max |Δ|, mean |Δ|) and asserted
//! against thresholds frozen from measured values with a documented
//! margin (ADR-0012 records the measurements). The exterior face is not
//! path-renderable in one fill; it is machine-checked by the partition
//! sum instead (its coverage is 1 − Σ bounded within 1e-9).
//!
//! AA-model caveat (spec instruction): different rasterizers implement
//! different edge filters; agreement is judged within the documented
//! tolerance, and anything beyond it is a FINDING, not a nuisance to be
//! widened away.

mod common;

use common::*;
use vice_geom::Pt;
use vice_ir::{CurveChain, Segment, ValidatedScene};
use vice_render::{render_partition, PartitionRender, RenderMesh, RenderOptions};

/// Typed disagreement metrics of one court comparison.
#[derive(Debug, Clone, Copy, PartialEq)]
struct CourtMetrics {
    max_abs_delta: f64,
    mean_abs_delta: f64,
    n_pixels: usize,
}

fn metrics(ours: &[f64], reference: &[f64]) -> CourtMetrics {
    assert_eq!(ours.len(), reference.len());
    let mut max_abs = 0.0f64;
    let mut sum_abs = 0.0f64;
    for (a, b) in ours.iter().zip(reference) {
        let d = (a - b).abs();
        max_abs = max_abs.max(d);
        sum_abs += d;
    }
    CourtMetrics {
        max_abs_delta: max_abs,
        mean_abs_delta: sum_abs / ours.len() as f64,
        n_pixels: ours.len(),
    }
}

// ---------------------------------------------------------------------
// Reference renderers (dev-dependencies; their internals are not ours).
// ---------------------------------------------------------------------

/// Face loops → tiny-skia alpha coverage (nonzero winding, analytic AA).
fn tiny_skia_face_coverage(mesh: &RenderMesh, face: usize) -> Vec<f64> {
    let mut pb = tiny_skia::PathBuilder::new();
    for lp in &mesh.face_loops[face] {
        let pts = &lp.points;
        pb.move_to(pts[0].x as f32, pts[0].y as f32);
        for p in &pts[1..pts.len() - 1] {
            pb.line_to(p.x as f32, p.y as f32);
        }
        pb.close();
    }
    let path = pb.finish().expect("path");
    let mut pixmap = tiny_skia::Pixmap::new(mesh.width_px, mesh.height_px).expect("pixmap");
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
fn raqote_face_coverage(mesh: &RenderMesh, face: usize) -> Vec<f64> {
    let mut pb = raqote::PathBuilder::new();
    for lp in &mesh.face_loops[face] {
        let pts = &lp.points;
        pb.move_to(pts[0].x as f32, pts[0].y as f32);
        for p in &pts[1..pts.len() - 1] {
            pb.line_to(p.x as f32, p.y as f32);
        }
        pb.close();
    }
    let path = pb.finish();
    let mut dt = raqote::DrawTarget::new(mesh.width_px as i32, mesh.height_px as i32);
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
// Frozen thresholds. Measured 2026-07-26 on the committed scene set
// (full table in ADR-0012): pixel-aligned scenes agree EXACTLY (0.0 —
// the pixel-grid conventions of all three renderers coincide);
// half-pixel rects deviate only by 8-bit alpha rounding (0.00196);
// fractional/sloped edges show the references' supersampled AA models:
// tiny-skia measured max 0.1275 (consistent with <= 1/8 vertical
// supersampling error), raqote measured max 0.2318 (coarser, <= ~1/4).
// Thresholds are the measured maxima with ~1.3x (max) / ~2x (mean)
// margin; anything beyond is a FINDING, not a tolerance to widen.
// ---------------------------------------------------------------------

const TINY_SKIA_MAX_ABS: f64 = 0.17;
const TINY_SKIA_MEAN_ABS: f64 = 0.012;

const RAQOTE_MAX_ABS: f64 = 0.30;
const RAQOTE_MEAN_ABS: f64 = 0.02;

fn court_scene(name: &str, scene: &ValidatedScene) -> PartitionRender {
    let opts = RenderOptions::default();
    let render = render_partition(scene, &opts).expect("internal render");
    let mesh = RenderMesh::build(scene, opts.budget).expect("mesh");
    let exterior = mesh.exterior.index();
    for face in 0..mesh.face_loops.len() {
        if face == exterior {
            continue; // machine-checked via the partition sum instead
        }
        let ours = &render.face_coverage[face];

        let ts = tiny_skia_face_coverage(&mesh, face);
        let m_ts = metrics(ours, &ts);
        println!("court[{name}] face {face} vs tiny-skia: {m_ts:?}");
        assert!(
            m_ts.max_abs_delta <= TINY_SKIA_MAX_ABS,
            "{name}/f{face} tiny-skia max {}",
            m_ts.max_abs_delta
        );
        assert!(
            m_ts.mean_abs_delta <= TINY_SKIA_MEAN_ABS,
            "{name}/f{face} tiny-skia mean {}",
            m_ts.mean_abs_delta
        );

        let rq = raqote_face_coverage(&mesh, face);
        let m_rq = metrics(ours, &rq);
        println!("court[{name}] face {face} vs raqote:    {m_rq:?}");
        assert!(
            m_rq.max_abs_delta <= RAQOTE_MAX_ABS,
            "{name}/f{face} raqote max {}",
            m_rq.max_abs_delta
        );
        assert!(
            m_rq.mean_abs_delta <= RAQOTE_MEAN_ABS,
            "{name}/f{face} raqote mean {}",
            m_rq.mean_abs_delta
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
            // Positive-signed-area order (the embedding gate rejected the
            // reversed listing of this very fixture — as it must).
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

/// Native-curve arm: tiny-skia renders the ORIGINAL quad/cubic curves
/// (its own flattening, not ours), so the comparison covers our
/// curve→polyline budget too, not only polygon coverage.
#[test]
fn native_curve_court_covers_the_tessellation_budget() {
    let scene = quad_blob_scene();
    let opts = RenderOptions::default();
    let render = render_partition(&scene, &opts).expect("internal render");

    let mut pb = tiny_skia::PathBuilder::new();
    pb.move_to(6.0, 16.0);
    pb.quad_to(16.0, 2.0, 26.0, 16.0);
    pb.cubic_to(28.0, 28.0, 4.0, 28.0, 6.0, 16.0);
    pb.close();
    let path = pb.finish().unwrap();
    let mut pixmap = tiny_skia::Pixmap::new(32, 32).unwrap();
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
    let reference: Vec<f64> = pixmap
        .data()
        .chunks_exact(4)
        .map(|px| f64::from(px[3]) / 255.0)
        .collect();
    let m = metrics(&render.face_coverage[1], &reference);
    println!("court[native_curves] face 1 vs tiny-skia: {m:?}");
    assert!(
        m.max_abs_delta <= TINY_SKIA_MAX_ABS,
        "max {}",
        m.max_abs_delta
    );
    assert!(
        m.mean_abs_delta <= TINY_SKIA_MEAN_ABS,
        "mean {}",
        m.mean_abs_delta
    );
}

/// Math-only reference: dense binary supersampling of the ANALYTIC disk
/// indicator (k² samples per pixel; no rasterizer code at all). Error of
/// the sampled reference itself is O(boundary length / k) per pixel.
#[test]
fn analytic_disk_supersampling_court() {
    let (cx, cy, r) = (16.0, 16.0, 10.0);
    let scene = circle_scene(32, 32, cx, cy, r);
    let render = render_partition(&scene, &RenderOptions::default()).unwrap();

    const K: u32 = 64;
    let mut reference = vec![0.0f64; 32 * 32];
    for py in 0..32u32 {
        for px in 0..32u32 {
            let mut inside = 0u32;
            for sy in 0..K {
                for sx in 0..K {
                    let x = f64::from(px) + (f64::from(sx) + 0.5) / f64::from(K);
                    let y = f64::from(py) + (f64::from(sy) + 0.5) / f64::from(K);
                    let (dx, dy) = (x - cx, y - cy);
                    if dx * dx + dy * dy <= r * r {
                        inside += 1;
                    }
                }
            }
            reference[(py * 32 + px) as usize] = f64::from(inside) / f64::from(K * K);
        }
    }
    let m = metrics(&render.face_coverage[1], &reference);
    println!("court[analytic_disk] face 1 vs {K}x{K} sampling: {m:?}");
    // The sampled reference's own error is O(boundary length / K) per
    // pixel; measured max 0.0123 / mean 0.00056 at K = 64 (ADR-0012),
    // frozen with ~1.6x / ~1.8x margin.
    assert!(m.max_abs_delta <= 0.02, "max {}", m.max_abs_delta);
    assert!(m.mean_abs_delta <= 0.001, "mean {}", m.mean_abs_delta);
}
