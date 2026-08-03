//! Independently parsed and rendered M8 SVG delivery.

use serde::Serialize;
use sha2::{Digest, Sha256};
use vice_image::{CanonicalImage, DecodeLimits};
use vice_ir::canonical_scene_bytes;
use vice_opt::{score_serialized_full_resolution, PriorCodeLengths};
use vice_svg::{
    build_export_plan, canonical_export_plan_bytes, materialize_svg,
    parse_and_render_independently, SvgProfile,
};
use vice_verify::{
    quantize_and_verify, seal_delivery, DeliverySeal, DeliverySealConfig, QuantizationPolicy,
    VerificationConfig,
};

use super::{
    multiregion_boundary_bindings, propose_multiregion_seeds, M8ExactConfig, M8SolvedCandidate,
    MultiregionMaterializeError, MultiregionSeedError,
};

pub const M8_DELIVERY_SCHEMA: &str = "vice-classic/m8-delivery/v1";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct M8DeliveryConfig {
    pub verification: VerificationConfig,
    pub quantization: QuantizationPolicy,
    pub seal: DeliverySealConfig,
    pub export_decimal_places: u32,
    pub apron_width_px: f64,
}

impl Default for M8DeliveryConfig {
    fn default() -> Self {
        Self {
            verification: VerificationConfig {
                render_options: vice_render::RenderOptions::default(),
                max_g1_spread_rad: vice_fit::GATE_MAX_G1_SPREAD_RAD,
                curve_separation_margin_px: 1e-9,
            },
            quantization: QuantizationPolicy { decimal_places: 12 },
            // Measurement ceiling only. The measured M8 gate is frozen in a
            // later config-only commit and this report says so explicitly.
            seal: DeliverySealConfig {
                max_profile_channel_delta: 255,
                max_profile_mean_channel_delta: 255.0,
                max_internal_channel_delta: 255,
                max_internal_mean_channel_delta: 255.0,
            },
            export_decimal_places: 12,
            apron_width_px: 0.01,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct M8DeliveryReport {
    pub schema: &'static str,
    pub source_sha256: String,
    pub selected_exact_id: String,
    pub delivery_config_sha256: String,
    pub release_gates_frozen: bool,
    pub production_admitted: bool,
    pub pure_serialized_pixel_bits: f64,
    pub seam_serialized_pixel_bits: f64,
    pub seal: DeliverySeal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct M8DeliveryArtifacts {
    pub scene_json: Vec<u8>,
    pub plan_json: Vec<u8>,
    pub pure_svg: Vec<u8>,
    pub seam_svg: Vec<u8>,
    pub pure_png: Vec<u8>,
    pub seam_png: Vec<u8>,
    pub seal_json: Vec<u8>,
    pub report: M8DeliveryReport,
}

#[derive(Debug, thiserror::Error)]
pub enum M8DeliveryError {
    #[error(transparent)]
    Decode(#[from] vice_image::ImageError),
    #[error(transparent)]
    Seed(#[from] MultiregionSeedError),
    #[error(transparent)]
    Binding(#[from] MultiregionMaterializeError),
    #[error("the exact winner's source digest does not match the supplied PNG")]
    SourceMismatch,
    #[error("the exact winner's seed is absent from deterministic reproposal")]
    MissingSeed,
    #[error("M8 delivery configuration is malformed")]
    InvalidConfig,
    #[error("quantized verification failed: {0}")]
    Quantization(#[from] vice_verify::QuantizationError),
    #[error("SVG export plan failed: {0}")]
    Export(#[from] vice_svg::ExportPlanError),
    #[error("SVG materialization failed: {0}")]
    Svg(#[from] vice_svg::SvgMaterializationError),
    #[error("independent SVG court failed: {0}")]
    Independent(#[from] vice_svg::IndependentSvgError),
    #[error("delivery seal failed: {0}")]
    Seal(#[from] vice_verify::DeliverySealError),
    #[error("serialized SVG likelihood failed: {0}")]
    Likelihood(#[from] vice_opt::LikelihoodError),
    #[error("canonical artifact failed: {0}")]
    Canonical(#[from] vice_ir::SceneError),
    #[error("delivery report serialization failed: {0}")]
    Json(#[from] serde_json::Error),
}

pub fn seal_multiregion_delivery(
    png_bytes: &[u8],
    solved: &M8SolvedCandidate,
    exact_cfg: &M8ExactConfig,
    cfg: &M8DeliveryConfig,
) -> Result<M8DeliveryArtifacts, M8DeliveryError> {
    validate_config(cfg)?;
    let image = CanonicalImage::decode_png(png_bytes, &DecodeLimits::default())?;
    if image.source_sha256() != solved.report.source_sha256 {
        return Err(M8DeliveryError::SourceMismatch);
    }
    let seeds = propose_multiregion_seeds(png_bytes)?;
    let seed = seeds
        .seeds
        .iter()
        .find(|seed| seed.id == solved.report.selected.seed_id)
        .ok_or(M8DeliveryError::MissingSeed)?;
    let bindings = multiregion_boundary_bindings(seed, &solved.scene)?;
    let verified =
        quantize_and_verify(&solved.scene, &bindings, cfg.verification, cfg.quantization)?;
    let plan = build_export_plan(
        verified.scene(),
        cfg.export_decimal_places,
        cfg.apron_width_px,
    )?;
    let plan_json = canonical_export_plan_bytes(&plan)?;
    let pure_svg = materialize_svg(&plan, SvgProfile::PurePartition)?;
    let seam_svg = materialize_svg(&plan, SvgProfile::SeamSafe)?;
    let pure = parse_and_render_independently(&pure_svg)?;
    let seam = parse_and_render_independently(&seam_svg)?;
    let seal = seal_delivery(&verified, &plan, &pure, &seam, cfg.seal)?;
    let zero_priors = PriorCodeLengths {
        topology_bits: 0.0,
        geometry_bits: 0.0,
        paint_bits: 0.0,
        relation_bits: 0.0,
        formation_bits: 0.0,
    };
    let pure_score = score_serialized_full_resolution(
        verified.scene(),
        &image,
        pure.premultiplied_rgba8(),
        pure.width_px(),
        pure.height_px(),
        exact_cfg.likelihood,
        zero_priors,
    )?;
    let seam_score = score_serialized_full_resolution(
        verified.scene(),
        &image,
        seam.premultiplied_rgba8(),
        seam.width_px(),
        seam.height_px(),
        exact_cfg.likelihood,
        zero_priors,
    )?;
    let report = M8DeliveryReport {
        schema: M8_DELIVERY_SCHEMA,
        source_sha256: image.source_sha256().to_string(),
        selected_exact_id: solved.report.selected.id.clone(),
        delivery_config_sha256: config_digest(cfg),
        release_gates_frozen: false,
        production_admitted: false,
        pure_serialized_pixel_bits: pure_score.pixel_bits,
        seam_serialized_pixel_bits: seam_score.pixel_bits,
        seal,
    };
    let seal_json = serde_json::to_vec(&report)?;
    Ok(M8DeliveryArtifacts {
        scene_json: canonical_scene_bytes(verified.scene())?,
        plan_json,
        pure_svg,
        seam_svg,
        pure_png: pure.png_bytes().to_vec(),
        seam_png: seam.png_bytes().to_vec(),
        seal_json,
        report,
    })
}

fn validate_config(cfg: &M8DeliveryConfig) -> Result<(), M8DeliveryError> {
    if cfg.export_decimal_places == 0
        || cfg.export_decimal_places > 15
        || !cfg.apron_width_px.is_finite()
        || cfg.apron_width_px <= 0.0
    {
        return Err(M8DeliveryError::InvalidConfig);
    }
    Ok(())
}

#[derive(Serialize)]
struct DeliveryConfigIdentity {
    chord_tolerance_px: f64,
    max_g1_spread_rad: f64,
    curve_separation_margin_px: f64,
    quantization: QuantizationPolicy,
    seal: DeliverySealConfig,
    export_decimal_places: u32,
    apron_width_px: f64,
}

fn config_digest(cfg: &M8DeliveryConfig) -> String {
    let identity = DeliveryConfigIdentity {
        chord_tolerance_px: cfg.verification.render_options.budget.chord_tolerance.px(),
        max_g1_spread_rad: cfg.verification.max_g1_spread_rad,
        curve_separation_margin_px: cfg.verification.curve_separation_margin_px,
        quantization: cfg.quantization,
        seal: cfg.seal,
        export_decimal_places: cfg.export_decimal_places,
        apron_width_px: cfg.apron_width_px,
    };
    hex::encode(Sha256::digest(
        serde_json::to_vec(&identity).expect("M8 delivery config serializes"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png() -> Vec<u8> {
        let (w, h) = (12u32, 6u32);
        let colors = [[230, 20, 20], [20, 220, 30], [20, 40, 230]];
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                pixels[i..i + 3].copy_from_slice(&colors[(x / 4) as usize]);
                pixels[i + 3] = 255;
            }
        }
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, w, h);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&pixels).unwrap();
        }
        bytes
    }

    #[test]
    fn multicolor_delivery_is_parsed_rendered_and_sealed_from_its_bytes() {
        let bytes = png();
        let exact_cfg = M8ExactConfig::default();
        let solved = super::super::solve_multiregion_exact(&bytes, &exact_cfg).unwrap();
        let artifacts =
            seal_multiregion_delivery(&bytes, &solved, &exact_cfg, &M8DeliveryConfig::default())
                .unwrap();
        assert!(artifacts.pure_svg.starts_with(b"<svg"));
        assert!(artifacts.seam_svg.starts_with(b"<svg"));
        assert!(!artifacts.pure_png.is_empty() && !artifacts.seam_png.is_empty());
        assert_eq!(artifacts.report.seal.parser_id, vice_svg::SVG_PARSER_ID);
        assert!(!artifacts.report.release_gates_frozen);
        assert!(!artifacts.report.production_admitted);
    }
}
