//! M2 gate tests for the seal revalidation skeleton (spec §20.2 in M2
//! scope): validate → canonical bytes → parse → re-validate → render →
//! digest; the cycle is deterministic and the post-parse render is
//! byte-identical across cycles.

mod common;

use common::*;
use vice_ir::ValidatedScene;
use vice_render::{seal_render_cycle, RenderOptions, SealError};

fn opts() -> RenderOptions {
    RenderOptions::default()
}

#[test]
fn seal_cycle_succeeds_and_is_deterministic_across_repeats() {
    for scene in [
        rect_scene(32, 32, 3.3, 4.7, 21.6, 27.2, red()),
        shared_edge_scene(48, 32),
        donut_scene(48, 48, 8.0, 8.0, 30.0, 9.0),
        triple_junction_scene(49, 49),
    ] {
        let raw = scene.into_inner();
        let a = seal_render_cycle(&raw, &opts()).expect("seal cycle");
        let b = seal_render_cycle(&raw, &opts()).expect("seal cycle repeat");
        assert_eq!(a, b, "seal reports must be identical across repeats");
        assert_eq!(a.scene_digest_sha256.len(), 64);
        assert_eq!(a.render_digest_sha256.len(), 64);
        assert!(a.canonical_bytes_len > 2);
    }
}

/// Frozen golden digests (Tier A; additionally expected to reproduce on
/// the Linux CI because the render path of these scenes uses only IEEE
/// +,-,*,/ and correctly-rounded sqrt — no libm transcendentals; see
/// ADR-0008 §8. A CI mismatch is a determinism FINDING, not noise).
///
/// - `shared_edge_scene(48, 32)`: lines only.
/// - `quad_blob_scene`: quadratic + cubic Béziers (sqrt-based counts).
///
/// RE-FROZEN ONCE, in the reviewed commit that made the trapezoid
/// column-local (F-0008 / C032). Both SCENE digests are unchanged — the IR
/// and its canonical bytes were not touched. Of the two RENDER digests only
/// the Bézier one moved, and that split is itself the evidence for what the
/// change does: the lines-only scene is axis-aligned, so every edge takes
/// the vertical fast path where the trapezoid was already exact, while the
/// flattened Bézier scene has sloped edges, which is precisely and only
/// where the arithmetic changed. A change that had moved the lines-only
/// digest would have meant something else was going on.
///
/// - `shared_edge` render digest `1a9feb47…`: UNCHANGED across the fix.
/// - `quad_blob` render digest: `cc6f9615…` → `f14a9102…`.
#[test]
fn golden_render_digests_are_frozen() {
    let shared = shared_edge_scene(48, 32).into_inner();
    let a = seal_render_cycle(&shared, &opts()).unwrap();
    assert_eq!(
        a.scene_digest_sha256,
        "a944335805aae1a89c87b3e66741c48a97b6570a2e1a2ddeb1a6037882f6ea03"
    );
    assert_eq!(
        a.render_digest_sha256,
        "1a9feb47afff462d200fd1ec33fba34c4fb694f093ff64576b15135d30f1fe0e"
    );

    let blob = quad_blob_scene().into_inner();
    let b = seal_render_cycle(&blob, &opts()).unwrap();
    assert_eq!(
        b.scene_digest_sha256,
        "778abab606cde43574fe2a3620ebd593724cf754bd1201743c755df13aef7d12"
    );
    assert_eq!(
        b.render_digest_sha256,
        "f14a91023d5310d9b5477a4d563fb550e70fd77d57893c5122a1800c5c5bb949"
    );
}

