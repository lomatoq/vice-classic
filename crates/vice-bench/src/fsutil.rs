//! Filesystem and git helpers shared by the baseline runner and the asset
//! pin.
//!
//! This module holds no policy: it is the mechanical part of `runner.rs`
//! (path normalisation, artifact walking, disposable-tree removal, and the
//! sanitized git child processes). It exists as its own module because
//! `runner.rs` sat at 786 LOC against the §4.1 rule that a module over
//! 800 LOC must be split before merge, and the asset pin adds call sites
//! to both halves. Pure move: no behaviour is changed here.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::error::BaselineError;
use crate::hashing::sha256_file;
use crate::report::ArtifactRecord;

/// Every path that reaches a subprocess is absolute at the system boundary
/// (FAILURE_LEDGER F-0001: a relative `--out` resolved against the wrong
/// base once already).
pub fn absolute(p: &Path) -> PathBuf {
    std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf())
}

/// `path` relative to `base` with '/' separators, for report fields that
/// must not carry machine-specific absolute paths.
pub fn rel_display(path: &Path, base: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .display()
        .to_string()
        .replace('\\', "/")
}

pub fn collect_artifacts(root: &Path, declared: &BTreeSet<String>) -> Vec<ArtifactRecord> {
    let mut out = Vec::new();
    collect_walk(root, root, declared, &mut out);
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn collect_walk(
    dir: &Path,
    root: &Path,
    declared: &BTreeSet<String>,
    out: &mut Vec<ArtifactRecord>,
) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            collect_walk(&p, root, declared, out);
        } else if p.is_file() {
            let rel = rel_display(&p, root);
            let bytes = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
            let sha256 = sha256_file(&p).unwrap_or_else(|e| format!("HASH_ERROR:{e}"));
            out.push(ArtifactRecord {
                declared: declared.contains(&rel),
                path: rel,
                bytes,
                sha256,
            });
        }
    }
}

/// `remove_dir_all` that first clears read-only attributes (git object files
/// on Windows are read-only, which makes plain `remove_dir_all` fail).
pub fn force_remove_dir_all(p: &Path) -> std::io::Result<()> {
    if !p.exists() {
        return Ok(());
    }
    clear_readonly(p)?;
    std::fs::remove_dir_all(p)
}

fn clear_readonly(p: &Path) -> std::io::Result<()> {
    let meta = std::fs::symlink_metadata(p)?;
    let mut perms = meta.permissions();
    if perms.readonly() {
        // Windows-focused cleanup of read-only git pack files; the checkout
        // tree is disposable scratch state owned by this runner.
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        let _ = std::fs::set_permissions(p, perms);
    }
    if meta.is_dir() {
        for e in std::fs::read_dir(p)?.flatten() {
            clear_readonly(&e.path())?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// git children (sanitized like every other child: ADR-0007, REVIEW_M1 M1-N2)
// ---------------------------------------------------------------------------

pub fn git_output(args: &[&str], cwd: &Path) -> Result<std::process::Output, BaselineError> {
    crate::exec::sanitized_command("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| BaselineError::GitFailed {
            context: format!("git {}", args.join(" ")),
            detail: e.to_string(),
        })
}

pub fn git_ok(args: &[&str], cwd: &Path) -> Result<(), BaselineError> {
    let out = git_output(args, cwd)?;
    if !out.status.success() {
        return Err(BaselineError::GitFailed {
            context: format!("git {}", args.join(" ")),
            detail: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        });
    }
    Ok(())
}

pub fn git_stdout(args: &[&str], cwd: &Path) -> Result<String, BaselineError> {
    let out = git_output(args, cwd)?;
    if !out.status.success() {
        return Err(BaselineError::GitFailed {
            context: format!("git {}", args.join(" ")),
            detail: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rel_display_uses_forward_slashes_and_strips_base() {
        let base = Path::new("/a/b");
        assert_eq!(rel_display(&base.join("c").join("d.txt"), base), "c/d.txt");
        // A path outside the base is returned whole rather than mangled.
        assert!(rel_display(Path::new("/x/y.txt"), base).ends_with("y.txt"));
    }

    #[test]
    fn collect_artifacts_is_sorted_and_marks_declared() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("b.svg"), b"b").unwrap();
        std::fs::write(dir.path().join("a.svg"), b"a").unwrap();
        std::fs::write(dir.path().join("sub").join("c.png"), b"c").unwrap();
        let declared = BTreeSet::from(["a.svg".to_string()]);
        let got = collect_artifacts(dir.path(), &declared);
        let paths: Vec<&str> = got.iter().map(|a| a.path.as_str()).collect();
        assert_eq!(paths, vec!["a.svg", "b.svg", "sub/c.png"]);
        assert!(got[0].declared);
        assert!(!got[1].declared);
        assert_eq!(got[0].bytes, 1);
    }
}
