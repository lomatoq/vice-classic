use sha2::{Digest, Sha256};
use vice_geom::Pt;
use vice_ir::{
    BlendSpace, Boundary, BoundaryId, Canvas, ChainNode, CurveChain, ExteriorModel, Face, FaceId,
    GlobalFormationHypothesis, GraphVertex, HalfEdge, HalfEdgeId, JoinKind, LinearRgb, Paint,
    PixelFilter, PlanarGraph, QuantizationModel, Segment, VectorScene, VertexId,
};
use vice_svg::{build_export_plan, materialize_svg, parse_and_render_independently, SvgProfile};

use crate::{
    canvas_closure_sha256, preseal_scene, quantize_and_verify, rebind_scene_bindings,
    seal_delivery, topology_signature_sha256, BoundaryBinding, BoundaryBindingOrigin,
    DeliverySealConfig, DeliverySealError, QuantizationPolicy, VerificationConfig,
    VerificationError,
};

fn square_scene() -> VectorScene {
    let points = [
        Pt::new(2.0, 2.0),
        Pt::new(8.0, 2.0),
        Pt::new(8.0, 8.0),
        Pt::new(2.0, 8.0),
    ];
    let boundaries = (0..4)
        .map(|i| Boundary {
            left_face: FaceId(1),
            right_face: FaceId(0),
            start_vertex: VertexId(i),
            end_vertex: VertexId((i + 1) % 4),
            closure_join: None,
            curve: CurveChain::single(Segment::Line),
        })
        .collect();
    VectorScene {
        canvas: Canvas {
            width_px: 10,
            height_px: 10,
        },
        graph: PlanarGraph {
            exterior: FaceId(0),
            vertices: points.into_iter().map(|pos| GraphVertex { pos }).collect(),
            boundaries,
            half_edges: vec![
                HalfEdge {
                    boundary: BoundaryId(0),
                    forward: true,
                    twin: HalfEdgeId(4),
                    next: HalfEdgeId(1),
                    face: FaceId(1),
                },
                HalfEdge {
                    boundary: BoundaryId(1),
                    forward: true,
                    twin: HalfEdgeId(5),
                    next: HalfEdgeId(2),
                    face: FaceId(1),
                },
                HalfEdge {
                    boundary: BoundaryId(2),
                    forward: true,
                    twin: HalfEdgeId(6),
                    next: HalfEdgeId(3),
                    face: FaceId(1),
                },
                HalfEdge {
                    boundary: BoundaryId(3),
                    forward: true,
                    twin: HalfEdgeId(7),
                    next: HalfEdgeId(0),
                    face: FaceId(1),
                },
                HalfEdge {
                    boundary: BoundaryId(0),
                    forward: false,
                    twin: HalfEdgeId(0),
                    next: HalfEdgeId(7),
                    face: FaceId(0),
                },
                HalfEdge {
                    boundary: BoundaryId(1),
                    forward: false,
                    twin: HalfEdgeId(1),
                    next: HalfEdgeId(4),
                    face: FaceId(0),
                },
                HalfEdge {
                    boundary: BoundaryId(2),
                    forward: false,
                    twin: HalfEdgeId(2),
                    next: HalfEdgeId(5),
                    face: FaceId(0),
                },
                HalfEdge {
                    boundary: BoundaryId(3),
                    forward: false,
                    twin: HalfEdgeId(3),
                    next: HalfEdgeId(6),
                    face: FaceId(0),
                },
            ],
            faces: vec![
                Face {
                    loops: vec![HalfEdgeId(4)],
                    paint: Paint::TransparentExterior,
                },
                Face {
                    loops: vec![HalfEdgeId(0)],
                    paint: Paint::OpaqueSolid(LinearRgb::new(0.8, 0.1, 0.2)),
                },
            ],
        },
        formation: GlobalFormationHypothesis {
            blend_space: BlendSpace::LinearLight,
            pixel_filter: PixelFilter::Box,
            quantization: QuantizationModel::Uint8,
            exterior: ExteriorModel::Transparent,
        },
    }
}

fn config() -> VerificationConfig {
    VerificationConfig {
        render_options: vice_render::RenderOptions::default(),
        max_g1_spread_rad: 1e-9,
        curve_separation_margin_px: 1e-9,
    }
}