fn quad_blob_scene() -> ValidatedScene {
    use vice_geom::Pt;
    use vice_ir::{CurveChain, Segment};
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

/// The seal cycle refuses non-partitions with the same typed errors as
/// the render path (nothing is repaired on the way to a digest).
#[test]
fn seal_cycle_rejects_invalid_scenes_typed() {
    use vice_geom::Pt;
    // The B2 nesting violation.
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
    let raw = wire_scene_raw(
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
    );
    match seal_render_cycle(&raw, &opts()) {
        Err(SealError::Render(vice_render::RenderError::PartitionRangeViolation { .. })) => {}
        other => panic!("expected PartitionRangeViolation, got {other:?}"),
    }

    // A combinatorially invalid scene fails at step 1 with a typed
    // SceneError (never a panic).
    let mut broken = rect_scene(16, 16, 4.0, 4.0, 12.0, 12.0, red()).into_inner();
    broken.graph.half_edges[0].twin = vice_ir::HalfEdgeId(0);
    match seal_render_cycle(&broken, &opts()) {
        Err(SealError::Invalid(_)) => {}
        other => panic!("expected Invalid, got {other:?}"),
    }
}

/// Label-permutation robustness: a NON-canonical labeling of the same
/// scene seals to the SAME scene digest and the same render digest (the
/// cycle renders the canonical form, so labeling cannot leak into the
/// sealed digests).
#[test]
fn seal_digests_are_invariant_under_relabeling() {
    let scene = shared_edge_scene(48, 32).into_inner();
    let a = seal_render_cycle(&scene, &opts()).unwrap();

    // Reverse-permute all entity vectors (same permutation machinery the
    // M1 canonical tests used, re-implemented locally for render scenes).
    let permuted = permute_scene_reversed(&scene);
    assert_ne!(
        scene.graph.vertices, permuted.graph.vertices,
        "the permutation must actually change the labeling"
    );
    let b = seal_render_cycle(&permuted, &opts()).unwrap();
    assert_eq!(a, b, "sealed digests must not depend on labeling");
}

fn permute_scene_reversed(scene: &vice_ir::VectorScene) -> vice_ir::VectorScene {
    use vice_ir::{
        Boundary, BoundaryId, Face, FaceId, GraphVertex, HalfEdge, HalfEdgeId, VertexId,
    };
    let g = &scene.graph;
    let nv = g.vertices.len();
    let nb = g.boundaries.len();
    let nh = g.half_edges.len();
    let nf = g.faces.len();
    // Reversal permutations (exterior stays a valid face id — remapped).
    let pv = |i: usize| nv - 1 - i;
    let pb = |i: usize| nb - 1 - i;
    let ph = |i: usize| nh - 1 - i;
    let pf = |i: usize| nf - 1 - i;

    let mut vertices = vec![
        GraphVertex {
            pos: vice_geom::Pt::ZERO
        };
        nv
    ];
    for (i, v) in g.vertices.iter().enumerate() {
        vertices[pv(i)] = *v;
    }
    let placeholder = Boundary {
        left_face: FaceId(0),
        right_face: FaceId(0),
        start_vertex: VertexId(0),
        end_vertex: VertexId(0),
        closure_join: Some(vice_ir::JoinKind::Corner),
        curve: vice_ir::CurveChain::single(vice_ir::Segment::Line),
    };
    let mut boundaries = vec![placeholder; nb];
    for (i, b) in g.boundaries.iter().enumerate() {
        boundaries[pb(i)] = Boundary {
            left_face: FaceId(pf(b.left_face.index()) as u32),
            right_face: FaceId(pf(b.right_face.index()) as u32),
            start_vertex: VertexId(pv(b.start_vertex.index()) as u32),
            end_vertex: VertexId(pv(b.end_vertex.index()) as u32),
            closure_join: b.closure_join,
            curve: b.curve.clone(),
        };
    }
    let he_placeholder = HalfEdge {
        boundary: BoundaryId(0),
        forward: true,
        twin: HalfEdgeId(0),
        next: HalfEdgeId(0),
        face: FaceId(0),
    };
    let mut half_edges = vec![he_placeholder; nh];
    for (i, h) in g.half_edges.iter().enumerate() {
        half_edges[ph(i)] = HalfEdge {
            boundary: BoundaryId(pb(h.boundary.index()) as u32),
            forward: h.forward,
            twin: HalfEdgeId(ph(h.twin.index()) as u32),
            next: HalfEdgeId(ph(h.next.index()) as u32),
            face: FaceId(pf(h.face.index()) as u32),
        };
    }
    let face_placeholder = Face {
        loops: Vec::new(),
        paint: vice_ir::Paint::TransparentExterior,
    };
    let mut faces = vec![face_placeholder; nf];
    for (i, f) in g.faces.iter().enumerate() {
        faces[pf(i)] = Face {
            loops: f
                .loops
                .iter()
                .map(|l| HalfEdgeId(ph(l.index()) as u32))
                .collect(),
            paint: f.paint,
        };
    }
    vice_ir::VectorScene {
        canvas: scene.canvas,
        graph: vice_ir::PlanarGraph {
            exterior: FaceId(pf(g.exterior.index()) as u32),
            vertices,
            boundaries,
            half_edges,
            faces,
        },
        formation: scene.formation,
    }
}
