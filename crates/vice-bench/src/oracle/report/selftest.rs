//! Evidence that the gate mechanisms EXECUTE, produced by running them on
//! synthetic arms at report time.
//!
//! Why this block exists at all: at M3.5 the honest report contained no
//! causal delta, so "every delta is commensurable" was green because the set
//! was empty — meta-rule M-2 exactly. M4 publishes two arms and still no
//! effect (three of the four are needed), so the trap has not gone away, and
//! each gate row stays a conjunction of a property of the data AND a
//! selftest that drives the machinery through the state where the check DOES
//! execute.
//!
//! The numbers here measure nothing about the corpus. They are 1/2/4/8 so
//! they cannot be mistaken for one.

use serde::Serialize;

use crate::gt::raster::RasterProfile;
use crate::oracle::crime::InverseCrime;
use crate::oracle::design::INTERVENTION_SCHEMA_VERSION;
use crate::oracle::effects::pf_effects;
use crate::oracle::key::{
    CandidateBudget, CommensurableArms, CompatibilityKey, FactorialArm, KeyedMeasurement, Reduce,
    KEY_COMPONENTS,
};

/// One component of the compatibility key, and whether mutating it alone is
/// refused.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct KeyComponentCheck {
    pub component: &'static str,
    pub mismatch_refused: bool,
    pub refusal: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MechanismSelftest {
    pub note: &'static str,
    pub synthetic_arm_values: Vec<(&'static str, f64)>,
    pub effects_produced: u64,
    pub partition_main_effect: f64,
    pub formation_main_effect: f64,
    pub interaction: f64,
    /// The sequential difference §27.6 replaced, printed next to the main
    /// effect it is NOT.
    pub sequential_pf10_minus_pf00: f64,
    pub key_components: Vec<KeyComponentCheck>,
    pub all_key_components_refuse_a_mismatch: bool,
    /// Condition B1 (REVIEW_M3_5 M35-N3): the key is DERIVED from the
    /// measurements, so a set cannot be told a key that its arms do not
    /// have. Demonstrated by aggregating two measurements whose keys differ
    /// and showing the aggregation itself refuses.
    pub key_is_derived_from_the_measurements: bool,
    pub derivation_refusal: String,
    pub contaminated_arm_contaminates_the_aggregate: bool,
    pub all_clean_arms_leave_the_aggregate_clean: bool,
}

/// A measurement with a key, for the selftest only.
struct Synthetic {
    key: CompatibilityKey,
    value: f64,
    crime: InverseCrime,
}

impl crate::oracle::key::sealed::Sealed for Synthetic {}

impl KeyedMeasurement for Synthetic {
    fn measurement_key(&self) -> &CompatibilityKey {
        &self.key
    }
    fn measurement_value(&self, _metric: &str) -> Option<f64> {
        Some(self.value)
    }
    fn measurement_crime(&self) -> &InverseCrime {
        &self.crime
    }
}

fn selftest_key() -> CompatibilityKey {
    CompatibilityKey {
        backend_id: "selftest-backend".to_string(),
        config_hash: "selftest-config".to_string(),
        candidate_budget: CandidateBudget::NotApplicable,
        fixture_hash: "selftest-fixture".to_string(),
        intervention_schema_version: INTERVENTION_SCHEMA_VERSION.to_string(),
    }
}

fn mutate(key: &CompatibilityKey, component: &str) -> CompatibilityKey {
    let mut k = key.clone();
    match component {
        "backend_id" => k.backend_id.push_str("-other"),
        "config_hash" => k.config_hash.push_str("-other"),
        "candidate_budget" => k.candidate_budget = CandidateBudget::Candidates { max: 1 },
        "fixture_hash" => k.fixture_hash.push_str("-other"),
        "intervention_schema_version" => k.intervention_schema_version.push_str("-other"),
        other => unreachable!("unknown key component {other}"),
    }
    k
}

fn arm(id: &str, key: &CompatibilityKey, value: f64) -> FactorialArm {
    let m = Synthetic {
        key: key.clone(),
        value,
        crime: InverseCrime::Clean,
    };
    FactorialArm::aggregate(id, "selftest", &[&m], Reduce::Max).expect("one measurement is an arm")
}

impl MechanismSelftest {
    pub fn run() -> MechanismSelftest {
        let key = selftest_key();
        let values: Vec<(&'static str, f64)> =
            vec![("PF00", 1.0), ("PF10", 2.0), ("PF01", 4.0), ("PF11", 8.0)];
        let mut set = CommensurableArms::new();
        for (id, v) in &values {
            set.insert(&arm(id, &key, *v))
                .expect("synthetic arms share one key");
        }
        let e = pf_effects("selftest", &set);
        let val = |o: &crate::oracle::design::ArmOutcome<crate::oracle::key::CausalDelta>| {
            o.measured().map(|d| d.value()).unwrap_or(f64::NAN)
        };
        let produced = e
            .effects()
            .iter()
            .filter(|o| o.measured().is_some())
            .count() as u64;
        let ladder = set
            .contrast("selftest-sequential", &[("PF10", 1.0), ("PF00", -1.0)])
            .expect("both arms present")
            .value();

        // Walk the CLASS of key components: mutate each one alone and record
        // whether the set refuses.
        let key_components: Vec<KeyComponentCheck> = KEY_COMPONENTS
            .iter()
            .map(|component| {
                let mut probe = CommensurableArms::new();
                probe.insert(&arm("PF11", &key, 1.0)).expect("same key");
                let outcome = probe.insert(&arm("PF10", &mutate(&key, component), 2.0));
                KeyComponentCheck {
                    component,
                    mismatch_refused: outcome.is_err(),
                    refusal: outcome
                        .err()
                        .map(|e| e.to_string())
                        .unwrap_or_else(|| "NOT REFUSED".to_string()),
                }
            })
            .collect();

        // Condition B1: the key is derived, so two measurements that do not
        // share one cannot become an arm at all — the refusal happens at
        // AGGREGATION, before any value exists to pair a key with.
        let a = Synthetic {
            key: key.clone(),
            value: 1.0,
            crime: InverseCrime::Clean,
        };
        let b = Synthetic {
            key: mutate(&key, "backend_id"),
            value: 2.0,
            crime: InverseCrime::Clean,
        };
        let derived = FactorialArm::aggregate("PF11", "selftest", &[&a, &b], Reduce::Max);

        let dirty = InverseCrime::of(RasterProfile::ViceRender, RasterProfile::TinySkia);
        let mixed = InverseCrime::fold_all(
            [
                &InverseCrime::Clean,
                &InverseCrime::Clean,
                &dirty,
                &InverseCrime::Clean,
            ]
            .into_iter(),
        );
        let clean = InverseCrime::fold_all(
            [
                &InverseCrime::Clean,
                &InverseCrime::Clean,
                &InverseCrime::Clean,
            ]
            .into_iter(),
        );

        MechanismSelftest {
            note: "SYNTHETIC. These values measure nothing about the corpus; they exist so the \
                   gate rows below are not green merely because no delta was produced.",
            synthetic_arm_values: values,
            effects_produced: produced,
            partition_main_effect: val(&e.partition_main_effect),
            formation_main_effect: val(&e.formation_main_effect),
            interaction: val(&e.interaction),
            sequential_pf10_minus_pf00: ladder,
            all_key_components_refuse_a_mismatch: key_components.iter().all(|c| c.mismatch_refused)
                && key_components.len() == KEY_COMPONENTS.len(),
            key_components,
            key_is_derived_from_the_measurements: derived.is_err(),
            derivation_refusal: derived
                .err()
                .map(|e| e.to_string())
                .unwrap_or_else(|| "NOT REFUSED".to_string()),
            contaminated_arm_contaminates_the_aggregate: mixed.is_contaminated(),
            all_clean_arms_leave_the_aggregate_clean: !clean.is_contaminated(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// B1, as the selftest sees it: measurements that were NOT taken under
    /// one key cannot be aggregated into an arm, and the refusal names the
    /// component. The old API could not express this check at all, because
    /// the key was an argument beside the value.
    #[test]
    fn an_arm_cannot_be_derived_from_measurements_of_two_different_runs() {
        let st = MechanismSelftest::run();
        assert!(st.key_is_derived_from_the_measurements);
        assert!(
            st.derivation_refusal.contains("backend_id"),
            "{}",
            st.derivation_refusal
        );
        assert!(st.all_key_components_refuse_a_mismatch);
        assert_eq!(st.key_components.len(), KEY_COMPONENTS.len());
        // And the control: four arms that DO share a key produce the three
        // effects, so the refusals above are not a machine that refuses
        // everything.
        assert_eq!(st.effects_produced, 3);
        assert!((st.partition_main_effect - 2.5).abs() < 1e-12);
        assert!((st.formation_main_effect - 4.5).abs() < 1e-12);
        assert!((st.interaction - 1.5).abs() < 1e-12);
        assert!((st.sequential_pf10_minus_pf00 - 1.0).abs() < 1e-12);
    }
}
