//! Factorial effects and the geometry ladder, assembled only from
//! commensurable arms (spec §27.6).
//!
//! §27.6: *"For each metric publish the partition main effect, the formation
//! main effect and the interaction."* Not `PF10-PF00`, not `PF11-PF10` —
//! those are the sequential differences whose value depends on the order the
//! interventions were applied, which is the reason the spec replaced the
//! ladder in the first place.
//!
//! With ±1 coding of the two factors, the three published quantities are
//! contrasts over ALL FOUR arms:
//!
//! ```text
//! partition main = ½[(PF10 − PF00) + (PF11 − PF01)]
//! formation main = ½[(PF01 − PF00) + (PF11 − PF10)]
//! interaction    = ½[(PF11 − PF01) − (PF10 − PF00)]
//! ```
//!
//! Every one of them needs all four arms, so at a milestone where three arms
//! do not exist, all three effects are typed refusals. That is not a gap in
//! the harness: it is the honest reading of the design, and reporting a
//! half-effect from the two arms that happen to exist would be exactly the
//! order-dependent number §27.6 abolished.

use serde::Serialize;

use super::design::{ArmOutcome, GArm, MissingCapability, NotYetApplicable, PfArm};
use super::key::{CausalDelta, CommensurableArms};

/// The three effects of one 2×2 factorial, for one metric.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FactorialEffects {
    pub metric: String,
    pub key_fingerprint: String,
    /// Which of the four arms actually contributed a number.
    pub present_arms: Vec<String>,
    pub partition_main_effect: ArmOutcome<CausalDelta>,
    pub formation_main_effect: ArmOutcome<CausalDelta>,
    pub interaction: ArmOutcome<CausalDelta>,
}

impl FactorialEffects {
    pub fn effects(&self) -> [&ArmOutcome<CausalDelta>; 3] {
        [
            &self.partition_main_effect,
            &self.formation_main_effect,
            &self.interaction,
        ]
    }
}

/// The ±0.5 coefficients of the three effects, over the four arms.
fn coefficients(effect: &str) -> [(&'static str, f64); 4] {
    match effect {
        "partition_main_effect" => [("PF10", 0.5), ("PF00", -0.5), ("PF11", 0.5), ("PF01", -0.5)],
        "formation_main_effect" => [("PF01", 0.5), ("PF00", -0.5), ("PF11", 0.5), ("PF10", -0.5)],
        "interaction" => [("PF11", 0.5), ("PF01", -0.5), ("PF10", -0.5), ("PF00", 0.5)],
        other => unreachable!("unknown effect {other}"),
    }
}

pub const EFFECT_NAMES: &[&str] = &[
    "partition_main_effect",
    "formation_main_effect",
    "interaction",
];

fn effect_outcome(effect: &str, metric: &str, arms: &CommensurableArms) -> ArmOutcome<CausalDelta> {
    let coeffs = coefficients(effect);
    let absent: Vec<PfArm> = PfArm::ALL
        .iter()
        .copied()
        .filter(|a| !arms.contains(a.id()))
        .collect();
    if absent.is_empty() {
        let terms: Vec<(&str, f64)> = coeffs.iter().map(|(a, c)| (*a, *c)).collect();
        return match arms.contrast(&format!("{effect}[{metric}]"), &terms) {
            Ok(d) => ArmOutcome::Measured(d),
            // Unreachable in practice — every arm was just checked present —
            // but a panic here would be a claim, and a refusal is a fact.
            Err(e) => ArmOutcome::NotYetApplicable(NotYetApplicable::limitation(
                format!("{effect}[{metric}]"),
                e.to_string(),
                "M4",
            )),
        };
    }
    let mut missing: Vec<MissingCapability> = absent.iter().flat_map(|a| a.missing()).collect();
    missing.sort();
    missing.dedup();
    let mut refusal = NotYetApplicable::from_missing(format!("{effect}[{metric}]"), &missing);
    let names: Vec<&str> = absent.iter().map(|a| a.id()).collect();
    refusal.reason = format!(
        "{} needs all four arms of the 2x2 (a half effect from the arms that happen to exist \
         would be the order-dependent sequential difference spec 27.6 replaced). Absent: {}. {}",
        refusal.what,
        names.join(", "),
        refusal.reason
    );
    ArmOutcome::NotYetApplicable(refusal)
}

/// Build the three effects for one metric from whatever arms are present.
pub fn pf_effects(metric: &str, arms: &CommensurableArms) -> FactorialEffects {
    FactorialEffects {
        metric: metric.to_string(),
        key_fingerprint: arms.fingerprint(),
        present_arms: PfArm::ALL
            .iter()
            .filter(|a| arms.contains(a.id()))
            .map(|a| a.id().to_string())
            .collect(),
        partition_main_effect: effect_outcome("partition_main_effect", metric, arms),
        formation_main_effect: effect_outcome("formation_main_effect", metric, arms),
        interaction: effect_outcome("interaction", metric, arms),
    }
}

/// One entry of the geometry ladder of §27.6.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GeometryDelta {
    pub name: &'static str,
    /// What §27.6 says the quantity isolates.
    pub isolates: &'static str,
    pub outcome: ArmOutcome<CausalDelta>,
}

