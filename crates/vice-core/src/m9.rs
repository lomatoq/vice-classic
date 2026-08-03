//! M9 extended global formation API.

use serde::Serialize;
use sha2::{Digest, Sha256};
use vice_image::{CanonicalImage, DecodeLimits, EncodedImageFormat};
use vice_ir::{CodecResidualModel, ResizeChain, ValidatedScene};
use vice_opt::{
    score_codec_residual, CodecLikelihoodConfig, CodecLikelihoodError, CodecLikelihoodReport,
};

pub const M9_INSPECTION_SCHEMA: &str = "vice-classic/m9-formation-inspection/v1";
pub const M9_SCORE_SCHEMA: &str = "vice-classic/m9-formation-score/v1";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct M9FormationInspection {
    pub schema: &'static str,
    pub source_sha256: String,
    pub source_format: EncodedImageFormat,
    pub codec_residual: CodecResidualModel,
    pub global_kernel: Option<vice_evidence::GlobalKernelEstimate>,
    pub kernel_refusal: Option<String>,
    pub resize_chains: Vec<ResizeChain>,
    pub hypothesis_count: u64,
    pub hypothesis_ids_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct M9FormationScore {
    pub schema: &'static str,
    pub source_sha256: String,
    pub source_format: EncodedImageFormat,
    pub resize_chain: ResizeChain,
    pub pixel_filter: vice_ir::PixelFilter,
    pub formed_render_sha256: String,
    pub codec: CodecLikelihoodReport,
}

#[derive(Debug, thiserror::Error)]
pub enum M9FormationError {
    #[error(transparent)]
    Decode(#[from] vice_image::ImageError),
    #[error("M9 evidence did not select a formation: {0}")]
    Evidence(String),
    #[error(transparent)]
    Render(#[from] vice_render::FormationRenderError),
    #[error(transparent)]
    Codec(#[from] CodecLikelihoodError),
}

pub fn inspect_m9_formation(bytes: &[u8]) -> Result<M9FormationInspection, M9FormationError> {
    let image = CanonicalImage::decode(bytes, &DecodeLimits::default())?;
    let filters = std::iter::once(vice_ir::PixelFilter::Box)
        .chain(std::iter::once(vice_ir::PixelFilter::Triangle))
        .chain(
            vice_evidence::M9_GAUSSIAN_SIGMAS_PX
                .iter()
                .copied()
                .map(|sigma_px| vice_ir::PixelFilter::Gaussian { sigma_px }),
        )
        .collect::<Vec<_>>();
    let analysis = vice_evidence::analyze_full_for_filters(
        &image,
        &vice_evidence::ANALYSIS_CONFIG_V1,
        None,
        &filters,
    );
    let evidence = analysis
        .chosen
        .ok_or_else(|| M9FormationError::Evidence(format!("{:?}", analysis.report.outcome)))?;
    let (global_kernel, kernel_refusal) = match vice_evidence::estimate_global_kernel(&evidence) {
        Ok(estimate) => (Some(estimate), None),
        Err(error) => (None, Some(error.to_string())),
    };
    let hypotheses =
        vice_evidence::enumerate_m9(evidence.formation.exterior, image.encoded_format());
    let mut ids = hypotheses
        .iter()
        .map(vice_evidence::formation_m9_id)
        .collect::<Vec<_>>();
    ids.sort();
    Ok(M9FormationInspection {
        schema: M9_INSPECTION_SCHEMA,
        source_sha256: image.source_sha256().into(),
        source_format: image.encoded_format(),
        codec_residual: codec_model(image.encoded_format()),
        global_kernel,
        kernel_refusal,
        resize_chains: ResizeChain::ALL.into(),
        hypothesis_count: hypotheses.len() as u64,
        hypothesis_ids_sha256: hex::encode(Sha256::digest(ids.join("\n").as_bytes())),
    })
}

pub fn score_m9_formation(
    bytes: &[u8],
    scene: &ValidatedScene,
    resize_chain: ResizeChain,
    codec_config: CodecLikelihoodConfig,
) -> Result<M9FormationScore, M9FormationError> {
    let image = CanonicalImage::decode(bytes, &DecodeLimits::default())?;
    let render = vice_render::render_partition_formed(
        scene,
        &vice_render::RenderOptions::default(),
        resize_chain,
    )?;
    if render.width_px != image.width_px() || render.height_px != image.height_px() {
        return Err(M9FormationError::Codec(
            CodecLikelihoodError::DimensionMismatch,
        ));
    }
    let predicted = straight_srgb8(&render);
    let formed_render_sha256 = hex::encode(Sha256::digest(&predicted));
    let codec = score_codec_residual(&image, &predicted, codec_config)?;
    Ok(M9FormationScore {
        schema: M9_SCORE_SCHEMA,
        source_sha256: image.source_sha256().into(),
        source_format: image.encoded_format(),
        resize_chain,
        pixel_filter: scene.scene().formation.pixel_filter,
        formed_render_sha256,
        codec,
    })
}

pub fn score_m9_formation_calibrated(
    bytes: &[u8],
    scene: &ValidatedScene,
    resize_chain: ResizeChain,
) -> Result<M9FormationScore, M9FormationError> {
    let image = CanonicalImage::decode(bytes, &DecodeLimits::default())?;
    score_m9_formation(
        bytes,
        scene,
        resize_chain,
        vice_opt::calibrated_codec_likelihood_config(image.encoded_format()),
    )
}

fn codec_model(format: EncodedImageFormat) -> CodecResidualModel {
    match format {
        EncodedImageFormat::Jpeg => CodecResidualModel::JpegDct8x8,
        EncodedImageFormat::WebpLossy => CodecResidualModel::WebpTransform4x4,
        EncodedImageFormat::RawRgba8
        | EncodedImageFormat::Png
        | EncodedImageFormat::WebpLossless => CodecResidualModel::CleanCorrelation,
    }
}

fn straight_srgb8(render: &vice_render::PartitionRender) -> Vec<u8> {
    let mut output = Vec::with_capacity(render.composite.len() * 4);
    for pixel in &render.composite {
        let alpha = pixel.a.clamp(0.0, 1.0);
        if alpha <= 1e-12 {
            output.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            output.extend_from_slice(&[
                vice_ir::color::linear_to_srgb_u8((pixel.r / alpha).clamp(0.0, 1.0)),
                vice_ir::color::linear_to_srgb_u8((pixel.g / alpha).clamp(0.0, 1.0)),
                vice_ir::color::linear_to_srgb_u8((pixel.b / alpha).clamp(0.0, 1.0)),
                (alpha * 255.0).round() as u8,
            ]);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use vice_ir::{
        BlendSpace, Canvas, ExteriorModel, GlobalFormationHypothesis, PixelFilter, PlanarGraph,
        QuantizationModel, VectorScene,
    };

    fn transparent_png() -> Vec<u8> {
        let pixels = vec![0; 4 * 4 * 4];
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 4, 4);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&pixels).unwrap();
        }
        bytes
    }

    fn split_png() -> Vec<u8> {
        let mut pixels = Vec::with_capacity(32 * 32 * 4);
        for _y in 0..32 {
            for x in 0..32 {
                let rgb = if x < 16 { [24, 48, 72] } else { [220, 180, 40] };
                pixels.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
            }
        }
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 32, 32);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&pixels).unwrap();
        }
        bytes
    }

    #[test]
    fn inspection_exposes_the_complete_global_universe() {
        let report = inspect_m9_formation(&split_png()).unwrap();
        assert_eq!(report.source_format, EncodedImageFormat::Png);
        assert_eq!(report.resize_chains, ResizeChain::ALL);
        assert_eq!(report.hypothesis_count, 48);
        assert_eq!(report.hypothesis_ids_sha256.len(), 64);
    }

    #[test]
    fn formed_score_binds_resize_filter_codec_and_render_bytes() {
        let scene = ValidatedScene::new(VectorScene {
            canvas: Canvas {
                width_px: 4,
                height_px: 4,
            },
            graph: PlanarGraph::empty(),
            formation: GlobalFormationHypothesis {
                blend_space: BlendSpace::LinearLight,
                pixel_filter: PixelFilter::Gaussian { sigma_px: 1.5 },
                quantization: QuantizationModel::Uint8,
                exterior: ExteriorModel::Transparent,
            },
        })
        .unwrap();
        let config =
            CodecLikelihoodConfig::new(CodecResidualModel::CleanCorrelation, 1.0, 1.0, 1.0, 4.0)
                .unwrap();
        let report =
            score_m9_formation(&transparent_png(), &scene, ResizeChain::UpFromHalf, config)
                .unwrap();
        assert_eq!(report.codec.total_bits, 0.0);
        assert_eq!(report.resize_chain, ResizeChain::UpFromHalf);
        assert_eq!(report.formed_render_sha256.len(), 64);
    }
}
