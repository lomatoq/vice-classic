//! Burn-controlled M7 sealed-audit commands.

use std::path::Path;

use vice_bench::gt::split::{AuditSeal, SealStatus};
use vice_bench::m7::{
    self,
    governance::{GateDigestInput, M7ThresholdSource},
    MeasurementRequest, MeasurementScope,
};
use vice_bench::prereg::Preregistration;
use vice_core::{CoreConfig, Preset};

fn read_seal(path: &Path) -> Result<AuditSeal, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    serde_json::from_str(&text).map_err(|error| format!("parse {}: {error}", path.display()))
}

fn write_seal(path: &Path, seal: &AuditSeal) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(seal).map_err(|error| error.to_string())?;
    std::fs::write(path, [bytes.as_slice(), b"\n"].concat())
        .map_err(|error| format!("write {}: {error}", path.display()))
}

fn threshold_source(
    runner_attestation: &Path,
    gates: &Path,
    gate_provenance: &Path,
) -> Result<M7ThresholdSource, String> {
    m7::governance::load_threshold_source(runner_attestation, gates, gate_provenance)
}

fn release_hashes(
    manifest: &Path,
    gate_digest: &GateDigestInput,
) -> Result<(String, String, String), String> {
    let recorded = super::read_manifest(&manifest.to_path_buf())?;
    let corpus_hash = super::rebuild_matching(&recorded)?.hash();
    let gates_hash = gate_digest.sha256.clone();
    Ok((corpus_hash, Preregistration::v1().hash(), gates_hash))
}

#[allow(clippy::too_many_arguments)]
pub fn runner_attest(
    anchor_source: &str,
    event_commit: &str,
    repository_root: &Path,
    git_executable: &Path,
    vicec_executable: &Path,
    gates: &Path,
    gate_provenance: &Path,
    out: &Path,
) -> Result<m7::governance::RunnerAttestation, String> {
    let attestation = m7::governance::create_attestation(
        anchor_source,
        event_commit,
        repository_root,
        git_executable,
        vicec_executable,
        gates,
        gate_provenance,
    )?;
    m7::governance::write_attestation(out, &attestation)?;
    Ok(attestation)
}

pub fn open(
    seal_path: &Path,
    manifest: &Path,
    gates: &Path,
    runner_attestation: &Path,
    gate_provenance: &Path,
    note: &str,
) -> Result<AuditSeal, String> {
    let threshold_source = threshold_source(runner_attestation, gates, gate_provenance)?;
    let seal = read_seal(seal_path)?;
    if seal.status != SealStatus::Sealed {
        return Err(format!(
            "audit generation {} is {:?}; only a sealed generation may be opened",
            seal.generation, seal.status
        ));
    }
    if note.trim() != threshold_source.event_commit_sha {
        return Err("opening note must be exactly the externally anchored release commit".into());
    }
    let (corpus_hash, prereg_hash, gates_hash) =
        release_hashes(manifest, &threshold_source.digest_input)?;
    let opened = seal.open(&corpus_hash, &prereg_hash, &gates_hash, note);
    write_seal(seal_path, &opened)?;
    Ok(opened)
}

#[allow(clippy::too_many_arguments)]
pub fn measure(
    seal_path: &Path,
    manifest: &Path,
    gates: &Path,
    runner_attestation: &Path,
    gate_provenance: &Path,
    production_config: &Path,
    preset: Preset,
    out: &Path,
    workers: usize,
    shard_index: u32,
    shard_count: u32,
    resume: bool,
) -> Result<m7::MeasurementReport, String> {
    let threshold_source = threshold_source(runner_attestation, gates, gate_provenance)?;
    let seal = read_seal(seal_path)?;
    let (corpus_hash, prereg_hash, gates_hash) =
        release_hashes(manifest, &threshold_source.digest_input)?;
    seal.check(&corpus_hash, &prereg_hash, &gates_hash)
        .map_err(|error| error.to_string())?;
    let config = CoreConfig::load_production_for(preset, production_config)
        .map_err(|error| format!("production config refused: {error}"))?;
    let mut request = MeasurementRequest::new(MeasurementScope::SealedAudit);
    request.preset = preset;
    request.workers = workers;
    request.shard_index = shard_index;
    request.shard_count = shard_count;
    m7::measure_to_path_with_config(request, &config, out, resume)
}

