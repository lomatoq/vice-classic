//! Gate tests for the embedding witness `CertifiedMesh` (debt D-4;
//! REVIEW_M2_A M2-A-N8 / REVIEW_M2_B M2-B-N5).
//!
//! The point of the witness is that a NON-RENDERING consumer — the M3 GT
//! loader, identifiability metadata, the scorecard — can demand geometric
//! certification in its signature. These tests establish what the witness
//! does prove, what it deliberately does NOT prove, and that introducing it
//! changed no rendered value.

mod common;

use common::*;
use vice_geom::Pt;
use vice_ir::PixelFilter;
use vice_render::certified::CertifiedMesh;
use vice_render::domain::NumericDomain;
use vice_render::mesh::{RenderMesh, TessellationBudget};
use vice_render::partition::{render_digest_sha256, render_mesh_partition, RenderOptions};
use vice_render::render_error::RenderError;
use vice_render::roi::{render_mesh_partition_roi, PixelRect};
use vice_render::{render_partition, render_partition_roi};

fn opts() -> RenderOptions {
    RenderOptions::default()
}

/// The certificate is what the type says it is: a B4-class scene (loops
/// oriented wrongly) cannot become a `CertifiedMesh` at all.
#[test]
fn a_wrongly_oriented_scene_cannot_be_certified() {
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

    match CertifiedMesh::from_scene(&scene, opts()) {
        Err(RenderError::ExteriorPositiveLoop { .. }) => {}
        other => panic!("expected ExteriorPositiveLoop, got {other:?}"),
    }
}

/// And the certificate does NOT claim more than it proves.
///
/// A B2-class scene (island geometrically inside another, wired to the
/// exterior) has perfectly oriented loops, so it certifies — and the render
/// still refuses it on the per-pixel range check. Asserting this keeps a
/// future reader from treating `CertifiedMesh` as "the faces tile the
/// window", which it is not (ADR-0010: two complementary checks, one
/// geometric and one per-pixel).
#[test]
fn certification_does_not_claim_the_faces_tile_the_window() {
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
    .expect("combinatorially valid: M1 validation cannot see nesting");

    let certified = CertifiedMesh::from_scene(&scene, opts())
        .expect("orientation is fine here - that is the whole point");
    match render_mesh_partition(&certified) {
        Err(RenderError::PartitionRangeViolation { face, .. }) => {
            assert_eq!(face, vice_ir::FaceId(0), "the exterior goes negative");
        }
        other => panic!("expected PartitionRangeViolation, got {other:?}"),
    }
}

/// The witness is bound to the domain it was certified under, so a consumer
/// cannot pair a certificate with tolerances that were never proven.
#[test]
fn the_certificate_carries_the_domain_it_was_proven_on() {
    // Island far outside the default domain (|coord| <= 2^16).
    let scene = rect_scene(16, 16, 100_000.0, 4.0, 100_010.0, 12.0, red());

    match CertifiedMesh::from_scene(&scene, opts()) {
        Err(RenderError::OutsideNumericDomain { .. }) => {}
        other => panic!("expected OutsideNumericDomain, got {other:?}"),
    }

    // Deliberately widened domain: certification succeeds, and the render
    // reads the WIDENED tolerances out of the witness rather than from a
    // separately supplied value.
    let wide = RenderOptions::for_domain(NumericDomain {
        max_abs_coord_px: 1e6,
        max_canvas_dim_px: 1 << 14,
    });
    let certified = CertifiedMesh::from_scene(&scene, wide).expect("inside the widened domain");
    assert!(
        certified.options().tolerances().sum_abs_tol >= opts().tolerances().sum_abs_tol,
        "a wider domain must not silently keep the narrow tolerance"
    );
    render_mesh_partition(&certified).expect("renders under its own certificate");
}

/// Behaviour preservation: routing the renderer through the witness must
/// not have changed a single rendered bit, on both the full and ROI paths.
#[test]
fn certification_changed_no_rendered_value() {
    for scene in [
        rect_scene(24, 24, 3.3, 4.7, 18.1, 15.9, red()),
        donut_scene(32, 32, 4.0, 4.0, 20.0, 5.0),
        triple_junction_scene(24, 24),
        shared_edge_scene(48, 32),
    ] {
        let direct = render_partition(&scene, &opts()).expect("renders");
        let certified = CertifiedMesh::from_scene(&scene, opts()).expect("certifies");
        let through_witness = render_mesh_partition(&certified).expect("renders");
        assert_eq!(
            render_digest_sha256(&direct),
            render_digest_sha256(&through_witness),
            "the witness must be a type-level change only"
        );

        let rect = PixelRect {
            x0: 2,
            y0: 3,
            x1: 14,
            y1: 11,
        };
        let roi_direct = render_partition_roi(&scene, &opts(), rect).expect("roi");
        let roi_witness = render_mesh_partition_roi(&certified, rect).expect("roi");
        assert_eq!(
            roi_direct
                .composite
                .iter()
                .map(|p| (p.r.to_bits(), p.g.to_bits(), p.b.to_bits(), p.a.to_bits()))
                .collect::<Vec<_>>(),
            roi_witness
                .composite
                .iter()
                .map(|p| (p.r.to_bits(), p.g.to_bits(), p.b.to_bits(), p.a.to_bits()))
                .collect::<Vec<_>>(),
            "ROI through the witness must be bitwise identical"
        );
    }
}

/// The scene-level preconditions that used to sit in `render_partition`
/// still fire, and they fire at CERTIFICATION time — i.e. before anything
/// downstream can hold a mesh it should never have got.
#[test]
fn scene_level_refusals_happen_at_certification_time() {
    let mut raw = rect_scene(16, 16, 4.0, 4.0, 12.0, 12.0, red())
        .scene()
        .clone();
    raw.formation.pixel_filter = PixelFilter::Gaussian { sigma_px: 0.6 };
    let scene = vice_ir::ValidatedScene::new(raw).expect("valid scene, unsupported formation");
    match CertifiedMesh::from_scene(&scene, opts()) {
        Err(RenderError::UnsupportedPixelFilter { .. }) => {}
        other => panic!("expected UnsupportedPixelFilter, got {other:?}"),
    }

    // A mesh that blows the resource bound is refused by `certify` too, so
    // the bound is not something only the full render happens to check.
    let big = rect_scene(16, 16, 4.0, 4.0, 12.0, 12.0, red());
    let mut mesh = RenderMesh::build(&big, TessellationBudget::default_m2()).unwrap();
    mesh.width_px = 1 << 15;
    mesh.height_px = 1 << 15;
    match CertifiedMesh::certify(mesh, opts()) {
        Err(RenderError::CanvasTooLarge { .. }) => {}
        other => panic!("expected CanvasTooLarge, got {other:?}"),
    }
}