/// The interpretation deltas §27.6 lists for the geometry decomposition.
///
/// All of them subtract two G arms, and every G arm except G30 needs a
/// capability M6 owns, so all of them refuse here. They are listed rather
/// than omitted so that a reader can see the shape of the contract and tell
/// "absent because nothing produces it" from "absent because it was
/// forgotten" (§31).
pub fn geometry_deltas(arms: &CommensurableArms) -> Vec<GeometryDelta> {
    let spec: &[(&'static str, &'static str, GArm, GArm)] = &[
        (
            "G10-G00",
            "candidate-generation ceiling",
            GArm::G10,
            GArm::G00,
        ),
        (
            "G01-G00",
            "selector ceiling within the auto candidate set",
            GArm::G01,
            GArm::G00,
        ),
        ("G11-G00", "combined discrete ceiling", GArm::G11, GArm::G00),
        (
            "G20-G30",
            "parameter fitting, isolated against the renderer ceiling",
            GArm::G20,
            GArm::G30,
        ),
    ];
    spec.iter()
        .map(|(name, isolates, a, b)| {
            let mut missing: Vec<MissingCapability> = a
                .missing()
                .into_iter()
                .chain(b.missing())
                .filter(|_| !arms.contains(a.id()) || !arms.contains(b.id()))
                .collect();
            missing.sort();
            missing.dedup();
            let outcome = if missing.is_empty() {
                match arms.contrast(name, &[(a.id(), 1.0), (b.id(), -1.0)]) {
                    Ok(d) => ArmOutcome::Measured(d),
                    Err(e) => ArmOutcome::NotYetApplicable(NotYetApplicable::limitation(
                        *name,
                        e.to_string(),
                        "M7",
                    )),
                }
            } else {
                ArmOutcome::NotYetApplicable(NotYetApplicable::from_missing(*name, &missing))
            };
            GeometryDelta {
                name,
                isolates,
                outcome,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::crime::InverseCrime;
    use super::super::key::{CandidateBudget, CompatibilityKey};
    use super::*;

    fn key() -> CompatibilityKey {
        CompatibilityKey {
            backend_id: "canonical-roundtrip+exact-clip".to_string(),
            config_hash: "cfg".to_string(),
            candidate_budget: CandidateBudget::NotApplicable,
            fixture_hash: "fixture".to_string(),
            intervention_schema_version: "v1".to_string(),
        }
    }

    /// A set built the way production builds one: every arm carries the key
    /// it was measured under, and the set adopts it.
    fn with_arms(vals: &[(&str, f64)]) -> CommensurableArms {
        let mut set = CommensurableArms::new();
        for (arm, v) in vals {
            set.insert(&synthetic_arm(arm, *v)).unwrap();
        }
        set
    }

    pub(crate) fn synthetic_arm(id: &str, value: f64) -> super::super::key::FactorialArm {
        struct M(super::super::key::CompatibilityKey, f64);
        impl super::super::key::sealed::Sealed for M {}
        impl super::super::key::KeyedMeasurement for M {
            fn measurement_key(&self) -> &super::super::key::CompatibilityKey {
                &self.0
            }
            fn measurement_value(&self, _metric: &str) -> Option<f64> {
                Some(self.1)
            }
            fn measurement_crime(&self) -> &InverseCrime {
                &InverseCrime::Clean
            }
        }
        let m = M(key(), value);
        super::super::key::FactorialArm::aggregate(id, "m", &[&m], super::super::key::Reduce::Max)
            .expect("one measurement is an arm")
    }

    /// The M4 state: PF11 and PF10 exist, the other two do not, so all
    /// three effects still refuse — a contrast over half a factorial is the
    /// order-dependent difference §27.6 abolished.
    #[test]
    fn with_only_the_gt_partition_arms_all_three_effects_refuse_and_say_why() {
        let e = pf_effects(
            "edge_mean_abs_code",
            &with_arms(&[("PF11", 3.0), ("PF10", 2.0)]),
        );
        assert_eq!(e.present_arms, vec!["PF10".to_string(), "PF11".to_string()]);
        for outcome in e.effects() {
            let r = outcome.refusal().expect("must refuse");
            assert!(r.reason.contains("PF00"));
            assert!(r.reason.contains("PF01"));
            assert!(r.reason.contains("sequential difference"));
            assert_eq!(r.owner_milestone, "M4.5");
            assert!(r.missing.contains(&"auto_partition"));
        }
        let json = serde_json::to_string(&e).unwrap();
        assert!(
            !json.contains("\"value\""),
            "a refusal must carry no number"
        );
    }

    /// Three arms are still not enough: a main effect from three quarters
    /// of a factorial is not a main effect.
    #[test]
    fn three_of_four_arms_still_refuse() {
        let e = pf_effects(
            "max_abs_code",
            &with_arms(&[("PF11", 3.0), ("PF10", 2.0), ("PF01", 1.0)]),
        );
        assert_eq!(e.present_arms.len(), 3);
        for outcome in e.effects() {
            assert!(outcome.refusal().is_some());
        }
    }

    /// And with all four the effects ARE produced, with the 2² arithmetic.
    /// Without this the refusals above would be indistinguishable from a
    /// harness that can never produce anything (meta-rule M-2).
    #[test]
    fn all_four_arms_produce_the_three_effects_with_factorial_arithmetic() {
        let e = pf_effects(
            "m",
            &with_arms(&[("PF00", 1.0), ("PF10", 2.0), ("PF01", 4.0), ("PF11", 8.0)]),
        );
        let a = e.partition_main_effect.measured().expect("produced");
        let b = e.formation_main_effect.measured().expect("produced");
        let ab = e.interaction.measured().expect("produced");
        assert!((a.value() - 2.5).abs() < 1e-12);
        assert!((b.value() - 4.5).abs() < 1e-12);
        assert!((ab.value() - 1.5).abs() < 1e-12);
        for d in [a, b, ab] {
            assert_eq!(d.terms().len(), 4, "an effect uses all four arms");
            assert!(!d.key_fingerprint().is_empty());
        }
    }

    /// The geometry ladder refuses in M3.5 and names M6, but its shape is
    /// published so the reader can see what is missing.
    #[test]
    fn the_geometry_ladder_is_declared_and_refused() {
        let d = geometry_deltas(&with_arms(&[("G30", 1.0)]));
        assert_eq!(d.len(), 4);
        for entry in &d {
            let r = entry.outcome.refusal().expect("must refuse");
            assert_eq!(
                r.owner_milestone, "M7",
                "after M6 the geometry ladder is blocked on the harness rather than on the                  candidate stage, and its owner moved with it"
            );
            assert!(!entry.isolates.is_empty());
        }
        assert!(d.iter().any(|e| e.name == "G10-G00"));
    }
}
