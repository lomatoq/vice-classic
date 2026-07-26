//! Compatibility keys, and why subtracting incompatible arms is impossible
//! rather than discouraged (spec §27.6, §28 M3.5).
//!
//! §27.6 gives the key verbatim:
//!
//! ```text
//! backend_id + config_hash + candidate_budget + fixture_hash +
//! intervention_schema_version
//! ```
//!
//! and then one sentence: *"Incompatible arms may not be subtracted."*
//!
//! Written as a sentence that is a request. The §28 M3.5 gate says "no
//! causal deltas across incompatible runs", and a gate has to be a thing
//! that fails, so the rule is expressed as a type here:
//!
//! - [`CausalDelta`] has private fields and no public constructor;
//! - its only constructor is [`CommensurableArms::contrast`];
//! - a [`CommensurableArms`] set refuses, with a typed error naming the
//!   offending COMPONENT, any arm whose key differs from the set's own.
//!
//! So there is no sequence of public calls that yields a `CausalDelta`
//! spanning two keys. A caller can of course subtract two `f64`s by hand —
//! nothing in Rust prevents arithmetic — but the result is a bare number
//! that no report field accepts, because every effect field in this crate
//! is typed `CausalDelta` or a typed refusal. The claim this module makes
//! is exactly that and no more.

use std::collections::BTreeMap;

use serde::Serialize;

use super::crime::InverseCrime;
use crate::hashing::sha256_hex;

pub const ORACLE_CONFIG_SCHEMA: &str = "vice-classic/oracle-config/v1";

/// The search budget an arm ran under.
///
/// `NotApplicable` is not "zero candidates": zero would claim a search that
/// examined nothing, and M3.5 runs no search at all — the candidate IS the
/// injected ground truth. The distinction matters because the budget is a
/// key component: a future budgeted arm must not be comparable with an
/// unbudgeted one just because both serialize as a small number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CandidateBudget {
    NotApplicable,
    Candidates { max: u64 },
}

impl CandidateBudget {
    pub fn as_key_text(&self) -> String {
        match self {
            CandidateBudget::NotApplicable => "not_applicable".to_string(),
            CandidateBudget::Candidates { max } => format!("candidates:{max}"),
        }
    }
}

/// The five-component key of §27.6.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompatibilityKey {
    pub backend_id: String,
    pub config_hash: String,
    pub candidate_budget: CandidateBudget,
    pub fixture_hash: String,
    pub intervention_schema_version: String,
}

/// The components, as `(name, value)`, in a fixed order.
///
/// Exposed because the checks below iterate over it: naming the class of
/// components once and walking it is what stops the check from covering the
/// three components someone happened to think of (meta-rule M-1). A new
/// component added to the struct without being added here fails
/// `every_key_component_is_covered_by_the_walk`.
pub const KEY_COMPONENTS: &[&str] = &[
    "backend_id",
    "config_hash",
    "candidate_budget",
    "fixture_hash",
    "intervention_schema_version",
];

impl CompatibilityKey {
    fn component(&self, name: &str) -> String {
        match name {
            "backend_id" => self.backend_id.clone(),
            "config_hash" => self.config_hash.clone(),
            "candidate_budget" => self.candidate_budget.as_key_text(),
            "fixture_hash" => self.fixture_hash.clone(),
            "intervention_schema_version" => self.intervention_schema_version.clone(),
            other => unreachable!("unknown key component {other}"),
        }
    }

    /// The first component on which two keys differ, in the fixed order.
    pub fn first_difference(&self, other: &CompatibilityKey) -> Option<&'static str> {
        KEY_COMPONENTS
            .iter()
            .find(|name| self.component(name) != other.component(name))
            .copied()
    }

    /// Stable fingerprint over ALL components, in the declared order.
    pub fn fingerprint(&self) -> String {
        let joined: Vec<String> = KEY_COMPONENTS
            .iter()
            .map(|n| format!("{n}={}", self.component(n)))
            .collect();
        sha256_hex(joined.join("\u{1f}").as_bytes())
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum Incommensurable {
    #[error(
        "arm {arm} may not join this set: {component} differs ({got:?} vs {want:?}). \
         Spec 27.6: incompatible arms may not be subtracted."
    )]
    KeyMismatch {
        arm: String,
        component: &'static str,
        got: String,
        want: String,
    },
    #[error("arm {arm} is already present in this set")]
    Duplicate { arm: String },
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("contrast {label} needs arm {arm}, which this set does not contain")]
pub struct MissingArm {
    pub label: String,
    pub arm: String,
}

#[derive(Debug, Clone, PartialEq)]
struct ArmValue {
    value: f64,
    inverse_crime: InverseCrime,
}

/// A set of arms PROVEN to share one compatibility key.
///
/// Built one arm at a time; every insertion re-checks the whole key, so
/// there is no "trusted" path that skips the comparison.
#[derive(Debug, Clone)]
pub struct CommensurableArms {
    key: CompatibilityKey,
    fingerprint: String,
    arms: BTreeMap<String, ArmValue>,
}

