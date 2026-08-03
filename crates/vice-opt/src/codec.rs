//! M9 transform-aware JPEG/WebP residual likelihood.

use serde::Serialize;
use sha2::{Digest, Sha256};
use vice_image::{CanonicalImage, EncodedImageFormat};
use vice_ir::CodecResidualModel;

pub const CODEC_LIKELIHOOD_SCHEMA: &str = "vice-classic/codec-likelihood/v1";

pub const CLEAN_CODEC_LIKELIHOOD_CONFIG_V1: CodecLikelihoodConfig = CodecLikelihoodConfig {
    schema: CODEC_LIKELIHOOD_SCHEMA,
    model: CodecResidualModel::CleanCorrelation,
    dc_sigma_codes: 25.57,
    ac_sigma_codes: 25.57,
    alpha_sigma_codes: 25.57,
    student_t_degrees_of_freedom: 4.0,
};

pub const JPEG_CODEC_LIKELIHOOD_CONFIG_V1: CodecLikelihoodConfig = CodecLikelihoodConfig {
    schema: CODEC_LIKELIHOOD_SCHEMA,
    model: CodecResidualModel::JpegDct8x8,
    dc_sigma_codes: 32.37,
    ac_sigma_codes: 14.84,
    alpha_sigma_codes: 1.0,
    student_t_degrees_of_freedom: 4.0,
};

pub const WEBP_CODEC_LIKELIHOOD_CONFIG_V1: CodecLikelihoodConfig = CodecLikelihoodConfig {
    schema: CODEC_LIKELIHOOD_SCHEMA,
    model: CodecResidualModel::WebpTransform4x4,
    dc_sigma_codes: 44.17,
    ac_sigma_codes: 13.45,
    alpha_sigma_codes: 1.0,
    student_t_degrees_of_freedom: 4.0,
};

