//! The frozen gate values AS THE CODE HAS THEM (§27.7).
//!
//! Split out of `gates/mod.rs` in M6 when that file crossed §4.1's 800-line
//! cap. Each entry pulls its value from the constant or measurement bound that
//! actually governs behaviour, so a relaxed literal shows up as a mismatch
//! rather than as agreement between two copies of the same number.

use super::tests::GateExpectation;

/// The frozen values AS THE CODE HAS THEM. Each entry pulls the value
/// from the constant or the measurement bound that actually governs
/// behaviour, so a relaxed literal shows up here as a mismatch.
pub(super) fn frozen_values_from_code() -> Vec<(&'static str, String, GateExpectation)> {
    use crate::correlation::ResidualModel;
    use crate::dcel::report as dcelr;
    use crate::gt::degradation as deg;
    use crate::gt::split::SPLIT_POLICY_V1;
    use crate::prereg::Preregistration;
    use crate::topology::report as topo;

    let prereg = Preregistration::v1();
    let m7_bucket = prereg
        .buckets
        .iter()
        .find(|bucket| bucket.id == crate::m7::analysis::TARGET_BUCKET)
        .expect("M7 target bucket is preregistered");
    let admissible: Vec<&str> = ResidualModel::ALL
        .iter()
        .filter(|m| m.admissible_for_confidence())
        .map(|m| m.id())
        .collect();
    let diagnostic: Vec<&str> = ResidualModel::ALL
        .iter()
        .filter(|m| !m.admissible_for_confidence())
        .map(|m| m.id())
        .collect();

    let v: Vec<(&'static str, &'static str, GateExpectation)> = vec![
        // --- reliability: the statistical court -------------------
        (
            "reliability",
            "confidence",
            GateExpectation::num(prereg.confidence),
        ),
        (
            "reliability",
            "catastrophic_risk_target",
            GateExpectation::num(prereg.risk_target),
        ),
        (
            "reliability",
            "min_accepted_source_groups_zero_failures",
            GateExpectation::num(crate::reliability::required_groups_for_zero_failures(
                prereg.confidence,
                prereg.risk_target,
            ) as f64),
        ),
        (
            "reliability",
            "unit_of_trial",
            GateExpectation::text(crate::reliability::UNIT_OF_TRIAL),
        ),
        // --- corpus instruments: the C066 measurements ------------
        (
            "corpus_instruments",
            "supersample_max_abs",
            GateExpectation::num(crate::gt::raster::SUPERSAMPLE_MAX_ABS_GATE),
        ),
        (
            "corpus_instruments",
            "supersample_edge_mean_abs",
            GateExpectation::num(crate::gt::raster::SUPERSAMPLE_EDGE_MEAN_ABS_GATE),
        ),
        (
            "corpus_instruments",
            "vice_render_max_abs",
            GateExpectation::num(crate::gt::raster::VICE_RENDER_MAX_ABS_GATE),
        ),
        (
            "corpus_instruments",
            "tiny_skia_max_abs",
            GateExpectation::num(crate::gt::raster::EXTERNAL_ENGINE_MAX_ABS_GATE),
        ),
        (
            "corpus_instruments",
            "raqote_max_abs",
            GateExpectation::num(crate::gt::raster::EXTERNAL_ENGINE_MAX_ABS_GATE),
        ),
        // --- identifiability: the C067 calibration ----------------
        (
            "identifiability",
            "observability_floor_px",
            GateExpectation::num(deg::OBSERVABILITY_FLOOR_PX),
        ),
        (
            "identifiability",
            "rival_indistinguishable_codes",
            GateExpectation::num(f64::from(deg::RIVAL_INDISTINGUISHABLE_CODES)),
        ),
        (
            "identifiability",
            "quantization_floor_codes",
            GateExpectation::num(deg::QUANTIZATION_FLOOR_CODES),
        ),
        // --- topology envelope and its gate thresholds (M4.5) -----
        //
        // Every constant that decides which candidates exist, and every
        // threshold that decides whether a §28 M4.5 clause is green.
        // M45-N6 / RT45-A5: without these §27.7 had nothing to act on for
        // this milestone, and one commit could relax a clause and change
        // the code that meets it.
        (
            "topology",
            "field_tv_iterations",
            GateExpectation::num(f64::from(vice_topology::FIELD_CONFIG_V1.tv_iterations)),
        ),
        (
            "topology",
            "field_tv_step",
            GateExpectation::num(vice_topology::FIELD_CONFIG_V1.tv_step),
        ),
        (
            "topology",
            "field_tv_huber_delta",
            GateExpectation::num(vice_topology::FIELD_CONFIG_V1.tv_huber_delta),
        ),
        (
            "topology",
            "field_tv_data_weight",
            GateExpectation::num(vice_topology::FIELD_CONFIG_V1.tv_data_weight),
        ),
        (
            "topology",
            "field_deconv_iterations",
            GateExpectation::num(f64::from(vice_topology::FIELD_CONFIG_V1.deconv_iterations)),
        ),
        (
            "topology",
            "field_deconv_step",
            GateExpectation::num(vice_topology::FIELD_CONFIG_V1.deconv_step),
        ),
        (
            "topology",
            "level_max_plateau_levels",
            GateExpectation::num(vice_topology::LEVEL_CONFIG_V1.max_plateau_levels as f64),
        ),
        (
            "topology",
            "level_max_event_levels",
            GateExpectation::num(vice_topology::LEVEL_CONFIG_V1.max_event_levels as f64),
        ),
        (
            "topology",
            "level_min_event_persistence",
            GateExpectation::num(vice_topology::LEVEL_CONFIG_V1.min_event_persistence),
        ),
        (
            "topology",
            "level_fixed_smoke_count",
            GateExpectation::num(vice_topology::LEVEL_CONFIG_V1.fixed_smoke_levels.len() as f64),
        ),
        (
            "topology",
            "level_fixed_smoke_first",
            GateExpectation::num(vice_topology::LEVEL_CONFIG_V1.fixed_smoke_levels[0]),
        ),
        (
            "topology",
            "envelope_budget",
            GateExpectation::num(vice_topology::ENVELOPE_CONFIG_V1.budget as f64),
        ),
        (
            "topology",
            "envelope_per_quota_class",
            GateExpectation::num(vice_topology::ENVELOPE_CONFIG_V1.per_quota_class as f64),
        ),
        (
            "topology",
            "envelope_mass_scale",
            GateExpectation::num(vice_topology::ENVELOPE_CONFIG_V1.mass_scale),
        ),
        (
            "topology",
            "continuation_halo_px",
            GateExpectation::num(f64::from(vice_topology::CONTINUATION_CONFIG_V1.halo_px)),
        ),
        (
            "topology",
            "continuation_max_plans",
            GateExpectation::num(vice_topology::CONTINUATION_CONFIG_V1.max_plans as f64),
        ),
        (
            "dcel",
            "gate_min_arms",
            GateExpectation::num(f64::from(dcelr::MIN_ARMS)),
        ),
        (
            "dcel",
            "gate_min_structural_arms",
            GateExpectation::num(f64::from(dcelr::MIN_STRUCTURAL_ARMS)),
        ),
        (
            "dcel",
            "gate_min_convention_dependent_groups",
            GateExpectation::num(f64::from(dcelr::MIN_CONVENTION_DEPENDENT_GROUPS)),
        ),
        (
            "dcel",
            "gate_min_transactions",
            GateExpectation::num(f64::from(dcelr::MIN_TRANSACTIONS)),
        ),
        (
            "dcel",
            "gate_min_unrelated_chain_population",
            GateExpectation::num(f64::from(dcelr::MIN_UNRELATED_CHAIN_POPULATION)),
        ),
        (
            "dcel",
            "gate_min_resolving_power_probes",
            GateExpectation::num(f64::from(dcelr::MIN_RESOLVING_POWER_PROBES)),
        ),
        (
            "dcel",
            "gate_min_slots_perturbed",
            GateExpectation::num(f64::from(dcelr::MIN_SLOTS_PERTURBED)),
        ),
        (
            "dcel",
            "gate_min_register_arms_with_a_long_loop",
            GateExpectation::num(f64::from(dcelr::MIN_REGISTER_ARMS_WITH_A_LONG_LOOP)),
        ),
        // --- compound transactions (§28 M5, delivered M6) ----------
        (
            "dcel_compound",
            "gate_min_compound_transactions",
            GateExpectation::num(f64::from(dcelr::MIN_COMPOUND_TRANSACTIONS)),
        ),
        (
            "dcel_compound",
            "gate_min_distinct_compound_deltas",
            GateExpectation::num(f64::from(dcelr::MIN_DISTINCT_COMPOUND_DELTAS)),
        ),
        (
            "dcel_compound",
            "gate_min_transaction_shapes",
            GateExpectation::num(f64::from(dcelr::MIN_TRANSACTION_SHAPES)),
        ),
        (
            "topology",
            "gate_min_recall_arms",
            GateExpectation::num(f64::from(topo::MIN_RECALL_ARMS)),
        ),
        (
            "topology",
            "gate_min_recall_shape_families",
            GateExpectation::num(f64::from(topo::MIN_RECALL_SHAPE_FAMILIES)),
        ),
        (
            "topology",
            "gate_min_non_trivial_gt_arms",
            GateExpectation::num(f64::from(topo::MIN_NON_TRIVIAL_GT_ARMS)),
        ),
        (
            "topology",
            "gate_min_topology_pairs",
            GateExpectation::num(f64::from(topo::MIN_TOPOLOGY_PAIRS)),
        ),
        (
            "topology",
            "gate_min_classes_per_retaining_pair",
            GateExpectation::num(f64::from(topo::MIN_CLASSES_PER_RETAINING_PAIR)),
        ),
        // --- topology_controls -------------------------------------
        // Not thresholds of a row but numbers that decide whether a row's
        // CONTROL measures anything, which RT45-A12 showed is the same kind
        // of number: `0.3 -> 0.0001` on the knockout radius empties the
        // control and leaves clause 1 green.
        (
            "topology_controls",
            "gate_knockout_disk_radius_fraction",
            GateExpectation::num(crate::topology::KNOCKOUT_DISK_RADIUS_FRACTION),
        ),
        (
            "topology_controls",
            "gate_gt_majority_level",
            GateExpectation::num(crate::topology::GT_MAJORITY_LEVEL),
        ),
        // --- split -------------------------------------------------
        (
            "split",
            "policy_version",
            GateExpectation::text(SPLIT_POLICY_V1.version),
        ),
        (
            "split",
            "development_pct",
            GateExpectation::num(f64::from(SPLIT_POLICY_V1.development_pct)),
        ),
        (
            "split",
            "calibration_pct",
            GateExpectation::num(f64::from(SPLIT_POLICY_V1.calibration_pct)),
        ),
        (
            "split",
            "sealed_audit_pct",
            GateExpectation::num(f64::from(SPLIT_POLICY_V1.sealed_audit_pct)),
        ),
        (
            "split",
            "unit_of_assignment",
            GateExpectation::text(crate::gt::split::UNIT_OF_ASSIGNMENT),
        ),
        (
            "split",
            "held_out_profiles",
            GateExpectation::list(SPLIT_POLICY_V1.held_out_profiles),
        ),
        // --- noise scales: the M4 measurement ----------------------
        // Measured by
        // `corridor::tests::the_clean_bucket_noise_scale_is_measured_on_the_development_split`
        // and consumed by the corridor's sigma budget, so the frozen
        // number has a reader and a producer rather than being a
        // decoration (F-0019).
        (
            "noise_scales",
            "clean_bucket_sigma_codes",
            GateExpectation::num(vice_evidence::corridor::CLEAN_BUCKET_SIGMA_CODES),
        ),
        // --- geometry code table (§14.5; set by M6) -----------------
        // The three values `vice_fit::code` actually computes with. Claimed
        // here BEFORE the section is frozen, which the walk permits and §27.7
        // requires: the code side lands in one commit and the freeze is the
        // next, because a commit may not touch the gate file and production
        // code together.
        (
            "geometry_code_table",
            "bits_per_anchor",
            GateExpectation::num(vice_fit::GEOMETRY_CODE_TABLE_V1.bits_per_anchor()),
        ),
        (
            "geometry_code_table",
            "bits_per_segment_family",
            GateExpectation::num(vice_fit::GEOMETRY_CODE_TABLE_V1.bits_per_segment_family()),
        ),
        (
            "geometry_code_table",
            "bits_per_relation",
            GateExpectation::num(vice_fit::GEOMETRY_CODE_TABLE_V1.bits_per_relation()),
        ),
        // --- geometry pricing surface (RT6-A3; frozen at delta-1) ---
        // The gate above froze the code table's three numbers and the red
        // team repriced the grammar around them in one line (flag_bits of the
        // arc, 2.0 -> 0.0): decisions flipped and published corpus numbers
        // moved, including the G1 clause's population, with every test green
        // and the gate file untouched. The WHOLE pricing surface is therefore
        // frozen by content hash: `pricing_surface_v1` enumerates it by
        // calling the real functions, and this claim binds the frozen hash to
        // that enumeration. One repriced line now reddens this walk.
        (
            "geometry_pricing",
            "pricing_surface_sha256",
            GateExpectation::text(&crate::hashing::sha256_hex(
                vice_fit::pricing_surface_v1().as_bytes(),
            )),
        ),
        // --- M6 geometry gate and oracle decomposition -------------
        (
            "m6_geometry",
            "gate_max_g1_spread_rad",
            GateExpectation::num(vice_fit::GATE_MAX_G1_SPREAD_RAD),
        ),
        (
            "m6_geometry",
            "gate_min_g1_nodes",
            GateExpectation::num(vice_fit::GATE_MIN_G1_NODES as f64),
        ),
        (
            "m6_geometry",
            "gate_min_g1_positive_control_rad",
            GateExpectation::num(vice_fit::GATE_MIN_G1_POSITIVE_CONTROL_RAD),
        ),
        (
            "m6_geometry",
            "gate_max_breakpoint_fraction_delta",
            GateExpectation::num(vice_fit::GATE_MAX_BREAKPOINT_FRACTION_DELTA),
        ),
        (
            "m6_geometry",
            "gate_max_cut_rotation_delta_bits",
            GateExpectation::num(vice_fit::GATE_MAX_CUT_ROTATION_DELTA_BITS),
        ),
        (
            "m6_geometry",
            "gate_max_translation_delta_bits",
            GateExpectation::num(vice_fit::GATE_MAX_TRANSLATION_DELTA_BITS),
        ),
        (
            "m6_geometry",
            "gate_min_invariance_legs",
            GateExpectation::num(vice_fit::GATE_MIN_INVARIANCE_LEGS as f64),
        ),
        (
            "m6_geometry",
            "gate_min_no_bic_extra_segments",
            GateExpectation::num(vice_fit::GATE_MIN_NO_BIC_EXTRA_SEGMENTS as f64),
        ),
        (
            "m6_geometry",
            "candidate_budget_per_chain",
            GateExpectation::num(vice_fit::FIT_BUDGET_V1.cap() as f64),
        ),
        (
            "m6_geometry",
            "k_discrete_paths",
            GateExpectation::num(vice_fit::K_DISCRETE_PATHS as f64),
        ),
        (
            "m6_geometry",
            "max_canonical_cuts",
            GateExpectation::num(vice_fit::MAX_CANONICAL_CUTS as f64),
        ),
        (
            "m6_geometry",
            "gate_min_geometry_boundaries",
            GateExpectation::num(crate::geometry::GATE_MIN_GEOMETRY_BOUNDARIES as f64),
        ),
        (
            "m6_geometry",
            "gate_min_geometry_arms_per_boundary",
            GateExpectation::num(crate::geometry::GATE_MIN_GEOMETRY_ARMS_PER_BOUNDARY as f64),
        ),
        (
            "m6_geometry",
            "gate_min_oracle_candidate_injections",
            GateExpectation::num(crate::geometry::GATE_MIN_ORACLE_CANDIDATE_INJECTIONS as f64),
        ),
        (
            "m6_geometry",
            "gate_min_oracle_selector_changes",
            GateExpectation::num(crate::geometry::GATE_MIN_ORACLE_SELECTOR_CHANGES as f64),
        ),
        (
            "m6_geometry",
            "gate_min_injection_selector_changes",
            GateExpectation::num(crate::geometry::GATE_MIN_INJECTION_SELECTOR_CHANGES as f64),
        ),
        (
            "m6_geometry",
            "gate_min_forced_selector_changes",
            GateExpectation::num(crate::geometry::GATE_MIN_FORCED_SELECTOR_CHANGES as f64),
        ),
        (
            "m6_geometry",
            "gate_min_raster_derived_rows",
            GateExpectation::num(crate::geometry::GATE_MIN_RASTER_DERIVED_ROWS as f64),
        ),
        (
            "m6_geometry",
            "gate_min_multi_span_rows",
            GateExpectation::num(crate::geometry::GATE_MIN_MULTI_SPAN_ROWS as f64),
        ),
        (
            "m6_geometry",
            "gate_min_multi_family_rows",
            GateExpectation::num(crate::geometry::GATE_MIN_MULTI_FAMILY_ROWS as f64),
        ),
        (
            "m6_geometry",
            "gate_min_arc_rows",
            GateExpectation::num(crate::geometry::GATE_MIN_ARC_ROWS as f64),
        ),
        (
            "m6_geometry",
            "gate_min_quad_rows",
            GateExpectation::num(crate::geometry::GATE_MIN_QUAD_ROWS as f64),
        ),
        (
            "m6_geometry",
            "gate_min_cubic_rows",
            GateExpectation::num(crate::geometry::GATE_MIN_CUBIC_ROWS as f64),
        ),
        (
            "m6_geometry",
            "gate_min_forced_multi_candidate_rows",
            GateExpectation::num(crate::geometry::GATE_MIN_FORCED_MULTI_CANDIDATE_ROWS as f64),
        ),
        (
            "m6_geometry",
            "gate_min_forced_smooth_rows",
            GateExpectation::num(crate::geometry::GATE_MIN_FORCED_SMOOTH_ROWS as f64),
        ),
        (
            "m6_geometry",
            "gate_min_relation_selected_rows",
            GateExpectation::num(crate::geometry::GATE_MIN_RELATION_SELECTED_ROWS as f64),
        ),
        (
            "m6_geometry",
            "gate_min_primitive_selected_rows",
            GateExpectation::num(crate::geometry::GATE_MIN_PRIMITIVE_SELECTED_ROWS as f64),
        ),
        // --- M7 selective delivery --------------------------------
        // The preset-dependent values are the complete generation-4
        // calibration outputs frozen for M7. Keeping the preset dimension
        // here prevents the F-0138 scalar-gate collapse.
        (
            "boundary_accuracy",
            "p95_px",
            GateExpectation::num(crate::m7::M7_BOUNDARY_P95_GATE_PX),
        ),
        (
            "boundary_accuracy",
            "p99_px",
            GateExpectation::num(crate::m7::M7_BOUNDARY_P99_GATE_PX),
        ),
        (
            "boundary_accuracy",
            "max_px",
            GateExpectation::num(crate::m7::M7_BOUNDARY_MAX_GATE_PX),
        ),
        (
            "m7_selective",
            "quality_posterior_lower_bound_threshold",
            GateExpectation::num(0.000_526_476_012_730_330_5),
        ),
        (
            "m7_selective",
            "quality_empirical_unexplored_relative_mass_upper_bound",
            GateExpectation::num(1896.0),
        ),
        (
            "m7_selective",
            "quality_gate_max_posterior_predictive_bits_per_block",
            GateExpectation::num(0.249_761_695_832_955_54),
        ),
        (
            "m7_selective",
            "quality_gate_max_support_isotopy_displacement_px",
            GateExpectation::num(0.776_890_594_215_616_9),
        ),
        (
            "m7_selective",
            "fast_posterior_lower_bound_threshold",
            GateExpectation::num(0.001_581_236_510_458_300_8),
        ),
        (
            "m7_selective",
            "fast_empirical_unexplored_relative_mass_upper_bound",
            GateExpectation::num(629.0),
        ),
        (
            "m7_selective",
            "fast_gate_max_posterior_predictive_bits_per_block",
            GateExpectation::num(0.242_510_996_004_153_24),
        ),
        (
            "m7_selective",
            "fast_gate_max_support_isotopy_displacement_px",
            GateExpectation::num(0.757_582_927_178_493),
        ),
        (
            "m7_selective",
            "gate_min_top2_class_margin_bits",
            GateExpectation::num(crate::m7::analysis::PROPOSED_MIN_TOP2_CLASS_MARGIN_BITS),
        ),
        (
            "m7_selective",
            "gate_max_abs_residual_lag1",
            GateExpectation::num(crate::m7::analysis::PROPOSED_MAX_ABS_RESIDUAL_LAG1),
        ),
        (
            "m7_selective",
            "gate_max_topology_entropy_bits",
            GateExpectation::num(crate::m7::analysis::PROPOSED_MAX_TOPOLOGY_ENTROPY_BITS),
        ),
        (
            "m7_selective",
            "gate_max_formation_entropy_bits",
            GateExpectation::num(crate::m7::analysis::PROPOSED_MAX_FORMATION_ENTROPY_BITS),
        ),
        (
            "m7_selective",
            "gate_min_perturbation_stability",
            GateExpectation::num(crate::m7::analysis::PROPOSED_MIN_PERTURBATION_STABILITY),
        ),
        (
            "m7_selective",
            "gate_min_source_coverage_128_512",
            GateExpectation::num(m7_bucket.min_coverage_per_source),
        ),
        (
            "m7_selective",
            "gate_min_render_coverage_128_512",
            GateExpectation::num(m7_bucket.min_coverage_per_render),
        ),
        (
            "m7_selective",
            "gate_max_palette_code_delta",
            GateExpectation::num(f64::from(
                crate::m7::analysis::PROPOSED_MAX_PALETTE_CODE_DELTA,
            )),
        ),
        (
            "m7_selective",
            "gate_max_quality_p95_ms",
            GateExpectation::num(crate::m7::analysis::PROPOSED_MAX_QUALITY_P95_MS as f64),
        ),
        (
            "m7_selective",
            "gate_max_fast_p95_ms",
            GateExpectation::num(crate::m7::analysis::PROPOSED_MAX_FAST_P95_MS as f64),
        ),
        (
            "m7_selective",
            "gate_max_peak_memory_bytes",
            GateExpectation::num(
                vice_core::CoreConfig::development_for(vice_core::Preset::Quality)
                    .beam
                    .budget
                    .max_memory_bytes as f64,
            ),
        ),
        (
            "m7_selective",
            "gate_max_profile_channel_delta",
            GateExpectation::num(4.0),
        ),
        (
            "m7_selective",
            "gate_max_profile_mean_channel_delta",
            GateExpectation::num(0.0075),
        ),
        (
            "m7_selective",
            "gate_max_internal_channel_delta",
            GateExpectation::num(128.0),
        ),
        (
            "m7_selective",
            "gate_max_internal_mean_channel_delta",
            GateExpectation::num(1.5),
        ),
        (
            "m7_selective",
            "gate_max_complexity_growth_ratio",
            GateExpectation::num(2.0),
        ),
        (
            "m7_selective",
            "gate_min_blind_source_trials",
            GateExpectation::num(20.0),
        ),
        (
            "m7_selective",
            "gate_max_blind_one_sided_p_value",
            GateExpectation::num(0.05),
        ),
        (
            "m7_selective",
            "gate_min_blind_preference_rate",
            GateExpectation::num(0.50),
        ),
        (
            "m7_selective",
            "gate_min_pf_complete_rows",
            GateExpectation::num(1.0),
        ),
        (
            "m7_selective",
            "gate_min_complete_geometry_oracle_rows",
            GateExpectation::num(1.0),
        ),
        (
            "m7_selective",
            "gate_min_g20_recovery_rows",
            GateExpectation::num(1.0),
        ),
        (
            "m7_selective",
            "gate_min_g30_recovery_rows",
            GateExpectation::num(1.0),
        ),
        (
            "m7_selective",
            "gate_min_geometry_recovery_rate",
            GateExpectation::num(0.80),
        ),
        // --- likelihood --------------------------------------------
        (
            "likelihood",
            "allowed_production_residual_models",
            GateExpectation::list(&admissible),
        ),
        (
            "likelihood",
            "diagnostic_only_residual_models",
            GateExpectation::list(&diagnostic),
        ),
    ];
    v.into_iter()
        .map(|(s, k, e)| (s, k.to_string(), e))
        .collect()
}
