//! Complete M7 PF/G/O oracle and controlled-recovery verdict.

use std::collections::BTreeMap;

use serde::Serialize;

use super::{MeasurementReport, PfArmMeasurement, M7_MEASUREMENT_SCHEMA};
use crate::gates::GatesFile;
use crate::gt::split::{AuditSeal, SealStatus};
use crate::m7::governance::M7ThresholdSource;
use crate::prereg::Preregistration;

pub const M7_ORACLE_SCHEMA: &str = "vice-classic/m7-complete-oracle/v2";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PfArmAggregate {
    pub arm: String,
    pub rows: u64,
    pub mean_premultiplied_code_delta: f64,
    pub worst_premultiplied_code_delta: f64,
    pub min_identical_pixels_fraction: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PfEffects {
    pub rows: u64,
    pub partition_main_effect_mean_code: f64,
    pub formation_main_effect_mean_code: f64,
    pub interaction_mean_code: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PfPresetVerdict {
    pub preset: vice_core::Preset,
    pub candidate_rows: u64,
    pub complete_rows: u64,
    pub refused_rows: u64,
    pub arms: Vec<PfArmAggregate>,
    pub effects: PfEffects,
    pub gate_met: bool,
    pub refusals: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecoveryModeVerdict {
    pub mode: String,
    pub attempted: u64,
    pub measured: u64,
    pub recovered: u64,
    pub refused: u64,
    pub recovery_rate: f64,
    pub gate_met: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GeometryOracleVerdict {
    pub measurements: crate::geometry::M7GeometryExtension,
    pub complete_six_arm_rows: u64,
    pub g20_recovery: RecoveryModeVerdict,
    pub g30_recovery: RecoveryModeVerdict,
    pub gate_met: bool,
    pub refusals: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct M7OracleVerdict {
    pub schema: &'static str,
    pub audit_generation: u32,
    pub corpus_sha256: String,
    pub preregistration_sha256: String,
    pub gates_sha256: String,
    pub release_commit_sha: String,
    pub runner_attestation_sha256: String,
    pub gate_provenance_sha256: String,
    pub quality_report_sha256: String,
    pub fast_report_sha256: String,
    pub quality_pf: PfPresetVerdict,
    pub fast_pf: PfPresetVerdict,
    pub geometry: GeometryOracleVerdict,
    /// Historical independent renderer/serialization ceiling rerun. Kept as
    /// its canonical report value rather than relabelled as one of the new
    /// production-partition arms.
    pub renderer_ceiling_report: serde_json::Value,
    pub gate_met: bool,
    pub refusals: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
struct OracleGates {
    min_pf_complete_rows: u64,
    min_complete_geometry_rows: u64,
    min_g20_recovery_rows: u64,
    min_g30_recovery_rows: u64,
    min_recovery_rate: f64,
}

impl OracleGates {
    fn from_file(gates: &GatesFile) -> Result<Self, String> {
        Ok(Self {
            min_pf_complete_rows: integer(gates, "m7_selective", "gate_min_pf_complete_rows")?,
            min_complete_geometry_rows: integer(
                gates,
                "m7_selective",
                "gate_min_complete_geometry_oracle_rows",
            )?,
            min_g20_recovery_rows: integer(gates, "m7_selective", "gate_min_g20_recovery_rows")?,
            min_g30_recovery_rows: integer(gates, "m7_selective", "gate_min_g30_recovery_rows")?,
            min_recovery_rate: number(gates, "m7_selective", "gate_min_geometry_recovery_rate")?,
        })
    }
}

fn integer(gates: &GatesFile, section: &str, key: &str) -> Result<u64, String> {
    gates
        .gate_value(section, key)
        .map_err(|error| error.to_string())?
        .as_integer()
        .and_then(|value| value.try_into().ok())
        .ok_or_else(|| format!("{section}.{key} is not a non-negative integer"))
}

fn number(gates: &GatesFile, section: &str, key: &str) -> Result<f64, String> {
    let value = gates
        .gate_value(section, key)
        .map_err(|error| error.to_string())?;
    value
        .as_float()
        .or_else(|| value.as_integer().map(|value| value as f64))
        .filter(|value| value.is_finite())
        .ok_or_else(|| format!("{section}.{key} is not finite"))
}

pub fn run_release(
    quality: &MeasurementReport,
    fast: &MeasurementReport,
    audit: &AuditSeal,
    threshold_source: &M7ThresholdSource,
) -> Result<M7OracleVerdict, String> {
    let gates_file = &threshold_source.gates;
    validate_report(quality, vice_core::Preset::Quality)?;
    validate_report(fast, vice_core::Preset::Fast)?;
    if audit.status != SealStatus::Opened
        || audit.gates_hash != gates_file.sha256
        || audit.prereg_hash != Preregistration::v1().hash()
    {
        return Err("M7 oracle is not bound to this opened audit and frozen analysis plan".into());
    }
    let gates = OracleGates::from_file(gates_file)?;
    let quality_pf = analyze_pf(quality, gates)?;
    let fast_pf = analyze_pf(fast, gates)?;
    let measurements = crate::geometry::measure_m7_raw()?;
    let geometry = analyze_geometry(measurements, gates);
    let renderer_run = crate::oracle::run(crate::oracle::OracleScope::Full)?;
    let renderer_ceiling_report: serde_json::Value =
        serde_json::from_str(&crate::oracle::report::build(&renderer_run).canonical_json())
            .map_err(|error| error.to_string())?;
    let mut refusals = Vec::new();
    if !quality_pf.gate_met {
        refusals.push("Quality PF factorial is incomplete".into());
    }
    if !fast_pf.gate_met {
        refusals.push("Fast PF factorial is incomplete".into());
    }
    if !geometry.gate_met {
        refusals.push("six-arm geometry/recovery verdict is not green".into());
    }
    Ok(M7OracleVerdict {
        schema: M7_ORACLE_SCHEMA,
        audit_generation: audit.generation,
        corpus_sha256: audit.corpus_hash.clone(),
        preregistration_sha256: audit.prereg_hash.clone(),
        gates_sha256: audit.gates_hash.clone(),
        release_commit_sha: threshold_source.event_commit_sha.clone(),
        runner_attestation_sha256: threshold_source.attestation_sha256.clone(),
        gate_provenance_sha256: threshold_source.provenance_sha256.clone(),
        quality_report_sha256: super::report_content_sha256(quality),
        fast_report_sha256: super::report_content_sha256(fast),
        quality_pf,
        fast_pf,
        geometry,
        renderer_ceiling_report,
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
    {
        return Err(format!(
            "M7 oracle requires a complete {preset:?} sealed-audit report"
        ));
    }
    Ok(())
}

fn analyze_pf(report: &MeasurementReport, gates: OracleGates) -> Result<PfPresetVerdict, String> {
    let rows = report
        .rows
        .iter()
        .filter(|row| row.candidate_available)
        .collect::<Vec<_>>();
    let complete = rows
        .iter()
        .filter_map(|row| row.pf_oracle.as_ref())
        .filter(|oracle| oracle.complete)
        .collect::<Vec<_>>();
    let mut by_arm = BTreeMap::<&str, Vec<&PfArmMeasurement>>::new();
    for oracle in &complete {
        for arm in &oracle.arms {
            by_arm.entry(arm.arm.as_str()).or_default().push(arm);
        }
    }
    let mut arms = Vec::new();
    for id in ["PF00", "PF01", "PF10", "PF11"] {
        let values = by_arm.get(id).cloned().unwrap_or_default();
        arms.push(PfArmAggregate {
            arm: id.into(),
            rows: values.len() as u64,
            mean_premultiplied_code_delta: mean(
                values.iter().map(|arm| arm.mean_premultiplied_code_delta),
            ),
            worst_premultiplied_code_delta: values
                .iter()
                .map(|arm| arm.max_premultiplied_code_delta)
                .fold(0.0, f64::max),
            min_identical_pixels_fraction: values
                .iter()
                .map(|arm| arm.identical_pixels_fraction)
                .fold(1.0, f64::min),
        });
    }
    let effects_rows = complete
        .iter()
        .filter_map(|oracle| {
            let value = |id: &str| {
                oracle
                    .arms
                    .iter()
                    .find(|arm| arm.arm == id)
                    .map(|arm| arm.mean_premultiplied_code_delta)
            };
            Some([
                value("PF00")?,
                value("PF10")?,
                value("PF01")?,
                value("PF11")?,
            ])
        })
        .collect::<Vec<_>>();
    let effects = PfEffects {
        rows: effects_rows.len() as u64,
        partition_main_effect_mean_code: mean(
            effects_rows
                .iter()
                .map(|values| 0.5 * ((values[1] - values[0]) + (values[3] - values[2]))),
        ),
        formation_main_effect_mean_code: mean(
            effects_rows
                .iter()
                .map(|values| 0.5 * ((values[2] - values[0]) + (values[3] - values[1]))),
        ),
        interaction_mean_code: mean(
            effects_rows
                .iter()
                .map(|values| 0.5 * ((values[3] - values[2]) - (values[1] - values[0]))),
        ),
    };
    let refused_rows = rows.len().saturating_sub(complete.len()) as u64;
    let mut refusals = Vec::new();
    if (complete.len() as u64) < gates.min_pf_complete_rows {
        refusals.push(format!(
            "{} complete PF rows below frozen floor {}",
            complete.len(),
            gates.min_pf_complete_rows
        ));
    }
    if refused_rows != 0 {
        refusals.push(format!(
            "{refused_rows} candidate rows have an incomplete PF factorial"
        ));
    }
    if arms.iter().any(|arm| arm.rows != complete.len() as u64) {
        refusals.push("PF arms do not share one common row population".into());
    }
    Ok(PfPresetVerdict {
        preset: report.preset,
        candidate_rows: rows.len() as u64,
        complete_rows: complete.len() as u64,
        refused_rows,
        arms,
        effects,
        gate_met: refusals.is_empty(),
        refusals,
    })
}

fn analyze_geometry(
    measurements: crate::geometry::M7GeometryExtension,
    gates: OracleGates,
) -> GeometryOracleVerdict {
    let mode = |id: &str, minimum: u64| {
        let rows = measurements
            .recovery
            .iter()
            .filter(|row| row.mode == id)
            .collect::<Vec<_>>();
        let measured = rows.iter().filter(|row| row.status == "measured").count() as u64;
        let recovered = rows
            .iter()
            .filter(|row| row.normal_objective_recovered)
            .count() as u64;
        let recovery_rate = if measured == 0 {
            0.0
        } else {
            recovered as f64 / measured as f64
        };
        RecoveryModeVerdict {
            mode: id.into(),
            attempted: rows.len() as u64,
            measured,
            recovered,
            refused: rows.iter().filter(|row| row.status == "refused").count() as u64,
            recovery_rate,
            gate_met: measured >= minimum && recovery_rate >= gates.min_recovery_rate,
        }
    };
    let g20_recovery = mode("G20", gates.min_g20_recovery_rows);
    let g30_recovery = mode("G30", gates.min_g30_recovery_rows);
    let complete_six_arm_rows = measurements.complete_six_arm_rows as u64;
    let mut refusals = Vec::new();
    if complete_six_arm_rows < gates.min_complete_geometry_rows {
        refusals.push("complete six-arm geometry population is below its frozen floor".into());
    }
    if measurements
        .g30
        .iter()
        .any(|arm| !arm.canonical_roundtrip_identical)
    {
        refusals.push("a G30 canonical parameter roundtrip changed geometry".into());
    }
    if !g20_recovery.gate_met || !g30_recovery.gate_met {
        refusals.push("controlled G20/G30 recovery is below its frozen rate or population".into());
    }
    GeometryOracleVerdict {
        measurements,
        complete_six_arm_rows,
        g20_recovery,
        g30_recovery,
        gate_met: refusals.is_empty(),
        refusals,
    }
}

fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let values = values.collect::<Vec<_>>();
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}
