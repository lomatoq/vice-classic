//! Child process execution with an explicit wall-clock timeout.
//!
//! stdout+stderr are appended to a log file; nothing from a baseline is
//! interpreted, only recorded. On timeout the direct child is killed.
//! Known limitation (documented in STATUS_M0): on Windows, killing the direct
//! child does not kill grandchildren (e.g. rustc under cargo).

use std::fs::File;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

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
    let mut child = Command::new(&argv[0])
        .args(&argv[1..])
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_out))
        .stderr(Stdio::from(log_err))
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
