//! Complete M7 PF/G/O oracle and controlled-recovery verdict.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use super::{MeasurementReport, PfArmMeasurement, M7_SEALED_POPULATION_POLICY};
use crate::gates::GatesFile;
use crate::gt::split::{AuditSeal, SealStatus};
use crate::m7::governance::M7ThresholdSource;
use crate::prereg::Preregistration;

pub const M7_ORACLE_SCHEMA: &str = "vice-classic/m7-complete-oracle/v3";
const EXPECTED_GEOMETRY_FIXTURE_SET_SHA256: &str =
    "bbc6e19f74d627be96869a66b4ec997f65b4228418bfb12ba187b77bb3551494";
const EXPECTED_GEOMETRY_SOURCE_GROUPS: usize = 26;
const EXPECTED_GEOMETRY_SCENES: usize = 26;
const EXPECTED_GEOMETRY_ATTEMPTED: usize = 19;
const EXPECTED_GEOMETRY_MEASURED: usize = 11;
const EXPECTED_GEOMETRY_EXCLUSIONS: usize = 17;
const EXPECTED_COMPLETE_SIX_ARM_ROWS: usize = 11;
const EXPECTED_G20_MEASURED: u64 = 6;
const EXPECTED_G20_REFUSED: u64 = 5;
const EXPECTED_G30_MEASURED: u64 = 11;
const EXPECTED_G30_REFUSED: u64 = 0;
const EXPECTED_GEOMETRY_FIXTURES: [&str; 11] = [
    "adv/ambiguous/hole-or-not#holed/face:1/loop:0/stage-f-chain:0",
    "adv/checker-corner#a/face:1/loop:0/stage-f-chain:0",
    "adv/checker-corner#a/face:2/loop:0/stage-f-chain:1",
    "authored/pennant#a/face:1/loop:0/stage-f-chain:0",
    "m6-witness/four-arc-circle/face:1/loop:0/stage-f-chain:0",
    "m6-witness/line-cubic-cornered/face:1/loop:0/stage-f-chain:0",
    "m6-witness/mixed-bezier/face:1/loop:0/stage-f-chain:0",
    "m6-witness/smooth-cubic-loop/face:1/loop:0/stage-f-chain:0",
    "proc/polygon/000#a/face:1/loop:0/stage-f-chain:0",
    "proc/polygon/001#a/face:1/loop:0/stage-f-chain:0",
    "proc/polygon/003#a/face:1/loop:0/stage-f-chain:0",
];

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
pub struct RendererGateRow {
    pub clause: String,
    pub met: bool,
    pub evidence: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RendererOracleVerdict {
    pub report: serde_json::Value,
    pub gate_rows: Vec<RendererGateRow>,
    pub gate_met: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct M7OracleVerdict {
    pub schema: &'static str,
    pub audit_generation: u32,
    pub corpus_sha256: String,
    pub population_commitment_sha256: String,
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
    pub renderer: RendererOracleVerdict,
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
        || quality.procedural_generation != audit.generation
        || fast.procedural_generation != audit.generation
        || quality.population_policy != M7_SEALED_POPULATION_POLICY
        || fast.population_policy != M7_SEALED_POPULATION_POLICY
    {
        return Err("M7 oracle is not bound to this opened audit and frozen analysis plan".into());
    }
    let gates = OracleGates::from_file(gates_file)?;
    let population_commitment_sha256 = quality.population_commitment_sha256.clone();
    let quality_pf = analyze_pf(quality, gates)?;
    let fast_pf = analyze_pf(fast, gates)?;
    let measurements = crate::geometry::measure_m7_raw()?;
    let geometry = analyze_geometry(measurements, gates);
    let renderer_run = crate::oracle::run(crate::oracle::OracleScope::Full)?;
    let renderer_report = crate::oracle::report::build(&renderer_run);
    let renderer_gate_rows = renderer_report
        .gate_table()
        .into_iter()
        .map(|(clause, met, evidence)| RendererGateRow {
            clause: clause.into(),
            met,
            evidence,
        })
        .collect::<Vec<_>>();
    let renderer_gate_met =
        !renderer_gate_rows.is_empty() && renderer_gate_rows.iter().all(|row| row.met);
    let renderer_report_value: serde_json::Value =
        serde_json::from_str(&renderer_report.canonical_json())
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
    if !renderer_gate_met {
        refusals.push("independent renderer/O gate table is not entirely green".into());
    }
    Ok(M7OracleVerdict {
        schema: M7_ORACLE_SCHEMA,
        audit_generation: audit.generation,
        corpus_sha256: audit.corpus_hash.clone(),
        population_commitment_sha256,
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
        renderer: RendererOracleVerdict {
            report: renderer_report_value,
            gate_rows: renderer_gate_rows,
            gate_met: renderer_gate_met,
        },
        gate_met: refusals.is_empty(),
        refusals,
    })
}

fn validate_report(report: &MeasurementReport, preset: vice_core::Preset) -> Result<(), String> {
    super::validate_sealed_population(report)?;
    super::validate_execution_attestation(report)?;
    if report.preset != preset {
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
        let attempted = rows.len() as u64;
        let recovery_rate = if attempted == 0 {
            0.0
        } else {
            // Refusals remain in the denominator. A solver cannot improve a
            // recovery score by turning difficult fixtures into refusals.
            recovered as f64 / attempted as f64
        };
        RecoveryModeVerdict {
            mode: id.into(),
            attempted,
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
    let base = &measurements.base;
    let expected_fixtures = EXPECTED_GEOMETRY_FIXTURES
        .into_iter()
        .collect::<BTreeSet<_>>();
    let base_fixtures = base
        .rows
        .iter()
        .map(|row| row.fixture_id.as_str())
        .collect::<BTreeSet<_>>();
    let g30_fixtures = measurements
        .g30
        .iter()
        .map(|row| row.fixture_id.as_str())
        .collect::<BTreeSet<_>>();
    let recovery_keys = measurements
        .recovery
        .iter()
        .map(|row| (row.fixture_id.as_str(), row.mode))
        .collect::<BTreeSet<_>>();
    let expected_recovery_keys = expected_fixtures
        .iter()
        .flat_map(|fixture| [(*fixture, "G20"), (*fixture, "G30")])
        .collect::<BTreeSet<_>>();
    if base.fixture_set_hash != EXPECTED_GEOMETRY_FIXTURE_SET_SHA256
        || base.source_groups != EXPECTED_GEOMETRY_SOURCE_GROUPS
        || base.scenes != EXPECTED_GEOMETRY_SCENES
        || base.boundaries_attempted != EXPECTED_GEOMETRY_ATTEMPTED
        || base.boundaries_measured != EXPECTED_GEOMETRY_MEASURED
        || base.exclusions.len() != EXPECTED_GEOMETRY_EXCLUSIONS
        || measurements.complete_six_arm_rows != EXPECTED_COMPLETE_SIX_ARM_ROWS
        || measurements.g30.len() != EXPECTED_COMPLETE_SIX_ARM_ROWS
        || measurements.recovery.len() != 2 * EXPECTED_COMPLETE_SIX_ARM_ROWS
        || base_fixtures != expected_fixtures
        || g30_fixtures != expected_fixtures
        || recovery_keys != expected_recovery_keys
    {
        refusals.push(
            "geometry fixture identity or exact attempted/measured population changed".into(),
        );
    }
    let g20_exact = g20_recovery.attempted == EXPECTED_COMPLETE_SIX_ARM_ROWS as u64
        && g20_recovery.measured == EXPECTED_G20_MEASURED
        && g20_recovery.refused == EXPECTED_G20_REFUSED;
    let g30_exact = g30_recovery.attempted == EXPECTED_COMPLETE_SIX_ARM_ROWS as u64
        && g30_recovery.measured == EXPECTED_G30_MEASURED
        && g30_recovery.refused == EXPECTED_G30_REFUSED;
    if !g20_exact || !g30_exact {
        refusals.push("G20/G30 attempted, measured, or refused counts changed".into());
    }
    if complete_six_arm_rows != gates.min_complete_geometry_rows {
        refusals.push(
            "complete six-arm geometry population differs from its exact frozen count".into(),
        );
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
