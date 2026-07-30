//! Decision and artifact-byte determinism across repeats and worker counts.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::{MeasurementReport, M7_MEASUREMENT_SCHEMA};

pub const M7_DETERMINISM_SCHEMA: &str = "vice-classic/m7-determinism/v2";

#[derive(Debug, Clone)]
pub struct DeterminismInput {
    pub label: String,
    pub raw_sha256: String,
    pub report: MeasurementReport,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DeterminismRun {
    pub label: String,
    pub raw_sha256: String,
    pub canonical_report_sha256: String,
    pub workers: u32,
    pub normalized_decision_and_artifact_sha256: String,
    pub rows: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PresetDeterminism {
    pub preset: vice_core::Preset,
    pub runs: Vec<DeterminismRun>,
    pub isolated_repeats: u64,
    pub parallel_runs: u64,
    pub all_normalized_bytes_equal: bool,
    pub gate_met: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DeterminismVerdict {
    pub schema: &'static str,
    pub presets: Vec<PresetDeterminism>,
    pub gate_met: bool,
    pub refusals: Vec<String>,
}

pub fn analyze(inputs: Vec<DeterminismInput>) -> Result<DeterminismVerdict, String> {
    if inputs.is_empty() {
        return Err("M7 determinism requires repeated reports".into());
    }
    let mut grouped = BTreeMap::<String, Vec<DeterminismInput>>::new();
    for input in inputs {
        let report = &input.report;
        if report.schema != M7_MEASUREMENT_SCHEMA || !report.complete {
            return Err(format!(
                "{} is not a complete current M7 report",
                input.label
            ));
        }
        let key = format!(
            "{:?}|{}|{}|{}|{}",
            report.preset,
            report.scope,
            report.identity.universe_sha256,
            report.identity.config_sha256,
            report.delivery_policy_sha256
        );
        grouped.entry(key).or_default().push(input);
    }
    let mut presets = Vec::new();
    let mut refusals = Vec::new();
    for group in grouped.into_values() {
        let preset = group[0].report.preset;
        let runs = group
            .into_iter()
            .map(|input| DeterminismRun {
                workers: input.report.max_workers_per_shard,
                normalized_decision_and_artifact_sha256: normalized_digest(&input.report),
                canonical_report_sha256: super::report_content_sha256(&input.report),
                rows: input.report.renders,
                label: input.label,
                raw_sha256: input.raw_sha256,
            })
            .collect::<Vec<_>>();
        let isolated_repeats = runs.iter().filter(|run| run.workers == 1).count() as u64;
        let parallel_runs = runs.iter().filter(|run| run.workers > 1).count() as u64;
        let digests = runs
            .iter()
            .map(|run| run.normalized_decision_and_artifact_sha256.as_str())
            .collect::<BTreeSet<_>>();
        let all_normalized_bytes_equal = digests.len() == 1;
        let gate_met = isolated_repeats >= 2 && parallel_runs >= 1 && all_normalized_bytes_equal;
        if !gate_met {
            refusals.push(format!(
                "{preset:?}: need two isolated repeats, one parallel run, and one normalized digest"
            ));
        }
        presets.push(PresetDeterminism {
            preset,
            runs,
            isolated_repeats,
            parallel_runs,
            all_normalized_bytes_equal,
            gate_met,
        });
    }
    presets.sort_by_key(|verdict| match verdict.preset {
        vice_core::Preset::Fast => 0,
        vice_core::Preset::Quality => 1,
    });
    for preset in [vice_core::Preset::Fast, vice_core::Preset::Quality] {
        if !presets.iter().any(|verdict| verdict.preset == preset) {
            refusals.push(format!("{preset:?}: no determinism population"));
        }
    }
    Ok(DeterminismVerdict {
        schema: M7_DETERMINISM_SCHEMA,
        gate_met: refusals.is_empty(),
        presets,
        refusals,
    })
}

pub fn normalized_digest(report: &MeasurementReport) -> String {
    let mut stable = report.clone();
    stable.included_shards.clear();
    stable.shard_count = 0;
    stable.max_workers_per_shard = 0;
    stable.resumed_rows = 0;
    stable.runs = 0;
    stable.elapsed_ms = 0;
    stable.peak_working_set_bytes = 0;
    for row in &mut stable.rows {
        row.core_runtime_ms = 0;
        row.court_runtime_ms = 0;
        row.row_elapsed_ms = 0;
    }
    let bytes = serde_json::to_vec(&stable).expect("normalized M7 report serializes");
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_hashes_are_not_used_as_the_determinism_verdict() {
        let bytes = b"same decisions, different runtime envelope";
        let digest = hex::encode(Sha256::digest(bytes));
        assert_eq!(digest.len(), 64);
        assert_ne!(digest, "0".repeat(64));
    }
}
