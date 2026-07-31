use super::*;

#[test]
fn distance_and_quantiles_are_not_vacuous() {
    let source = [(Pt::new(0.0, 0.0), Pt::new(2.0, 0.0))];
    let target = [(Pt::new(0.0, 1.0), Pt::new(2.0, 1.0))];
    let mut distances = directed_distances(&source, &target);
    distances.sort_by(f64::total_cmp);
    assert!(distances.len() > 4);
    assert!((quantile(&distances, 0.95) - 1.0).abs() < 1e-12);
}

#[test]
fn boundary_index_is_exactly_the_brute_force_metric() {
    let segments = vec![
        (Pt::new(-2.0, 1.0), Pt::new(4.0, 1.5)),
        (Pt::new(3.0, -4.0), Pt::new(3.0, 8.0)),
        (Pt::new(9.0, 2.0), Pt::new(11.0, 5.0)),
    ];
    let index = SegmentIndex::build(segments.clone());
    for point in [
        Pt::new(0.0, 0.0),
        Pt::new(3.0, 7.0),
        Pt::new(10.0, 4.0),
        Pt::new(-20.0, 30.0),
    ] {
        let brute = segments
            .iter()
            .map(|&(a, b)| point_segment_distance(point, a, b))
            .fold(f64::INFINITY, f64::min);
        assert!((index.nearest(point) - brute).abs() < 1e-12);
    }
}

#[test]
fn smoke_scope_has_one_declared_non_inverse_crime_cell() {
    let cells = MeasurementScope::Smoke.cells();
    assert_eq!(cells.len(), 1);
    assert!(!cells[0].is_inverse_crime());
}

fn synthetic_row(group: &str) -> MeasurementRow {
    MeasurementRow {
        group_id: group.into(),
        scene_id: format!("{group}#a"),
        shape_family: "synthetic".into(),
        cell_id: "cell".into(),
        size_px: 128,
        rasterizer: "tiny-skia".into(),
        identifiability: "identifiable".into(),
        core_runtime_ms: 1,
        runtime_stages: vice_core::RuntimeStageSummary::default(),
        court_runtime_ms: 1,
        row_elapsed_ms: 2,
        decision_status: "measurement_refused".into(),
        decision_reason: Some("measurement_failure".into()),
        production_provenance: false,
        production_accepted: false,
        candidate_available: false,
        selected_hypothesis_id: None,
        selected_scene_digest_sha256: None,
        selected_delivery_digest_sha256: None,
        selected_artifact_bundle_sha256: None,
        selected_complexity: None,
        internal_baseline: None,
        pf_oracle: None,
        cost_refusal_histogram: Vec::new(),
        numerical_conditioning: NumericalConditioningDiagnostics::default(),
        search_truncated: None,
        explored_mass: None,
        topology_classes_upper_bound: None,
        formation_classes_upper_bound: None,
        top_topology_explored_mass: None,
        top_formation_explored_mass: None,
        selected_delivery_mass: None,
        retained_normalized_mass: None,
        delivery_classes: None,
        top2_class_margin_bits: None,
        posterior_lower_bound: None,
        posterior_bound_status: "absent".into(),
        unexplored_proxy_hypotheses: None,
        candidate_bytes: 0,
        serialized_pixel_bits: None,
        serialized_pixel_bits_per_block: None,
        support_isotopy_displacement_px: None,
        empirical_correlation_length_px: None,
        max_abs_lag1: None,
        topology_entropy_upper_bound: None,
        topology_entropy_bound_status: "absent".into(),
        formation_entropy_upper_bound: None,
        formation_entropy_bound_status: "absent".into(),
        perturbation_stability: None,
        phase_envelope_stable: None,
        sample_step_certificate_stable: None,
        render_tolerance_certificate_stable: None,
        render_tolerance_refusal: None,
        solver_certificate_stable: None,
        topology: None,
        boundary: None,
        max_palette_code_delta: None,
        profile_max_channel_delta: None,
        profile_mean_channel_delta: None,
        internal_to_pure_max_channel_delta: None,
        internal_to_pure_mean_channel_delta: None,
        internal_to_seam_max_channel_delta: None,
        internal_to_seam_mean_channel_delta: None,
        verifier_clean: false,
        measurement_refusal: Some("synthetic".into()),
    }
}

pub(super) fn synthetic_report(shard: u32, shard_count: u32) -> MeasurementReport {
    let row = synthetic_row(&format!("group-{shard}"));
    MeasurementReport {
        schema: M7_MEASUREMENT_SCHEMA.into(),
        scope: "calibration".into(),
        split: "calibration".into(),
        preset: Preset::Quality,
        procedural_generation: M7_PROCEDURAL_GENERATION,
        population_policy: M7_ALL_SPLIT_POPULATION_POLICY.into(),
        procedural_variants_per_family: M7_RELEASE_PROCEDURAL_VARIANTS,
        mandatory_sizes_px: M7_MANDATORY_SIZES.to_vec(),
        rasterizers: vec!["tiny-skia".into()],
        identity: vice_opt::ModelIdentity {
            universe_sha256: "u".into(),
            pricing_sha256: "p".into(),
            backend_sha256: "b".into(),
            config_sha256: "c".into(),
        },
        delivery_policy_sha256: "d".into(),
        confidence_calibration: None,
        included_shards: vec![shard],
        shard_count,
        max_workers_per_shard: 2,
        complete: true,
        expected_renders_included_shards: 1,
        resumed_rows: 0,
        runs: 1,
        rows: vec![row],
        source_groups: 1,
        renders: 1,
        candidates_available: 0,
        truncated_renders: 0,
        elapsed_ms: 2,
        peak_working_set_bytes: 1024,
    }
}