pub fn analyze(
    seal_path: &Path,
    manifest: &Path,
    gates_path: &Path,
    runner_attestation: &Path,
    gate_provenance: &Path,
    quality_report: &Path,
    fast_report: &Path,
    out: &Path,
) -> Result<m7::release::ReleaseVerdict, String> {
    let threshold_source = threshold_source(runner_attestation, gates_path, gate_provenance)?;
    let seal = read_seal(seal_path)?;
    let (corpus_hash, prereg_hash, gates_hash) =
        release_hashes(manifest, &threshold_source.digest_input)?;
    seal.check(&corpus_hash, &prereg_hash, &gates_hash)
        .map_err(|error| error.to_string())?;
    let quality = m7::read_report(quality_report)?;
    let fast = m7::read_report(fast_report)?;
    let verdict = m7::release::analyze_release(&quality, &fast, &seal, &threshold_source)?;
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(&verdict).map_err(|error| error.to_string())?;
    std::fs::write(out, format!("{text}\n"))
        .map_err(|error| format!("write {}: {error}", out.display()))?;
    Ok(verdict)
}

pub fn determinism(
    inputs: &[std::path::PathBuf],
    out: &Path,
) -> Result<m7::determinism::DeterminismVerdict, String> {
    let reports = inputs
        .iter()
        .map(|path| {
            Ok(m7::determinism::DeterminismInput {
                label: path.display().to_string(),
                raw_sha256: vice_bench::hashing::sha256_file(path)
                    .map_err(|error| error.to_string())?,
                report: m7::read_report(path)?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let verdict = m7::determinism::analyze(reports)?;
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(&verdict).map_err(|error| error.to_string())?;
    std::fs::write(out, format!("{text}\n"))
        .map_err(|error| format!("write {}: {error}", out.display()))?;
    Ok(verdict)
}

pub fn baseline_court(
    seal_path: &Path,
    manifest: &Path,
    gates_path: &Path,
    runner_attestation: &Path,
    gate_provenance: &Path,
    quality_report: &Path,
    fast_report: &Path,
    out: &Path,
) -> Result<m7::baseline::BaselineCourtVerdict, String> {
    let threshold_source = threshold_source(runner_attestation, gates_path, gate_provenance)?;
    let seal = read_seal(seal_path)?;
    let (corpus_hash, prereg_hash, gates_hash) =
        release_hashes(manifest, &threshold_source.digest_input)?;
    seal.check(&corpus_hash, &prereg_hash, &gates_hash)
        .map_err(|error| error.to_string())?;
    let quality = m7::read_report(quality_report)?;
    let fast = m7::read_report(fast_report)?;
    let verdict = m7::baseline::analyze(&quality, &fast, &seal, &threshold_source)?;
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(&verdict).map_err(|error| error.to_string())?;
    std::fs::write(out, format!("{text}\n"))
        .map_err(|error| format!("write {}: {error}", out.display()))?;
    Ok(verdict)
}

pub fn oracle(
    seal_path: &Path,
    manifest: &Path,
    gates_path: &Path,
    runner_attestation: &Path,
    gate_provenance: &Path,
    quality_report: &Path,
    fast_report: &Path,
    out: &Path,
) -> Result<m7::oracle::M7OracleVerdict, String> {
    let threshold_source = threshold_source(runner_attestation, gates_path, gate_provenance)?;
    let seal = read_seal(seal_path)?;
    let (corpus_hash, prereg_hash, gates_hash) =
        release_hashes(manifest, &threshold_source.digest_input)?;
    seal.check(&corpus_hash, &prereg_hash, &gates_hash)
        .map_err(|error| error.to_string())?;
    let quality = m7::read_report(quality_report)?;
    let fast = m7::read_report(fast_report)?;
    let verdict = m7::oracle::run_release(&quality, &fast, &seal, &threshold_source)?;
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(&verdict).map_err(|error| error.to_string())?;
    std::fs::write(out, format!("{text}\n"))
        .map_err(|error| format!("write {}: {error}", out.display()))?;
    Ok(verdict)
}

pub fn geometry_calibrate(out: &Path) -> Result<vice_bench::geometry::M7GeometryExtension, String> {
    let measurements = vice_bench::geometry::measure_m7_raw()?;
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(&measurements).map_err(|error| error.to_string())?;
    std::fs::write(out, format!("{text}\n"))
        .map_err(|error| format!("write {}: {error}", out.display()))?;
    Ok(measurements)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_opened_generation_cannot_be_opened_again() {
        let seal = AuditSeal::sealed(3).open("c", "p", "g", "sha");
        assert_eq!(seal.status, SealStatus::Opened);
        assert_ne!(seal.status, SealStatus::Sealed);
    }
}
