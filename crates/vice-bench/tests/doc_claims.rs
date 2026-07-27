//! Numbers the M4 documents quote, checked against the committed artifacts.
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
//! document quotes are now DECLARED in a table `docs/REPRODUCIBILITY_M4.md`
//! carries, each with the path into the artifact it comes from, and this test
//! resolves every path and compares. A quoted number that drifts from the
//! artifact fails here, the same way a module over 800 lines, an undeclared
//! environment read or a crate without `forbid(unsafe_code)` already do.
//!
//! What it does NOT do: read prose. A sentence elsewhere can still go stale.
//! What it does is make the quantities themselves have one source, so a
//! reviewer can check any of them by running one test rather than by
//! re-deriving them.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn artifact(name: &str) -> serde_json::Value {
    let path = repo_root().join("docs/gt").join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
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

struct Claim {
    artifact: String,
    path: String,
    value: f64,
    line: usize,
}

/// Parse the declared-claims table out of the document.
fn declared_claims() -> Vec<Claim> {
    let path = repo_root().join("docs/REPRODUCIBILITY_M4.md");
    let text = std::fs::read_to_string(&path).expect("REPRODUCIBILITY_M4");
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let cells: Vec<&str> = line.split('|').map(|c| c.trim()).collect();
        // | description | `artifact:path` | value |
        if cells.len() < 5 {
            continue;
        }
        let Some(key) = cells[2].strip_prefix('`').and_then(|k| k.strip_suffix('`')) else {
            continue;
        };
        let Some((art, p)) = key.split_once(':') else {
            continue;
        };
        let Ok(value) = cells[3].parse::<f64>() else {
            continue;
        };
        out.push(Claim {
            artifact: match art {
                "corridor" => "CORRIDOR_M4.json".to_string(),
                "oracle" => "ORACLE_M4.json".to_string(),
                other => panic!("line {}: unknown artifact {other:?}", i + 1),
            },
            path: p.to_string(),
            value,
            line: i + 1,
        });
    }
    out
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
    let corridor = artifact("CORRIDOR_M4.json");
    let oracle = artifact("ORACLE_M4.json");
    let mut checked = 0;
    for c in &claims {
        let root = if c.artifact.starts_with("CORRIDOR") {
            &corridor
        } else {
            &oracle
        };
        let got = resolve(root, &c.path).unwrap_or_else(|| {
            panic!(
                "REPRODUCIBILITY_M4:{} declares {}:{}, which the artifact does not carry",
                c.line, c.artifact, c.path
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
            "REPRODUCIBILITY_M4:{} quotes {} for {}:{}, the artifact says {} (rounded {})",
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

/// The four §28 M4 clause rows of `STATUS_M4` may quote only DECLARED numbers.
///
/// The declared table closed the class for numbers a document chooses to
/// declare. It does not stop a document from quoting a number that is in no
/// table at all — and that is what had happened to the gate table of
/// `STATUS_M4`: after the corridor was recalibrated (F-0027), rows G7, G9 and
/// G10 still carried the previous run's figures, so the section that reports
/// the four spec clauses reported them with numbers no artifact held. Three
/// more instances of F-0028, found by looking, which is how this class keeps
/// being found.
///
/// So those four rows are held to a stronger rule than the rest of the prose:
/// every numeric token in their Evidence column must be one of the values
/// this test resolved out of a committed artifact moments earlier.
///
/// Deliberately narrow, and the boundary is worth stating. A token containing
/// a letter, `§` or `@` is an identifier (`PF10`, `M4.5`, `§1.6`,
/// `coverage@95`), not a measurement, and is skipped; and the rule covers
/// four rows, not the document. A number elsewhere in `STATUS_M4` is still a
/// copy nobody checks. What becomes impossible is the specific thing that
/// happened twice: the rows that PRESENT the gate verdict drifting from the
/// run that produced it.
#[test]
fn the_status_clause_rows_quote_only_declared_numbers() {
    let text = std::fs::read_to_string(repo_root().join("docs/STATUS_M4.md")).expect("STATUS_M4");
    let corridor = artifact("CORRIDOR_M4.json");
    let oracle = artifact("ORACLE_M4.json");
    let declared: Vec<f64> = declared_claims()
        .iter()
        .map(|c| {
            let root = if c.artifact.starts_with("CORRIDOR") {
                &corridor
            } else {
                &oracle
            };
            resolve(root, &c.path).unwrap_or(f64::NAN)
        })
        .collect();
    assert!(
        declared.len() >= 12 && declared.iter().all(|v| v.is_finite()),
        "the declared table did not resolve, so this test would compare against nothing"
    );

    let rows: Vec<&str> = text
        .lines()
        .filter(|l| {
            ["| G7 ", "| G8 ", "| G9 ", "| G10 "]
                .iter()
                .any(|p| l.starts_with(p))
        })
        .collect();
    assert_eq!(
        rows.len(),
        4,
        "the four clause rows were not found; the gate table changed shape and this test would \
         silently check nothing"
    );

    let mut checked = 0;
    for row in rows {
        let evidence = row.split('|').nth(4).unwrap_or_default();
        for word in evidence.split(|c: char| c.is_whitespace() || c == '/' || c == '(' || c == ')')
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
                "STATUS_M4 clause row quotes {token}, which is not one of the {} declared \
                 measurements: it is either stale (F-0028) or it needs a row in the declared \
                 table of REPRODUCIBILITY_M4.\nrow: {row}",
                declared.len()
            );
            checked += 1;
        }
    }
    assert!(
        checked >= 12,
        "only {checked} numbers scanned across the four clause rows; the scan is not reaching them"
    );
    println!("{checked} numbers in the STATUS clause rows are declared measurements");
}
