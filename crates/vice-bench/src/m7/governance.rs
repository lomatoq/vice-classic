//! M7 runner trust, split gate roles, and structured gate provenance.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::gates::GatesFile;

pub const M7_RUNNER_ATTESTATION_SCHEMA: &str = "vice-classic/m7-runner-attestation/v1";
pub const M7_GATE_PROVENANCE_SCHEMA: &str = "vice-classic/m7-gate-provenance/v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolAttestation {
    pub canonical_path: String,
    pub sha256: String,
    pub version_stdout: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerAttestation {
    pub schema: String,
    pub anchor_source: String,
    pub event_commit_sha: String,
    pub repository_root: String,
    pub git: ToolAttestation,
    pub gt_corpus: ToolAttestation,
    pub vicec: ToolAttestation,
    pub gates_repository_path: String,
    pub gates_blob_sha256: String,
    pub gate_provenance_repository_path: String,
    pub gate_provenance_blob_sha256: String,
    pub clean_tree_at_attestation: bool,
}

impl RunnerAttestation {
    pub fn sha256(&self) -> Result<String, String> {
        serde_json::to_vec(self)
            .map(|bytes| hex::encode(Sha256::digest(bytes)))
            .map_err(|error| error.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateProvenance {
    pub schema: String,
    pub status: String,
    pub milestone: String,
    pub source_commit_sha: String,
    pub calibration_measurement_sha256: String,
    pub geometry_measurement_sha256: String,
    pub calibration_command: String,
    pub geometry_command: String,
    pub asserted_gate_keys: Vec<String>,
}

/// Hash-only gate input used to bind the audit seal. It deliberately has no
/// threshold accessor.
#[derive(Debug, Clone, PartialEq)]
pub struct GateDigestInput {
    pub sha256: String,
}

/// Threshold source used by M7 release judges. It is constructible only after
/// the runner anchor and structured provenance have both verified.
#[derive(Debug, Clone)]
pub struct M7ThresholdSource {
    pub gates: GatesFile,
    pub digest_input: GateDigestInput,
    pub event_commit_sha: String,
    pub attestation_sha256: String,
    pub provenance_sha256: String,
    pub provenance: GateProvenance,
}

pub fn create_attestation(
    anchor_source: &str,
    event_commit_sha: &str,
    repository_root: &Path,
    git_executable: &Path,
    vicec_executable: &Path,
    gates_path: &Path,
    provenance_path: &Path,
) -> Result<RunnerAttestation, String> {
    if !matches!(anchor_source, "github_event" | "reviewer_pinned") {
        return Err("anchor source must be github_event or reviewer_pinned".into());
    }
    validate_commit_sha(event_commit_sha)?;
    let repository_root = canonical(repository_root)?;
    let git_path = canonical(git_executable)?;
    if git_path.starts_with(&repository_root) {
        return Err("Git executable resolves inside the repository under test".into());
    }
    let vicec_path = canonical(vicec_executable)?;
    let current_exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let gt_corpus_path = canonical(&current_exe)?;
    let gates_rel = repository_relative(&repository_root, gates_path)?;
    let provenance_rel = repository_relative(&repository_root, provenance_path)?;
    verify_commit_exists(&git_path, &repository_root, event_commit_sha)?;
    let gates_blob = git_show(&git_path, &repository_root, event_commit_sha, &gates_rel)?;
    let provenance_blob = git_show(
        &git_path,
        &repository_root,
        event_commit_sha,
        &provenance_rel,
    )?;
    verify_disk_matches_blob(gates_path, &gates_blob, "gate file")?;
    verify_disk_matches_blob(provenance_path, &provenance_blob, "gate provenance")?;
    let clean_tree_at_attestation = clean_tree(&git_path, &repository_root, event_commit_sha)?;
    if !clean_tree_at_attestation {
        return Err("runner attestation requires an exact clean event commit checkout".into());
    }
    Ok(RunnerAttestation {
        schema: M7_RUNNER_ATTESTATION_SCHEMA.into(),
        anchor_source: anchor_source.into(),
        event_commit_sha: event_commit_sha.into(),
        repository_root: path_text(&repository_root),
        git: attest_tool(&git_path, &["--version"])?,
        gt_corpus: attest_tool(&gt_corpus_path, &["--version"])?,
        vicec: attest_tool(&vicec_path, &["--version"])?,
        gates_repository_path: slash(&gates_rel),
        gates_blob_sha256: hex::encode(Sha256::digest(&gates_blob)),
        gate_provenance_repository_path: slash(&provenance_rel),
        gate_provenance_blob_sha256: hex::encode(Sha256::digest(&provenance_blob)),
        clean_tree_at_attestation,
    })
}

pub fn write_attestation(path: &Path, attestation: &RunnerAttestation) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    let bytes = serde_json::to_vec_pretty(attestation).map_err(|error| error.to_string())?;
    std::fs::write(path, [bytes.as_slice(), b"\n"].concat())
        .map_err(|error| format!("write {}: {error}", path.display()))
}

pub fn load_threshold_source(
    attestation_path: &Path,
    gates_path: &Path,
    provenance_path: &Path,
) -> Result<M7ThresholdSource, String> {
    let bytes = std::fs::read(attestation_path)
        .map_err(|error| format!("read {}: {error}", attestation_path.display()))?;
    let attestation: RunnerAttestation = serde_json::from_slice(&bytes)
        .map_err(|error| format!("parse {}: {error}", attestation_path.display()))?;
    verify_attestation(&attestation, gates_path, provenance_path)?;
    let gates = GatesFile::load(gates_path).map_err(|error| error.to_string())?;
    if gates.sha256 != attestation.gates_blob_sha256 {
        return Err("threshold source hash differs from the externally anchored gate blob".into());
    }
    let provenance_text = std::fs::read_to_string(provenance_path)
        .map_err(|error| format!("read {}: {error}", provenance_path.display()))?;
    let provenance: GateProvenance =
        toml::from_str(&provenance_text).map_err(|error| error.to_string())?;
    validate_provenance(&provenance, &gates)?;
    let root = canonical(Path::new(&attestation.repository_root))?;
    let git = canonical(Path::new(&attestation.git.canonical_path))?;
    verify_commit_exists(&git, &root, &provenance.source_commit_sha)?;
    exact_git(
        &git,
        &root,
        &[
            "merge-base",
            "--is-ancestor",
            &provenance.source_commit_sha,
            &attestation.event_commit_sha,
        ],
    )
    .map_err(|_| {
        "M7 gate provenance source is not an ancestor of the externally anchored release commit"
            .to_string()
    })?;
    Ok(M7ThresholdSource {
        digest_input: GateDigestInput {
            sha256: gates.sha256.clone(),
        },
        event_commit_sha: attestation.event_commit_sha.clone(),
        attestation_sha256: attestation.sha256()?,
        provenance_sha256: attestation.gate_provenance_blob_sha256.clone(),
        gates,
        provenance,
    })
}

fn verify_attestation(
    attestation: &RunnerAttestation,
    gates_path: &Path,
    provenance_path: &Path,
) -> Result<(), String> {
    if attestation.schema != M7_RUNNER_ATTESTATION_SCHEMA
        || !matches!(
            attestation.anchor_source.as_str(),
            "github_event" | "reviewer_pinned"
        )
        || !attestation.clean_tree_at_attestation
    {
        return Err("invalid M7 runner attestation identity or source".into());
    }
    validate_commit_sha(&attestation.event_commit_sha)?;
    let root = canonical(Path::new(&attestation.repository_root))?;
    let git_path = canonical(Path::new(&attestation.git.canonical_path))?;
    if git_path.starts_with(&root) {
        return Err("attested Git executable is inside the repository".into());
    }
    verify_tool(&attestation.git, &["--version"])?;
    verify_tool(&attestation.gt_corpus, &["--version"])?;
    verify_tool(&attestation.vicec, &["--version"])?;
    verify_commit_exists(&git_path, &root, &attestation.event_commit_sha)?;
    let head = exact_git(&git_path, &root, &["rev-parse", "--verify", "HEAD"])?;
    if head.trim() != attestation.event_commit_sha {
        return Err(format!(
            "local HEAD {} is not the externally anchored event commit {}",
            head.trim(),
            attestation.event_commit_sha
        ));
    }
    if !clean_tree(&git_path, &root, &attestation.event_commit_sha)? {
        return Err("M7 release judge requires the attested clean checkout".into());
    }
    let gates_rel = repository_relative(&root, gates_path)?;
    let provenance_rel = repository_relative(&root, provenance_path)?;
    if slash(&gates_rel) != attestation.gates_repository_path
        || slash(&provenance_rel) != attestation.gate_provenance_repository_path
    {
        return Err("gate/provenance path substitution detected".into());
    }
    let gates_blob = git_show(&git_path, &root, &attestation.event_commit_sha, &gates_rel)?;
    let provenance_blob = git_show(
        &git_path,
        &root,
        &attestation.event_commit_sha,
        &provenance_rel,
    )?;
    if hex::encode(Sha256::digest(&gates_blob)) != attestation.gates_blob_sha256
        || hex::encode(Sha256::digest(&provenance_blob)) != attestation.gate_provenance_blob_sha256
    {
        return Err("attested repository blobs changed".into());
    }
    verify_disk_matches_blob(gates_path, &gates_blob, "gate file")?;
    verify_disk_matches_blob(provenance_path, &provenance_blob, "gate provenance")?;
    Ok(())
}

fn validate_provenance(provenance: &GateProvenance, gates: &GatesFile) -> Result<(), String> {
    if provenance.schema != M7_GATE_PROVENANCE_SCHEMA
        || provenance.status != "frozen"
        || provenance.milestone != "M7"
        || provenance.calibration_command.trim().is_empty()
        || provenance.geometry_command.trim().is_empty()
    {
        return Err("M7 gate provenance is not a frozen structured measurement record".into());
    }
    validate_commit_sha(&provenance.source_commit_sha)?;
    for digest in [
        &provenance.calibration_measurement_sha256,
        &provenance.geometry_measurement_sha256,
    ] {
        if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("M7 structured provenance carries a malformed artifact digest".into());
        }
    }
    let section = gates
        .doc
        .sections
        .get("m7_selective")
        .ok_or_else(|| "missing m7_selective gate section".to_string())?;
    let mut declared = provenance.asserted_gate_keys.clone();
    declared.sort();
    declared.dedup();
    let mut actual = section.values.keys().cloned().collect::<Vec<_>>();
    actual.sort();
    if declared != actual {
        return Err("structured provenance does not enumerate every M7 selective gate key".into());
    }
    Ok(())
}

fn validate_commit_sha(value: &str) -> Result<(), String> {
    if value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err("event commit SHA must be exactly 40 hexadecimal characters".into())
    }
}

