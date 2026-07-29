use crate::gates::GatesFile;

use super::{
    GeometryArmResult, GeometryBoundaryRow, GeometryGateRow, GeometryGateTable,
    GeometryMeasurements, GeometryOracleConfig, ARM_IDS,
};

/// Frozen population floors consumed by the geometry-oracle row. Load-bearing
/// implementation constants are cross-checked separately by frozen claims.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeometryGateConfig {
    pub min_boundaries: usize,
    pub min_arms_per_boundary: usize,
    pub min_candidate_injections: usize,
    pub min_selector_changes: usize,
    pub min_injection_selector_changes: usize,
    pub min_forced_selector_changes: usize,
    pub min_raster_derived_rows: usize,
    pub min_multi_span_rows: usize,
    pub min_multi_family_rows: usize,
    pub min_arc_rows: usize,
    pub min_quad_rows: usize,
    pub min_cubic_rows: usize,
    pub min_forced_multi_candidate_rows: usize,
    pub min_forced_smooth_rows: usize,
    pub min_relation_selected_rows: usize,
    pub min_primitive_selected_rows: usize,
}

impl GeometryGateConfig {
    pub fn from_gates(gates: &GatesFile) -> Result<GeometryGateConfig, String> {
        let read = |key: &str| -> Result<usize, String> {
            let value = gates
                .gate_value("m6_geometry", key)
                .map_err(|e| e.to_string())?;
            value
                .as_integer()
                .and_then(|v| usize::try_from(v).ok())
                .ok_or_else(|| format!("[m6_geometry].{key} is not a non-negative integer"))
        };
        Ok(GeometryGateConfig {
            min_boundaries: read("gate_min_geometry_boundaries")?,
            min_arms_per_boundary: read("gate_min_geometry_arms_per_boundary")?,
            min_candidate_injections: read("gate_min_oracle_candidate_injections")?,
            min_selector_changes: read("gate_min_oracle_selector_changes")?,
            min_injection_selector_changes: read("gate_min_injection_selector_changes")?,
            min_forced_selector_changes: read("gate_min_forced_selector_changes")?,
            min_raster_derived_rows: read("gate_min_raster_derived_rows")?,
            min_multi_span_rows: read("gate_min_multi_span_rows")?,
            min_multi_family_rows: read("gate_min_multi_family_rows")?,
            min_arc_rows: read("gate_min_arc_rows")?,
            min_quad_rows: read("gate_min_quad_rows")?,
            min_cubic_rows: read("gate_min_cubic_rows")?,
            min_forced_multi_candidate_rows: read("gate_min_forced_multi_candidate_rows")?,
            min_forced_smooth_rows: read("gate_min_forced_smooth_rows")?,
            min_relation_selected_rows: read("gate_min_relation_selected_rows")?,
            min_primitive_selected_rows: read("gate_min_primitive_selected_rows")?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DerivedCoverage {
    pub(super) candidate_injections: usize,
    pub(super) oracle_selector_changes: usize,
    pub(super) injection_selector_changes: usize,
    pub(super) forced_selector_changes: usize,
    pub(super) raster_derived_rows: usize,
    pub(super) multi_span_rows: usize,
    pub(super) multi_family_rows: usize,
    pub(super) arc_rows: usize,
    pub(super) quad_rows: usize,
    pub(super) cubic_rows: usize,
    pub(super) forced_multi_candidate_rows: usize,
    pub(super) forced_smooth_rows: usize,
    pub(super) relation_selected_rows: usize,
    pub(super) primitive_selected_rows: usize,
}

fn row_arm<'a>(row: &'a GeometryBoundaryRow, id: &str) -> Option<&'a GeometryArmResult> {
    row.arms.iter().find(|result| result.arm == id)
}

pub(super) fn derive_coverage(
    rows: &[GeometryBoundaryRow],
    config: &GeometryOracleConfig,
) -> DerivedCoverage {
    let has_family = |row: &GeometryBoundaryRow, family: &str| {
        row.gt_families.iter().any(|candidate| *candidate == family)
    };
    DerivedCoverage {
        candidate_injections: rows.iter().map(|row| row.injected_models).sum(),
        oracle_selector_changes: rows
            .iter()
            .filter(|row| {
                matches!(
                    (row_arm(row, "G00"), row_arm(row, "G01")),
                    (Some(base), Some(oracle)) if base.geometry_sha256 != oracle.geometry_sha256
                )
            })
            .count(),
        injection_selector_changes: rows
            .iter()
            .filter(|row| {
                matches!(
                    (row_arm(row, "G00"), row_arm(row, "G10")),
                    (Some(base), Some(injected)) if base.geometry_sha256 != injected.geometry_sha256
                )
            })
            .count(),
        forced_selector_changes: rows
            .iter()
            .filter(|row| {
                matches!(
                    (row_arm(row, "G20"), row_arm(row, "G11")),
                    (Some(base), Some(oracle)) if base.geometry_sha256 != oracle.geometry_sha256
                )
            })
            .count(),
        raster_derived_rows: rows
            .iter()
            .filter(|row| {
                !row.render_cell.is_empty()
                    && row.stage_f_truth_match_px.is_finite()
                    && row.stage_f_truth_match_px <= config.max_stage_f_truth_match_px
            })
            .count(),
        multi_span_rows: rows
            .iter()
            .filter(|row| row.gt_families.len() > 1 && !row.gt_breakpoints.is_empty())
            .count(),
        multi_family_rows: rows
            .iter()
            .filter(|row| {
                row.gt_families
                    .iter()
                    .copied()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    > 1
            })
            .count(),
        arc_rows: rows
            .iter()
            .filter(|row| has_family(row, "circular_arc"))
            .count(),
        quad_rows: rows
            .iter()
            .filter(|row| has_family(row, "quadratic_bezier"))
            .count(),
        cubic_rows: rows
            .iter()
            .filter(|row| has_family(row, "cubic_bezier"))
            .count(),
        forced_multi_candidate_rows: rows
            .iter()
            .filter(|row| row_arm(row, "G20").is_some_and(|result| result.candidate_models > 1))
            .count(),
        forced_smooth_rows: rows
            .iter()
            .filter(|row| {
                row_arm(row, "G20").is_some_and(|result| {
                    result.closure_smooth || result.smooth.iter().any(|smooth| *smooth)
                })
            })
            .count(),
        relation_selected_rows: rows
            .iter()
            .filter(|row| row.arms.iter().any(|result| result.relations_selected > 0))
            .count(),
        primitive_selected_rows: rows
            .iter()
            .filter(|row| row.arms.iter().any(|result| result.primitive_selected))
            .count(),
    }
}

pub fn evaluate_gate(run: &GeometryMeasurements, gates: GeometryGateConfig) -> GeometryGateTable {
    let derived = derive_coverage(&run.rows, &run.config);
    let published = DerivedCoverage {
        candidate_injections: run.oracle_candidate_injections,
        oracle_selector_changes: run.oracle_selector_changes,
        injection_selector_changes: run.injection_selector_changes,
        forced_selector_changes: run.forced_selector_changes,
        raster_derived_rows: run.raster_derived_rows,
        multi_span_rows: run.multi_span_rows,
        multi_family_rows: run.multi_family_rows,
        arc_rows: run.arc_rows,
        quad_rows: run.quad_rows,
        cubic_rows: run.cubic_rows,
        forced_multi_candidate_rows: run.forced_multi_candidate_rows,
        forced_smooth_rows: run.forced_smooth_rows,
        relation_selected_rows: run.relation_selected_rows,
        primitive_selected_rows: run.primitive_selected_rows,
    };
    let aggregates_rederive = published == derived
        && run.boundaries_measured == run.rows.len()
        && run.rows.iter().all(|row| {
            let changed = |left: &str, right: &str| {
                matches!(
                    (row_arm(row, left), row_arm(row, right)),
                    (Some(a), Some(b)) if a.geometry_sha256 != b.geometry_sha256
                )
            };
            row.oracle_selector_changed == changed("G00", "G01")
                && row.injection_selector_changed == changed("G00", "G10")
                && row.forced_selector_changed == changed("G20", "G11")
        })
        && run.aggregates.iter().all(|aggregate| {
            aggregate.boundaries
                == run
                    .rows
                    .iter()
                    .filter(|row| row.arms.iter().any(|arm| arm.arm == aggregate.arm))
                    .count()
        });
    let arms_found = run.rows.iter().map(|row| row.arms.len()).min().unwrap_or(0);
    let arms_met = arms_found >= gates.min_arms_per_boundary
        && run.rows.iter().all(|row| {
            row.arms
                .iter()
                .map(|arm| arm.arm)
                .eq(ARM_IDS.iter().copied())
        });
    let fingerprint = run.compatibility_key.fingerprint();
    let compatible = run.rows.iter().all(|row| {
        row.arms
            .iter()
            .all(|arm| arm.compatibility_key == run.compatibility_key)
    });
    let mut rows = vec![
        GeometryGateRow {
            clause: "published_aggregates_rederive_from_rows",
            met: aggregates_rederive,
            measured: format!("{published:?}"),
            required: "exact row-derived counts and aggregate populations".to_string(),
        },
        GeometryGateRow {
            clause: "common_geometry_population",
            met: run.boundaries_measured >= gates.min_boundaries,
            measured: run.boundaries_measured.to_string(),
            required: format!(">= {}", gates.min_boundaries),
        },
        GeometryGateRow {
            clause: "G00_G10_G01_G11_G20_all_measured",
            met: arms_met,
            measured: format!("{arms_found} arms on every common boundary"),
            required: format!(
                ">= {} and exact declared arm set",
                gates.min_arms_per_boundary
            ),
        },
        GeometryGateRow {
            clause: "no_subtraction_across_incompatible_arms",
            met: compatible && run.boundaries_measured > 0,
            measured: format!(
                "{} arm measurements share key {}",
                run.boundaries_measured * ARM_IDS.len(),
                fingerprint
            ),
            required: "one identical five-component §27.6 key".to_string(),
        },
        GeometryGateRow {
            clause: "oracle_candidate_injection_is_exercised",
            met: derived.candidate_injections >= gates.min_candidate_injections,
            measured: derived.candidate_injections.to_string(),
            required: format!(">= {}", gates.min_candidate_injections),
        },
        GeometryGateRow {
            clause: "oracle_selector_is_load_bearing",
            met: derived.oracle_selector_changes >= gates.min_selector_changes,
            measured: derived.oracle_selector_changes.to_string(),
            required: format!(">= {}", gates.min_selector_changes),
        },
    ];
    let mut push_floor = |clause: &'static str, measured: usize, required: usize| {
        rows.push(GeometryGateRow {
            clause,
            met: measured >= required,
            measured: measured.to_string(),
            required: format!(">= {required}"),
        })
    };
    push_floor(
        "G10_injection_changes_the_auto_selector",
        derived.injection_selector_changes,
        gates.min_injection_selector_changes,
    );
    push_floor(
        "G11_oracle_selector_changes_the_forced_choice",
        derived.forced_selector_changes,
        gates.min_forced_selector_changes,
    );
    push_floor(
        "fit_inputs_are_raster_derived_stage_f_rows",
        derived.raster_derived_rows,
        gates.min_raster_derived_rows,
    );
    push_floor(
        "multi_span_breakpoint_rows_are_measured",
        derived.multi_span_rows,
        gates.min_multi_span_rows,
    );
    push_floor(
        "heterogeneous_family_rows_are_measured",
        derived.multi_family_rows,
        gates.min_multi_family_rows,
    );
    push_floor(
        "circular_arc_GT_rows_are_measured",
        derived.arc_rows,
        gates.min_arc_rows,
    );
    push_floor(
        "quadratic_GT_rows_are_measured",
        derived.quad_rows,
        gates.min_quad_rows,
    );
    push_floor(
        "cubic_GT_rows_are_measured",
        derived.cubic_rows,
        gates.min_cubic_rows,
    );
    push_floor(
        "G20_has_multiple_join_candidates",
        derived.forced_multi_candidate_rows,
        gates.min_forced_multi_candidate_rows,
    );
    push_floor(
        "G20_selects_smooth_joint_models",
        derived.forced_smooth_rows,
        gates.min_forced_smooth_rows,
    );
    push_floor(
        "Stage_H_relations_are_selected",
        derived.relation_selected_rows,
        gates.min_relation_selected_rows,
    );
    push_floor(
        "Stage_H_primitives_are_selected",
        derived.primitive_selected_rows,
        gates.min_primitive_selected_rows,
    );
    GeometryGateTable {
        met: rows.iter().all(|row| row.met),
        rows,
    }
}
