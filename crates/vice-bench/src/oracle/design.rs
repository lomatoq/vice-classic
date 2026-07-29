//! The experimental DESIGN of the factorial oracle suite (spec §27.6).
//!
//! §27.6 replaces the sequential ladder `O2-O1`, `O3-O2` — whose numbers
//! depend on the order the interventions were applied — with a factorial:
//! partition and formation are crossed 2×2, and what is published is the
//! partition main effect, the formation main effect and their interaction.
//!
//! This module is the design carried by the historical M4 oracle report.  The
//! M6 geometry implementation and its five-arm measurements live in
//! `crate::geometry` and `docs/gt/GEOMETRY_M6.json`; they are intentionally not
//! retroactively embedded into an artifact whose `milestone` field is `M4`.
//!
//! Absence is DATA here, never a type for a later milestone to implement
//! (§32 rule 7, §27.6 "fake placeholder arms are forbidden"). An arm that
//! cannot be produced honestly yields [`NotYetApplicable`], which names the
//! capabilities it is missing and the milestone that owns each of them. It
//! does not yield zero, an empty struct, or a trait waiting for M4.

use serde::Serialize;

pub const INTERVENTION_SCHEMA_VERSION: &str = "vice-classic/oracle-intervention/v1";

/// A capability the M4 report harness did not have, with the milestone that
/// supplied it.
///
/// This is a list of facts at the report's M4 snapshot, not an interface: it
/// declares no method a future milestone must implement, and adding the
/// capability means deleting the variant's use, not filling it in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MissingCapability {
    /// Proposing a visible partition from an image (§28 M4.5).
    AutoPartition,
    // `FormationEstimation` USED to be here and is gone, not filled in:
    // M4 estimates the global formation from the image
    // (`vice_evidence::analysis`), so the capability exists and the variant
    // that recorded its absence has no subject. That is what this enum's
    // doc comment promised would happen — "adding the capability means
    // deleting the variant's use" — and deleting the variant itself is the
    // stronger form: nothing can now record an absence that ended.
    // `CandidateGeneration`, `Selector` and `ParameterFit` USED to be here and
    // are gone, not filled in. M6 delivered all three — `vice_fit::span` and
    // `vice_fit::schedule` generate typed span candidates, `vice_fit::grammar`
    // selects among them by the explicit §14.5 code length, and
    // `vice_fit::solve` fits the continuous parameters of a chosen family — so
    // the variants that recorded their absence have no subject. This is the
    // same deletion `FormationEstimation` got when M4 delivered it, and it is
    // the stronger form of "adding the capability means deleting the variant's
    // use": nothing can now record an absence that ended.
    //
    // What remains, and what the G ladder is ACTUALLY blocked on, is below.
    /// Driving the Stage G/H pipeline from an injected partition and scoring
    /// the geometry it returns against ground truth (§27.6's G arms are all
    /// stated "при GT partition + GT formation").
    ///
    /// M6 later supplied this in `crate::geometry::bind_observation`, with an
    /// explicit `(scene id, BoundaryId)` binding and symmetric geometry error.
    GeometryPipelineArm,
    /// Injecting an oracle into the geometry search: a GT-compatible candidate
    /// (G10), a selector that ranks by agreement with GT rather than by code
    /// length (G01), both (G11), or forced GT-equivalent families and
    /// breakpoints (G20).
    ///
    /// M6 later supplied all four interventions plus the typed G20 forced-fit
    /// API; the historical refusal remains serialized in the M4 report.
    OracleInjection,
}

impl MissingCapability {
    pub fn id(&self) -> &'static str {
        match self {
            MissingCapability::AutoPartition => "auto_partition",
            MissingCapability::GeometryPipelineArm => "geometry_pipeline_arm",
            MissingCapability::OracleInjection => "oracle_injection",
        }
    }

    pub fn owner_milestone(&self) -> &'static str {
        match self {
            MissingCapability::AutoPartition => "M4.5",
            MissingCapability::GeometryPipelineArm | MissingCapability::OracleInjection => "M6",
        }
    }

    /// Order of milestones, so an arm missing several capabilities can name
    /// the one that actually gates it rather than an arbitrary member.
    fn milestone_rank(&self) -> u8 {
        match self.owner_milestone() {
            "M4" => 0,
            "M4.5" => 1,
            "M5" => 2,
            "M6" => 3,
            "M7" => 4,
            _ => 5,
        }
    }
}

