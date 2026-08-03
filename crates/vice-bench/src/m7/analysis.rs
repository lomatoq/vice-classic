//! M7 calibration analysis over raw held-out rows.
//!
//! This module never opens the sealed audit and never turns a placeholder
//! into a gate. It proposes the confidence/search-mass values that a later,
//! gate-only commit may freeze. Reliability is aggregated by source group,
//! while every render remains visible for coverage and tail diagnostics.

use std::collections::BTreeSet;

use serde::Serialize;

use super::{
    MeasurementReport, MeasurementRow, M7_CALIBRATION_POPULATION_POLICY, M7_MEASUREMENT_SCHEMA,
};
use crate::correlation::ResidualModel;
use crate::gt::raster::RasterProfile;
use crate::gt::split::{AuditSeal, SealStatus};
use crate::prereg::Preregistration;
use crate::reliability::{risk_coverage, RenderOutcome, RiskCoverage};

mod delivery;
pub use delivery::DeliveryCalibration;
use delivery::{calibrate_delivery_seal, delivery_diagnostics_permit};
mod policy;
use policy::{
    diagnostics_permit, paint_calibration_classes, select_observable_policy, ObservablePolicy,
};
#[cfg(test)]
use policy::fixed_diagnostics_permit;

pub const M7_CALIBRATION_ANALYSIS_SCHEMA: &str =
    "vice-classic/m7-confidence-calibration-analysis/v16";
