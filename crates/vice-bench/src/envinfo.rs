//! Environment manifest: which OS/toolchain versions produced a report,
//! plus the child-process environment policy actually in force.
//! Recorded verbatim in every report; its canonical JSON is hashed so two
//! reports can be compared for environment identity.
//!
//! Schema note: `command_env` was added in M1 (REVIEW_M0 condition 5 / N6).
//! The recorded M0 artifacts under docs/baselines/M0 keep the older shape;
//! byte-comparing env.json against them is only meaningful at M0 commits
//! (see docs/REPRODUCIBILITY_M0.md).

use std::collections::BTreeMap;

use serde::Serialize;

use crate::exec::{CHILD_ENV_REMOVE, CHILD_ENV_SET};

pub const CHILD_ENV_POLICY: &str = "vice-classic/child-env/v1";

#[derive(Debug, Clone, Serialize)]
pub struct EnvManifest {
    pub os: String,
    pub arch: String,
    pub family: String,
    pub logical_cpus: usize,
    pub rustc: Option<String>,
    pub cargo: Option<String>,
    pub git: Option<String>,
    pub python: Option<String>,
    pub command_env: ChildEnvRecord,
}

/// The relevant slice of the child-process environment (REVIEW_M0 N6):
/// which influence-carrying variables are removed, which are pinned, and
/// which of them were PRESENT in the ambient environment of this run.
/// Ambient VALUES are deliberately not recorded: after sanitation they
/// cannot influence children, and presence is the audit-relevant fact.
#[derive(Debug, Clone, Serialize)]
pub struct ChildEnvRecord {
    /// Fixed policy identifier (constants live in `exec.rs`).
    pub policy: String,
    /// Variables removed from every child environment.
    pub removed: Vec<String>,
    /// Variables pinned to fixed values in every child environment.
    pub set: BTreeMap<String, String>,
    /// Names from `removed`/`set` that WERE present in the ambient
    /// environment when this manifest was collected (values omitted).
    pub ambient_overrides_present: Vec<String>,
}

/// Collect the child-environment policy record. Reading variable PRESENCE
/// here is recording, not behaviour control: the policy itself is a fixed
/// constant (spec §32 rule 4).
pub fn child_env_policy() -> ChildEnvRecord {
    let mut ambient = Vec::new();
    for name in CHILD_ENV_REMOVE {
        if std::env::var_os(name).is_some() {
            ambient.push((*name).to_string());
        }
    }
    for (name, _) in CHILD_ENV_SET {
        if std::env::var_os(name).is_some() {
            ambient.push((*name).to_string());
        }
    }
    ambient.sort();
    ChildEnvRecord {
        policy: CHILD_ENV_POLICY.to_string(),
        removed: CHILD_ENV_REMOVE.iter().map(|s| s.to_string()).collect(),
        set: CHILD_ENV_SET
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        ambient_overrides_present: ambient,
    }
}

fn tool_version(cmd: &str, arg: &str) -> Option<String> {
    let out = std::process::Command::new(cmd).arg(arg).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let mut s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        s = String::from_utf8_lossy(&out.stderr).trim().to_string();
    }
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

pub fn collect() -> EnvManifest {
    EnvManifest {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        family: std::env::consts::FAMILY.to_string(),
        logical_cpus: std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(0),
        rustc: tool_version("rustc", "-V"),
        cargo: tool_version("cargo", "-V"),
        git: tool_version("git", "--version"),
        python: tool_version("python", "--version"),
        command_env: child_env_policy(),
    }
}

/// Canonical JSON (fixed struct field order) used for the environment hash.
pub fn canonical_json(m: &EnvManifest) -> String {
    serde_json::to_string(m).expect("env manifest serializes")
}