/// Where the visible partition handed to the backend came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PartitionSource {
    /// Produced by the system under test.
    Auto,
    /// Injected from the fixture's measured partition truth.
    GroundTruth,
}

/// Where the global image-formation hypothesis came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FormationSource {
    /// Estimated by the system under test.
    Estimated,
    /// Injected from the degradation cell that produced the observation.
    GroundTruth,
}

/// The four arms of the partition × formation factorial (§27.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum PfArm {
    Pf00,
    Pf10,
    Pf01,
    Pf11,
}

impl PfArm {
    pub const ALL: &'static [PfArm] = &[PfArm::Pf00, PfArm::Pf10, PfArm::Pf01, PfArm::Pf11];

    pub fn id(&self) -> &'static str {
        match self {
            PfArm::Pf00 => "PF00",
            PfArm::Pf10 => "PF10",
            PfArm::Pf01 => "PF01",
            PfArm::Pf11 => "PF11",
        }
    }

    pub fn partition(&self) -> PartitionSource {
        match self {
            PfArm::Pf00 | PfArm::Pf01 => PartitionSource::Auto,
            PfArm::Pf10 | PfArm::Pf11 => PartitionSource::GroundTruth,
        }
    }

    pub fn formation(&self) -> FormationSource {
        match self {
            PfArm::Pf00 | PfArm::Pf10 => FormationSource::Estimated,
            PfArm::Pf01 | PfArm::Pf11 => FormationSource::GroundTruth,
        }
    }

    /// What this arm needs that the system does not have. Empty means the
    /// arm is producible now.
    pub fn missing(&self) -> Vec<MissingCapability> {
        let mut v = Vec::new();
        if self.partition() == PartitionSource::Auto {
            v.push(MissingCapability::AutoPartition);
        }
        // The formation side no longer contributes: M4 estimates it. PF10
        // therefore became producible, which is the whole point of the
        // milestone's factorial clause.
        v
    }
}

/// The geometry decomposition of §27.6, evaluated at GT partition + GT
/// formation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum GArm {
    G00,
    G10,
    G01,
    G11,
    G20,
    G30,
}

impl GArm {
    pub const ALL: &'static [GArm] = &[
        GArm::G00,
        GArm::G10,
        GArm::G01,
        GArm::G11,
        GArm::G20,
        GArm::G30,
    ];

    pub fn id(&self) -> &'static str {
        match self {
            GArm::G00 => "G00",
            GArm::G10 => "G10",
            GArm::G01 => "G01",
            GArm::G11 => "G11",
            GArm::G20 => "G20",
            GArm::G30 => "G30",
        }
    }

    /// What §27.6 says each arm isolates, quoted so the report explains
    /// itself without the spec at hand.
    pub fn isolates(&self) -> &'static str {
        match self {
            GArm::G00 => "auto candidates + auto selector",
            GArm::G10 => "oracle GT-compatible candidate injected + auto selector",
            GArm::G01 => "auto candidates + oracle selector over the available set",
            GArm::G11 => "oracle candidate set + oracle selector (combined discrete ceiling)",
            GArm::G20 => "forced GT-equivalent families/breakpoints + auto parameter fit",
            GArm::G30 => "GT parameters, no optimizer: the renderer/serialization ceiling",
        }
    }

    pub fn missing(&self) -> Vec<MissingCapability> {
        match self {
            // G30 injects the GT parameters and runs no search at all, so
            // it needs nothing the harness lacks. It is the one arm M3.5
            // can measure, and §28 M3.5 asks for exactly it.
            GArm::G30 => Vec::new(),
            // These are refusals in the M4 report snapshot. The separate M6
            // geometry artifact now measures all five arms.
            GArm::G00 => vec![MissingCapability::GeometryPipelineArm],
            GArm::G10 | GArm::G01 | GArm::G11 | GArm::G20 => vec![
                MissingCapability::GeometryPipelineArm,
                MissingCapability::OracleInjection,
            ],
        }
    }
}

/// Why an arm or an effect produced no number.
///
/// A typed refusal with a reason and an owner, which is what §27.6 asks for
/// when it forbids fake placeholder arms: the absence is recorded as data a
/// reader can act on, not as a zero that a later aggregation would average.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NotYetApplicable {
    /// The arm or effect that is absent.
    pub what: String,
    /// Capability ids that would have to exist first.
    pub missing: Vec<&'static str>,
    /// The milestone that owns the last of them.
    pub owner_milestone: &'static str,
    pub reason: String,
}

