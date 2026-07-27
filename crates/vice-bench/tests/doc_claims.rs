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
//! What it still does NOT do: read prose. A sentence elsewhere can go stale.
//! What it does is make the quantities themselves have one source.

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
const CLAUSE_ROWS: &[(&str, &[&str])] = &[
    ("docs/STATUS_M4.md", &["| G7 ", "| G8 ", "| G9 ", "| G10 "]),
    (
        "docs/STATUS_M4_5.md",
        &["| T1 ", "| T2 ", "| T3 "],
    ),
];

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
    for doc in DECLARING_DOCS {
        let path = repo_root().join(doc);
        let Ok(text) = std::fs::read_to_string(&path) else {
            // A milestone that has not written its document yet is not an
            // error; a document that exists and is unparsable is.
            continue;
        };
        for (i, line) in text.lines().enumerate() {
            let carries_key = KEY_PREFIXES.iter().any(|k| line.contains(k));
            if carries_key {
                key_bearing += 1;
            }
            let Some(cells) = table_cells(line) else {
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
