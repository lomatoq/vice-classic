//! Frozen gates and the §27.7 rule, enforced (spec §27.7, §32 rule 9).
//!
//! §27.7 says gate/config/noise/code tables change in a SEPARATE reviewed
//! commit and a feature PR may not weaken its own gate. Written as a rule
//! it is a request; written here it is a check that fails.
//!
//! Two mechanisms:
//!
//! - [`GatesFile`] loads `configs/GATES_V1.toml`, hashes it, and
//!   distinguishes FROZEN entries from PLACEHOLDERS. A placeholder is not a
//!   threshold — it names the milestone that will set it — and
//!   [`GatesFile::gate_value`] refuses to return one, so nothing can gate
//!   on a number nobody has justified.
//! - [`same_commit_violation`] is the §27.7 rule itself: a change set that
//!   touches a gate file AND production code is a violation. CI runs it on
//!   every push, so the rule is not a matter of remembering.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::hashing::sha256_file;

pub const GATES_SCHEMA: &str = "vice-classic/gates/v1";

/// Paths whose modification is a GATE change.
pub const GATE_PATHS: &[&str] = &["configs/GATES_V1.toml"];

/// Paths whose modification is a FEATURE change. Kept as prefixes so a new
/// crate or fixture directory is covered without being listed (M-1: the
/// class, not the enumeration).
pub const FEATURE_PATH_PREFIXES: &[&str] = &["crates/", "tests/fixtures/"];

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct GateSection {
    pub status: String,
    #[serde(default)]
    pub set_by_milestone: Option<String>,
    /// Everything else in the section, as declared.
    #[serde(flatten)]
    pub values: BTreeMap<String, toml::Value>,
}