pub const PROPOSED_BOUNDARY_P95_PX: f64 = super::M7_BOUNDARY_P95_GATE_PX;
pub const PROPOSED_BOUNDARY_P99_PX: f64 = super::M7_BOUNDARY_P99_GATE_PX;
pub const PROPOSED_BOUNDARY_MAX_PX: f64 = super::M7_BOUNDARY_MAX_GATE_PX;
pub const PROPOSED_MAX_PALETTE_CODE_DELTA: u8 = 4;
pub const PROPOSED_MAX_QUALITY_P95_MS: u64 = 10_000;
pub const PROPOSED_MAX_FAST_P95_MS: u64 = 1_000;
pub const PROPOSED_MIN_TOP2_CLASS_MARGIN_BITS: f64 = 0.0;
pub const PROPOSED_MAX_ABS_RESIDUAL_LAG1: f64 = 0.90;
pub const PROPOSED_MAX_TOPOLOGY_ENTROPY_BITS: f64 = 1.0;
pub const PROPOSED_MAX_FORMATION_ENTROPY_BITS: f64 = 1.0;
pub const PROPOSED_MIN_PERTURBATION_STABILITY: f64 = 0.95;
pub const TARGET_BUCKET: &str = "flat2-clean-aa-identifiable-128-512";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ThresholdEvaluation {
    pub posterior_lower_bound_threshold: f64,
    pub maximum_posterior_predictive_bits_per_block: f64,
    pub maximum_support_isotopy_displacement_px: f64,
    pub maximum_evidence_palette_shift_codes: u8,
    pub minimum_palette_support_px: u64,
    pub maximum_palette_interval_radius_codes: u8,
    pub reliability: RiskCoverage,
    pub minimum_source_coverage: f64,
    pub minimum_render_coverage: f64,
    pub source_coverage_met: bool,
    pub render_coverage_met: bool,
    pub accepted_render_boundary_p95_q95_px: Option<f64>,
    pub accepted_render_boundary_p99_q99_px: Option<f64>,
    pub accepted_boundary_max_px: Option<f64>,
    pub accepted_boundary_samples: u64,
    pub boundary_p95_met: bool,
    pub boundary_p99_met: bool,
    pub boundary_max_met: bool,
    pub zero_catastrophic_required_by_core: bool,
    pub eligible: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CalibrationAnalysis {
    pub schema: &'static str,
    pub measurement_schema: String,
    pub measurement_sha256: String,
    pub preregistration_sha256: String,
    pub identity: vice_opt::ModelIdentity,
    pub audit_generation: u32,
    pub audit_status: String,
    pub raw_rows: u64,
    pub target_rows: u64,
    pub empirical_unexplored_relative_mass_upper_bound: f64,
    pub runtime_preset: vice_core::Preset,
    pub runtime_scope_size_px: u32,
    pub runtime_p95_ms: u64,
    pub runtime_limit_ms: u64,
    pub runtime_isolated: bool,
    pub runtime_met: Option<bool>,
    pub runtime_release_blocking: bool,
    pub runtime_policy: &'static str,
    pub threshold_evaluations: Vec<ThresholdEvaluation>,
    pub selected_threshold: Option<f64>,
    pub selected_maximum_posterior_predictive_bits_per_block: Option<f64>,
    pub selected_maximum_support_isotopy_displacement_px: Option<f64>,
    pub selected_maximum_evidence_palette_shift_codes: Option<u8>,
    pub selected_minimum_palette_support_px: Option<u64>,
    pub selected_maximum_palette_interval_radius_codes: Option<u8>,
    pub delivery_calibration: DeliveryCalibration,
    pub delivery_seal: vice_verify::DeliverySealConfig,
    pub calibration: Option<vice_core::ConfidenceCalibration>,
    pub production_config: Option<ProductionConfigProposal>,
    pub gate_met: bool,
    pub refusals: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProductionConfigProposal {
    pub schema: &'static str,
    pub preset: vice_core::Preset,
    pub delivery_seal: vice_verify::DeliverySealConfig,
    pub calibration: vice_core::ConfidenceCalibration,
    pub identity: vice_opt::ModelIdentity,
}

pub fn analyze_calibration(
    report: &MeasurementReport,
    audit: &AuditSeal,
) -> Result<CalibrationAnalysis, String> {
    if report.schema != M7_MEASUREMENT_SCHEMA
        || report.scope != "calibration"
        || report.split != "calibration"
        || report.procedural_generation != audit.generation
        || report.population_policy != M7_CALIBRATION_POPULATION_POLICY
        || !report.complete
        || report.included_shards.len() != report.shard_count as usize
        || report.renders != report.expected_renders_included_shards
    {
        return Err(
            "M7 calibration analysis requires one complete merged calibration report bound to \
             the current procedural/audit generation"
                .into(),
        );
    }
    let prereg = Preregistration::v1();
    prereg
        .check()
        .map_err(|errors| format!("invalid preregistration: {}", errors.join("; ")))?;
    let bucket = prereg
        .buckets
        .iter()
        .find(|bucket| bucket.id == TARGET_BUCKET)
        .ok_or_else(|| format!("preregistration has no {TARGET_BUCKET} bucket"))?;
    let target_rows = report
        .rows
        .iter()
        .filter(|row| {
            row.size_px >= bucket.min_size_px
                && row.size_px <= bucket.max_size_px
                && bucket
                    .identifiability
                    .contains(&row.identifiability.as_str())
        })
        .collect::<Vec<_>>();
    if target_rows.is_empty() {
        return Err(format!("M7 report has no rows in {TARGET_BUCKET}"));
    }
    let empirical_upper = target_rows.iter().try_fold(0u64, |upper, row| {
        if row.search_truncated == Some(true) && row.unexplored_proxy_hypotheses.is_none() {
            Err(format!(
                "{} / {} is truncated without an unexplored-mass proxy",
                row.group_id, row.cell_id
            ))
        } else {
            Ok(upper.max(row.unexplored_proxy_hypotheses.unwrap_or(0)))
        }
    })? as f64;
    let delivery_calibration = calibrate_delivery_seal(&target_rows)?;
    let delivery_seal = delivery_calibration.proposal;
    let observable_policy = select_observable_policy(
        &target_rows,
        empirical_upper,
        delivery_seal,
        bucket.min_coverage_per_source,
        bucket.min_coverage_per_render,
        prereg.confidence,
        prereg.risk_target,
    )?;

    let mut scores = target_rows
        .iter()
        .filter_map(|row| effective_lower_bound(row, empirical_upper))
        .filter(|score| score.is_finite() && *score >= 0.0)
        .collect::<Vec<_>>();
    scores.sort_by(f64::total_cmp);
    scores.dedup_by(|left, right| left.total_cmp(right).is_eq());

    let mut threshold_evaluations = Vec::with_capacity(scores.len());
    for threshold in scores {
        let Some(policy) = observable_policy else {
            break;
        };
        let paint_classes = paint_calibration_classes(target_rows.iter().copied().filter(|row| {
            row.candidate_available
                && diagnostics_permit(row, empirical_upper, delivery_seal, policy)
                && effective_lower_bound(row, empirical_upper)
                    .is_some_and(|score| score >= threshold)
        }));
        let supported_paint_classes = paint_classes
            .iter()
            .map(|class| class.name.as_str())
            .collect::<BTreeSet<_>>();
        let outcomes = target_rows
            .iter()
            .map(|row| {
                let accepted = row.candidate_available
                    && diagnostics_permit(row, empirical_upper, delivery_seal, policy)
                    && row
                        .paint_calibration_class
                        .as_deref()
                        .is_some_and(|class| supported_paint_classes.contains(class))
                    && effective_lower_bound(row, empirical_upper)
                        .is_some_and(|score| score >= threshold);
                Ok(RenderOutcome {
                    group_id: row.group_id.clone(),
                    cell_id: row.cell_id.clone(),
                    profile: RasterProfile::from_id(&row.rasterizer).ok_or_else(|| {
                        format!("unknown rasterizer profile {:?}", row.rasterizer)
                    })?,
                    accepted,
                    catastrophic: accepted
                        && !catastrophic_kinds(row, delivery_seal, PROPOSED_MAX_PALETTE_CODE_DELTA)
                            .is_empty(),
                    mandatory: true,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let reliability = risk_coverage(
            TARGET_BUCKET,
            &outcomes,
            prereg.confidence,
            prereg.risk_target,
            Some((ResidualModel::Block, true)),
        );
        let accepted_rows = target_rows
            .iter()
            .copied()
            .filter(|row| {
                row.candidate_available
                    && diagnostics_permit(row, empirical_upper, delivery_seal, policy)
                    && row
                        .paint_calibration_class
                        .as_deref()
                        .is_some_and(|class| supported_paint_classes.contains(class))
                    && effective_lower_bound(row, empirical_upper)
                        .is_some_and(|score| score >= threshold)
            })
            .collect::<Vec<_>>();
        // A boundary p95/p99 is pooled over physical boundary samples, not a
        // demand that every render's own tail summary stay below the gate.
        // Per-render q95/q99 remain visible as conservative diagnostics.
        let population_tail = super::boundary_population_tail(
            &accepted_rows,
            PROPOSED_BOUNDARY_P95_PX,
            PROPOSED_BOUNDARY_P99_PX,
            PROPOSED_BOUNDARY_MAX_PX,
        );
        let accepted_render_boundary_p95_q95_px =
            population_tail.as_ref().map(|tail| tail.render_p95_q95_px);
        let accepted_render_boundary_p99_q99_px =
            population_tail.as_ref().map(|tail| tail.render_p99_q99_px);
        let accepted_boundary_max_px = population_tail.as_ref().map(|tail| tail.max_px);
        let accepted_boundary_samples = population_tail
            .as_ref()
            .map_or(0, |tail| tail.pooled_samples);
        let source_coverage_met = reliability.coverage_per_source >= bucket.min_coverage_per_source;
        let render_coverage_met = reliability.coverage_per_render >= bucket.min_coverage_per_render;
        let boundary_p95_met = population_tail
            .as_ref()
            .is_some_and(|tail| tail.p95_gate_met);
        let boundary_p99_met = population_tail
            .as_ref()
            .is_some_and(|tail| tail.p99_gate_met);
        let boundary_max_met = population_tail
            .as_ref()
            .is_some_and(|tail| tail.max_gate_met);
        let zero_catastrophic_required_by_core = reliability.groups_catastrophic == 0;
        let eligible = reliability.contract_met
            && source_coverage_met
            && render_coverage_met
            && boundary_p95_met
            && boundary_p99_met
            && boundary_max_met
            && zero_catastrophic_required_by_core;
        threshold_evaluations.push(ThresholdEvaluation {
            posterior_lower_bound_threshold: threshold,
            maximum_posterior_predictive_bits_per_block: policy.maximum_predictive_bits_per_block,
            maximum_support_isotopy_displacement_px: policy.maximum_support_displacement_px,
            maximum_evidence_palette_shift_codes: policy.maximum_evidence_palette_shift_codes,
            minimum_palette_support_px: policy.minimum_palette_support_px,
            maximum_palette_interval_radius_codes: policy.maximum_palette_interval_radius_codes,
            reliability,
            minimum_source_coverage: bucket.min_coverage_per_source,
            minimum_render_coverage: bucket.min_coverage_per_render,
            source_coverage_met,
            render_coverage_met,
            accepted_render_boundary_p95_q95_px,
            accepted_render_boundary_p99_q99_px,
            accepted_boundary_max_px,
            accepted_boundary_samples,
            boundary_p95_met,
            boundary_p99_met,
            boundary_max_met,
            zero_catastrophic_required_by_core,
            eligible,
        });
    }
    let selected = threshold_evaluations
        .iter()
        .find(|evaluation| evaluation.eligible);
    let selected_threshold = selected.map(|evaluation| evaluation.posterior_lower_bound_threshold);
    let selected_maximum_posterior_predictive_bits_per_block =
        selected.map(|evaluation| evaluation.maximum_posterior_predictive_bits_per_block);
    let selected_maximum_support_isotopy_displacement_px =
        selected.map(|evaluation| evaluation.maximum_support_isotopy_displacement_px);
    let selected_maximum_evidence_palette_shift_codes =
        selected.map(|evaluation| evaluation.maximum_evidence_palette_shift_codes);
    let selected_minimum_palette_support_px =
        selected.map(|evaluation| evaluation.minimum_palette_support_px);
    let selected_maximum_palette_interval_radius_codes =
        selected.map(|evaluation| evaluation.maximum_palette_interval_radius_codes);
    let supported_selection_classes = selected
        .map(|evaluation| {
            target_rows
                .iter()
                .filter(|row| {
                    row.candidate_available
                        && diagnostics_permit(
                            row,
                            empirical_upper,
                            delivery_seal,
                            ObservablePolicy {
                                maximum_predictive_bits_per_block: evaluation
                                    .maximum_posterior_predictive_bits_per_block,
                                maximum_support_displacement_px: evaluation
                                    .maximum_support_isotopy_displacement_px,
                                maximum_evidence_palette_shift_codes: evaluation
                                    .maximum_evidence_palette_shift_codes,
                                minimum_palette_support_px: evaluation.minimum_palette_support_px,
                                maximum_palette_interval_radius_codes: evaluation
                                    .maximum_palette_interval_radius_codes,
                            },
                        )
                        && effective_lower_bound(row, empirical_upper).is_some_and(|score| {
                            score >= evaluation.posterior_lower_bound_threshold
                        })
                })
                .filter_map(|row| row.selected_hypothesis_id.as_deref())
                .map(vice_core::selection_calibration_class)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let paint_calibration_classes = selected
        .map(|evaluation| {
            let policy = ObservablePolicy {
                maximum_predictive_bits_per_block: evaluation
                    .maximum_posterior_predictive_bits_per_block,
                maximum_support_displacement_px: evaluation.maximum_support_isotopy_displacement_px,
                maximum_evidence_palette_shift_codes: evaluation
                    .maximum_evidence_palette_shift_codes,
                minimum_palette_support_px: evaluation.minimum_palette_support_px,
                maximum_palette_interval_radius_codes: evaluation
                    .maximum_palette_interval_radius_codes,
            };
            paint_calibration_classes(target_rows.iter().copied().filter(|row| {
                row.candidate_available
                    && diagnostics_permit(row, empirical_upper, delivery_seal, policy)
                    && effective_lower_bound(row, empirical_upper)
                        .is_some_and(|score| score >= evaluation.posterior_lower_bound_threshold)
            }))
        })
        .unwrap_or_default();
    const RUNTIME_SCOPE_SIZE_PX: u32 = 512;
    let runtime_p95_ms = runtime_quantile(
        &target_rows
            .iter()
            .filter(|row| row.size_px == RUNTIME_SCOPE_SIZE_PX)
            .map(|row| row.core_runtime_ms)
            .collect::<Vec<_>>(),
        0.95,
    );
    let runtime_limit_ms = match report.preset {
        vice_core::Preset::Fast => PROPOSED_MAX_FAST_P95_MS,
        vice_core::Preset::Quality => PROPOSED_MAX_QUALITY_P95_MS,
    };
    let runtime_isolated = report.max_workers_per_shard == 1 && report.shard_count == 1;
    let runtime_met = runtime_isolated.then_some(runtime_p95_ms <= runtime_limit_ms);
    let audit_untouched = audit.status == SealStatus::Sealed;
    let measurement_sha256 = calibration_measurement_digest(report);
    let calibration = selected.map(|evaluation| vice_core::ConfidenceCalibration {
        schema: "vice-classic/confidence-calibration/v4".into(),
        model_universe_sha256: report.identity.universe_sha256.clone(),
        pricing_sha256: report.identity.pricing_sha256.clone(),
        backend_sha256: report.identity.backend_sha256.clone(),
        config_sha256: report.identity.config_sha256.clone(),
        calibration_split_sha256: measurement_sha256.clone(),
        sealed_audit_generation: format!("generation-{}-sealed", audit.generation),
        sealed_audit_untouched: audit_untouched,
        confidence_level: prereg.confidence,
        catastrophic_risk_target: prereg.risk_target,
        accepted_source_groups: evaluation.reliability.groups_accepted,
        catastrophic_source_groups: evaluation.reliability.groups_catastrophic,
        posterior_lower_bound_threshold: evaluation.posterior_lower_bound_threshold,
        minimum_top2_class_margin_bits: PROPOSED_MIN_TOP2_CLASS_MARGIN_BITS,
        maximum_posterior_predictive_bits_per_block: evaluation
            .maximum_posterior_predictive_bits_per_block,
        maximum_support_isotopy_displacement_px: evaluation.maximum_support_isotopy_displacement_px,
        maximum_evidence_palette_shift_codes: evaluation.maximum_evidence_palette_shift_codes,
        minimum_palette_support_px: evaluation.minimum_palette_support_px,
        maximum_palette_interval_radius_codes: evaluation.maximum_palette_interval_radius_codes,
        maximum_abs_residual_lag1: PROPOSED_MAX_ABS_RESIDUAL_LAG1,
        maximum_topology_entropy_bits: PROPOSED_MAX_TOPOLOGY_ENTROPY_BITS,
        maximum_formation_entropy_bits: PROPOSED_MAX_FORMATION_ENTROPY_BITS,
        minimum_perturbation_stability: PROPOSED_MIN_PERTURBATION_STABILITY,
        empirical_unexplored_relative_mass_upper_bound: Some(empirical_upper),
        supported_selection_classes: supported_selection_classes.clone(),
        paint_calibration_classes: paint_calibration_classes.clone(),
        buckets: vec![vice_core::CalibrationBucket {
            name: TARGET_BUCKET.into(),
            accepted_source_groups: evaluation.reliability.groups_accepted,
            eligible_source_groups: evaluation.reliability.groups_total,
            minimum_coverage: bucket.min_coverage_per_source,
        }],
    });
    let mut refusals = Vec::new();
    if observable_policy.is_none() {
        refusals.push(
            "no observable predictive/support policy meets source/render coverage and \
             zero-catastrophic gates before posterior thresholding"
                .into(),
        );
    } else if selected.is_none() {
        refusals.push(
            "no posterior threshold simultaneously meets clustered risk, source coverage, \
             boundary p95/p99/max tails, and the core zero-catastrophic contract"
                .into(),
        );
    }
    // §29 calls these provisional research targets and explicitly makes
    // correctness the early priority. They remain visible diagnostics, while
    // M7-37's finite elapsed/memory/evaluation caps remain hard release gates.
    // Confidence calibration itself is worker-count invariant and must not be
    // withheld merely because the complete corpus used bounded parallelism.
    const RUNTIME_POLICY: &str = "provisional M7 research diagnostic on an isolated 512px run; \
                                  non-blocking for confidence/release, with bounded elapsed, \
                                  memory, hypothesis, and render budgets enforced separately";
    if !audit_untouched {
        refusals.push("sealed audit is not sealed and untouched".into());
    }
    if let Some(calibration) = &calibration {
        if let Err(reason) = calibration.validate_for_identity(&report.identity) {
            refusals.push(format!("core rejected proposed calibration: {reason}"));
        }
    }
    let gate_met = refusals.is_empty() && calibration.is_some();
    let production_config = (gate_met && calibration.is_some()).then(|| ProductionConfigProposal {
        schema: "vice-classic/m7-production-config/v1",
        preset: report.preset,
        delivery_seal,
        calibration: calibration.clone().expect("checked above"),
        identity: report.identity.clone(),
    });
    Ok(CalibrationAnalysis {
        schema: M7_CALIBRATION_ANALYSIS_SCHEMA,
        measurement_schema: report.schema.clone(),
        measurement_sha256,
        preregistration_sha256: prereg.hash(),
        identity: report.identity.clone(),
        audit_generation: audit.generation,
        audit_status: format!("{:?}", audit.status).to_lowercase(),
        raw_rows: report.rows.len().try_into().unwrap_or(u64::MAX),
        target_rows: target_rows.len().try_into().unwrap_or(u64::MAX),
        empirical_unexplored_relative_mass_upper_bound: empirical_upper,
        runtime_preset: report.preset,
        runtime_scope_size_px: RUNTIME_SCOPE_SIZE_PX,
        runtime_p95_ms,
        runtime_limit_ms,
        runtime_isolated,
        runtime_met,
        runtime_release_blocking: super::release::M7_RUNTIME_RELEASE_BLOCKING,
        runtime_policy: RUNTIME_POLICY,
        threshold_evaluations,
        selected_threshold,
        selected_maximum_posterior_predictive_bits_per_block,
        selected_maximum_support_isotopy_displacement_px,
        selected_maximum_evidence_palette_shift_codes,
        selected_minimum_palette_support_px,
        selected_maximum_palette_interval_radius_codes,
        delivery_calibration,
        delivery_seal,
        calibration,
        production_config,
        gate_met,
        refusals,
    })
}

pub(crate) fn intrinsic_catastrophic_kinds(row: &MeasurementRow) -> Vec<&'static str> {
    if !row.candidate_available {
        return Vec::new();
    }
    let mut kinds = Vec::new();
    if !row.topology.as_ref().is_some_and(|topology| topology.exact) {
        kinds.push("wrong_visible_topology");
    }
    if !row.verifier_clean {
        kinds.push("verification_failure");
    }
    kinds
}

fn catastrophic_kinds(
    row: &MeasurementRow,
    delivery_seal: vice_verify::DeliverySealConfig,
    max_palette_code_delta: u8,
) -> Vec<&'static str> {
    let mut kinds = intrinsic_catastrophic_kinds(row);
    if !row.candidate_available {
        return kinds;
    }
    if !delivery_diagnostics_permit(row, delivery_seal) {
        kinds.push("serialized_mismatch");
    }
    if row
        .max_palette_code_delta
        .is_none_or(|delta| delta > max_palette_code_delta)
    {
        kinds.push("wrong_paint_or_missing_face");
    }
    kinds
}

fn effective_lower_bound(row: &MeasurementRow, empirical_upper: f64) -> Option<f64> {
    if row.search_truncated == Some(false) {
        return row.posterior_lower_bound;
    }
    let selected = row.selected_delivery_mass?;
    let explored = row.explored_mass?;
    let denominator = explored + empirical_upper;
    (selected.is_finite() && selected >= 0.0 && denominator.is_finite() && denominator > 0.0)
        .then_some(selected / denominator)
}

fn calibrated_entropy_upper_bound(
    row: &MeasurementRow,
    empirical_unexplored_relative_mass_upper_bound: f64,
    topology: bool,
) -> Option<f64> {
    let explored_mass = row.explored_mass?;
    let denominator = explored_mass + empirical_unexplored_relative_mass_upper_bound;
    let (top_mass, class_count) = if topology {
        (
            row.top_topology_explored_mass?,
            row.topology_classes_upper_bound?,
        )
    } else {
        (
            row.top_formation_explored_mass?,
            row.formation_classes_upper_bound?,
        )
    };
    (denominator.is_finite() && denominator > 0.0 && top_mass >= 0.0)
        .then(|| (top_mass / denominator).clamp(0.0, 1.0))
        .and_then(|top_probability| {
            vice_opt::finite_class_entropy_upper_bound(top_probability, class_count)
        })
}

fn runtime_quantile(values: &[u64], quantile: f64) -> u64 {
    if values.is_empty() {
        return u64::MAX;
    }
    let mut values = values.to_vec();
    values.sort_unstable();
    let index = ((values.len() - 1) as f64 * quantile).ceil() as usize;
    values[index.min(values.len() - 1)]
}

fn calibration_measurement_digest(report: &MeasurementReport) -> String {
    super::determinism::normalized_digest(report)
}

#[cfg(test)]
mod tests;
