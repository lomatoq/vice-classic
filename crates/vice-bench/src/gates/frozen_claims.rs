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
