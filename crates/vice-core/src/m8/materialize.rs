//! Lower a shared multicolour grid DCEL into the canonical production IR.

use std::collections::BTreeMap;

use sha2::Digest;
use vice_geom::Pt;
use vice_ir::{
    BlendSpace, Boundary, BoundaryId, Canvas, CurveChain, ExteriorModel, Face, FaceId,
    GlobalFormationHypothesis, GraphVertex, HalfEdge, HalfEdgeId, LinearRgb, Paint, PixelFilter,
    PlanarGraph, QuantizationModel, Segment, ValidatedScene, VectorScene, VertexId,
};
use vice_verify::{topology_signature_sha256, BoundaryBinding};

use super::MultiregionSeed;

#[derive(Debug, thiserror::Error)]
pub enum MultiregionMaterializeError {
    #[error("unknown M8 blend-space id {0}")]
    UnknownBlendSpace(String),
    #[error("the multicolour DCEL exterior is not canonical face zero")]
    ExteriorNotCanonical,
    #[error("visible face {face:?} has no fitted opaque paint")]
    MissingPaint { face: FaceId },
    #[error("a multicolour face loop is empty")]
    EmptyLoop,
    #[error("materialized M8 scene is invalid: {0}")]
    InvalidScene(#[from] vice_ir::SceneError),
    #[error("M8 boundary binding failed: {0}")]
    Binding(#[from] vice_verify::VerificationError),
}

pub fn multiregion_boundary_bindings(
    seed: &MultiregionSeed,
    scene: &VectorScene,
) -> Result<Vec<BoundaryBinding>, MultiregionMaterializeError> {
    if seed.dcel.boundaries.len() != scene.graph.boundaries.len() {
        return Err(MultiregionMaterializeError::ExteriorNotCanonical);
    }
    let topology = topology_signature_sha256(scene)?;
    let mut bindings = Vec::with_capacity(seed.dcel.boundaries.len());
    for boundary in &seed.dcel.boundaries {
        let ir_boundary = BoundaryId(boundary.id.0);
        let support = vec![
            Pt::new(f64::from(boundary.start.0), f64::from(boundary.start.1)),
            Pt::new(f64::from(boundary.end.0), f64::from(boundary.end.1)),
        ];
        let material = format!(
            "{}\u{1f}{}\u{1f}{}\u{1f}{:?}\u{1f}{:?}\u{1f}{}",
            seed.dcel.rag_sha256,
            boundary.id.0,
            boundary.left.0,
            boundary.start,
            boundary.end,
            boundary.right.0
        );
        let dcel_digest = hex::encode(sha2::Sha256::digest(material.as_bytes()));
        if same_canvas_edge(scene.canvas, support[0], support[1]) {
            bindings.push(BoundaryBinding::new_canvas_segment(
                scene.canvas,
                dcel_digest,
                ir_boundary,
                topology.clone(),
                support,
            )?);
        } else {
            let observed_digest = hex::encode(sha2::Sha256::digest(
                format!("M8/observed-chain/v1\u{1f}{material}").as_bytes(),
            ));
            bindings.push(BoundaryBinding::new_observed(
                observed_digest,
                dcel_digest,
                ir_boundary,
                topology.clone(),
                1.0,
                support,
            )?);
        }
    }
    Ok(bindings)
}

fn same_canvas_edge(canvas: Canvas, a: Pt, b: Pt) -> bool {
    let width = f64::from(canvas.width_px);
    let height = f64::from(canvas.height_px);
    (a.x == 0.0 && b.x == 0.0)
        || (a.x == width && b.x == width)
        || (a.y == 0.0 && b.y == 0.0)
        || (a.y == height && b.y == height)
}

pub fn materialize_multiregion_seed(
    seed: &MultiregionSeed,
) -> Result<ValidatedScene, MultiregionMaterializeError> {
    let exterior = seed
        .rag
        .exterior
        .ok_or(MultiregionMaterializeError::ExteriorNotCanonical)?;
    if exterior.0 != 0 || !seed.dcel.faces.first().is_some_and(|face| face.is_exterior) {
        return Err(MultiregionMaterializeError::ExteriorNotCanonical);
    }

    let lattice_vertices = seed
        .dcel
        .boundaries
        .iter()
        .flat_map(|boundary| [boundary.start, boundary.end])
        .collect::<std::collections::BTreeSet<_>>();
    let vertex_ids = lattice_vertices
        .iter()
        .enumerate()
        .map(|(index, point)| (*point, VertexId(index as u32)))
        .collect::<BTreeMap<_, _>>();
    let vertices = lattice_vertices
        .iter()
        .map(|&(x, y)| GraphVertex {
            pos: Pt::new(f64::from(x), f64::from(y)),
        })
        .collect::<Vec<_>>();

    let boundaries = seed
        .dcel
        .boundaries
        .iter()
        .map(|boundary| Boundary {
            left_face: FaceId(boundary.left.0),
            right_face: FaceId(boundary.right.0),
            start_vertex: vertex_ids[&boundary.start],
            end_vertex: vertex_ids[&boundary.end],
            closure_join: None,
            curve: CurveChain::single(Segment::Line),
        })
        .collect::<Vec<_>>();

    let mut half_edges = seed
        .dcel
        .boundaries
        .iter()
        .flat_map(|boundary| {
            let forward = HalfEdgeId(boundary.id.0 * 2);
            let reverse = HalfEdgeId(boundary.id.0 * 2 + 1);
            [
                HalfEdge {
                    boundary: BoundaryId(boundary.id.0),
                    forward: true,
                    twin: reverse,
                    next: forward,
                    face: FaceId(boundary.left.0),
                },
                HalfEdge {
                    boundary: BoundaryId(boundary.id.0),
                    forward: false,
                    twin: forward,
                    next: reverse,
                    face: FaceId(boundary.right.0),
                },
            ]
        })
        .collect::<Vec<_>>();

    let fitted_paints = seed
        .paint_fit
        .paints
        .iter()
        .map(|paint| (paint.face, paint.quantized_srgb8))
        .collect::<BTreeMap<_, _>>();
    let mut faces = Vec::with_capacity(seed.dcel.faces.len());
    for face in &seed.dcel.faces {
        let mut representatives = Vec::with_capacity(face.loops.len());
        for lp in &face.loops {
            let first = *lp.first().ok_or(MultiregionMaterializeError::EmptyLoop)?;
            representatives.push(HalfEdgeId(first.0));
            for index in 0..lp.len() {
                let current = HalfEdgeId(lp[index].0);
                let next = HalfEdgeId(lp[(index + 1) % lp.len()].0);
                half_edges[current.index()].next = next;
            }
        }
        let ir_face = FaceId(face.region.0);
        let paint = if face.is_exterior {
            Paint::TransparentExterior
        } else {
            let rgb = fitted_paints
                .get(&ir_face)
                .copied()
                .ok_or(MultiregionMaterializeError::MissingPaint { face: ir_face })?;
            Paint::OpaqueSolid(LinearRgb::new(
                vice_ir::color::srgb_u8_to_linear(rgb[0]),
                vice_ir::color::srgb_u8_to_linear(rgb[1]),
                vice_ir::color::srgb_u8_to_linear(rgb[2]),
            ))
        };
        faces.push(Face {
            loops: representatives,
            paint,
        });
    }

    let blend_space = match seed.blend_space {
        "linear_light" => BlendSpace::LinearLight,
        "encoded_srgb" => BlendSpace::EncodedSrgb,
        other => {
            return Err(MultiregionMaterializeError::UnknownBlendSpace(
                other.to_string(),
            ))
        }
    };
    let scene = VectorScene {
        canvas: Canvas {
            width_px: seed.rag.width_px as u32,
            height_px: seed.rag.height_px as u32,
        },
        graph: PlanarGraph {
            exterior: FaceId(0),
            vertices,
            boundaries,
            half_edges,
            faces,
        },
        formation: GlobalFormationHypothesis {
            blend_space,
            pixel_filter: PixelFilter::Box,
            quantization: QuantizationModel::Uint8,
            exterior: if seed.exterior_is_transparent {
                ExteriorModel::Transparent
            } else {
                ExteriorModel::Opaque
            },
        },
    };
    ValidatedScene::new(scene).map_err(MultiregionMaterializeError::from)
}
