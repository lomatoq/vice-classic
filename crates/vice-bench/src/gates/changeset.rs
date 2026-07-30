//! The §27.7 rule as a predicate over a change set.
//!
//! §27.7 says a gate/config/noise/code table changes in a SEPARATE reviewed
//! commit and a feature PR may not weaken its own gate. Written as a rule it
//! is a request; here it is a pure function that fails, and
//! `gt-corpus gates-check` runs it over `git diff --name-status` for every
//! commit of every pushed range.
//!
//! The parsing is the part that has been wrong twice, so it is written
//! against the CLASS of `--name-status` output forms rather than against the
//! examples that came up: see [`ChangedPath::parse`].

use super::GATE_PATHS;

/// Paths whose modification is a FEATURE change. Kept as prefixes so a new
/// crate or fixture directory is covered without being listed (M-1: the
/// class, not the enumeration).
pub const FEATURE_PATH_PREFIXES: &[&str] = &["crates/", "tests/fixtures/"];

/// How a path was touched, as `git diff --name-status` reports it.
///
/// The distinction matters because §27.7 forbids WEAKENING your own gate,
/// and a gate that did not exist cannot be weakened. Creating the gate file
/// alongside the code that reads it is therefore allowed - explicitly, as a
/// named rule with its own test, not as an oversight (REVIEW_M3 M3-N2).
/// Everything else that touches an existing gate - modification, deletion,
/// rename - is a gate change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Added,
    Other,
}

impl ChangeKind {
    /// Parse a `git diff --name-status` status field.
    ///
    /// EXACTLY `A` exempts. The first version matched on the first
    /// character, so any field beginning with `A` exempted (REVIEW_M3
    /// M3-D2); `R100` was `Other` only by accident of the first letter, and
    /// a hand-written `--changed "Argh<TAB>path"` was an exemption. An
    /// unrecognised status must never become one, so everything else -
    /// including scored statuses, multi-letter merge statuses and the empty
    /// string - is `Other`.
    pub fn from_status(field: &str) -> ChangeKind {
        if field == "A" {
            ChangeKind::Added
        } else {
            ChangeKind::Other
        }
    }
}

/// Does this status field carry a similarity score and a SECOND path?
///
/// `git diff --name-status` emits renames and copies with three columns:
/// `R100<TAB>old<TAB>new`. Everything else has two.
fn is_two_path_status(field: &str) -> bool {
    let mut chars = field.chars();
    matches!(chars.next(), Some('R') | Some('C')) && chars.all(|c| c.is_ascii_digit())
}

/// Undo git's C-style path quoting (`core.quotePath`, on by default).
///
/// A path containing non-ASCII or control characters is emitted as
/// `"crates/\303\251.rs"`. Left quoted, its `crates/` prefix would not
/// match, so a gate change alongside such a file would pass the rule. The
/// gate path itself is plain ASCII and is never quoted, so this matters on
/// the FEATURE side - which is exactly the half a check of the obvious case
/// would have missed.
fn unquote_path(s: &str) -> String {
    let inner = match s.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        Some(v) => v,
        None => return s.to_string(),
    };
    let mut out = Vec::with_capacity(inner.len());
    let bytes = inner.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'\\' || i + 1 >= bytes.len() {
            out.push(bytes[i]);
            i += 1;
            continue;
        }
        let c = bytes[i + 1];
        match c {
            b'0'..=b'7' if i + 3 < bytes.len() => {
                let oct = &inner[i + 1..i + 4];
                match u8::from_str_radix(oct, 8) {
                    Ok(v) => {
                        out.push(v);
                        i += 4;
                    }
                    Err(_) => {
                        out.push(c);
                        i += 2;
                    }
                }
            }
            b'n' => {
                out.push(b'\n');
                i += 2;
            }
            b't' => {
                out.push(b'\t');
                i += 2;
            }
            b'r' => {
                out.push(b'\r');
                i += 2;
            }
            other => {
                out.push(other);
                i += 2;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// An input the parser does not recognize.
///
/// The parser used to fall back to "the whole line is a path" for anything
/// it could not read, which is a SILENT PASS: REVIEW_M3_5 M35-N6 fed it
/// NUL-separated output (`--name-status -z`) and got "no gate/feature
/// co-change in 1 path(s)" and exit 0. A predicate whose failure mode is
/// approval is worse than no predicate, so an unrecognized form is a typed
/// refusal and the caller exits non-zero.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UnrecognizedForm {
    #[error(
        "the change set contains a NUL byte, so it is NUL-separated output (git --name-status -z) \
         and not the line-oriented form this predicate parses. Refusing rather than reading it as \
         one long path: spec 27.7 is a rule whose failure mode must not be approval"
    )]
    NulSeparated,
    #[error(
        "status {status:?} names two paths but the line has {columns} column(s): this is not a \
         form git emits, and guessing at it would be guessing at whether a gate moved"
    )]
    TruncatedRename { status: String, columns: usize },
}