impl GateSection {
    pub fn is_frozen(&self) -> bool {
        self.status == "frozen"
    }
    pub fn is_placeholder(&self) -> bool {
        self.status == "placeholder"
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct GatesDoc {
    pub schema: String,
    pub version: String,
    #[serde(flatten)]
    pub sections: BTreeMap<String, GateSection>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GatesFile {
    pub doc: GatesDoc,
    pub sha256: String,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum GateError {
    #[error("cannot read {path}: {detail}")]
    Io { path: String, detail: String },
    #[error("cannot parse {path}: {detail}")]
    Parse { path: String, detail: String },
    #[error("gates file has schema {got:?}, expected {want:?}")]
    WrongSchema { got: String, want: String },
    #[error("section {section:?} has status {status:?}; expected 'frozen' or 'placeholder'")]
    BadStatus { section: String, status: String },
    #[error("section {section:?} is a PLACEHOLDER (set by {milestone}): it is not a threshold and nothing may gate on it")]
    PlaceholderUsedAsGate { section: String, milestone: String },
    #[error("section {section:?} has no value {key:?}")]
    MissingValue { section: String, key: String },
}

impl GatesFile {
    pub fn load(path: &Path) -> Result<GatesFile, GateError> {
        let text = std::fs::read_to_string(path).map_err(|e| GateError::Io {
            path: path.display().to_string(),
            detail: e.to_string(),
        })?;
        let doc: GatesDoc = toml::from_str(&text).map_err(|e| GateError::Parse {
            path: path.display().to_string(),
            detail: e.to_string(),
        })?;
        if doc.schema != GATES_SCHEMA {
            return Err(GateError::WrongSchema {
                got: doc.schema,
                want: GATES_SCHEMA.to_string(),
            });
        }
        for (name, s) in &doc.sections {
            if !s.is_frozen() && !s.is_placeholder() {
                return Err(GateError::BadStatus {
                    section: name.clone(),
                    status: s.status.clone(),
                });
            }
            if s.is_placeholder() && s.set_by_milestone.is_none() {
                return Err(GateError::BadStatus {
                    section: name.clone(),
                    status: "placeholder without set_by_milestone".to_string(),
                });
            }
        }
        let sha256 = sha256_file(path).map_err(|e| GateError::Io {
            path: path.display().to_string(),
            detail: e.to_string(),
        })?;
        Ok(GatesFile { doc, sha256 })
    }

    /// Read a FROZEN gate value. A placeholder is refused: gating on a
    /// number no measurement supports is exactly the failure §27.7 and
    /// F-0010 are about.
    pub fn gate_value(&self, section: &str, key: &str) -> Result<&toml::Value, GateError> {
        let s = self
            .doc
            .sections
            .get(section)
            .ok_or_else(|| GateError::MissingValue {
                section: section.to_string(),
                key: key.to_string(),
            })?;
        if s.is_placeholder() {
            return Err(GateError::PlaceholderUsedAsGate {
                section: section.to_string(),
                milestone: s
                    .set_by_milestone
                    .clone()
                    .unwrap_or_else(|| "unknown".into()),
            });
        }
        s.values.get(key).ok_or_else(|| GateError::MissingValue {
            section: section.to_string(),
            key: key.to_string(),
        })
    }

    pub fn frozen_sections(&self) -> Vec<&str> {
        self.doc
            .sections
            .iter()
            .filter(|(_, s)| s.is_frozen())
            .map(|(n, _)| n.as_str())
            .collect()
    }

    pub fn placeholder_sections(&self) -> Vec<&str> {
        self.doc
            .sections
            .iter()
            .filter(|(_, s)| s.is_placeholder())
            .map(|(n, _)| n.as_str())
            .collect()
    }
}

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
    /// Parse a `git diff --name-status` status letter. Unknown letters are
    /// `Other`: an unrecognised status must not become an exemption.
    pub fn from_status(letter: &str) -> ChangeKind {
        match letter.chars().next() {
            Some('A') => ChangeKind::Added,
            _ => ChangeKind::Other,
        }
    }
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

    /// Parse one `git diff --name-status` line. A bare path with no status
    /// column is treated as a modification, which is the conservative
    /// reading.
    pub fn parse(line: &str) -> Option<ChangedPath> {
        let line = line.trim();
        if line.is_empty() {
            return None;
        }
        match line.split_once(char::is_whitespace) {
            Some((status, rest)) if status.len() <= 4 && !rest.trim().is_empty() => {
                Some(ChangedPath {
                    path: rest.trim().to_string(),
                    kind: ChangeKind::from_status(status),
                })
            }
            _ => Some(ChangedPath::modified(line)),
        }
    }
}

/// The §27.7 rule as a predicate over a change set.
///
/// Returns the offending (gate path, feature path) pair when a single
/// change set MODIFIES a gate and touches production code. Pure, so it can
/// be tested without git, and used by `gt-corpus gates-check` against
/// `git diff --name-status` over the whole pushed range.
pub fn same_commit_violation(changed: &[ChangedPath]) -> Option<(String, String)> {
    let norm: Vec<ChangedPath> = changed
        .iter()
        .map(|c| ChangedPath {
            path: c.path.replace('\\', "/"),
            kind: c.kind,
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

    fn gates_path() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../configs/GATES_V1.toml")
    }

    #[test]
    fn the_committed_gates_file_loads_and_separates_frozen_from_placeholder() {
        let g = GatesFile::load(&gates_path()).expect("committed gates file must load");
        assert_eq!(g.doc.schema, GATES_SCHEMA);
        assert_eq!(g.sha256.len(), 64);
        let frozen = g.frozen_sections();
        let placeholders = g.placeholder_sections();
        assert!(frozen.contains(&"reliability"));
        assert!(frozen.contains(&"identifiability"));
        assert!(frozen.contains(&"corpus_instruments"));
        assert!(frozen.contains(&"split"));
        assert!(frozen.contains(&"likelihood"));
        assert!(
            placeholders.contains(&"boundary_accuracy")
                && placeholders.contains(&"geometry_code_table")
                && placeholders.contains(&"noise_scales"),
            "the tables M3 cannot set must be declared as placeholders, not invented"
        );
    }

    /// A placeholder is not a threshold. Reading one as a gate is refused,
    /// which is what stops a later milestone from quietly gating on a zero.
    #[test]
    fn a_placeholder_cannot_be_used_as_a_gate() {
        let g = GatesFile::load(&gates_path()).unwrap();
        assert!(g.gate_value("reliability", "confidence").is_ok());
        match g.gate_value("boundary_accuracy", "p95_px") {
            Err(GateError::PlaceholderUsedAsGate { milestone, .. }) => {
                assert_eq!(milestone, "M7")
            }
            other => panic!("a placeholder must be refused, got {other:?}"),
        }
    }

    /// The frozen numbers must agree with the code that measured them.
    /// A gates file that drifted from the implementation would be a
    /// contract nobody is under.
    #[test]
    fn the_frozen_numbers_agree_with_the_code_that_produced_them() {
        let g = GatesFile::load(&gates_path()).unwrap();
        let f = |s: &str, k: &str| g.gate_value(s, k).unwrap().as_float().unwrap();
        let i = |s: &str, k: &str| g.gate_value(s, k).unwrap().as_integer().unwrap();

        assert_eq!(f("reliability", "confidence"), 0.99);
        assert_eq!(f("reliability", "catastrophic_risk_target"), 0.01);
        assert_eq!(
            i("reliability", "min_accepted_source_groups_zero_failures") as u64,
            crate::reliability::required_groups_for_zero_failures(0.99, 0.01),
            "the frozen sample size must be the DERIVED one"
        );
        assert_eq!(
            f("identifiability", "observability_floor_px"),
            crate::gt::degradation::OBSERVABILITY_FLOOR_PX
        );
        assert_eq!(
            i("split", "development_pct") as u32,
            crate::gt::split::SPLIT_POLICY_V1.development_pct
        );
        assert_eq!(
            i("split", "sealed_audit_pct") as u32,
            crate::gt::split::SPLIT_POLICY_V1.sealed_audit_pct
        );
        let prereg = crate::prereg::Preregistration::v1();
        assert_eq!(f("reliability", "confidence"), prereg.confidence);
        assert_eq!(
            f("reliability", "catastrophic_risk_target"),
            prereg.risk_target
        );
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

    /// Status letters are parsed conservatively: only `A` exempts, and an
    /// unrecognised or absent letter is a modification.
    #[test]
    fn only_addition_exempts_and_unknown_statuses_do_not() {
        assert_eq!(ChangeKind::from_status("A"), ChangeKind::Added);
        for letter in ["M", "D", "R100", "C75", "T", "U", "X", ""] {
            assert_eq!(
                ChangeKind::from_status(letter),
                ChangeKind::Other,
                "status {letter:?} must not exempt"
            );
        }
        // A bare path (no status column) is a modification.
        let p = ChangedPath::parse("configs/GATES_V1.toml").unwrap();
        assert_eq!(p.kind, ChangeKind::Other);
        assert_eq!(p.path, "configs/GATES_V1.toml");
        for line in [
            "A\tconfigs/GATES_V1.toml",
            "A  configs/GATES_V1.toml",
            "R100\tconfigs/OLD.toml",
        ] {
            let p = ChangedPath::parse(line).unwrap();
            assert!(!p.path.contains('\t'), "{line:?} -> {p:?}");
            assert!(!p.path.starts_with(' '));
        }
        assert!(ChangedPath::parse("   ").is_none());
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
}
