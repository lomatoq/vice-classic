//! Numbers the milestone documents quote, checked against the committed
//! artifacts.
//!
//! REVIEW_M4 M4-N3 found five statements presented as measurements that were
//! not: "497 arms" against 500, "1233 probes" against 1242, a step-invariance
//! triple that belonged to an older run, "85 modules" against 87 and "six
//! crates" against seven. None of them changed a verdict — `STATUS_M4` §2
//! carried the same quantities correctly — but condition B3 of the previous
//! gate says in so many words that what a STATUS or a REPRODUCIBILITY
//! presents as measured must be measured, and this was the third instance of
//! that class.
//!
//! Correcting five numbers would leave the class open, so the numbers a
//! document quotes are DECLARED in a table each `REPRODUCIBILITY_*.md`
//! carries, each with the path into the artifact it comes from, and this test
//! resolves every path and compares. A quoted number that drifts from the
//! artifact fails here, the same way a module over 800 lines, an undeclared
//! environment read or a crate without `forbid(unsafe_code)` already do.
//!
//! ## Condition D2 (REVIEW_M4 addendum, M4-N12)
//!
//! The first version split table rows on `'|'` with no knowledge of markdown
//! escaping, and the reviewer walked through the hole twice:
//!
//! 1. `REPRODUCIBILITY_M4.md` carried 41 rows with an artifact key and this
//!    file parsed 40. The row for `max \|α̂ − true coverage\|` produced seven
//!    cells instead of five, the key moved out of `cells[2]`, and one
//!    DECLARED quantity silently stopped being checked. "Every declared
//!    number agrees" was true of a set nobody could see the size of.
//! 2. In the `STATUS_M4` clause row G9 the same escapes truncated
//!    `row.split('|').nth(4)`, so every token after them was invisible. The
//!    reviewer replaced `= 0.0090 over 143 arms` with `= 0.9999 over 999
//!    arms` and all three tests stayed green.
//!
//! Both are the same defect: a parser that does not model the syntax it
//! reads. So the split is now escape-aware ([`table_cells`]), and — because a
//! parser can go blind again in a way this fix does not anticipate — the
//! mechanism carries controls ON ITSELF:
//!
//! - every line that CONTAINS an artifact key must parse into a claim, and a
//!   line that carries a key without yielding one is an error naming the
//!   line. Counting is not enough: a count can agree while both sides are
//!   zero, so the count is asserted AND every key-bearing line is resolved;
//! - each clause row must contribute a NONZERO number of checked tokens, so a
//!   row whose evidence column stops being found fails instead of passing
//!   vacuously;
//! - the escape-aware split is itself unit-tested against the row shape that
//!   defeated the old one.
//!
//! ## What this does NOT do, stated at the width it actually holds
//!
//! Condition 19 is that the paragraph above promised more than the code keeps,
//! so the promise is narrowed here rather than the finding being answered with
//! a sentence.
//!
//! - **It does not read prose.** A sentence outside a table can go stale in
//!   silence, and one did: STATUS_M4_5 §3 said "78 declared values" for the
//!   whole delta. The mitigation is not a check — it is that the count is
//!   PRINTED by this test and the documents now spell it in words, so a reader
//!   who wants the figure has to take it from the run. That makes it noticeable,
//!   not verified, and the difference is the point.
//! - **Membership is not equality.** A row under [`CLAUSE_ROWS`] only has to
//!   quote numbers that appear SOMEWHERE among the declared values. With
//!   ninety-five of them, most small integers, a false measurement can usually
//!   be assembled from someone else's. The positional tier fixes that for the
//!   rows it covers, and only for those.
//! - **The positional tier covers rows that NAME a spec clause.** That is
//!   derived rather than listed (RT45-A11), but a row reporting a clause without
//!   naming it would not be caught. A row declares itself by its name or it does
//!   not declare itself at all, and the alternative is the hand-list that failed
//!   three times in this milestone.
//! - **It compares documents to artifacts, not artifacts to reality.** That a
//!   number agrees with `TOPOLOGY_M4_5.json` says nothing about whether the
//!   harness that produced the JSON measured the right thing. That is what the
//!   gate rows, their controls and their knockouts are for.
//!
//! What it DOES do is make every quantity a document presents as measured have
//! exactly one source, and make an edit to one side fail against the other.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

/// Documents that declare numbers, and the artifact each key names.
///
/// A milestone adds its own row here; the mechanism is not per-milestone.
const DECLARING_DOCS: &[&str] = &["docs/REPRODUCIBILITY_M4.md", "docs/REPRODUCIBILITY_M4_5.md"];

