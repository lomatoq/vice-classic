//! M7 selective Flat2 vectorization.
//!
//! This crate is the first production owner of the complete §7/§30 path:
//! decode → evidence → complementary-connectivity DCEL → typed k-best fit →
//! full-resolution posterior → confidence/abstention → quantized verification
//! → two-profile serialized delivery seal. A non-success outcome contains no
//! SVG bytes.

#![forbid(unsafe_code)]

mod candidate;
mod config;
mod pipeline;
mod scene;
mod types;

pub use config::{
    CalibrationBucket, ConfidenceCalibration, CoreConfig, Intent, IntentPriorPolicy, Preset,
    VectorizeRequest,
};
pub use pipeline::{vectorize, vectorize_with_config};
pub use types::{
    CandidateFailureStage, CandidateRefusal, DecisionStatus, FailureReason, SuccessArtifacts,
    VectorizeOutcome, VectorizeReport, VectorizeSuccess, CORE_REPORT_SCHEMA,
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

    fn two_component_png() -> Vec<u8> {
        let width = 48usize;
        let height = 32usize;
        let mut rgba = vec![0u8; width * height * 4];
        for y in 0..height {
            for x in 0..width {
                let mut inside = 0u32;
                for sy in 0..8 {
                    for sx in 0..8 {
                        let px = x as f64 + (f64::from(sx) + 0.5) / 8.0;
                        let py = y as f64 + (f64::from(sy) + 0.5) / 8.0;
                        if [(12.0, 16.0, 7.0), (35.0, 16.0, 7.0)]
                            .iter()
                            .any(|&(cx, cy, radius)| {
                                (px - cx).powi(2) + (py - cy).powi(2) <= radius * radius
                            })
                        {
                            inside += 1;
                        }
                    }
                }
                let alpha = ((inside * 255 + 32) / 64) as u8;
                let offset = (y * width + x) * 4;
                rgba[offset..offset + 4].copy_from_slice(&[220, 40, 30, alpha]);
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

    fn annulus_png() -> Vec<u8> {
        let width = 48usize;
        let height = 48usize;
        let mut rgba = vec![0u8; width * height * 4];
        for y in 0..height {
            for x in 0..width {
                let mut inside = 0u32;
                for sy in 0..8 {
                    for sx in 0..8 {
                        let px = x as f64 + (f64::from(sx) + 0.5) / 8.0 - 24.0;
                        let py = y as f64 + (f64::from(sy) + 0.5) / 8.0 - 24.0;
                        let radius_sq = px * px + py * py;
                        if (6.0f64.powi(2)..=14.0f64.powi(2)).contains(&radius_sq) {
                            inside += 1;
                        }
                    }
                }
                let alpha = ((inside * 255 + 32) / 64) as u8;
                let offset = (y * width + x) * 4;
                rgba[offset..offset + 4].copy_from_slice(&[30, 100, 230, alpha]);
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
            empirical_unexplored_relative_mass_upper_bound: Some(0.0),
            buckets: vec![CalibrationBucket {
                name: "all".into(),
                accepted_source_groups,
                eligible_source_groups: accepted_source_groups,
                minimum_coverage: 1.0,
            }],
        };
        let delivery = vice_opt::DeliveryPosterior {
            delivery_digest: "d".into(),
            explored_mass: 1.0,
            retained_normalized_mass: 1.0,
            posterior_lower_bound: vice_opt::BoundValue::Certified(1.0),
        };
        assert!(calibration(458).permits(&identity, &delivery).is_err());
        assert!(calibration(459).permits(&identity, &delivery).is_ok());
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

    #[test]
    fn disconnected_flat2_components_share_one_scene_and_palette() {
        let mut config = CoreConfig::development_for(Preset::Fast);
        config.k_discrete_paths = 1;
        let outcome =
            vectorize_with_config(&two_component_png(), &VectorizeRequest::default(), &config);
        assert!(!matches!(outcome, VectorizeOutcome::Failed(_)));
        assert_eq!(outcome.report().fits.len(), 2);
        assert!(
            !outcome.report().candidates.is_empty(),
            "multi-component candidates were refused: {:?}",
            outcome.report().candidate_refusals
        );
        let scene = &outcome.report().candidates[0].pre_quantization;
        assert_eq!(scene.boundaries, 2);
        assert_eq!(scene.observed_chain_bindings, 2);
        assert!(
            outcome
                .report()
                .candidates
                .iter()
                .any(|candidate| candidate.hypothesis_id.starts_with("scene-repetition-"))
                || outcome
                    .report()
                    .candidate_refusals
                    .iter()
                    .any(|refusal| refusal.hypothesis_id.starts_with("scene-repetition-")),
            "scene repetition was not searched"
        );
    }

    #[test]
    fn transparent_hole_is_preserved_as_a_dcel_face() {
        let config = CoreConfig::development_for(Preset::Fast);
        let outcome = vectorize_with_config(&annulus_png(), &VectorizeRequest::default(), &config);
        assert!(!matches!(outcome, VectorizeOutcome::Failed(_)));
        assert_eq!(outcome.report().fits.len(), 2);
        assert!(
            !outcome.report().candidates.is_empty(),
            "annulus candidates were refused: {:?}",
            outcome.report().candidate_refusals
        );
        let scene = &outcome.report().candidates[0].pre_quantization;
        assert_eq!(scene.boundaries, 2);
        assert_eq!(scene.faces, 3);
        assert_eq!(scene.observed_chain_bindings, 2);
    }
}
