//! M9 global formation expansion: resize history, broader PSF estimation,
//! and codec-residual identity. Nothing in this module is edge-indexed.

use serde::Serialize;
use vice_image::EncodedImageFormat;
use vice_ir::{
    BlendSpace, CodecResidualModel, ExteriorModel, GlobalFormationHypothesis, PixelFilter,
    QuantizationModel, ResizeChain,
};

use crate::boundary::contour_length_px;
use crate::formation::{resolved_fraction, transition_width_px, MIN_RESOLVED_FRACTION};
use crate::mixture::Flat2Evidence;

pub const M9_FORMATION_SCHEMA: &str = "vice-classic/formation-m9/v1";
pub const M9_GAUSSIAN_SIGMAS_PX: &[f64] = &[0.35, 0.5, 0.75, 1.0, 1.5, 2.0];
pub const M9_GAUSSIAN_WIDTH_PER_SIGMA: f64 = 2.38;
pub const M9_KERNEL_SCORE_MARGIN: f64 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct M9KernelProfile {
    pub sigma_px: f64,
    pub width_px: f64,
    /// Empirical development-population spread, with the M4 0.04 px
    /// instrument floor retained when a wider profile has fewer resolved
    /// arms. The floor prevents undersampling from making M9 overconfident.
    pub sd_px: f64,
}

pub const M9_KERNEL_PROFILES_V1: &[M9KernelProfile] = &[
    M9KernelProfile {
        sigma_px: 0.35,
        width_px: 0.936,
        sd_px: 0.100,
    },
    M9KernelProfile {
        sigma_px: 0.5,
        width_px: 1.197,
        sd_px: 0.039,
    },
    M9KernelProfile {
        sigma_px: 0.75,
        width_px: 1.760,
        sd_px: 0.040,
    },
    M9KernelProfile {
        sigma_px: 1.0,
        width_px: 2.370,
        sd_px: 0.040,
    },
    M9KernelProfile {
        sigma_px: 1.5,
        width_px: 3.502,
        sd_px: 0.046,
    },
    M9KernelProfile {
        sigma_px: 2.0,
        width_px: 4.618,
        sd_px: 0.040,
    },
];

