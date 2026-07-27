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
    Candidates {
        max: u64,
    },
    /// The hypothesis set was enumerated EXHAUSTIVELY: nothing was
    /// truncated, so no search bound is claimed and none is needed. Distinct
    /// from `NotApplicable`, which means no search happened at all — M4 runs
    /// a formation estimator, so "not applicable" stopped being true and the
    /// key moved with it.
    Exhaustive {
        formation_family: u64,
    },
}

impl CandidateBudget {
    pub fn as_key_text(&self) -> String {
        match self {
            CandidateBudget::NotApplicable => "not_applicable".to_string(),
            CandidateBudget::Candidates { max } => format!("candidates:{max}"),
            CandidateBudget::Exhaustive { formation_family } => {
                format!("exhaustive:formation_family={formation_family}")
            }
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
    pub(crate) fn component(&self, name: &str) -> String {
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

/// A measurement that KNOWS the conditions it was taken under.
///
/// This is condition B1 of REVIEW_M3_5 (finding M35-N3) as a type. The old
/// `insert(arm, key, value, crime)` took the key and the value as two
/// independent arguments, so the guarantee was "the caller presented one
/// key", not "the operands were measured under one". On the single
/// production call site the set was built from `key` and then filled with
/// the same `key`, so `first_difference` compared a key with itself and
/// could not fire. M4 publishes a second arm, so the hole stopped being
/// theoretical.
pub trait KeyedMeasurement {
    fn measurement_key(&self) -> &CompatibilityKey;
    fn measurement_value(&self, metric: &str) -> Option<f64>;
    fn measurement_crime(&self) -> &InverseCrime;
}

/// How several measurements become one arm value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reduce {
    /// Worst case: the conservative statistic for an error metric.
    Max,
    /// Worst case for an agreement fraction.
    Min,
    Mean,
}

impl Reduce {
    pub fn as_str(&self) -> &'static str {
        match self {
            Reduce::Max => "max",
            Reduce::Min => "min",
            Reduce::Mean => "mean",
        }
    }
}

/// One arm of a factorial, DERIVED from the measurements it aggregates.
///
/// The key is not an argument. It is built from the members: the components
/// that must be identical are TAKEN from them (and a member that disagrees
/// is a typed refusal naming the component), and the fixture component is a
/// hash over the members' own fixture hashes, so it is a function of exactly
/// the measurements that went in. There is no constructor that accepts a key.
#[derive(Debug, Clone, PartialEq)]
pub struct FactorialArm {
    id: String,
    key: CompatibilityKey,
    value: f64,
    inverse_crime: InverseCrime,
    members: u64,
    reduce: Reduce,
}

impl FactorialArm {
    pub fn aggregate<M: KeyedMeasurement>(
        id: &str,
        metric: &str,
        members: &[&M],
        reduce: Reduce,
    ) -> Result<FactorialArm, Incommensurable> {
        let first = members
            .first()
            .ok_or_else(|| Incommensurable::NoMeasurements {
                arm: id.to_string(),
            })?;
        let base = first.measurement_key();
        for m in members.iter().skip(1) {
            // Every component EXCEPT the fixture must be identical: the
            // fixture is what an aggregate ranges over.
            let k = m.measurement_key();
            if let Some(component) = KEY_COMPONENTS
                .iter()
                .filter(|n| **n != "fixture_hash")
                .find(|n| base.component(n) != k.component(n))
            {
                return Err(Incommensurable::KeyMismatch {
                    arm: id.to_string(),
                    component,
                    got: k.component(component),
                    want: base.component(component),
                });
            }
        }
        let mut fixtures: Vec<String> = members
            .iter()
            .map(|m| m.measurement_key().fixture_hash.clone())
            .collect();
        fixtures.sort();
        fixtures.dedup();
        let key = CompatibilityKey {
            backend_id: base.backend_id.clone(),
            config_hash: base.config_hash.clone(),
            candidate_budget: base.candidate_budget,
            fixture_hash: sha256_hex(fixtures.join("\u{1f}").as_bytes()),
            intervention_schema_version: base.intervention_schema_version.clone(),
        };
        let values: Vec<f64> = members
            .iter()
            .filter_map(|m| m.measurement_value(metric))
            .collect();
        if values.is_empty() {
            return Err(Incommensurable::NoMeasurements {
                arm: id.to_string(),
            });
        }
        let value = match reduce {
            Reduce::Max => values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            Reduce::Min => values.iter().copied().fold(f64::INFINITY, f64::min),
            Reduce::Mean => values.iter().sum::<f64>() / values.len() as f64,
        };
        Ok(FactorialArm {
            id: id.to_string(),
            key,
            value,
            inverse_crime: InverseCrime::fold_all(members.iter().map(|m| m.measurement_crime())),
            members: members.len() as u64,
            reduce,
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn key(&self) -> &CompatibilityKey {
        &self.key
    }
    pub fn value(&self) -> f64 {
        self.value
    }
    pub fn members(&self) -> u64 {
        self.members
    }
    pub fn reduce(&self) -> Reduce {
        self.reduce
    }
    pub fn inverse_crime(&self) -> &InverseCrime {
        &self.inverse_crime
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum Incommensurable {
    #[error("arm {arm} aggregates no measurement: an arm with no data is not an arm")]
    NoMeasurements { arm: String },
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
#[derive(Debug, Clone, Default)]
pub struct CommensurableArms {
    key: Option<CompatibilityKey>,
    arms: BTreeMap<String, ArmValue>,
}

impl CommensurableArms {
    /// An empty set with NO key. The key is adopted from the first arm
    /// inserted, so there is nowhere for a caller to assert one.
    pub fn new() -> CommensurableArms {
        CommensurableArms {
            key: None,
            arms: BTreeMap::new(),
        }
    }

    pub fn key(&self) -> Option<&CompatibilityKey> {
        self.key.as_ref()
    }

    pub fn fingerprint(&self) -> String {
        self.key
            .as_ref()
            .map(|k| k.fingerprint())
            .unwrap_or_default()
    }

    pub fn arm_ids(&self) -> Vec<&str> {
        self.arms.keys().map(|s| s.as_str()).collect()
    }

    pub fn contains(&self, arm: &str) -> bool {
        self.arms.contains_key(arm)
    }

    /// Add one arm. The arm CARRIES its key — there is no parameter for one
    /// — so what the set proves is that the operands were measured under one
    /// key, not that a caller said so (condition B1 / M35-N3).
    pub fn insert(&mut self, arm: &FactorialArm) -> Result<(), Incommensurable> {
        match &self.key {
            None => self.key = Some(arm.key.clone()),
            Some(k) => {
                if let Some(component) = k.first_difference(&arm.key) {
                    return Err(Incommensurable::KeyMismatch {
                        arm: arm.id.clone(),
                        component,
                        got: arm.key.component(component),
                        want: k.component(component),
                    });
                }
            }
        }
        if self.arms.contains_key(&arm.id) {
            return Err(Incommensurable::Duplicate {
                arm: arm.id.clone(),
            });
        }
        self.arms.insert(
            arm.id.clone(),
            ArmValue {
                value: arm.value,
                inverse_crime: arm.inverse_crime.clone(),
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
            key_fingerprint: self.fingerprint(),
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

    struct M(CompatibilityKey, f64, InverseCrime);

    impl KeyedMeasurement for M {
        fn measurement_key(&self) -> &CompatibilityKey {
            &self.0
        }
        fn measurement_value(&self, _metric: &str) -> Option<f64> {
            Some(self.1)
        }
        fn measurement_crime(&self) -> &InverseCrime {
            &self.2
        }
    }

    fn arm(id: &str, k: &CompatibilityKey, v: f64) -> FactorialArm {
        let m = M(k.clone(), v, InverseCrime::Clean);
        FactorialArm::aggregate(id, "m", &[&m], Reduce::Max).unwrap()
    }

    /// The walk over key components must cover the whole struct. Adding a
    /// field without adding it to `KEY_COMPONENTS` changes the serialized
    /// key but not the fingerprint, which is exactly the hole meta-rule M-1
    /// is about, so the two are compared here.
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

    /// Mutating ANY ONE of the five components makes two arms
    /// incommensurable, and the refusal names which one. The class, not an
    /// example.
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

            let mut set = CommensurableArms::new();
            set.insert(&arm("PF11", &base, 1.0)).unwrap();
            let err = set
                .insert(&arm("PF10", &other, 2.0))
                .expect_err("a differing key must be refused");
            match err {
                Incommensurable::KeyMismatch { component: c, .. } => {
                    // The fixture component is the one an AGGREGATE ranges
                    // over, so two arms differing only in it differ in the
                    // DERIVED fixture hash, which is still a mismatch.
                    assert!(c == *component || c == "fixture_hash", "{c} vs {component}");
                }
                other => panic!("{other:?}"),
            }
            assert!(set
                .contrast("effect", &[("PF11", 1.0), ("PF10", -1.0)])
                .is_err());
        }
    }

    /// Condition B1 (REVIEW_M3_5 M35-N3), as the property that closes it: an
    /// arm is DERIVED from its measurements, so measurements taken under two
    /// different keys cannot become one arm at all. There is no parameter
    /// through which a caller could assert otherwise.
    #[test]
    fn an_arm_cannot_be_aggregated_from_measurements_of_two_runs() {
        let here = M(key("exact-clip", "f1"), 1.0, InverseCrime::Clean);
        let there = M(key("vice-render", "f2"), 2.0, InverseCrime::Clean);
        match FactorialArm::aggregate("PF11", "m", &[&here, &there], Reduce::Max) {
            Err(Incommensurable::KeyMismatch { component, .. }) => {
                assert_eq!(component, "backend_id")
            }
            other => panic!("{other:?}"),
        }
        // The control: measurements that DO share everything but the fixture
        // aggregate fine, and the aggregate's fixture component is a
        // function of theirs, not of anything a caller supplied.
        let a = M(key("exact-clip", "f1"), 1.0, InverseCrime::Clean);
        let b = M(key("exact-clip", "f2"), 5.0, InverseCrime::Clean);
        let agg = FactorialArm::aggregate("PF11", "m", &[&a, &b], Reduce::Max).unwrap();
        assert_eq!(agg.value(), 5.0);
        assert_eq!(agg.members(), 2);
        assert_ne!(agg.key().fixture_hash, "f1");
        assert_ne!(agg.key().fixture_hash, "f2");
        let swapped = FactorialArm::aggregate("PF11", "m", &[&b, &a], Reduce::Max).unwrap();
        assert_eq!(
            agg.key().fixture_hash,
            swapped.key().fixture_hash,
            "the derived fixture component must not depend on iteration order"
        );
        // A different member set is a different key: an aggregate cannot be
        // compared with one taken over other fixtures.
        let fewer = FactorialArm::aggregate("PF11", "m", &[&a], Reduce::Max).unwrap();
        assert_ne!(agg.key().fixture_hash, fewer.key().fixture_hash);
        // And an arm over nothing is refused rather than being a zero.
        let none: Vec<&M> = Vec::new();
        assert!(matches!(
            FactorialArm::aggregate("PF11", "m", &none, Reduce::Max),
            Err(Incommensurable::NoMeasurements { .. })
        ));
    }

    /// The machinery is not merely refusing everything: four commensurable
    /// arms produce the three factorial effects, with the standard 2x2
    /// arithmetic.
    #[test]
    fn commensurable_arms_do_produce_the_three_factorial_effects() {
        let k = key("exact-clip", "fixture-a");
        let mut set = CommensurableArms::new();
        for (id, v) in [("PF00", 1.0), ("PF10", 2.0), ("PF01", 4.0), ("PF11", 8.0)] {
            set.insert(&arm(id, &k, v)).unwrap();
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
        assert_eq!(a.terms().len(), 4);
        assert!(!a.key_fingerprint().is_empty());

        // A main effect is NOT a sequential difference.
        let ladder = set
            .contrast("sequential", &[("PF10", 1.0), ("PF00", -1.0)])
            .unwrap();
        assert!((ladder.value() - 1.0).abs() < 1e-12);
        assert_ne!(ladder.value(), a.value());
    }

    /// Contamination reaches the delta through the fold, and one dirty
    /// measurement among clean ones is enough.
    #[test]
    fn a_delta_inherits_contamination_from_any_of_its_arms() {
        // Two arms over the SAME fixture set, so their derived keys agree;
        // one of the measurements behind the first is contaminated.
        let crime = InverseCrime::of(RasterProfile::ViceRender, RasterProfile::TinySkia);
        let a1 = M(key("vice-render", "f1"), 1.0, InverseCrime::Clean);
        let a2 = M(key("vice-render", "f2"), 2.0, crime);
        let b1 = M(key("vice-render", "f1"), 3.0, InverseCrime::Clean);
        let b2 = M(key("vice-render", "f2"), 1.0, InverseCrime::Clean);
        let dirty_arm = FactorialArm::aggregate("PF10", "m", &[&a1, &a2], Reduce::Max).unwrap();
        let clean_arm = FactorialArm::aggregate("PF00", "m", &[&b1, &b2], Reduce::Min).unwrap();
        assert!(dirty_arm.inverse_crime().is_contaminated());
        assert!(!clean_arm.inverse_crime().is_contaminated());

        let mut set = CommensurableArms::new();
        set.insert(&dirty_arm).unwrap();
        set.insert(&clean_arm).unwrap();
        let d = set
            .contrast("sequential", &[("PF10", 1.0), ("PF00", -1.0)])
            .unwrap();
        assert!(d.inverse_crime().is_contaminated());
        assert!(!d.inverse_crime().warnings().is_empty());

        // Control: an all-clean contrast is clean.
        let k = key("exact-clip", "f");
        let mut clean = CommensurableArms::new();
        clean.insert(&arm("PF00", &k, 1.0)).unwrap();
        clean.insert(&arm("PF10", &k, 2.0)).unwrap();
        assert!(!clean
            .contrast("sequential", &[("PF10", 1.0), ("PF00", -1.0)])
            .unwrap()
            .inverse_crime()
            .is_contaminated());
    }

    #[test]
    fn an_arm_cannot_be_inserted_twice_and_a_missing_arm_is_named() {
        let k = key("exact-clip", "f");
        let mut set = CommensurableArms::new();
        set.insert(&arm("PF11", &k, 1.0)).unwrap();
        assert!(matches!(
            set.insert(&arm("PF11", &k, 9.0)),
            Err(Incommensurable::Duplicate { .. })
        ));
        let err = set
            .contrast("x", &[("PF11", 1.0), ("PF00", -1.0)])
            .unwrap_err();
        assert_eq!(err.arm, "PF00");
    }

    /// The exhaustive budget is a DISTINCT key component value: an M3.5 arm
    /// (no search at all) and an M4 arm (an exhaustively enumerated family)
    /// are not commensurable, which is the key doing its job rather than a
    /// versioning accident.
    #[test]
    fn an_exhaustive_budget_is_not_the_same_as_no_search() {
        let mut a = key("b", "f");
        a.candidate_budget = CandidateBudget::NotApplicable;
        let mut b = key("b", "f");
        b.candidate_budget = CandidateBudget::Exhaustive {
            formation_family: 8,
        };
        assert_ne!(a.fingerprint(), b.fingerprint());
        assert_eq!(a.first_difference(&b), Some("candidate_budget"));
        assert!(b.candidate_budget.as_key_text().contains("exhaustive"));
    }
}
