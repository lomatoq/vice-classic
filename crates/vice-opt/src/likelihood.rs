//! Full-resolution, correlation-aware observation likelihood (spec §17).

use serde::Serialize;
use thiserror::Error;
use vice_image::ObservationTensor;
use vice_ir::{BlendSpace, Paint, QuantizationModel, VectorScene};
use vice_render::PartitionRender;

use crate::trust_region::ScoreScope;

/// The audited production residual model implemented by this milestone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResidualModelId {
    /// Non-overlapping blocks, each no smaller than calibrated correlation
    /// support. One robust observation is charged per block/channel.
    CorrelationBlockV1,
}

/// Frozen inputs of the correlation-block likelihood.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct BlockLikelihoodConfig {
    pub residual_model_id: ResidualModelId,
    pub block_size_px: u32,
    pub calibrated_correlation_support_px: f64,
    pub sigma_by_channel: [f64; 4],
    pub student_t_degrees_of_freedom: f64,
}

impl BlockLikelihoodConfig {
    pub fn new(
        block_size_px: u32,
        calibrated_correlation_support_px: f64,
        sigma_by_channel: [f64; 4],
        student_t_degrees_of_freedom: f64,
    ) -> Result<Self, LikelihoodError> {
        let cfg = Self {
            residual_model_id: ResidualModelId::CorrelationBlockV1,
            block_size_px,
            calibrated_correlation_support_px,
            sigma_by_channel,
            student_t_degrees_of_freedom,
        };
        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(self) -> Result<(), LikelihoodError> {
        if self.block_size_px == 0
            || !self.calibrated_correlation_support_px.is_finite()
            || self.calibrated_correlation_support_px <= 0.0
            || f64::from(self.block_size_px) < self.calibrated_correlation_support_px.ceil()
        {
            return Err(LikelihoodError::CorrelationSupport {
                block_size_px: self.block_size_px,
                calibrated_support_px: self.calibrated_correlation_support_px,
            });
        }
        if self
            .sigma_by_channel
            .iter()
            .any(|v| !v.is_finite() || *v <= 0.0)
            || !self.student_t_degrees_of_freedom.is_finite()
            || self.student_t_degrees_of_freedom <= 2.0
        {
            return Err(LikelihoodError::InvalidNoiseModel);
        }
        Ok(())
    }
}

/// Non-pixel code lengths. The type intentionally has no evidence-loss field:
/// boundary maps and corridors are proposals/diagnostics, not a second copy of
/// the observed pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PriorCodeLengths {
    pub topology_bits: f64,
    pub geometry_bits: f64,
    pub paint_bits: f64,
    pub relation_bits: f64,
    pub formation_bits: f64,
}

impl PriorCodeLengths {
    fn validate(self) -> Result<(), LikelihoodError> {
        let nonnegative = [
            self.topology_bits,
            self.geometry_bits,
            self.paint_bits,
            self.formation_bits,
        ]
        .into_iter()
        .all(|v| v.is_finite() && v >= 0.0);
        // Relation codes may be negative when a constrained model saves more
        // parameter bits than its relation tag costs.
        if !nonnegative || !self.relation_bits.is_finite() {
            return Err(LikelihoodError::InvalidPrior);
        }
        Ok(())
    }
}