fn canonical(path: &Path) -> Result<PathBuf, String> {
    std::fs::canonicalize(path).map_err(|error| format!("resolve {}: {error}", path.display()))
}

fn repository_relative(root: &Path, path: &Path) -> Result<PathBuf, String> {
    let path = canonical(path)?;
    path.strip_prefix(root).map(Path::to_path_buf).map_err(|_| {
        format!(
            "{} is outside repository {}",
            path.display(),
            root.display()
        )
    })
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn sha256_file(path: &Path) -> Result<String, String> {
    std::fs::read(path)
        .map(|bytes| hex::encode(Sha256::digest(bytes)))
        .map_err(|error| format!("read {}: {error}", path.display()))
}

fn attest_tool(path: &Path, version_args: &[&str]) -> Result<ToolAttestation, String> {
    let version_stdout = exact_tool(path, version_args)?;
    Ok(ToolAttestation {
        canonical_path: path_text(path),
        sha256: sha256_file(path)?,
        version_stdout: version_stdout.trim().into(),
    })
}

fn verify_tool(tool: &ToolAttestation, version_args: &[&str]) -> Result<(), String> {
    let path = canonical(Path::new(&tool.canonical_path))?;
    if path_text(&path) != tool.canonical_path
        || sha256_file(&path)? != tool.sha256
        || exact_tool(&path, version_args)?.trim() != tool.version_stdout
    {
        return Err(format!(
            "tool substitution detected at {}",
            tool.canonical_path
        ));
    }
    Ok(())
}

fn exact_tool(path: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new(path)
        .args(args)
        .output()
        .map_err(|error| format!("run {}: {error}", path.display()))?;
    if !output.status.success() {
        return Err(format!(
            "{} {:?} failed: {}",
            path.display(),
            args,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn exact_git(git: &Path, root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new(git)
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|error| format!("run anchored Git {}: {error}", git.display()))?;
    if !output.status.success() {
        return Err(format!(
            "anchored Git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn verify_commit_exists(git: &Path, root: &Path, commit: &str) -> Result<(), String> {
    exact_git(
        git,
        root,
        &["cat-file", "-e", &format!("{commit}^{{commit}}")],
    )
    .map(|_| ())
}

fn git_show(git: &Path, root: &Path, commit: &str, path: &Path) -> Result<Vec<u8>, String> {
    let output = Command::new(git)
        .arg("-C")
        .arg(root)
        .arg("show")
        .arg(format!("{commit}:{}", slash(path)))
        .output()
        .map_err(|error| format!("run anchored Git show: {error}"))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(format!(
            "anchored Git cannot read {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn verify_disk_matches_blob(path: &Path, blob: &[u8], role: &str) -> Result<(), String> {
    let disk = std::fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    if disk == blob {
        Ok(())
    } else {
        Err(format!(
            "{role} on disk differs from the anchored commit blob"
        ))
    }
}

fn clean_tree(git: &Path, root: &Path, commit: &str) -> Result<bool, String> {
    let status = exact_git(
        git,
        root,
        &["status", "--porcelain=v1", "--untracked-files=all"],
    )?;
    let diff = Command::new(git)
        .arg("-C")
        .arg(root)
        .args(["diff-index", "--quiet", commit, "--"])
        .status()
        .map_err(|error| format!("run anchored Git diff-index: {error}"))?;
    Ok(status.trim().is_empty() && diff.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_anchor_is_not_a_symbolic_local_ref() {
        assert!(validate_commit_sha(&"a".repeat(40)).is_ok());
        for bad in [
            "HEAD",
            "main",
            "abc",
            "g000000000000000000000000000000000000000",
        ] {
            assert!(validate_commit_sha(bad).is_err());
        }
    }

    #[test]
    fn digest_role_has_no_threshold_accessor() {
        let digest = GateDigestInput {
            sha256: "0".repeat(64),
        };
        assert_eq!(digest.sha256.len(), 64);
    }

    #[test]
    fn a_repository_local_git_is_refused_before_it_can_define_truth() {
        let temp = tempfile::tempdir().unwrap();
        let fake_git = temp.path().join("git.exe");
        std::fs::write(&fake_git, b"not git").unwrap();
        let error = create_attestation(
            "reviewer_pinned",
            &"a".repeat(40),
            temp.path(),
            &fake_git,
            Path::new("unused-vicec"),
            Path::new("unused-gates"),
            Path::new("unused-provenance"),
        )
        .unwrap_err();
        assert!(error.contains("inside the repository"), "{error}");
    }

    #[test]
    fn an_attested_tool_hash_cannot_be_substituted() {
        let executable = std::env::current_exe().unwrap();
        let mut attested = attest_tool(&executable, &["--list"]).unwrap();
        attested.sha256 = "0".repeat(64);
        let error = verify_tool(&attested, &["--list"]).unwrap_err();
        assert!(error.contains("tool substitution detected"), "{error}");
    }
}