/// The gate tables whose CLAUSE rows may quote only declared numbers, with
/// the row prefixes that identify them.
///
/// MEMBERSHIP tier: every numeric token must be one of the declared values.
/// It catches the historical F-0028 shape — a number left over from an older
/// run — and it is what `STATUS_M4` is held to, because a signed document may
/// only receive an addendum and its rows cannot be rewritten into the
/// positional form below.
const CLAUSE_ROWS: &[(&str, &[&str])] = &[
    ("docs/STATUS_M4.md", &["| G7 ", "| G8 ", "| G9 ", "| G10 "]),
    ("docs/STATUS_M4_5.md", &["| T1 ", "| T2 ", "| T3 "]),
    // The delta gate tables are NOT here. Their rows are declared `membership`
    // or `unit-test`, and since RT45-A22 those tiers publish no measured
    // quantity at all - so there is nothing for a membership check to check,
    // and a row that contributed zero tokens would fail the non-vacuity rule
    // for the right reason. The quantities those rows used to carry now live in
    // T1d, bound position by position, and the rows point at it.
    // Condition 10. One of the five original B3 defects (M4-N3) was in THIS
    // file - a step-invariance triple from an older run - and it stayed
    // outside the mechanism that closes B3, so two of the class's three
    // instances were closed and the third was not. The list of documents
    // under check is itself an object of meta-rule M-1.
    (
        "REQUIREMENTS_TRACEABILITY.md",
        &["| M45-11 ", "| M45-13 ", "| M45-14 "],
    ),
];

/// POSITIONAL tier: the i-th number of the row must EQUAL the i-th declared
/// key, not merely appear somewhere in the artifacts.
///
/// M45-N11 and RT45-A4 are the same attack from two cold contexts: the declared
/// values include many small integers, so a false measurement can almost always
/// be assembled out of somebody else's numbers. Swapping `31` for `56` in the
/// row that reports spec clause 1 left all four tests green, and so did
/// rewriting "recall 100 of 100" as "recall 56 of 132 ... budget lost 2".
///
/// RT45-A11 is that the fix covered the WRONG ROWS. `T1d/T2d/T3d` were listed
/// here by hand and bound; `T1/T2/T3` — the rows that say "клауза спеки" in so
/// many words — stayed under membership, and the attack went through them in
/// silence. A hand-list of row ids is the same shape of defect as a hand-list of
/// function names (M-1), and it failed the same way.
///
/// So the set is DERIVED, from what a row says about itself. A row belongs to
/// the positional tier when it reports one of the three §28 M4.5 clauses BY
/// NAME, and the three names come from the spec rather than from this file's
/// opinion. Any future row reporting a clause is covered the day it is written,
/// in any document under check.
const SPEC_CLAUSE_NAMES: &[&str] = &[
    "GT-equivalent topology present in envelope",
    "Ambiguous fixtures retain alternatives",
    "No magic-threshold-only architecture",
];

/// Documents whose gate tables are under the positional tier.
///
/// `STATUS_M4` is deliberately absent and it is not an oversight: it is signed,
/// it may only receive an addendum, and its rows cannot be rewritten into the
/// positional form. It stays under membership, which is what `CLAUSE_ROWS`
/// says.
const POSITIONAL_DOCS: &[&str] = &["docs/STATUS_M4_5.md"];

/// A gate table is a markdown table with this header. Both the milestone table
/// and the delta table carry it, so the derivation does not have to know that
/// there are two.
const GATE_TABLE_HEADER: &[&str] = &["", "#", "Gate", "Статус", "Evidence", ""];

/// EVERY data row of EVERY gate table, as (document, row prefix).
///
/// Derived from the document's own structure: a gate table is found by its
/// header, and every row under it belongs to this set whatever it is called.
/// Deliberately NOT "rows that report a spec clause" any more.
///
/// RT45-A17: identifying a row by the clause name in its title is a derivation
/// closed on itself, because the title lives in the document being checked and
/// one commit edits both. Renaming `T1`'s title to Russian took it out of the
/// tier, a decorative seventh row satisfied the non-vacuity counter, and a
/// forged `recall 56 из 132` under a `PASS` went through in silence. M45-N20 is
/// the other end of the same hole: a row in NEITHER tier, because the
/// membership tier was a hand-list of prefixes.
///
/// Both close the same way, and it is the form both cold contexts asked for:
/// every row of every gate table carries a SPECIFICATION, and a row without one
/// is an error rather than an omission. A rename no longer helps - the row id
/// keeps its spec - and a new row cannot arrive unspecified.
fn gate_table_rows() -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    for doc in POSITIONAL_DOCS {
        let Ok(text) = std::fs::read_to_string(repo_root().join(doc)) else {
            continue;
        };
        let mut in_gate_table = false;
        for line in text.lines() {
            let Some(cells) = table_cells(line) else {
                in_gate_table = false;
                continue;
            };
            if cells == GATE_TABLE_HEADER {
                in_gate_table = true;
                continue;
            }
            if !in_gate_table || cells.len() < 5 {
                continue;
            }
            if cells[1].chars().all(|c| c == '-' || c.is_whitespace()) {
                continue;
            }
            out.push((*doc, format!("| {} ", cells[1])));
        }
    }
    out
}

