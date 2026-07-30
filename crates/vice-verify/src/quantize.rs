use serde::Serialize;
use thiserror::Error;
use vice_geom::Pt;
use vice_ir::{JoinKind, Paint, PixelFilter, Segment, VectorScene};

use crate::scene::{preseal_scene_reusing, PresealedScene, VerificationConfig, VerificationError};

#[derive(Debug, Default)]
pub struct QuantizedVerificationWorkspace {
    render_workspace: vice_render::PartitionRenderWorkspace,
    pre_render: Option<vice_render::PartitionRender>,
    post_render: Option<vice_render::PartitionRender>,
}

impl QuantizedVerificationWorkspace {
    pub fn recycle(&mut self, verified: QuantizedVerifiedScene) {
        let QuantizedVerifiedScene { pre, post, .. } = verified;
        self.pre_render = Some(pre.render);
        self.post_render = Some(post.render);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct QuantizationPolicy {
    pub decimal_places: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PostQuantizationCertificate {
    pub decimal_places: u32,
    pub scalar_sites_quantized: u64,
    pub scalar_sites_changed: u64,
    pub max_scalar_delta: f64,
    pub max_boundary_displacement_px: f64,
    pub pre_scene_digest_sha256: String,
    pub post_scene_digest_sha256: String,
    pub topology_signature_sha256: String,
    pub post_render_digest_sha256: String,
}

#[derive(Debug, Clone)]
pub struct QuantizedVerifiedScene {
    pub(crate) pre: PresealedScene,
    pub(crate) post: PresealedScene,
    pub(crate) certificate: PostQuantizationCertificate,
}

impl QuantizedVerifiedScene {
    pub fn scene(&self) -> &VectorScene {
        self.post.scene()
    }
    pub fn render(&self) -> &vice_render::PartitionRender {
        self.post.render()
    }
    pub fn bindings(&self) -> &[crate::BoundaryBinding] {
        self.post.bindings()
    }
    pub fn preseal_certificate(&self) -> &crate::PresealCertificate {
        self.post.certificate()
    }
    pub fn pre_quantization_certificate(&self) -> &crate::PresealCertificate {
        self.pre.certificate()
    }
    pub fn post_quantization_certificate(&self) -> &PostQuantizationCertificate {
        &self.certificate
    }
}

#[derive(Debug, Error)]
pub enum QuantizationError {
    #[error("quantization policy is invalid")]
    InvalidPolicy,
    #[error("pre/post scene verification failed: {0}")]
    Verification(#[from] VerificationError),
    #[error("quantization changed the combinatorial topology")]
    TopologyChanged,
    #[error("quantized boundary left its observed-chain isotopy tube")]
    IsotopyTube,
}

fn quantize_scalar(
    value: &mut f64,
    scale: f64,
    sites: &mut u64,
    changed: &mut u64,
    max_delta: &mut f64,
) {
    *sites += 1;
    let old = *value;
    let rounded = (old * scale).round() / scale;
    *value = if rounded == 0.0 { 0.0 } else { rounded };
    if *value != old {
        *changed += 1;
        *max_delta = max_delta.max((*value - old).abs());
    }
}

fn quantize_tangent_angle(
    value: &mut f64,
    scale: f64,
    sites: &mut u64,
    changed: &mut u64,
    max_delta: &mut f64,
) {
    *sites += 1;
    let old = *value;
    let rounded = (old * scale).round() / scale;
    // Canonical IR uses (-π, π]. A nearest decimal can cross the open/closed
    // edge even when the source value is exactly valid (notably +π), so choose
    // the nearest in-range decimal on that edge. The post-G1 verifier decides
    // whether the resulting angular move is still admissible.
    *value = if rounded > std::f64::consts::PI {
        (std::f64::consts::PI * scale).floor() / scale
    } else if rounded <= -std::f64::consts::PI {
        (-std::f64::consts::PI * scale).ceil() / scale
    } else if rounded == 0.0 {
        0.0
    } else {
        rounded
    };
    if *value != old {
        *changed += 1;
        *max_delta = max_delta.max((*value - old).abs());
    }
}

fn quantize_rotation(
    value: &mut f64,
    scale: f64,
    sites: &mut u64,
    changed: &mut u64,
    max_delta: &mut f64,
) {
    *sites += 1;
    let old = *value;
    let rounded = (old * scale).round() / scale;
    // Elliptic-arc rotation is canonical on [0, π).
    *value = if rounded >= std::f64::consts::PI {
        (std::f64::consts::PI * scale).floor() / scale
    } else if rounded <= 0.0 {
        0.0
    } else {
        rounded
    };
    if *value != old {
        *changed += 1;
        *max_delta = max_delta.max((*value - old).abs());
    }
}

fn quantize_point(p: &mut Pt, scale: f64, sites: &mut u64, changed: &mut u64, max_delta: &mut f64) {
    quantize_scalar(&mut p.x, scale, sites, changed, max_delta);
    quantize_scalar(&mut p.y, scale, sites, changed, max_delta);
}

fn quantize_scene(
    scene: &VectorScene,
    policy: QuantizationPolicy,
) -> Result<(VectorScene, u64, u64, f64), QuantizationError> {
    if policy.decimal_places > 12 {
        return Err(QuantizationError::InvalidPolicy);
    }
    let scale = 10f64.powi(policy.decimal_places as i32);
    if !scale.is_finite() {
        return Err(QuantizationError::InvalidPolicy);
    }
    let mut output = scene.clone();
    let mut sites = 0u64;
    let mut changed = 0u64;
    let mut max_delta = 0.0f64;
    for vertex in &mut output.graph.vertices {
        quantize_point(
            &mut vertex.pos,
            scale,
            &mut sites,
            &mut changed,
            &mut max_delta,
        );
    }
    for boundary in &mut output.graph.boundaries {
        if let Some(JoinKind::SmoothG1 { tangent_angle_rad }) = &mut boundary.closure_join {
            quantize_tangent_angle(
                tangent_angle_rad,
                scale,
                &mut sites,
                &mut changed,
                &mut max_delta,
            );
        }
        for node in &mut boundary.curve.interior_nodes {
            quantize_point(
                &mut node.pos,
                scale,
                &mut sites,
                &mut changed,
                &mut max_delta,
            );
            if let JoinKind::SmoothG1 { tangent_angle_rad } = &mut node.join {
                quantize_tangent_angle(
                    tangent_angle_rad,
                    scale,
                    &mut sites,
                    &mut changed,
                    &mut max_delta,
                );
            }
        }
        for segment in &mut boundary.curve.segments {
            match segment {
                Segment::Line => {}
                Segment::CircularArc { radius_px, .. } => {
                    quantize_scalar(radius_px, scale, &mut sites, &mut changed, &mut max_delta)
                }
                Segment::EllipticArc {
                    rx_px,
                    ry_px,
                    x_axis_rotation_rad,
                    ..
                } => {
                    quantize_scalar(rx_px, scale, &mut sites, &mut changed, &mut max_delta);
                    quantize_scalar(ry_px, scale, &mut sites, &mut changed, &mut max_delta);
                    quantize_rotation(
                        x_axis_rotation_rad,
                        scale,
                        &mut sites,
                        &mut changed,
                        &mut max_delta,
                    );
                }
                Segment::Quad { ctrl } => {
                    quantize_point(ctrl, scale, &mut sites, &mut changed, &mut max_delta)
                }
                Segment::Cubic { ctrl1, ctrl2 } => {
                    quantize_point(ctrl1, scale, &mut sites, &mut changed, &mut max_delta);
                    quantize_point(ctrl2, scale, &mut sites, &mut changed, &mut max_delta);
                }
            }
        }
    }
    for face in &mut output.graph.faces {
        if let Paint::OpaqueSolid(color) = &mut face.paint {
            quantize_scalar(
                &mut color.r,
                scale,
                &mut sites,
                &mut changed,
                &mut max_delta,
            );
            quantize_scalar(
                &mut color.g,
                scale,
                &mut sites,
                &mut changed,
                &mut max_delta,
            );
            quantize_scalar(
                &mut color.b,
                scale,
                &mut sites,
                &mut changed,
                &mut max_delta,
            );
        }
    }
    if let PixelFilter::Gaussian { sigma_px } = &mut output.formation.pixel_filter {
        quantize_scalar(sigma_px, scale, &mut sites, &mut changed, &mut max_delta);
    }
    Ok((output, sites, changed, max_delta))
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

pub fn quantize_and_verify(
    scene: &VectorScene,
    bindings: &[crate::BoundaryBinding],
    verification: VerificationConfig,
    policy: QuantizationPolicy,
) -> Result<QuantizedVerifiedScene, QuantizationError> {
    quantize_and_verify_with_workspace(
        scene,
        bindings,
        verification,
        policy,
        &mut QuantizedVerificationWorkspace::default(),
    )
}

pub fn quantize_and_verify_with_workspace(
    scene: &VectorScene,
    bindings: &[crate::BoundaryBinding],
    verification: VerificationConfig,
    policy: QuantizationPolicy,
    workspace: &mut QuantizedVerificationWorkspace,
) -> Result<QuantizedVerifiedScene, QuantizationError> {
    let pre = preseal_scene_reusing(
        scene,
        bindings,
        verification,
        workspace.pre_render.take(),
        &mut workspace.render_workspace,
    )?;
    let (quantized, sites, changed, max_delta) = quantize_scene(scene, policy)?;
    let post = preseal_scene_reusing(
        &quantized,
        bindings,
        verification,
        workspace.post_render.take(),
        &mut workspace.render_workspace,
    )?;
    if pre.certificate().topology_signature_sha256 != post.certificate().topology_signature_sha256 {
        return Err(QuantizationError::TopologyChanged);
    }
    let mut max_boundary_displacement_px = 0.0f64;
    for (index, (before, after)) in pre
        .mesh
        .mesh()
        .boundary_polylines
        .iter()
        .zip(&post.mesh.mesh().boundary_polylines)
        .enumerate()
    {
        let displacement = directed_polyline_distance(&before.points, &after.points)
            .max(directed_polyline_distance(&after.points, &before.points))
            + before.max_deviation_px
            + after.max_deviation_px;
        max_boundary_displacement_px = max_boundary_displacement_px.max(displacement);
        let tube = bindings
            .iter()
            .find(|binding| binding.boundary().index() == index)
            .ok_or(QuantizationError::IsotopyTube)?
            .isotopy_tube_px();
        if displacement > tube {
            return Err(QuantizationError::IsotopyTube);
        }
    }
    let certificate = PostQuantizationCertificate {
        decimal_places: policy.decimal_places,
        scalar_sites_quantized: sites,
        scalar_sites_changed: changed,
        max_scalar_delta: max_delta,
        max_boundary_displacement_px,
        pre_scene_digest_sha256: pre.certificate().scene_digest_sha256.clone(),
        post_scene_digest_sha256: post.certificate().scene_digest_sha256.clone(),
        topology_signature_sha256: post.certificate().topology_signature_sha256.clone(),
        post_render_digest_sha256: post.certificate().render_digest_sha256.clone(),
    };
    Ok(QuantizedVerifiedScene {
        pre,
        post,
        certificate,
    })
}
