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

#[test]
fn failed_free_chain_baseline_is_published_as_a_typed_court_refusal() {
    let groups = groups_with_variants_filtered_for_generation(
        MeasurementScope::Calibration.variants(),
        M7_PROCEDURAL_GENERATION,
        |group_id| group_id == "authored/keyhole",
    )
    .unwrap();
    let group = groups
        .iter()
        .find(|group| group.id == "authored/keyhole")
        .expect("development calibration contains the keyhole adversary");
    let cell = MeasurementScope::Calibration
        .cells()
        .into_iter()
        .find(|cell| cell.size_px == 128)
        .expect("calibration declares a 128px court cell");
    let row = measure_one(
        &group.id,
        &group.shape_family,
        &group.scenes[0],
        &cell,
        group
            .equivalence_class
            .as_ref()
            .map_or(1, |class| class.members.len()),
        &vice_core::CoreConfig::development(),
        MeasurementExecution {
            preset: vice_core::Preset::Quality,
            capture_baseline: true,
        },
    );
    assert!(row.candidate_available);
    assert!(row.internal_baseline.is_none());
    assert!(!row.internal_baseline_refusals.is_empty());
    assert!(row
        .internal_baseline_refusals
        .iter()
        .all(|refusal| refusal.starts_with("Preseal:")));
}

pub(super) fn synthetic_row(group: &str) -> MeasurementRow {
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
        internal_baseline_refusals: Vec::new(),
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
        evidence_palette_shift_codes: None,
        palette_support_px: None,
        palette_interval_radius_codes: None,
        paint_calibration_class: None,
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
        population_policy: M7_CALIBRATION_POPULATION_POLICY.into(),
        population_commitment_sha256: "a".repeat(64),
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
        execution_attestation: None,
    }
}

pub(super) fn sealed_report(role: M7RunRole, evidence_digit: char) -> MeasurementReport {
    let mut report = synthetic_report(0, 1);
    report.scope = "sealed_audit".into();
    report.split = "sealed_audit".into();
    report.preset = role.preset();
    report.population_policy = M7_SEALED_POPULATION_POLICY.into();
    report.population_commitment_sha256 =
        crate::gt::corpus::M7_SUCCESSOR_POPULATION_COMMITMENT_SHA256.into();
    report.included_shards = vec![0];
    report.shard_count = 1;
    report.max_workers_per_shard = role.workers();
    report.rows.clear();
    for family in crate::gt::corpus::M7_SEALED_FLAT2_FAMILIES {
        for variant in 0..M7_RELEASE_PROCEDURAL_VARIANTS {
            let group_id = format!("proc/{family}/{variant:03}");
            for cell in MeasurementScope::SealedAudit.cells() {
                let mut row = synthetic_row(&group_id);
                row.scene_id = format!("{group_id}#a");
                row.cell_id = cell.id();
                row.size_px = cell.size_px;
                row.rasterizer = cell.profile.as_str().into();
                report.rows.push(row);
            }
        }
    }
    report.source_groups = M7_SEALED_SOURCE_GROUPS;
    report.renders = M7_SEALED_ROWS;
    report.expected_renders_included_shards = M7_SEALED_ROWS;
    report.complete = true;
    let production_config_sha256 = match role.preset() {
        Preset::Fast => vice_core::M7_FAST_PRODUCTION_CONFIG_SHA256,
        Preset::Quality => vice_core::M7_QUALITY_PRODUCTION_CONFIG_SHA256,
    };
    attach_execution_attestation(
        &mut report,
        MeasurementExecutionContext {
            schema: M7_EXECUTION_ATTESTATION_SCHEMA.into(),
            role,
            run_id: format!("m7-generation8-{role:?}"),
            candidate_commit_sha: "1".repeat(40),
            runner_attestation_sha256: "2".repeat(64),
            production_config_sha256: production_config_sha256.into(),
            corpus_sha256: "3".repeat(64),
            population_commitment_sha256:
                crate::gt::corpus::M7_SUCCESSOR_POPULATION_COMMITMENT_SHA256.into(),
            workers: role.workers(),
            shard_count: 1,
        },
        evidence_digit.to_string().repeat(64),
    )
    .unwrap();
    report
}