impl NotYetApplicable {
    /// Build a refusal from a set of missing capabilities. Panics on an
    /// empty set on purpose: "not applicable for no reason" is exactly the
    /// placeholder §27.6 forbids, and it must not be constructible.
    pub fn from_missing(
        what: impl Into<String>,
        missing: &[MissingCapability],
    ) -> NotYetApplicable {
        assert!(
            !missing.is_empty(),
            "a refusal must name what is missing; an unexplained not_yet_applicable is a \
             placeholder"
        );
        let owner = missing
            .iter()
            .max_by_key(|m| m.milestone_rank())
            .expect("non-empty")
            .owner_milestone();
        let what = what.into();
        let names: Vec<String> = missing
            .iter()
            .map(|m| format!("{} ({})", m.id(), m.owner_milestone()))
            .collect();
        NotYetApplicable {
            reason: format!(
                "{what} cannot be produced honestly: the harness has no {}. Spec 27.6 forbids a \
                 placeholder arm, so no number is reported.",
                names.join(", ")
            ),
            what,
            missing: missing.iter().map(|m| m.id()).collect(),
            owner_milestone: owner,
        }
    }

    /// A refusal that is not about a missing milestone capability but about
    /// a stated limit of this harness (e.g. an instrument that cannot
    /// realize the formation of a cell).
    pub fn limitation(
        what: impl Into<String>,
        reason: impl Into<String>,
        owner_milestone: &'static str,
    ) -> NotYetApplicable {
        NotYetApplicable {
            what: what.into(),
            missing: Vec::new(),
            owner_milestone,
            reason: reason.into(),
        }
    }
}

/// The outcome of one declared arm or effect.
///
/// Two variants and no third: a measurement or a typed refusal. There is
/// deliberately no `Placeholder`, no `Unknown(f64)` and no default, so the
/// absent case cannot be turned into a number by anything downstream.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "outcome", content = "detail", rename_all = "snake_case")]
pub enum ArmOutcome<T> {
    Measured(T),
    NotYetApplicable(NotYetApplicable),
}

impl<T> ArmOutcome<T> {
    pub fn measured(&self) -> Option<&T> {
        match self {
            ArmOutcome::Measured(v) => Some(v),
            ArmOutcome::NotYetApplicable(_) => None,
        }
    }

    pub fn refusal(&self) -> Option<&NotYetApplicable> {
        match self {
            ArmOutcome::Measured(_) => None,
            ArmOutcome::NotYetApplicable(r) => Some(r),
        }
    }
}

/// One arm as it appears in the report: what was injected, and what came out.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ArmDeclaration {
    pub arm: &'static str,
    pub intervention_schema_version: &'static str,
    pub partition_source: Option<PartitionSource>,
    pub formation_source: Option<FormationSource>,
    pub isolates: Option<&'static str>,
    pub outcome: ArmOutcome<String>,
}

impl ArmDeclaration {
    pub fn pf(arm: PfArm, measured: Option<String>) -> ArmDeclaration {
        let missing = arm.missing();
        let outcome = match measured {
            Some(note) => ArmOutcome::Measured(note),
            None => {
                ArmOutcome::NotYetApplicable(NotYetApplicable::from_missing(arm.id(), &missing))
            }
        };
        ArmDeclaration {
            arm: arm.id(),
            intervention_schema_version: INTERVENTION_SCHEMA_VERSION,
            partition_source: Some(arm.partition()),
            formation_source: Some(arm.formation()),
            isolates: None,
            outcome,
        }
    }