pub fn calibrated_codec_likelihood_config(format: EncodedImageFormat) -> CodecLikelihoodConfig {
    match format {
        EncodedImageFormat::Jpeg => JPEG_CODEC_LIKELIHOOD_CONFIG_V1,
        EncodedImageFormat::WebpLossy => WEBP_CODEC_LIKELIHOOD_CONFIG_V1,
        EncodedImageFormat::RawRgba8
        | EncodedImageFormat::Png
        | EncodedImageFormat::WebpLossless => CLEAN_CODEC_LIKELIHOOD_CONFIG_V1,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct CodecLikelihoodConfig {
    pub schema: &'static str,
    pub model: CodecResidualModel,
    pub dc_sigma_codes: f64,
    pub ac_sigma_codes: f64,
    pub alpha_sigma_codes: f64,
    pub student_t_degrees_of_freedom: f64,
}

impl CodecLikelihoodConfig {
    pub fn new(
        model: CodecResidualModel,
        dc_sigma_codes: f64,
        ac_sigma_codes: f64,
        alpha_sigma_codes: f64,
        student_t_degrees_of_freedom: f64,
    ) -> Result<Self, CodecLikelihoodError> {
        let config = Self {
            schema: CODEC_LIKELIHOOD_SCHEMA,
            model,
            dc_sigma_codes,
            ac_sigma_codes,
            alpha_sigma_codes,
            student_t_degrees_of_freedom,
        };
        config.validate()?;
        Ok(config)
    }

    fn validate(self) -> Result<(), CodecLikelihoodError> {
        if self.schema != CODEC_LIKELIHOOD_SCHEMA
            || [
                self.dc_sigma_codes,
                self.ac_sigma_codes,
                self.alpha_sigma_codes,
            ]
            .iter()
            .any(|sigma| !sigma.is_finite() || *sigma <= 0.0)
            || !self.student_t_degrees_of_freedom.is_finite()
            || self.student_t_degrees_of_freedom <= 2.0
        {
            Err(CodecLikelihoodError::InvalidConfig)
        } else {
            Ok(())
        }
    }

    pub fn digest_sha256(self) -> String {
        hex::encode(Sha256::digest(
            serde_json::to_vec(&self).expect("typed codec config serializes"),
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CodecLikelihoodReport {
    pub schema: &'static str,
    pub source_sha256: String,
    pub source_format: EncodedImageFormat,
    pub residual_model: CodecResidualModel,
    pub config_sha256: String,
    pub block_size_px: u32,
    pub blocks: u64,
    pub transform_coefficients: u64,
    pub total_bits: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CodecResidualCalibrationStats {
    pub model: CodecResidualModel,
    pub blocks: u64,
    pub dc_rms_codes: f64,
    pub ac_rms_codes: f64,
    pub alpha_rms_codes: f64,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CodecLikelihoodError {
    #[error("codec likelihood config is invalid")]
    InvalidConfig,
    #[error("prediction dimensions do not match the observation")]
    DimensionMismatch,
    #[error("residual model {model:?} does not match source format {format:?}")]
    ModelFormatMismatch {
        model: CodecResidualModel,
        format: EncodedImageFormat,
    },
    #[error("codec likelihood produced a non-finite score")]
    NonFinite,
}

pub fn score_codec_residual(
    observed: &CanonicalImage,
    predicted_straight_srgb8: &[u8],
    config: CodecLikelihoodConfig,
) -> Result<CodecLikelihoodReport, CodecLikelihoodError> {
    config.validate()?;
    if predicted_straight_srgb8.len() != observed.pixel_count() * 4 {
        return Err(CodecLikelihoodError::DimensionMismatch);
    }
    let expected = match observed.encoded_format() {
        EncodedImageFormat::Jpeg => CodecResidualModel::JpegDct8x8,
        EncodedImageFormat::WebpLossy => CodecResidualModel::WebpTransform4x4,
        EncodedImageFormat::RawRgba8
        | EncodedImageFormat::Png
        | EncodedImageFormat::WebpLossless => CodecResidualModel::CleanCorrelation,
    };
    if config.model != expected {
        return Err(CodecLikelihoodError::ModelFormatMismatch {
            model: config.model,
            format: observed.encoded_format(),
        });
    }
    let block_size = match config.model {
        CodecResidualModel::CleanCorrelation => 2,
        CodecResidualModel::JpegDct8x8 => 8,
        CodecResidualModel::WebpTransform4x4 => 4,
    };
    let width = observed.width_px() as usize;
    let height = observed.height_px() as usize;
    let blocks_x = width.div_ceil(block_size);
    let blocks_y = height.div_ceil(block_size);
    let mut total_bits = 0.0;
    let mut coefficients = 0u64;
    for by in 0..blocks_y {
        for bx in 0..blocks_x {
            for channel in 0..4 {
                let residual = residual_block(
                    observed.straight_srgb8(),
                    predicted_straight_srgb8,
                    width,
                    height,
                    bx * block_size,
                    by * block_size,
                    block_size,
                    channel,
                );
                let transformed = match config.model {
                    CodecResidualModel::CleanCorrelation => residual,
                    CodecResidualModel::JpegDct8x8 => dct_2d(&residual, 8),
                    CodecResidualModel::WebpTransform4x4 => hadamard_4x4(&residual),
                };
                for (index, coefficient) in transformed.into_iter().enumerate() {
                    let sigma = if channel == 3 {
                        config.alpha_sigma_codes
                    } else if index == 0 {
                        config.dc_sigma_codes
                    } else {
                        config.ac_sigma_codes
                    };
                    total_bits +=
                        robust_bits(coefficient / sigma, config.student_t_degrees_of_freedom);
                    coefficients += 1;
                }
            }
        }
    }
    if !total_bits.is_finite() {
        return Err(CodecLikelihoodError::NonFinite);
    }
    Ok(CodecLikelihoodReport {
        schema: CODEC_LIKELIHOOD_SCHEMA,
        source_sha256: observed.source_sha256().into(),
        source_format: observed.encoded_format(),
        residual_model: config.model,
        config_sha256: config.digest_sha256(),
        block_size_px: block_size as u32,
        blocks: (blocks_x * blocks_y) as u64,
        transform_coefficients: coefficients,
        total_bits,
    })
}

pub fn measure_codec_residual(
    observed: &CanonicalImage,
    predicted_straight_srgb8: &[u8],
) -> Result<CodecResidualCalibrationStats, CodecLikelihoodError> {
    if predicted_straight_srgb8.len() != observed.pixel_count() * 4 {
        return Err(CodecLikelihoodError::DimensionMismatch);
    }
    let model = match observed.encoded_format() {
        EncodedImageFormat::Jpeg => CodecResidualModel::JpegDct8x8,
        EncodedImageFormat::WebpLossy => CodecResidualModel::WebpTransform4x4,
        EncodedImageFormat::RawRgba8
        | EncodedImageFormat::Png
        | EncodedImageFormat::WebpLossless => CodecResidualModel::CleanCorrelation,
    };
    let block_size = match model {
        CodecResidualModel::CleanCorrelation => 2,
        CodecResidualModel::JpegDct8x8 => 8,
        CodecResidualModel::WebpTransform4x4 => 4,
    };
    let width = observed.width_px() as usize;
    let height = observed.height_px() as usize;
    let mut dc = Vec::new();
    let mut ac = Vec::new();
    let mut alpha = Vec::new();
    let mut blocks = 0u64;
    for by in 0..height.div_ceil(block_size) {
        for bx in 0..width.div_ceil(block_size) {
            blocks += 1;
            for channel in 0..4 {
                let residual = residual_block(
                    observed.straight_srgb8(),
                    predicted_straight_srgb8,
                    width,
                    height,
                    bx * block_size,
                    by * block_size,
                    block_size,
                    channel,
                );
                let transformed = match model {
                    CodecResidualModel::CleanCorrelation => residual,
                    CodecResidualModel::JpegDct8x8 => dct_2d(&residual, 8),
                    CodecResidualModel::WebpTransform4x4 => hadamard_4x4(&residual),
                };
                if channel == 3 {
                    alpha.extend(transformed);
                } else {
                    dc.push(transformed[0]);
                    ac.extend_from_slice(&transformed[1..]);
                }
            }
        }
    }
    Ok(CodecResidualCalibrationStats {
        model,
        blocks,
        dc_rms_codes: rms(&dc),
        ac_rms_codes: rms(&ac),
        alpha_rms_codes: rms(&alpha),
    })
}

fn rms(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        (values.iter().map(|value| value * value).sum::<f64>() / values.len() as f64).sqrt()
    }
}

#[allow(clippy::too_many_arguments)]
fn residual_block(
    observed: &[u8],
    predicted: &[u8],
    width: usize,
    height: usize,
    x0: usize,
    y0: usize,
    size: usize,
    channel: usize,
) -> Vec<f64> {
    let mut block = vec![0.0; size * size];
    for y in 0..size {
        for x in 0..size {
            let px = (x0 + x).min(width - 1);
            let py = (y0 + y).min(height - 1);
            let index = (py * width + px) * 4 + channel;
            block[y * size + x] = f64::from(observed[index]) - f64::from(predicted[index]);
        }
    }
    block
}

fn dct_2d(block: &[f64], size: usize) -> Vec<f64> {
    let mut output = vec![0.0; size * size];
    for v in 0..size {
        for u in 0..size {
            let cu = if u == 0 { 1.0 / 2.0f64.sqrt() } else { 1.0 };
            let cv = if v == 0 { 1.0 / 2.0f64.sqrt() } else { 1.0 };
            let mut sum = 0.0;
            for y in 0..size {
                for x in 0..size {
                    sum += block[y * size + x]
                        * ((2 * x + 1) as f64 * u as f64 * std::f64::consts::PI
                            / (2 * size) as f64)
                            .cos()
                        * ((2 * y + 1) as f64 * v as f64 * std::f64::consts::PI
                            / (2 * size) as f64)
                            .cos();
                }
            }
            output[v * size + u] = 2.0 / size as f64 * cu * cv * sum;
        }
    }
    output
}

fn hadamard_4x4(block: &[f64]) -> Vec<f64> {
    const H: [[f64; 4]; 4] = [
        [0.5, 0.5, 0.5, 0.5],
        [0.5, -0.5, 0.5, -0.5],
        [0.5, 0.5, -0.5, -0.5],
        [0.5, -0.5, -0.5, 0.5],
    ];
    let mut output = vec![0.0; 16];
    for v in 0..4 {
        for u in 0..4 {
            for y in 0..4 {
                for x in 0..4 {
                    output[v * 4 + u] += H[v][y] * block[y * 4 + x] * H[u][x];
                }
            }
        }
    }
    output
}

fn robust_bits(z: f64, degrees_of_freedom: f64) -> f64 {
    0.5 * (degrees_of_freedom + 1.0) * (1.0 + z * z / degrees_of_freedom).log2()
}

#[cfg(test)]
mod tests {
    use super::*;
    use vice_image::IccAssumption;

    fn raw(bytes: Vec<u8>) -> CanonicalImage {
        CanonicalImage::from_straight_srgb8(4, 4, bytes, true, IccAssumption::NoProfileAssumedSrgb)
            .unwrap()
    }

    #[test]
    fn exact_clean_prediction_costs_zero_and_is_bound_to_its_config() {
        let bytes = vec![127; 4 * 4 * 4];
        let image = raw(bytes.clone());
        let config =
            CodecLikelihoodConfig::new(CodecResidualModel::CleanCorrelation, 1.0, 1.0, 1.0, 4.0)
                .unwrap();
        let report = score_codec_residual(&image, &bytes, config).unwrap();
        assert_eq!(report.total_bits, 0.0);
        assert_eq!(report.block_size_px, 2);
        assert_eq!(report.config_sha256.len(), 64);
    }

    #[test]
    fn a_codec_model_cannot_be_applied_to_a_clean_source() {
        let image = raw(vec![0; 4 * 4 * 4]);
        let config =
            CodecLikelihoodConfig::new(CodecResidualModel::JpegDct8x8, 2.0, 4.0, 1.0, 4.0).unwrap();
        assert!(matches!(
            score_codec_residual(&image, image.straight_srgb8(), config),
            Err(CodecLikelihoodError::ModelFormatMismatch { .. })
        ));
    }

    #[test]
    fn transforms_are_orthonormal_on_constant_blocks() {
        let jpeg = dct_2d(&vec![3.0; 64], 8);
        assert!((jpeg[0] - 24.0).abs() < 1e-9);
        assert!(jpeg[1..].iter().all(|value| value.abs() < 1e-9));
        let webp = hadamard_4x4(&[3.0; 16]);
        assert!((webp[0] - 12.0).abs() < 1e-9);
        assert!(webp[1..].iter().all(|value| value.abs() < 1e-9));
    }
}
