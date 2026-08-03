use super::*;
use crate::gates::{GateSection, GatesDoc};

fn file(status: &str) -> GatesFile {
    let boundary = GateSection {
        status: status.into(),
        set_by_milestone: (status == "placeholder").then(|| "M7".into()),
        values: [
            ("p95_px", toml::Value::Float(0.35)),
            ("p99_px", toml::Value::Float(0.60)),
            ("max_px", toml::Value::Float(1.50)),
        ]
        .into_iter()
        .map(|(key, value)| (key.into(), value))
        .collect(),
    };
    let selective = GateSection {
        status: status.into(),
        set_by_milestone: (status == "placeholder").then(|| "M7".into()),
        values: [
            ("gate_min_source_coverage_128_512", toml::Value::Float(0.8)),
            ("gate_min_render_coverage_128_512", toml::Value::Float(0.8)),
            ("gate_max_palette_code_delta", toml::Value::Integer(4)),
            ("gate_max_quality_p95_ms", toml::Value::Integer(10_000)),
            ("gate_max_fast_p95_ms", toml::Value::Integer(1_000)),
            (
                "gate_max_peak_memory_bytes",
                toml::Value::Integer(1_073_741_824),
            ),
            ("gate_max_profile_channel_delta", toml::Value::Integer(0)),
            (
                "gate_max_profile_mean_channel_delta",
                toml::Value::Float(0.0),
            ),
            ("gate_max_internal_channel_delta", toml::Value::Integer(64)),
            (
                "gate_max_internal_mean_channel_delta",
                toml::Value::Float(0.25),
            ),
            (
                "quality_posterior_lower_bound_threshold",
                toml::Value::Float(0.75),
            ),
            (
                "quality_empirical_unexplored_relative_mass_upper_bound",
                toml::Value::Float(1.0),
            ),
            (
                "quality_gate_max_posterior_predictive_bits_per_block",
                toml::Value::Float(0.10),
            ),
            (
                "quality_gate_max_support_isotopy_displacement_px",
                toml::Value::Float(0.5),
            ),
            (
                "fast_posterior_lower_bound_threshold",
                toml::Value::Float(0.80),
            ),
            (
                "fast_empirical_unexplored_relative_mass_upper_bound",
                toml::Value::Float(0.5),
            ),
            (
                "fast_gate_max_posterior_predictive_bits_per_block",
                toml::Value::Float(0.08),
            ),
            (
                "fast_gate_max_support_isotopy_displacement_px",
                toml::Value::Float(0.4),
            ),
            ("gate_min_top2_class_margin_bits", toml::Value::Float(0.0)),
            ("gate_max_abs_residual_lag1", toml::Value::Float(0.90)),
            ("gate_max_topology_entropy_bits", toml::Value::Float(1.0)),
            ("gate_max_formation_entropy_bits", toml::Value::Float(1.0)),
            ("gate_min_perturbation_stability", toml::Value::Float(0.95)),
        ]
        .into_iter()
        .map(|(key, value)| (key.into(), value))
        .collect(),
    };
    GatesFile {
        doc: GatesDoc {
            schema: crate::gates::GATES_SCHEMA.into(),
            version: "v1".into(),
            sections: [
                ("boundary_accuracy".into(), boundary),
                ("m7_selective".into(), selective),
            ]
            .into_iter()
            .collect(),
        },
        sha256: "0".repeat(64),
    }
}

#[test]
fn release_values_are_unreadable_until_the_gate_only_freeze() {
    assert!(M7ReleaseGates::from_file(&file("placeholder")).is_err());
    let frozen = M7ReleaseGates::from_file(&file("frozen")).expect("frozen values load");
    assert_eq!(frozen.max_internal_channel_delta, 64);
    assert_eq!(frozen.min_render_coverage, 0.8);
}

fn calibration_for(
    preset: PresetCalibrationGates,
    gates: M7ReleaseGates,
) -> vice_core::ConfidenceCalibration {
    vice_core::ConfidenceCalibration {
        schema: "vice-classic/confidence-calibration/v4".into(),
        model_universe_sha256: "1".repeat(64),
        pricing_sha256: "2".repeat(64),
        backend_sha256: "3".repeat(64),
        config_sha256: "4".repeat(64),
        calibration_split_sha256: "5".repeat(64),
        sealed_audit_generation: "generation-1-sealed".into(),
        sealed_audit_untouched: true,
        confidence_level: 0.99,
        catastrophic_risk_target: 0.01,
        accepted_source_groups: 459,
        catastrophic_source_groups: 0,
        posterior_lower_bound_threshold: preset.posterior_lower_bound_threshold,
        minimum_top2_class_margin_bits: gates.min_top2_class_margin_bits,
        maximum_posterior_predictive_bits_per_block: preset.max_posterior_predictive_bits_per_block,
        maximum_support_isotopy_displacement_px: preset.max_support_isotopy_displacement_px,
        maximum_evidence_palette_shift_codes: 2,
        minimum_palette_support_px: 1,
        maximum_palette_interval_radius_codes: 4,
        maximum_abs_residual_lag1: gates.max_abs_residual_lag1,
        maximum_topology_entropy_bits: gates.max_topology_entropy_bits,
        maximum_formation_entropy_bits: gates.max_formation_entropy_bits,
        minimum_perturbation_stability: gates.min_perturbation_stability,
        empirical_unexplored_relative_mass_upper_bound: Some(
            preset.empirical_unexplored_relative_mass_upper_bound,
        ),
        supported_selection_classes: vec!["flat2/general".into()],
        paint_calibration_classes: vec![vice_core::PaintCalibrationClass {
            name: "flat2/general|delivery:t0/srgb/box/u8/opaque|paint:fg-point+bg-point".into(),
            accepted_source_groups: 459,
        }],
        buckets: Vec::new(),
    }
}

#[test]
fn each_preset_requires_its_own_frozen_calibration_gate() {
    let gates = M7ReleaseGates::from_file(&file("frozen")).expect("frozen values load");
    let quality = calibration_for(gates.quality_calibration, gates);
    let fast = calibration_for(gates.fast_calibration, gates);

    assert!(confidence_fields_match(
        &quality,
        vice_core::Preset::Quality,
        gates
    ));
    assert!(!confidence_fields_match(
        &quality,
        vice_core::Preset::Fast,
        gates
    ));
    assert!(confidence_fields_match(
        &fast,
        vice_core::Preset::Fast,
        gates
    ));
    assert!(!confidence_fields_match(
        &fast,
        vice_core::Preset::Quality,
        gates
    ));
}

#[test]
fn a_provisional_wall_clock_miss_cannot_refuse_an_m7_release() {
    assert!(!runtime_blocks_release(false));
    assert!(!runtime_blocks_release(true));
    assert!(M7_RUNTIME_POLICY.contains("non-blocking for release"));
}