#[test]
fn source_group_shards_are_stable_and_never_multi_assign() {
    for index in 0..100 {
        let id = format!("group/{index:03}");
        let shard = measurement_shard(&id, 7);
        assert!(shard < 7);
        assert_eq!(measurement_shard(&id, 7), shard);
        assert_eq!(
            (0..7)
                .filter(|candidate| measurement_shard(&id, 7) == *candidate)
                .count(),
            1
        );
    }
}

#[test]
fn successor_audit_population_excludes_reused_nonprocedural_sources() {
    assert!(MeasurementScope::SealedAudit.admits_origin(FixtureOrigin::Procedural));
    assert!(!MeasurementScope::SealedAudit.admits_origin(FixtureOrigin::Authored));
    assert!(!MeasurementScope::SealedAudit.admits_origin(FixtureOrigin::Adversarial));
    assert_eq!(
        MeasurementScope::SealedAudit.population_policy(),
        M7_SEALED_POPULATION_POLICY
    );
    for family in ["nested_island", "arc_disk", "thin_bridge", "dot_cluster"] {
        assert!(MeasurementScope::SealedAudit.admits_shape_family(family));
    }
    for family in ["shared_edge", "two_islands", "triple_junction"] {
        assert!(!MeasurementScope::SealedAudit.admits_shape_family(family));
    }
}

#[test]
fn calibration_and_audit_share_the_flat2_supported_model_boundary() {
    let groups = groups_with_variants_filtered_for_generation(
        1,
        M7_PROCEDURAL_GENERATION,
        |_| true,
    )
    .unwrap();
    let group = |family: &str| {
        groups
            .iter()
            .find(|group| group.shape_family == family)
            .unwrap_or_else(|| panic!("missing {family}"))
    };

    assert!(MeasurementScope::Calibration
        .admits_group(group("annulus"))
        .unwrap());
    assert!(!MeasurementScope::Calibration
        .admits_group(group("shared_edge"))
        .unwrap());
    assert!(!MeasurementScope::SealedAudit
        .admits_group(group("shared_edge"))
        .unwrap());
    assert!(MeasurementScope::SealedAudit
        .admits_group(group("nested_island"))
        .unwrap());
}

#[test]
fn merge_is_complete_only_for_one_copy_of_every_shard() {
    let partial = merge_reports(vec![synthetic_report(1, 2)]).expect("partial merge");
    assert!(!partial.complete);
    let merged = merge_reports(vec![synthetic_report(1, 2), synthetic_report(0, 2)])
        .expect("complete merge");
    assert!(merged.complete);
    assert_eq!(merged.included_shards, vec![0, 1]);
    assert_eq!(merged.renders, 2);
    assert!(merge_reports(vec![synthetic_report(0, 2), synthetic_report(0, 2)]).is_err());
}

#[test]
fn merge_refuses_mixed_generations_and_population_policies() {
    let first = synthetic_report(0, 2);
    let mut different_generation = synthetic_report(1, 2);
    different_generation.procedural_generation += 1;
    assert!(merge_reports(vec![first.clone(), different_generation]).is_err());

    let mut different_population = synthetic_report(1, 2);
    different_population.population_policy = "vice-classic/m7-population/other/v1".into();
    assert!(merge_reports(vec![first, different_population]).is_err());
}

#[test]
fn quality_keeps_the_certified_primary_lane_on_the_successor_annulus_tail() {
    const SHARDS: u32 = 4096;
    let group = "proc/annulus/000";
    let shard = measurement_shard(group, SHARDS);
    let mut request = MeasurementRequest::new(MeasurementScope::Calibration);
    request.preset = Preset::Quality;
    request.size_filter = Some(128);
    request.workers = 1;
    request.shard_index = shard;
    request.shard_count = SHARDS;

    let report = measure(request).expect("targeted calibration measurement");
    let row = report
        .rows
        .iter()
        .find(|row| row.group_id == group)
        .expect("the stable shard contains the regression group");
    let boundary = row
        .boundary
        .as_ref()
        .expect("the protected primary lane remains measurable");

    assert!(row.candidate_available);
    assert!(
        row.selected_hypothesis_id
            .as_deref()
            .is_some_and(|id| id.contains("/t0/")),
        "{:?}",
        row.selected_hypothesis_id
    );
    // p99 is a population quantile, not a per-row ceiling. A single row in
    // the upper one percent may exceed it while the preregistered population
    // still passes. The normative per-row bound is the frozen maximum.
    assert!(
        boundary.max_px <= M7_BOUNDARY_MAX_GATE_PX,
        "{}",
        boundary.max_px
    );
}
