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

/// `code` contains a call to `pattern` (`name(`) as a WHOLE identifier.
///
/// `contains` alone matches `frozen_calibration_groups(` when looking for
/// `groups(`, which made the second echelon fire on the one legal door.
fn contains_call(code: &str, pattern: &str) -> bool {
    code.match_indices(pattern).any(|(i, _)| {
        i == 0
            || !code[..i]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_')
    })
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

/// The types a frozen coefficient is actually MEASURED on.
///
/// RT45-A19: the type seal stops at the first type the project does not
/// declare. `rgba8()` hands out `&[u8]`, coverage is `Vec<Vec<f64>>`, a foreign
/// crate's public type prints itself through `String` - none of them can be
/// sealed, and three `pub fn` handed out 22 of 22 sealed-audit groups and
/// 258 048 bytes with no new type at all. Past this point there are no more
/// types, so the second echelon must stop looking for a TYPE in return position
/// and look for a CALL of the pipeline from a module that may not make it.
const CORPUS_PRODUCT_TYPES: &[&str] = &["RenderedFixture", "CoverageStack", "CellRaster"];

/// Fully-public functions allowed to return a corpus type: NONE.
///
/// The set is empty on purpose. The corpus types are `pub(crate)`, so a public
/// item that returned one is a compile error before this test runs; the empty
/// set is the second echelon saying the same thing. The one public door,
/// `corridor::frozen_calibration_population`, returns an OPAQUE
/// `FrozenPopulation` and therefore does not appear here — which is the whole
/// difference between this attempt and the previous three.
const PUBLIC_CORPUS_DOORS: &[&str] = &[];

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
    fns_returning(CORPUS_BEARING_TYPES)
}

/// The PIPELINE: functions that turn a corpus fixture into measurable output.
///
/// Derived by RETURN TYPE, the same way corpus doors are, because that is what
/// makes a function part of the pipeline - not its name.
fn pipeline_fns() -> Vec<CorpusFn> {
    fns_returning(CORPUS_PRODUCT_TYPES)
}