/// One entry of a change set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedPath {
    pub path: String,
    pub kind: ChangeKind,
}

impl ChangedPath {
    pub fn modified(path: impl Into<String>) -> ChangedPath {
        ChangedPath {
            path: path.into(),
            kind: ChangeKind::Other,
        }
    }

    pub fn added(path: impl Into<String>) -> ChangedPath {
        ChangedPath {
            path: path.into(),
            kind: ChangeKind::Added,
        }
    }

    /// Parse one `git diff --name-status` line into the paths it touches.
    ///
    /// Returns a Vec because the output form is not one shape: a rename or a
    /// copy has THREE columns and names TWO paths. REVIEW_M3 M3-D1 measured
    /// what the previous single-path parser did with
    /// `R100<TAB>configs/GATES_V1.toml<TAB>configs/GATES_V2.toml`: it split
    /// on the first whitespace, so the "path" became
    /// `GATES_V1.toml\tGATES_V2.toml`, matched no gate path, and renaming the
    /// gate file away alongside a code change passed the rule. The unit test
    /// that was supposed to cover this used a two-column `R100<TAB>old` form
    /// that git does not emit — the fixture belonged to a subclass where the
    /// defect is unreachable (meta-rule M-2).
    ///
    /// So the parser is written against the CLASS of `--name-status` output
    /// (meta-rule M-1), and `every_name_status_output_form_is_parsed`
    /// enumerates it: the two-column statuses `A D M T U X B`, the
    /// three-column `R<score>` and `C<score>`, quoted paths on either side,
    /// multi-letter merge statuses, and the bare path a hand-written
    /// `--changed` can supply.
    ///
    /// Both paths of a rename or copy are reported, and BOTH as `Other`:
    /// only a literal `A` exempts, and moving a gate file is not creating
    /// one.
    pub fn parse(line: &str) -> Result<Vec<ChangedPath>, UnrecognizedForm> {
        if line.contains('\0') {
            return Err(UnrecognizedForm::NulSeparated);
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.trim().is_empty() {
            return Ok(Vec::new());
        }
        // git separates the columns with TAB. Fall back to whitespace so a
        // hand-written `--changed "M path"` still works, but only when there
        // is no TAB to trust.
        let mut cols: Vec<&str> = if line.contains('\t') {
            line.split('\t').collect()
        } else {
            line.trim().splitn(2, char::is_whitespace).collect()
        };
        cols.retain(|c| !c.trim().is_empty());
        if cols.is_empty() {
            return Ok(Vec::new());
        }
        if cols.len() == 1 {
            // A bare path with no status column: the conservative reading,
            // and a DECLARED form (a hand-written `--changed`), not a
            // fallback for anything unparsed.
            return Ok(vec![ChangedPath::modified(unquote_path(cols[0].trim()))]);
        }
        let status = cols[0].trim();
        let kind = ChangeKind::from_status(status);
        if is_two_path_status(status) {
            if cols.len() < 3 {
                return Err(UnrecognizedForm::TruncatedRename {
                    status: status.to_string(),
                    columns: cols.len(),
                });
            }
            return Ok(vec![
                ChangedPath {
                    path: unquote_path(cols[1].trim()),
                    kind: ChangeKind::Other,
                },
                ChangedPath {
                    path: unquote_path(cols[2].trim()),
                    kind: ChangeKind::Other,
                },
            ]);
        }
        Ok(vec![ChangedPath {
            path: unquote_path(cols[1].trim()),
            kind,
        }])
    }
}

/// The §27.7 rule as a predicate over a change set.
///
/// Returns the offending (gate path, feature path) pair when a single
/// change set MODIFIES a gate and touches production code. Pure, so it can
/// be tested without git, and used by `gt-corpus gates-check` against
/// `git diff --name-status` over the whole pushed range.
pub fn same_commit_violation(changed: &[ChangedPath]) -> Option<(String, String)> {
    same_commit_violation_with_base(changed, &[])
}

/// The §27.7 rule, with the gate paths that ALREADY EXISTED at the base of
/// the push.
///
/// The exemption is "a gate that does not exist cannot be weakened", and
/// judging it against the commit's own parent is not the same question:
/// REVIEW_M3_5 M35-N6 showed a two-commit sequence — delete the gate file in
/// one commit, re-add it with the code in the next — where every commit is
/// individually legal and the frozen value changes anyway. A path that
/// existed at the base of the push is an EXISTING gate for every commit of
/// that push, whatever its own parent says.
pub fn same_commit_violation_with_base(
    changed: &[ChangedPath],
    existing_at_base: &[String],
) -> Option<(String, String)> {
    let existing: Vec<String> = existing_at_base
        .iter()
        .map(|p| p.replace(char::from(92u8), "/"))
        .collect();
    let norm: Vec<ChangedPath> = changed
        .iter()
        .map(|c| {
            let path = c.path.replace(char::from(92u8), "/");
            let kind = if c.kind == ChangeKind::Added && existing.contains(&path) {
                // Created in THIS commit, but the path was already a gate at
                // the base of the push: the exemption does not apply.
                ChangeKind::Other
            } else {
                c.kind
            };
            ChangedPath { path, kind }
        })
        .collect();
    // Only an EXISTING gate can be weakened; creating one is the exemption.
    let gate = norm
        .iter()
        .find(|c| GATE_PATHS.contains(&c.path.as_str()) && c.kind != ChangeKind::Added)?;
    let feature = norm.iter().find(|c| {
        FEATURE_PATH_PREFIXES
            .iter()
            .any(|pre| c.path.starts_with(pre))
    })?;
    Some((gate.path.clone(), feature.path.clone()))
}
#[cfg(test)]
mod tests {
    use super::*;

