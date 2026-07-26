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
//!
//! Coverage (REVIEW_M1 M1-N2): the policy applies to EVERY child this crate
//! spawns — baseline build/run children (`run_with_timeout`), git
//! provenance children (`runner::git_output` and the pin probe) and
//! tool-version probes (`envinfo::tool_version`). All of them construct
//! their `Command` via [`sanitized_command`].

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

/// The ONLY constructor for child commands in this crate: a `Command` with
/// the fixed environment policy already applied. Every child — baseline
/// build/run children, git provenance children and tool-version probes —
/// must be created through this function so the ADR-0007 claim "the policy
/// is applied to every child" is enforced by construction, not by
/// convention (REVIEW_M1 M1-N2).
pub fn sanitized_command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    apply_child_env_policy(&mut cmd);
    cmd
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
    let mut cmd = sanitized_command(&argv[0]);
    cmd.args(&argv[1..])
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_out))
        .stderr(Stdio::from(log_err));
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    /// Every removed variable appears as an explicit None override and every
    /// pinned variable as its fixed value on a sanitized command, so the
    /// policy holds for ANY spawn site using [`sanitized_command`] — the
    /// spawn-level behaviour is additionally covered by tests/child_env.rs.
    #[test]
    fn sanitized_command_carries_the_full_policy() {
        let cmd = sanitized_command("git");
        let envs: Vec<(&OsStr, Option<&OsStr>)> = cmd.get_envs().collect();
        for name in CHILD_ENV_REMOVE {
            assert!(
                envs.iter()
                    .any(|(k, v)| *k == OsStr::new(name) && v.is_none()),
                "{name} must be removed"
            );
        }
        for (name, value) in CHILD_ENV_SET {
            assert!(
                envs.iter()
                    .any(|(k, v)| *k == OsStr::new(name) && *v == Some(OsStr::new(value))),
                "{name} must be pinned to {value}"
            );
        }
        assert_eq!(cmd.get_program(), OsStr::new("git"));
    }
}