/// The KIND declared for a row in the specification table.
///
/// Condition 23: the membership tier used to be a hand-list of row prefixes, so
/// a row absent from it was checked by nothing. Every row declares its kind in
/// the same table as the positional bindings now, so a row is in exactly one
/// tier and never in none.
///
/// `unit-test` is a real third kind, not a hatch: rows T4-T9 report what a unit
/// test measured and their numbers are in no artifact. What stops it BEING a
/// hatch is the constraint the test enforces separately - a row that names a
/// spec clause may only be positional.
fn declared_row_kinds() -> Vec<(String, &'static str)> {
    let path = repo_root().join("docs/REPRODUCIBILITY_M4_5.md");
    let text = std::fs::read_to_string(&path).expect("REPRODUCIBILITY_M4_5");
    let mut out = Vec::new();
    for line in text.lines() {
        let Some(cells) = table_cells(line) else {
            continue;
        };
        if cells.len() < 4 {
            continue;
        }
        let Some(row) = cells[1].strip_prefix("row ") else {
            continue;
        };
        let kind = match cells[2].as_str() {
            "membership" => "membership",
            "unit-test" => "unit-test",
            n if n.parse::<usize>().is_ok() => "positional",
            _ => continue,
        };
        out.push((format!("| {row} "), kind));
    }
    out.sort();
    out.dedup();
    out
}

/// Artifact file names, by the prefix a declared key uses.
fn artifact_file(kind: &str) -> Option<&'static str> {
    match kind {
        "corridor" => Some("CORRIDOR_M4.json"),
        "oracle" => Some("ORACLE_M4.json"),
        "topology" => Some("TOPOLOGY_M4_5.json"),
        _ => None,
    }
}

/// The key prefixes a table row may carry, in the exact form they appear in
/// the markdown. Used both to FIND key-bearing lines and to reject a key
/// whose artifact is unknown.
const KEY_PREFIXES: &[&str] = &["`corridor:", "`oracle:", "`topology:"];

fn artifact(name: &str) -> serde_json::Value {
    let path = repo_root().join("docs/gt").join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Split a markdown table row into cells, honouring the escape `\|`.
///
/// This is the whole of condition D2's first half. `split('|')` is wrong for
/// markdown: a backslash-escaped bar is CONTENT, and a row that contains one
/// — `max \|α̂ − true coverage\|` — yields two spurious cells, which moved a
/// declared key out of the column this file reads and truncated a clause
/// row's evidence column. The escape is consumed here, so a cell carries the
/// text a reader sees.
///
/// Returns `None` for a line that is not a table row at all, so ordinary
/// prose containing a bar cannot be mistaken for one.
fn table_cells(line: &str) -> Option<Vec<String>> {
    if !line.trim_start().starts_with('|') {
        return None;
    }
    let mut cells: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'|') => {
                chars.next();
                cur.push('|');
            }
            '|' => {
                cells.push(cur.trim().to_string());
                cur.clear();
            }
            other => cur.push(other),
        }
    }
    cells.push(cur.trim().to_string());
    Some(cells)
}

struct Claim {
    artifact: &'static str,
    path: String,
    value: f64,
    doc: &'static str,
    line: usize,
}

/// Parse the declared-claims tables out of every declaring document.
///
/// Every line that CARRIES a key must yield a claim. That is the control on
/// the mechanism: the previous version discarded an unparsable key-bearing
/// row silently, and one declared quantity spent the whole milestone
/// unchecked.
fn declared_claims() -> Vec<Claim> {
    let mut out = Vec::new();
    let mut key_bearing = 0usize;
    let mut positional = 0usize;
    for doc in DECLARING_DOCS {
        let path = repo_root().join(doc);
        let Ok(text) = std::fs::read_to_string(&path) else {
            // A milestone that has not written its document yet is not an
            // error; a document that exists and is unparsable is.
            continue;
        };
        for (i, line) in text.lines().enumerate() {
            let cells = table_cells(line);
            // A positional-specification row (`| row T1d | 3 | key |`) also
            // carries a key, in a different column, and is resolved by
            // `positional_spec` instead. Recognised BEFORE the key-bearing
            // count, so the two mechanisms do not each expect to own the
            // other's rows.
            if cells
                .as_ref()
                .and_then(|c| c.get(1))
                .is_some_and(|c| c.starts_with("row "))
            {
                positional += 1;
                continue;
            }
            let carries_key = KEY_PREFIXES.iter().any(|k| line.contains(k));
            if carries_key {
                key_bearing += 1;
            }
            let Some(cells) = cells else {
                assert!(
                    !carries_key,
                    "{doc}:{} carries an artifact key but is not a table row",
                    i + 1
                );
                continue;
            };
            let parsed = parse_claim(&cells, doc, i + 1);
            match parsed {
                Some(c) => out.push(c),
                None => assert!(
                    !carries_key,
                    "{doc}:{} carries an artifact key and did NOT parse into a declared claim. \
                     This is exactly condition D2: a row whose key the parser cannot see is a \
                     quantity nobody checks.\nrow: {line}",
                    i + 1
                ),
            }
        }
    }
    assert!(
        positional >= 20,
        "only {positional} positional-specification rows seen; if the positional table stops          being recognised its rows fall back into the membership count and both mechanisms go          quiet"
    );
    assert_eq!(
        key_bearing,
        out.len(),
        "{} lines carry an artifact key and {} claims parsed; the parser is not seeing the table",
        key_bearing,
        out.len()
    );
    out
}

