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
///
/// REVIEW_M4 M4-N10: the pattern matched `std::env::var` and `env::var(`, and
/// `use std::env;` followed by `env::var_os(` matched neither. Nothing in the
/// tree calls it, so nothing was hidden — but a surface check that a rename
/// walks around is not a surface check. Every reading form is listed now, and
/// the list is asserted to be reached by an actual call rather than trusted.
#[test]
fn only_the_recorded_modules_read_an_environment_variable() {
    /// Every way to read the environment in std. `var`/`var_os` are the
    /// readers; `vars`/`vars_os` enumerate, which reads all of them at once
    /// and is therefore the same question.
    const READS: &[&str] = &["env::var(", "env::var_os(", "env::vars(", "env::vars_os("];
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
            READS.iter().any(|r| text.contains(r))
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

    // The pattern list is not vacuous: each form must be one this tree could
    // actually contain, and at least one declared module must be reached
    // through the plain reader. Without this the whole test passes on a
    // pattern list of typos.
    let declared_text: String = DECLARED
        .iter()
        .map(|m| std::fs::read_to_string(repo_root().join("crates").join(m)).unwrap_or_default())
        .collect();
    assert!(
        READS.iter().any(|r| declared_text.contains(r)),
        "no declared module matches any pattern, so the search proves nothing"
    );
    assert!(
        READS.iter().all(|r| r.starts_with("env::")),
        "a pattern that does not name the module path would match unrelated code"
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

/// Integration-test files of the whole workspace, as (crate-relative path,
/// source) pairs with comment lines stripped.
fn integration_tests() -> Vec<(String, String)> {
    let mut crates: Vec<PathBuf> = std::fs::read_dir(repo_root().join("crates"))
        .expect("crates dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    crates.sort();
    let mut out = Vec::new();
    for c in &crates {
        let dir = c.join("tests");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
        paths.sort();
        for p in paths {
            if p.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&p).expect("read");
                out.push((rel(&p), strip_comments(&text)));
            }
        }
    }
    out
}

fn rel(p: &Path) -> String {
    p.display()
        .to_string()
        .replace(char::from(92u8), "/")
        .rsplit("crates/")
        .next()
        .unwrap_or_default()
        .to_string()
}

fn strip_comments(text: &str) -> String {
    text.lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The WIDE corpus population, by function name.
///
/// The CALL and DECLARATION patterns are built from these at run time rather
/// than written out, so this file does not contain the very literals it
/// searches for and does not have to exempt itself. An exemption is where
/// the next hole goes.
const WIDE_POPULATION_FNS: &[&str] = &["all_groups", "procedural_groups"];

fn call_patterns() -> Vec<String> {
    WIDE_POPULATION_FNS
        .iter()
        .map(|n| format!("{n}("))
        .collect()
}

/// A frozen coefficient may only be measured on the population
/// `corridor::frozen_calibration_groups` defines — and the wide population
/// must be UNREACHABLE from where the measurements live, not merely unused.
///
/// REVIEW_M4 M4-N1 found four of five corpus-wide measurements filtering the
/// sealed-audit split and one not — the one that froze a production constant.
/// The answer then was one legal population plus a walk over the source of
/// `corridor/tests.rs` looking for two literals. The REVIEW_M4 addendum
/// (M4-N11, condition D1) walked around that walk in a single line:
///
/// ```text
/// use crate::gt::corpus::all_groups as every_group;   // no literal to find
/// ```
///
/// 104 renders became 286, the sealed audit was back inside the frozen kernel
/// table, and nothing failed. So the seal is now the compiler, and this test
/// is the SECOND ECHELON over the class rather than the first line over one
/// file. Four clauses:
///
/// 1. `all_groups` and `procedural_groups` are declared `pub(crate)`. Widening
///    either back to `pub` fails here — which is the only way the compiler's
///    seal could be undone;
/// 2. the measurements live in `tests/frozen_calibration.rs`, a separate
///    crate, and reach the corpus through `frozen_calibration_groups()`;
/// 3. NO integration test anywhere in the workspace names the wide
///    population. It could not compile if it did; asserting it is what makes
///    the intent survive a future `pub`;
/// 4. inside `vice-bench` the SET of modules that call the wide population is
///    declared, exactly as the environment-reading surface is. A new in-crate
///    caller is a failure a reviewer looks at, not a line nobody notices
///    (meta-rule M-1: a surface, not a place).
#[test]
fn the_wide_corpus_population_is_unreachable_from_the_measurements() {
    // 1. The seal itself.
    for (module, name) in [
        ("vice-bench/src/gt/corpus.rs", "all_groups"),
        ("vice-bench/src/gt/grammar.rs", "procedural_groups"),
    ] {
        assert!(
            WIDE_POPULATION_FNS.contains(&name),
            "{name} is sealed here but is not in the population list, so clause 3 would not              search for it"
        );
        let decl = format!("pub(crate) fn {name}(");
        let text = std::fs::read_to_string(repo_root().join("crates").join(module)).expect(module);
        assert!(
            text.contains(&decl),
            "{module} no longer declares `{decl}`. That declaration IS condition D1: widened to \
             `pub`, an integration test can name the wide population again and the seal is back \
             to being a habit"
        );
    }

    // 2. The measurements exist, outside the crate, on the legal population.
    let tests = integration_tests();
    let frozen = tests
        .iter()
        .find(|(p, _)| p == "vice-bench/tests/frozen_calibration.rs")
        .expect("the frozen-calibration measurements must exist and be OUTSIDE the crate");
    assert!(
        frozen.1.contains("frozen_calibration_groups()"),
        "the frozen-calibration file reaches no legal population at all; this test would pass on \
         an empty file"
    );

    // 3. No integration test names the wide population.
    let calls = call_patterns();
    for (path, code) in &tests {
        for direct in &calls {
            assert!(
                !code.contains(direct.as_str()),
                "{path} reaches the corpus through {direct}: a frozen coefficient must come from \
                 corridor::frozen_calibration_groups(), which excludes the sealed audit and the \
                 held-out profile"
            );
        }
    }

    // 4. The in-crate surface is declared, not assumed.
    const DECLARED: &[&str] = &[
        // Declares and assembles the whole corpus; the manifest IS the wide
        // population.
        "vice-bench/src/gt/corpus.rs",
        // Declares `procedural_groups`.
        "vice-bench/src/gt/grammar.rs",
        // Harness runs that walk the corpus and skip the sealed audit at
        // run time; they report, they do not freeze anything.
        "vice-bench/src/corridor/mod.rs",
        "vice-bench/src/oracle/mod.rs",
        "vice-bench/src/topology/mod.rs",
        // In-crate unit tests of corpus machinery itself, which are about
        // the corpus rather than about a frozen coefficient.
        "vice-bench/src/correlation.rs",
        "vice-bench/src/gt/raster.rs",
        "vice-bench/src/gt/split.rs",
    ];
    let mut found: Vec<String> = production_modules()
        .iter()
        .filter(|(p, _)| {
            let code = strip_comments(&std::fs::read_to_string(p).unwrap_or_default());
            calls.iter().any(|d| code.contains(d.as_str()))
        })
        .map(|(p, _)| rel(p))
        .collect();
    found.sort();
    let mut declared: Vec<String> = DECLARED.iter().map(|s| (*s).to_string()).collect();
    declared.sort();
    assert_eq!(
        found, declared,
        "the set of modules that can see the WIDE corpus population changed; every entry is \
         reviewed, not assumed"
    );
}