impl CommensurableArms {
    pub fn new(key: CompatibilityKey) -> CommensurableArms {
        let fingerprint = key.fingerprint();
        CommensurableArms {
            key,
            fingerprint,
            arms: BTreeMap::new(),
        }
    }

    pub fn key(&self) -> &CompatibilityKey {
        &self.key
    }

    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    pub fn arm_ids(&self) -> Vec<&str> {
        self.arms.keys().map(|s| s.as_str()).collect()
    }

    pub fn contains(&self, arm: &str) -> bool {
        self.arms.contains_key(arm)
    }

    /// Add one arm. The key travels WITH the value, so an arm cannot be
    /// inserted without presenting the conditions it was measured under.
    pub fn insert(
        &mut self,
        arm: &str,
        key: &CompatibilityKey,
        value: f64,
        inverse_crime: InverseCrime,
    ) -> Result<(), Incommensurable> {
        if let Some(component) = self.key.first_difference(key) {
            return Err(Incommensurable::KeyMismatch {
                arm: arm.to_string(),
                component,
                got: key.component(component),
                want: self.key.component(component),
            });
        }
        if self.arms.contains_key(arm) {
            return Err(Incommensurable::Duplicate {
                arm: arm.to_string(),
            });
        }
        self.arms.insert(
            arm.to_string(),
            ArmValue {
                value,
                inverse_crime,
            },
        );
        Ok(())
    }