fn parse_claim(cells: &[String], doc: &'static str, line: usize) -> Option<Claim> {
    // | description | `artifact:path` | value |
    if cells.len() < 5 {
        return None;
    }
    let key = cells[2].strip_prefix('`')?.strip_suffix('`')?;
    let (kind, path) = key.split_once(':')?;
    let artifact = artifact_file(kind)
        .unwrap_or_else(|| panic!("{doc}:{line}: unknown artifact {kind:?} in key {key:?}"));
    let value = cells[3].parse::<f64>().ok()?;
    Some(Claim {
        artifact,
        path: path.to_string(),
        value,
        doc,
        line,
    })
}

/// Resolve a dotted path, with `coverage@95` selecting from the
/// `(level, value)` pairs the report publishes. The level is written in
/// PERCENT so that the path separator stays unambiguous.
fn resolve(root: &serde_json::Value, path: &str) -> Option<f64> {
    let mut here = root.clone();
    for step in path.split('.') {
        if let Some(level) = step.strip_prefix("coverage@") {
            let want: f64 = level.parse::<f64>().ok()? / 100.0;
            let pair = here
                .get("coverage")?
                .as_array()?
                .iter()
                .find(|p| {
                    p.get(0)
                        .and_then(|l| l.as_f64())
                        .is_some_and(|l| (l - want).abs() < 1e-9)
                })?
                .clone();
            here = pair.get(1)?.clone();
            continue;
        }
        // A numeric step indexes an ARRAY. Needed by the M4.5 table, whose
        // per-field and per-bucket rows live in arrays; `Value::get` with a
        // string key returns `None` on an array, so without this a declared
        // path would silently fail to resolve — and a path that silently
        // fails to resolve is the very hole condition D2 closed.
        if let Ok(i) = step.parse::<usize>() {
            here = here.get(i)?.clone();
            continue;
        }
        here = here.get(step)?.clone();
    }
    here.as_f64()
}

fn resolve_claim(c: &Claim) -> Option<f64> {
    resolve(&artifact(c.artifact), &c.path)
}

/// Every number the documents declare must equal the artifact's.
#[test]
fn every_declared_number_matches_the_committed_artifact() {
    let claims = declared_claims();
    assert!(
        claims.len() >= 12,
        "only {} claims parsed; the table is not being read",
        claims.len()
    );
    let mut checked = 0;
    for c in &claims {
        let got = resolve_claim(c).unwrap_or_else(|| {
            panic!(
                "{}:{} declares {}:{}, which the artifact does not carry",
                c.doc, c.line, c.artifact, c.path
            )
        });
        // Integers must match exactly; a rounded float must match to the
        // precision it is quoted at, so 0.9964 is checked as 0.9964 and not
        // as "close enough".
        let decimals = 4;
        let scale = 10f64.powi(decimals);
        let rounded = (got * scale).round() / scale;
        assert!(
            (rounded - c.value).abs() < 1e-9,
            "{}:{} quotes {} for {}:{}, the artifact says {} (rounded {})",
            c.doc,
            c.line,
            c.value,
            c.artifact,
            c.path,
            got,
            rounded
        );
        checked += 1;
    }
    println!("{checked} declared numbers agree with the committed artifacts");
}

/// The check is not vacuous: a wrong declaration is caught.
///
/// Without this, "every declared number matches" would pass on a table whose
/// paths all silently failed to resolve — which is the shape of the mistake
/// the whole file exists to prevent.
#[test]
fn a_wrong_declaration_would_be_caught() {
    let corridor = artifact("CORRIDOR_M4.json");
    let real = resolve(&corridor, "arms_measured").expect("the artifact carries it");
    assert!(real > 0.0);
    assert!(
        resolve(&corridor, "arms_measured_typo").is_none(),
        "an unresolvable path must be an error, not a silent skip"
    );
    let held = resolve(&corridor, "held_out.coverage@95").expect("coverage selector works");
    assert!(held > 0.5 && held <= 1.0, "{held}");
    assert!(
        resolve(&corridor, "held_out.coverage@80").is_none(),
        "a level the report does not publish must not resolve"
    );
}

