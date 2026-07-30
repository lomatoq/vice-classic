//! Full-resolution, correlation-aware observation likelihood (spec §17).

use serde::Serialize;
use thiserror::Error;
use vice_image::ObservationTensor;
use vice_ir::{BlendSpace, Paint, QuantizationModel, VectorScene};
use vice_render::PartitionRender;

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
}

fn predicted_observation(scene: &VectorScene, render: &PartitionRender) -> Vec<[f64; 4]> {
    let n = render.width_px as usize * render.height_px as usize;
    let mut predicted = vec![[0.0; 4]; n];
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
    predicted
}

fn serialized_prediction(bytes: &[u8], blend_space: BlendSpace) -> Vec<[f64; 4]> {
    bytes
        .chunks_exact(4)
        .map(|pixel| {
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
        })
        .collect()
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
    cfg.validate()?;
    priors.validate()?;
    if !matches!(scene.formation.quantization, QuantizationModel::Uint8) {
        return Err(LikelihoodError::UnsupportedQuantization);
    }
    if scene.canvas.width_px != observed.width_px()
        || scene.canvas.height_px != observed.height_px()
        || render.width_px != observed.width_px()
        || render.height_px != observed.height_px()
        || render.composite.len() != observed.pixel_count()
    {
        return Err(LikelihoodError::DimensionMismatch);
    }
    if render.face_coverage.len() != scene.graph.faces.len()
        || render
            .face_coverage
            .iter()
            .any(|v| v.len() != observed.pixel_count())
    {
        return Err(LikelihoodError::FaceCoverageMismatch);
    }

    score_prediction(
        scene,
        observed,
        predicted_observation(scene, render),
        cfg,
        priors,
        PredictionSource::CertifiedInternalPartition,
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
    cfg.validate()?;
    priors.validate()?;
    if !matches!(scene.formation.quantization, QuantizationModel::Uint8) {
        return Err(LikelihoodError::UnsupportedQuantization);
    }
    if scene.canvas.width_px != observed.width_px()
        || scene.canvas.height_px != observed.height_px()
        || width_px != observed.width_px()
        || height_px != observed.height_px()
        || premultiplied_srgb8.len() != observed.pixel_count() * 4
    {
        return Err(LikelihoodError::DimensionMismatch);
    }
    score_prediction(
        scene,
        observed,
        serialized_prediction(premultiplied_srgb8, scene.formation.blend_space),
        cfg,
        priors,
        PredictionSource::SerializedSvgRender,
    )
}

fn score_prediction(
    scene: &VectorScene,
    observed: &vice_image::CanonicalImage,
    predicted: Vec<[f64; 4]>,
    cfg: BlockLikelihoodConfig,
    priors: PriorCodeLengths,
    prediction_source: PredictionSource,
) -> Result<ScoreBreakdown, LikelihoodError> {
    let tensor = ObservationTensor::of(observed, scene.formation.blend_space);
    if predicted.len() != tensor.len() {
        return Err(LikelihoodError::DimensionMismatch);
    }
    let mut residual = vec![[0.0; 4]; tensor.len()];
    let mut deadzone = 0u64;
    for i in 0..tensor.len() {
        let obs = tensor.premul(i);
        let q = tensor.quantization_halfwidth(i);
        for ch in 0..4 {
            let raw = obs[ch] - predicted[i][ch];
            let outside = (raw.abs() - q[ch]).max(0.0);
            residual[i][ch] = raw.signum() * outside;
            if outside == 0.0 {
                deadzone += 1;
            }
        }
    }

    let (w, h) = (observed.width_px() as usize, observed.height_px() as usize);
    let block = cfg.block_size_px as usize;
    let mut pixel_bits = 0.0;
    let mut blocks = 0u64;
    for y0 in (0..h).step_by(block) {
        for x0 in (0..w).step_by(block) {
            let y1 = (y0 + block).min(h);
            let x1 = (x0 + block).min(w);
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
    for r in &residual {
        for (value, sigma) in r.iter().zip(cfg.sigma_by_channel) {
            let z2 = value.powi(2) / sigma.powi(2);
            iid_bits += robust_bits(z2, cfg.student_t_degrees_of_freedom);
        }
    }
    let lag1_x = lag1(&residual, w, h, 1, 0);
    let lag1_y = lag1(&residual, w, h, 0, 1);
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
            source_sha256: observed.source_sha256().to_owned(),
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
mod tests {
    use super::*;
    use vice_image::IccAssumption;
    use vice_ir::color::PremulRgba;
    use vice_ir::{
        BlendSpace, Canvas, ExteriorModel, GlobalFormationHypothesis, PixelFilter, PlanarGraph,
    };

    fn scene(w: u32, h: u32) -> VectorScene {
        VectorScene {
            canvas: Canvas {
                width_px: w,
                height_px: h,
            },
            graph: PlanarGraph::empty(),
            formation: GlobalFormationHypothesis {
                blend_space: BlendSpace::LinearLight,
                pixel_filter: PixelFilter::Box,
                quantization: QuantizationModel::Uint8,
                exterior: ExteriorModel::Transparent,
            },
        }
    }

    fn cfg() -> BlockLikelihoodConfig {
        BlockLikelihoodConfig::new(2, 2.0, [0.01; 4], 4.0).unwrap()
    }

    fn priors() -> PriorCodeLengths {
        PriorCodeLengths {
            topology_bits: 1.0,
            geometry_bits: 2.0,
            paint_bits: 3.0,
            relation_bits: -0.5,
            formation_bits: 1.0,
        }
    }

    #[test]
    fn exact_quantized_match_charges_no_pixel_bits_and_iid_is_diagnostic_only() {
        let image = vice_image::CanonicalImage::from_straight_srgb8(
            2,
            2,
            vec![0; 16],
            true,
            IccAssumption::SrgbChunkDeclared,
        )
        .unwrap();
        let render = PartitionRender {
            width_px: 2,
            height_px: 2,
            face_coverage: vec![vec![1.0; 4]],
            composite: vec![
                PremulRgba {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.0,
                };
                4
            ],
        };
        let got = score_full_resolution(&scene(2, 2), &image, &render, cfg(), priors()).unwrap();
        assert_eq!(got.pixel_bits, 0.0);
        assert_eq!(got.total_bits, 6.5);
        assert_eq!(got.diagnostics.iid_pixel_diagnostic_bits, 0.0);
    }

    #[test]
    fn serialized_svg_bytes_are_the_final_likelihood_prediction() {
        let image = vice_image::CanonicalImage::from_straight_srgb8(
            2,
            2,
            vec![0; 16],
            true,
            IccAssumption::SrgbChunkDeclared,
        )
        .unwrap();
        let got =
            score_serialized_full_resolution(&scene(2, 2), &image, &[0; 16], 2, 2, cfg(), priors())
                .unwrap();
        assert_eq!(got.pixel_bits, 0.0);
        assert_eq!(
            got.diagnostics.prediction_source,
            PredictionSource::SerializedSvgRender
        );
    }

    #[test]
    fn one_constant_error_is_charged_once_per_correlation_block_not_per_pixel() {
        let image = vice_image::CanonicalImage::from_straight_srgb8(
            4,
            2,
            vec![255; 32],
            true,
            IccAssumption::SrgbChunkDeclared,
        )
        .unwrap();
        let render = PartitionRender {
            width_px: 4,
            height_px: 2,
            face_coverage: vec![vec![1.0; 8]],
            composite: vec![
                PremulRgba {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.0,
                };
                8
            ],
        };
        let got = score_full_resolution(&scene(4, 2), &image, &render, cfg(), priors()).unwrap();
        assert_eq!(got.diagnostics.blocks, 2);
        assert!(got.pixel_bits > 0.0);
        assert!(got.diagnostics.iid_pixel_diagnostic_bits > got.pixel_bits);
    }

    #[test]
    fn block_below_calibrated_support_is_refused() {
        assert!(matches!(
            BlockLikelihoodConfig::new(2, 2.1, [0.01; 4], 4.0),
            Err(LikelihoodError::CorrelationSupport { .. })
        ));
    }
}
