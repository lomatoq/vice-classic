//! Environment manifest: which OS/toolchain versions produced a report.
//! Recorded verbatim in every report; its canonical JSON is hashed so two
//! reports can be compared for environment identity.

use serde::Serialize;

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
    }
}

/// Canonical JSON (fixed struct field order) used for the environment hash.
pub fn canonical_json(m: &EnvManifest) -> String {
    serde_json::to_string(m).expect("env manifest serializes")
}
