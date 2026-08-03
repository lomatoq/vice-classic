use super::*;
use serde_json::json;

fn production_bytes(stale_calibration_field: Option<&str>) -> Vec<u8> {
    let preset = Preset::Fast;
    let seal = DeliverySealConfig {
        max_profile_channel_delta: 1,
        max_profile_mean_channel_delta: 0.01,
        max_internal_channel_delta: 1,
        max_internal_mean_channel_delta: 0.01,
    };
    let mut config = CoreConfig::development_for(preset);
    config.seal = seal;
    let identity = config.identity();
    let mut calibration = json!({
        "schema": "vice-classic/confidence-calibration/v4",
        "model_universe_sha256": identity.universe_sha256,
        "pricing_sha256": identity.pricing_sha256,
        "backend_sha256": identity.backend_sha256,
        "config_sha256": identity.config_sha256,
        "calibration_split_sha256": "1".repeat(64),
        "sealed_audit_generation": "audit-1",
        "sealed_audit_untouched": true,
        "confidence_level": 0.99,
        "catastrophic_risk_target": 0.01,
        "accepted_source_groups": 459,
        "catastrophic_source_groups": 0,
        "posterior_lower_bound_threshold": 0.95,
        "minimum_top2_class_margin_bits": 0.0,
        "maximum_posterior_predictive_bits_per_block": 0.1,
        "maximum_support_isotopy_displacement_px": 0.5,
        "maximum_evidence_palette_shift_codes": 2,
        "minimum_palette_support_px": 1,
        "maximum_palette_interval_radius_codes": 4,
        "maximum_abs_residual_lag1": 0.9,
        "maximum_topology_entropy_bits": 1.0,
        "maximum_formation_entropy_bits": 1.0,
        "minimum_perturbation_stability": 0.95,
        "empirical_unexplored_relative_mass_upper_bound": 0.25,
        "supported_selection_classes": ["flat2/general"],
        "paint_calibration_classes": [{
            "name": "flat2/general|delivery:t0/srgb/box/u8/opaque|paint:fg-point+bg-point",
            "accepted_source_groups": 459
        }],
        "buckets": [{
            "name": "all",
            "accepted_source_groups": 459,
            "eligible_source_groups": 459,
            "minimum_coverage": 1.0
        }]
    });
    if let Some(field) = stale_calibration_field {
        calibration[field] = json!("2".repeat(64));
    }
    serde_json::to_vec(&json!({
        "schema": "vice-classic/m7-production-config/v1",
        "preset": "fast",
        "delivery_seal": {
            "max_profile_channel_delta": seal.max_profile_channel_delta,
            "max_profile_mean_channel_delta": seal.max_profile_mean_channel_delta,
            "max_internal_channel_delta": seal.max_internal_channel_delta,
            "max_internal_mean_channel_delta": seal.max_internal_mean_channel_delta
        },
        "calibration": calibration,
        "identity": identity
    }))
    .unwrap()
}

#[test]
fn only_exact_trust_anchored_bytes_can_set_production() {
    let bytes = production_bytes(None);
    let digest = hex::encode(Sha256::digest(&bytes));
    let config = CoreConfig::production_from_bytes(Preset::Fast, &bytes, &digest)
        .expect("valid pinned config");
    assert!(config.is_sealed_production());

    let mut tampered = bytes;
    tampered.push(b' ');
    assert!(matches!(
        CoreConfig::production_from_bytes(Preset::Fast, &tampered, &digest),
        Err(ProductionConfigError::UntrustedDigest)
    ));
}

#[test]
fn a_freshly_pinned_file_still_cannot_carry_stale_calibration() {
    for field in [
        "model_universe_sha256",
        "pricing_sha256",
        "backend_sha256",
        "config_sha256",
    ] {
        let bytes = production_bytes(Some(field));
        let digest = hex::encode(Sha256::digest(&bytes));
        assert!(
            matches!(
                CoreConfig::production_from_bytes(Preset::Fast, &bytes, &digest),
                Err(ProductionConfigError::Calibration(
                    "calibration_identity_or_audit"
                ))
            ),
            "stale calibration field {field} was accepted"
        );
    }
}

#[test]
fn confidence_does_not_create_a_circular_model_config_identity() {
    let mut config = CoreConfig::development_for(Preset::Fast);
    let before = config.identity();
    let file: serde_json::Value = serde_json::from_slice(&production_bytes(None)).unwrap();
    let mut calibration: ConfidenceCalibration =
        serde_json::from_value(file["calibration"].clone()).unwrap();
    calibration.model_universe_sha256 = before.universe_sha256.clone();
    calibration.pricing_sha256 = before.pricing_sha256.clone();
    calibration.backend_sha256 = before.backend_sha256.clone();
    calibration.config_sha256 = before.config_sha256.clone();
    config.confidence = Some(calibration);
    config.sealed_production = true;
    assert_eq!(config.identity(), before);
}

#[test]
fn freezing_delivery_thresholds_does_not_rekey_the_calibrated_posterior() {
    let mut config = CoreConfig::development_for(Preset::Quality);
    let posterior = config.identity();
    let delivery = config.delivery_policy_sha256();
    config.seal = DeliverySealConfig {
        max_profile_channel_delta: 0,
        max_profile_mean_channel_delta: 0.0,
        max_internal_channel_delta: 64,
        max_internal_mean_channel_delta: 0.25,
    };
    assert_eq!(config.identity(), posterior);
    assert_ne!(config.delivery_policy_sha256(), delivery);
}

#[test]
fn m7_presets_share_the_protected_primary_lane_but_quality_keeps_the_wider_court() {
    let quality = CoreConfig::development_for(Preset::Quality);
    let fast = CoreConfig::development_for(Preset::Fast);

    assert_eq!(quality.k_discrete_paths, 4);
    assert_eq!(fast.k_discrete_paths, 4);
    assert_eq!(quality.trust_region.max_rounds, 4);
    assert_eq!(fast.trust_region.max_rounds, 2);
    assert!(quality.requires_fast_admission_witness());
    assert!(!fast.requires_fast_admission_witness());
    assert!(quality.beam.width > fast.beam.width);
    assert_eq!(quality.beam.budget.max_materializations, 8);
    assert_eq!(fast.beam.budget.max_materializations, 5);
    assert!(
        quality.beam.budget.max_candidates_considered > fast.beam.budget.max_candidates_considered
    );
}

#[test]
fn committed_m7_production_configs_match_the_compiled_trust_anchors() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for (preset, relative) in [
        (Preset::Quality, "configs/M7_PRODUCTION_QUALITY.json"),
        (Preset::Fast, "configs/M7_PRODUCTION_FAST.json"),
    ] {
        let config = CoreConfig::load_production_for(preset, &root.join(relative))
            .unwrap_or_else(|error| panic!("{relative}: {error}"));
        assert!(config.is_sealed_production());
        assert_eq!(config.preset(), preset);
    }
}