#[test]
fn topology_signature_survives_canonical_scene_relabeling() {
    let scene = square_scene();
    let original_bindings = bindings(&scene);
    let before = topology_signature_sha256(&scene).unwrap();
    let bytes = vice_ir::canonical_scene_bytes(&scene).unwrap();
    let roundtrip = vice_ir::parse_scene(&bytes).unwrap();
    assert_eq!(topology_signature_sha256(&roundtrip).unwrap(), before);
    let rebound = rebind_scene_bindings(&roundtrip, &original_bindings, config()).unwrap();
    preseal_scene(&roundtrip, &rebound, config()).unwrap();
}

fn bindings(scene: &VectorScene) -> Vec<BoundaryBinding> {
    let topology = topology_signature_sha256(scene).unwrap();
    (0..scene.graph.boundaries.len())
        .map(|index| {
            let boundary = &scene.graph.boundaries[index];
            BoundaryBinding::new_observed(
                hex::encode(Sha256::digest(format!("chain-{index}"))),
                hex::encode(Sha256::digest(format!("dcel-boundary-{index}"))),
                BoundaryId(index as u32),
                topology.clone(),
                0.1,
                vec![
                    scene.graph.vertices[boundary.start_vertex.index()].pos,
                    scene.graph.vertices[boundary.end_vertex.index()].pos,
                ],
            )
            .unwrap()
        })
        .collect()
}

#[test]
fn every_boundary_identity_survives_preseal_and_quantization() {
    let scene = square_scene();
    let bound = bindings(&scene);
    let pre = preseal_scene(&scene, &bound, config()).unwrap();
    assert_eq!(pre.certificate().observed_chain_bindings, 4);
    let post = quantize_and_verify(
        &scene,
        &bound,
        config(),
        QuantizationPolicy { decimal_places: 6 },
    )
    .unwrap();
    assert_eq!(
        post.pre_quantization_certificate()
            .topology_signature_sha256,
        post.post_quantization_certificate()
            .topology_signature_sha256
    );
    assert_eq!(post.bindings(), bound);
}

#[test]
fn an_exact_canvas_closure_has_a_distinct_non_observed_identity() {
    let canvas = Canvas {
        width_px: 10,
        height_px: 10,
    };
    let mut scene = square_scene();
    scene.graph.vertices = vec![GraphVertex {
        pos: Pt::new(0.0, 0.0),
    }];
    scene.graph.boundaries = vec![Boundary {
        left_face: FaceId(1),
        right_face: FaceId(0),
        start_vertex: VertexId(0),
        end_vertex: VertexId(0),
        closure_join: Some(JoinKind::Corner),
        curve: CurveChain {
            interior_nodes: vec![
                ChainNode {
                    pos: Pt::new(10.0, 0.0),
                    join: JoinKind::Corner,
                },
                ChainNode {
                    pos: Pt::new(10.0, 10.0),
                    join: JoinKind::Corner,
                },
                ChainNode {
                    pos: Pt::new(0.0, 10.0),
                    join: JoinKind::Corner,
                },
            ],
            segments: vec![Segment::Line; 4],
        },
    }];
    scene.graph.half_edges = vec![
        HalfEdge {
            boundary: BoundaryId(0),
            forward: true,
            twin: HalfEdgeId(1),
            next: HalfEdgeId(0),
            face: FaceId(1),
        },
        HalfEdge {
            boundary: BoundaryId(0),
            forward: false,
            twin: HalfEdgeId(0),
            next: HalfEdgeId(1),
            face: FaceId(0),
        },
    ];
    scene.graph.faces[0].loops = vec![HalfEdgeId(1)];
    scene.graph.faces[1].loops = vec![HalfEdgeId(0)];
    let topology = topology_signature_sha256(&scene).unwrap();
    let binding = BoundaryBinding::new_canvas_closure(
        canvas,
        BoundaryId(0),
        topology,
        vec![
            Pt::new(0.0, 0.0),
            Pt::new(10.0, 0.0),
            Pt::new(10.0, 10.0),
            Pt::new(0.0, 10.0),
            Pt::new(0.0, 0.0),
        ],
    )
    .unwrap();
    assert_eq!(
        binding.origin(),
        &BoundaryBindingOrigin::CanvasClosure {
            canvas_sha256: canvas_closure_sha256(canvas)
        }
    );
    let pre = preseal_scene(&scene, &[binding], config()).unwrap();
    assert_eq!(pre.certificate().observed_chain_bindings, 0);
    assert_eq!(pre.certificate().dcel_boundary_bindings, 0);
}

#[test]
fn a_missing_or_stale_chain_binding_is_a_hard_refusal() {
    let scene = square_scene();
    let mut bound = bindings(&scene);
    bound.pop();
    assert!(matches!(
        preseal_scene(&scene, &bound, config()),
        Err(VerificationError::BoundaryBinding)
    ));
}

