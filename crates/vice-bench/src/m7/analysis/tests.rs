use super::*;
use crate::gt::grammar::M7_PROCEDURAL_GENERATION;
use crate::m7::{BoundaryGateCounts, BoundaryTail};
use crate::m7::{MeasurementReport, TopologyComparison};

fn boundary(
    samples: u64,
    p95_px: f64,
    p99_px: f64,
    max_px: f64,
    p95_at_or_below: u64,
    p99_at_or_below: u64,
) -> BoundaryTail {
    BoundaryTail {
        samples,
        p95_px,
        p99_px,
        max_px,
        gate_counts: BoundaryGateCounts {
            p95_gate_px: PROPOSED_BOUNDARY_P95_PX,
            samples_at_or_below_p95_gate: p95_at_or_below,
            p99_gate_px: PROPOSED_BOUNDARY_P99_PX,
            samples_at_or_below_p99_gate: p99_at_or_below,
        },
    }
}

fn row(index: usize, catastrophic: bool) -> MeasurementRow {
    MeasurementRow {
        group_id: format!("group-{index:03}"),
        scene_id: format!("group-{index:03}#a"),
        shape_family: "synthetic".into(),
        cell_id: "s512_ptiny-skia".into(),
        size_px: 512,
        rasterizer: "tiny-skia".into(),
        identifiability: "identifiable".into(),
        core_runtime_ms: 100,
        runtime_stages: vice_core::RuntimeStageSummary::default(),
        court_runtime_ms: 1,
        row_elapsed_ms: 101,
        decision_status: "ambiguous".into(),
        decision_reason: Some("confidence".into()),
        production_provenance: false,
        production_accepted: false,
        candidate_available: true,
        selected_hypothesis_id: Some("h".into()),
        selected_scene_digest_sha256: Some("4".repeat(64)),
        selected_delivery_digest_sha256: Some("5".repeat(64)),
        selected_artifact_bundle_sha256: Some("6".repeat(64)),
        selected_complexity: None,
        internal_baseline: None,
        internal_baseline_refusals: Vec::new(),
        pf_oracle: None,
        cost_refusal_histogram: Vec::new(),
        numerical_conditioning: crate::m7::NumericalConditioningDiagnostics::default(),
        search_truncated: Some(true),
        explored_mass: Some(1.0),
        topology_classes_upper_bound: Some(1),
        formation_classes_upper_bound: Some(1),
        top_topology_explored_mass: Some(1.0),
        top_formation_explored_mass: Some(1.0),
        selected_delivery_mass: Some(1.0),
        retained_normalized_mass: Some(1.0),
        delivery_classes: Some(1),
        top2_class_margin_bits: None,
        posterior_lower_bound: None,
        posterior_bound_status: "unknown".into(),
        unexplored_proxy_hypotheses: Some(10),
        candidate_bytes: 100,
        serialized_pixel_bits: Some(1.0),
        serialized_pixel_bits_per_block: Some(0.01),
        support_isotopy_displacement_px: Some(0.1),
        empirical_correlation_length_px: Some(1.0),
        max_abs_lag1: Some(0.0),
        topology_entropy_upper_bound: Some(0.0),
        topology_entropy_bound_status: "empirically_calibrated".into(),
        formation_entropy_upper_bound: Some(0.0),
        formation_entropy_bound_status: "empirically_calibrated".into(),
        perturbation_stability: Some(1.0),
        phase_envelope_stable: Some(true),
        sample_step_certificate_stable: Some(true),
        render_tolerance_certificate_stable: Some(true),
        render_tolerance_refusal: None,
        solver_certificate_stable: Some(true),
        topology: Some(TopologyComparison {
            truth_visible_faces: 2,
            selected_visible_faces: if catastrophic { 1 } else { 2 },
            truth_components: 1,
            selected_components: 1,
            truth_holes: 0,
            selected_holes: 0,
            truth_exterior: "opaque".into(),
            selected_exterior: "opaque".into(),
            exact: !catastrophic,
        }),
        boundary: Some(boundary(100, 0.2, 0.3, 0.4, 100, 100)),
        max_palette_code_delta: Some(1),
        profile_max_channel_delta: Some(0),
        profile_mean_channel_delta: Some(0.0),
        internal_to_pure_max_channel_delta: Some(0),
        internal_to_pure_mean_channel_delta: Some(0.0),
        internal_to_seam_max_channel_delta: Some(0),
        internal_to_seam_mean_channel_delta: Some(0.0),
        verifier_clean: true,
        measurement_refusal: None,
    }
}

