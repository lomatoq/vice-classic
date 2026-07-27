//! Hygiene rules of §4.1 and §32, as checks over the CLASS rather than as
//! claims in a status report.
//!
//! REVIEW_M3_5 M35-N2 found a STATUS sentence that was false as a
//! measurement: "no production module over 800 LOC; the maximum is 789",
//! while `oracle/report.rs` was 874. The fix that matters is not editing the
//! sentence — it is that the rule now walks every production module, so the
//! next module to cross the line fails a test instead of aging quietly in a
//! document nobody re-measures (meta-rule M-1).

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn production_modules() -> Vec<(PathBuf, usize)> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
        paths.sort();
        for p in paths {
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|e| e == "rs") {
                out.push(p);
            }
        }
    }
    let mut crates: Vec<PathBuf> = std::fs::read_dir(repo_root().join("crates"))
        .expect("crates dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    crates.sort();
    let mut files = Vec::new();
    for c in crates {
        walk(&c.join("src"), &mut files);
    }
    files
        .into_iter()
        .map(|p| {
            let n = std::fs::read_to_string(&p).expect("read").lines().count();
            (p, n)
        })
        .collect()
}

/// §4.1: "a module over 800-1000 LOC must be split before merge". Applied to
/// the CLASS of production modules, with the same metric (`wc -l`) the
/// review used.
#[test]
fn no_production_module_is_over_the_size_rule() {
    let modules = production_modules();
    assert!(
        modules.len() > 30,
        "the walk found only {} modules; it is not covering the workspace",
        modules.len()
    );
    let over: Vec<String> = modules
        .iter()
        .filter(|(_, n)| *n > 800)
        .map(|(p, n)| format!("{} = {n} lines", p.display()))
        .collect();
    let largest = modules.iter().map(|(_, n)| *n).max().unwrap_or(0);
    println!(
        "{} production modules, largest {largest} lines",
        modules.len()
    );
    assert!(
        over.is_empty(),
        "production modules over the 800-line rule of spec 4.1: {over:?}"
    );
}

/// §32 rule 4: no hidden environment switches.
///
/// "No module reads an environment variable" is FALSE and would be the wrong
/// rule: the M0 baseline runner reads them on purpose, to RECORD the
/// environment in `env.json` and to sanitize the environment of child
/// processes (ADR-0007). What the rule is about is behaviour switches, and
/// what is mechanically checkable is the SURFACE: the set of modules that
/// touch `std::env::var` is frozen here, so a fourth one is a test failure a
/// reviewer has to look at rather than a flag nobody notices.
///
/// `std::env::consts` is a compile-time constant and is not an environment
/// variable — the distinction REVIEW_M3_5 M35-N8 asked to be stated rather
/// than blurred.
#[test]
fn only_the_recorded_modules_read_an_environment_variable() {
    const DECLARED: &[&str] = &[
        // Records the environment into the report (M0).
        "vice-bench/src/envinfo.rs",
        // Sanitizes the environment of every child process (ADR-0007).
        "vice-bench/src/runner.rs",
        // Resolves the baseline mirrors from documented variables (M0).
        "vice-bench/src/bin/baseline-runner.rs",
    ];
    let mut found: Vec<String> = production_modules()
        .iter()
        .filter(|(p, _)| {
            let text = std::fs::read_to_string(p).unwrap_or_default();
            text.contains("std::env::var") || text.contains("env::var(")
        })
        .map(|(p, _)| {
            p.display()
                .to_string()
                .replace(char::from(92u8), "/")
                .rsplit("crates/")
                .next()
                .unwrap_or_default()
                .to_string()
        })
        .collect();
    found.sort();
    let mut declared: Vec<String> = DECLARED.iter().map(|s| (*s).to_string()).collect();
    declared.sort();
    assert_eq!(
        found, declared,
        "the environment-reading surface changed; every entry is reviewed, not assumed"
    );
}

/// §32 rule 7 / §4.1: no placeholder API for a later milestone.
#[test]
fn production_code_carries_no_unimplemented_placeholder() {
    let offenders: Vec<String> = production_modules()
        .iter()
        .filter(|(p, _)| {
            let text = std::fs::read_to_string(p).unwrap_or_default();
            text.contains("todo!(") || text.contains("unimplemented!(")
        })
        .map(|(p, _)| p.display().to_string())
        .collect();
    assert!(offenders.is_empty(), "placeholders: {offenders:?}");
}

/// Every crate of the workspace forbids unsafe code.
#[test]
fn every_crate_forbids_unsafe_code() {
    let mut checked = 0;
    let mut crates: Vec<PathBuf> = std::fs::read_dir(repo_root().join("crates"))
        .expect("crates dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    crates.sort();
    for c in &crates {
        let lib = c.join("src/lib.rs");
        if !lib.exists() {
            continue;
        }
        let text = std::fs::read_to_string(&lib).expect("read");
        assert!(
            text.contains("#![forbid(unsafe_code)]"),
            "{} does not forbid unsafe code",
            lib.display()
        );
        checked += 1;
    }
    assert!(checked >= 4, "only {checked} crates checked");
}
