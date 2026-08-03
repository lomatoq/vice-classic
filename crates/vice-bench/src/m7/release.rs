//! Frozen-gate analysis of the untouched M7 sealed audit.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use super::{
    analysis::intrinsic_catastrophic_kinds, MeasurementReport, MeasurementRow,
    M7_SEALED_POPULATION_POLICY,
};
use crate::correlation::ResidualModel;
use crate::gates::GatesFile;
use crate::gt::raster::RasterProfile;
use crate::gt::split::{AuditSeal, SealStatus};
use crate::m7::governance::M7ThresholdSource;
use crate::prereg::Preregistration;
use crate::reliability::{risk_coverage, RenderOutcome, RiskCoverage};

pub const M7_RELEASE_VERDICT_SCHEMA: &str = "vice-classic/m7-release-verdict/v10";
pub const M7_RUNTIME_RELEASE_BLOCKING: bool = false;
pub const M7_RUNTIME_POLICY: &str = "provisional M7 research diagnostic on an isolated 512px run; \
                                     non-blocking for release, with bounded elapsed, memory, \
                                     hypothesis, and render budgets enforced separately";
const TARGET_BUCKET: &str = "flat2-clean-aa-identifiable-128-512";

fn runtime_blocks_release(runtime_gate_met: bool) -> bool {
    M7_RUNTIME_RELEASE_BLOCKING && !runtime_gate_met
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PresetCalibrationGates {
    pub posterior_lower_bound_threshold: f64,
    pub empirical_unexplored_relative_mass_upper_bound: f64,
    pub max_posterior_predictive_bits_per_block: f64,
    pub max_support_isotopy_displacement_px: f64,
    pub max_evidence_palette_shift_codes: u8,
    pub min_palette_support_px: u64,
    pub max_palette_interval_radius_codes: u8,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct M7ReleaseGates {
    pub boundary_p95_px: f64,
    pub boundary_p99_px: f64,
    pub boundary_max_px: f64,
    pub min_source_coverage: f64,
    pub min_render_coverage: f64,
    pub max_palette_code_delta: u8,
    pub max_quality_p95_ms: u64,
    pub max_fast_p95_ms: u64,
    pub max_peak_memory_bytes: u64,
    pub max_profile_channel_delta: u8,
    pub max_profile_mean_channel_delta: f64,
    pub max_internal_channel_delta: u8,
    pub max_internal_mean_channel_delta: f64,
    pub quality_calibration: PresetCalibrationGates,
    pub fast_calibration: PresetCalibrationGates,
    pub min_top2_class_margin_bits: f64,
    pub max_abs_residual_lag1: f64,
    pub max_topology_entropy_bits: f64,
    pub max_formation_entropy_bits: f64,
    pub min_perturbation_stability: f64,
}

impl M7ReleaseGates {
    pub fn from_file(gates: &GatesFile) -> Result<Self, String> {
        Ok(Self {
            boundary_p95_px: f64_gate(gates, "boundary_accuracy", "p95_px")?,
            boundary_p99_px: f64_gate(gates, "boundary_accuracy", "p99_px")?,
            boundary_max_px: f64_gate(gates, "boundary_accuracy", "max_px")?,
            min_source_coverage: f64_gate(
                gates,
                "m7_selective",
                "gate_min_source_coverage_128_512",
            )?,
            min_render_coverage: f64_gate(
                gates,
                "m7_selective",
                "gate_min_render_coverage_128_512",
            )?,
            max_palette_code_delta: u64_gate(gates, "m7_selective", "gate_max_palette_code_delta")?
                .try_into()
                .map_err(|_| "palette gate does not fit u8")?,
            max_quality_p95_ms: u64_gate(gates, "m7_selective", "gate_max_quality_p95_ms")?,
            max_fast_p95_ms: u64_gate(gates, "m7_selective", "gate_max_fast_p95_ms")?,
            max_peak_memory_bytes: u64_gate(gates, "m7_selective", "gate_max_peak_memory_bytes")?,
            max_profile_channel_delta: u64_gate(
                gates,
                "m7_selective",
                "gate_max_profile_channel_delta",
            )?
            .try_into()
            .map_err(|_| "profile delta gate does not fit u8")?,
            max_profile_mean_channel_delta: f64_gate(
                gates,
                "m7_selective",
                "gate_max_profile_mean_channel_delta",
            )?,
            max_internal_channel_delta: u64_gate(
                gates,
                "m7_selective",
                "gate_max_internal_channel_delta",
            )?
            .try_into()
            .map_err(|_| "internal delta gate does not fit u8")?,
            max_internal_mean_channel_delta: f64_gate(
                gates,
                "m7_selective",
                "gate_max_internal_mean_channel_delta",
            )?,
            quality_calibration: preset_calibration_gates(gates, "quality")?,
            fast_calibration: preset_calibration_gates(gates, "fast")?,
            min_top2_class_margin_bits: f64_gate(
                gates,
                "m7_selective",
                "gate_min_top2_class_margin_bits",
            )?,
            max_abs_residual_lag1: f64_gate(gates, "m7_selective", "gate_max_abs_residual_lag1")?,
            max_topology_entropy_bits: f64_gate(
                gates,
                "m7_selective",
                "gate_max_topology_entropy_bits",
            )?,
            max_formation_entropy_bits: f64_gate(
                gates,
                "m7_selective",
                "gate_max_formation_entropy_bits",
            )?,
            min_perturbation_stability: f64_gate(
                gates,
                "m7_selective",
                "gate_min_perturbation_stability",
            )?,
        })
    }
}

fn preset_calibration_gates(
    gates: &GatesFile,
    preset: &str,
) -> Result<PresetCalibrationGates, String> {
    Ok(PresetCalibrationGates {
        posterior_lower_bound_threshold: f64_gate(
            gates,
            "m7_selective",
            &format!("{preset}_posterior_lower_bound_threshold"),
        )?,
        empirical_unexplored_relative_mass_upper_bound: f64_gate(
            gates,
            "m7_selective",
            &format!("{preset}_empirical_unexplored_relative_mass_upper_bound"),
        )?,
        max_posterior_predictive_bits_per_block: f64_gate(
            gates,
            "m7_selective",
            &format!("{preset}_gate_max_posterior_predictive_bits_per_block"),
        )?,
        max_support_isotopy_displacement_px: f64_gate(
            gates,
            "m7_selective",
            &format!("{preset}_gate_max_support_isotopy_displacement_px"),
        )?,
        max_evidence_palette_shift_codes: u64_gate(
            gates,
            "m7_selective",
            &format!("{preset}_gate_max_evidence_palette_shift_codes"),
        )?
        .try_into()
        .map_err(|_| format!("{preset} palette-shift gate does not fit u8"))?,
        min_palette_support_px: u64_gate(
            gates,
            "m7_selective",
            &format!("{preset}_gate_min_palette_support_px"),
        )?,
        max_palette_interval_radius_codes: u64_gate(
            gates,
            "m7_selective",
            &format!("{preset}_gate_max_palette_interval_radius_codes"),
        )?
        .try_into()
        .map_err(|_| format!("{preset} palette-interval gate does not fit u8"))?,
    })
}

fn f64_gate(gates: &GatesFile, section: &str, key: &str) -> Result<f64, String> {
    gates
        .gate_value(section, key)
        .map_err(|error| error.to_string())?
        .as_float()
        .or_else(|| {
            gates
                .gate_value(section, key)
                .ok()
                .and_then(toml::Value::as_integer)
                .map(|value| value as f64)
        })
        .filter(|value| value.is_finite())
        .ok_or_else(|| format!("{section}.{key} is not a finite number"))
}

fn u64_gate(gates: &GatesFile, section: &str, key: &str) -> Result<u64, String> {
    gates
        .gate_value(section, key)
        .map_err(|error| error.to_string())?
        .as_integer()
        .and_then(|value| value.try_into().ok())
        .ok_or_else(|| format!("{section}.{key} is not a non-negative integer"))
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StratumSummary {
    pub renders: u64,
    pub accepted: u64,
    pub catastrophic: u64,
    pub refusal_statuses: BTreeMap<String, u64>,
    pub refusal_reasons: BTreeMap<String, u64>,
    pub cost_refusal_histogram: BTreeMap<String, u64>,
    pub numerical_conditioning: ConditioningSummary,
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct ConditioningSummary {
    pub evidence_pairs: u64,
    pub evidence_pairs_refused: u64,
    pub evidence_conditioning_min: Option<f64>,
    pub evidence_conditioning_max: Option<f64>,
    pub fit_runs: u64,
    pub fit_material_cost_samples: u64,
    pub fit_worst_normal_to_euclidean_ratio: Option<f64>,
    pub fit_worst_ratio_at_deviation_px: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PresetReleaseVerdict {
    pub preset: vice_core::Preset,
    pub identity: vice_opt::ModelIdentity,
    pub delivery_policy_sha256: String,
    pub reliability: RiskCoverage,
    pub source_coverage_met: bool,
    pub render_coverage_met: bool,
    pub accepted_render_boundary_p95_q95_px: Option<f64>,
    pub accepted_render_boundary_p99_q99_px: Option<f64>,
    pub accepted_boundary_max_px: Option<f64>,
    pub accepted_boundary_samples: u64,
    pub boundary_gates_met: bool,
    pub max_palette_code_delta: Option<u8>,
    pub palette_gate_met: bool,
    pub runtime_p95_ms: u64,
    pub runtime_limit_ms: u64,
    pub runtime_isolated: bool,
    pub runtime_gate_met: bool,
    pub runtime_release_blocking: bool,
    pub runtime_policy: &'static str,
    pub peak_working_set_bytes: u64,
    pub memory_gate_met: bool,
    pub calibration_gate_met: bool,
    pub catastrophic_kinds: BTreeMap<String, u64>,
    pub by_family: BTreeMap<String, StratumSummary>,
    pub by_size: BTreeMap<String, StratumSummary>,
    pub by_renderer: BTreeMap<String, StratumSummary>,
    pub by_source_group: BTreeMap<String, StratumSummary>,
    pub gate_met: bool,
    pub refusals: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReleaseVerdict {
    pub schema: &'static str,
    pub audit_generation: u32,
    pub audit_status: String,
    pub corpus_sha256: String,
    pub population_commitment_sha256: String,
    pub preregistration_sha256: String,
    pub gates_sha256: String,
    pub release_commit_sha: String,
    pub runner_attestation_sha256: String,
    pub gate_provenance_sha256: String,
    pub quality_calibration_measurement_sha256: String,
    pub fast_calibration_measurement_sha256: String,
    pub geometry_measurement_sha256: String,
    pub quality_production_config_sha256: String,
    pub fast_production_config_sha256: String,
    pub quality_report_sha256: String,
    pub fast_report_sha256: String,
    pub quality: PresetReleaseVerdict,
    pub fast: PresetReleaseVerdict,
    pub same_population: bool,
    pub gate_met: bool,
    pub refusals: Vec<String>,
}

pub fn analyze_release(
    quality: &MeasurementReport,
    fast: &MeasurementReport,
    audit: &AuditSeal,
    threshold_source: &M7ThresholdSource,
) -> Result<ReleaseVerdict, String> {
    let gates_file = &threshold_source.gates;
    validate_report(quality, vice_core::Preset::Quality)?;
    validate_report(fast, vice_core::Preset::Fast)?;
    if audit.status != SealStatus::Opened {
        return Err("M7 release analysis requires an opened audit generation".into());
    }
    if quality.procedural_generation != audit.generation
        || fast.procedural_generation != audit.generation
        || quality.population_policy != M7_SEALED_POPULATION_POLICY
        || fast.population_policy != M7_SEALED_POPULATION_POLICY
    {
        return Err(
            "M7 release reports are not bound to this fresh procedural audit generation".into(),
        );
    }
    let prereg = Preregistration::v1();
    prereg
        .check()
        .map_err(|errors| format!("invalid preregistration: {}", errors.join("; ")))?;
    if audit.prereg_hash != prereg.hash() || audit.gates_hash != gates_file.sha256 {
        return Err("audit record is not bound to this preregistration and gate file".into());
    }
    let gates = M7ReleaseGates::from_file(gates_file)?;
    let quality_verdict = analyze_preset(
        quality,
        &prereg,
        gates,
        &threshold_source
            .provenance
            .quality_calibration_measurement_sha256,
    )?;
    let fast_verdict = analyze_preset(
        fast,
        &prereg,
        gates,
        &threshold_source
            .provenance
            .fast_calibration_measurement_sha256,
    )?;
    let quality_keys = population_keys(quality);
    let fast_keys = population_keys(fast);
    let same_population = quality_keys == fast_keys;
    let mut refusals = Vec::new();
    if !same_population {
        refusals
            .push("Fast and Quality sealed reports do not cover the same mandatory rows".into());
    }
    if !quality_verdict.gate_met {
        refusals.push("Quality sealed-audit verdict is not green".into());
    }
    if !fast_verdict.gate_met {
        refusals.push("Fast sealed-audit verdict is not green".into());
    }
    Ok(ReleaseVerdict {
        schema: M7_RELEASE_VERDICT_SCHEMA,
        audit_generation: audit.generation,
        audit_status: "opened".into(),
        corpus_sha256: audit.corpus_hash.clone(),
        population_commitment_sha256: quality.population_commitment_sha256.clone(),
        preregistration_sha256: audit.prereg_hash.clone(),
        gates_sha256: audit.gates_hash.clone(),
        release_commit_sha: threshold_source.event_commit_sha.clone(),
        runner_attestation_sha256: threshold_source.attestation_sha256.clone(),
        gate_provenance_sha256: threshold_source.provenance_sha256.clone(),
        quality_calibration_measurement_sha256: threshold_source
            .provenance
            .quality_calibration_measurement_sha256
            .clone(),
        fast_calibration_measurement_sha256: threshold_source
            .provenance
            .fast_calibration_measurement_sha256
            .clone(),
        geometry_measurement_sha256: threshold_source
            .provenance
            .geometry_measurement_sha256
            .clone(),
        quality_production_config_sha256: threshold_source
            .provenance
            .quality_production_config_sha256
            .clone(),
        fast_production_config_sha256: threshold_source
            .provenance
            .fast_production_config_sha256
            .clone(),
        quality_report_sha256: super::report_content_sha256(quality),
        fast_report_sha256: super::report_content_sha256(fast),
        quality: quality_verdict,
        fast: fast_verdict,
        same_population,
        gate_met: refusals.is_empty(),
        refusals,
    })
}

fn validate_report(report: &MeasurementReport, preset: vice_core::Preset) -> Result<(), String> {
    super::validate_sealed_population(report)?;
    super::validate_execution_attestation(report)?;
    if report.preset != preset {
        return Err(format!(
            "M7 release analysis requires one complete merged {preset:?} sealed-audit report"
        ));
    }
    Ok(())
}

pub(crate) fn target_rows(report: &MeasurementReport) -> Vec<&MeasurementRow> {
    report
        .rows
        .iter()
        .filter(|row| (128..=512).contains(&row.size_px) && row.identifiability == "identifiable")
        .collect()
}

fn analyze_preset(
    report: &MeasurementReport,
    prereg: &Preregistration,
    gates: M7ReleaseGates,
    expected_calibration_sha256: &str,
) -> Result<PresetReleaseVerdict, String> {
    let rows = target_rows(report);
    if rows.is_empty() {
        return Err(format!(
            "{:?} report has no rows in {TARGET_BUCKET}",
            report.preset
        ));
    }
    let outcomes = rows
        .iter()
        .map(|row| {
            let accepted = row.production_accepted && row.production_provenance;
            Ok(RenderOutcome {
                group_id: row.group_id.clone(),
                cell_id: row.cell_id.clone(),
                profile: RasterProfile::from_id(&row.rasterizer)
                    .ok_or_else(|| format!("unknown rasterizer {:?}", row.rasterizer))?,
                accepted,
                catastrophic: accepted && !catastrophic_with_gates(row, gates).is_empty(),
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
    let accepted = rows
        .iter()
        .copied()
        .filter(|row| row.production_accepted && row.production_provenance)
        .collect::<Vec<_>>();
    let population_tail = super::boundary_population_tail(
        &accepted,
        gates.boundary_p95_px,
        gates.boundary_p99_px,
        gates.boundary_max_px,
    );
    let accepted_render_boundary_p95_q95_px =
        population_tail.as_ref().map(|tail| tail.render_p95_q95_px);
    let accepted_render_boundary_p99_q99_px =
        population_tail.as_ref().map(|tail| tail.render_p99_q99_px);
    let accepted_boundary_max_px = population_tail.as_ref().map(|tail| tail.max_px);
    let accepted_boundary_samples = population_tail
        .as_ref()
        .map_or(0, |tail| tail.pooled_samples);
    let boundary_gates_met = population_tail
        .as_ref()
        .is_some_and(|tail| tail.p95_gate_met && tail.p99_gate_met && tail.max_gate_met);
    let max_palette_code_delta = accepted
        .iter()
        .filter_map(|row| row.max_palette_code_delta)
        .max();
    let palette_gate_met =
        max_palette_code_delta.is_some_and(|value| value <= gates.max_palette_code_delta);
    let runtime_p95_ms = quantile(
        &rows
            .iter()
            .map(|row| row.core_runtime_ms)
            .collect::<Vec<_>>(),
        0.95,
    );
    let runtime_limit_ms = match report.preset {
        vice_core::Preset::Fast => gates.max_fast_p95_ms,
        vice_core::Preset::Quality => gates.max_quality_p95_ms,
    };
    let runtime_isolated = report.max_workers_per_shard == 1 && report.shard_count == 1;
    let runtime_gate_met = runtime_isolated && runtime_p95_ms <= runtime_limit_ms;
    let memory_gate_met = report.peak_working_set_bytes <= gates.max_peak_memory_bytes;
    let calibration_gate_met =
        calibration_matches_gates(report, gates, expected_calibration_sha256);
    let source_coverage_met = reliability.coverage_per_source >= gates.min_source_coverage;
    let render_coverage_met = reliability.coverage_per_render >= gates.min_render_coverage;
    let mut kinds = BTreeMap::new();
    for row in &accepted {
        for kind in catastrophic_with_gates(row, gates) {
            *kinds.entry(kind.to_string()).or_default() += 1;
        }
    }
    let mut refusals = Vec::new();
    if !reliability.contract_met {
        refusals.push("clustered 99% catastrophic-risk contract is not met".into());
    }
    if !source_coverage_met || !render_coverage_met {
        refusals.push("source/render selective coverage is below its frozen floor".into());
    }
    if !kinds.is_empty() {
        refusals.push("one or more accepted rows are catastrophic".into());
    }
    if !boundary_gates_met {
        refusals.push("accepted boundary tail exceeds a frozen gate".into());
    }
    if !palette_gate_met {
        refusals.push("accepted palette error exceeds the frozen gate".into());
    }
    // §29's wall-clock numbers are provisional research diagnostics. The
    // hard M7 resource contract is the bounded-growth ledger plus process
    // memory; an honest miss remains in this verdict without converting a
    // correctness-qualified release into a refusal.
    if runtime_blocks_release(runtime_gate_met) {
        refusals.push("provisional wall-clock research target is not met".into());
    }
    if !memory_gate_met {
        refusals.push("process peak memory exceeds the frozen gate".into());
    }
    if !calibration_gate_met {
        refusals.push("production confidence calibration differs from the frozen gates".into());
    }
    Ok(PresetReleaseVerdict {
        preset: report.preset,
        identity: report.identity.clone(),
        delivery_policy_sha256: report.delivery_policy_sha256.clone(),
        reliability,
        source_coverage_met,
        render_coverage_met,
        accepted_render_boundary_p95_q95_px,
        accepted_render_boundary_p99_q99_px,
        accepted_boundary_max_px,
        accepted_boundary_samples,
        boundary_gates_met,
        max_palette_code_delta,
        palette_gate_met,
        runtime_p95_ms,
        runtime_limit_ms,
        runtime_isolated,
        runtime_gate_met,
        runtime_release_blocking: M7_RUNTIME_RELEASE_BLOCKING,
        runtime_policy: M7_RUNTIME_POLICY,
        peak_working_set_bytes: report.peak_working_set_bytes,
        memory_gate_met,
        calibration_gate_met,
        catastrophic_kinds: kinds,
        by_family: stratify(&rows, |row| row.shape_family.clone(), gates),
        by_size: stratify(&rows, |row| row.size_px.to_string(), gates),
        by_renderer: stratify(&rows, |row| row.rasterizer.clone(), gates),
        by_source_group: stratify(&rows, |row| row.group_id.clone(), gates),
        gate_met: refusals.is_empty(),
        refusals,
    })
}

fn calibration_matches_gates(
    report: &MeasurementReport,
    gates: M7ReleaseGates,
    expected_calibration_sha256: &str,
) -> bool {
    report
        .confidence_calibration
        .as_ref()
        .is_some_and(|calibration| {
            calibration.validate_for_identity(&report.identity).is_ok()
                && calibration.calibration_split_sha256 == expected_calibration_sha256
                && confidence_fields_match(calibration, report.preset, gates)
        })
}

fn confidence_fields_match(
    calibration: &vice_core::ConfidenceCalibration,
    preset: vice_core::Preset,
    gates: M7ReleaseGates,
) -> bool {
    let preset_gates = match preset {
        vice_core::Preset::Quality => gates.quality_calibration,
        vice_core::Preset::Fast => gates.fast_calibration,
    };
    calibration.posterior_lower_bound_threshold == preset_gates.posterior_lower_bound_threshold
        && calibration.empirical_unexplored_relative_mass_upper_bound
            == Some(preset_gates.empirical_unexplored_relative_mass_upper_bound)
        && calibration.minimum_top2_class_margin_bits == gates.min_top2_class_margin_bits
        && calibration.maximum_posterior_predictive_bits_per_block
            == preset_gates.max_posterior_predictive_bits_per_block
        && calibration.maximum_support_isotopy_displacement_px
            == preset_gates.max_support_isotopy_displacement_px
        && calibration.maximum_evidence_palette_shift_codes
            == preset_gates.max_evidence_palette_shift_codes
        && calibration.minimum_palette_support_px == preset_gates.min_palette_support_px
        && calibration.maximum_palette_interval_radius_codes
            == preset_gates.max_palette_interval_radius_codes
        && calibration.maximum_abs_residual_lag1 == gates.max_abs_residual_lag1
        && calibration.maximum_topology_entropy_bits == gates.max_topology_entropy_bits
        && calibration.maximum_formation_entropy_bits == gates.max_formation_entropy_bits
        && calibration.minimum_perturbation_stability == gates.min_perturbation_stability
}

pub(crate) fn catastrophic_with_gates(
    row: &MeasurementRow,
    gates: M7ReleaseGates,
) -> Vec<&'static str> {
    let mut kinds = intrinsic_catastrophic_kinds(row);
    if row
        .max_palette_code_delta
        .is_none_or(|value| value > gates.max_palette_code_delta)
        && !kinds.contains(&"wrong_paint_or_missing_face")
    {
        kinds.push("wrong_paint_or_missing_face");
    }
    if !delivery_within_gates(row, gates) && !kinds.contains(&"serialized_mismatch") {
        kinds.push("serialized_mismatch");
    }
    kinds
}

fn delivery_within_gates(row: &MeasurementRow, gates: M7ReleaseGates) -> bool {
    row.profile_max_channel_delta
        .is_some_and(|value| value <= gates.max_profile_channel_delta)
        && row
            .profile_mean_channel_delta
            .is_some_and(|value| value <= gates.max_profile_mean_channel_delta)
        && row
            .internal_to_pure_max_channel_delta
            .is_some_and(|value| value <= gates.max_internal_channel_delta)
        && row
            .internal_to_pure_mean_channel_delta
            .is_some_and(|value| value <= gates.max_internal_mean_channel_delta)
        && row
            .internal_to_seam_max_channel_delta
            .is_some_and(|value| value <= gates.max_internal_channel_delta)
        && row
            .internal_to_seam_mean_channel_delta
            .is_some_and(|value| value <= gates.max_internal_mean_channel_delta)
}

fn quantile(values: &[u64], q: f64) -> u64 {
    if values.is_empty() {
        return u64::MAX;
    }
    let mut values = values.to_vec();
    values.sort_unstable();
    let index = ((values.len() - 1) as f64 * q).ceil() as usize;
    values[index.min(values.len() - 1)]
}

fn stratify(
    rows: &[&MeasurementRow],
    key: impl Fn(&MeasurementRow) -> String,
    gates: M7ReleaseGates,
) -> BTreeMap<String, StratumSummary> {
    let mut out = BTreeMap::new();
    for row in rows {
        let accepted = row.production_accepted && row.production_provenance;
        let entry = out.entry(key(row)).or_insert_with(|| StratumSummary {
            renders: 0,
            accepted: 0,
            catastrophic: 0,
            refusal_statuses: BTreeMap::new(),
            refusal_reasons: BTreeMap::new(),
            cost_refusal_histogram: BTreeMap::new(),
            numerical_conditioning: ConditioningSummary::default(),
        });
        entry.renders += 1;
        for refusal in &row.cost_refusal_histogram {
            *entry
                .cost_refusal_histogram
                .entry(format!("{}/{}", refusal.family, refusal.reason))
                .or_default() += refusal.count;
        }
        merge_conditioning(&mut entry.numerical_conditioning, row);
        if accepted {
            entry.accepted += 1;
            entry.catastrophic += u64::from(!catastrophic_with_gates(row, gates).is_empty());
        } else {
            *entry
                .refusal_statuses
                .entry(row.decision_status.clone())
                .or_default() += 1;
            *entry
                .refusal_reasons
                .entry(
                    row.decision_reason
                        .clone()
                        .unwrap_or_else(|| "unclassified".into()),
                )
                .or_default() += 1;
        }
    }
    out
}

fn merge_conditioning(summary: &mut ConditioningSummary, row: &MeasurementRow) {
    let diagnostics = &row.numerical_conditioning;
    summary.evidence_pairs = summary
        .evidence_pairs
        .saturating_add(diagnostics.evidence_pairs);
    summary.evidence_pairs_refused = summary
        .evidence_pairs_refused
        .saturating_add(diagnostics.evidence_pairs_refused);
    summary.fit_runs = summary.fit_runs.saturating_add(diagnostics.fit_runs);
    summary.fit_material_cost_samples = summary
        .fit_material_cost_samples
        .saturating_add(diagnostics.fit_material_cost_samples);
    if diagnostics.evidence_conditioning_min.is_some_and(|value| {
        summary
            .evidence_conditioning_min
            .is_none_or(|old| value < old)
    }) {
        summary.evidence_conditioning_min = diagnostics.evidence_conditioning_min;
    }
    if diagnostics.evidence_conditioning_max.is_some_and(|value| {
        summary
            .evidence_conditioning_max
            .is_none_or(|old| value > old)
    }) {
        summary.evidence_conditioning_max = diagnostics.evidence_conditioning_max;
    }
    if diagnostics
        .fit_worst_normal_to_euclidean_ratio
        .is_some_and(|value| {
            summary
                .fit_worst_normal_to_euclidean_ratio
                .is_none_or(|old| value > old)
        })
    {
        summary.fit_worst_normal_to_euclidean_ratio =
            diagnostics.fit_worst_normal_to_euclidean_ratio;
        summary.fit_worst_ratio_at_deviation_px = diagnostics.fit_worst_ratio_at_deviation_px;
    }
}

fn population_keys(report: &MeasurementReport) -> BTreeSet<(&str, &str, &str)> {
    report
        .rows
        .iter()
        .map(|row| {
            (
                row.group_id.as_str(),
                row.scene_id.as_str(),
                row.cell_id.as_str(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests;
