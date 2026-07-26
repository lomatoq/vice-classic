//! M2 gate test "ROI render == full render in the ROI window" (spec §16.1
//! ROI with dependency closure, §28 M2). Equality is asserted BITWISE —
//! the closure is exact, not approximate.

mod common;

use common::*;
use vice_ir::ValidatedScene;
use vice_render::{render_partition, render_partition_roi, PixelRect, RenderError, RenderOptions};

fn opts() -> RenderOptions {
    RenderOptions::default()
}

fn assert_roi_equals_full_window(scene: &ValidatedScene, roi: PixelRect) {
    let full = render_partition(scene, &opts()).expect("full render");
    let part = render_partition_roi(scene, &opts(), roi).expect("roi render");
    let w = full.width_px as usize;
    let (rw, rh) = (roi.width() as usize, roi.height() as usize);

    for (f, cov) in part.face_coverage.iter().enumerate() {
        for r in 0..rh {
            for c in 0..rw {
                let full_v = full.face_coverage[f][(roi.y0 as usize + r) * w + roi.x0 as usize + c];
                let roi_v = cov[r * rw + c];
                // BITWISE equality: same pieces, same order, same f64 ops.
                assert!(
                    full_v.to_bits() == roi_v.to_bits(),
                    "face {f} pixel ({c},{r}): {full_v} vs {roi_v}"
                );
            }
        }
    }
    for r in 0..rh {
        for c in 0..rw {
            let fp = full.composite[(roi.y0 as usize + r) * w + roi.x0 as usize + c];
            let rp = part.composite[r * rw + c];
            assert!(
                fp.r.to_bits() == rp.r.to_bits()
                    && fp.g.to_bits() == rp.g.to_bits()
                    && fp.b.to_bits() == rp.b.to_bits()
                    && fp.a.to_bits() == rp.a.to_bits(),
                "composite differs at ({c},{r})"
            );
        }
    }
}

#[test]
fn roi_equals_full_render_window_bitwise() {
    let rect = rect_scene(32, 32, 3.3, 4.7, 21.6, 27.2, red());
    let shared = shared_edge_scene(48, 32);
    let donut = donut_scene(48, 48, 8.0, 8.0, 30.0, 9.0);
    let junction = triple_junction_scene(49, 49);

    // Windows: interior, boundary-crossing, edge-touching, single pixel,
    // full canvas, single row and single column.
    for (scene, rois) in [
        (
            &rect,
            vec![
                PixelRect {
                    x0: 2,
                    y0: 3,
                    x1: 24,
                    y1: 29,
                },
                PixelRect {
                    x0: 0,
                    y0: 0,
                    x1: 32,
                    y1: 32,
                },
                PixelRect {
                    x0: 21,
                    y0: 27,
                    x1: 22,
                    y1: 28,
                },
            ],
        ),
        (
            &shared,
            vec![
                PixelRect {
                    x0: 20,
                    y0: 4,
                    x1: 30,
                    y1: 28,
                }, // across the shared edge
                PixelRect {
                    x0: 0,
                    y0: 15,
                    x1: 48,
                    y1: 16,
                }, // single row
                PixelRect {
                    x0: 24,
                    y0: 0,
                    x1: 25,
                    y1: 32,
                }, // single column
            ],
        ),
        (
            &donut,
            vec![
                PixelRect {
                    x0: 10,
                    y0: 10,
                    x1: 40,
                    y1: 40,
                },
                PixelRect {
                    x0: 20,
                    y0: 20,
                    x1: 28,
                    y1: 28,
                }, // inside the hole
            ],
        ),
        (
            &junction,
            vec![
                PixelRect {
                    x0: 22,
                    y0: 22,
                    x1: 27,
                    y1: 27,
                }, // junction pixel zone
                PixelRect {
                    x0: 0,
                    y0: 24,
                    x1: 49,
                    y1: 25,
                },
            ],
        ),
    ] {
        for roi in rois {
            assert_roi_equals_full_window(scene, roi);
        }
    }
}