/// The escape-aware split, tested against the row shape that defeated the
/// version before it (condition D2).
///
/// A control in both directions: the escaped row must yield the same cell
/// COUNT as the unescaped one, and an ordinary row must be unaffected.
#[test]
fn the_row_split_honours_the_markdown_escape() {
    let escaped = r"| max \|a - b\| | `corridor:formation_recovery.max_alpha_error` | 0.009 |";
    let plain = "| max abs error | `corridor:formation_recovery.max_alpha_error` | 0.009 |";
    let e = table_cells(escaped).expect("a table row");
    let p = table_cells(plain).expect("a table row");
    assert_eq!(
        e.len(),
        p.len(),
        "the escaped bars produced {} cells against {}: this is M4-N12 verbatim",
        e.len(),
        p.len()
    );
    assert_eq!(e[1], "max |a - b|", "the escape must survive as content");
    assert_eq!(e[2], "`corridor:formation_recovery.max_alpha_error`");
    assert_eq!(e[3], "0.009");
    // A naive split is still wrong, which is what makes this test mean
    // something.
    assert_eq!(
        escaped.split('|').count(),
        7,
        "the naive split must still see seven cells, or this row no longer reproduces the defect"
    );
    assert!(table_cells("prose | with a bar").is_none());
}

/// Numeric tokens of a clause row's evidence column, in order.
fn row_numbers(row: &str) -> Vec<(String, f64)> {
    let cells = table_cells(row).expect("a clause row is a table row");
    let evidence = cells.get(4).cloned().unwrap_or_default();
    let mut out = Vec::new();
    for word in evidence.split(|c: char| c.is_whitespace() || c == '/' || c == '(' || c == ')') {
        let token = word.trim_matches(|c: char| !c.is_ascii_digit());
        if token.is_empty()
            || word
                .chars()
                .any(|c| c.is_alphabetic() || c == '§' || c == '@')
        {
            continue;
        }
        if let Ok(v) = token.parse::<f64>() {
            out.push((token.to_string(), v));
        }
    }
    out
}

/// The positional row specification declared in `REPRODUCIBILITY_M4_5.md`.
///
/// Rows of the form `| T1d | 3 | topology:recall_all.hits |`: row prefix,
/// 1-based position, artifact key.
fn positional_spec() -> Vec<(String, usize, Claim)> {
    let path = repo_root().join("docs/REPRODUCIBILITY_M4_5.md");
    let text = std::fs::read_to_string(&path).expect("REPRODUCIBILITY_M4_5");
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let Some(cells) = table_cells(line) else {
            continue;
        };
        if cells.len() < 5 {
            continue;
        }
        let Some(row) = cells[1].strip_prefix("row ") else {
            continue;
        };
        let Ok(pos) = cells[2].parse::<usize>() else {
            continue;
        };
        let Some(key) = cells[3].strip_prefix('`').and_then(|k| k.strip_suffix('`')) else {
            continue;
        };
        let Some((kind, keypath)) = key.split_once(':') else {
            continue;
        };
        let artifact = artifact_file(kind)
            .unwrap_or_else(|| panic!("REPRODUCIBILITY_M4_5:{}: unknown artifact {kind:?}", i + 1));
        out.push((
            format!("| {row} "),
            pos,
            Claim {
                artifact,
                path: keypath.to_string(),
                value: f64::NAN,
                doc: "docs/REPRODUCIBILITY_M4_5.md",
                line: i + 1,
            },
        ));
    }
    out
}

