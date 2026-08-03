//! M10 stroke/line-art lane and fill-vs-stroke model selection.

use std::collections::BTreeSet;

use serde::Serialize;
use vice_image::{CanonicalImage, DecodeLimits, EncodedImageFormat};
use vice_ir::{
    Paint, ResizeChain, Segment, StrokeCap, StrokeJoin, StrokeVertexStyle, ValidatedScene,
    ValidatedStrokeScene,
};
use vice_opt::{CodecLikelihoodError, CodecLikelihoodReport};

use crate::{CoreConfig, VectorizeRequest};

pub const M10_INSPECTION_SCHEMA: &str = "vice-classic/m10-line-art-inspection/v1";
pub const M10_SELECTION_SCHEMA: &str = "vice-classic/m10-model-selection/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum M10ModelKind {
    Fill,
    Stroke,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum M10Decision {
    Fill,
    Stroke,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct M10Inspection {
    pub schema: &'static str,
    pub evidence: vice_evidence::LineArtEvidenceReport,
    pub candidate_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct M10CandidateScore {
    pub model: M10ModelKind,
    pub identity_sha256: String,
    pub cap: Option<StrokeCap>,
    pub join: Option<StrokeJoin>,
    pub codec: CodecLikelihoodReport,
    pub topology_bits: f64,
    pub geometry_bits: f64,
    pub paint_bits: f64,
    pub model_class_bits: f64,
    pub total_bits: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct M10SelectionReport {
    pub schema: &'static str,
    pub source_sha256: String,
    pub source_format: EncodedImageFormat,
    pub evidence: Option<vice_evidence::LineArtEvidenceReport>,
    pub stroke_refusal: Option<String>,
    pub decision: M10Decision,
    pub selected_identity_sha256: String,
    pub selected_total_bits: f64,
    pub runner_up_total_bits: Option<f64>,
    pub margin_bits: Option<f64>,
    pub fill: M10CandidateScore,
    pub strokes: Vec<M10CandidateScore>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct M10Selection {
    pub report: M10SelectionReport,
    pub selected_stroke: Option<ValidatedStrokeScene>,
    pub selected_straight_srgb8: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum M10Error {
    #[error(transparent)]
    Decode(#[from] vice_image::ImageError),
    #[error(transparent)]
    Evidence(#[from] vice_evidence::LineArtRefusal),
    #[error(transparent)]
    StrokeRender(#[from] vice_render::StrokeRenderError),
    #[error(transparent)]
    FillRender(#[from] vice_render::FormationRenderError),
    #[error(transparent)]
    Codec(#[from] CodecLikelihoodError),
    #[error("fill candidate dimensions do not match the source")]
    FillDimensionMismatch,
    #[error("the existing fill lane produced no verified candidate: {detail}")]
    FillCandidateUnavailable { detail: String },
    #[error("the existing fill candidate could not be parsed: {detail}")]
    FillCandidateInvalid { detail: String },
    #[error("M10 candidate inventory is empty or non-finite")]
    InvalidInventory,
}

pub fn inspect_m10_line_art(bytes: &[u8]) -> Result<M10Inspection, M10Error> {
    let image = CanonicalImage::decode(bytes, &DecodeLimits::default())?;
    let proposal = vice_evidence::propose_line_art_strokes(&image)?;
    let mut candidate_ids = proposal
        .candidates
        .iter()
        .map(vice_ir::stroke_scene_digest_sha256)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| M10Error::FillCandidateInvalid {
            detail: error.to_string(),
        })?;
    candidate_ids.sort();
    Ok(M10Inspection {
        schema: M10_INSPECTION_SCHEMA,
        evidence: proposal.report,
        candidate_ids,
    })
}

/// Automatic M10 path: obtain the existing lane's best verified fill witness,
/// then compare it to every stroke style with one common raster objective.
pub fn select_m10_line_art(
    bytes: &[u8],
    request: &VectorizeRequest,
) -> Result<M10Selection, M10Error> {
    let config = CoreConfig::development_for(request.preset);
    let fill_run =
        crate::pipeline::vectorize_for_calibration_without_baseline(bytes, request, &config);
    let selected = fill_run
        .selected
        .ok_or_else(|| M10Error::FillCandidateUnavailable {
            detail: format!("{:?}", fill_run.outcome.report().reason),
        })?;
    let raw_scene = vice_ir::parse_scene(&selected.scene_json).map_err(|error| {
        M10Error::FillCandidateInvalid {
            detail: error.to_string(),
        }
    })?;
    let fill = ValidatedScene::new(raw_scene).map_err(|error| M10Error::FillCandidateInvalid {
        detail: error.to_string(),
    })?;
    select_m10_line_art_against_fill(bytes, &fill)
}

pub fn select_m10_line_art_against_fill(
    bytes: &[u8],
    fill: &ValidatedScene,
) -> Result<M10Selection, M10Error> {
    let image = CanonicalImage::decode(bytes, &DecodeLimits::default())?;
    let fill_render = vice_render::render_partition_formed(
        fill,
        &vice_render::RenderOptions::default(),
        ResizeChain::None,
    )?;
    if fill_render.width_px != image.width_px() || fill_render.height_px != image.height_px() {
        return Err(M10Error::FillDimensionMismatch);
    }
    let fill_pixels = partition_to_srgb8(&fill_render);
    let fill_score = score_fill(&image, fill, &fill_pixels)?;
    let proposal = match vice_evidence::propose_line_art_strokes(&image) {
        Ok(proposal) => proposal,
        Err(error) => {
            return Ok(M10Selection {
                report: M10SelectionReport {
                    schema: M10_SELECTION_SCHEMA,
                    source_sha256: image.source_sha256().into(),
                    source_format: image.encoded_format(),
                    evidence: None,
                    stroke_refusal: Some(error.to_string()),
                    decision: M10Decision::Fill,
                    selected_identity_sha256: fill_score.identity_sha256.clone(),
                    selected_total_bits: fill_score.total_bits,
                    runner_up_total_bits: None,
                    margin_bits: None,
                    fill: fill_score,
                    strokes: Vec::new(),
                },
                selected_stroke: None,
                selected_straight_srgb8: fill_pixels,
            });
        }
    };
    let mut stroke_scenes = Vec::with_capacity(proposal.candidates.len());
    let mut stroke_pixels = Vec::with_capacity(proposal.candidates.len());
    let mut stroke_scores = Vec::with_capacity(proposal.candidates.len());
    for scene in proposal.candidates {
        let render = vice_render::render_stroke_scene(&scene)?;
        let score = score_stroke(&image, &scene, &render.straight_srgb8)?;
        stroke_scenes.push(scene);
        stroke_pixels.push(render.straight_srgb8);
        stroke_scores.push(score);
    }
    if stroke_scores.is_empty()
        || !fill_score.total_bits.is_finite()
        || stroke_scores
            .iter()
            .any(|score| !score.total_bits.is_finite())
    {
        return Err(M10Error::InvalidInventory);
    }
    let best_stroke = (0..stroke_scores.len())
        .min_by(|a, b| {
            stroke_scores[*a]
                .total_bits
                .total_cmp(&stroke_scores[*b].total_bits)
                .then_with(|| {
                    stroke_scores[*a]
                        .identity_sha256
                        .cmp(&stroke_scores[*b].identity_sha256)
                })
        })
        .expect("nonempty inventory");
    let (decision, selected_identity, selected_total, runner_up, selected_stroke, pixels) =
        if stroke_scores[best_stroke].total_bits < fill_score.total_bits {
            (
                M10Decision::Stroke,
                stroke_scores[best_stroke].identity_sha256.clone(),
                stroke_scores[best_stroke].total_bits,
                fill_score.total_bits,
                Some(stroke_scenes[best_stroke].clone()),
                stroke_pixels[best_stroke].clone(),
            )
        } else {
            (
                M10Decision::Fill,
                fill_score.identity_sha256.clone(),
                fill_score.total_bits,
                stroke_scores[best_stroke].total_bits,
                None,
                fill_pixels,
            )
        };
    Ok(M10Selection {
        report: M10SelectionReport {
            schema: M10_SELECTION_SCHEMA,
            source_sha256: image.source_sha256().into(),
            source_format: image.encoded_format(),
            evidence: Some(proposal.report),
            stroke_refusal: None,
            decision,
            selected_identity_sha256: selected_identity,
            selected_total_bits: selected_total,
            runner_up_total_bits: Some(runner_up),
            margin_bits: Some(runner_up - selected_total),
            fill: fill_score,
            strokes: stroke_scores,
        },
        selected_stroke,
        selected_straight_srgb8: pixels,
    })
}

fn score_fill(
    image: &CanonicalImage,
    scene: &ValidatedScene,
    pixels: &[u8],
) -> Result<M10CandidateScore, M10Error> {
    let raw = scene.scene();
    let table = vice_fit::GEOMETRY_CODE_TABLE_V1;
    let canvas = f64::from(raw.canvas.width_px.max(raw.canvas.height_px));
    let geometry_bits = raw.graph.vertices.len() as f64 * table.anchor_bits(canvas)
        + raw
            .graph
            .boundaries
            .iter()
            .map(|boundary| {
                boundary
                    .curve
                    .segments
                    .iter()
                    .map(|segment| segment_code(segment, canvas))
                    .sum::<f64>()
            })
            .sum::<f64>();
    let topology_bits = 1.0
        + log2_factorial(raw.graph.faces.len())
        + log2_factorial(raw.graph.boundaries.len())
        + log2_factorial(raw.graph.vertices.len());
    let paint_bits = unique_paints(raw.graph.faces.iter().map(|face| face.paint)) * 24.0;
    finish_score(
        image,
        pixels,
        M10ModelKind::Fill,
        vice_ir::scene_digest_sha256(raw).map_err(|error| M10Error::FillCandidateInvalid {
            detail: error.to_string(),
        })?,
        None,
        None,
        topology_bits,
        geometry_bits,
        paint_bits,
    )
}

fn score_stroke(
    image: &CanonicalImage,
    scene: &ValidatedStrokeScene,
    pixels: &[u8],
) -> Result<M10CandidateScore, M10Error> {
    let raw = scene.scene();
    let table = vice_fit::GEOMETRY_CODE_TABLE_V1;
    let canvas = f64::from(raw.canvas.width_px.max(raw.canvas.height_px));
    let geometry_bits = raw.vertices.len() as f64 * table.anchor_bits(canvas)
        + raw.edges.len() as f64 * table.coordinate_bits(canvas)
        + raw
            .edges
            .iter()
            .flat_map(|edge| &edge.centerline.segments)
            .map(|segment| segment_code(segment, canvas))
            .sum::<f64>();
    let vertex_symbols = raw
        .vertices
        .iter()
        .map(|vertex| match vertex.style {
            StrokeVertexStyle::Cap(_) | StrokeVertexStyle::Join(_) => 3.0f64.log2(),
            StrokeVertexStyle::Junction(_) => 0.0,
        })
        .sum::<f64>();
    let endpoint_bits = if raw.vertices.len() <= 1 {
        0.0
    } else {
        2.0 * raw.edges.len() as f64 * (raw.vertices.len() as f64).log2()
    };
    let topology_bits = 1.0 + vertex_symbols + endpoint_bits + log2_factorial(raw.edges.len());
    let paint_bits = unique_paints(
        raw.edges
            .iter()
            .map(|edge| edge.paint)
            .chain(std::iter::once(raw.background)),
    ) * 24.0;
    let cap = raw.vertices.iter().find_map(|vertex| match vertex.style {
        StrokeVertexStyle::Cap(cap) => Some(cap),
        _ => None,
    });
    let join = raw.vertices.iter().find_map(|vertex| match vertex.style {
        StrokeVertexStyle::Join(join) => Some(join),
        _ => None,
    });
    finish_score(
        image,
        pixels,
        M10ModelKind::Stroke,
        vice_ir::stroke_scene_digest_sha256(scene).map_err(|error| {
            M10Error::FillCandidateInvalid {
                detail: error.to_string(),
            }
        })?,
        cap,
        join,
        topology_bits,
        geometry_bits,
        paint_bits,
    )
}

#[allow(clippy::too_many_arguments)]
fn finish_score(
    image: &CanonicalImage,
    pixels: &[u8],
    model: M10ModelKind,
    identity_sha256: String,
    cap: Option<StrokeCap>,
    join: Option<StrokeJoin>,
    topology_bits: f64,
    geometry_bits: f64,
    paint_bits: f64,
) -> Result<M10CandidateScore, M10Error> {
    let codec = vice_opt::score_codec_residual(
        image,
        pixels,
        vice_opt::calibrated_codec_likelihood_config(image.encoded_format()),
    )?;
    let model_class_bits = 1.0;
    let total_bits =
        codec.total_bits + topology_bits + geometry_bits + paint_bits + model_class_bits;
    Ok(M10CandidateScore {
        model,
        identity_sha256,
        cap,
        join,
        codec,
        topology_bits,
        geometry_bits,
        paint_bits,
        model_class_bits,
        total_bits,
    })
}

fn segment_code(segment: &Segment, canvas: f64) -> f64 {
    let table = vice_fit::GEOMETRY_CODE_TABLE_V1;
    let coordinate = table.coordinate_bits(canvas);
    table.bits_per_segment_family()
        + match segment {
            Segment::Line => 0.0,
            Segment::CircularArc { .. } => coordinate + 2.0,
            Segment::EllipticArc { .. } => 3.0 * coordinate + 2.0,
            Segment::Quad { .. } => 2.0 * coordinate,
            Segment::Cubic { .. } => 4.0 * coordinate,
        }
}

fn log2_factorial(count: usize) -> f64 {
    (2..=count).map(|value| (value as f64).log2()).sum()
}

fn unique_paints(paints: impl Iterator<Item = Paint>) -> f64 {
    let mut values = BTreeSet::new();
    for paint in paints {
        let key = match paint {
            Paint::TransparentExterior => "transparent".to_string(),
            Paint::OpaqueSolid(rgb) => format!("{:.17}/{:.17}/{:.17}", rgb.r, rgb.g, rgb.b),
        };
        values.insert(key);
    }
    values.len() as f64
}

fn partition_to_srgb8(render: &vice_render::PartitionRender) -> Vec<u8> {
    let mut output = Vec::with_capacity(render.composite.len() * 4);
    for pixel in &render.composite {
        if pixel.a <= 1e-12 {
            output.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            output.extend_from_slice(&[
                vice_ir::color::linear_to_srgb_u8(pixel.r / pixel.a),
                vice_ir::color::linear_to_srgb_u8(pixel.g / pixel.a),
                vice_ir::color::linear_to_srgb_u8(pixel.b / pixel.a),
                (pixel.a.clamp(0.0, 1.0) * 255.0).round() as u8,
            ]);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use vice_geom::Pt;
    use vice_ir::{
        BlendSpace, Boundary, BoundaryId, Canvas, ExteriorModel, Face, FaceId,
        GlobalFormationHypothesis, GraphVertex, HalfEdge, HalfEdgeId, LinearRgb, PixelFilter,
        PlanarGraph, QuantizationModel, VectorScene, VertexId,
    };

    fn rectangle_scene(
        width: u32,
        height: u32,
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
    ) -> ValidatedScene {
        let boundaries = (0..4)
            .map(|index| Boundary {
                left_face: FaceId(1),
                right_face: FaceId(0),
                start_vertex: VertexId(index),
                end_vertex: VertexId((index + 1) % 4),
                closure_join: None,
                curve: vice_ir::CurveChain::single(Segment::Line),
            })
            .collect::<Vec<_>>();
        let next = [2, 7, 4, 1, 6, 3, 0, 5];
        let half_edges = (0..8)
            .map(|index| HalfEdge {
                boundary: BoundaryId(index / 2),
                forward: index % 2 == 0,
                twin: HalfEdgeId(index ^ 1),
                next: HalfEdgeId(next[index as usize]),
                face: FaceId(if index % 2 == 0 { 1 } else { 0 }),
            })
            .collect();
        ValidatedScene::new(VectorScene {
            canvas: Canvas {
                width_px: width,
                height_px: height,
            },
            graph: PlanarGraph {
                exterior: FaceId(0),
                vertices: [
                    Pt::new(x0, y0),
                    Pt::new(x1, y0),
                    Pt::new(x1, y1),
                    Pt::new(x0, y1),
                ]
                .into_iter()
                .map(|pos| GraphVertex { pos })
                .collect(),
                boundaries,
                half_edges,
                faces: vec![
                    Face {
                        loops: vec![HalfEdgeId(1)],
                        paint: Paint::TransparentExterior,
                    },
                    Face {
                        loops: vec![HalfEdgeId(0)],
                        paint: Paint::OpaqueSolid(LinearRgb::new(0.0, 0.0, 0.0)),
                    },
                ],
            },
            formation: GlobalFormationHypothesis {
                blend_space: BlendSpace::LinearLight,
                pixel_filter: PixelFilter::Box,
                quantization: QuantizationModel::Uint8,
                exterior: ExteriorModel::Transparent,
            },
        })
        .unwrap()
    }

    fn rectangle_png(width: u32, height: u32, x0: u32, y0: u32, x1: u32, y1: u32) -> Vec<u8> {
        let mut rgba = vec![0u8; (width * height * 4) as usize];
        for y in y0..y1 {
            for x in x0..x1 {
                let offset = ((y * width + x) * 4) as usize;
                rgba[offset + 3] = 255;
            }
        }
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&rgba).unwrap();
        }
        bytes
    }

    #[test]
    fn a_thin_bar_selects_a_stroke_over_its_exact_fill_outline() {
        let bytes = rectangle_png(32, 16, 4, 7, 28, 10);
        let fill = rectangle_scene(32, 16, 4.0, 7.0, 28.0, 10.0);
        let selected = select_m10_line_art_against_fill(&bytes, &fill).unwrap();
        assert_eq!(selected.report.decision, M10Decision::Stroke);
        assert_eq!(selected.report.strokes.len(), 3);
        assert!(selected.report.margin_bits.unwrap() > 0.0);
        assert!(selected.selected_stroke.is_some());
    }

    #[test]
    fn a_large_solid_region_remains_a_fill() {
        let bytes = rectangle_png(32, 32, 4, 4, 28, 28);
        let fill = rectangle_scene(32, 32, 4.0, 4.0, 28.0, 28.0);
        let selected = select_m10_line_art_against_fill(&bytes, &fill).unwrap();
        assert_eq!(selected.report.decision, M10Decision::Fill);
        assert!(selected.selected_stroke.is_none());
    }
}