#[test]
fn sealed_population_and_execution_are_exact_and_tamper_evident() {
    let report = sealed_report(M7RunRole::QualityPrimary, 'a');
    validate_sealed_population(&report).unwrap();
    validate_execution_attestation(&report).unwrap();

    let mut missing = report.clone();
    missing.rows.pop();
    missing.renders -= 1;
    assert!(validate_sealed_population(&missing).is_err());

    let mut tampered = report;
    tampered.rows[0].decision_status = "success".into();
    assert!(validate_execution_attestation(&tampered).is_err());
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
    let groups =
        groups_with_variants_filtered_for_generation(1, M7_PROCEDURAL_GENERATION, |_| true)
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

#[test]
#[ignore = "explicit M7 development population court; run before calibration freeze"]
fn generation_four_failure_classes_have_a_verified_candidate_after_the_generic_repairs() {
    let cases = [
        ("proc/dot_cluster/002", 128),
        ("proc/dot_cluster/147", 512),
        ("proc/nested_island/057", 128),
        ("proc/dot_cluster/039", 256),
        ("proc/dot_cluster/112", 512),
        ("proc/nested_island/023", 128),
        ("proc/thin_bridge/195", 256),
        ("proc/thin_bridge/000", 128),
    ];
    let wanted = cases
        .iter()
        .map(|(group, _)| *group)
        .collect::<std::collections::BTreeSet<_>>();
    let groups =
        groups_with_variants_filtered_for_generation(200, 4, |group| wanted.contains(group))
            .expect("construct the frozen failure witnesses");
    let cells = MeasurementScope::SealedAudit.cells();
    let config = CoreConfig::development_for(Preset::Fast);
    for (group, size) in cases {
        let source = groups
            .iter()
            .find(|source| source.id == group)
            .unwrap_or_else(|| panic!("missing {group}"));
        let cell = cells
            .iter()
            .find(|cell| cell.size_px == size)
            .unwrap_or_else(|| panic!("missing {size}px release cell"));
        let row = measure_one(
            source.id.as_str(),
            source.shape_family.as_str(),
            &source.scenes[0],
            cell,
            1,
            &config,
            MeasurementExecution {
                preset: Preset::Fast,
                capture_baseline: false,
            },
        );
        assert!(
            row.candidate_available,
            "{group} at {size}px remained {:?}: {:?}",
            row.decision_reason, row.measurement_refusal
        );
        assert!(row.topology.as_ref().is_some_and(|topology| topology.exact));
        assert!(row.verifier_clean);
    }
}

#[test]
#[ignore = "explicit full preflight over every generation-four Fast refusal"]
fn every_generation_four_fast_refusal_is_remeasured_in_one_preflight() {
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;

    let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let old_report_path =
        repository.join("runs/m7/generation4-audit-fast-clean-event-6ac2659/merged.json");
    let config_path =
        repository.join("runs/m7/calibration-generation4-flat2-fast/production-config.json");
    let output_path = repository.join("runs/m7/generation4-fast-refusal-preflight-current.json");
    let old_report = read_report(&old_report_path).expect("read generation-four Fast audit");
    let old_refusals = old_report
        .rows
        .iter()
        .filter(|row| !row.production_accepted)
        .map(|row| {
            (
                row.group_id.clone(),
                row.scene_id.clone(),
                row.cell_id.clone(),
            )
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        old_refusals.len(),
        1_116,
        "generation-four refusal court drifted"
    );

    let wanted_groups = old_refusals
        .iter()
        .map(|(group, _, _)| group.as_str())
        .collect::<BTreeSet<_>>();
    let groups =
        groups_with_variants_filtered_for_generation(M7_RELEASE_PROCEDURAL_VARIANTS, 4, |group| {
            wanted_groups.contains(group)
        })
        .expect("construct every refused generation-four group");
    let cells = MeasurementScope::SealedAudit
        .cells()
        .into_iter()
        .map(|cell| (cell.id(), cell))
        .collect::<BTreeMap<_, _>>();
    let mut tasks = Vec::with_capacity(old_refusals.len());
    for group in groups {
        let equivalence_members = group
            .equivalence_class
            .as_ref()
            .map_or(1, |class| class.members.len());
        for scene in &group.scenes {
            for (wanted_group, wanted_scene, wanted_cell) in &old_refusals {
                if wanted_group == &group.id && wanted_scene == scene.id() {
                    tasks.push((
                        group.id.clone(),
                        group.shape_family.clone(),
                        scene.clone(),
                        *cells
                            .get(wanted_cell)
                            .unwrap_or_else(|| panic!("missing cell {wanted_cell}")),
                        equivalence_members,
                    ));
                }
            }
        }
    }
    assert_eq!(
        tasks.len(),
        old_refusals.len(),
        "preflight task matrix is incomplete"
    );

    let config = CoreConfig::load_production_for(Preset::Fast, &config_path)
        .expect("load generation-four Fast production decision policy");
    let workers = std::thread::available_parallelism()
        .map_or(4, usize::from)
        .clamp(1, 8);
    let chunk_size = tasks.len().div_ceil(workers);
    let total = old_refusals.len();
    let completed = AtomicUsize::new(0);
    let started = Instant::now();
    let rows = std::thread::scope(|scope| {
        let handles = tasks
            .chunks(chunk_size)
            .map(|chunk| {
                let config = &config;
                let completed = &completed;
                scope.spawn(move || {
                    chunk
                        .iter()
                        .map(|(group, family, scene, cell, equivalence_members)| {
                            let row = measure_one(
                                group,
                                family,
                                scene,
                                cell,
                                *equivalence_members,
                                config,
                                MeasurementExecution {
                                    preset: Preset::Fast,
                                    capture_baseline: false,
                                },
                            );
                            let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
                            if done.is_multiple_of(25) || done == total {
                                eprintln!("M7 refusal preflight: {done}/{total}");
                            }
                            row
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .flat_map(|handle| handle.join().expect("preflight worker did not panic"))
            .collect::<Vec<_>>()
    });
    let remaining = rows
        .iter()
        .filter(|row| !row.production_accepted)
        .collect::<Vec<_>>();
    let report = serde_json::json!({
        "schema": "vice-classic/m7-generation4-refusal-preflight/v1",
        "source_report": old_report_path,
        "decision_config": config_path,
        "refusals_remeasured": rows.len(),
        "production_accepted": rows.len() - remaining.len(),
        "remaining_refusals": remaining.len(),
        "elapsed_ms": started.elapsed().as_millis(),
        "rows": rows,
    });
    std::fs::write(
        &output_path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&report).expect("preflight report serializes")
        ),
    )
    .expect("write complete preflight report");
    assert!(
        remaining.is_empty(),
        "{} generation-four Fast refusals remain; complete report: {}",
        remaining.len(),
        output_path.display()
    );
}