fn fns_returning(types: &[&str]) -> Vec<CorpusFn> {
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
            if !types.iter().any(|t| ret.contains(t)) {
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
/// - REDTEAM RT45-A9 / REVIEW M45-N14: the scan read the TEXT after `->` and
///   a hand-kept list of two type names. A `pub type Fixtures =
///   Vec<GtSourceGroup>` in return position and a `pub struct Basket { pub
///   groups: Vec<GtSourceGroup> }` walked past it and reached **60 groups,
///   all 22 sealed-audit**, with `fmt` clean, `clippy -D warnings` silent and
///   all seven tests here green — worse than the defect it replaced.
///
/// Three failures, one cause: a check that READS THE SOURCE has to enumerate
/// the syntaxes by which a value leaves a crate — return type, type alias,
/// public field, trait item, `impl Trait`, closure, `static`, `Deref` — and an
/// enumeration is exactly what "surface, not place" forbids.
///
/// So the proof moved to the type system (see
/// `the_corpus_types_are_sealed_by_the_compiler`), and THIS test is second
/// echelon: it asserts the public corpus surface is empty, which the compiler
/// already guarantees, so that the intent survives someone widening a type
/// back to `pub`.
/// The seal itself: the corpus types are `pub(crate)` and the lint that makes
/// that binding is DENIED.
///
/// This is the whole of condition 14 / RT45-A9, and it is four lines because
/// the mechanism is four lines. `GtSourceGroup`, `GtScene` and `AmbiguityPair`
/// are crate-private; `#![deny(private_interfaces, private_bounds)]` turns the
/// warn-by-default lints into hard errors, so EVERY syntax that would hand one
/// out fails to compile rather than needing to be noticed by a parser.
///
/// Verified against both attacks verbatim, planted inside the crate exactly
/// where the red team put them:
///
/// ```text
/// error: type `GtSourceGroup` is more private than the item `Rt45Fixtures`
/// error: type `GtSourceGroup` is more private than the item `rt45_alias_door`
/// error: type `GtSourceGroup` is more private than the item `Rt45Basket::groups`
/// ```
///
/// What this test guards is the seal's own preconditions: widen a type back to
/// `pub`, or drop the `deny`, and the compiler stops being the judge. Those are
/// the only two ways to undo it, and both fail here.
#[test]
fn the_corpus_types_are_sealed_by_the_compiler() {
    // Two groups, and the second is the delta-3 correction. The corpus types
    // answer what the corpus IS; the PRODUCT types answer what a frozen
    // measurement CONSUMES, and RT45-A14 showed the second is the one that
    // matters: sealing `GtSourceGroup` while `RenderedFixture` stayed `pub`
    // with `pub` fields and `render_cell` stayed `pub(crate)` let six lines in
    // an already-declared module hand an integration test 63 renders over all
    // 22 sealed-audit groups. The currency of F-0026 is the render.
    //
    // This list is not a guess and not a place where the next finding gets
    // appended: it is exactly what the compiler enumerated when the seal was
    // applied, one error at a time, until it stopped. The judge is still the
    // compiler; this guards the seal's preconditions.
    // DERIVED, not six hand-written pairs (condition 29 / M45-N26). The
    // property is: every type this crate treats as corpus-bearing or as corpus
    // PRODUCT must be declared `pub(crate)`. Both name sets already exist for
    // other reasons, so the list of declarations follows them - a type added to
    // either set is checked the day it is added, and there is no place here for
    // a seventh pair to be appended.
    let sealed_types: Vec<&str> = CORPUS_BEARING_TYPES
        .iter()
        .chain(CORPUS_PRODUCT_TYPES.iter())
        .chain(["GtScene"].iter())
        .copied()
        .collect();
    assert!(
        sealed_types.len() >= 6,
        "only {} types to check; the derivation is reading nothing",
        sealed_types.len()
    );
    let sources: Vec<(String, String)> = production_modules()
        .iter()
        .map(|(p, _)| (rel(p), std::fs::read_to_string(p).unwrap_or_default()))
        .collect();
    for ty in &sealed_types {
        let decl = format!("struct {ty}");
        let sites: Vec<&(String, String)> =
            sources.iter().filter(|(_, t)| t.contains(&decl)).collect();
        assert_eq!(
            sites.len(),
            1,
            "{ty} is declared in {} modules; the seal has no single site to check",
            sites.len()
        );
        let (module, text) = sites[0];
        assert!(
            text.contains(&format!("pub(crate) {decl}")),
            "{module} no longer declares `pub(crate) {decl}`. Widened to `pub`, a type alias, a              public field or a return position carries a corpus fixture - or the render a frozen              coefficient is measured on - out of the crate again (RT45-A9, RT45-A14)"
        );
    }
    // And no corpus-bearing type knows how to print itself. `format!("{g:?}")`
    // returns a `String`, `String` is public, and no visibility modifier can
    // close that door - it handed an integration test 878 067 characters of
    // sealed-audit geometry (RT45-A14, second instance). The only way to shut
    // it is for the type not to implement `Debug` at all.
    for (module, ty) in [
        ("vice-bench/src/gt/mod.rs", "struct GtScene"),
        ("vice-bench/src/gt/mod.rs", "struct GtSourceGroup"),
        ("vice-bench/src/gt/adversarial.rs", "struct AmbiguityPair"),
    ] {
        let text = std::fs::read_to_string(repo_root().join("crates").join(module)).expect(module);
        let before = &text[..text.find(ty).unwrap_or_else(|| panic!("{ty} in {module}"))];
        let derive = before
            .rfind("#[derive(")
            .map(|i| &before[i..])
            .unwrap_or("");
        assert!(
            !derive.contains("Debug"),
            "{ty} derives Debug again. A type whose values may not leave the crate must not know              how to print itself: the printed form is a String, and String is public"
        );
    }

    let lib = std::fs::read_to_string(repo_root().join("crates/vice-bench/src/lib.rs"))
        .expect("vice-bench lib.rs");
    assert!(
        lib.contains("#![deny(private_interfaces, private_bounds)]"),
        "vice-bench no longer denies private_interfaces/private_bounds. They are warn-by-default, \
         so without the deny a public item may hand out a sealed corpus type with only a warning \
         - and a warning is what three previous scans were unable to notice"
    );
}

/// The seal's SECOND question: not what crosses the boundary, but how wide it
/// is.
///
/// The compiler answers the first. It does not answer the second, and the
/// difference is where the fourth attempt failed: `FrozenPopulation::new` was
/// `pub(crate)`, and `pub(crate)` restricts nobody inside `vice-bench`. Four
/// lines in any module of the crate minted the legal-looking opaque handle over
/// the whole corpus and handed an integration test **60 groups, 22 of them
/// sealed-audit, 63 scenes** against the legal 22 and 24 — the number
/// REVIEW_M4_5 addendum 2 reproduced in a clean clone.
///
/// What closes it is not this test. It is `FrozenPopulation::of`, which is
/// private to `gt::legal` and REFUSES any group the split policy does not call
/// `development`, proven by measurement in
/// `gt::legal::tests::the_handle_refuses_a_population_that_is_not_development`.
/// This test guards that mechanism's two preconditions — one mint site, and it
/// does not name the wide population — and the second list is DERIVED from the
/// signature scan rather than written out, so a new wide accessor is covered
/// the day it is added.
#[test]
fn the_only_mint_site_for_the_legal_handle_cannot_widen_it() {
    let path = repo_root().join("crates/vice-bench/src/gt/legal.rs");
    let text = std::fs::read_to_string(&path).expect("the legal door module");
    let production = text
        .split_once("\n#[cfg(test)]")
        .map(|(p, _)| p.to_string())
        .expect("legal.rs must keep its in-crate refusal test");

    assert!(
        production.contains("fn of(") && !production.contains("pub(crate) fn of("),
        "the constructor of FrozenPopulation is no longer private to gt::legal. `pub(crate)` on \
         it is exactly the C188 defect: every module of vice-bench could then mint the handle \
         over all 60 groups, 22 of them sealed-audit"
    );
    let code = strip_comments(&production);
    let mints = code
        .match_indices("FrozenPopulation {")
        .filter(|(i, _)| {
            let before = code[..*i].trim_end();
            // The type's own declaration and its inherent impl are not mints.
            !before.ends_with("struct") && !before.ends_with("impl")
        })
        .count();
    assert_eq!(
        mints, 1,
        "gt/legal.rs has {mints} struct literals of FrozenPopulation; there must be exactly one, \
         inside `of`, or a mint that skips the split refusal has been added next to the one that \
         performs it"
    );

    // Derived, not listed: every sealed corpus accessor EXCEPT the filtered
    // one. The mint site may name `frozen_calibration_groups` and nothing else,
    // because everything else in this set is wider than `development`.
    let forbidden: Vec<String> = corpus_returning_fns()
        .into_iter()
        .filter(|f| !f.fully_public && f.name != "frozen_calibration_groups")
        .map(|f| format!("{}(", f.name))
        .collect();
    assert!(
        forbidden.len() >= 3,
        "only {} wide accessors derived; this test would be searching for nothing",
        forbidden.len()
    );
    for wide in &forbidden {
        // `code`, not `production`: the module documents the three failed
        // attempts by name, and a doc comment naming a defect is the opposite
        // of committing it.
        assert!(
            !contains_call(&code, wide),
            "the one mint site of the frozen-measurement handle names {wide}. It may reach the \
             corpus through the split-filtered accessor and no other way: the refusal inside \
             `of` would turn this into a run-time error, but a mint site that TRIES is a review \
             finding, not a caught bug"
        );
    }
}

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
        frozen.1.contains("frozen_calibration_population()"),
        "the frozen-calibration file reaches no legal population at all; this test would pass on \
         an empty file"
    );

    // No integration test names any of the sealed accessors. It could not
    // compile if it did; asserting it is what makes the intent survive a
    // future widening, and the names are derived from the scan rather than
    // written out, so this file does not have to exempt itself.
    // Whole-word calls only. The previous version searched for the bare
    // substring, so a sealed helper named `groups` made the echelon fire on
    // `frozen_calibration_groups(` — the ONE legal door (RT45-A9's honest
    // calibration note). A guard that trips on the only lawful call is not a
    // guard, it is a coincidence.
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
                !contains_call(code, direct),
                "{path} reaches the corpus through {direct}: a frozen coefficient must come from \
                 corridor::frozen_calibration_population(), which excludes the sealed audit and \
                 the held-out profile"
            );
        }
    }

    // D1's SECOND clause, as a habit guard on the frozen measurements.
    //
    // The clause is enforced by the type - `FrozenScene::render` takes a
    // `LegalCell` and `rasterize` a `&LegalProfile`, neither of which can be
    // built outside the legal handle - so this is second echelon and says so.
    // It models the specific habit RT45-A15 exploited: three of the five frozen
    // measurements used to pick a cell out of `matrix_v1()` by literal id, and
    // swapping one literal to `s64_ptiny-skia_…` measured a frozen coefficient
    // with the engine §28 M4 asks the system to generalize TO.
    let frozen_src = &frozen.1;
    for habit in ["matrix_v1(", "RasterProfile::"] {
        assert!(
            !contains_call(frozen_src, habit),
            "the frozen-calibration measurements name {habit}. A cell and a profile must come               from the legal handle, which cannot produce a held-out engine; naming the raw               matrix or the raw profile enum is how the held-out rasterizer got in (RT45-A15)"
        );
    }
    assert!(
        frozen_src.contains("legal_cells()") && frozen_src.contains("legal_profiles()"),
        "the frozen measurements reach neither the legal cell set nor the legal profile set, so          the check above would pass on a file that measures nothing"
    );

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
        // The ONE mint site of the frozen-measurement handle. It names the
        // split-filtered accessor and nothing wider, which
        // `the_only_mint_site_for_the_legal_handle_cannot_widen_it` checks.
        "vice-bench/src/gt/legal.rs",
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
            sealed.iter().any(|d| contains_call(&code, d))
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