/// The i-th number of a clause row EQUALS the i-th declared key.
///
/// Membership was not enough and both cold contexts proved it the same way:
/// with 78 declared values, most of them small integers, a wrong measurement
/// can be assembled out of other people's numbers. This binds each position to
/// a key, so the row reports what the artifact says or it fails.
#[test]
fn the_delta_clause_rows_equal_their_declared_keys_position_by_position() {
    let spec = positional_spec();
    assert!(
        spec.len() >= 20,
        "only {} positional bindings declared; the specification table is not being read",
        spec.len()
    );
    let all_rows = gate_table_rows();
    assert!(
        all_rows.len() >= 20,
        "only {} gate-table rows derived ({all_rows:?}); the derivation is not reading the gate          tables and this test would check nothing",
        all_rows.len()
    );
    let doc_text: std::collections::BTreeMap<&str, String> = POSITIONAL_DOCS
        .iter()
        .filter_map(|d| {
            std::fs::read_to_string(repo_root().join(d))
                .ok()
                .map(|t| (*d, t))
        })
        .collect();
    let gate_cell = |prefix: &str, doc: &str| -> String {
        doc_text[doc]
            .lines()
            .find(|l| l.starts_with(prefix))
            .and_then(table_cells)
            .and_then(|c| c.get(2).cloned())
            .unwrap_or_default()
    };

    // PER CLAUSE, not "at least six rows in total". The previous version put
    // this assertion inside `for name in SPEC_CLAUSE_NAMES` and then checked the
    // TOTAL length, so the loop variable was unused and the message promised a
    // per-clause guarantee the code did not make (RT45-A17).
    for name in SPEC_CLAUSE_NAMES {
        let reporting = all_rows
            .iter()
            .filter(|(doc, prefix)| gate_cell(prefix, doc).contains(name))
            .count();
        assert!(
            reporting >= 2,
            "clause {name:?} is reported by {reporting} gate row(s); the milestone table and the              delta table must both carry it, or one of them stopped reporting a spec clause"
        );
    }

    // EVERY row declares exactly one kind. A row with no declaration is the
    // ERROR, not the omission.
    let kinds = declared_row_kinds();
    for (doc, prefix) in &all_rows {
        let mut declared: Vec<&str> = kinds
            .iter()
            .filter(|(r, _)| r == prefix)
            .map(|(_, k)| *k)
            .collect();
        declared.dedup();
        assert_eq!(
            declared.len(),
            1,
            "{doc}: gate row {prefix:?} declares {declared:?}. Every row of a gate table must              have exactly ONE specification in REPRODUCIBILITY_M4_5 - a positional binding per              number, `membership`, or `unit-test`. A row in no tier is checked by nothing"
        );
        // CONDITION 28 / RT45-A22: the kind is DERIVED from what the row
        // contains, not only from what it declares. A row carrying a numeric
        // token is reporting a measurement, so it cannot be `unit-test` - the
        // kind that is checked by nothing. The declaration and the content must
        // agree, and the content is not something a tier downgrade can edit
        // away without also deleting the numbers it wanted to publish.
        let row_line = doc_text[doc]
            .lines()
            .find(|l| l.starts_with(prefix.as_str()))
            .expect("the row exists");
        let numbers = row_numbers(row_line);
        // WHOLE-CELL PARSE, applied to EVERY gate row whatever its tier.
        //
        // RT45-A29 is the fifth form of one attack (A4 -> A11 -> A17 -> A22 ->
        // A29), and every previous answer repaired the TIER rather than what the
        // tier reads. `row_numbers` extracts ASCII digits, so a quantity written
        // any other way is invisible to it: fullwidth `５６`, a vulgar fraction,
        // a Roman numeral. The attack put "recall ５６ из １３２" into `T1` - a
        // POSITIONAL row carrying the verdict of spec clause 1 under PASS - and
        // six mechanisms said nothing, because none of them was looking at the
        // characters that were actually there.
        //
        // So the criterion is not "does this contain a digit" but "is the whole
        // cell accounted for". Every character of the evidence column must be
        // one this file can classify: a letter, whitespace, markup punctuation,
        // or an ASCII digit that the positional binding then consumes. Anything
        // else is UNPARSED, and unparsed is an error rather than a pass. That
        // needs no table of notations and no list of alphabets - `is_numeric`
        // covers every Unicode numeric category, and the catch-all covers what
        // it does not.
        let evidence = table_cells(row_line)
            .and_then(|c| c.get(4).cloned())
            .unwrap_or_default();
        for (i, ch) in evidence.char_indices() {
            // ASCII digits are the notation `row_numbers` parses: it already
            // separates a QUANTITY (`56`) from an identifier that contains
            // digits (`§11.4`, `RT45-A29`, `C217`), and the tier check below
            // uses it. Every OTHER numeric character in Unicode - fullwidth
            // `５６`, a vulgar fraction, a Roman numeral - is a notation this
            // file does not parse, so whatever it says is published under no
            // mechanism at all.
            if ch.is_ascii_digit() || !ch.is_numeric() {
                continue;
            }
            panic!(
                "{doc}: row {prefix:?} carries the UNPARSED numeric character {ch:?} (U+{:04X})                  at byte {i}. The criterion is not \"does this contain a digit\" but \"is the                  whole cell accounted for\": a quantity in a notation nothing reads is published                  under no mechanism, which is how `recall ５６ из １３２` went into a PASS row                  that reports a spec clause (RT45-A29)",
                ch as u32
            );
        }

        // ALL unchecked tiers, not just `unit-test`. Condition 28 covered one
        // of the two and RT45-A22 simply declared the other: `membership` does
        // not bind a number to a key either, it only requires the number to
        // appear SOMEWHERE among the declared values - which, with ninety-five
        // of them and most of them small integers, a forged figure usually
        // does.
        //
        // And the rule is about the QUANTITY, not the glyph. Condition 28 said
        // "a row with a number", I answered by spelling eighteen of them as
        // words, and the reviewer was right that no tier reads words either. A
        // row in an unchecked tier publishes no measured quantity in ANY
        // notation; a quantity that must be published gets a key.
        if declared[0] != "positional" {
            assert!(
                numbers.is_empty(),
                "{doc}: row {prefix:?} is declared `{}` and carries {} numeric token(s) {:?}. A                  row that publishes a measured quantity is reporting a measurement, and neither                  `membership` nor `unit-test` binds it to a key - so the figure would be                  published under no mechanism at all. Bind it positionally or do not publish it                  (condition 28, RT45-A22)",
                declared[0],
                numbers.len(),
                numbers.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>()
            );
        }

        // And the third kind cannot cover the rows the attack aims at.
        if SPEC_CLAUSE_NAMES
            .iter()
            .any(|n| gate_cell(prefix, doc).contains(n))
        {
            assert_eq!(
                declared[0], "positional",
                "{doc}: row {prefix:?} reports a spec clause and is declared {:?}. A row that                  reports a clause carries the numbers a reader takes for the verdict, so it is                  bound position by position or it is not accepted",
                declared[0]
            );
        }
    }
    let not_positional: Vec<String> = kinds
        .iter()
        .filter(|(_, k)| *k != "positional")
        .map(|(r, _)| r.clone())
        .collect();
    assert!(
        !not_positional.is_empty(),
        "no row is declared membership or unit-test, so the branch above never runs"
    );

    let rows_under_check: Vec<(&str, String)> = all_rows
        .iter()
        .filter(|(_, prefix)| !not_positional.contains(prefix))
        .cloned()
        .collect();
    let mut checked = 0;
    for (doc, prefix) in &rows_under_check {
        let doc: &str = doc;
        let prefix: &str = prefix;
        let Ok(text) = std::fs::read_to_string(repo_root().join(doc)) else {
            continue;
        };
        {
            let rows: Vec<&str> = text.lines().filter(|l| l.starts_with(prefix)).collect();
            assert_eq!(
                rows.len(),
                1,
                "{doc}: positional row {prefix:?} found {} times",
                rows.len()
            );
            let numbers = row_numbers(rows[0]);
            let mut keys: Vec<&(String, usize, Claim)> =
                spec.iter().filter(|(r, _, _)| r == prefix).collect();
            keys.sort_by_key(|(_, p, _)| *p);
            assert!(
                !keys.is_empty(),
                "{doc}: row {prefix:?} has no positional specification in REPRODUCIBILITY_M4_5"
            );
            assert_eq!(
                numbers.len(),
                keys.len(),
                "{doc}: row {prefix:?} carries {} numbers and {} are declared. Positional \
                 checking means EVERY number in the row is bound to a key: an unbound one is a \
                 number nobody checks.\nnumbers: {:?}",
                numbers.len(),
                keys.len(),
                numbers.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>()
            );
            for (i, ((token, value), (_, pos, claim))) in numbers.iter().zip(&keys).enumerate() {
                assert_eq!(*pos, i + 1, "positions must be 1..n without gaps");
                let got = resolve_claim(claim).unwrap_or_else(|| {
                    panic!(
                        "REPRODUCIBILITY_M4_5:{} binds {} to {}:{}, which the artifact does not \
                         carry",
                        claim.line, prefix, claim.artifact, claim.path
                    )
                });
                let scale = 10f64.powi(token.split('.').nth(1).map_or(0, |f| f.len()) as i32);
                assert!(
                    ((got * scale).round() / scale - value).abs() < 1e-9,
                    "{doc} row {prefix:?} position {}: the row says {token}, but {}:{} is {got}. \
                     This is the check membership could not make: the number is bound to the key \
                     it claims to report (M45-N11, RT45-A4)",
                    i + 1,
                    claim.artifact,
                    claim.path
                );
                checked += 1;
            }
        }
    }
    assert!(checked >= 20, "only {checked} positional numbers checked");
    println!("{checked} clause-row numbers equal their declared keys position by position");
}