#[test]
fn a_declared_smooth_join_that_geometry_does_not_read_is_rejected() {
    let mut scene = square_scene();
    scene.graph.boundaries[0].curve = CurveChain {
        interior_nodes: vec![ChainNode {
            pos: Pt::new(5.0, 2.0),
            join: JoinKind::SmoothG1 {
                tangent_angle_rad: std::f64::consts::FRAC_PI_2,
            },
        }],
        segments: vec![Segment::Line, Segment::Line],
    };
    vice_ir::validate_scene(&scene).unwrap();
    assert!(matches!(
        preseal_scene(&scene, &bindings(&scene), config()),
        Err(VerificationError::G1 { .. })
    ));
}

#[test]
fn a_declared_smooth_closure_seam_is_independently_checked() {
    let mut scene = square_scene();
    scene.graph.vertices = vec![GraphVertex {
        pos: Pt::new(5.0, 2.0),
    }];
    scene.graph.boundaries = vec![Boundary {
        left_face: FaceId(1),
        right_face: FaceId(0),
        start_vertex: VertexId(0),
        end_vertex: VertexId(0),
        closure_join: Some(JoinKind::SmoothG1 {
            tangent_angle_rad: std::f64::consts::FRAC_PI_2,
        }),
        curve: CurveChain {
            interior_nodes: vec![ChainNode {
                pos: Pt::new(5.0, 8.0),
                join: JoinKind::Corner,
            }],
            segments: vec![
                Segment::Cubic {
                    ctrl1: Pt::new(8.0, 2.0),
                    ctrl2: Pt::new(8.0, 8.0),
                },
                Segment::Cubic {
                    ctrl1: Pt::new(2.0, 8.0),
                    ctrl2: Pt::new(2.0, 2.0),
                },
            ],
        },
    }];
    scene.graph.half_edges = vec![
        HalfEdge {
            boundary: BoundaryId(0),
            forward: true,
            twin: HalfEdgeId(1),
            next: HalfEdgeId(0),
            face: FaceId(1),
        },
        HalfEdge {
            boundary: BoundaryId(0),
            forward: false,
            twin: HalfEdgeId(0),
            next: HalfEdgeId(1),
            face: FaceId(0),
        },
    ];
    scene.graph.faces[0].loops = vec![HalfEdgeId(1)];
    scene.graph.faces[1].loops = vec![HalfEdgeId(0)];
    vice_ir::validate_scene(&scene).unwrap();
    assert!(matches!(
        preseal_scene(&scene, &bindings(&scene), config()),
        Err(VerificationError::G1 {
            boundary: BoundaryId(0),
            ..
        })
    ));
}

#[test]
fn the_seal_reconstructs_expected_svg_bytes_before_trusting_render_witnesses() {
    let scene = square_scene();
    let quantized = quantize_and_verify(
        &scene,
        &bindings(&scene),
        config(),
        QuantizationPolicy { decimal_places: 6 },
    )
    .unwrap();
    let plan = build_export_plan(quantized.scene(), 6, 0.5).unwrap();
    let pure_bytes = materialize_svg(&plan, SvgProfile::PurePartition).unwrap();
    let seam_bytes = materialize_svg(&plan, SvgProfile::SeamSafe).unwrap();
    let pure = parse_and_render_independently(&pure_bytes).unwrap();
    let seam = parse_and_render_independently(&seam_bytes).unwrap();
    let seal = seal_delivery(
        &quantized,
        &plan,
        &pure,
        &seam,
        DeliverySealConfig {
            max_profile_channel_delta: 0,
            max_profile_mean_channel_delta: 0.0,
            max_internal_channel_delta: 255,
            max_internal_mean_channel_delta: 255.0,
        },
    )
    .unwrap();
    assert_eq!(seal.renderer_ids.len(), 2);
    assert_eq!(seal.profile_comparison.max_channel_delta, 0);

    let forged = String::from_utf8(pure_bytes).unwrap().replace("8 2", "7 2");
    let forged_witness = parse_and_render_independently(forged.as_bytes()).unwrap();
    assert!(matches!(
        seal_delivery(
            &quantized,
            &plan,
            &forged_witness,
            &seam,
            DeliverySealConfig {
                max_profile_channel_delta: 255,
                max_profile_mean_channel_delta: 255.0,
                max_internal_channel_delta: 255,
                max_internal_mean_channel_delta: 255.0,
            }
        ),
        Err(DeliverySealError::SvgBytesMismatch)
    ));
}
