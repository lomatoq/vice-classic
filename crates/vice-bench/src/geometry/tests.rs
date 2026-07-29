use super::*;

fn one_boundary_measurement() -> GeometryMeasurements {
    let config = GeometryOracleConfig::default();
    let key = CompatibilityKey {
        backend_id: BACKEND_ID.to_string(),
        config_hash: "cfg".to_string(),
        candidate_budget: CandidateBudget::Candidates {
            max: config.candidate_budget as u64,
        },
        fixture_hash: "fixture".to_string(),
        intervention_schema_version: INTERVENTION_SCHEMA.to_string(),
    };
    let fingerprint = key.fingerprint();
    let arms = ARM_IDS
        .iter()
        .map(|arm| GeometryArmResult {
            arm,
            compatibility_key_fingerprint: fingerprint.clone(),
            candidate_models: 1,
            selected_source: if *arm == "G00" || *arm == "G01" {
                "automatic"
            } else {
                "forced_gt"
            },
            families: vec!["line"],
            breakpoints: Vec::new(),
            smooth: Vec::new(),
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
        oracle_candidate_injections: 1,
        oracle_selector_changes: 1,
        exclusions: Vec::new(),
        aggregates: Vec::new(),
        rows: vec![GeometryBoundaryRow {
            fixture_id: "scene/boundary:0".to_string(),
            scene_id: "scene".to_string(),
            boundary_id: 0,
            samples: 4,
            gt_families: vec!["line"],
            gt_breakpoints: Vec::new(),
            injected_models: 1,
            oracle_selector_changed: true,
            arms,
        }],
    }
}

#[test]
fn incompatible_arm_fingerprints_make_the_gate_red() {
    let mut run = one_boundary_measurement();
    run.rows[0].arms[0]
        .compatibility_key_fingerprint
        .push_str("-other");
    let gate = evaluate_gate(
        &run,
        GeometryGateConfig {
            min_boundaries: 1,
            min_arms_per_boundary: 5,
            min_candidate_injections: 1,
            min_selector_changes: 1,
        },
    );
    assert!(!gate.met);
    assert!(gate
        .rows
        .iter()
        .any(|row| row.clause == "no_subtraction_across_incompatible_arms" && !row.met));
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
    assert!(run.boundaries_measured > 0);
    assert!(
        run.rows.iter().all(|row| row.arms.len() == ARM_IDS.len()),
        "a common-population row is missing an arm"
    );
    assert!(run.oracle_candidate_injections > 0);
}