    /// A linear contrast over arms of this set — the ONLY constructor of
    /// [`CausalDelta`].
    ///
    /// A factorial effect is a contrast with coefficients ±0.5; a sequential
    /// difference would be one with coefficients ±1 over two arms. Both go
    /// through here, so both are subject to the same key check, and neither
    /// can be assembled from arms of two different runs.
    pub fn contrast(&self, label: &str, terms: &[(&str, f64)]) -> Result<CausalDelta, MissingArm> {
        let mut value = 0.0;
        let mut crime = InverseCrime::Clean;
        let mut recorded = Vec::with_capacity(terms.len());
        for (arm, coeff) in terms {
            let v = self.arms.get(*arm).ok_or_else(|| MissingArm {
                label: label.to_string(),
                arm: (*arm).to_string(),
            })?;
            value += coeff * v.value;
            crime = crime.fold(&v.inverse_crime);
            recorded.push(ContrastTerm {
                arm: (*arm).to_string(),
                coefficient: *coeff,
            });
        }
        Ok(CausalDelta {
            label: label.to_string(),
            value,
            terms: recorded,
            key_fingerprint: self.fingerprint.clone(),
            inverse_crime: crime,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ContrastTerm {
    pub arm: String,
    pub coefficient: f64,
}

/// A causal delta: a signed combination of arms that all shared one
/// compatibility key.
///
/// Fields are private and there is no public constructor. The only way to
/// obtain one is [`CommensurableArms::contrast`], and a `CommensurableArms`
/// cannot contain two arms whose keys differ. That is the §28 M3.5 clause
/// "no causal deltas across incompatible runs", as a type.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CausalDelta {
    label: String,
    value: f64,
    terms: Vec<ContrastTerm>,
    key_fingerprint: String,
    inverse_crime: InverseCrime,
}

impl CausalDelta {
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn value(&self) -> f64 {
        self.value
    }
    pub fn terms(&self) -> &[ContrastTerm] {
        &self.terms
    }
    pub fn key_fingerprint(&self) -> &str {
        &self.key_fingerprint
    }
    pub fn inverse_crime(&self) -> &InverseCrime {
        &self.inverse_crime
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gt::raster::RasterProfile;

    fn key(backend: &str, fixture: &str) -> CompatibilityKey {
        CompatibilityKey {
            backend_id: backend.to_string(),
            config_hash: "cfg".to_string(),
            candidate_budget: CandidateBudget::NotApplicable,
            fixture_hash: fixture.to_string(),
            intervention_schema_version: "v1".to_string(),
        }
    }

    /// The walk over key components must cover the whole struct. Adding a
    /// field without adding it to `KEY_COMPONENTS` changes the serialized
    /// key but not the fingerprint, which is exactly the hole meta-rule M-1
    /// is about — so the two are compared here.
    #[test]
    fn every_key_component_is_covered_by_the_walk() {
        let k = key("b", "f");
        let json = serde_json::to_value(&k).unwrap();
        let fields: std::collections::BTreeSet<String> = json
            .as_object()
            .unwrap()
            .keys()
            .map(|s| s.to_string())
            .collect();
        let walked: std::collections::BTreeSet<String> =
            KEY_COMPONENTS.iter().map(|s| (*s).to_string()).collect();
        assert_eq!(
            fields, walked,
            "a compatibility key component is not covered by the fingerprint walk"
        );
        assert_eq!(KEY_COMPONENTS.len(), 5, "spec 27.6 names five components");
    }

    /// Mutating ANY ONE of the five components is refused, and the refusal
    /// names which one. The class, not an example.
    #[test]
    fn each_of_the_five_components_alone_makes_two_arms_incommensurable() {
        let base = key("exact-clip", "fixture-a");
        for component in KEY_COMPONENTS {
            let mut other = base.clone();
            match *component {
                "backend_id" => other.backend_id = "vice-render".into(),
                "config_hash" => other.config_hash = "cfg2".into(),
                "candidate_budget" => {
                    other.candidate_budget = CandidateBudget::Candidates { max: 1 }
                }
                "fixture_hash" => other.fixture_hash = "fixture-b".into(),
                "intervention_schema_version" => other.intervention_schema_version = "v2".into(),
                x => panic!("unhandled component {x}"),
            }
            assert_ne!(base.fingerprint(), other.fingerprint(), "{component}");

            let mut set = CommensurableArms::new(base.clone());
            set.insert("PF11", &base, 1.0, InverseCrime::Clean).unwrap();
            let err = set
                .insert("PF10", &other, 2.0, InverseCrime::Clean)
                .expect_err("a differing key must be refused");
            match err {
                Incommensurable::KeyMismatch { component: c, .. } => assert_eq!(&c, component),
                other => panic!("{other:?}"),
            }
            // And the delta that would have used it does not exist.
            assert!(set
                .contrast("effect", &[("PF11", 1.0), ("PF10", -1.0)])
                .is_err());
        }
    }

    /// The machinery is not merely refusing everything: four commensurable
    /// arms produce the three factorial effects, with the standard 2²
    /// arithmetic. Without this control, "no deltas across incompatible
    /// runs" would be satisfied by producing no deltas at all.
    #[test]
    fn commensurable_arms_do_produce_the_three_factorial_effects() {
        let k = key("exact-clip", "fixture-a");
        let mut set = CommensurableArms::new(k.clone());
        for (arm, v) in [("PF00", 1.0), ("PF10", 2.0), ("PF01", 4.0), ("PF11", 8.0)] {
            set.insert(arm, &k, v, InverseCrime::Clean).unwrap();
        }
        let a = set
            .contrast(
                "partition_main_effect",
                &[("PF10", 0.5), ("PF00", -0.5), ("PF11", 0.5), ("PF01", -0.5)],
            )
            .unwrap();
        let b = set
            .contrast(
                "formation_main_effect",
                &[("PF01", 0.5), ("PF00", -0.5), ("PF11", 0.5), ("PF10", -0.5)],
            )
            .unwrap();
        let ab = set
            .contrast(
                "interaction",
                &[("PF11", 0.5), ("PF01", -0.5), ("PF10", -0.5), ("PF00", 0.5)],
            )
            .unwrap();
        assert!((a.value() - 2.5).abs() < 1e-12, "{}", a.value());
        assert!((b.value() - 4.5).abs() < 1e-12, "{}", b.value());
        assert!((ab.value() - 1.5).abs() < 1e-12, "{}", ab.value());
        assert_eq!(a.key_fingerprint(), k.fingerprint());
        assert_eq!(a.terms().len(), 4);

        // A main effect is NOT a sequential difference: PF10-PF00 alone is
        // 1.0 and the main effect is 2.5, so the two are distinguishable and
        // the report cannot be publishing the ladder under a factorial name.
        let ladder = set
            .contrast("sequential", &[("PF10", 1.0), ("PF00", -1.0)])
            .unwrap();
        assert!((ladder.value() - 1.0).abs() < 1e-12);
        assert_ne!(ladder.value(), a.value());
    }

    /// Contamination reaches the delta through the fold, and one dirty arm
    /// out of four is enough.
    #[test]
    fn a_delta_inherits_contamination_from_any_of_its_arms() {
        let k = key("vice-render", "fixture-a");
        let mut set = CommensurableArms::new(k.clone());
        set.insert("PF00", &k, 1.0, InverseCrime::Clean).unwrap();
        set.insert(
            "PF10",
            &k,
            2.0,
            InverseCrime::of(RasterProfile::ViceRender, RasterProfile::TinySkia),
        )
        .unwrap();
        let d = set
            .contrast("sequential", &[("PF10", 1.0), ("PF00", -1.0)])
            .unwrap();
        assert!(d.inverse_crime().is_contaminated());
        assert!(!d.inverse_crime().warnings().is_empty());

        // Control: an all-clean contrast is clean.
        let mut clean = CommensurableArms::new(k.clone());
        clean.insert("PF00", &k, 1.0, InverseCrime::Clean).unwrap();
        clean.insert("PF10", &k, 2.0, InverseCrime::Clean).unwrap();
        assert!(!clean
            .contrast("sequential", &[("PF10", 1.0), ("PF00", -1.0)])
            .unwrap()
            .inverse_crime()
            .is_contaminated());
    }

    #[test]
    fn an_arm_cannot_be_inserted_twice_and_a_missing_arm_is_named() {
        let k = key("exact-clip", "f");
        let mut set = CommensurableArms::new(k.clone());
        set.insert("PF11", &k, 1.0, InverseCrime::Clean).unwrap();
        assert!(matches!(
            set.insert("PF11", &k, 9.0, InverseCrime::Clean),
            Err(Incommensurable::Duplicate { .. })
        ));
        let err = set
            .contrast("x", &[("PF11", 1.0), ("PF00", -1.0)])
            .unwrap_err();
        assert_eq!(err.arm, "PF00");
    }
}