/// The §28 clause rows of every gate table may quote only DECLARED numbers.
///
/// The declared table closed the class for numbers a document chooses to
/// declare. It does not stop a document from quoting a number that is in no
/// table at all — and that is what had happened to the gate table of
/// `STATUS_M4`: after the corridor was recalibrated (F-0027), rows G7, G9 and
/// G10 still carried the previous run's figures, so the section that reports
/// the spec clauses reported them with numbers no artifact held.
///
/// Deliberately narrow, and the boundary is worth stating. A token containing
/// a letter, `§` or `@` is an identifier (`PF10`, `M4.5`, `§1.6`,
/// `coverage@95`), not a measurement, and is skipped; and the rule covers the
/// clause rows, not the document. What becomes impossible is the specific
/// thing that happened twice: the rows that PRESENT the gate verdict drifting
/// from the run that produced it.
///
/// Condition D2's second half is the per-row control below: a row that
/// contributes ZERO checked tokens fails. Previously a row could contribute
/// nothing — because an escape had truncated its evidence column — and the
/// aggregate count still passed on the other three.
#[test]
fn the_status_clause_rows_quote_only_declared_numbers() {
    let claims = declared_claims();
    let declared: Vec<f64> = claims
        .iter()
        .map(|c| resolve_claim(c).unwrap_or(f64::NAN))
        .collect();
    assert!(
        declared.len() >= 12 && declared.iter().all(|v| v.is_finite()),
        "the declared table did not resolve, so this test would compare against nothing"
    );
    // Printed because STATUS_M4_5 §3 and row T11 refer to this number in PROSE,
    // and prose is the one thing this mechanism cannot check (condition 19).
    // The documents spell it out in words rather than digits, so a reader who
    // wants the figure has to take it from here - where it is measured - rather
    // than from a sentence that can go stale in silence.
    println!("{} declared measurements resolve", declared.len());

    let mut tables = 0;
    let mut total = 0;
    for (doc, prefixes) in CLAUSE_ROWS {
        let Ok(text) = std::fs::read_to_string(repo_root().join(doc)) else {
            continue;
        };
        tables += 1;
        for prefix in *prefixes {
            let rows: Vec<&str> = text.lines().filter(|l| l.starts_with(prefix)).collect();
            assert_eq!(
                rows.len(),
                1,
                "{doc}: clause row {prefix:?} was found {} times; the gate table changed shape \
                 and this test would silently check nothing",
                rows.len()
            );
            let row = rows[0];
            let cells = table_cells(row).expect("a clause row is a table row");
            let evidence = cells.get(4).cloned().unwrap_or_default();
            let mut in_row = 0;
            for word in
                evidence.split(|c: char| c.is_whitespace() || c == '/' || c == '(' || c == ')')
            {
                let token = word.trim_matches(|c: char| !c.is_ascii_digit());
                if token.is_empty()
                    || word
                        .chars()
                        .any(|c| c.is_alphabetic() || c == '§' || c == '@')
                {
                    continue;
                }
                let Ok(value) = token.parse::<f64>() else {
                    continue;
                };
                let scale = 10f64.powi(token.split('.').nth(1).map_or(0, |f| f.len()) as i32);
                assert!(
                    declared
                        .iter()
                        .any(|d| ((d * scale).round() / scale - value).abs() < 1e-9),
                    "{doc} clause row {prefix:?} quotes {token}, which is not one of the {} \
                     declared measurements: it is either stale (F-0028) or it needs a row in a \
                     declared table.\nrow: {row}",
                    declared.len()
                );
                in_row += 1;
            }
            assert!(
                in_row > 0,
                "{doc} clause row {prefix:?} contributed ZERO checked tokens. A clause row that \
                 reports a spec clause without a single verified number is the state condition \
                 D2 exists to make impossible.\nrow: {row}"
            );
            total += in_row;
        }
    }
    assert!(tables > 0, "no gate table was found at all");
    println!("{total} numbers across {tables} gate tables are declared measurements");
}