    pub fn g(arm: GArm, measured: Option<String>) -> ArmDeclaration {
        let missing = arm.missing();
        let outcome = match measured {
            Some(note) => ArmOutcome::Measured(note),
            None => {
                ArmOutcome::NotYetApplicable(NotYetApplicable::from_missing(arm.id(), &missing))
            }
        };
        ArmDeclaration {
            arm: arm.id(),
            intervention_schema_version: INTERVENTION_SCHEMA_VERSION,
            partition_source: Some(PartitionSource::GroundTruth),
            formation_source: Some(FormationSource::GroundTruth),
            isolates: Some(arm.isolates()),
            outcome,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 2×2 is a real crossing: each factor takes both levels, and the
    /// four arms are the four distinct combinations.
    #[test]
    fn the_factorial_crosses_both_factors() {
        let combos: std::collections::BTreeSet<(PartitionSource, FormationSource)> = PfArm::ALL
            .iter()
            .map(|a| (a.partition(), a.formation()))
            .collect();
        assert_eq!(
            combos.len(),
            4,
            "the design must be a crossing, not a ladder"
        );
        assert_eq!(PfArm::Pf11.partition(), PartitionSource::GroundTruth);
        assert_eq!(PfArm::Pf11.formation(), FormationSource::GroundTruth);
        assert_eq!(PfArm::Pf00.partition(), PartitionSource::Auto);
        assert_eq!(PfArm::Pf00.formation(), FormationSource::Estimated);
    }

    /// TWO PF arms are producible at M4 — the estimated-formation arm
    /// joined the ground-truth one — and the other two name what they still
    /// need. If a later milestone adds a partitioner and forgets to remove
    /// the capability, this test fails loudly.
    #[test]
    fn both_ground_truth_partition_arms_are_producible_in_m4() {
        assert!(PfArm::Pf11.missing().is_empty());
        assert!(
            PfArm::Pf10.missing().is_empty(),
            "M4 estimates the formation, so PF10 stopped being a refusal"
        );
        for arm in [PfArm::Pf00, PfArm::Pf01] {
            let missing = arm.missing();
            assert_eq!(missing, vec![MissingCapability::AutoPartition]);
            let r = NotYetApplicable::from_missing(arm.id(), &missing);
            assert!(r.reason.contains("27.6"));
            assert_eq!(r.owner_milestone, "M4.5");
        }
    }

    /// The historical M4 report truthfully records G30 as its only producible
    /// geometry arm and names M6 as the owner of the five missing arms.
    ///
    /// The change is the point. M6 delivered candidate generation, the
    /// code-length selector and the parameter fit, so the three variants that
    /// recorded their absence are deleted. What the G ladder is actually
    /// blocked on is the HARNESS: no path from a fixture's GT partition into
    /// Stage G, no geometry error against GT geometry, and no way to express a
    /// GT boundary as a candidate over an observed chain's samples. All three
    /// need the chain identity STATUS_M6 limitation 57 prices.
    ///
    /// The current M6 result is tested in `crate::geometry`; this test protects
    /// the provenance of an older report instead of rewriting history.
    #[test]
    fn the_m4_report_names_m6_as_the_owner_of_its_missing_geometry_arms() {
        assert!(GArm::G30.missing().is_empty());
        for arm in [GArm::G00, GArm::G01, GArm::G10, GArm::G11, GArm::G20] {
            let missing = arm.missing();
            assert!(!missing.is_empty(), "{} claims to be producible", arm.id());
            assert_eq!(
                NotYetApplicable::from_missing(arm.id(), &missing).owner_milestone,
                "M6"
            );
        }
        // And the capabilities M6 delivered are GONE rather than renamed: the
        // whole id set of the enum is two entries plus the M4.5 one.
        let mut ids: Vec<&str> = [GArm::G00, GArm::G01, GArm::G10, GArm::G11, GArm::G20]
            .iter()
            .flat_map(|a| a.missing())
            .map(|m| m.id())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids, vec!["geometry_pipeline_arm", "oracle_injection"]);
    }

    /// A refusal with nothing missing is a placeholder, and must not exist.
    #[test]
    #[should_panic(expected = "must name what is missing")]
    fn an_unexplained_refusal_cannot_be_constructed() {
        let _ = NotYetApplicable::from_missing("PFxx", &[]);
    }

    /// A refusal serializes as a refusal, not as a number: nothing
    /// downstream can read a zero out of it.
    #[test]
    fn a_refused_arm_carries_no_value_in_the_artifact() {
        let d = ArmDeclaration::pf(PfArm::Pf00, None);
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains("not_yet_applicable"));
        assert!(json.contains("auto_partition"));
        assert!(json.contains("M4.5"));
        assert!(d.outcome.measured().is_none());
        assert!(d.outcome.refusal().is_some());

        let m = ArmDeclaration::pf(PfArm::Pf11, Some("measured".into()));
        assert!(m.outcome.measured().is_some());
    }
}
