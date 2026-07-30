//! Frozen-gate analysis of the untouched M7 sealed audit.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use super::{
    analysis::catastrophic_kinds, MeasurementReport, MeasurementRow, M7_MEASUREMENT_SCHEMA,
};
use crate::correlation::ResidualModel;
use crate::gates::GatesFile;
use crate::gt::raster::RasterProfile;
use crate::gt::split::{AuditSeal, SealStatus};
use crate::m7::governance::M7ThresholdSource;
use crate::prereg::Preregistration;
use crate::reliability::{risk_coverage, RenderOutcome, RiskCoverage};

pub const M7_RELEASE_VERDICT_SCHEMA: &str = "vice-classic/m7-release-verdict/v1";
const TARGET_BUCKET: &str = "flat2-clean-aa-identifiable-128-512";

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
        })
    }
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
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PresetReleaseVerdict {
    pub preset: vice_core::Preset,
    pub identity: vice_opt::ModelIdentity,
    pub delivery_policy_sha256: String,
    pub reliability: RiskCoverage,
    pub source_coverage_met: bool,
    pub render_coverage_met: bool,
    pub accepted_boundary_p95_worst_px: Option<f64>,
    pub accepted_boundary_p99_worst_px: Option<f64>,
    pub accepted_boundary_max_worst_px: Option<f64>,
    pub boundary_gates_met: bool,
    pub max_palette_code_delta: Option<u8>,
    pub palette_gate_met: bool,
    pub runtime_p95_ms: u64,
    pub runtime_limit_ms: u64,
    pub runtime_isolated: bool,
    pub runtime_gate_met: bool,
    pub peak_working_set_bytes: u64,
    pub memory_gate_met: bool,
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
    pub preregistration_sha256: String,
    pub gates_sha256: String,
    pub release_commit_sha: String,
    pub runner_attestation_sha256: String,
    pub gate_provenance_sha256: String,
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
    let prereg = Preregistration::v1();
    prereg
        .check()
        .map_err(|errors| format!("invalid preregistration: {}", errors.join("; ")))?;
    if audit.prereg_hash != prereg.hash() || audit.gates_hash != gates_file.sha256 {
        return Err("audit record is not bound to this preregistration and gate file".into());
    }
    let gates = M7ReleaseGates::from_file(gates_file)?;
    let quality_verdict = analyze_preset(quality, &prereg, gates)?;
    let fast_verdict = analyze_preset(fast, &prereg, gates)?;
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
        preregistration_sha256: audit.prereg_hash.clone(),
        gates_sha256: audit.gates_hash.clone(),
        release_commit_sha: threshold_source.event_commit_sha.clone(),
        runner_attestation_sha256: threshold_source.attestation_sha256.clone(),
        gate_provenance_sha256: threshold_source.provenance_sha256.clone(),
        quality: quality_verdict,
        fast: fast_verdict,
        same_population,
        gate_met: refusals.is_empty(),
        refusals,
    })
}

