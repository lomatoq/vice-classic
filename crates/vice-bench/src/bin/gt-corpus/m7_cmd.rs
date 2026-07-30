//! Burn-controlled M7 sealed-audit commands.

use std::path::Path;

use vice_bench::gates::GatesFile;
use vice_bench::gt::split::{AuditSeal, SealStatus};
use vice_bench::m7::{self, MeasurementRequest, MeasurementScope};
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

fn release_hashes(manifest: &Path, gates: &Path) -> Result<(String, String, String), String> {
    let recorded = super::read_manifest(&manifest.to_path_buf())?;
    let corpus_hash = super::rebuild_matching(&recorded)?.hash();
    let gates_hash = GatesFile::load_for_a_gate_decision(gates)
        .map_err(|error| error.to_string())?
        .sha256;
    Ok((corpus_hash, Preregistration::v1().hash(), gates_hash))
}

pub fn open(
    seal_path: &Path,
    manifest: &Path,
    gates: &Path,
    note: &str,
) -> Result<AuditSeal, String> {
    let seal = read_seal(seal_path)?;
    if seal.status != SealStatus::Sealed {
        return Err(format!(
            "audit generation {} is {:?}; only a sealed generation may be opened",
            seal.generation, seal.status
        ));
    }
    if note.trim().is_empty() {
        return Err("opening note must identify the release candidate".into());
    }
    let (corpus_hash, prereg_hash, gates_hash) = release_hashes(manifest, gates)?;
    let opened = seal.open(&corpus_hash, &prereg_hash, &gates_hash, note);
    write_seal(seal_path, &opened)?;
    Ok(opened)
}

#[allow(clippy::too_many_arguments)]
pub fn measure(
    seal_path: &Path,
    manifest: &Path,
    gates: &Path,
    production_config: &Path,
    preset: Preset,
    out: &Path,
    workers: usize,
    shard_index: u32,
    shard_count: u32,
    resume: bool,
) -> Result<m7::MeasurementReport, String> {
    let seal = read_seal(seal_path)?;
    let (corpus_hash, prereg_hash, gates_hash) = release_hashes(manifest, gates)?;
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
    quality_report: &Path,
    fast_report: &Path,
    out: &Path,
) -> Result<m7::release::ReleaseVerdict, String> {
    let seal = read_seal(seal_path)?;
    let (corpus_hash, prereg_hash, gates_hash) = release_hashes(manifest, gates_path)?;
    seal.check(&corpus_hash, &prereg_hash, &gates_hash)
        .map_err(|error| error.to_string())?;
    let gates =
        GatesFile::load_for_a_gate_decision(gates_path).map_err(|error| error.to_string())?;
    let quality = m7::read_report(quality_report)?;
    let fast = m7::read_report(fast_report)?;
    let verdict = m7::release::analyze_release(&quality, &fast, &seal, &gates)?;
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(&verdict).map_err(|error| error.to_string())?;
    std::fs::write(out, format!("{text}\n"))
        .map_err(|error| format!("write {}: {error}", out.display()))?;
    Ok(verdict)
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