/// The vertical dependency closure is real: an ROI band that excludes an
/// island entirely still reproduces the full render's window bitwise
/// (edges outside the band rows are skipped by construction, and that
/// skipping provably cannot change in-band values).
#[test]
fn roi_band_excluding_distant_geometry_is_still_exact() {
    // Two islands far apart vertically.
    let vs = [
        vice_geom::Pt::new(4.0, 2.0),
        vice_geom::Pt::new(12.0, 2.0),
        vice_geom::Pt::new(12.0, 8.0),
        vice_geom::Pt::new(4.0, 8.0),
        vice_geom::Pt::new(4.0, 40.0),
        vice_geom::Pt::new(12.0, 40.0),
        vice_geom::Pt::new(12.0, 46.0),
        vice_geom::Pt::new(4.0, 46.0),
    ];
    let scene = wire_scene(
        16,
        48,
        &vs,
        &[transparent(), red(), blue()],
        &[
            line(0, 1, 1, 0),
            line(1, 2, 1, 0),
            line(2, 3, 1, 0),
            line(3, 0, 1, 0),
            line(4, 5, 2, 0),
            line(5, 6, 2, 0),
            line(6, 7, 2, 0),
            line(7, 4, 2, 0),
        ],
    );
    // Band around the SECOND island only.
    assert_roi_equals_full_window(
        &scene,
        PixelRect {
            x0: 0,
            y0: 38,
            x1: 16,
            y1: 48,
        },
    );
    // Band between the islands: pure exterior.
    let mid = render_partition_roi(
        &scene,
        &opts(),
        PixelRect {
            x0: 0,
            y0: 16,
            x1: 16,
            y1: 32,
        },
    )
    .unwrap();
    assert!(mid.face_coverage[0].iter().all(|v| *v == 1.0));
    assert!(mid.face_coverage[1].iter().all(|v| *v == 0.0));
}

#[test]
fn invalid_roi_is_a_typed_error() {
    let scene = rect_scene(16, 16, 4.0, 4.0, 12.0, 12.0, red());
    for roi in [
        PixelRect {
            x0: 4,
            y0: 4,
            x1: 4,
            y1: 8,
        }, // empty
        PixelRect {
            x0: 8,
            y0: 8,
            x1: 4,
            y1: 12,
        }, // inverted
        PixelRect {
            x0: 0,
            y0: 0,
            x1: 17,
            y1: 16,
        }, // out of bounds
    ] {
        match render_partition_roi(&scene, &opts(), roi) {
            Err(RenderError::RoiInvalid { .. }) => {}
            other => panic!("expected RoiInvalid for {roi:?}, got {other:?}"),
        }
    }
}

/// Certificate scope is explicit: the ROI path still certifies loop
/// orientation globally, and catches partition violations INSIDE the
/// window; violations entirely outside the window are (documented) not
/// observable from that window.
#[test]
fn roi_certificate_scope_is_window_local_for_range_checks() {
    // The B2 nesting violation from partition_gate: inner island wired to
    // the exterior, bad pixels in [9,15)^2.
    let vs = [
        vice_geom::Pt::new(4.0, 4.0),
        vice_geom::Pt::new(20.0, 4.0),
        vice_geom::Pt::new(20.0, 20.0),
        vice_geom::Pt::new(4.0, 20.0),
        vice_geom::Pt::new(9.0, 9.0),
        vice_geom::Pt::new(15.0, 9.0),
        vice_geom::Pt::new(15.0, 15.0),
        vice_geom::Pt::new(9.0, 15.0),
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
            line(4, 5, 2, 0),
            line(5, 6, 2, 0),
            line(6, 7, 2, 0),
            line(7, 4, 2, 0),
        ],
    ))
    .unwrap();
    // Window covering the violation: typed reject.
    match render_partition_roi(
        &scene,
        &opts(),
        PixelRect {
            x0: 8,
            y0: 8,
            x1: 16,
            y1: 16,
        },
    ) {
        Err(RenderError::PartitionRangeViolation { .. }) => {}
        other => panic!("expected PartitionRangeViolation, got {other:?}"),
    }
    // Window fully outside the violation: renders (documented local
    // certificate scope; the FULL render of this scene still rejects, as
    // partition_gate asserts).
    render_partition_roi(
        &scene,
        &opts(),
        PixelRect {
            x0: 0,
            y0: 0,
            x1: 24,
            y1: 8,
        },
    )
    .expect("window-local certificates only");
}