/// Every row of a markdown table has the SAME number of cells as its header.
///
/// Condition 24 / M45-N21. `REQUIREMENTS_TRACEABILITY.md` was brought under this
/// mechanism by condition 10 — with a list of three row prefixes. The row that
/// broke was `M45-17`: it lost the separator between the requirement column and
/// the mechanism column in C189, so the text read "…а не просто не обойдена
/// корпусные типы `pub(crate)`…", six cells where the table has seven. It is
/// also, precisely, the row documenting the BLOCKING condition.
///
/// A file entered "under the mechanism" by three of its rows is entered by three
/// rows. This is the property instead: it is derived from each table's own
/// header, it needs no list of prefixes, and it covers tables that do not exist
/// yet. It is one loop, which is the honest measure of what condition 10 should
/// have cost.
#[test]
fn every_table_row_has_as_many_cells_as_its_header() {
    let docs = [
        "REQUIREMENTS_TRACEABILITY.md",
        "docs/STATUS_M4_5.md",
        "docs/REPRODUCIBILITY_M4_5.md",
        "docs/REPRODUCIBILITY_M4.md",
    ];
    let mut checked = 0;
    for doc in docs {
        let Ok(text) = std::fs::read_to_string(repo_root().join(doc)) else {
            panic!("{doc} is missing");
        };
        let mut header: Option<(usize, usize)> = None; // (line, cells)
        for (i, line) in text.lines().enumerate() {
            let Some(cells) = table_cells(line) else {
                header = None;
                continue;
            };
            // A separator row `|---|---|` both confirms the line above was a
            // header and fixes the table's width.
            let is_sep = cells.len() > 2
                && cells[1..cells.len() - 1]
                    .iter()
                    .all(|c| !c.is_empty() && c.chars().all(|ch| ch == '-' || ch == ':'));
            if is_sep {
                header = Some((i + 1, cells.len()));
                continue;
            }
            let Some((hline, width)) = header else {
                continue;
            };
            assert_eq!(
                cells.len(),
                width,
                "{doc}:{} has {} cells and the table opened at line {hline} has {width}. A row \
                 that lost or gained a separator is a row the table parser reads wrongly, and \
                 every mechanism over that table starts by parsing it (condition D2, F-0029)\nrow: \
                 {line}",
                i + 1,
                cells.len()
            );
            checked += 1;
        }
    }
    assert!(
        checked >= 200,
        "only {checked} table rows checked; the scan is not finding the tables"
    );
    println!("{checked} table rows have the cell count their header declares");
}
