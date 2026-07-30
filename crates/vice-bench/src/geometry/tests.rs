use super::*;

#[test]
fn optimizer_recovery_is_not_relabelled_by_an_independent_gt_diagnostic() {
    let (recovered, truth_improved) = classify_recovery(10.0, 4.0, Some(0.08), Some(0.14));
    assert!(recovered);
    assert_eq!(truth_improved, Some(false));
}

#[test]
fn the_geometry_intervention_config_binds_the_model_and_pricing_versions() {
    let config = GeometryOracleConfig::default();
    assert_eq!(
        config.model_universe_hash,
        model_universe_hash(&SupportedModelUniverseV1::v1())
    );
    assert_eq!(
        config.geometry_pricing_sha256,
        sha256_hex(vice_fit::pricing_surface_v1().as_bytes())
    );
    assert_eq!(config.model_universe_hash.len(), 64);
    assert_eq!(config.geometry_pricing_sha256.len(), 64);
    assert_eq!(config.backend_source_sha256.len(), 64);
    assert_eq!(config.max_canonical_cuts, vice_fit::MAX_CANONICAL_CUTS);
}

#[test]
fn the_backend_digest_covers_every_fitting_and_intervention_rust_source() {
    fn collect_rs(root: &std::path::Path, workspace: &std::path::Path, out: &mut Vec<String>) {
        for entry in std::fs::read_dir(root).expect("source directory") {
            let path = entry.expect("source entry").path();
            if path.is_dir() {
                collect_rs(&path, workspace, out);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                out.push(
                    path.strip_prefix(workspace)
                        .expect("source lies in workspace")
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }

    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest
        .parent()
        .and_then(std::path::Path::parent)
        .expect("workspace");
    let mut actual = Vec::new();
    collect_rs(
        &workspace.join("crates/vice-fit/src"),
        workspace,
        &mut actual,
    );
    collect_rs(&manifest.join("src/geometry"), workspace, &mut actual);
    actual.sort();

    let mut registered: Vec<String> = BACKEND_SOURCE_PATHS
        .iter()
        .map(|(path, _)| (*path).to_string())
        .filter(|path| path.ends_with(".rs"))
        .collect();
    registered.sort();
    assert_eq!(
        registered, actual,
        "a fitting/intervention source can change behaviour without moving the compatibility key"
    );
}

fn one_boundary_measurement() -> GeometryMeasurements {
    let config = GeometryOracleConfig::default();
    let key = compatibility_key(&config, "cfg", "fixture");
    let arms = ARM_IDS
        .iter()
        .map(|arm| GeometryArmResult {
            arm,
            compatibility_key: key.clone(),
            candidate_models: if *arm == "G11" || *arm == "G20" { 2 } else { 1 },
            selected_source: if *arm == "G00" || *arm == "G01" {
                "automatic"
            } else {
                "forced_gt"
            },
            families: vec!["line"],
            breakpoints: Vec::new(),
            smooth: if *arm == "G20" {
                vec![true]
            } else {
                Vec::new()
            },
            closure_smooth: false,
            relations_considered: usize::from(*arm == "G10"),
            relations_selected: usize::from(*arm == "G10"),
            primitives_considered: usize::from(*arm == "G00"),
            primitive_selected: *arm == "G00",
            selected_geometry: if *arm == "G00" {
                "loop_primitive"
            } else {
                "typed_chain"
            },
            geometry_sha256: match *arm {
                "G00" => "auto-0",
                "G01" => "auto-1",
                "G10" | "G20" => "forced-0",
                "G11" => "forced-1",
                _ => unreachable!(),
            }
            .to_string(),
            code_bits: 1.0,
            proposal_cost_px: 0.0,
            error: GeometryError {
                symmetric_max_px: 0.0,
                symmetric_mean_px: 0.0,
                truth_to_model_max_px: 0.0,
                model_to_truth_max_px: 0.0,
            },
        })
        .collect();
    let rows = vec![GeometryBoundaryRow {
        fixture_id: "scene/boundary:0".to_string(),
        scene_id: "scene".to_string(),
        boundary_id: 0,
        samples: 4,
        gt_families: vec!["circular_arc", "quadratic_bezier", "cubic_bezier"],
        gt_breakpoints: vec![1, 2],
        stage_f_truth_match_px: 0.1,
        render_cell: "exact-raster".to_string(),
        injected_models: 2,
        oracle_selector_changed: true,
        injection_selector_changed: true,
        forced_selector_changed: true,
        arms,
    }];
    let derived = derive_coverage(&rows, &config);
    let aggregates = aggregate(&rows);
    GeometryMeasurements {
        schema: GEOMETRY_M6_SCHEMA,
        milestone: "M6",
        platform: Platform::current(),
        config,
        config_hash: "cfg".to_string(),
        fixture_set_hash: "fixture".to_string(),
        compatibility_key: key,
        source_groups: 1,
        scenes: 1,
        boundaries_attempted: 1,
        boundaries_measured: 1,
        exact_gt_reference_max_px: 0.0,
        oracle_candidate_injections: derived.candidate_injections,
        oracle_selector_changes: derived.oracle_selector_changes,
        injection_selector_changes: derived.injection_selector_changes,
        forced_selector_changes: derived.forced_selector_changes,
        raster_derived_rows: derived.raster_derived_rows,
        multi_span_rows: derived.multi_span_rows,
        multi_family_rows: derived.multi_family_rows,
        arc_rows: derived.arc_rows,
        quad_rows: derived.quad_rows,
        cubic_rows: derived.cubic_rows,
        forced_multi_candidate_rows: derived.forced_multi_candidate_rows,
        forced_smooth_rows: derived.forced_smooth_rows,
        relation_selected_rows: derived.relation_selected_rows,
        primitive_selected_rows: derived.primitive_selected_rows,
        exclusions: Vec::new(),
        aggregates,
        rows,
    }
}

fn one_floor_gate() -> GeometryGateConfig {
    GeometryGateConfig {
        min_boundaries: 1,
        min_arms_per_boundary: 5,
        min_candidate_injections: 1,
        min_selector_changes: 1,
        min_injection_selector_changes: 1,
        min_forced_selector_changes: 1,
        min_raster_derived_rows: 1,
        min_multi_span_rows: 1,
        min_multi_family_rows: 1,
        min_arc_rows: 1,
        min_quad_rows: 1,
        min_cubic_rows: 1,
        min_forced_multi_candidate_rows: 1,
        min_forced_smooth_rows: 1,
        min_relation_selected_rows: 1,
        min_primitive_selected_rows: 1,
    }
}

fn assert_clause_red(run: &GeometryMeasurements, clause: &str) {
    let gate = evaluate_gate(run, one_floor_gate());
    assert!(
        gate.rows.iter().any(|row| row.clause == clause && !row.met),
        "{clause} remained green: {gate:?}"
    );
}

fn row_arm_mut<'a>(row: &'a mut GeometryBoundaryRow, id: &str) -> &'a mut GeometryArmResult {
    row.arms
        .iter_mut()
        .find(|result| result.arm == id)
        .expect("declared arm")
}

#[test]
fn incompatible_arm_keys_make_the_gate_red() {
    let mut run = one_boundary_measurement();
    run.rows[0].arms[0]
        .compatibility_key
        .backend_id
        .push_str("-other");
    assert_clause_red(&run, "no_subtraction_across_incompatible_arms");
}

#[test]
fn every_geometry_gate_row_has_a_negative_knockout() {
    let baseline = one_boundary_measurement();
    assert!(evaluate_gate(&baseline, one_floor_gate()).met);

    let mut run = baseline.clone();
    run.oracle_candidate_injections = 0;
    assert_clause_red(&run, "published_aggregates_rederive_from_rows");

    let mut run = baseline.clone();
    run.aggregates[0].boundaries += 1;
    assert_clause_red(&run, "published_aggregates_rederive_from_rows");

    let mut run = baseline.clone();
    run.aggregates[0].mean_symmetric_max_px += 1.0;
    assert_clause_red(&run, "published_aggregates_rederive_from_rows");

    let mut run = baseline.clone();
    run.aggregates[0].worst_symmetric_max_px += 1.0;
    assert_clause_red(&run, "published_aggregates_rederive_from_rows");

    let mut run = baseline.clone();
    run.aggregates[0].selected_auto += 1;
    assert_clause_red(&run, "published_aggregates_rederive_from_rows");

    let mut run = baseline.clone();
    run.aggregates[0].selected_forced += 1;
    assert_clause_red(&run, "published_aggregates_rederive_from_rows");

    let mut run = baseline.clone();
    run.boundaries_measured = 0;
    assert_clause_red(&run, "common_geometry_population");

    let mut run = baseline.clone();
    run.rows[0].arms.pop();
    assert_clause_red(&run, "G00_G10_G01_G11_G20_all_measured");

    let mut run = baseline.clone();
    run.rows[0].injected_models = 0;
    assert_clause_red(&run, "oracle_candidate_injection_is_exercised");

    let mut run = baseline.clone();
    row_arm_mut(&mut run.rows[0], "G01").geometry_sha256 = "auto-0".to_string();
    assert_clause_red(&run, "oracle_selector_is_load_bearing");

    let mut run = baseline.clone();
    row_arm_mut(&mut run.rows[0], "G10").geometry_sha256 = "auto-0".to_string();
    assert_clause_red(&run, "G10_injection_changes_the_auto_selector");

    let mut run = baseline.clone();
    row_arm_mut(&mut run.rows[0], "G11").geometry_sha256 = "forced-0".to_string();
    assert_clause_red(&run, "G11_oracle_selector_changes_the_forced_choice");

    let mut run = baseline.clone();
    run.rows[0].render_cell.clear();
    assert_clause_red(&run, "fit_inputs_are_raster_derived_stage_f_rows");

    let mut run = baseline.clone();
    run.rows[0].gt_breakpoints.clear();
    assert_clause_red(&run, "multi_span_breakpoint_rows_are_measured");

    let mut run = baseline.clone();
    run.rows[0].gt_families = vec!["line", "line"];
    assert_clause_red(&run, "heterogeneous_family_rows_are_measured");
    assert_clause_red(&run, "circular_arc_GT_rows_are_measured");
    assert_clause_red(&run, "quadratic_GT_rows_are_measured");
    assert_clause_red(&run, "cubic_GT_rows_are_measured");

    let mut run = baseline.clone();
    row_arm_mut(&mut run.rows[0], "G20").candidate_models = 1;
    assert_clause_red(&run, "G20_has_multiple_join_candidates");

    let mut run = baseline.clone();
    let forced = row_arm_mut(&mut run.rows[0], "G20");
    forced.smooth.clear();
    forced.closure_smooth = false;
    assert_clause_red(&run, "G20_selects_smooth_joint_models");

    let mut run = baseline.clone();
    row_arm_mut(&mut run.rows[0], "G10").relations_selected = 0;
    assert_clause_red(&run, "Stage_H_relations_are_selected");

    let mut run = baseline;
    row_arm_mut(&mut run.rows[0], "G00").primitive_selected = false;
    assert_clause_red(&run, "Stage_H_primitives_are_selected");
}

#[test]
#[ignore = "walks every eligible development boundary and runs five geometry arms"]
fn the_full_m6_geometry_population_is_measured() {
    let run = measure_raw().expect("geometry run");
    println!(
        "groups {} scenes {} boundaries {}/{} exclusions {} injections {} selector changes {} key {}",
        run.source_groups,
        run.scenes,
        run.boundaries_measured,
        run.boundaries_attempted,
        run.exclusions.len(),
        run.oracle_candidate_injections,
        run.oracle_selector_changes,
        run.compatibility_key.fingerprint()
    );
    println!(
        "coverage raster {} multi-span {} multi-family {} arc {} quad {} cubic {} forced-multi {} forced-smooth {} relation-selected {} primitive-selected {}",
        run.raster_derived_rows,
        run.multi_span_rows,
        run.multi_family_rows,
        run.arc_rows,
        run.quad_rows,
        run.cubic_rows,
        run.forced_multi_candidate_rows,
        run.forced_smooth_rows,
        run.relation_selected_rows,
        run.primitive_selected_rows,
    );
    for arm in &run.aggregates {
        println!(
            "{} boundaries {} mean max {:.6} px worst {:.6} px auto {} forced {}",
            arm.arm,
            arm.boundaries,
            arm.mean_symmetric_max_px,
            arm.worst_symmetric_max_px,
            arm.selected_auto,
            arm.selected_forced
        );
    }
    for row in &run.rows {
        println!(
            "ROW {} samples {} match {:.6} families {:?} breaks {:?} injected {} G01 {} G10 {} G11 {} candidates {:?}",
            row.fixture_id,
            row.samples,
            row.stage_f_truth_match_px,
            row.gt_families,
            row.gt_breakpoints,
            row.injected_models,
            row.oracle_selector_changed,
            row.injection_selector_changed,
            row.forced_selector_changed,
            row.arms
                .iter()
                .map(|arm| {
                    (
                        arm.arm,
                        arm.candidate_models,
                        arm.error.symmetric_max_px,
                        &arm.families,
                        &arm.smooth,
                    )
                })
                .collect::<Vec<_>>()
        );
    }
    for exclusion in &run.exclusions {
        println!(
            "EXCLUSION {} {} {}",
            exclusion.fixture_id, exclusion.stage, exclusion.reason
        );
    }
    assert!(run.boundaries_measured > 0);
    assert!(
        run.rows.iter().all(|row| row.arms.len() == ARM_IDS.len()),
        "a common-population row is missing an arm"
    );
    assert!(run.oracle_candidate_injections > 0);
}
