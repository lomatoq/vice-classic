use std::collections::BTreeSet;

use serde::Serialize;
use sha2::{Digest, Sha256};
use thiserror::Error;
use vice_geom::flatten::circular_arc_center;
use vice_geom::predicates::{closed_segments_intersect, shared_endpoint_segments_overlap};
use vice_geom::Pt;
use vice_ir::{BoundaryId, JoinKind, Segment, ValidatedScene, VectorScene, VertexId};
use vice_render::{
    render_digest_sha256, render_mesh_partition, CertifiedMesh, PartitionRender, RenderOptions,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VerificationConfig {
    pub render_options: RenderOptions,
    pub max_g1_spread_rad: f64,
    pub curve_separation_margin_px: f64,
}

impl VerificationConfig {
    fn validate(self) -> Result<(), VerificationError> {
        if !self.max_g1_spread_rad.is_finite()
            || self.max_g1_spread_rad < 0.0
            || !self.curve_separation_margin_px.is_finite()
            || self.curve_separation_margin_px <= 0.0
        {
            Err(VerificationError::InvalidConfig)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BoundaryBinding {
    observed_chain_sha256: String,
    dcel_boundary_sha256: String,
    support_geometry_sha256: String,
    boundary: BoundaryId,
    topology_signature_sha256: String,
    isotopy_tube_px: f64,
    support_polyline: Vec<Pt>,
}

impl BoundaryBinding {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        observed_chain_sha256: impl Into<String>,
        dcel_boundary_sha256: impl Into<String>,
        boundary: BoundaryId,
        topology_signature_sha256: impl Into<String>,
        isotopy_tube_px: f64,
        support_polyline: Vec<Pt>,
    ) -> Result<Self, VerificationError> {
        let observed_chain_sha256 = observed_chain_sha256.into();
        let dcel_boundary_sha256 = dcel_boundary_sha256.into();
        let topology_signature_sha256 = topology_signature_sha256.into();
        let digest_valid = |value: &str| {
            value.len() == 64
                && value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        };
        if !digest_valid(&observed_chain_sha256)
            || !digest_valid(&dcel_boundary_sha256)
            || !digest_valid(&topology_signature_sha256)
            || !isotopy_tube_px.is_finite()
            || isotopy_tube_px <= 0.0
            || support_polyline.len() < 2
            || support_polyline.iter().any(|point| !point.is_finite())
        {
            return Err(VerificationError::BoundaryBinding);
        }
        let support_bytes = serde_json::to_vec(&support_polyline)
            .map_err(|_| VerificationError::BoundaryBinding)?;
        Ok(Self {
            observed_chain_sha256,
            dcel_boundary_sha256,
            support_geometry_sha256: hex::encode(Sha256::digest(support_bytes)),
            boundary,
            topology_signature_sha256,
            isotopy_tube_px,
            support_polyline,
        })
    }

    pub fn observed_chain_sha256(&self) -> &str {
        &self.observed_chain_sha256
    }
    pub fn dcel_boundary_sha256(&self) -> &str {
        &self.dcel_boundary_sha256
    }
    pub fn support_geometry_sha256(&self) -> &str {
        &self.support_geometry_sha256
    }
    pub fn boundary(&self) -> BoundaryId {
        self.boundary
    }
    pub fn isotopy_tube_px(&self) -> f64 {
        self.isotopy_tube_px
    }
    pub fn support_polyline(&self) -> &[Pt] {
        &self.support_polyline
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PresealCertificate {
    pub scene_digest_sha256: String,
    pub topology_signature_sha256: String,
    pub render_digest_sha256: String,
    pub boundaries: u64,
    pub faces: u64,
    pub observed_chain_bindings: u64,
    pub dcel_boundary_bindings: u64,
    pub g1_nodes: u64,
    pub worst_g1_spread_rad: f64,
    pub max_tessellation_deviation_px: f64,
    pub curve_pair_checks: u64,
    pub max_support_isotopy_displacement_px: f64,
}

#[derive(Debug, Clone)]
pub struct PresealedScene {
    pub(crate) scene: VectorScene,
    pub(crate) mesh: CertifiedMesh,
    pub(crate) render: PartitionRender,
    pub(crate) bindings: Vec<BoundaryBinding>,
    pub(crate) certificate: PresealCertificate,
}

impl PresealedScene {
    pub fn scene(&self) -> &VectorScene {
        &self.scene
    }
    pub fn render(&self) -> &PartitionRender {
        &self.render
    }
    pub fn bindings(&self) -> &[BoundaryBinding] {
        &self.bindings
    }
    pub fn certificate(&self) -> &PresealCertificate {
        &self.certificate
    }
}

#[derive(Debug, Error)]
pub enum VerificationError {
    #[error("verification configuration is invalid")]
    InvalidConfig,
    #[error("scene is invalid: {0}")]
    InvalidScene(#[from] vice_ir::SceneError),
    #[error("render certification failed: {0}")]
    Render(#[from] vice_render::RenderError),
    #[error("boundary bindings are missing, duplicated, stale, or malformed")]
    BoundaryBinding,
    #[error("segment tangent is degenerate at boundary {boundary:?}, segment {segment}")]
    DegenerateTangent {
        boundary: BoundaryId,
        segment: usize,
    },
    #[error("G1 mismatch at boundary {boundary:?}, node {node}: {spread_rad} rad")]
    G1 {
        boundary: BoundaryId,
        node: usize,
        spread_rad: f64,
    },
    #[error("tessellated boundaries intersect outside a declared shared vertex")]
    Intersection,
    #[error("curve separation cannot be certified at the requested tessellation budget")]
    UncertifiedCurveSeparation,
    #[error("topology signature serialization failed")]
    TopologySignature,
}

#[derive(Serialize)]
struct TopologyIdentity {
    exterior: u32,
    boundaries: Vec<(u32, u32, u32, u32)>,
    half_edges: Vec<(u32, bool, u32, u32, u32)>,
    face_loops: Vec<Vec<u32>>,
}

pub fn topology_signature_sha256(scene: &VectorScene) -> Result<String, VerificationError> {
    let graph = &scene.graph;
    let identity = TopologyIdentity {
        exterior: graph.exterior.0,
        boundaries: graph
            .boundaries
            .iter()
            .map(|b| {
                (
                    b.left_face.0,
                    b.right_face.0,
                    b.start_vertex.0,
                    b.end_vertex.0,
                )
            })
            .collect(),
        half_edges: graph
            .half_edges
            .iter()
            .map(|h| (h.boundary.0, h.forward, h.twin.0, h.next.0, h.face.0))
            .collect(),
        face_loops: graph
            .faces
            .iter()
            .map(|f| f.loops.iter().map(|h| h.0).collect())
            .collect(),
    };
    let bytes = serde_json::to_vec(&identity).map_err(|_| VerificationError::TopologySignature)?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn unit(v: Pt) -> Option<Pt> {
    let length = v.length();
    (length.is_finite() && length > 0.0).then(|| v * (1.0 / length))
}

fn arc_tangents(p0: Pt, p1: Pt, radius: f64, large: bool, ccw: bool) -> Option<(Pt, Pt)> {
    let arc = circular_arc_center(p0, p1, radius, large, ccw).ok()?;
    let tangent = |p: Pt| {
        let radial = p - arc.center;
        let v = if ccw {
            Pt::new(-radial.y, radial.x)
        } else {
            Pt::new(radial.y, -radial.x)
        };
        unit(v)
    };
    Some((tangent(p0)?, tangent(p1)?))
}

#[allow(clippy::too_many_arguments)]
fn elliptic_tangents(
    p0: Pt,
    p1: Pt,
    rx: f64,
    ry: f64,
    rotation: f64,
    large: bool,
    ccw: bool,
) -> Option<(Pt, Pt)> {
    let (sin_phi, cos_phi) = rotation.sin_cos();
    let to_circle = |p: Pt| {
        Pt::new(
            (cos_phi * p.x + sin_phi * p.y) / rx,
            (-sin_phi * p.x + cos_phi * p.y) / ry,
        )
    };
    let q0 = to_circle(p0);
    let q1 = to_circle(p1);
    let arc = circular_arc_center(q0, q1, 1.0, large, ccw).ok()?;
    let tangent = |theta: f64| {
        let sign = if ccw { 1.0 } else { -1.0 };
        let dx = sign * -theta.sin() * rx;
        let dy = sign * theta.cos() * ry;
        unit(Pt::new(
            cos_phi * dx - sin_phi * dy,
            sin_phi * dx + cos_phi * dy,
        ))
    };
    Some((
        tangent(arc.theta0_rad)?,
        tangent(arc.theta0_rad + arc.sweep_rad)?,
    ))
}

fn tangents(segment: &Segment, p0: Pt, p1: Pt) -> Option<(Pt, Pt)> {
    match *segment {
        Segment::Line => {
            let u = unit(p1 - p0)?;
            Some((u, u))
        }
        Segment::Quad { ctrl } => Some((unit(ctrl - p0)?, unit(p1 - ctrl)?)),
        Segment::Cubic { ctrl1, ctrl2 } => Some((unit(ctrl1 - p0)?, unit(p1 - ctrl2)?)),
        Segment::CircularArc {
            radius_px,
            large_arc,
            ccw,
        } => arc_tangents(p0, p1, radius_px, large_arc, ccw),
        Segment::EllipticArc {
            rx_px,
            ry_px,
            x_axis_rotation_rad,
            large_arc,
            ccw,
        } => elliptic_tangents(p0, p1, rx_px, ry_px, x_axis_rotation_rad, large_arc, ccw),
    }
}

fn angle_gap(a: Pt, b: Pt) -> f64 {
    (a.x * b.y - a.y * b.x).atan2(a.dot(b)).abs()
}

fn verify_g1(scene: &VectorScene, max_spread: f64) -> Result<(u64, f64), VerificationError> {
    let mut nodes = 0u64;
    let mut worst = 0.0f64;
    for (bi, boundary) in scene.graph.boundaries.iter().enumerate() {
        let points = boundary.curve.node_positions(
            scene.graph.vertices[boundary.start_vertex.index()].pos,
            scene.graph.vertices[boundary.end_vertex.index()].pos,
        );
        let mut ends = Vec::with_capacity(boundary.curve.segments.len());
        for (si, segment) in boundary.curve.segments.iter().enumerate() {
            let Some(value) = tangents(segment, points[si], points[si + 1]) else {
                return Err(VerificationError::DegenerateTangent {
                    boundary: BoundaryId(bi as u32),
                    segment: si,
                });
            };
            ends.push(value);
        }
        for (ni, node) in boundary.curve.interior_nodes.iter().enumerate() {
            let JoinKind::SmoothG1 { tangent_angle_rad } = node.join else {
                continue;
            };
            nodes += 1;
            let declared = Pt::new(tangent_angle_rad.cos(), tangent_angle_rad.sin());
            let spread = angle_gap(ends[ni].1, ends[ni + 1].0)
                .max(angle_gap(ends[ni].1, declared))
                .max(angle_gap(ends[ni + 1].0, declared));
            worst = worst.max(spread);
            if spread > max_spread {
                return Err(VerificationError::G1 {
                    boundary: BoundaryId(bi as u32),
                    node: ni,
                    spread_rad: spread,
                });
            }
        }
        if let Some(JoinKind::SmoothG1 { tangent_angle_rad }) = boundary.closure_join {
            nodes += 1;
            let declared = Pt::new(tangent_angle_rad.cos(), tangent_angle_rad.sin());
            let last = ends.len() - 1;
            let spread = angle_gap(ends[last].1, ends[0].0)
                .max(angle_gap(ends[last].1, declared))
                .max(angle_gap(ends[0].0, declared));
            worst = worst.max(spread);
            if spread > max_spread {
                return Err(VerificationError::G1 {
                    boundary: BoundaryId(bi as u32),
                    node: boundary.curve.interior_nodes.len(),
                    spread_rad: spread,
                });
            }
        }
    }
    Ok((nodes, worst))
}

fn point_segment_distance(p: Pt, a: Pt, b: Pt) -> f64 {
    let d = b - a;
    let length_sq = d.length_sq();
    if length_sq == 0.0 {
        p.dist(a)
    } else {
        let t = ((p - a).dot(d) / length_sq).clamp(0.0, 1.0);
        p.dist(a + d * t)
    }
}

fn directed_polyline_distance(points: &[Pt], target: &[Pt]) -> f64 {
    points.iter().fold(0.0f64, |worst, point| {
        let best = target
            .windows(2)
            .map(|segment| point_segment_distance(*point, segment[0], segment[1]))
            .fold(f64::INFINITY, f64::min);
        worst.max(best)
    })
}

fn segment_distance(a: Pt, b: Pt, c: Pt, d: Pt) -> f64 {
    if closed_segments_intersect(a, b, c, d) {
        0.0
    } else {
        point_segment_distance(a, c, d)
            .min(point_segment_distance(b, c, d))
            .min(point_segment_distance(c, a, b))
            .min(point_segment_distance(d, a, b))
    }
}

fn endpoint_vertex(
    scene: &VectorScene,
    boundary: usize,
    point_index: usize,
    point_count: usize,
) -> Option<VertexId> {
    let b = &scene.graph.boundaries[boundary];
    if point_index == 0 {
        Some(b.start_vertex)
    } else if point_index + 1 == point_count {
        Some(b.end_vertex)
    } else {
        None
    }
}

fn allowed_touch(
    scene: &VectorScene,
    mesh: &vice_render::RenderMesh,
    ba: usize,
    sa: usize,
    bb: usize,
    sb: usize,
) -> bool {
    let pa = &mesh.boundary_polylines[ba].points;
    let pb = &mesh.boundary_polylines[bb].points;
    let a = [pa[sa], pa[sa + 1]];
    let b = [pb[sb], pb[sb + 1]];
    if ba == bb {
        let adjacent = sa.abs_diff(sb) == 1
            || (scene.graph.boundaries[ba].start_vertex == scene.graph.boundaries[ba].end_vertex
                && ((sa == 0 && sb + 2 == pa.len()) || (sb == 0 && sa + 2 == pa.len())));
        if !adjacent {
            return false;
        }
        let Some(shared) = a.into_iter().find(|p| b.contains(p)) else {
            return false;
        };
        let other_a = if a[0] == shared { a[1] } else { a[0] };
        let other_b = if b[0] == shared { b[1] } else { b[0] };
        return !shared_endpoint_segments_overlap(shared, other_a, other_b);
    }
    for (ia, point_a) in [(sa, a[0]), (sa + 1, a[1])] {
        for (ib, point_b) in [(sb, b[0]), (sb + 1, b[1])] {
            if point_a != point_b {
                continue;
            }
            let Some(va) = endpoint_vertex(scene, ba, ia, pa.len()) else {
                continue;
            };
            let Some(vb) = endpoint_vertex(scene, bb, ib, pb.len()) else {
                continue;
            };
            if va == vb {
                let other_a = if a[0] == point_a { a[1] } else { a[0] };
                let other_b = if b[0] == point_b { b[1] } else { b[0] };
                return !shared_endpoint_segments_overlap(point_a, other_a, other_b);
            }
        }
    }
    false
}

fn verify_curve_separation(
    scene: &VectorScene,
    mesh: &vice_render::RenderMesh,
    margin: f64,
) -> Result<u64, VerificationError> {
    let mut checks = 0u64;
    for ba in 0..mesh.boundary_polylines.len() {
        let pa = &mesh.boundary_polylines[ba];
        for bb in ba..mesh.boundary_polylines.len() {
            let pb = &mesh.boundary_polylines[bb];
            for sa in 0..pa.points.len() - 1 {
                for sb in 0..pb.points.len() - 1 {
                    if ba == bb && sa == sb {
                        continue;
                    }
                    checks += 1;
                    let (a, b) = (pa.points[sa], pa.points[sa + 1]);
                    let (c, d) = (pb.points[sb], pb.points[sb + 1]);
                    let allowed = allowed_touch(scene, mesh, ba, sa, bb, sb);
                    if closed_segments_intersect(a, b, c, d) {
                        if allowed {
                            continue;
                        }
                        return Err(VerificationError::Intersection);
                    }
                    if allowed {
                        continue;
                    }
                    let certified_margin = pa.max_deviation_px + pb.max_deviation_px + margin;
                    if segment_distance(a, b, c, d) <= certified_margin {
                        return Err(VerificationError::UncertifiedCurveSeparation);
                    }
                }
            }
        }
    }
    Ok(checks)
}

fn verify_bindings(
    scene: &VectorScene,
    bindings: &[BoundaryBinding],
    topology: &str,
) -> Result<(), VerificationError> {
    if bindings.len() != scene.graph.boundaries.len() {
        return Err(VerificationError::BoundaryBinding);
    }
    let mut boundaries = BTreeSet::new();
    let mut chains = BTreeSet::new();
    let mut dcel_boundaries = BTreeSet::new();
    for binding in bindings {
        if binding.boundary.index() >= scene.graph.boundaries.len()
            || binding.topology_signature_sha256 != topology
            || !boundaries.insert(binding.boundary)
            || !chains.insert(binding.observed_chain_sha256.as_str())
            || !dcel_boundaries.insert(binding.dcel_boundary_sha256.as_str())
        {
            return Err(VerificationError::BoundaryBinding);
        }
    }
    Ok(())
}

pub fn preseal_scene(
    scene: &VectorScene,
    bindings: &[BoundaryBinding],
    cfg: VerificationConfig,
) -> Result<PresealedScene, VerificationError> {
    cfg.validate()?;
    let validated = ValidatedScene::new(scene.clone())?;
    let topology = topology_signature_sha256(scene)?;
    verify_bindings(scene, bindings, &topology)?;
    let (g1_nodes, worst_g1_spread_rad) = verify_g1(scene, cfg.max_g1_spread_rad)?;
    let mesh = CertifiedMesh::from_scene(&validated, cfg.render_options)?;
    let curve_pair_checks =
        verify_curve_separation(scene, mesh.mesh(), cfg.curve_separation_margin_px)?;
    let mut max_support_isotopy_displacement_px = 0.0f64;
    for binding in bindings {
        let fitted = &mesh.mesh().boundary_polylines[binding.boundary.index()];
        let displacement =
            directed_polyline_distance(&fitted.points, &binding.support_polyline).max(
                directed_polyline_distance(&binding.support_polyline, &fitted.points),
            ) + fitted.max_deviation_px;
        max_support_isotopy_displacement_px = max_support_isotopy_displacement_px.max(displacement);
        if displacement > binding.isotopy_tube_px {
            return Err(VerificationError::BoundaryBinding);
        }
    }
    let render = render_mesh_partition(&mesh)?;
    let max_tessellation_deviation_px = mesh
        .mesh()
        .boundary_polylines
        .iter()
        .map(|b| b.max_deviation_px)
        .fold(0.0, f64::max);
    let certificate = PresealCertificate {
        scene_digest_sha256: vice_ir::scene_digest_sha256(scene)?,
        topology_signature_sha256: topology,
        render_digest_sha256: render_digest_sha256(&render),
        boundaries: scene.graph.boundaries.len() as u64,
        faces: scene.graph.faces.len() as u64,
        observed_chain_bindings: bindings.len() as u64,
        dcel_boundary_bindings: bindings.len() as u64,
        g1_nodes,
        worst_g1_spread_rad,
        max_tessellation_deviation_px,
        curve_pair_checks,
        max_support_isotopy_displacement_px,
    };
    Ok(PresealedScene {
        scene: scene.clone(),
        mesh,
        render,
        bindings: bindings.to_vec(),
        certificate,
    })
}
