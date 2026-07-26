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
    pub fn parse(line: &str) -> Vec<ChangedPath> {
        let line = line.trim_end_matches(['\r', '\n']);
        if line.trim().is_empty() {
            return Vec::new();
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
            return Vec::new();
        }
        if cols.len() == 1 {
            // A bare path with no status column: the conservative reading.
            return vec![ChangedPath::modified(unquote_path(cols[0].trim()))];
        }
        let status = cols[0].trim();
        let kind = ChangeKind::from_status(status);
        if is_two_path_status(status) && cols.len() >= 3 {
            return vec![
                ChangedPath {
                    path: unquote_path(cols[1].trim()),
                    kind: ChangeKind::Other,
                },
                ChangedPath {
                    path: unquote_path(cols[2].trim()),
                    kind: ChangeKind::Other,
                },
            ];
        }
        vec![ChangedPath {
            path: unquote_path(cols[1].trim()),
            kind,
        }]
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

    /// EVERY frozen value must have a consumer in the code, and every one
    /// must agree with it.
    ///
    /// The first version of this test checked six values by name. REVIEW_M3
    /// M3-N3 measured the consequence: eleven of seventeen frozen values -
    /// including the whole `[corpus_instruments]` section, whose numbers
    /// were MEASURED in C066 - were read by nobody and compared by nothing.
    /// The live thresholds were literals in the code, so `worst_super <
    /// 0.06` could have been relaxed to `< 0.2` without touching the gate
    /// file, without moving the gates hash, without tripping the §27.7
    /// predicate and without the burn policy noticing.
    ///
    /// That is meta-rule M-1 exactly - a rule applied to the enumeration of
    /// six addresses instead of the class of seventeen - inside the
    /// milestone that wrote M-1 down. So the test now walks the CLASS: every
    /// frozen section, every key, no name written twice.
    #[test]
    fn every_frozen_value_agrees_with_the_code_that_uses_it() {
        let g = GatesFile::load(&gates_path()).unwrap();
        let expected = frozen_values_from_code();

        // 1. Every frozen key in the file is claimed by the code.
        let mut unclaimed = Vec::new();
        for section in g.frozen_sections() {
            for key in g.doc.sections[section].values.keys() {
                if !expected.iter().any(|(s, k, _)| *s == section && k == key) {
                    unclaimed.push(format!("{section}.{key}"));
                }
            }
        }
        assert!(
            unclaimed.is_empty(),
            "frozen values that nothing in the code reads or checks: {unclaimed:?}. \
             A value with no consumer is not a gate - give it one, or stop calling it frozen."
        );

        // 2. Every claimed key exists in the file and matches.
        for (section, key, want) in &expected {
            let got = g
                .gate_value(section, key)
                .unwrap_or_else(|e| panic!("{section}.{key}: {e}"));
            assert_eq!(
                &GateExpectation::of(got),
                want,
                "{section}.{key}: the gate file and the code disagree"
            );
        }

        // 3. And the walk is not vacuous: it must have covered every frozen
        // section and a plausible number of keys.
        let covered: std::collections::BTreeSet<&str> =
            expected.iter().map(|(s, _, _)| *s).collect();
        let frozen: std::collections::BTreeSet<&str> = g.frozen_sections().into_iter().collect();
        assert_eq!(covered, frozen, "a frozen section was skipped entirely");
        assert!(
            expected.len() >= 17,
            "only {} frozen values checked; the file had 17 when this was written",
            expected.len()
        );
    }

    /// How a frozen value is compared, without caring which TOML type the
    /// author reached for.
    #[derive(Debug, PartialEq)]
    enum GateExpectation {
        Num(f64),
        Text(String),
        List(Vec<String>),
    }

    impl GateExpectation {
        fn of(v: &toml::Value) -> GateExpectation {
            match v {
                toml::Value::Float(f) => GateExpectation::Num(*f),
                toml::Value::Integer(i) => GateExpectation::Num(*i as f64),
                toml::Value::String(s) => GateExpectation::Text(s.clone()),
                toml::Value::Array(a) => GateExpectation::List(
                    a.iter()
                        .map(|x| x.as_str().unwrap_or_default().to_string())
                        .collect(),
                ),
                other => GateExpectation::Text(other.to_string()),
            }
        }

        fn num(v: f64) -> GateExpectation {
            GateExpectation::Num(v)
        }
        fn text(v: &str) -> GateExpectation {
            GateExpectation::Text(v.to_string())
        }
        fn list(v: &[&str]) -> GateExpectation {
            GateExpectation::List(v.iter().map(|s| (*s).to_string()).collect())
        }
    }

    /// The frozen values AS THE CODE HAS THEM. Each entry pulls the value
    /// from the constant or the measurement bound that actually governs
    /// behaviour, so a relaxed literal shows up here as a mismatch.
    fn frozen_values_from_code() -> Vec<(&'static str, String, GateExpectation)> {
        use crate::correlation::ResidualModel;
        use crate::gt::degradation as deg;
        use crate::gt::split::SPLIT_POLICY_V1;
        use crate::prereg::Preregistration;

        let prereg = Preregistration::v1();
        let admissible: Vec<&str> = ResidualModel::ALL
            .iter()
            .filter(|m| m.admissible_for_confidence())
            .map(|m| m.id())
            .collect();
        let diagnostic: Vec<&str> = ResidualModel::ALL
            .iter()
            .filter(|m| !m.admissible_for_confidence())
            .map(|m| m.id())
            .collect();

        let v: Vec<(&'static str, &'static str, GateExpectation)> = vec![
            // --- reliability: the statistical court -------------------
            (
                "reliability",
                "confidence",
                GateExpectation::num(prereg.confidence),
            ),
            (
                "reliability",
                "catastrophic_risk_target",
                GateExpectation::num(prereg.risk_target),
            ),
            (
                "reliability",
                "min_accepted_source_groups_zero_failures",
                GateExpectation::num(crate::reliability::required_groups_for_zero_failures(
                    prereg.confidence,
                    prereg.risk_target,
                ) as f64),
            ),
            (
                "reliability",
                "unit_of_trial",
                GateExpectation::text(crate::reliability::UNIT_OF_TRIAL),
            ),
            // --- corpus instruments: the C066 measurements ------------
            (
                "corpus_instruments",
                "supersample_max_abs",
                GateExpectation::num(crate::gt::raster::SUPERSAMPLE_MAX_ABS_GATE),
            ),
            (
                "corpus_instruments",
                "supersample_edge_mean_abs",
                GateExpectation::num(crate::gt::raster::SUPERSAMPLE_EDGE_MEAN_ABS_GATE),
            ),
            (
                "corpus_instruments",
                "vice_render_max_abs",
                GateExpectation::num(crate::gt::raster::VICE_RENDER_MAX_ABS_GATE),
            ),
            (
                "corpus_instruments",
                "tiny_skia_max_abs",
                GateExpectation::num(crate::gt::raster::EXTERNAL_ENGINE_MAX_ABS_GATE),
            ),
            (
                "corpus_instruments",
                "raqote_max_abs",
                GateExpectation::num(crate::gt::raster::EXTERNAL_ENGINE_MAX_ABS_GATE),
            ),
            // --- identifiability: the C067 calibration ----------------
            (
                "identifiability",
                "observability_floor_px",
                GateExpectation::num(deg::OBSERVABILITY_FLOOR_PX),
            ),
            (
                "identifiability",
                "rival_indistinguishable_codes",
                GateExpectation::num(f64::from(deg::RIVAL_INDISTINGUISHABLE_CODES)),
            ),
            (
                "identifiability",
                "quantization_floor_codes",
                GateExpectation::num(deg::QUANTIZATION_FLOOR_CODES),
            ),
            // --- split -------------------------------------------------
            (
                "split",
                "policy_version",
                GateExpectation::text(SPLIT_POLICY_V1.version),
            ),
            (
                "split",
                "development_pct",
                GateExpectation::num(f64::from(SPLIT_POLICY_V1.development_pct)),
            ),
            (
                "split",
                "calibration_pct",
                GateExpectation::num(f64::from(SPLIT_POLICY_V1.calibration_pct)),
            ),
            (
                "split",
                "sealed_audit_pct",
                GateExpectation::num(f64::from(SPLIT_POLICY_V1.sealed_audit_pct)),
            ),
            (
                "split",
                "unit_of_assignment",
                GateExpectation::text(crate::gt::split::UNIT_OF_ASSIGNMENT),
            ),
            (
                "split",
                "held_out_profiles",
                GateExpectation::list(SPLIT_POLICY_V1.held_out_profiles),
            ),
            // --- likelihood --------------------------------------------
            (
                "likelihood",
                "allowed_production_residual_models",
                GateExpectation::list(&admissible),
            ),
            (
                "likelihood",
                "diagnostic_only_residual_models",
                GateExpectation::list(&diagnostic),
            ),
        ];
        v.into_iter()
            .map(|(s, k, e)| (s, k.to_string(), e))
            .collect()
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
            let got = ChangedPath::parse(&format!("{st}\tconfigs/GATES_V1.toml"));
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
            ));
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
        let got = ChangedPath::parse("MM\tcrates/vice-bench/src/lib.rs");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].path, "crates/vice-bench/src/lib.rs");
        assert_eq!(got[0].kind, ChangeKind::Other);

        // 4. Quoted paths: git C-quotes anything with non-ASCII bytes, and a
        // quoted path must still match its prefix.
        let got = ChangedPath::parse("M\t\"crates/vice-bench/src/\\303\\251.rs\"");
        assert_eq!(got.len(), 1);
        assert!(
            got[0].path.starts_with("crates/"),
            "a quoted path must be unquoted or its prefix never matches: {:?}",
            got[0].path
        );
        assert!(got[0].path.ends_with("é.rs"), "{:?}", got[0].path);
        let got = ChangedPath::parse("R100\t\"a/\\303\\251.rs\"\t\"b/\\303\\251.rs\"");
        assert_eq!(got.len(), 2);
        assert!(got[0].path.starts_with("a/") && got[1].path.starts_with("b/"));

        // 5. A bare path, which only a hand-written `--changed` produces.
        let got = ChangedPath::parse("configs/GATES_V1.toml");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].kind, ChangeKind::Other);
        assert_eq!(got[0].path, "configs/GATES_V1.toml");
        let got = ChangedPath::parse("A  configs/GATES_V1.toml");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].path, "configs/GATES_V1.toml");
        assert_eq!(got[0].kind, ChangeKind::Added);

        // 6. Blank input yields nothing.
        for blank in ["", "   ", "\t", "\r\n"] {
            assert!(ChangedPath::parse(blank).is_empty(), "{blank:?}");
        }
    }

    /// The rule ITSELF, over the whole class: renaming or copying an
    /// EXISTING gate file next to production code is a violation, which is
    /// what the gate file's header has always claimed and what the parser
    /// did not do (M3-D1).
    #[test]
    fn renaming_or_copying_a_gate_file_with_code_is_a_violation() {
        let parse_all = |lines: &[&str]| -> Vec<ChangedPath> {
            lines.iter().flat_map(|l| ChangedPath::parse(l)).collect()
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
