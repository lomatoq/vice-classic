use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use vice_ir::{
    canonical_scene_bytes, parse_scene, Boundary, BoundaryId, FaceId, HalfEdgeId, Paint,
    PlanarGraph, Segment, VectorScene,
};

pub const EXPORT_PLAN_SCHEMA: &str = "vice-classic/export-plan/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FacePlan {
    face_id: u32,
    z_index: u32,
    fill_srgb8: String,
    path_d: String,
}

impl FacePlan {
    pub fn face_id(&self) -> u32 {
        self.face_id
    }
    pub fn z_index(&self) -> u32 {
        self.z_index
    }
    pub fn fill_srgb8(&self) -> &str {
        &self.fill_srgb8
    }
    pub fn path_d(&self) -> &str {
        &self.path_d
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApronPlan {
    boundary_id: u32,
    lower_face: u32,
    upper_face: u32,
    stroke_srgb8: String,
    width_px: f64,
    path_d: String,
}

impl ApronPlan {
    pub fn boundary_id(&self) -> u32 {
        self.boundary_id
    }
    pub fn lower_face(&self) -> u32 {
        self.lower_face
    }
    pub fn upper_face(&self) -> u32 {
        self.upper_face
    }
    pub fn stroke_srgb8(&self) -> &str {
        &self.stroke_srgb8
    }
    pub fn width_px(&self) -> f64 {
        self.width_px
    }
    pub fn path_d(&self) -> &str {
        &self.path_d
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportPlan {
    schema: String,
    scene_digest_sha256: String,
    width_px: u32,
    height_px: u32,
    decimal_places: u32,
    apron_width_px: f64,
    faces: Vec<FacePlan>,
    aprons: Vec<ApronPlan>,
}

impl ExportPlan {
    pub fn schema(&self) -> &str {
        &self.schema
    }
    pub fn scene_digest_sha256(&self) -> &str {
        &self.scene_digest_sha256
    }
    pub fn width_px(&self) -> u32 {
        self.width_px
    }
    pub fn height_px(&self) -> u32 {
        self.height_px
    }
    pub fn decimal_places(&self) -> u32 {
        self.decimal_places
    }
    pub fn apron_width_px(&self) -> f64 {
        self.apron_width_px
    }
    pub fn faces(&self) -> &[FacePlan] {
        &self.faces
    }
    pub fn aprons(&self) -> &[ApronPlan] {
        &self.aprons
    }
    pub fn digest_sha256(&self) -> Result<String, ExportPlanError> {
        Ok(hex::encode(Sha256::digest(canonical_export_plan_bytes(
            self,
        )?)))
    }
}

#[derive(Debug, Error)]
pub enum ExportPlanError {
    #[error("scene is invalid: {0}")]
    InvalidScene(#[from] vice_ir::SceneError),
    #[error("canonical scene parse failed: {0}")]
    CanonicalParse(String),
    #[error("export precision or apron width is invalid")]
    InvalidPolicy,
    #[error("face loop is not closed")]
    OpenLoop,
    #[error("export plan serialization failed: {0}")]
    Serialization(String),
    #[error("export plan is not canonical after roundtrip")]
    NonCanonicalRoundtrip,
}

fn number(v: f64, places: u32) -> String {
    let value = if v == 0.0 { 0.0 } else { v };
    let mut text = format!("{:.*}", places as usize, value);
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    if text == "-0" {
        "0".into()
    } else {
        text
    }
}

fn point(p: vice_geom::Pt, places: u32) -> String {
    format!("{} {}", number(p.x, places), number(p.y, places))
}

fn color(paint: Paint) -> Option<String> {
    let Paint::OpaqueSolid(c) = paint else {
        return None;
    };
    Some(format!(
        "#{:02x}{:02x}{:02x}",
        vice_ir::color::linear_to_srgb_u8(c.r),
        vice_ir::color::linear_to_srgb_u8(c.g),
        vice_ir::color::linear_to_srgb_u8(c.b)
    ))
}

fn emit_segment(
    output: &mut String,
    segment: &Segment,
    endpoint: vice_geom::Pt,
    forward: bool,
    places: u32,
) {
    match *segment {
        Segment::Line => output.push_str(&format!(" L {}", point(endpoint, places))),
        Segment::Quad { ctrl } => output.push_str(&format!(
            " Q {} {}",
            point(ctrl, places),
            point(endpoint, places)
        )),
        Segment::Cubic { ctrl1, ctrl2 } => {
            let (c1, c2) = if forward {
                (ctrl1, ctrl2)
            } else {
                (ctrl2, ctrl1)
            };
            output.push_str(&format!(
                " C {} {} {}",
                point(c1, places),
                point(c2, places),
                point(endpoint, places)
            ));
        }
        Segment::CircularArc {
            radius_px,
            large_arc,
            ccw,
        } => output.push_str(&format!(
            " A {r} {r} 0 {large} {sweep} {end}",
            r = number(radius_px, places),
            large = u8::from(large_arc),
            sweep = u8::from(if forward { ccw } else { !ccw }),
            end = point(endpoint, places)
        )),
        Segment::EllipticArc {
            rx_px,
            ry_px,
            x_axis_rotation_rad,
            large_arc,
            ccw,
        } => output.push_str(&format!(
            " A {rx} {ry} {rotation} {large} {sweep} {end}",
            rx = number(rx_px, places),
            ry = number(ry_px, places),
            rotation = number(x_axis_rotation_rad.to_degrees(), places),
            large = u8::from(large_arc),
            sweep = u8::from(if forward { ccw } else { !ccw }),
            end = point(endpoint, places)
        )),
    }
}

fn append_boundary(
    output: &mut String,
    graph: &PlanarGraph,
    boundary: &Boundary,
    forward: bool,
    places: u32,
) {
    let points = boundary.curve.node_positions(
        graph.vertices[boundary.start_vertex.index()].pos,
        graph.vertices[boundary.end_vertex.index()].pos,
    );
    if forward {
        for (index, segment) in boundary.curve.segments.iter().enumerate() {
            emit_segment(output, segment, points[index + 1], true, places);
        }
    } else {
        for index in (0..boundary.curve.segments.len()).rev() {
            emit_segment(
                output,
                &boundary.curve.segments[index],
                points[index],
                false,
                places,
            );
        }
    }
}

fn start_of_half_edge(graph: &PlanarGraph, half_edge: HalfEdgeId) -> vice_geom::Pt {
    graph.vertices[graph.he_origin(half_edge).index()].pos
}

fn face_path(graph: &PlanarGraph, face: FaceId, places: u32) -> Result<String, ExportPlanError> {
    let mut output = String::new();
    for &start in &graph.faces[face.index()].loops {
        output.push_str(&format!(
            "M {}",
            point(start_of_half_edge(graph, start), places)
        ));
        let mut current = start;
        for _ in 0..=graph.half_edges.len() {
            let he = graph.half_edges[current.index()];
            let boundary = &graph.boundaries[he.boundary.index()];
            append_boundary(&mut output, graph, boundary, he.forward, places);
            current = he.next;
            if current == start {
                output.push_str(" Z ");
                break;
            }
        }
        if current != start {
            return Err(ExportPlanError::OpenLoop);
        }
    }
    Ok(output.trim().to_owned())
}

fn boundary_path(graph: &PlanarGraph, boundary: BoundaryId, places: u32) -> String {
    let b = &graph.boundaries[boundary.index()];
    let mut output = format!(
        "M {}",
        point(graph.vertices[b.start_vertex.index()].pos, places)
    );
    append_boundary(&mut output, graph, b, true, places);
    output
}

/// Build an export plan from canonicalized scene labeling. Aprons are admitted
/// only on an opaque/opaque shared boundary whose endpoint degree is two; an
/// exterior edge, gap endpoint, or junction therefore cannot receive one.
pub fn build_export_plan(
    scene: &VectorScene,
    decimal_places: u32,
    apron_width_px: f64,
) -> Result<ExportPlan, ExportPlanError> {
    vice_ir::validate_scene(scene)?;
    if decimal_places > 12
        || !apron_width_px.is_finite()
        || apron_width_px <= 0.0
        || apron_width_px > 4.0
    {
        return Err(ExportPlanError::InvalidPolicy);
    }
    let canonical_bytes = canonical_scene_bytes(scene)?;
    let canonical = parse_scene(&canonical_bytes)
        .map_err(|e| ExportPlanError::CanonicalParse(e.to_string()))?;
    let graph = &canonical.graph;
    let mut faces = Vec::new();
    for (index, face) in graph.faces.iter().enumerate() {
        let id = FaceId(index as u32);
        if id == graph.exterior {
            continue;
        }
        let Some(fill) = color(face.paint) else {
            continue;
        };
        faces.push(FacePlan {
            face_id: index as u32,
            z_index: index as u32,
            fill_srgb8: fill,
            path_d: face_path(graph, id, decimal_places)?,
        });
    }

    let mut degree = vec![0usize; graph.vertices.len()];
    for boundary in &graph.boundaries {
        degree[boundary.start_vertex.index()] += 1;
        degree[boundary.end_vertex.index()] += 1;
    }
    let mut aprons = Vec::new();
    for (index, boundary) in graph.boundaries.iter().enumerate() {
        let Some(left) = color(graph.faces[boundary.left_face.index()].paint) else {
            continue;
        };
        let Some(right) = color(graph.faces[boundary.right_face.index()].paint) else {
            continue;
        };
        if boundary.left_face == graph.exterior
            || boundary.right_face == graph.exterior
            || degree[boundary.start_vertex.index()] != 2
            || degree[boundary.end_vertex.index()] != 2
        {
            continue;
        }
        let (lower, upper, stroke) = if boundary.left_face < boundary.right_face {
            (boundary.left_face, boundary.right_face, left)
        } else {
            (boundary.right_face, boundary.left_face, right)
        };
        aprons.push(ApronPlan {
            boundary_id: index as u32,
            lower_face: lower.0,
            upper_face: upper.0,
            stroke_srgb8: stroke,
            width_px: apron_width_px,
            path_d: boundary_path(graph, BoundaryId(index as u32), decimal_places),
        });
    }
    aprons.sort_by_key(|a| (a.lower_face, a.upper_face, a.boundary_id));
    Ok(ExportPlan {
        schema: EXPORT_PLAN_SCHEMA.into(),
        scene_digest_sha256: vice_ir::scene_digest_sha256(&canonical)?,
        width_px: canonical.canvas.width_px,
        height_px: canonical.canvas.height_px,
        decimal_places,
        apron_width_px,
        faces,
        aprons,
    })
}

pub fn canonical_export_plan_bytes(plan: &ExportPlan) -> Result<Vec<u8>, ExportPlanError> {
    let first =
        serde_json::to_vec(plan).map_err(|e| ExportPlanError::Serialization(e.to_string()))?;
    let parsed: ExportPlan = serde_json::from_slice(&first)
        .map_err(|e| ExportPlanError::Serialization(e.to_string()))?;
    let second =
        serde_json::to_vec(&parsed).map_err(|e| ExportPlanError::Serialization(e.to_string()))?;
    if first != second {
        return Err(ExportPlanError::NonCanonicalRoundtrip);
    }
    Ok(first)
}
