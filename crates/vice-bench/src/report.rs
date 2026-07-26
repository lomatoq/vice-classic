//! Report structures.
//!
//! Two files are written per run:
//! - `report.json` — full record incl. runtimes, logs, absolute mirror paths.
//!   Machine-specific and time-varying by design (documented).
//! - `hashes.json` — the reproducibility artifact, split into a `normative`
//!   and an `informational` section (debt D-1; REVIEW_M0 N3, REVIEW_M1
//!   M1-N4).
//!
//! Why the split. `hashes.json` was documented as "the deterministic subset
//! for byte comparison", yet it carried `binary_sha256`, which legitimately
//! differs between two correct runs: the reviewer's rebuild of the same
//! source with the same toolchain produced a different binary hash. So the
//! documented instruction — compare the files — could never succeed, and
//! the artifact's own contract was false.
//!
//! Now the contract is executable. `normative` holds exactly what a
//! reproduction must match: config hash, corpus hashes, resolved pin SHAs,
//! staged asset hashes, statuses/error kinds, output artifact hashes and the
//! determinism verdicts. `informational` holds facts that are true of the
//! run but not required to match — today `binary_sha256`. `compare-hashes`
//! diffs the normative section and exits nonzero on any difference, so
//! "compare the files" is a command, not a request.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::assets::AssetRecord;
use crate::envinfo::EnvManifest;
use crate::error::ErrorRecord;

/// Artifact contract v3 = v1 + declared asset provenance (M0 blocker B2,
/// C059) + the normative/informational split (debt D-1, C061).
///
/// The recorded M0 artifacts under `docs/baselines/M0/` stay at v1 and are
/// historical: they are reproducible only at the M0-era commits that wrote
/// them, which was already the documented situation for `env.json` after
/// C009. Changing the tag is what keeps that statement checkable.
pub const REPORT_SCHEMA: &str = "vice-classic/baseline-report/v3";
pub const HASHES_SCHEMA: &str = "vice-classic/baseline-hashes/v3";

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
    /// Out-of-tree files staged into the checkout, each verified against its
    /// declared sha256 and length before the copy (see `assets`).
    pub assets: Vec<AssetRecord>,
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
    /// Everything a reproduction is REQUIRED to match, byte for byte.
    pub normative: NormativeHashes,
    /// True of this run, not required to match. Kept in the artifact so it
    /// stays auditable; excluded from the comparison so the comparison can
    /// actually be performed.
    pub informational: InformationalHashes,
}

#[derive(Debug, Serialize)]
pub struct NormativeHashes {
    pub config_sha256: String,
    pub environment_sha256: String,
    pub corpus: BTreeMap<String, String>,
    pub baselines: BTreeMap<String, BaselineHashes>,
}

