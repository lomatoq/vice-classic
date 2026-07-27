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

/// Types whose VALUES are corpus fixtures.
///
/// Not function names. M45-N1 and RT45-A3 are the same finding about the same
/// mistake: the previous version of this file sealed two NAMES
/// (`all_groups`, `procedural_groups`) and called the class closed, while
/// `authored_groups()` and `all_adversarial_groups()` — two of the three
/// summands of `all_groups()` — stayed public. An integration test reached 12
/// corpus groups through them, TWO of them sealed audit
/// (`authored/leaf`, `authored/bracket`), compiled, passed, and left this file
/// green.
///
/// A name list can only ever close the names on it. What closes the class is
/// the TYPE: any function that hands out a `GtSourceGroup` hands out a corpus
/// fixture, whatever it is called, and `AmbiguityPair` is on the list because
/// it carries one in a field.
const CORPUS_BEARING_TYPES: &[&str] = &["GtSourceGroup", "AmbiguityPair"];

/// The only fully-public function allowed to return one, as `module::name`.
///
/// One entry. §27.1 says scoring the sealed audit is what opens it, and this
/// is the single door through which a frozen coefficient may see the corpus:
/// development groups, development-legal profiles, nothing else.
const PUBLIC_CORPUS_DOORS: &[&str] = &["corridor/mod.rs::frozen_calibration_groups"];

/// One declaration of a function whose return type mentions a corpus type.
struct CorpusFn {
    module: String,
    name: String,
    fully_public: bool,
}

/// Every function in the workspace whose RETURN type mentions a corpus type.
///
/// Signatures are read across line breaks (rustfmt wraps them), and only the
/// text AFTER `->` is examined — `split_of_group(&self, g: &GtSourceGroup)`
/// takes a fixture and returns a `Split`, and counting it would make the
/// declared set meaningless.
fn corpus_returning_fns() -> Vec<CorpusFn> {
    let mut out = Vec::new();
    for (path, _) in production_modules() {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        // Only the PRODUCTION half of each file. A `fn corpus()` helper inside
        // `#[cfg(test)] mod tests` is not API surface, and deriving a
        // call-site pattern from its name would send the second echelon
        // hunting for the word `corpus(` across the workspace — which is
        // exactly the false positive this scan produced on its first run.
        //
        // The cut is at an INLINE `#[cfg(test)] mod tests {`, not at every
        // `#[cfg(test)]`: `corridor/mod.rs` declares `#[cfg(test)] mod tests;`
        // near the top, and cutting there discarded the whole file including
        // the one legal public door — which the non-vacuity assertion below
        // caught immediately, and which is the reason that assertion exists.
        let all: Vec<&str> = text.lines().collect();
        let cut = all
            .iter()
            .position(|l| l.trim() == "#[cfg(test)]")
            .filter(|i| {
                all.get(i + 1)
                    .is_some_and(|n| n.contains("mod ") && n.contains('{'))
            })
            .unwrap_or(all.len());
        let lines: Vec<&str> = all[..cut].to_vec();
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            let Some(after_fn) = fn_name_at(trimmed) else {
                continue;
            };
            // The signature runs until the line that opens the body (or the
            // `where` clause). Ten lines is far more than rustfmt ever needs
            // and the window stops at the brace anyway.
            let mut sig = String::new();
            for l in lines.iter().skip(i).take(10) {
                sig.push_str(l);
                sig.push(' ');
                if l.contains('{') || l.trim_end().ends_with(';') {
                    break;
                }
            }
            let Some((_, ret)) = sig.split_once("->") else {
                continue;
            };
            // Stop at the body brace so a fixture mentioned in the first line
            // of an implementation is not read as a return type.
            let ret = ret.split('{').next().unwrap_or_default();
            if !CORPUS_BEARING_TYPES.iter().any(|t| ret.contains(t)) {
                continue;
            }
            out.push(CorpusFn {
                module: rel(&path),
                name: after_fn.to_string(),
                // `pub(crate)` and `pub(super)` are NOT fully public: the
                // distinguishing text is the parenthesis right after `pub`.
                fully_public: trimmed.starts_with("pub fn ")
                    || trimmed.starts_with("pub async fn "),
            });
        }
    }
    out
}

/// The identifier after `fn`, for a line that declares one.
fn fn_name_at(trimmed: &str) -> Option<&str> {
    let idx = trimmed.find("fn ")?;
    // `fn` must start the line or follow a visibility / qualifier word, so
    // that `.filter(|f| f.name ...)` inside a body is not read as one.
    let before = trimmed[..idx].trim_end();
    let is_decl = before.is_empty()
        || before.ends_with("pub")
        || before.ends_with(')')
        || before.ends_with("const")
        || before.ends_with("unsafe")
        || before.ends_with("async")
        || before.ends_with("extern");
    if !is_decl {
        return None;
    }
    let rest = &trimmed[idx + 3..];
    let end = rest.find(|c: char| !c.is_alphanumeric() && c != '_')?;
    let name = &rest[..end];
    (!name.is_empty()).then_some(name)
}

