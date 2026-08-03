use vice_geom::Pt;
use vice_ir::{
    BlendSpace, Boundary, Canvas, ChainNode, CurveChain, ExteriorModel, Face, FaceId,
    GlobalFormationHypothesis, GraphVertex, HalfEdge, HalfEdgeId, JoinKind, LinearRgb, Paint,
    PixelFilter, PlanarGraph, QuantizationModel, Segment, VectorScene, VertexId,
};

use crate::{
    build_export_plan, canonical_export_plan_bytes, materialize_svg,
    parse_and_render_independently, IndependentSvgError, SvgProfile,
};

fn closed_square(start: Pt, max: Pt) -> CurveChain {
    CurveChain {
        interior_nodes: vec![
            ChainNode {
                pos: Pt::new(max.x, start.y),
                join: JoinKind::Corner,
            },
            ChainNode {
                pos: max,
                join: JoinKind::Corner,
            },
            ChainNode {
                pos: Pt::new(start.x, max.y),
                join: JoinKind::Corner,
            },
        ],
        segments: vec![Segment::Line; 4],
    }
}

fn nested_scene() -> VectorScene {
    let outer = Boundary {
        left_face: FaceId(1),
        right_face: FaceId(0),
        start_vertex: VertexId(0),
        end_vertex: VertexId(0),
        closure_join: Some(JoinKind::Corner),
        curve: closed_square(Pt::new(1.0, 1.0), Pt::new(9.0, 9.0)),
    };
    let inner = Boundary {
        left_face: FaceId(2),
        right_face: FaceId(1),
        start_vertex: VertexId(1),
        end_vertex: VertexId(1),
        closure_join: Some(JoinKind::Corner),
        curve: closed_square(Pt::new(3.0, 3.0), Pt::new(7.0, 7.0)),
    };
    VectorScene {
        canvas: Canvas {
            width_px: 10,
            height_px: 10,
        },
        graph: PlanarGraph {
            exterior: FaceId(0),
            vertices: vec![
                GraphVertex {
                    pos: Pt::new(1.0, 1.0),
                },
                GraphVertex {
                    pos: Pt::new(3.0, 3.0),
                },
            ],
            boundaries: vec![outer, inner],
            half_edges: vec![
                HalfEdge {
                    boundary: vice_ir::BoundaryId(0),
                    forward: true,
                    twin: HalfEdgeId(1),
                    next: HalfEdgeId(0),
                    face: FaceId(1),
                },
                HalfEdge {
                    boundary: vice_ir::BoundaryId(0),
                    forward: false,
                    twin: HalfEdgeId(0),
                    next: HalfEdgeId(1),
                    face: FaceId(0),
                },
                HalfEdge {
                    boundary: vice_ir::BoundaryId(1),
                    forward: true,
                    twin: HalfEdgeId(3),
                    next: HalfEdgeId(2),
                    face: FaceId(2),
                },
                HalfEdge {
                    boundary: vice_ir::BoundaryId(1),
                    forward: false,
                    twin: HalfEdgeId(2),
                    next: HalfEdgeId(3),
                    face: FaceId(1),
                },
            ],
            faces: vec![
                Face {
                    loops: vec![HalfEdgeId(1)],
                    paint: Paint::TransparentExterior,
                },
                Face {
                    loops: vec![HalfEdgeId(0), HalfEdgeId(3)],
                    paint: Paint::OpaqueSolid(LinearRgb::new(1.0, 0.0, 0.0)),
                },
                Face {
                    loops: vec![HalfEdgeId(2)],
                    paint: Paint::OpaqueSolid(LinearRgb::new(0.0, 0.0, 1.0)),
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

fn three_paint_nested_scene() -> VectorScene {
    let mut scene = nested_scene();
    scene.graph.vertices.push(GraphVertex {
        pos: Pt::new(4.0, 4.0),
    });
    scene.graph.boundaries.push(Boundary {
        left_face: FaceId(3),
        right_face: FaceId(2),
        start_vertex: VertexId(2),
        end_vertex: VertexId(2),
        closure_join: Some(JoinKind::Corner),
        curve: closed_square(Pt::new(4.0, 4.0), Pt::new(6.0, 6.0)),
    });
    scene.graph.half_edges.extend([
        HalfEdge {
            boundary: vice_ir::BoundaryId(2),
            forward: true,
            twin: HalfEdgeId(5),
            next: HalfEdgeId(4),
            face: FaceId(3),
        },
        HalfEdge {
            boundary: vice_ir::BoundaryId(2),
            forward: false,
            twin: HalfEdgeId(4),
            next: HalfEdgeId(5),
            face: FaceId(2),
        },
    ]);
    scene.graph.faces[2].loops.push(HalfEdgeId(5));
    scene.graph.faces.push(Face {
        loops: vec![HalfEdgeId(4)],
        paint: Paint::OpaqueSolid(LinearRgb::new(0.0, 1.0, 0.0)),
    });
    scene
}

#[test]
fn both_profiles_share_one_canonical_plan_and_independently_render() {
    let scene = nested_scene();
    vice_ir::validate_scene(&scene).unwrap();
    let plan = build_export_plan(&scene, 6, 0.5).unwrap();
    assert_eq!(plan.faces().len(), 2);
    assert_eq!(plan.aprons().len(), 1);
    let bytes = canonical_export_plan_bytes(&plan).unwrap();
    let parsed: crate::ExportPlan = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(parsed, plan);

    let pure_bytes = materialize_svg(&plan, SvgProfile::PurePartition).unwrap();
    let seam_bytes = materialize_svg(&plan, SvgProfile::SeamSafe).unwrap();
    let pure = parse_and_render_independently(&pure_bytes).unwrap();
    let seam = parse_and_render_independently(&seam_bytes).unwrap();
    assert_eq!(pure.apron_paths(), 0);
    assert_eq!(seam.apron_paths(), 1);
    assert_eq!(pure.scene_digest_sha256(), seam.scene_digest_sha256());
    assert!(!pure.png_bytes().is_empty());
    assert!(!seam.png_bytes().is_empty());
}

#[test]
fn multicolor_faces_share_one_plan_and_both_svg_profiles_render_independently() {
    let scene = three_paint_nested_scene();
    vice_ir::validate_scene(&scene).unwrap();
    let plan = build_export_plan(&scene, 6, 0.5).unwrap();
    assert_eq!(plan.faces().len(), 3);
    assert_eq!(
        plan.faces()
            .iter()
            .map(|face| face.fill_srgb8())
            .collect::<Vec<_>>(),
        vec!["#ff0000", "#0000ff", "#00ff00"]
    );
    let pure = materialize_svg(&plan, SvgProfile::PurePartition).unwrap();
    let seam = materialize_svg(&plan, SvgProfile::SeamSafe).unwrap();
    let pure_render = parse_and_render_independently(&pure).unwrap();
    let seam_render = parse_and_render_independently(&seam).unwrap();
    assert_eq!(
        pure_render.scene_digest_sha256(),
        seam_render.scene_digest_sha256()
    );
}

#[test]
fn an_apron_is_never_put_on_the_exterior_boundary() {
    let plan = build_export_plan(&nested_scene(), 6, 0.5).unwrap();
    assert_eq!(plan.aprons().len(), 1);
    assert_eq!(plan.aprons()[0].boundary_id(), 1);
}

#[test]
fn independent_xml_contract_rejects_a_forged_apron_count() {
    let plan = build_export_plan(&nested_scene(), 6, 0.5).unwrap();
    let bytes = materialize_svg(&plan, SvgProfile::SeamSafe).unwrap();
    let text = String::from_utf8(bytes)
        .unwrap()
        .replace("data-vice-aprons=\"1\"", "data-vice-aprons=\"0\"");
    assert!(matches!(
        parse_and_render_independently(text.as_bytes()),
        Err(IndependentSvgError::ProfileContract)
    ));
}