fn validate_report(report: &MeasurementReport, preset: vice_core::Preset) -> Result<(), String> {
    if report.schema != M7_MEASUREMENT_SCHEMA
        || report.scope != "sealed_audit"
        || report.split != "sealed_audit"
        || report.preset != preset
        || !report.complete
        || report.included_shards.len() != report.shard_count as usize
        || report.renders != report.expected_renders_included_shards
    {
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
    let accepted_boundary_p95_worst_px = max_boundary(&accepted, |tail| tail.p95_px);
    let accepted_boundary_p99_worst_px = max_boundary(&accepted, |tail| tail.p99_px);
    let accepted_boundary_max_worst_px = max_boundary(&accepted, |tail| tail.max_px);
    let boundary_gates_met = accepted_boundary_p95_worst_px
        .is_some_and(|value| value <= gates.boundary_p95_px)
        && accepted_boundary_p99_worst_px.is_some_and(|value| value <= gates.boundary_p99_px)
        && accepted_boundary_max_worst_px.is_some_and(|value| value <= gates.boundary_max_px);
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
    if !runtime_gate_met {
        refusals.push("isolated runtime p95 gate is not met".into());
    }
    if !memory_gate_met {
        refusals.push("process peak memory exceeds the frozen gate".into());
    }
    Ok(PresetReleaseVerdict {
        preset: report.preset,
        identity: report.identity.clone(),
        delivery_policy_sha256: report.delivery_policy_sha256.clone(),
        reliability,
        source_coverage_met,
        render_coverage_met,
        accepted_boundary_p95_worst_px,
        accepted_boundary_p99_worst_px,
        accepted_boundary_max_worst_px,
        boundary_gates_met,
        max_palette_code_delta,
        palette_gate_met,
        runtime_p95_ms,
        runtime_limit_ms,
        runtime_isolated,
        runtime_gate_met,
        peak_working_set_bytes: report.peak_working_set_bytes,
        memory_gate_met,
        catastrophic_kinds: kinds,
        by_family: stratify(&rows, |row| row.shape_family.clone(), gates),
        by_size: stratify(&rows, |row| row.size_px.to_string(), gates),
        by_renderer: stratify(&rows, |row| row.rasterizer.clone(), gates),
        by_source_group: stratify(&rows, |row| row.group_id.clone(), gates),
        gate_met: refusals.is_empty(),
        refusals,
    })
}

pub(crate) fn catastrophic_with_gates(
    row: &MeasurementRow,
    gates: M7ReleaseGates,
) -> Vec<&'static str> {
    let mut kinds = catastrophic_kinds(row);
    if row.boundary.as_ref().is_none_or(|tail| {
        tail.p99_px > gates.boundary_p99_px || tail.max_px > gates.boundary_max_px
    }) && !kinds.contains(&"gross_boundary_outlier")
    {
        kinds.push("gross_boundary_outlier");
    }
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

fn max_boundary(
    rows: &[&MeasurementRow],
    value: impl Fn(&super::BoundaryTail) -> f64,
) -> Option<f64> {
    rows.iter()
        .filter_map(|row| row.boundary.as_ref().map(&value))
        .max_by(f64::total_cmp)
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
        });
        entry.renders += 1;
        if accepted {
            entry.accepted += 1;
            entry.catastrophic += u64::from(!catastrophic_with_gates(row, gates).is_empty());
        } else {
            *entry
                .refusal_statuses
                .entry(row.decision_status.clone())
                .or_default() += 1;
        }
    }
    out
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
mod tests {
    use super::*;
    use crate::gates::{GateSection, GatesDoc};

    fn file(status: &str) -> GatesFile {
        let boundary = GateSection {
            status: status.into(),
            set_by_milestone: (status == "placeholder").then(|| "M7".into()),
            values: [
                ("p95_px", toml::Value::Float(0.35)),
                ("p99_px", toml::Value::Float(0.60)),
                ("max_px", toml::Value::Float(1.50)),
            ]
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
        };
        let selective = GateSection {
            status: status.into(),
            set_by_milestone: (status == "placeholder").then(|| "M7".into()),
            values: [
                ("gate_min_source_coverage_128_512", toml::Value::Float(0.8)),
                ("gate_min_render_coverage_128_512", toml::Value::Float(0.8)),
                ("gate_max_palette_code_delta", toml::Value::Integer(4)),
                ("gate_max_quality_p95_ms", toml::Value::Integer(10_000)),
                ("gate_max_fast_p95_ms", toml::Value::Integer(1_000)),
                (
                    "gate_max_peak_memory_bytes",
                    toml::Value::Integer(1_073_741_824),
                ),
                ("gate_max_profile_channel_delta", toml::Value::Integer(0)),
                (
                    "gate_max_profile_mean_channel_delta",
                    toml::Value::Float(0.0),
                ),
                ("gate_max_internal_channel_delta", toml::Value::Integer(64)),
                (
                    "gate_max_internal_mean_channel_delta",
                    toml::Value::Float(0.25),
                ),
            ]
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
        };
        GatesFile {
            doc: GatesDoc {
                schema: crate::gates::GATES_SCHEMA.into(),
                version: "v1".into(),
                sections: [
                    ("boundary_accuracy".into(), boundary),
                    ("m7_selective".into(), selective),
                ]
                .into_iter()
                .collect(),
            },
            sha256: "0".repeat(64),
        }
    }

    #[test]
    fn release_values_are_unreadable_until_the_gate_only_freeze() {
        assert!(M7ReleaseGates::from_file(&file("placeholder")).is_err());
        let frozen = M7ReleaseGates::from_file(&file("frozen")).expect("frozen values load");
        assert_eq!(frozen.max_internal_channel_delta, 64);
        assert_eq!(frozen.min_render_coverage, 0.8);
    }
}