/// A frozen coefficient may only be measured on the population
/// `corridor::frozen_calibration_groups` defines — and every OTHER way to
/// obtain a corpus fixture must be unreachable from outside the crate.
///
/// This is condition 1 of REVIEW_M4_5, and it replaces a check that looked for
/// two literal call sites. The history is worth keeping because it is the same
/// lesson twice:
///
/// - REVIEW_M4 M4-N1: the split filter was on four corpus measurements and not
///   the fifth. Answer: one legal population, plus a text scan for
///   `all_groups(` / `procedural_groups(`;
/// - REVIEW_M4 addendum M4-N11: the scan was walked around with
///   `use ... as ...`. Answer: `pub(crate)` on those two names, plus the scan
///   as second echelon;
/// - REVIEW_M4_5 M45-N1 / REDTEAM RT45-A3: the seal was on two NAMES, and
///   `authored_groups()` / `all_adversarial_groups()` were still public. An
///   integration test reached two SEALED-AUDIT groups and nothing failed.
///
/// So the check is no longer over names at all. It enumerates every function
/// in the workspace whose RETURN TYPE hands out a corpus fixture and asserts
/// the set of FULLY PUBLIC ones equals one declared door. A new public
/// accessor — under any name, in any module — fails here, which is what
/// "surface, not place" was supposed to mean the first two times.
#[test]
fn every_public_path_to_a_corpus_fixture_is_the_declared_one() {
    let fns = corpus_returning_fns();

    // Non-vacuity, in both directions. The scan must find the private
    // machinery too, or a parser that matched nothing would pass; and it must
    // find at least one of each visibility, or "all of them are sealed" could
    // be true of an empty set.
    assert!(
        fns.len() >= 6,
        "the signature scan found only {} corpus-returning functions; it is not parsing \
         declarations and every assertion below would be vacuous: {:?}",
        fns.len(),
        fns.iter().map(|f| &f.name).collect::<Vec<_>>()
    );
    assert!(
        fns.iter().any(|f| !f.fully_public),
        "the scan sees no sealed corpus function at all, so it cannot distinguish sealed from \
         public"
    );
    assert!(
        fns.iter().any(|f| f.fully_public),
        "the scan sees no public corpus function at all; the declared door would be compared \
         against an empty set and the equality below would prove nothing"
    );

    let mut public: Vec<String> = fns
        .iter()
        .filter(|f| f.fully_public)
        .map(|f| {
            format!(
                "{}::{}",
                f.module.trim_start_matches("vice-bench/src/"),
                f.name
            )
        })
        .collect();
    public.sort();
    public.dedup();
    let mut declared: Vec<String> = PUBLIC_CORPUS_DOORS
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    declared.sort();

    assert_eq!(
        public,
        declared,
        "the set of FULLY PUBLIC functions handing out a corpus fixture changed. Every entry is \
         a door through which an integration test can reach the sealed audit (M45-N1), so every \
         entry is reviewed rather than assumed. Sealed ones seen: {:?}",
        fns.iter()
            .filter(|f| !f.fully_public)
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>()
    );
}

/// Second echelon, and named as such: the call-site scan that used to be the
/// first line of defence.
///
/// It models a habit — "somebody writes `all_groups()` out of muscle memory" —
/// and habits are worth catching. It is NOT the proof; the proof is the
/// visibility enumeration above, which the compiler enforces.
#[test]
fn the_measurements_reach_the_corpus_through_the_legal_population() {
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

    // No integration test names any of the sealed accessors. It could not
    // compile if it did; asserting it is what makes the intent survive a
    // future widening, and the names are derived from the scan rather than
    // written out, so this file does not have to exempt itself.
    let sealed: Vec<String> = corpus_returning_fns()
        .into_iter()
        .filter(|f| !f.fully_public)
        .map(|f| format!("{}(", f.name))
        .collect();
    assert!(
        sealed.len() >= 4,
        "only {} sealed accessors derived; the second echelon would be searching for nothing",
        sealed.len()
    );
    for (path, code) in &tests {
        for direct in &sealed {
            assert!(
                !code.contains(direct.as_str()),
                "{path} reaches the corpus through {direct}: a frozen coefficient must come from \
                 corridor::frozen_calibration_groups(), which excludes the sealed audit and the \
                 held-out profile"
            );
        }
    }

    // And the in-crate surface is declared, not assumed.
    const DECLARED: &[&str] = &[
        // Declares and assembles the whole corpus.
        "vice-bench/src/gt/corpus.rs",
        "vice-bench/src/gt/grammar.rs",
        "vice-bench/src/gt/authored.rs",
        "vice-bench/src/gt/adversarial.rs",
        // Builds one procedural variant; reachable only from `grammar`.
        "vice-bench/src/gt/recipes.rs",
        // Harness runs that walk the corpus and skip the sealed audit at
        // run time; they report, they do not freeze anything.
        "vice-bench/src/corridor/mod.rs",
        "vice-bench/src/oracle/mod.rs",
        "vice-bench/src/topology/mod.rs",
        "vice-bench/src/topology/ambiguity.rs",
        // In-crate unit tests of corpus machinery itself.
        "vice-bench/src/correlation.rs",
        "vice-bench/src/gt/raster.rs",
        "vice-bench/src/gt/split.rs",
        "vice-bench/src/topology/tests.rs",
    ];
    let mut found: Vec<String> = production_modules()
        .iter()
        .filter(|(p, _)| {
            let code = strip_comments(&std::fs::read_to_string(p).unwrap_or_default());
            sealed.iter().any(|d| code.contains(d.as_str()))
        })
        .map(|(p, _)| rel(p))
        .collect();
    found.sort();
    found.dedup();
    let mut declared: Vec<String> = DECLARED.iter().map(|s| (*s).to_string()).collect();
    declared.sort();
    assert_eq!(
        found, declared,
        "the set of modules that can see a corpus fixture changed; every entry is reviewed, not \
         assumed"
    );
}