    /// Status fields are parsed conservatively: EXACTLY `A` exempts.
    ///
    /// The first version looked at the first character only, so anything
    /// beginning with `A` exempted (REVIEW_M3 M3-D2). Unreachable through
    /// git, reachable through a hand-written `--changed`, and an exemption
    /// nobody declared.
    #[test]
    fn only_a_literal_addition_exempts_and_nothing_else_does() {
        assert_eq!(ChangeKind::from_status("A"), ChangeKind::Added);
        for field in [
            "M", "D", "R100", "C75", "T", "U", "X", "B", "", "AM", "MM", "Argh", "a", "A ", " A",
            "R", "C", "A100",
        ] {
            assert_eq!(
                ChangeKind::from_status(field),
                ChangeKind::Other,
                "status {field:?} must not exempt"
            );
        }
    }

    #[test]
    fn structured_gate_provenance_is_protected_like_the_threshold_file() {
        let changed = vec![
            ChangedPath::modified("configs/M7_GATE_PROVENANCE_V1.toml"),
            ChangedPath::modified("crates/vice-bench/src/m7.rs"),
        ];
        let violation = same_commit_violation(&changed).expect("must reject mixed gate/code edit");
        assert_eq!(violation.0, "configs/M7_GATE_PROVENANCE_V1.toml");
    }

    /// EVERY output form of `git diff --name-status`, and what the predicate
    /// makes of it.
    ///
    /// This is the class M3-D1 was found in: the parser handled the
    /// two-column form and the rule's own header claimed rename was covered,
    /// while the three-column form git actually emits for a rename slipped
    /// through with a tab inside the "path". Enumerated here rather than
    /// sampled, so a form that is added later has to be added here too.
    #[test]
    fn every_name_status_output_form_is_parsed() {
        // 1. Two columns, one path, for every status git spells that way.
        for st in ["A", "D", "M", "T", "U", "X", "B"] {
            let got = ChangedPath::parse(&format!("{st}\tconfigs/GATES_V1.toml")).unwrap();
            assert_eq!(got.len(), 1, "{st}");
            assert_eq!(got[0].path, "configs/GATES_V1.toml", "{st}");
            assert_eq!(
                got[0].kind,
                if st == "A" {
                    ChangeKind::Added
                } else {
                    ChangeKind::Other
                },
                "{st}"
            );
        }

        // 2. Three columns for rename and copy: BOTH paths, both `Other`.
        for st in ["R100", "R087", "C75", "C100"] {
            let got = ChangedPath::parse(&format!(
                "{st}\tconfigs/GATES_V1.toml\tconfigs/GATES_V2.toml"
            ))
            .unwrap();
            assert_eq!(got.len(), 2, "{st} names two paths");
            assert_eq!(got[0].path, "configs/GATES_V1.toml");
            assert_eq!(got[1].path, "configs/GATES_V2.toml");
            assert!(got.iter().all(|c| c.kind == ChangeKind::Other), "{st}");
            assert!(
                got.iter().all(|c| !c.path.contains('\t')),
                "{st}: a tab inside a path is how M3-D1 hid"
            );
        }

        // 3. Multi-letter merge statuses do not exempt and do not eat the path.
        let got = ChangedPath::parse("MM\tcrates/vice-bench/src/lib.rs").unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].path, "crates/vice-bench/src/lib.rs");
        assert_eq!(got[0].kind, ChangeKind::Other);

