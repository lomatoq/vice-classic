//! M7 selective Flat2 vectorization.
//!
//! This crate is the first production owner of the complete §7/§30 path:
//! decode → evidence → complementary-connectivity DCEL → typed k-best fit →
//! full-resolution posterior → confidence/abstention → quantized verification
//! → two-profile serialized delivery seal. A non-success outcome contains no
//! SVG bytes.

#![forbid(unsafe_code)]

mod config;
mod pipeline;
mod scene;
mod types;

pub use config::{
    CalibrationBucket, ConfidenceCalibration, CoreConfig, Intent, Preset, VectorizeRequest,
};
pub use pipeline::{vectorize, vectorize_with_config};
pub use types::{
    DecisionStatus, FailureReason, SuccessArtifacts, VectorizeOutcome, VectorizeReport,
    VectorizeSuccess, CORE_REPORT_SCHEMA,
};

#[cfg(test)]
mod tests {
    use super::*;

    fn square_png() -> Vec<u8> {
        let width = 32;
        let height = 32;
        let mut rgba = vec![0u8; width * height * 4];
        for y in 7..25 {
            for x in 7..25 {
                let offset = (y * width + x) * 4;
                rgba[offset..offset + 4].copy_from_slice(&[220, 40, 30, 255]);
            }
        }
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, width as u32, height as u32);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&rgba).unwrap();
        }
        bytes
    }

    #[test]
    fn exact_zero_failure_bound_needs_at_least_459_independent_groups() {
        let identity = CoreConfig::development().identity();
        let calibration = |accepted_source_groups| ConfidenceCalibration {
            schema: "vice-classic/confidence-calibration/v1".into(),
            model_universe_sha256: identity.universe_sha256.clone(),
            calibration_split_sha256: "1".repeat(64),
            sealed_audit_generation: "audit-1".into(),
            sealed_audit_untouched: true,
            confidence_level: 0.99,
            catastrophic_risk_target: 0.01,
            accepted_source_groups,
            catastrophic_source_groups: 0,
            posterior_lower_bound_threshold: 0.95,
            buckets: vec![CalibrationBucket {
                name: "all".into(),
                accepted_source_groups,
                eligible_source_groups: accepted_source_groups,
                minimum_coverage: 1.0,
            }],
        };
        assert!(calibration(458).permits(&identity, 1.0).is_err());
        assert!(calibration(459).permits(&identity, 1.0).is_ok());
    }

    #[test]
    fn invalid_input_is_a_typed_failure_with_no_artifact_variant() {
        let outcome = vectorize(b"not a png", &VectorizeRequest::default());
        assert!(matches!(outcome, VectorizeOutcome::Failed(_)));
        assert!(matches!(
            outcome.report().reason,
            Some(FailureReason::Decode { .. })
        ));
    }

    #[test]
    fn a_flat2_png_enters_the_selective_pipeline_without_false_success() {
        let mut config = CoreConfig::development();
        config.k_discrete_paths = 1;
        let outcome = vectorize_with_config(&square_png(), &VectorizeRequest::default(), &config);
        assert!(!matches!(
            outcome,
            VectorizeOutcome::Success(_) | VectorizeOutcome::Failed(_)
        ));
        assert!(outcome.report().evidence.is_some());
    }
}