pub fn codec_residual_for_format(format: EncodedImageFormat) -> CodecResidualModel {
    match format {
        EncodedImageFormat::Jpeg => CodecResidualModel::JpegDct8x8,
        EncodedImageFormat::WebpLossy => CodecResidualModel::WebpTransform4x4,
        EncodedImageFormat::RawRgba8
        | EncodedImageFormat::Png
        | EncodedImageFormat::WebpLossless => CodecResidualModel::CleanCorrelation,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ExtendedFormationHypothesis {
    pub base: GlobalFormationHypothesis,
    pub resize_chain: ResizeChain,
    pub codec_residual: CodecResidualModel,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct KernelCandidate {
    pub filter: PixelFilter,
    pub predicted_width_px: f64,
    pub standardized_penalty: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GlobalKernelEstimate {
    pub schema: &'static str,
    pub transition_width_px: f64,
    pub contour_length_px: f64,
    pub resolved_fraction: f64,
    pub candidates: Vec<KernelCandidate>,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum KernelEstimationError {
    #[error(
        "kernel is not identifiable because the foreground is unresolved ({resolved_fraction})"
    )]
    Unresolved { resolved_fraction: f64 },
    #[error("kernel evidence has malformed dimensions or contour")]
    Malformed,
}

pub fn formation_m9_id(formation: &ExtendedFormationHypothesis) -> String {
    let resize = match formation.resize_chain {
        ResizeChain::None => "native",
        ResizeChain::DownFrom2x => "down2x",
        ResizeChain::UpFromHalf => "uphalf",
    };
    let codec = match formation.codec_residual {
        CodecResidualModel::CleanCorrelation => "clean",
        CodecResidualModel::JpegDct8x8 => "jpeg-dct8",
        CodecResidualModel::WebpTransform4x4 => "webp-t4",
    };
    format!(
        "{}/{resize}/{codec}",
        crate::formation::formation_id(&formation.base)
    )
}

pub fn enumerate_m9(
    exterior: ExteriorModel,
    format: EncodedImageFormat,
) -> Vec<ExtendedFormationHypothesis> {
    let filters = std::iter::once(PixelFilter::Box)
        .chain(std::iter::once(PixelFilter::Triangle))
        .chain(
            M9_GAUSSIAN_SIGMAS_PX
                .iter()
                .copied()
                .map(|sigma_px| PixelFilter::Gaussian { sigma_px }),
        )
        .collect::<Vec<_>>();
    let mut hypotheses = Vec::new();
    for blend_space in [BlendSpace::LinearLight, BlendSpace::EncodedSrgb] {
        for pixel_filter in &filters {
            for resize_chain in ResizeChain::ALL {
                hypotheses.push(ExtendedFormationHypothesis {
                    base: GlobalFormationHypothesis {
                        blend_space,
                        pixel_filter: *pixel_filter,
                        quantization: QuantizationModel::Uint8,
                        exterior,
                    },
                    resize_chain,
                    codec_residual: codec_residual_for_format(format),
                });
            }
        }
    }
    hypotheses
}

pub fn estimate_global_kernel(
    evidence: &Flat2Evidence,
) -> Result<GlobalKernelEstimate, KernelEstimationError> {
    estimate_global_kernel_from_alpha(
        evidence.alpha_field(),
        evidence.width_px() as usize,
        evidence.height_px() as usize,
    )
}

pub fn estimate_global_kernel_from_alpha(
    alpha: &[f64],
    width: usize,
    height: usize,
) -> Result<GlobalKernelEstimate, KernelEstimationError> {
    if width == 0
        || height == 0
        || alpha.len() != width * height
        || alpha.iter().any(|value| !value.is_finite())
    {
        return Err(KernelEstimationError::Malformed);
    }
    let resolved = resolved_fraction(alpha);
    if resolved < MIN_RESOLVED_FRACTION {
        return Err(KernelEstimationError::Unresolved {
            resolved_fraction: resolved,
        });
    }
    let contour = contour_length_px(alpha, width, height, 0.5);
    if !contour.is_finite() || contour <= 0.0 {
        return Err(KernelEstimationError::Malformed);
    }
    let observed = transition_width_px(alpha, contour);
    let mut all = vec![
        candidate(PixelFilter::Box, 0.797, 0.187, observed),
        candidate(PixelFilter::Triangle, 1.020, 0.046, observed),
    ];
    all.extend(M9_KERNEL_PROFILES_V1.iter().map(|profile| {
        candidate(
            PixelFilter::Gaussian {
                sigma_px: profile.sigma_px,
            },
            profile.width_px,
            profile.sd_px,
            observed,
        )
    }));
    let best = all
        .iter()
        .map(|candidate| candidate.standardized_penalty)
        .fold(f64::INFINITY, f64::min);
    all.retain(|candidate| candidate.standardized_penalty <= best + M9_KERNEL_SCORE_MARGIN);
    Ok(GlobalKernelEstimate {
        schema: M9_FORMATION_SCHEMA,
        transition_width_px: observed,
        contour_length_px: contour,
        resolved_fraction: resolved,
        candidates: all,
    })
}

fn candidate(
    filter: PixelFilter,
    predicted_width_px: f64,
    sd_px: f64,
    observed_width_px: f64,
) -> KernelCandidate {
    let z = (observed_width_px - predicted_width_px) / sd_px;
    KernelCandidate {
        filter,
        predicted_width_px,
        standardized_penalty: 0.5 * z * z,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vertical_gaussian(sigma: f64) -> (Vec<f64>, usize, usize) {
        let (width, height) = (64, 64);
        let alpha = (0..height)
            .flat_map(|_| {
                (0..width).map(move |x| {
                    let d = x as f64 + 0.5 - 32.0;
                    0.5 * (1.0 + erf(d / (sigma * std::f64::consts::SQRT_2)))
                })
            })
            .collect();
        (alpha, width, height)
    }

    #[test]
    fn broader_global_kernel_is_estimated_without_an_edge_index() {
        let (alpha, width, height) = vertical_gaussian(1.5);
        let estimate = estimate_global_kernel_from_alpha(&alpha, width, height).unwrap();
        assert!(estimate
            .candidates
            .iter()
            .any(|candidate| { candidate.filter == PixelFilter::Gaussian { sigma_px: 1.5 } }));
        assert!(estimate.resolved_fraction >= MIN_RESOLVED_FRACTION);
    }

    #[test]
    fn codec_identity_and_resize_family_are_total_and_deterministic() {
        let hypotheses = enumerate_m9(ExteriorModel::Opaque, EncodedImageFormat::Jpeg);
        assert_eq!(hypotheses.len(), 2 * 8 * 3);
        assert!(hypotheses
            .iter()
            .all(|hypothesis| hypothesis.codec_residual == CodecResidualModel::JpegDct8x8));
        let ids = hypotheses
            .iter()
            .map(formation_m9_id)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(ids.len(), hypotheses.len());
    }

    #[test]
    fn unresolved_shapes_refuse_kernel_estimation() {
        assert!(matches!(
            estimate_global_kernel_from_alpha(&[0.5; 16], 4, 4),
            Err(KernelEstimationError::Unresolved { .. })
        ));
    }

    fn erf(x: f64) -> f64 {
        let sign = if x < 0.0 { -1.0 } else { 1.0 };
        let x = x.abs();
        let t = 1.0 / (1.0 + 0.327_591_1 * x);
        let y = 1.0
            - (((((1.061_405_429 * t - 1.453_152_027) * t) + 1.421_413_741) * t - 0.284_496_736)
                * t
                + 0.254_829_592)
                * t
                * (-x * x).exp();
        sign * y
    }
}