/// No §28 M4.5 gate row compares against a number that did not come out of the
/// frozen gate file.
///
/// Condition 7 / M45-N6 / RT45-A5 asked for the thresholds to be REGISTERED,
/// and C179/C180 registered them. RT45-A10 is the follow-up: the previous
/// version of this test walked the source of `gate_table` and required each
/// `>=` to name a registered constant, so what it registered was the SPELLING.
/// `MIN_RECALL_ARMS / 20` names the constant and compares against 1; so does
/// `- 16`; so does `.saturating_sub(19)`.
///
/// Two things replaced it, and neither is a source scan:
///
/// - the row compares against [`TopologyGateConfig`], whose `Threshold` has no
///   arithmetic impls and no public constructor, so the arithmetic forms are
///   compile errors and the only source of a value is the committed file;
/// - `topology::tests::each_gate_row_flips_at_exactly_the_registered_threshold`
///   finds the EFFECTIVE boundary by scanning the measurement until each row
///   changes, and compares that to the file. It never reads the source, so it
///   is blind to how the comparison is written — on EITHER side.
///
/// What is left here is the third thing, which neither of those covers: that a
/// NEW bare number does not appear in a row tomorrow. The rule is a property of
/// the class, not a list of the five thresholds that exist today — any
/// comparison against a non-zero literal is an unregistered threshold. Zero is
/// excluded and the exclusion is deliberate: `== 0` and `> 0` are existence and
/// absence checks ("nothing was lost", "this population is not empty"), not
/// calibrated numbers, and registering them would make the gate file a list of
/// zeroes.
#[test]
fn no_gate_row_compares_against_an_unregistered_number() {
    let src = std::fs::read_to_string(repo_root().join("crates/vice-bench/src/topology/gate.rs"))
        .expect("the gate-row module");
    assert!(
        src.contains("pub fn gate_table("),
        "the gate-row module no longer declares gate_table; this scan would be reading nothing"
    );
    // THE WHOLE MODULE, not the text after `pub fn gate_table(`.
    //
    // Condition 21 / M45-N18: the scope was a PLACE, so a predicate declared
    // ABOVE the function was outside it. `fn rv_enough_saddles(r) -> bool {
    // r.saddle_alternatives_total > 7 }` moved the effective value of a clause-3
    // conjunct from 0 to 7 in one feature commit, with fmt clean, clippy silent,
    // nine hygiene tests green, `gates-check` exit 0 and the gate file untouched
    // — while the SAME literal inside `gate_table` was caught immediately and
    // with a byte offset. The instrument was sound and its scope was a place,
    // which is meta-rule M-1 applied to a scan for the sixth time in this
    // milestone.
    //
    // The module is the unit because the rows are what it exists to compute: a
    // helper anywhere in it is a helper of a gate row. A predicate in ANOTHER
    // module is not covered by this and is covered by the boundary scan
    // instead — stated as a limitation rather than left implied.
    let body = strip_comments(&src);

    // Every comparison operator, not just `>=` and `<=`: the old boundary let
    // `>` and `<` through, and a threshold written as `> 19` is a threshold.
    let mut comparisons = 0;
    for op in [">=", "<=", ">", "<"] {
        for (i, _) in body.match_indices(op) {
            // `->` and the second character of `>=` / `<=` are not comparisons.
            let prev = body[..i].chars().last().unwrap_or(' ');
            if prev == '-' || prev == '<' || prev == '>' || prev == '=' {
                continue;
            }
            if op.len() == 1 && body[i + 1..].starts_with('=') {
                continue;
            }
            comparisons += 1;
            let lit: String = body[i + op.len()..]
                .trim_start()
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            assert!(
                lit.is_empty() || lit.trim_end_matches(['0', '.']).is_empty(),
                "a gate row compares against the literal {lit} at byte {i}. A number that decides                  whether a spec clause is green belongs in configs/GATES_V1.toml and must reach                  the row through TopologyGateConfig, so that §27.7 can stop one commit from                  relaxing a clause and changing the code that has to meet it (M45-N6, RT45-A5,                  RT45-A10)"
            );
        }
    }
    assert!(
        comparisons >= 5,
        "only {comparisons} comparisons found in gate_table; the scan is not reaching the rows          and would pass on a renamed function"
    );

    // Non-vacuity in the other direction, and the reason this test is not
    // simply deleted: the rows must go through the loaded config, and the
    // number of thresholds they use must match the number the file registers.
    // A row that quietly stopped consulting one of them would pass every check
    // above.
    let gates = std::fs::read_to_string(repo_root().join("configs/GATES_V1.toml")).expect("gates");
    let topology = gates
        .split_once("[topology]")
        .expect("the topology section exists")
        .1;
    // The registered thresholds, as FIELD names: `gate_min_recall_arms` in the
    // file is `cfg.min_recall_arms` in the row. Derived from the file, so a key
    // added tomorrow is covered without editing this test.
    let registered: std::collections::BTreeSet<String> = topology
        .lines()
        .take_while(|l| !l.trim_start().starts_with('['))
        .filter_map(|l| l.trim_start().strip_prefix("gate_"))
        .filter_map(|l| l.split_whitespace().next())
        .filter(|k| k.starts_with("min_"))
        .map(|k| k.to_string())
        .collect();
    assert!(
        registered.len() >= 5,
        "only {} gate_min_* keys under [topology]; the comparison below would be against nothing",
        registered.len()
    );

    // Distinct fields the rows consult, not call sites: one threshold is
    // legitimately compared twice (both sides of a retaining pair), and a test
    // that counted calls would be asserting a coincidence.
    //
    // Whitespace is removed first, and the reason is a finding in miniature:
    // rustfmt broke `cfg.min_non_trivial_gt_arms` across three lines, the scan
    // stopped seeing it, and a guard against unregistered numbers reported that
    // a REGISTERED one had no consumer. A scan that does not model the layout
    // of the text it reads is the same defect as a parser that does not model
    // markdown escaping (F-0029, condition D2).
    let flat: String = body.chars().filter(|c| !c.is_whitespace()).collect();
    let mut used: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for (i, _) in flat.match_indices("cfg.") {
        let name: String = flat[i + 4..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() {
            used.insert(name);
        }
    }
    assert_eq!(
        used, registered,
        "the gate rows consult {used:?} and the frozen file registers {registered:?}. Either a          row stopped reading one of its thresholds, or a threshold was registered that no row          uses - and a registered value with no consumer is not a gate"
    );

    // And the loader really asks for each of them BY NAME.
    for field in &registered {
        assert!(
            src.contains(&format!("\"gate_{field}\"")),
            "gate_{field} is registered under [topology] and TopologyGateConfig::from_gates never              asks for it"
        );
    }
}

/// No workflow can point the gate-row thresholds at a different file.
///
/// RT45-A18. §27.7 is enforced over `GATE_PATHS`, so it protects a file NAME.
/// While the source of the thresholds was a `--gates` argument, the decision
/// followed a line of YAML instead: a copy of the gate file with all five
/// thresholds at `1`, plus one edited line in `ci.yml`, relaxed every §28 M4.5
/// clause with zero lines under `crates/`, the gate file untouched, and
/// `gates-check` returning exit 0 because neither changed path is a gate path.
///
/// The flag is gone — `topology_cmd` reads `GATE_PATHS[0]`, a constant — so the
/// primary answer is that there is nothing to point elsewhere. This is the
/// second echelon: it catches the flag being reintroduced, and it catches a
/// workflow naming a gate-like file the rule does not protect.
#[test]
fn no_workflow_redirects_the_gate_file() {
    let dir = repo_root().join(".github/workflows");
    let mut files = 0;
    let mut invocations = 0;
    let mut gate_args = 0;
    for entry in std::fs::read_dir(&dir).expect("the workflow directory") {
        let path = entry.expect("a workflow entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("yml") {
            continue;
        }
        files += 1;
        let text = std::fs::read_to_string(&path).expect("a workflow");
        let name = path.file_name().unwrap().to_string_lossy().to_string();

        // `topology` has no `--gates` any more, so its thresholds cannot be
        // redirected at all. Other subcommands still take one, and for those the
        // rule is the red team's second formulation: every invocation must name
        // exactly a protected path.
        for (i, _) in text.match_indices("--gates") {
            let arg: String = text[i + "--gates".len()..]
                .trim_start()
                .chars()
                .take_while(|c| !c.is_whitespace())
                .collect();
            assert!(
                vice_bench::gates::GATE_PATHS.contains(&arg.as_str()),
                "{name} passes --gates {arg}, which is not in GATE_PATHS. §27.7 protects the \
                 paths in GATE_PATHS, so a gate file named anywhere else moves the decision to a \
                 path no rule covers (RT45-A18)"
            );
            gate_args += 1;
        }
        invocations += text.matches("gt-corpus -- topology").count();
    }
    assert!(files >= 1, "no workflow file was read at all");
    assert!(
        invocations >= 1,
        "no workflow invokes `gt-corpus topology`; this test would be guarding nothing"
    );
    assert!(
        gate_args >= 1,
        "no workflow passes --gates at all, so the path check above never ran"
    );

    // And there is no SECOND gate-shaped file for anything to be pointed at.
    // The red team's sidecar was a copy of the gate file with every threshold
    // at 1; what makes a file a gate is that it declares frozen sections, so
    // that is what is looked for rather than a name or a location.
    // THE WHOLE REPOSITORY, not `configs/`. Condition 26 / M45-N24: a sidecar
    // only had to be written somewhere else for this to miss it, so the scope
    // was a directory rather than the property. `target/`, `.git/` and `runs/`
    // are skipped as build output, history and recorded past runs.
    fn walk(dir: &std::path::Path, out: &mut Vec<String>, root: &std::path::Path) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if path.is_dir() {
                if matches!(name.as_str(), "target" | ".git" | "runs" | "node_modules") {
                    continue;
                }
                walk(&path, out, root);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            if !text.contains("status = \"frozen\"") {
                continue;
            }
            out.push(
                path.strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    let mut gate_shaped = Vec::new();
    walk(&repo_root(), &mut gate_shaped, &repo_root());
    gate_shaped.sort();
    let mut protected: Vec<String> = vice_bench::gates::GATE_PATHS
        .iter()
        .map(|p| (*p).to_string())
        .collect();
    protected.sort();
    assert_eq!(
        gate_shaped, protected,
        "the set of files declaring frozen gate sections is not the set §27.7 protects. A second          gate-shaped file is a second gate that no rule covers, and pointing anything at it costs          one line (RT45-A18)"
    );
    println!(
        "{files} workflow file(s), {invocations} topology invocation(s), {gate_args} --gates          argument(s), every one naming a protected path"
    );
}

/// A FREE call of `name(`, not a method call of the same name.
///
/// `FrozenScene::rasterize` is deliberately named after the pipeline function it
/// wraps, so `s.rasterize(&t, &profile, psf)` in a frozen measurement is the
/// LEGAL door being used correctly, while `rasterize(scene.certified(), …)` is
/// the pipeline being called directly. A scan that cannot tell them apart calls
/// the one legal usage a violation - which is the shape of the false positive
/// this file already fixed once, on `groups(`.
fn contains_free_call(code: &str, call: &str) -> bool {
    let name = call.trim_end_matches('(');
    code.match_indices(call).any(|(i, _)| {
        let before = code[..i].chars().rev().find(|c| !c.is_whitespace());
        if before == Some('.') {
            return false;
        }
        code[..i]
            .chars()
            .last()
            .is_none_or(|c| !c.is_alphanumeric() && c != '_')
            && !name.is_empty()
    })
}

/// Only declared modules may CALL the pipeline that turns a fixture into
/// measurable output.
///
/// RT45-A19, as a class rule rather than a patch. The type seal ends at the
/// first type this project does not declare: `LegalRender::rgba8()` returns
/// `&[u8]`, coverage is `Vec<Vec<f64>>`, and a foreign crate's public type
/// prints itself into a `String`. Three `pub fn` in an already-declared module
/// handed an integration test 22 of 22 sealed-audit groups and 258 048 bytes
/// with no new type at all, and no visibility modifier could have stopped any of
/// them. Past `Vec<u8>` there is nothing left to seal.
///
/// So the echelon changes what it looks for. Not a TYPE in return position - a
/// CALL of the pipeline from a module that has no business making one. The
/// pipeline is DERIVED by return type, so a function added to it tomorrow is
/// covered the day it is written; only the set of callers is declared, and that
/// set is the thing a reviewer should be reading anyway.
#[test]
fn only_declared_modules_call_the_render_pipeline() {
    let pipeline: Vec<String> = pipeline_fns()
        .into_iter()
        .map(|f| format!("{}(", f.name))
        .collect();
    assert!(
        pipeline.len() >= 3,
        "only {} pipeline functions derived ({pipeline:?}); the scan is not reading the \
         signatures and would be searching for nothing",
        pipeline.len()
    );
    // The three the red team used must be among them, or the derivation has
    // drifted away from the thing it is supposed to describe.
    // Written WITHOUT the parenthesis on purpose: with it, this file would
    // contain the very call pattern it forbids an integration test from
    // containing, and the guard would trip on itself. Same shape as the
    // `groups(` false positive this file already fixed once.
    for must in ["render_cell", "rasterize", "render_cell_raster"] {
        let wanted = format!("{must}(");
        assert!(
            pipeline.contains(&wanted),
            "the derived pipeline {pipeline:?} does not contain {must}, which is one of the calls \
             RT45-A19 used"
        );
    }

    // Modules that may turn a fixture into pixels or coverage. Every entry is a
    // harness that walks the corpus at run time and reports, the corpus
    // machinery itself, or the one legal handle.
    const MAY_CALL: &[&str] = &[
        // The pipeline's own modules.
        "vice-bench/src/gt/degradation.rs",
        "vice-bench/src/gt/raster.rs",
        // The one legal door: this is the whole point of it existing.
        "vice-bench/src/gt/legal.rs",
        // Corpus machinery that renders to build or label fixtures.
        "vice-bench/src/gt/corpus.rs",
        "vice-bench/src/gt/adversarial.rs",
        // Harness runs: they report, they freeze nothing.
        "vice-bench/src/corridor/mod.rs",
        "vice-bench/src/oracle/ceiling.rs",
        "vice-bench/src/topology/mod.rs",
        "vice-bench/src/topology/ambiguity.rs",
        "vice-bench/src/correlation.rs",
    ];
    let mut found: Vec<String> = production_modules()
        .iter()
        .filter(|(p, _)| {
            let code = strip_comments(&std::fs::read_to_string(p).unwrap_or_default());
            pipeline.iter().any(|c| contains_free_call(&code, c))
        })
        .map(|(p, _)| rel(p))
        .collect();
    found.sort();
    found.dedup();
    let mut declared: Vec<String> = MAY_CALL.iter().map(|s| (*s).to_string()).collect();
    declared.sort();
    assert_eq!(
        found, declared,
        "the set of modules that turn a corpus fixture into measurable output changed. Past the \
         last sealable type this is the only mechanism left, so every entry is reviewed rather \
         than assumed (RT45-A19)"
    );

    // And no INTEGRATION test calls the pipeline directly - that is the
    // boundary the seal used to guard by type and can no longer guard past
    // `Vec<u8>`.
    for (path, code) in integration_tests() {
        for call in &pipeline {
            assert!(
                !contains_free_call(&code, call),
                "{path} calls {call} directly. A frozen coefficient is measured through \
                 FrozenScene, which cannot be handed a held-out profile or a sealed-audit scene"
            );
        }
    }
}
