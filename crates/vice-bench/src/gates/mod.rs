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

//!
//! Split into two files because it outgrew the §4.1 size rule while the
//! §27.7 predicate was being widened to the whole class of
//! `git diff --name-status` output forms (C095). The seam is the one the
//! module doc already describes: the frozen gates FILE here, the change-set
//! PREDICATE in [`changeset`]. Both are re-exported, so `gates::…` keeps
//! naming the same items.

pub mod changeset;

pub use changeset::{
    same_commit_violation, same_commit_violation_with_base, ChangeKind, ChangedPath,
    UnrecognizedForm, FEATURE_PATH_PREFIXES,
};

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::hashing::sha256_file;

pub const GATES_SCHEMA: &str = "vice-classic/gates/v1";

/// Paths whose modification is a GATE change.
pub const GATE_PATHS: &[&str] = &["configs/GATES_V1.toml"];

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

#[cfg(test)]
mod tests {
    use super::*;

    fn gates_path() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../configs/GATES_V1.toml")
    }

    /// Every section is one of the two kinds, and each kind is populated.
    ///
    /// Written as a property of the CLASS rather than as a list of section
    /// names: a milestone that MEASURES a placeholder is supposed to freeze
    /// it (M4 froze `noise_scales`), and a test that names the placeholders
    /// one by one turns that intended transition into a failure — which is
    /// meta-rule M-1 in the file whose whole subject is frozen values. What
    /// must stay true is the structure: nothing is half-declared, every
    /// placeholder names its owner, and both kinds still exist.
    #[test]
    fn the_committed_gates_file_loads_and_separates_frozen_from_placeholder() {
        let g = GatesFile::load(&gates_path()).expect("committed gates file must load");
        assert_eq!(g.doc.schema, GATES_SCHEMA);
        assert_eq!(g.sha256.len(), 64);
        let frozen = g.frozen_sections();
        let placeholders = g.placeholder_sections();
        assert_eq!(
            frozen.len() + placeholders.len(),
            g.doc.sections.len(),
            "a section is frozen or a placeholder; there is no third state"
        );
        assert!(frozen.len() >= 5, "frozen sections: {frozen:?}");
        assert!(
            !placeholders.is_empty(),
            "the tables no measurement supports yet must be DECLARED, not invented"
        );
        for p in &placeholders {
            let s = &g.doc.sections[*p];
            assert!(
                s.set_by_milestone.as_deref().is_some_and(|m| !m.is_empty()),
                "placeholder {p} names no owner"
            );
            assert!(
                g.gate_value(p, "any").is_err(),
                "a placeholder must not be readable as a gate"
            );
        }
        // The frozen sections M3 established are still frozen: a later
        // milestone may ADD to this set, never remove from it.
        for s in [
            "reliability",
            "identifiability",
            "corpus_instruments",
            "split",
            "likelihood",
        ] {
            assert!(frozen.contains(&s), "{s} stopped being frozen");
        }
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

        // 2. Every claimed key of a FROZEN section exists in the file and
        // matches.
        //
        // A claim about a PLACEHOLDER section is inert: a placeholder is not
        // a threshold and `gate_value` refuses to return one. Claims are
        // allowed to exist before the section is frozen, because the code
        // side has to land first — §27.7 forbids changing a gate file and
        // production code in one commit, so the constant and its claim are
        // one commit and the freeze is the next.
        let frozen: std::collections::BTreeSet<&str> = g.frozen_sections().into_iter().collect();
        let mut checked = 0;
        for (section, key, want) in &expected {
            if !frozen.contains(section) {
                continue;
            }
            let got = g
                .gate_value(section, key)
                .unwrap_or_else(|e| panic!("{section}.{key}: {e}"));
            assert_eq!(
                &GateExpectation::of(got),
                want,
                "{section}.{key}: the gate file and the code disagree"
            );
            checked += 1;
        }

        // 3. And the walk is not vacuous: it must have covered every frozen
        // section and a plausible number of keys.
        let covered: std::collections::BTreeSet<&str> = expected
            .iter()
            .map(|(s, _, _)| *s)
            .filter(|s| frozen.contains(s))
            .collect();
        assert_eq!(covered, frozen, "a frozen section was skipped entirely");
        assert!(
            checked >= 17,
            "only {checked} frozen values checked; the file had 17 when this was written"
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
        use crate::topology::report as topo;

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
            // --- topology envelope and its gate thresholds (M4.5) -----
            //
            // Every constant that decides which candidates exist, and every
            // threshold that decides whether a §28 M4.5 clause is green.
            // M45-N6 / RT45-A5: without these §27.7 had nothing to act on for
            // this milestone, and one commit could relax a clause and change
            // the code that meets it.
            (
                "topology",
                "field_tv_iterations",
                GateExpectation::num(f64::from(vice_topology::FIELD_CONFIG_V1.tv_iterations)),
            ),
            (
                "topology",
                "field_tv_step",
                GateExpectation::num(vice_topology::FIELD_CONFIG_V1.tv_step),
            ),
            (
                "topology",
                "field_tv_huber_delta",
                GateExpectation::num(vice_topology::FIELD_CONFIG_V1.tv_huber_delta),
            ),
            (
                "topology",
                "field_tv_data_weight",
                GateExpectation::num(vice_topology::FIELD_CONFIG_V1.tv_data_weight),
            ),
            (
                "topology",
                "field_deconv_iterations",
                GateExpectation::num(f64::from(vice_topology::FIELD_CONFIG_V1.deconv_iterations)),
            ),
            (
                "topology",
                "field_deconv_step",
                GateExpectation::num(vice_topology::FIELD_CONFIG_V1.deconv_step),
            ),
            (
                "topology",
                "level_max_plateau_levels",
                GateExpectation::num(vice_topology::LEVEL_CONFIG_V1.max_plateau_levels as f64),
            ),
            (
                "topology",
                "level_max_event_levels",
                GateExpectation::num(vice_topology::LEVEL_CONFIG_V1.max_event_levels as f64),
            ),
            (
                "topology",
                "level_min_event_persistence",
                GateExpectation::num(vice_topology::LEVEL_CONFIG_V1.min_event_persistence),
            ),
            (
                "topology",
                "level_fixed_smoke_count",
                GateExpectation::num(vice_topology::LEVEL_CONFIG_V1.fixed_smoke_levels.len() as f64),
            ),
            (
                "topology",
                "level_fixed_smoke_first",
                GateExpectation::num(vice_topology::LEVEL_CONFIG_V1.fixed_smoke_levels[0]),
            ),
            (
                "topology",
                "envelope_budget",
                GateExpectation::num(vice_topology::ENVELOPE_CONFIG_V1.budget as f64),
            ),
            (
                "topology",
                "envelope_per_quota_class",
                GateExpectation::num(vice_topology::ENVELOPE_CONFIG_V1.per_quota_class as f64),
            ),
            (
                "topology",
                "envelope_mass_scale",
                GateExpectation::num(vice_topology::ENVELOPE_CONFIG_V1.mass_scale),
            ),
            (
                "topology",
                "continuation_halo_px",
                GateExpectation::num(f64::from(vice_topology::CONTINUATION_CONFIG_V1.halo_px)),
            ),
            (
                "topology",
                "continuation_max_plans",
                GateExpectation::num(vice_topology::CONTINUATION_CONFIG_V1.max_plans as f64),
            ),
            (
                "topology",
                "gate_min_recall_arms",
                GateExpectation::num(f64::from(topo::MIN_RECALL_ARMS)),
            ),
            (
                "topology",
                "gate_min_recall_shape_families",
                GateExpectation::num(f64::from(topo::MIN_RECALL_SHAPE_FAMILIES)),
            ),
            (
                "topology",
                "gate_min_non_trivial_gt_arms",
                GateExpectation::num(f64::from(topo::MIN_NON_TRIVIAL_GT_ARMS)),
            ),
            (
                "topology",
                "gate_min_topology_pairs",
                GateExpectation::num(f64::from(topo::MIN_TOPOLOGY_PAIRS)),
            ),
            (
                "topology",
                "gate_min_classes_per_retaining_pair",
                GateExpectation::num(f64::from(topo::MIN_CLASSES_PER_RETAINING_PAIR)),
            ),
            // --- topology_controls -------------------------------------
            // Not thresholds of a row but numbers that decide whether a row's
            // CONTROL measures anything, which RT45-A12 showed is the same kind
            // of number: `0.3 -> 0.0001` on the knockout radius empties the
            // control and leaves clause 1 green.
            (
                "topology_controls",
                "gate_knockout_disk_radius_fraction",
                GateExpectation::num(crate::topology::KNOCKOUT_DISK_RADIUS_FRACTION),
            ),
            (
                "topology_controls",
                "gate_gt_majority_level",
                GateExpectation::num(crate::topology::GT_MAJORITY_LEVEL),
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
            // --- noise scales: the M4 measurement ----------------------
            // Measured by
            // `corridor::tests::the_clean_bucket_noise_scale_is_measured_on_the_development_split`
            // and consumed by the corridor's sigma budget, so the frozen
            // number has a reader and a producer rather than being a
            // decoration (F-0019).
            (
                "noise_scales",
                "clean_bucket_sigma_codes",
                GateExpectation::num(vice_evidence::corridor::CLEAN_BUCKET_SIGMA_CODES),
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
}