        // 4. Quoted paths: git C-quotes anything with non-ASCII bytes, and a
        // quoted path must still match its prefix.
        let got = ChangedPath::parse("M\t\"crates/vice-bench/src/\\303\\251.rs\"").unwrap();
        assert_eq!(got.len(), 1);
        assert!(
            got[0].path.starts_with("crates/"),
            "a quoted path must be unquoted or its prefix never matches: {:?}",
            got[0].path
        );
        assert!(got[0].path.ends_with("é.rs"), "{:?}", got[0].path);
        let got = ChangedPath::parse("R100\t\"a/\\303\\251.rs\"\t\"b/\\303\\251.rs\"").unwrap();
        assert_eq!(got.len(), 2);
        assert!(got[0].path.starts_with("a/") && got[1].path.starts_with("b/"));

        // 5. A bare path, which only a hand-written `--changed` produces.
        let got = ChangedPath::parse("configs/GATES_V1.toml").unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].kind, ChangeKind::Other);
        assert_eq!(got[0].path, "configs/GATES_V1.toml");
        let got = ChangedPath::parse("A  configs/GATES_V1.toml").unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].path, "configs/GATES_V1.toml");
        assert_eq!(got[0].kind, ChangeKind::Added);

        // 6. Blank input yields nothing.
        for blank in ["", "   ", "\t", "\r\n"] {
            assert!(ChangedPath::parse(blank).unwrap().is_empty(), "{blank:?}");
        }

        // 7. An UNRECOGNIZED form is refused, not read as a path. This is
        // the half that used to be a silent pass (M35-N6).
        assert_eq!(
            ChangedPath::parse("M\0configs/GATES_V1.toml\0"),
            Err(UnrecognizedForm::NulSeparated)
        );
        assert!(matches!(
            ChangedPath::parse("R100\tconfigs/GATES_V1.toml"),
            Err(UnrecognizedForm::TruncatedRename { .. })
        ));
    }

    /// The rule ITSELF, over the whole class: renaming or copying an
    /// EXISTING gate file next to production code is a violation, which is
    /// what the gate file's header has always claimed and what the parser
    /// did not do (M3-D1).
    #[test]
    fn renaming_or_copying_a_gate_file_with_code_is_a_violation() {
        let parse_all = |lines: &[&str]| -> Vec<ChangedPath> {
            lines
                .iter()
                .flat_map(|l| ChangedPath::parse(l).expect("a declared form"))
                .collect()
        };

        // The reviewer's exact repro, which used to exit 0.
        let renamed = parse_all(&[
            "R100\tconfigs/GATES_V1.toml\tconfigs/GATES_V2.toml",
            "M\tcrates/vice-bench/src/lib.rs",
        ]);
        let (gate, feature) = same_commit_violation(&renamed)
            .expect("renaming the gate file alongside code must be a violation");
        assert_eq!(gate, "configs/GATES_V1.toml");
        assert!(feature.starts_with("crates/"));

        // A copy that lands ON the gate path is equally a gate change.
        assert!(same_commit_violation(&parse_all(&[
            "C100\tconfigs/OTHER.toml\tconfigs/GATES_V1.toml",
            "M\tcrates/vice-bench/src/gates.rs",
        ]))
        .is_some());

        // Deletion and typechange too, for the same reason.
        for st in ["D", "M", "T"] {
            assert!(
                same_commit_violation(&parse_all(&[
                    &format!("{st}\tconfigs/GATES_V1.toml"),
                    "M\tcrates/vice-bench/src/lib.rs",
                ]))
                .is_some(),
                "{st} must not exempt"
            );
        }

        // Controls. Renaming an unrelated config with code is fine, and
        // CREATING the gate file with its loader is still the named
        // exemption - the fix must not turn every rename into a violation.
        assert!(same_commit_violation(&parse_all(&[
            "R100\tconfigs/preset_fast.toml\tconfigs/preset_quick.toml",
            "M\tcrates/vice-bench/src/lib.rs",
        ]))
        .is_none());
        assert!(same_commit_violation(&parse_all(&[
            "A\tconfigs/GATES_V1.toml",
            "A\tcrates/vice-bench/src/gates.rs",
        ]))
        .is_none());
    }

    /// The two-commit bypass of REVIEW_M3_5 M35-N6: delete the gate file in
    /// one commit, re-add it with the code in the next. Every commit is
    /// individually legal against its own parent, and the frozen value
    /// changes anyway. Judged against the BASE OF THE PUSH it is a
    /// violation, and judged without a base it is still the named exemption
    /// - which is why the base has to be passed rather than inferred.
    #[test]
    fn re_adding_a_gate_that_existed_at_the_push_base_is_not_a_creation() {
        let readded = [
            ChangedPath::added("configs/GATES_V1.toml"),
            ChangedPath::modified("crates/vice-bench/src/lib.rs"),
        ];
        // Without a base: the commit looks like the C072 exemption.
        assert!(same_commit_violation(&readded).is_none());
        // With the base: the gate existed before the push, so re-adding it
        // beside code is the change §27.7 forbids.
        let base = vec!["configs/GATES_V1.toml".to_string()];
        let (gate, feature) = same_commit_violation_with_base(&readded, &base)
            .expect("re-adding an existing gate with code is a violation");
        assert_eq!(gate, "configs/GATES_V1.toml");
        assert!(feature.starts_with("crates/"));
        // Control: a gate path that did NOT exist at the base keeps the
        // exemption, so a genuinely new gate can still be created with its
        // loader.
        assert!(same_commit_violation_with_base(&readded, &[]).is_none());
        assert!(
            same_commit_violation_with_base(&readded, &["configs/OTHER.toml".to_string()])
                .is_none()
        );
    }

    /// The change set that actually shipped in M3: C072 created the gate
    /// file together with its loader, and the predicate must accept it -
    /// while the same file set with the gate MODIFIED must not.
    #[test]
    fn the_real_c072_change_set_is_accepted_and_its_modified_twin_is_not() {
        let created = [
            ChangedPath::added("configs/GATES_V1.toml"),
            ChangedPath::added("crates/vice-bench/src/gates.rs"),
            ChangedPath::modified("crates/vice-bench/src/lib.rs"),
            ChangedPath::added("crates/vice-bench/src/prereg.rs"),
        ];
        assert!(same_commit_violation(&created).is_none());

        let mut modified = created;
        modified[0] = ChangedPath::modified("configs/GATES_V1.toml");
        assert!(same_commit_violation(&modified).is_some());
    }

    /// The §27.7 rule itself: a change set may not MODIFY a gate and touch
    /// production code together.
    #[test]
    fn changing_a_gate_together_with_code_is_a_violation() {
        let m = ChangedPath::modified;
        let a = ChangedPath::added;

        assert!(same_commit_violation(&[
            m("crates/vice-bench/src/gt/raster.rs"),
            m("docs/STATUS_M3.md"),
        ])
        .is_none());

        assert!(
            same_commit_violation(&[m("configs/GATES_V1.toml"), m("docs/adr/ADR-0018.md")])
                .is_none(),
            "a gate change with documentation is exactly the allowed form"
        );

        let (gate, feature) = same_commit_violation(&[
            m("configs/GATES_V1.toml"),
            m("crates/vice-bench/src/gt/degradation.rs"),
        ])
        .expect("must be a violation");
        assert_eq!(gate, "configs/GATES_V1.toml");
        assert!(feature.starts_with("crates/"));

        // Fixtures are features too: weakening a gate while changing the
        // corpus it is measured on is the same move.
        assert!(same_commit_violation(&[
            m("configs/GATES_V1.toml"),
            m("tests/fixtures/gt/authored/leaf.svg"),
        ])
        .is_some());

        // Windows-style separators must not smuggle a change past the rule.
        assert!(same_commit_violation(&[
            m("configs\\GATES_V1.toml"),
            m("crates\\vice-bench\\src\\lib.rs"),
        ])
        .is_some());

        // The NAMED exemption (REVIEW_M3 M3-N2): creating the gate file
        // alongside its loader is allowed, because §27.7 forbids WEAKENING
        // a gate and a gate that does not exist cannot be weakened. This is
        // the case of C072, which the one-commit-deep CI check never saw.
        assert!(
            same_commit_violation(&[
                a("configs/GATES_V1.toml"),
                a("crates/vice-bench/src/gates.rs"),
                m("crates/vice-bench/src/lib.rs"),
            ])
            .is_none(),
            "creating a gate file with its loader is the named exemption"
        );

        // And the exemption is narrow: once the file EXISTS, touching it
        // with code is a violation again, and deletion is not an addition.
        assert!(same_commit_violation(&[
            m("configs/GATES_V1.toml"),
            a("crates/vice-bench/src/gates.rs"),
        ])
        .is_some());
        assert!(same_commit_violation(&[
            ChangedPath {
                path: "configs/GATES_V1.toml".into(),
                kind: ChangeKind::from_status("D"),
            },
            m("crates/vice-bench/src/lib.rs"),
        ])
        .is_some());
    }
}