/// Machine-readable declaration that final pixel ownership is exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoreOwnership {
    FullResolutionObservationOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PredictionSource {
    CertifiedInternalPartition,
    SerializedSvgRender,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LikelihoodDiagnostics {
    pub residual_model_id: ResidualModelId,
    pub prediction_source: PredictionSource,
    pub source_sha256: String,
    pub calibrated_correlation_support_px: f64,
    pub empirical_correlation_length_px: f64,
    pub lag1_x: f64,
    pub lag1_y: f64,
    pub block_size_px: u32,
    pub blocks: u64,
    pub quantization_deadzone_components: u64,
    /// Diagnostic only. It is never added to `total_bits`.
    pub iid_pixel_diagnostic_bits: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScoreBreakdown {
    pub ownership: ScoreOwnership,
    pub pixel_bits: f64,
    pub topology_bits: f64,
    pub geometry_bits: f64,
    pub paint_bits: f64,
    pub relation_bits: f64,
    pub formation_bits: f64,
    pub total_bits: f64,
    pub diagnostics: LikelihoodDiagnostics,
}

/// Reusable full-resolution arrays for repeated scores of one observation.
///
/// Trust-region search evaluates many nearby scenes at identical dimensions.
/// Reusing these arrays bounds the process working set without retaining any
/// score, scene, or optimizer state between evaluations.
#[derive(Debug, Default)]
pub struct LikelihoodWorkspace {
    predicted: Vec<[f64; 4]>,
    residual: Vec<[f64; 4]>,
}

#[derive(Debug, Error, Clone, PartialEq)]
pub enum LikelihoodError {
    #[error("block size {block_size_px} is below calibrated correlation support {calibrated_support_px}")]
    CorrelationSupport {
        block_size_px: u32,
        calibrated_support_px: f64,
    },
    #[error("invalid likelihood noise model")]
    InvalidNoiseModel,
    #[error("invalid prior code lengths")]
    InvalidPrior,
    #[error("scene, observation, and render dimensions disagree")]
    DimensionMismatch,
    #[error("render face coverage does not match the scene graph")]
    FaceCoverageMismatch,
    #[error("only uint8 quantization is supported in the M7 Flat2 likelihood")]
    UnsupportedQuantization,
    #[error("likelihood produced a non-finite score")]
    NonFiniteScore,
    #[error("ROI likelihood scope is malformed or outside the canvas")]
    InvalidScoreScope,
}

fn predicted_observation(
    scene: &VectorScene,
    render: &PartitionRender,
    predicted: &mut Vec<[f64; 4]>,
) {
    let n = render.width_px as usize * render.height_px as usize;
    predicted.clear();
    predicted.resize(n, [0.0; 4]);
    for (face, coverage) in scene.graph.faces.iter().zip(&render.face_coverage) {
        let p = match face.paint {
            Paint::OpaqueSolid(c) => {
                vice_image::paint_observation_premul(c, scene.formation.blend_space)
            }
            Paint::TransparentExterior => [0.0; 4],
        };
        for (out, a) in predicted.iter_mut().zip(coverage) {
            for ch in 0..4 {
                out[ch] += *a * p[ch];
            }
        }
    }
}

fn serialized_prediction(bytes: &[u8], blend_space: BlendSpace, predicted: &mut Vec<[f64; 4]>) {
    predicted.clear();
    predicted.extend(bytes.chunks_exact(4).map(|pixel| {
        let alpha = f64::from(pixel[3]) / 255.0;
        if blend_space == BlendSpace::EncodedSrgb {
            [
                f64::from(pixel[0]) / 255.0,
                f64::from(pixel[1]) / 255.0,
                f64::from(pixel[2]) / 255.0,
                alpha,
            ]
        } else if alpha == 0.0 {
            [0.0; 4]
        } else {
            let linear = |channel: u8| {
                let encoded = (f64::from(channel) / 255.0 / alpha).clamp(0.0, 1.0);
                vice_ir::color::srgb_encoded_to_linear(encoded) * alpha
            };
            [linear(pixel[0]), linear(pixel[1]), linear(pixel[2]), alpha]
        }
    }));
}

fn lag1(residual: &[[f64; 4]], width: usize, height: usize, dx: usize, dy: usize) -> f64 {
    let mut dot = 0.0;
    let mut aa = 0.0;
    let mut bb = 0.0;
    for y in 0..height.saturating_sub(dy) {
        for x in 0..width.saturating_sub(dx) {
            let a = residual[y * width + x];
            let b = residual[(y + dy) * width + x + dx];
            for ch in 0..4 {
                dot += a[ch] * b[ch];
                aa += a[ch] * a[ch];
                bb += b[ch] * b[ch];
            }
        }
    }
    if aa == 0.0 || bb == 0.0 {
        0.0
    } else {
        (dot / (aa * bb).sqrt()).clamp(-1.0, 1.0)
    }
}

fn correlation_length(rho: f64) -> f64 {
    let a = rho.abs();
    if !(0.0..1.0).contains(&a) || a == 0.0 {
        1.0
    } else {
        (-1.0 / a.ln()).max(1.0)
    }
}

fn robust_bits(z2: f64, degrees_of_freedom: f64) -> f64 {
    0.5 * (degrees_of_freedom + 1.0) * (1.0 + z2 / degrees_of_freedom).log2()
}

/// Score the entire observed raster against a certified partition render.
///
/// Quantization is represented as the exact observed uint8 cell: prediction
/// inside that cell has zero residual, while distance outside the cell is
/// charged. Non-overlapping correlation blocks ensure a single blurred edge
/// cannot be treated as hundreds of independent pixel observations.
pub fn score_full_resolution(
    scene: &VectorScene,
    observed: &vice_image::CanonicalImage,
    render: &PartitionRender,
    cfg: BlockLikelihoodConfig,
    priors: PriorCodeLengths,
) -> Result<ScoreBreakdown, LikelihoodError> {
    score_full_resolution_scope(scene, observed, render, cfg, priors, ScoreScope::FULL)
}

/// Score the globally aligned correlation blocks intersecting an ROI plus its
/// declared dependency halo. The block grid is never recut at the ROI edge, so
/// parent and child remain comparable to each other and to a later full check.
pub fn score_full_resolution_scope(
    scene: &VectorScene,
    observed: &vice_image::CanonicalImage,
    render: &PartitionRender,
    cfg: BlockLikelihoodConfig,
    priors: PriorCodeLengths,
    scope: ScoreScope,
) -> Result<ScoreBreakdown, LikelihoodError> {
    let observation = ObservationTensor::of(observed, scene.formation.blend_space);
    score_full_resolution_scope_with_tensor(scene, &observation, render, cfg, priors, scope)
}

/// Cached-observation counterpart of [`score_full_resolution_scope`].
///
/// Trust-region evaluation scores the same immutable observation many times.
/// Carrying its tensor across those evaluations avoids repeatedly allocating
/// two full-resolution four-channel arrays without changing any arithmetic.
pub fn score_full_resolution_scope_with_tensor(
    scene: &VectorScene,
    observation: &ObservationTensor,
    render: &PartitionRender,
    cfg: BlockLikelihoodConfig,
    priors: PriorCodeLengths,
    scope: ScoreScope,
) -> Result<ScoreBreakdown, LikelihoodError> {
    score_full_resolution_scope_with_workspace(
        scene,
        observation,
        render,
        cfg,
        priors,
        scope,
        &mut LikelihoodWorkspace::default(),
    )
}

/// Reuses both full-resolution scratch arrays as well as the observation.
#[allow(clippy::too_many_arguments)]
pub fn score_full_resolution_scope_with_workspace(
    scene: &VectorScene,
    observation: &ObservationTensor,
    render: &PartitionRender,
    cfg: BlockLikelihoodConfig,
    priors: PriorCodeLengths,
    scope: ScoreScope,
    workspace: &mut LikelihoodWorkspace,
) -> Result<ScoreBreakdown, LikelihoodError> {
    cfg.validate()?;
    priors.validate()?;
    if !matches!(scene.formation.quantization, QuantizationModel::Uint8) {
        return Err(LikelihoodError::UnsupportedQuantization);
    }
    if observation.blend_space() != scene.formation.blend_space
        || scene.canvas.width_px != observation.width_px()
        || scene.canvas.height_px != observation.height_px()
        || render.width_px != observation.width_px()
        || render.height_px != observation.height_px()
        || render.composite.len() != observation.len()
    {
        return Err(LikelihoodError::DimensionMismatch);
    }
    if render.face_coverage.len() != scene.graph.faces.len()
        || render
            .face_coverage
            .iter()
            .any(|v| v.len() != observation.len())
    {
        return Err(LikelihoodError::FaceCoverageMismatch);
    }

    predicted_observation(scene, render, &mut workspace.predicted);
    score_prediction(
        observation,
        workspace,
        cfg,
        priors,
        PredictionSource::CertifiedInternalPartition,
        scope,
    )
}

/// Score the actual independently rendered serialized SVG bytes. The input is
/// premultiplied sRGB8 exactly as produced by the delivery renderer; it is
/// transformed into the selected formation's observation space without
/// un-premultiplying transparent RGB.
pub fn score_serialized_full_resolution(
    scene: &VectorScene,
    observed: &vice_image::CanonicalImage,
    premultiplied_srgb8: &[u8],
    width_px: u32,
    height_px: u32,
    cfg: BlockLikelihoodConfig,
    priors: PriorCodeLengths,
) -> Result<ScoreBreakdown, LikelihoodError> {
    score_serialized_full_resolution_scope(
        scene,
        observed,
        premultiplied_srgb8,
        width_px,
        height_px,
        cfg,
        priors,
        ScoreScope::FULL,
    )
}

/// Serialized-delivery counterpart of [`score_full_resolution_scope`].
#[allow(clippy::too_many_arguments)]
pub fn score_serialized_full_resolution_scope(
    scene: &VectorScene,
    observed: &vice_image::CanonicalImage,
    premultiplied_srgb8: &[u8],
    width_px: u32,
    height_px: u32,
    cfg: BlockLikelihoodConfig,
    priors: PriorCodeLengths,
    scope: ScoreScope,
) -> Result<ScoreBreakdown, LikelihoodError> {
    let observation = ObservationTensor::of(observed, scene.formation.blend_space);
    score_serialized_full_resolution_scope_with_tensor(
        scene,
        &observation,
        premultiplied_srgb8,
        width_px,
        height_px,
        cfg,
        priors,
        scope,
    )
}

/// Cached-observation counterpart of [`score_serialized_full_resolution_scope`].
#[allow(clippy::too_many_arguments)]
pub fn score_serialized_full_resolution_scope_with_tensor(
    scene: &VectorScene,
    observation: &ObservationTensor,
    premultiplied_srgb8: &[u8],
    width_px: u32,
    height_px: u32,
    cfg: BlockLikelihoodConfig,
    priors: PriorCodeLengths,
    scope: ScoreScope,
) -> Result<ScoreBreakdown, LikelihoodError> {
    score_serialized_full_resolution_scope_with_workspace(
        scene,
        observation,
        premultiplied_srgb8,
        width_px,
        height_px,
        cfg,
        priors,
        scope,
        &mut LikelihoodWorkspace::default(),
    )
}

/// Reuses both full-resolution scratch arrays as well as the observation.
#[allow(clippy::too_many_arguments)]
pub fn score_serialized_full_resolution_scope_with_workspace(
    scene: &VectorScene,
    observation: &ObservationTensor,
    premultiplied_srgb8: &[u8],
    width_px: u32,
    height_px: u32,
    cfg: BlockLikelihoodConfig,
    priors: PriorCodeLengths,
    scope: ScoreScope,
    workspace: &mut LikelihoodWorkspace,
) -> Result<ScoreBreakdown, LikelihoodError> {
    cfg.validate()?;
    priors.validate()?;
    if !matches!(scene.formation.quantization, QuantizationModel::Uint8) {
        return Err(LikelihoodError::UnsupportedQuantization);
    }
    if observation.blend_space() != scene.formation.blend_space
        || scene.canvas.width_px != observation.width_px()
        || scene.canvas.height_px != observation.height_px()
        || width_px != observation.width_px()
        || height_px != observation.height_px()
        || premultiplied_srgb8.len() != observation.len() * 4
    {
        return Err(LikelihoodError::DimensionMismatch);
    }
    serialized_prediction(
        premultiplied_srgb8,
        scene.formation.blend_space,
        &mut workspace.predicted,
    );
    score_prediction(
        observation,
        workspace,
        cfg,
        priors,
        PredictionSource::SerializedSvgRender,
        scope,
    )
}

#[derive(Debug, Clone, Copy)]
struct ScoreWindow {
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
}

fn score_window(
    scope: ScoreScope,
    width: usize,
    height: usize,
) -> Result<ScoreWindow, LikelihoodError> {
    if scope == ScoreScope::FULL {
        return Ok(ScoreWindow {
            x0: 0,
            y0: 0,
            x1: width,
            y1: height,
        });
    }
    let Some(roi) = scope.roi else {
        return Err(LikelihoodError::InvalidScoreScope);
    };
    if scope.global
        || scope.halo_px == 0
        || roi.x0 >= roi.x1
        || roi.y0 >= roi.y1
        || roi.x1 as usize > width
        || roi.y1 as usize > height
    {
        return Err(LikelihoodError::InvalidScoreScope);
    }
    let halo = scope.halo_px as usize;
    Ok(ScoreWindow {
        x0: (roi.x0 as usize).saturating_sub(halo),
        y0: (roi.y0 as usize).saturating_sub(halo),
        x1: (roi.x1 as usize).saturating_add(halo).min(width),
        y1: (roi.y1 as usize).saturating_add(halo).min(height),
    })
}

fn score_prediction(
    tensor: &ObservationTensor,
    workspace: &mut LikelihoodWorkspace,
    cfg: BlockLikelihoodConfig,
    priors: PriorCodeLengths,
    prediction_source: PredictionSource,
    scope: ScoreScope,
) -> Result<ScoreBreakdown, LikelihoodError> {
    if workspace.predicted.len() != tensor.len() {
        return Err(LikelihoodError::DimensionMismatch);
    }
    workspace.residual.clear();
    workspace.residual.resize(tensor.len(), [0.0; 4]);
    let mut deadzone = 0u64;
    for i in 0..tensor.len() {
        let obs = tensor.premul(i);
        let q = tensor.quantization_halfwidth(i);
        for ch in 0..4 {
            let raw = obs[ch] - workspace.predicted[i][ch];
            let outside = (raw.abs() - q[ch]).max(0.0);
            workspace.residual[i][ch] = raw.signum() * outside;
            if outside == 0.0 {
                deadzone += 1;
            }
        }
    }
    let residual = &workspace.residual;

    let (w, h) = (tensor.width_px() as usize, tensor.height_px() as usize);
    let window = score_window(scope, w, h)?;
    let block = cfg.block_size_px as usize;
    let mut pixel_bits = 0.0;
    let mut blocks = 0u64;
    for y0 in (0..h).step_by(block) {
        for x0 in (0..w).step_by(block) {
            let y1 = (y0 + block).min(h);
            let x1 = (x0 + block).min(w);
            if x0 >= window.x1 || x1 <= window.x0 || y0 >= window.y1 || y1 <= window.y0 {
                continue;
            }
            let count = ((y1 - y0) * (x1 - x0)) as f64;
            for (ch, sigma) in cfg.sigma_by_channel.iter().enumerate() {
                let mut energy = 0.0;
                for y in y0..y1 {
                    for x in x0..x1 {
                        energy += residual[y * w + x][ch].powi(2);
                    }
                }
                let z2 = energy / count / sigma.powi(2);
                pixel_bits += robust_bits(z2, cfg.student_t_degrees_of_freedom);
            }
            blocks += 1;
        }
    }
    let mut iid_bits = 0.0;
    for r in residual {
        for (value, sigma) in r.iter().zip(cfg.sigma_by_channel) {
            let z2 = value.powi(2) / sigma.powi(2);
            iid_bits += robust_bits(z2, cfg.student_t_degrees_of_freedom);
        }
    }
    let lag1_x = lag1(residual, w, h, 1, 0);
    let lag1_y = lag1(residual, w, h, 0, 1);
    let empirical = correlation_length(lag1_x.abs().max(lag1_y.abs()));
    let total_bits = pixel_bits
        + priors.topology_bits
        + priors.geometry_bits
        + priors.paint_bits
        + priors.relation_bits
        + priors.formation_bits;
    if !total_bits.is_finite() {
        return Err(LikelihoodError::NonFiniteScore);
    }
    Ok(ScoreBreakdown {
        ownership: ScoreOwnership::FullResolutionObservationOnly,
        pixel_bits,
        topology_bits: priors.topology_bits,
        geometry_bits: priors.geometry_bits,
        paint_bits: priors.paint_bits,
        relation_bits: priors.relation_bits,
        formation_bits: priors.formation_bits,
        total_bits,
        diagnostics: LikelihoodDiagnostics {
            residual_model_id: cfg.residual_model_id,
            prediction_source,
            source_sha256: tensor.source_sha256().to_owned(),
            calibrated_correlation_support_px: cfg.calibrated_correlation_support_px,
            empirical_correlation_length_px: empirical,
            lag1_x,
            lag1_y,
            block_size_px: cfg.block_size_px,
            blocks,
            quantization_deadzone_components: deadzone,
            iid_pixel_diagnostic_bits: iid_bits,
        },
    })
}

#[cfg(test)]
mod tests;
