use serde::Serialize;
use sha2::{Digest, Sha256};
use thiserror::Error;
use vice_ir::color::linear_to_srgb_encoded;
use vice_svg::{
    build_export_plan, canonical_export_plan_bytes, materialize_svg, ExportPlan,
    IndependentlyRenderedSvg, SvgProfile, SVG_PARSER_ID, SVG_RENDERER_ID,
};

use crate::QuantizedVerifiedScene;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct DeliverySealConfig {
    pub max_profile_channel_delta: u8,
    pub max_profile_mean_channel_delta: f64,
    pub max_internal_channel_delta: u8,
    pub max_internal_mean_channel_delta: f64,
}

impl DeliverySealConfig {
    fn validate(self) -> Result<(), DeliverySealError> {
        if !self.max_profile_mean_channel_delta.is_finite()
            || self.max_profile_mean_channel_delta < 0.0
            || !self.max_internal_mean_channel_delta.is_finite()
            || self.max_internal_mean_channel_delta < 0.0
        {
            Err(DeliverySealError::InvalidConfig)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct DeliveryComparison {
    pub max_channel_delta: u8,
    pub mean_channel_delta: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DeliverySeal {
    pub scene_digest_sha256: String,
    pub export_plan_digest_sha256: String,
    pub pure_partition_svg_digest_sha256: String,
    pub seam_safe_svg_digest_sha256: String,
    pub pure_partition_render_digest_sha256: String,
    pub seam_safe_render_digest_sha256: String,
    pub parser_id: String,
    pub renderer_ids: Vec<String>,
    pub profile_comparison: DeliveryComparison,
    pub internal_to_pure_comparison: DeliveryComparison,
    pub internal_to_seam_comparison: DeliveryComparison,
    pub apron_paths: u64,
}

#[derive(Debug, Error)]
pub enum DeliverySealError {
    #[error("delivery seal configuration is invalid")]
    InvalidConfig,
    #[error("export plan does not exactly reconstruct from the verified scene")]
    ExportPlanMismatch,
    #[error("serialized SVG witness has wrong scene, profile, dimensions, or apron count")]
    WitnessMismatch,
    #[error("serialized SVG bytes do not match the canonical materialization")]
    SvgBytesMismatch,
    #[error("render buffers have different dimensions")]
    RenderDimensions,
    #[error("PurePartition and SeamSafe render divergence exceeds the gate")]
    ProfileDivergence,
    #[error("serialized render diverges from the certified internal render")]
    InternalDivergence,
    #[error("export plan operation failed: {0}")]
    Export(String),
}

fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn compare(a: &[u8], b: &[u8]) -> Result<DeliveryComparison, DeliverySealError> {
    if a.len() != b.len() || a.is_empty() {
        return Err(DeliverySealError::RenderDimensions);
    }
    let mut max = 0u8;
    let mut sum = 0u64;
    for (&left, &right) in a.iter().zip(b) {
        let delta = left.abs_diff(right);
        max = max.max(delta);
        sum += u64::from(delta);
    }
    Ok(DeliveryComparison {
        max_channel_delta: max,
        mean_channel_delta: sum as f64 / a.len() as f64,
    })
}

fn internal_premultiplied_srgb8(scene: &QuantizedVerifiedScene) -> Vec<u8> {
    let mut output = Vec::with_capacity(scene.render().composite.len() * 4);
    for pixel in &scene.render().composite {
        let alpha = pixel.a.clamp(0.0, 1.0);
        for linear_premul in [pixel.r, pixel.g, pixel.b] {
            let straight = if alpha == 0.0 {
                0.0
            } else {
                (linear_premul / alpha).clamp(0.0, 1.0)
            };
            let encoded_premul = linear_to_srgb_encoded(straight) * alpha;
            output.push((encoded_premul * 255.0).round().clamp(0.0, 255.0) as u8);
        }
        output.push((alpha * 255.0).round().clamp(0.0, 255.0) as u8);
    }
    output
}

fn witness_matches(
    witness: &IndependentlyRenderedSvg,
    profile: SvgProfile,
    scene_digest: &str,
    width: u32,
    height: u32,
    aprons: usize,
) -> bool {
    witness.profile() == profile
        && witness.scene_digest_sha256() == scene_digest
        && witness.width_px() == width
        && witness.height_px() == height
        && witness.apron_paths() == aprons
}

pub fn seal_delivery(
    scene: &QuantizedVerifiedScene,
    plan: &ExportPlan,
    pure: &IndependentlyRenderedSvg,
    seam: &IndependentlyRenderedSvg,
    cfg: DeliverySealConfig,
) -> Result<DeliverySeal, DeliverySealError> {
    cfg.validate()?;
    let expected = build_export_plan(scene.scene(), plan.decimal_places(), plan.apron_width_px())
        .map_err(|e| DeliverySealError::Export(e.to_string()))?;
    if &expected != plan
        || plan.scene_digest_sha256()
            != scene
                .post_quantization_certificate()
                .post_scene_digest_sha256
    {
        return Err(DeliverySealError::ExportPlanMismatch);
    }
    if !witness_matches(
        pure,
        SvgProfile::PurePartition,
        plan.scene_digest_sha256(),
        plan.width_px(),
        plan.height_px(),
        0,
    ) || !witness_matches(
        seam,
        SvgProfile::SeamSafe,
        plan.scene_digest_sha256(),
        plan.width_px(),
        plan.height_px(),
        plan.aprons().len(),
    ) {
        return Err(DeliverySealError::WitnessMismatch);
    }
    let expected_pure = materialize_svg(plan, SvgProfile::PurePartition)
        .map_err(|e| DeliverySealError::Export(e.to_string()))?;
    let expected_seam = materialize_svg(plan, SvgProfile::SeamSafe)
        .map_err(|e| DeliverySealError::Export(e.to_string()))?;
    if pure.svg_digest_sha256() != digest(&expected_pure)
        || seam.svg_digest_sha256() != digest(&expected_seam)
    {
        return Err(DeliverySealError::SvgBytesMismatch);
    }

    let profile = compare(pure.premultiplied_rgba8(), seam.premultiplied_rgba8())?;
    if profile.max_channel_delta > cfg.max_profile_channel_delta
        || profile.mean_channel_delta > cfg.max_profile_mean_channel_delta
    {
        return Err(DeliverySealError::ProfileDivergence);
    }
    let internal = internal_premultiplied_srgb8(scene);
    let internal_to_pure = compare(&internal, pure.premultiplied_rgba8())?;
    let internal_to_seam = compare(&internal, seam.premultiplied_rgba8())?;
    if internal_to_pure.max_channel_delta > cfg.max_internal_channel_delta
        || internal_to_pure.mean_channel_delta > cfg.max_internal_mean_channel_delta
        || internal_to_seam.max_channel_delta > cfg.max_internal_channel_delta
        || internal_to_seam.mean_channel_delta > cfg.max_internal_mean_channel_delta
    {
        return Err(DeliverySealError::InternalDivergence);
    }
    let plan_bytes =
        canonical_export_plan_bytes(plan).map_err(|e| DeliverySealError::Export(e.to_string()))?;
    Ok(DeliverySeal {
        scene_digest_sha256: plan.scene_digest_sha256().to_owned(),
        export_plan_digest_sha256: digest(&plan_bytes),
        pure_partition_svg_digest_sha256: pure.svg_digest_sha256().to_owned(),
        seam_safe_svg_digest_sha256: seam.svg_digest_sha256().to_owned(),
        pure_partition_render_digest_sha256: pure.render_digest_sha256().to_owned(),
        seam_safe_render_digest_sha256: seam.render_digest_sha256().to_owned(),
        parser_id: SVG_PARSER_ID.into(),
        renderer_ids: vec![
            "vice-render/certified-partition".into(),
            SVG_RENDERER_ID.into(),
        ],
        profile_comparison: profile,
        internal_to_pure_comparison: internal_to_pure,
        internal_to_seam_comparison: internal_to_seam,
        apron_paths: plan.aprons().len() as u64,
    })
}