#[derive(Debug, Serialize)]
pub struct InformationalHashes {
    /// Baseline name -> built binary sha256. Toolchain- and
    /// checkout-path-dependent: REVIEW_M0 probe P1 rebuilt the same source
    /// with the same toolchain at the same path and got a different hash
    /// while the produced SVGs matched byte for byte.
    pub binary_sha256: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
pub struct BaselineHashes {
    pub status: String,
    pub error_kind: Option<String>,
    pub resolved_sha: Option<String>,
    /// Declared out-of-tree asset path -> sha256 actually staged. Normative:
    /// the asset is pinned exactly as strongly as the commit, so two runs
    /// that agree here used the same donor state.
    pub assets: BTreeMap<String, String>,
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
    let mut binary_sha256 = BTreeMap::new();
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
        if let Some(h) = b.build.as_ref().and_then(|x| x.binary_sha256.clone()) {
            binary_sha256.insert(b.name.clone(), h);
        }
        baselines.insert(
            b.name.clone(),
            BaselineHashes {
                status: b.status.clone(),
                error_kind: b.error.as_ref().map(|e| e.kind.clone()),
                resolved_sha: b.resolved_sha.clone(),
                assets: b
                    .assets
                    .iter()
                    .map(|a| (a.path.clone(), a.sha256.clone()))
                    .collect(),
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
        normative: NormativeHashes {
            config_sha256: report.config_sha256.clone(),
            environment_sha256: report.environment_sha256.clone(),
            corpus,
            baselines,
        },
        informational: InformationalHashes { binary_sha256 },
    }
}

/// Compare the NORMATIVE sections of two `hashes.json` documents.
///
/// Returns the list of differing JSON pointers (empty = reproduced). Works
/// on parsed JSON rather than on the structs so it can be pointed at a
/// recorded artifact written by an older binary — which is exactly the
/// situation a clean-checkout reproduction is in.
pub fn compare_normative(a: &serde_json::Value, b: &serde_json::Value) -> Vec<String> {
    let mut diffs = Vec::new();
    diff_json("/schema", a.get("schema"), b.get("schema"), &mut diffs);
    diff_json(
        "/normative",
        a.get("normative"),
        b.get("normative"),
        &mut diffs,
    );
    diffs.sort();
    diffs
}

fn diff_json(
    path: &str,
    a: Option<&serde_json::Value>,
    b: Option<&serde_json::Value>,
    out: &mut Vec<String>,
) {
    match (a, b) {
        (None, None) => {}
        (Some(x), Some(y)) if x == y => {}
        (Some(serde_json::Value::Object(x)), Some(serde_json::Value::Object(y))) => {
            let mut keys: Vec<&String> = x.keys().chain(y.keys()).collect();
            keys.sort();
            keys.dedup();
            for k in keys {
                diff_json(&format!("{path}/{k}"), x.get(k), y.get(k), out);
            }
        }
        (x, y) => out.push(format!("{path}: {} != {}", brief(x), brief(y))),
    }
}

/// Render one side of a difference. Long values are elided: a whole-subtree
/// difference (a baseline present on one side only) would otherwise print
/// tens of kilobytes of JSON and bury the pointer, which is the part the
/// reader needs.
fn brief(v: Option<&serde_json::Value>) -> String {
    const MAX: usize = 120;
    let Some(v) = v else {
        return "<absent>".to_string();
    };
    let s = v.to_string();
    if s.chars().count() <= MAX {
        return s;
    }
    let head: String = s.chars().take(MAX).collect();
    format!("{head}... ({} chars)", s.chars().count())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(binary: &str, artifact: &str) -> serde_json::Value {
        serde_json::json!({
            "schema": HASHES_SCHEMA,
            "normative": {
                "config_sha256": "c0",
                "environment_sha256": "e0",
                "corpus": { "a.png": "aa" },
                "baselines": { "d": { "status": "completed", "artifacts": { "a.png": { "a.svg": artifact } } } }
            },
            "informational": { "binary_sha256": { "d": binary } }
        })
    }

    #[test]
    fn comparison_ignores_the_informational_section_and_only_that() {
        // The exact situation REVIEW_M0 hit: same results, different binary.
        assert!(compare_normative(&doc("b1", "s1"), &doc("b2", "s1")).is_empty());

        // Specificity: a real difference is reported, with its location.
        let diffs = compare_normative(&doc("b1", "s1"), &doc("b1", "s2"));
        assert_eq!(diffs.len(), 1, "{diffs:?}");
        assert!(diffs[0].starts_with("/normative/baselines/d/artifacts/a.png/a.svg"));

        // A missing key is a difference, not a silent pass.
        let mut short = doc("b1", "s1");
        short["normative"]["baselines"]["d"]
            .as_object_mut()
            .unwrap()
            .remove("status");
        let diffs = compare_normative(&doc("b1", "s1"), &short);
        assert_eq!(diffs.len(), 1);
        assert!(diffs[0].contains("<absent>"));

        // And a schema change is a difference: comparing across artifact
        // contracts must not look like a successful reproduction.
        let mut other = doc("b1", "s1");
        other["schema"] = serde_json::json!("vice-classic/baseline-hashes/v1");
        assert!(!compare_normative(&doc("b1", "s1"), &other).is_empty());
    }
}