fn report_with_groups(group_count: usize, catastrophic: bool) -> MeasurementReport {
    let rows = (0..group_count)
        .map(|index| row(index, catastrophic && index == 0))
        .collect::<Vec<_>>();
    MeasurementReport {
        schema: M7_MEASUREMENT_SCHEMA.into(),
        scope: "calibration".into(),
        split: "calibration".into(),
        preset: vice_core::Preset::Quality,
        procedural_generation: M7_PROCEDURAL_GENERATION,
        population_policy: M7_CALIBRATION_POPULATION_POLICY.into(),
        procedural_variants_per_family: 200,
        mandatory_sizes_px: vec![128, 256, 512],
        rasterizers: vec!["tiny-skia".into()],
        identity: vice_opt::ModelIdentity::new(
            "0".repeat(64),
            "1".repeat(64),
            "2".repeat(64),
            "3".repeat(64),
        )
        .unwrap(),
        delivery_policy_sha256: "4".repeat(64),
        confidence_calibration: None,
        included_shards: vec![0],
        shard_count: 1,
        max_workers_per_shard: 1,
        complete: true,
        expected_renders_included_shards: rows.len() as u64,
        resumed_rows: 0,
        runs: 1,
        source_groups: rows.len() as u64,
        renders: rows.len() as u64,
        candidates_available: rows.len() as u64,
        truncated_renders: rows.len() as u64,
        rows,
        elapsed_ms: 1,
        peak_working_set_bytes: 1024,
    }
}

fn report(catastrophic: bool) -> MeasurementReport {
    report_with_groups(459, catastrophic)
}

#[test]
fn zero_failure_459_group_population_mints_a_core_valid_calibration() {
    let report = report(false);
    let analysis = analyze_calibration(&report, &AuditSeal::sealed(M7_PROCEDURAL_GENERATION))
        .expect("analysis succeeds");
    assert!(analysis.gate_met);
    assert!(analysis.runtime_isolated);
    assert_eq!(analysis.runtime_met, Some(true));
    assert_eq!(analysis.runtime_scope_size_px, 512);
    assert_eq!(
        analysis.delivery_seal,
        vice_verify::DeliverySealConfig {
            max_profile_channel_delta: 1,
            max_profile_mean_channel_delta: 0.0,
            max_internal_channel_delta: 0,
            max_internal_mean_channel_delta: 0.0,
        }
    );
    let selected = analysis
        .threshold_evaluations
        .iter()
        .find(|evaluation| evaluation.eligible)
        .expect("eligible threshold");
    assert!(selected.boundary_p95_met);
    assert!(selected.boundary_p99_met);
    assert!(selected.boundary_max_met);
    assert_eq!(selected.accepted_render_boundary_p99_q99_px, Some(0.3));
    assert_eq!(selected.accepted_boundary_max_px, Some(0.4));
    let calibration = analysis.calibration.expect("calibration");
    assert_eq!(calibration.accepted_source_groups, 459);
    assert_eq!(calibration.catastrophic_source_groups, 0);
    assert!(calibration.validate_for_identity(&report.identity).is_ok());
}

#[test]
fn calibration_refuses_a_report_from_another_audit_generation() {
    let error = analyze_calibration(
        &report(false),
        &AuditSeal::sealed(M7_PROCEDURAL_GENERATION - 1),
    )
    .expect_err("a burned generation must not calibrate its successor");
    assert!(error.contains("current procedural/audit generation"));
}

#[test]
fn a_parallel_calibration_mints_confidence_but_not_a_runtime_claim() {
    let mut report = report(false);
    report.max_workers_per_shard = 2;
    let analysis = analyze_calibration(&report, &AuditSeal::sealed(M7_PROCEDURAL_GENERATION))
        .expect("analysis succeeds");
    assert!(analysis.gate_met);
    assert!(analysis.production_config.is_some());
    assert!(!analysis.runtime_isolated);
    assert_eq!(analysis.runtime_met, None);
    assert!(!analysis.runtime_release_blocking);
}

#[test]
fn an_indistinguishable_bad_row_prevents_a_zero_failure_calibration() {
    let analysis = analyze_calibration(&report(true), &AuditSeal::sealed(M7_PROCEDURAL_GENERATION))
        .expect("analysis succeeds");
    assert!(!analysis.gate_met);
    assert!(analysis.calibration.is_none());
}

#[test]
fn predictive_mismatch_abstains_before_reliability_is_minted() {
    let mut report = report_with_groups(460, true);
    report.rows[0].serialized_pixel_bits_per_block = Some(0.11);
    let analysis = analyze_calibration(&report, &AuditSeal::sealed(M7_PROCEDURAL_GENERATION))
        .expect("analysis succeeds");
    assert!(analysis.gate_met);
    let calibration = analysis.calibration.expect("calibration");
    assert_eq!(calibration.accepted_source_groups, 459);
    assert_eq!(calibration.catastrophic_source_groups, 0);
}

