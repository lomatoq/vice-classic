//! Report structures.
//!
//! Two files are written per run:
//! - `report.json` — full record incl. runtimes, logs, absolute mirror paths.
//!   Machine-specific and time-varying by design (documented).
//! - `hashes.json` — the deterministic subset used for reproducibility
//!   comparison: only content hashes, pin SHAs and stable statuses, in
//!   BTreeMap (sorted) order. Two runs of the same binary on the same
//!   machine over the same corpus must produce byte-identical `hashes.json`
//!   unless a baseline itself is nondeterministic — which is then visible
//!   in the `*_deterministic` fields and the report mismatches.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::envinfo::EnvManifest;
use crate::error::ErrorRecord;

pub const REPORT_SCHEMA: &str = "vice-classic/m0-baseline-report/v1";
pub const HASHES_SCHEMA: &str = "vice-classic/m0-baseline-hashes/v1";

#[derive(Debug, Clone, Serialize)]
pub struct ToolInfo {
    pub name: String,
    pub version: String,
    /// SHA-256 of the runner executable itself.
    pub exe_sha256: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RunReport {
    pub schema: String,
    pub tool: ToolInfo,
    pub config: String,
    pub config_sha256: String,
    pub environment: EnvManifest,
    pub environment_sha256: String,
    pub repeats: u32,
    pub corpus: Vec<CorpusEntry>,
    pub baselines: Vec<BaselineReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CorpusEntry {
    pub name: String,
    pub sha256: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Serialize)]
pub struct BaselineReport {
    pub name: String,
    pub repo: String,
    pub pin_sha: String,
    pub mirror: String,
    /// "completed" (prepare/build worked; individual runs may still have
    /// failed and carry their own typed errors) or "failed" (typed error in
    /// `error`). A failed baseline never aborts the other baselines.
    pub status: String,
    pub error: Option<ErrorRecord>,
    pub resolved_sha: Option<String>,
    pub build: Option<BuildRecord>,
    pub notes: String,
    pub runs: Vec<RunRecord>,
    pub determinism: Option<DeterminismRecord>,
}

#[derive(Debug, Serialize)]
pub struct BuildRecord {
    pub command: Vec<String>,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub duration_ms: u64,
    pub binary_path: Option<String>,
    pub binary_sha256: Option<String>,
    pub log: String,
}

#[derive(Debug, Serialize)]
pub struct RunRecord {
    pub input: String,
    pub repeat: u32,
    pub command: Vec<String>,
    pub status: String, // "ok" | "failed"
    pub error: Option<ErrorRecord>,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub duration_ms: u64,
    pub artifacts: Vec<ArtifactRecord>,
    pub log: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArtifactRecord {
    /// Path relative to this run's out dir, '/'-separated.
    pub path: String,
    /// True if listed in the baseline's declared `outputs`.
    pub declared: bool,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Serialize)]
pub struct DeterminismRecord {
    pub repeats: u32,
    /// All declared outputs byte-identical across repeats. None when there
    /// were no declared artifacts to compare (or repeats < 2).
    pub primary_deterministic: Option<bool>,
    /// Every file the baseline wrote is byte-identical across repeats.
    pub all_artifacts_deterministic: Option<bool>,
    pub mismatches: Vec<DeterminismMismatch>,
}

#[derive(Debug, Serialize)]
pub struct DeterminismMismatch {
    pub input: String,
    pub artifact: String,
    pub declared: bool,
    /// One entry per repeat; "MISSING" when the artifact was absent.
    pub sha256_by_repeat: Vec<String>,
}

// ---------------------------------------------------------------------------
// hashes.json
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct HashesFile {
    pub schema: String,
    pub config_sha256: String,
    pub corpus: BTreeMap<String, String>,
    pub baselines: BTreeMap<String, BaselineHashes>,
}

#[derive(Debug, Serialize)]
pub struct BaselineHashes {
    pub status: String,
    pub error_kind: Option<String>,
    pub resolved_sha: Option<String>,
    /// Toolchain- and checkout-path-dependent; see STATUS_M0 "known
    /// nondeterminism" before comparing across machines.
    pub binary_sha256: Option<String>,
    /// input file -> artifact rel path -> sha256, from repeat 0.
    pub artifacts: BTreeMap<String, BTreeMap<String, String>>,
    pub primary_deterministic: Option<bool>,
    pub all_artifacts_deterministic: Option<bool>,
}

pub fn hashes_from_report(report: &RunReport) -> HashesFile {
    let corpus = report
        .corpus
        .iter()
        .map(|c| (c.name.clone(), c.sha256.clone()))
        .collect::<BTreeMap<_, _>>();

    let mut baselines = BTreeMap::new();
    for b in &report.baselines {
        let mut artifacts: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        for r in &b.runs {
            if r.repeat != 0 {
                continue;
            }
            let entry = artifacts.entry(r.input.clone()).or_default();
            for a in &r.artifacts {
                entry.insert(a.path.clone(), a.sha256.clone());
            }
        }
        baselines.insert(
            b.name.clone(),
            BaselineHashes {
                status: b.status.clone(),
                error_kind: b.error.as_ref().map(|e| e.kind.clone()),
                resolved_sha: b.resolved_sha.clone(),
                binary_sha256: b.build.as_ref().and_then(|x| x.binary_sha256.clone()),
                artifacts,
                primary_deterministic: b.determinism.as_ref().and_then(|d| d.primary_deterministic),
                all_artifacts_deterministic: b
                    .determinism
                    .as_ref()
                    .and_then(|d| d.all_artifacts_deterministic),
            },
        );
    }

    HashesFile {
        schema: HASHES_SCHEMA.to_string(),
        config_sha256: report.config_sha256.clone(),
        corpus,
        baselines,
    }
}
