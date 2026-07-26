//! Child process execution with an explicit wall-clock timeout and a FIXED
//! child environment policy.
//!
//! stdout+stderr are appended to a log file; nothing from a baseline is
//! interpreted, only recorded. On timeout the direct child is killed.
//! Known limitation (documented in STATUS_M0): on Windows, killing the direct
//! child does not kill grandchildren (e.g. rustc under cargo).
//!
//! Environment policy (REVIEW_M0 condition 5 / N6, ADR-0007): ambient
//! variables that can silently change a donor build or run are REMOVED for
//! every child, and PYTHONHASHSEED is pinned to 0 for determinism of python
//! donors. The policy is a fixed constant — not configurable, so it cannot
//! become a hidden behaviour knob (spec §32 rule 4) — and it is recorded in
//! `env.json` (`envinfo::child_env_policy`).

use std::fs::File;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Ambient variables removed from every child environment. Each of these
/// can alter a donor build/run result without leaving a trace in the
/// report; the recorded M0 baselines were produced with none of them set.
pub const CHILD_ENV_REMOVE: &[&str] = &[
    "RUSTFLAGS",
    "RUSTDOCFLAGS",
    "RUSTC_WRAPPER",
    "RUSTC_WORKSPACE_WRAPPER",
    "RUSTC_BOOTSTRAP",
    "CARGO_ENCODED_RUSTFLAGS",
    "CARGO_BUILD_RUSTFLAGS",
    "CARGO_BUILD_RUSTDOCFLAGS",
    "CARGO_INCREMENTAL",
    "CARGO_TARGET_DIR",
    "CARGO_BUILD_TARGET_DIR",
];

/// Variables pinned to fixed values in every child environment.
/// PYTHONHASHSEED=0 removes per-process hash randomization from python
/// donors (set/dict iteration order) — empirically the M0 corpus outputs
/// did not depend on it (repeats with random seeds matched byte-for-byte),
/// so pinning cannot change the recorded artifacts.
pub const CHILD_ENV_SET: &[(&str, &str)] = &[("PYTHONHASHSEED", "0")];

/// Apply the fixed child-environment policy to a command.
pub fn apply_child_env_policy(cmd: &mut Command) {
    for name in CHILD_ENV_REMOVE {
        cmd.env_remove(name);
    }
    for (name, value) in CHILD_ENV_SET {
        cmd.env(name, value);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecOutcome {
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub duration_ms: u64,
}

/// Run `argv` (argv[0] = program) in `cwd`, appending combined output to
/// `log_path`. Returns Err only when the process could not be spawned or the
/// log file could not be created.
pub fn run_with_timeout(
    argv: &[String],
    cwd: &Path,
    timeout: Duration,
    log_path: &Path,
) -> Result<ExecOutcome, String> {
    assert!(!argv.is_empty(), "empty command");
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create log dir: {e}"))?;
    }
    let log_out = File::create(log_path).map_err(|e| format!("create log {log_path:?}: {e}"))?;
    let log_err = log_out
        .try_clone()
        .map_err(|e| format!("clone log handle: {e}"))?;

    let start = Instant::now();
    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..])
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_out))
        .stderr(Stdio::from(log_err));
    apply_child_env_policy(&mut cmd);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("spawn {:?}: {e}", argv[0]))?;

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return Ok(ExecOutcome {
                    exit_code: status.code(),
                    timed_out: false,
                    duration_ms: start.elapsed().as_millis() as u64,
                });
            }
            Ok(None) => {}
            Err(e) => return Err(format!("wait: {e}")),
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(ExecOutcome {
                exit_code: None,
                timed_out: true,
                duration_ms: start.elapsed().as_millis() as u64,
            });
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}