#[test]
fn population_p95_allows_a_sparse_non_catastrophic_render_tail() {
    let mut report = report(false);
    report.rows[0].boundary = Some(boundary(100, 0.50, 0.55, 0.70, 95, 100));
    let analysis = analyze_calibration(&report, &AuditSeal::sealed(M7_PROCEDURAL_GENERATION))
        .expect("analysis succeeds");
    assert!(analysis.gate_met);
    let selected = analysis
        .threshold_evaluations
        .iter()
        .find(|evaluation| evaluation.eligible)
        .expect("eligible threshold");
    assert_eq!(selected.accepted_render_boundary_p95_q95_px, Some(0.2));
    assert_eq!(selected.accepted_render_boundary_p99_q99_px, Some(0.3));
    assert_eq!(selected.accepted_boundary_max_px, Some(0.7));
}

#[test]
fn population_p95_rejects_a_material_high_tail_fraction() {
    let mut report = report(false);
    for row in report.rows.iter_mut().take(24) {
        row.boundary = Some(boundary(100, 0.50, 0.55, 0.70, 0, 100));
    }
    let analysis = analyze_calibration(&report, &AuditSeal::sealed(M7_PROCEDURAL_GENERATION))
        .expect("analysis succeeds");
    assert!(!analysis.gate_met);
    assert!(analysis.calibration.is_none());
}

#[test]
fn differently_preregistered_boundary_counts_cannot_enter_calibration() {
    let mut report = report(false);
    report.rows[0]
        .boundary
        .as_mut()
        .expect("fixture boundary")
        .gate_counts
        .p95_gate_px = 0.36;
    let analysis = analyze_calibration(&report, &AuditSeal::sealed(M7_PROCEDURAL_GENERATION))
        .expect("analysis succeeds");
    assert!(!analysis.gate_met);
    assert!(analysis.calibration.is_none());
}

#[test]
fn population_max_still_rejects_a_single_render_outlier() {
    let mut report = report(false);
    report.rows[0].boundary = Some(boundary(100, 0.50, 0.61, 1.51, 95, 99));
    let analysis = analyze_calibration(&report, &AuditSeal::sealed(M7_PROCEDURAL_GENERATION))
        .expect("analysis succeeds");
    assert!(!analysis.gate_met);
    assert!(analysis.calibration.is_none());
}

#[test]
fn observed_support_separates_a_tail_failure_without_reading_ground_truth_in_production() {
    let mut report = report_with_groups(460, true);
    report.rows[0].support_isotopy_displacement_px = Some(0.2);
    let analysis = analyze_calibration(&report, &AuditSeal::sealed(M7_PROCEDURAL_GENERATION))
        .expect("analysis succeeds");
    assert!(analysis.gate_met);
    let calibration = analysis.calibration.expect("calibration");
    assert_eq!(calibration.accepted_source_groups, 459);
    assert_eq!(calibration.catastrophic_source_groups, 0);
    assert_eq!(calibration.maximum_support_isotopy_displacement_px, 0.1);
}

#[test]
fn predictive_ceiling_is_measured_instead_of_hard_coded_to_point_one() {
    let mut report = report(false);
    for row in &mut report.rows {
        row.serialized_pixel_bits_per_block = Some(0.2);
    }
    let analysis = analyze_calibration(&report, &AuditSeal::sealed(M7_PROCEDURAL_GENERATION))
        .expect("analysis succeeds");
    assert!(analysis.gate_met);
    assert_eq!(
        analysis
            .calibration
            .expect("calibration")
            .maximum_posterior_predictive_bits_per_block,
        0.2
    );
}

#[test]
fn empirical_omitted_mass_cannot_hide_multi_class_entropy() {
    let mut row = row(0, false);
    row.topology_classes_upper_bound = Some(4);
    row.top_topology_explored_mass = Some(0.1);
    let entropy = calibrated_entropy_upper_bound(&row, 10.0, true).expect("finite entropy bound");
    assert!(entropy > PROPOSED_MAX_TOPOLOGY_ENTROPY_BITS);
    assert!(!fixed_diagnostics_permit(
        &row,
        10.0,
        vice_verify::DeliverySealConfig {
            max_profile_channel_delta: 0,
            max_profile_mean_channel_delta: 0.0,
            max_internal_channel_delta: 0,
            max_internal_mean_channel_delta: 0.0,
        }
    ));
}
