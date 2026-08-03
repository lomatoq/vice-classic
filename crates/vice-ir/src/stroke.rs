//! M10 centerline graph IR for line art.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use vice_geom::Pt;

use crate::{Canvas, CurveChain, Paint, Segment};

pub const STROKE_SCENE_SCHEMA: &str = "vice-classic/stroke-scene/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StrokeVertexId(pub u32);

impl StrokeVertexId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StrokeEdgeId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum StrokeCap {
    Butt,
    Round,
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum StrokeJoin {
    Miter { limit: f64 },
    Round,
    Bevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum StrokeJunction {
    /// One shared round hub, not overlapping independently capped paths.
    Round,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum StrokeVertexStyle {
    Cap(StrokeCap),
    Join(StrokeJoin),
    Junction(StrokeJunction),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrokeVertex {
    pub position: Pt,
    pub style: StrokeVertexStyle,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrokeEdge {
    pub id: StrokeEdgeId,
    pub start: StrokeVertexId,
    pub end: StrokeVertexId,
    pub centerline: CurveChain,
    /// Constant physical width in render pixels. Width variation is not
    /// silently approximated by this first M10 model version.
    pub width_px: f64,
    pub paint: Paint,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrokeScene {
    pub canvas: Canvas,
    pub vertices: Vec<StrokeVertex>,
    pub edges: Vec<StrokeEdge>,
    pub background: Paint,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ValidatedStrokeScene(StrokeScene);

impl ValidatedStrokeScene {
    pub fn new(scene: StrokeScene) -> Result<Self, StrokeIrError> {
        validate(&scene)?;
        Ok(Self(scene))
    }

    pub fn scene(&self) -> &StrokeScene {
        &self.0
    }

    pub fn into_inner(self) -> StrokeScene {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum StrokeIrError {
    #[error("stroke canvas has zero extent")]
    EmptyCanvas,
    #[error("stroke scene has no edges")]
    EmptyGraph,
    #[error("stroke vertex {vertex} has a non-finite position")]
    NonFiniteVertex { vertex: usize },
    #[error("stroke edge ids must equal their canonical array positions")]
    NonCanonicalEdgeIds,
    #[error("stroke edge {edge} references missing vertex {vertex}")]
    MissingVertex { edge: usize, vertex: u32 },
    #[error("stroke edge {edge} has invalid width {width_px}")]
    InvalidWidth { edge: usize, width_px: f64 },
    #[error("stroke edge {edge} must use opaque finite paint")]
    InvalidPaint { edge: usize },
    #[error("stroke background must be transparent or opaque finite paint")]
    InvalidBackground,
    #[error("stroke edge {edge} has malformed centerline arity")]
    MalformedCenterline { edge: usize },
    #[error("stroke edge {edge} centerline parameter {parameter} is invalid")]
    InvalidCenterline { edge: usize, parameter: String },
    #[error("stroke vertex {vertex} degree {degree} disagrees with style {style}")]
    StyleDegree {
        vertex: usize,
        degree: usize,
        style: &'static str,
    },
    #[error("stroke vertex {vertex} has invalid miter limit {limit}")]
    InvalidMiter { vertex: usize, limit: f64 },
    #[error("stroke vertex {vertex} joins edges with different paint or width")]
    IncompatibleIncidentEdges { vertex: usize },
    #[error("stroke serialization failed: {detail}")]
    Serialization { detail: String },
}

fn validate(scene: &StrokeScene) -> Result<(), StrokeIrError> {
    if scene.canvas.width_px == 0 || scene.canvas.height_px == 0 {
        return Err(StrokeIrError::EmptyCanvas);
    }
    if scene.edges.is_empty() {
        return Err(StrokeIrError::EmptyGraph);
    }
    for (vertex, value) in scene.vertices.iter().enumerate() {
        if !value.position.is_finite() || value.position.has_negative_zero() {
            return Err(StrokeIrError::NonFiniteVertex { vertex });
        }
        if let StrokeVertexStyle::Join(StrokeJoin::Miter { limit }) = value.style {
            if !limit.is_finite() || limit < 1.0 {
                return Err(StrokeIrError::InvalidMiter { vertex, limit });
            }
        }
    }
    validate_paint(scene.background).ok_or(StrokeIrError::InvalidBackground)?;
    let mut degree = vec![0usize; scene.vertices.len()];
    let mut incident = vec![Vec::<usize>::new(); scene.vertices.len()];
    for (edge_index, edge) in scene.edges.iter().enumerate() {
        if edge.id.0 as usize != edge_index {
            return Err(StrokeIrError::NonCanonicalEdgeIds);
        }
        for id in [edge.start, edge.end] {
            if id.index() >= scene.vertices.len() {
                return Err(StrokeIrError::MissingVertex {
                    edge: edge_index,
                    vertex: id.0,
                });
            }
        }
        if !edge.width_px.is_finite() || edge.width_px <= 0.0 {
            return Err(StrokeIrError::InvalidWidth {
                edge: edge_index,
                width_px: edge.width_px,
            });
        }
        if !matches!(edge.paint, Paint::OpaqueSolid(_)) || validate_paint(edge.paint).is_none() {
            return Err(StrokeIrError::InvalidPaint { edge: edge_index });
        }
        validate_centerline(edge_index, &edge.centerline)?;
        if edge.start == edge.end {
            degree[edge.start.index()] += 2;
            incident[edge.start.index()].extend([edge_index, edge_index]);
        } else {
            degree[edge.start.index()] += 1;
            degree[edge.end.index()] += 1;
            incident[edge.start.index()].push(edge_index);
            incident[edge.end.index()].push(edge_index);
        }
    }
    for (vertex, (value, degree)) in scene.vertices.iter().zip(degree).enumerate() {
        let valid = matches!(
            (degree, value.style),
            (1, StrokeVertexStyle::Cap(_))
                | (2, StrokeVertexStyle::Join(_))
                | (3.., StrokeVertexStyle::Junction(_))
        );
        if !valid {
            let style = match value.style {
                StrokeVertexStyle::Cap(_) => "cap",
                StrokeVertexStyle::Join(_) => "join",
                StrokeVertexStyle::Junction(_) => "junction",
            };
            return Err(StrokeIrError::StyleDegree {
                vertex,
                degree,
                style,
            });
        }
        if degree >= 2 {
            let first = &scene.edges[incident[vertex][0]];
            if incident[vertex].iter().any(|edge| {
                scene.edges[*edge].paint != first.paint
                    || scene.edges[*edge].width_px != first.width_px
            }) {
                return Err(StrokeIrError::IncompatibleIncidentEdges { vertex });
            }
        }
    }
    Ok(())
}

fn validate_paint(paint: Paint) -> Option<()> {
    match paint {
        Paint::TransparentExterior => Some(()),
        Paint::OpaqueSolid(rgb)
            if rgb
                .components()
                .iter()
                .all(|value| value.is_finite() && (0.0..=1.0).contains(value)) =>
        {
            Some(())
        }
        Paint::OpaqueSolid(_) => None,
    }
}

fn validate_centerline(edge: usize, chain: &CurveChain) -> Result<(), StrokeIrError> {
    if chain.segments.is_empty() || chain.interior_nodes.len() + 1 != chain.segments.len() {
        return Err(StrokeIrError::MalformedCenterline { edge });
    }
    for (index, node) in chain.interior_nodes.iter().enumerate() {
        if !node.pos.is_finite() || node.pos.has_negative_zero() {
            return Err(StrokeIrError::InvalidCenterline {
                edge,
                parameter: format!("node[{index}]"),
            });
        }
    }
    for (index, segment) in chain.segments.iter().enumerate() {
        let valid = match segment {
            Segment::Line => true,
            Segment::CircularArc { radius_px, .. } => radius_px.is_finite() && *radius_px > 0.0,
            Segment::EllipticArc {
                rx_px,
                ry_px,
                x_axis_rotation_rad,
                ..
            } => {
                rx_px.is_finite()
                    && *rx_px > 0.0
                    && ry_px.is_finite()
                    && *ry_px > 0.0
                    && x_axis_rotation_rad.is_finite()
                    && (0.0..std::f64::consts::PI).contains(x_axis_rotation_rad)
            }
            Segment::Quad { ctrl } => ctrl.is_finite() && !ctrl.has_negative_zero(),
            Segment::Cubic { ctrl1, ctrl2 } => {
                ctrl1.is_finite()
                    && !ctrl1.has_negative_zero()
                    && ctrl2.is_finite()
                    && !ctrl2.has_negative_zero()
            }
        };
        if !valid {
            return Err(StrokeIrError::InvalidCenterline {
                edge,
                parameter: format!("segment[{index}]"),
            });
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct StrokeEnvelope<'a> {
    schema: &'static str,
    scene: &'a StrokeScene,
}

pub fn stroke_scene_bytes(scene: &ValidatedStrokeScene) -> Result<Vec<u8>, StrokeIrError> {
    serde_json::to_vec(&StrokeEnvelope {
        schema: STROKE_SCENE_SCHEMA,
        scene: scene.scene(),
    })
    .map_err(|error| StrokeIrError::Serialization {
        detail: error.to_string(),
    })
}

pub fn stroke_scene_digest_sha256(scene: &ValidatedStrokeScene) -> Result<String, StrokeIrError> {
    Ok(hex::encode(Sha256::digest(stroke_scene_bytes(scene)?)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LinearRgb, Segment};

    fn line() -> StrokeScene {
        StrokeScene {
            canvas: Canvas {
                width_px: 16,
                height_px: 16,
            },
            vertices: vec![
                StrokeVertex {
                    position: Pt::new(2.0, 8.0),
                    style: StrokeVertexStyle::Cap(StrokeCap::Round),
                },
                StrokeVertex {
                    position: Pt::new(14.0, 8.0),
                    style: StrokeVertexStyle::Cap(StrokeCap::Round),
                },
            ],
            edges: vec![StrokeEdge {
                id: StrokeEdgeId(0),
                start: StrokeVertexId(0),
                end: StrokeVertexId(1),
                centerline: CurveChain::single(Segment::Line),
                width_px: 3.0,
                paint: Paint::OpaqueSolid(LinearRgb::new(0.0, 0.0, 0.0)),
            }],
            background: Paint::OpaqueSolid(LinearRgb::new(1.0, 1.0, 1.0)),
        }
    }

    #[test]
    fn a_valid_line_round_trips_to_stable_bytes() {
        let scene = ValidatedStrokeScene::new(line()).unwrap();
        assert_eq!(
            stroke_scene_bytes(&scene).unwrap(),
            stroke_scene_bytes(&scene).unwrap()
        );
        assert_eq!(stroke_scene_digest_sha256(&scene).unwrap().len(), 64);
    }

    #[test]
    fn endpoint_and_junction_styles_are_degree_checked() {
        let mut invalid = line();
        invalid.vertices[0].style = StrokeVertexStyle::Junction(StrokeJunction::Round);
        assert!(matches!(
            ValidatedStrokeScene::new(invalid),
            Err(StrokeIrError::StyleDegree { degree: 1, .. })
        ));
    }
}
