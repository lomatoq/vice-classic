//! Decision and artifact-byte determinism across repeats and worker counts.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::{M7RunRole, MeasurementReport, MeasurementRow};

pub const M7_DETERMINISM_SCHEMA: &str = "vice-classic/m7-determinism/v3";

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
    pub role: M7RunRole,
    pub run_id: String,
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
    pub release_commit_sha: String,
    pub runner_attestation_sha256: String,
    pub corpus_sha256: String,
    pub population_commitment_sha256: String,
    pub presets: Vec<PresetDeterminism>,
    pub gate_met: bool,
    pub refusals: Vec<String>,
}

pub fn analyze(inputs: Vec<DeterminismInput>) -> Result<DeterminismVerdict, String> {
    const ROLES: [M7RunRole; 6] = [
        M7RunRole::FastParallel,
        M7RunRole::FastPrimary,
        M7RunRole::FastRepeat,
        M7RunRole::QualityParallel,
        M7RunRole::QualityPrimary,
        M7RunRole::QualityRepeat,
    ];
    if inputs.len() != ROLES.len() {
        return Err("typed_refusal: determinism requires exactly six typed run roles".into());
    }
    let mut grouped = BTreeMap::<String, Vec<DeterminismInput>>::new();
    let mut roles = BTreeSet::new();
    let mut run_ids = BTreeSet::new();
    let mut raw_hashes = BTreeSet::new();
    let mut evidence_hashes = BTreeSet::new();
    let mut governance = None;
    for input in inputs {
        let report = &input.report;
        super::validate_sealed_population(report)?;
        super::validate_execution_attestation(report)?;
        let attestation = report.execution_attestation.as_ref().expect("validated");
        let context = &attestation.context;
        if !roles.insert(context.role)
            || !run_ids.insert(context.run_id.clone())
            || !raw_hashes.insert(input.raw_sha256.clone())
            || !evidence_hashes.insert(attestation.evidence_commitment_sha256.clone())
        {
            return Err(
                "typed_refusal: determinism roles, executions, and raw evidence must be distinct"
                    .into(),
            );
        }
        let this_governance = (
            context.candidate_commit_sha.clone(),
            context.runner_attestation_sha256.clone(),
            context.corpus_sha256.clone(),
            context.population_commitment_sha256.clone(),
        );
        if governance
            .as_ref()
            .is_some_and(|expected| expected != &this_governance)
        {
            return Err("typed_refusal: determinism inputs mix governance contexts".into());
        }
        governance = Some(this_governance);
        let key = format!(
            "{:?}|{}|{}|{}|{}|{}|{}",
            report.preset,
            report.scope,
            report.procedural_generation,
            report.population_policy,
            report.identity.universe_sha256,
            report.identity.config_sha256,
            report.delivery_policy_sha256
        );
        grouped.entry(key).or_default().push(input);
    }
    if roles != ROLES.into_iter().collect() {
        return Err("typed_refusal: determinism roles are missing or swapped".into());
    }
    let mut presets = Vec::new();
    let mut refusals = Vec::new();
    for group in grouped.into_values() {
        let preset = group[0].report.preset;
        let runs = group
            .into_iter()
            .map(|input| {
                let execution = input
                    .report
                    .execution_attestation
                    .as_ref()
                    .expect("validated");
                DeterminismRun {
                    workers: input.report.max_workers_per_shard,
                    normalized_decision_and_artifact_sha256: normalized_digest(&input.report),
                    canonical_report_sha256: super::report_content_sha256(&input.report),
                    rows: input.report.renders,
                    role: execution.context.role,
                    run_id: execution.context.run_id.clone(),
                    label: input.label,
                    raw_sha256: input.raw_sha256,
                }
            })
            .collect::<Vec<_>>();
        let isolated_repeats = runs.iter().filter(|run| run.workers == 1).count() as u64;
        let parallel_runs = runs.iter().filter(|run| run.workers > 1).count() as u64;
        let digests = runs
            .iter()
            .map(|run| run.normalized_decision_and_artifact_sha256.as_str())
            .collect::<BTreeSet<_>>();
        let all_normalized_bytes_equal = digests.len() == 1;
        let expected_roles = match preset {
            vice_core::Preset::Fast => [
                M7RunRole::FastParallel,
                M7RunRole::FastPrimary,
                M7RunRole::FastRepeat,
            ],
            vice_core::Preset::Quality => [
                M7RunRole::QualityParallel,
                M7RunRole::QualityPrimary,
                M7RunRole::QualityRepeat,
            ],
        };
        let run_roles = runs.iter().map(|run| run.role).collect::<BTreeSet<_>>();
        let gate_met = isolated_repeats == 2
            && parallel_runs == 1
            && run_roles == expected_roles.into_iter().collect()
            && all_normalized_bytes_equal;
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
    let (
        release_commit_sha,
        runner_attestation_sha256,
        corpus_sha256,
        population_commitment_sha256,
    ) = governance.expect("six validated inputs establish governance");
    Ok(DeterminismVerdict {
        schema: M7_DETERMINISM_SCHEMA,
        release_commit_sha,
        runner_attestation_sha256,
        corpus_sha256,
        population_commitment_sha256,
        gate_met: refusals.is_empty(),
        presets,
        refusals,
    })
}

pub fn normalized_digest(report: &MeasurementReport) -> String {
    #[derive(Serialize)]
    struct DecisionAndArtifact<'a> {
        group_id: &'a str,
        scene_id: &'a str,
        cell_id: &'a str,
        decision_status: &'a str,
        decision_reason: Option<&'a str>,
        production_provenance: bool,
        production_accepted: bool,
        candidate_available: bool,
        selected_hypothesis_id: Option<&'a str>,
        selected_scene_digest_sha256: Option<&'a str>,
        selected_delivery_digest_sha256: Option<&'a str>,
        selected_artifact_bundle_sha256: Option<&'a str>,
        verifier_clean: bool,
        measurement_refusal: Option<&'a str>,
    }

    impl<'a> From<&'a MeasurementRow> for DecisionAndArtifact<'a> {
        fn from(row: &'a MeasurementRow) -> Self {
            Self {
                group_id: &row.group_id,
                scene_id: &row.scene_id,
                cell_id: &row.cell_id,
                decision_status: &row.decision_status,
                decision_reason: row.decision_reason.as_deref(),
                production_provenance: row.production_provenance,
                production_accepted: row.production_accepted,
                candidate_available: row.candidate_available,
                selected_hypothesis_id: row.selected_hypothesis_id.as_deref(),
                selected_scene_digest_sha256: row.selected_scene_digest_sha256.as_deref(),
                selected_delivery_digest_sha256: row.selected_delivery_digest_sha256.as_deref(),
                selected_artifact_bundle_sha256: row.selected_artifact_bundle_sha256.as_deref(),
                verifier_clean: row.verifier_clean,
                measurement_refusal: row.measurement_refusal.as_deref(),
            }
        }
    }

    #[derive(Serialize)]
    struct Projection<'a> {
        schema: &'a str,
        scope: &'a str,
        split: &'a str,
        preset: vice_core::Preset,
        procedural_generation: u32,
        population_policy: &'a str,
        identity: &'a vice_opt::ModelIdentity,
        delivery_policy_sha256: &'a str,
        rows: Vec<DecisionAndArtifact<'a>>,
    }

    let stable = Projection {
        schema: &report.schema,
        scope: &report.scope,
        split: &report.split,
        preset: report.preset,
        procedural_generation: report.procedural_generation,
        population_policy: &report.population_policy,
        identity: &report.identity,
        delivery_policy_sha256: &report.delivery_policy_sha256,
        rows: report.rows.iter().map(DecisionAndArtifact::from).collect(),
    };
    let bytes = serde_json::to_vec(&stable).expect("M7 determinism projection serializes");
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::m7::tests::{sealed_report, synthetic_report};

    #[test]
    fn raw_hashes_are_not_used_as_the_determinism_verdict() {
        let bytes = b"same decisions, different runtime envelope";
        let digest = hex::encode(Sha256::digest(bytes));
        assert_eq!(digest.len(), 64);
        assert_ne!(digest, "0".repeat(64));
    }

    #[test]
    fn diagnostics_are_outside_the_decision_and_artifact_projection() {
        let mut changed = synthetic_report(0, 1);
        let original = changed.clone();
        changed.rows[0].candidate_bytes = 99;
        changed.rows[0].unexplored_proxy_hypotheses = Some(101);
        changed.rows[0].core_runtime_ms = 1234;
        changed.elapsed_ms = 5678;
        assert_eq!(normalized_digest(&original), normalized_digest(&changed));
    }

    #[test]
    fn decisions_and_selected_artifact_bytes_are_inside_the_projection() {
        let original = synthetic_report(0, 1);
        let mut decision_changed = original.clone();
        decision_changed.rows[0].decision_status = "success".into();
        assert_ne!(
            normalized_digest(&original),
            normalized_digest(&decision_changed)
        );

        let mut artifact_changed = original.clone();
        artifact_changed.rows[0].selected_artifact_bundle_sha256 = Some("a".repeat(64));
        assert_ne!(
            normalized_digest(&original),
            normalized_digest(&artifact_changed)
        );
    }

    #[test]
    fn determinism_requires_six_distinct_typed_executions() {
        let roles = [
            M7RunRole::FastParallel,
            M7RunRole::FastPrimary,
            M7RunRole::FastRepeat,
            M7RunRole::QualityParallel,
            M7RunRole::QualityPrimary,
            M7RunRole::QualityRepeat,
        ];
        let inputs = roles
            .into_iter()
            .enumerate()
            .map(|(index, role)| DeterminismInput {
                label: format!("{role:?}"),
                raw_sha256: format!("{:064x}", index + 1),
                report: sealed_report(role, char::from(b'a' + index as u8)),
            })
            .collect::<Vec<_>>();
        assert!(analyze(inputs.clone()).unwrap().gate_met);

        let mut duplicated = inputs;
        let duplicate_run_id = duplicated[0]
            .report
            .execution_attestation
            .as_ref()
            .unwrap()
            .context
            .run_id
            .clone();
        let attestation = duplicated[1].report.execution_attestation.as_ref().unwrap();
        let mut context = attestation.context.clone();
        let evidence = attestation.evidence_commitment_sha256.clone();
        context.run_id = duplicate_run_id;
        crate::m7::attach_execution_attestation(&mut duplicated[1].report, context, evidence)
            .unwrap();
        assert!(analyze(duplicated).is_err());
    }
}
