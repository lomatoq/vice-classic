//! M11 solid/linear/radial classification with one common raster objective.

use serde::Serialize;
use vice_image::{CanonicalImage, DecodeLimits, EncodedImageFormat};
use vice_ir::{GradientPaint, ValidatedGradientScene, MAX_GRADIENT_STOPS};
use vice_opt::{CodecLikelihoodError, CodecLikelihoodReport};

pub const M11_CLASSIFICATION_SCHEMA: &str = "vice-classic/m11-gradient-classification/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum M11GradientKind {
    Solid,
    Linear,
    Radial,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct M11CandidateScore {
    pub kind: M11GradientKind,
    pub identity_sha256: String,
    pub stop_count: u64,
    pub discontinuity_count: u64,
    pub codec: CodecLikelihoodReport,
    pub geometry_bits: f64,
    pub stop_bits: f64,
    pub model_class_bits: f64,
    pub total_bits: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct M11ClassificationReport {
    pub schema: &'static str,
    pub source_sha256: String,
    pub source_format: EncodedImageFormat,
    pub evidence: vice_evidence::GradientEvidenceReport,
    pub decision: M11GradientKind,
    pub selected_identity_sha256: String,
    pub selected_total_bits: f64,
    pub runner_up_total_bits: f64,
    pub margin_bits: f64,
    pub candidates: Vec<M11CandidateScore>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct M11Classification {
    pub report: M11ClassificationReport,
    pub selected_scene: ValidatedGradientScene,
    pub selected_scene_json: Vec<u8>,
    pub selected_straight_srgb8: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum M11Error {
    #[error(transparent)]
    Decode(#[from] vice_image::ImageError),
    #[error(transparent)]
    Evidence(#[from] vice_evidence::GradientEvidenceRefusal),
    #[error(transparent)]
    Render(#[from] vice_render::GradientRenderError),
    #[error(transparent)]
    Codec(#[from] CodecLikelihoodError),
    #[error("gradient identity/serialization failed: {detail}")]
    Identity { detail: String },
    #[error("gradient classifier produced an empty or non-finite inventory")]
    InvalidInventory,
}

pub fn inspect_m11_gradients(
    bytes: &[u8],
) -> Result<vice_evidence::GradientEvidenceReport, M11Error> {
    let image = CanonicalImage::decode(bytes, &DecodeLimits::default())?;
    Ok(vice_evidence::propose_gradients(&image)?.report)
}

pub fn classify_m11_gradient(bytes: &[u8]) -> Result<M11Classification, M11Error> {
    let image = CanonicalImage::decode(bytes, &DecodeLimits::default())?;
    let proposal = vice_evidence::propose_gradients(&image)?;
    let mut scenes = Vec::with_capacity(proposal.candidates.len());
    let mut renders = Vec::with_capacity(proposal.candidates.len());
    let mut scores = Vec::with_capacity(proposal.candidates.len());
    for scene in proposal.candidates {
        let render = vice_render::render_gradient_scene(&scene)?;
        let score = score(&image, &scene, &render.straight_srgb8)?;
        scenes.push(scene);
        renders.push(render.straight_srgb8);
        scores.push(score);
    }
    if scores.len() < 3 || scores.iter().any(|score| !score.total_bits.is_finite()) {
        return Err(M11Error::InvalidInventory);
    }
    let mut order = (0..scores.len()).collect::<Vec<_>>();
    order.sort_by(|a, b| {
        scores[*a]
            .total_bits
            .total_cmp(&scores[*b].total_bits)
            .then_with(|| scores[*a].identity_sha256.cmp(&scores[*b].identity_sha256))
    });
    let selected = order[0];
    let runner_up = scores[order[1]].total_bits;
    let selected_scene_json =
        vice_ir::gradient_scene_bytes(&scenes[selected]).map_err(|error| M11Error::Identity {
            detail: error.to_string(),
        })?;
    Ok(M11Classification {
        report: M11ClassificationReport {
            schema: M11_CLASSIFICATION_SCHEMA,
            source_sha256: image.source_sha256().into(),
            source_format: image.encoded_format(),
            evidence: proposal.report,
            decision: scores[selected].kind,
            selected_identity_sha256: scores[selected].identity_sha256.clone(),
            selected_total_bits: scores[selected].total_bits,
            runner_up_total_bits: runner_up,
            margin_bits: runner_up - scores[selected].total_bits,
            candidates: scores,
        },
        selected_scene: scenes[selected].clone(),
        selected_scene_json,
        selected_straight_srgb8: renders[selected].clone(),
    })
}

fn score(
    image: &CanonicalImage,
    scene: &ValidatedGradientScene,
    pixels: &[u8],
) -> Result<M11CandidateScore, M11Error> {
    let codec = vice_opt::score_codec_residual(
        image,
        pixels,
        vice_opt::calibrated_codec_likelihood_config(image.encoded_format()),
    )?;
    let coordinate_bits = vice_fit::GEOMETRY_CODE_TABLE_V1
        .coordinate_bits(f64::from(image.width_px().max(image.height_px())));
    let (kind, stops, geometry_parameters) = match &scene.scene().paint {
        GradientPaint::Solid { .. } => (M11GradientKind::Solid, None, 0.0),
        GradientPaint::Linear { stops, .. } => (M11GradientKind::Linear, Some(stops), 4.0),
        GradientPaint::Radial { stops, .. } => (M11GradientKind::Radial, Some(stops), 3.0),
    };
    let stop_count = stops.map_or(1, Vec::len);
    if stop_count > MAX_GRADIENT_STOPS {
        return Err(M11Error::InvalidInventory);
    }
    let discontinuity_count = stops.map_or(0, |values| {
        values
            .windows(2)
            .filter(|pair| pair[0].offset == pair[1].offset)
            .count()
    });
    let geometry_bits = geometry_parameters * coordinate_bits;
    let stop_bits = if stops.is_some() {
        stop_count as f64 * (12.0 + 24.0) + log2_factorial(stop_count)
    } else {
        24.0
    };
    let model_class_bits = 3.0f64.log2();
    let total_bits = codec.total_bits + geometry_bits + stop_bits + model_class_bits;
    Ok(M11CandidateScore {
        kind,
        identity_sha256: vice_ir::gradient_scene_digest_sha256(scene).map_err(|error| {
            M11Error::Identity {
                detail: error.to_string(),
            }
        })?,
        stop_count: stop_count as u64,
        discontinuity_count: discontinuity_count as u64,
        codec,
        geometry_bits,
        stop_bits,
        model_class_bits,
        total_bits,
    })
}

fn log2_factorial(count: usize) -> f64 {
    (2..=count).map(|value| (value as f64).log2()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use vice_ir::color::linear_to_srgb_u8;

    fn png(width: u32, height: u32, pixel: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                rgba.extend_from_slice(&pixel(x, y));
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
    fn flat_input_stays_on_the_solid_path() {
        let bytes = png(32, 32, |_, _| [80, 120, 200, 255]);
        let result = classify_m11_gradient(&bytes).unwrap();
        assert_eq!(result.report.decision, M11GradientKind::Solid);
        assert_eq!(result.report.candidates[0].kind, M11GradientKind::Solid);
    }

    #[test]
    fn a_single_pixel_is_a_valid_solid_not_degenerate_geometry() {
        let bytes = png(1, 1, |_, _| [80, 120, 200, 255]);
        let result = classify_m11_gradient(&bytes).unwrap();
        assert_eq!(result.report.decision, M11GradientKind::Solid);
    }

    #[test]
    fn a_linear_ramp_selects_compact_linear_geometry() {
        let bytes = png(48, 24, |x, _| {
            let t = x as f64 / 47.0;
            [linear_to_srgb_u8(t), 0, linear_to_srgb_u8(1.0 - t), 255]
        });
        let result = classify_m11_gradient(&bytes).unwrap();
        assert_eq!(result.report.decision, M11GradientKind::Linear);
        assert!(result.report.margin_bits > 0.0);
    }

    #[test]
    fn a_radial_ramp_selects_compact_radial_geometry() {
        let bytes = png(33, 33, |x, y| {
            let distance = ((x as f64 - 16.0).hypot(y as f64 - 16.0) / 22.627).clamp(0.0, 1.0);
            let value = linear_to_srgb_u8(distance);
            [value, value, value, 255]
        });
        let result = classify_m11_gradient(&bytes).unwrap();
        assert_eq!(result.report.decision, M11GradientKind::Radial);
    }

    #[test]
    fn a_hard_step_keeps_a_duplicate_stop() {
        let bytes = png(48, 16, |x, _| {
            if x < 24 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            }
        });
        let result = classify_m11_gradient(&bytes).unwrap();
        assert_eq!(result.report.decision, M11GradientKind::Linear);
        assert!(match &result.selected_scene.scene().paint {
            GradientPaint::Linear { stops, .. } => stops
                .windows(2)
                .any(|pair| pair[0].offset == pair[1].offset),
            _ => false,
        });
    }

    #[test]
    fn the_scene_schema_is_bound_in_the_serialized_product() {
        let bytes = png(8, 8, |_, _| [10, 20, 30, 255]);
        let result = classify_m11_gradient(&bytes).unwrap();
        assert!(String::from_utf8(result.selected_scene_json)
            .unwrap()
            .contains(vice_ir::GRADIENT_SCENE_SCHEMA));
    }
}
