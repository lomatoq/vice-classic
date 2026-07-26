//! The M3 scorecard (spec §28 M3, §31 report contract in M3 scope).
//!
//! What a scorecard can honestly say at this milestone. There is no
//! vectorizer, so there is nothing to score: every reliability row is a
//! DEFICIT against the sample-size contract and every confidence claim is
//! a refusal. What the report does carry is the state of the measurement
//! apparatus — the hashes that pin the corpus, the universe, the analysis
//! plan and the gates; the composition of the corpus and its splits; the
//! measured accuracy of the corpus's own instruments; and the measured
//! residual correlation.
//!
//! §31's full field list belongs to M7, when a run produces posteriors and
//! certificates. Emitting those fields now as zeros would be exactly the
//! "placeholder API" §32 rule 7 forbids, so this report contains the
//! subset that has a producer and says so.

use serde::Serialize;

use crate::correlation::{guard_confidence_claim, ResidualModel};
use crate::gates::GatesFile;
use crate::gt::corpus::CorpusManifest;
use crate::gt::split::{AuditSeal, Split};
use crate::prereg::Preregistration;
use crate::reliability::{risk_coverage, RiskCoverage};
use crate::universe::{model_universe_hash, SupportedModelUniverseV1};

pub const SCORECARD_SCHEMA: &str = "vice-classic/m3-scorecard/v1";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Hashes {
    pub model_universe: String,
    pub preregistration: String,
    pub gates_sha256: String,
    pub corpus: String,
    pub split_policy: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Composition {
    pub source_groups: u64,
    pub scenes: u64,
    pub renders: u64,
    pub cells: u64,
    pub groups_by_origin: Vec<(String, u64)>,
    pub groups_by_split: Vec<(String, u64)>,
    pub renders_by_identifiability: Vec<(String, u64)>,
    pub renders_by_profile: Vec<(String, u64)>,
    pub intentionally_ambiguous_groups: u64,
    pub inverse_crime_renders: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ConfidenceStatus {
    pub residual_model: Option<String>,
    pub calibrated: bool,
    /// Always populated in M3: the reason no confidence may be claimed.
    pub refusal: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Scorecard {
    pub schema: &'static str,
    pub milestone: &'static str,
    pub hashes: Hashes,
    pub audit_seal_generation: u32,
    pub audit_seal_status: String,
    pub composition: Composition,
    pub reliability: Vec<RiskCoverage>,
    pub confidence: ConfidenceStatus,
    /// Facts the report deliberately does NOT contain, with the milestone
    /// that will produce them. Stated so a reader can tell "absent because
    /// nothing produces it" from "absent because it was forgotten".
    pub not_yet_produced: Vec<&'static str>,
}

fn counts(pairs: impl Iterator<Item = (String, usize)>) -> Vec<(String, u64)> {
    let mut v: Vec<(String, u64)> = pairs.map(|(k, n)| (k, n as u64)).collect();
    v.sort();
    v
}

/// Build the M3 scorecard from the measurement apparatus.
pub fn build(manifest: &CorpusManifest, gates: &GatesFile, seal: &AuditSeal) -> Scorecard {
    let universe = SupportedModelUniverseV1::v1();
    let prereg = Preregistration::v1();

    let mut by_origin: std::collections::BTreeMap<String, usize> = Default::default();
    let mut by_split: std::collections::BTreeMap<String, usize> = Default::default();
    for g in &manifest.groups {
        *by_origin.entry(g.origin.to_string()).or_default() += 1;
        *by_split.entry(g.split.to_string()).or_default() += 1;
    }

    // Reliability, per preregistered bucket. There are no outcomes: nothing
    // produces them, so every row reports zero accepted groups and the
    // deficit against the contract.
    let reliability: Vec<RiskCoverage> = prereg
        .buckets
        .iter()
        .map(|b| risk_coverage(b.id, &[], prereg.confidence, prereg.risk_target))
        .collect();

    let refusal = guard_confidence_claim(Some(ResidualModel::Block), false, 9.0)
        .err()
        .map(|e| e.to_string());

    Scorecard {
        schema: SCORECARD_SCHEMA,
        milestone: "M3",
        hashes: Hashes {
            model_universe: model_universe_hash(&universe),
            preregistration: prereg.hash(),
            gates_sha256: gates.sha256.clone(),
            corpus: manifest.hash(),
            split_policy: manifest.split_policy_version.clone(),
        },
        audit_seal_generation: seal.generation,
        audit_seal_status: format!("{:?}", seal.status).to_lowercase(),
        composition: Composition {
            source_groups: manifest.source_groups() as u64,
            scenes: manifest.groups.iter().map(|g| g.scenes.len() as u64).sum(),
            renders: manifest.renders.len() as u64,
            cells: manifest.cells.len() as u64,
            groups_by_origin: counts(by_origin.into_iter()),
            groups_by_split: counts(by_split.into_iter()),
            renders_by_identifiability: counts(
                manifest
                    .identifiability_counts
                    .iter()
                    .map(|(k, v)| (k.clone(), *v)),
            ),
            renders_by_profile: counts(
                manifest
                    .renders_by_profile
                    .iter()
                    .map(|(k, v)| (k.clone(), *v)),
            ),
            intentionally_ambiguous_groups: manifest
                .groups
                .iter()
                .filter(|g| g.intentionally_ambiguous)
                .count() as u64,
            inverse_crime_renders: manifest.renders.iter().filter(|r| r.inverse_crime).count()
                as u64,
        },
        reliability,
        confidence: ConfidenceStatus {
            residual_model: None,
            calibrated: false,
            refusal,
        },
        not_yet_produced: vec![
            "posterior bits, delivery-equivalence confidence and DecisionIntervals (M7)",
            "search truncation / unexplored-mass certificates (M7; both Unknown in the universe)",
            "boundary p50/p95/p99 metrics against GT (M7: nothing produces a boundary yet)",
            "factorial oracle arms PF/G (M3.5)",
            "human court (§27.4: there is no output to judge)",
            "sealed-audit results (the audit is SEALED and M3 does not open it)",
        ],
    }
}

impl Scorecard {
    pub fn canonical_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("scorecard serializes")
    }

    /// The M3 gate table, as booleans a reader can check rather than
    /// prose. Every entry names the mechanism that makes it true.
    pub fn gate_table(&self, manifest: &CorpusManifest) -> Vec<(&'static str, bool, String)> {
        vec![
            (
                "no test leakage",
                manifest.groups.iter().all(|g| {
                    manifest
                        .groups
                        .iter()
                        .filter(|o| o.shape_family == g.shape_family)
                        .all(|o| o.split == g.split)
                }) && manifest.renders.iter().all(|r| {
                    r.split != Split::Development.as_str() || !r.cell_id.contains("tiny-skia")
                }),
                "whole shape families move together; the held-out profile never appears in \
                 development"
                    .to_string(),
            ),
            (
                "sealed-audit burn policy active",
                self.audit_seal_status == "sealed",
                format!(
                    "generation {} is {}; opening records the three hashes and any later change \
                     is a typed BurnViolation",
                    self.audit_seal_generation, self.audit_seal_status
                ),
            ),
            (
                "source-group independence defined",
                self.reliability.iter().all(|r| r.required_groups == 459),
                "the trial unit is the source group; the sample-size contract is derived and \
                 reports a deficit"
                    .to_string(),
            ),
            (
                "supported universe is finite/versioned",
                SupportedModelUniverseV1::v1().check_finite().is_ok(),
                format!("model_universe_hash {}", self.hashes.model_universe),
            ),
            (
                "correlation-aware likelihood protocol exists before any confidence claim",
                self.confidence.refusal.is_some(),
                self.confidence
                    .refusal
                    .clone()
                    .unwrap_or_else(|| "MISSING".to_string()),
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gt::corpus::{build_manifest, test_cell_filter};
    use crate::gt::split::SPLIT_POLICY_V1;
    use std::sync::OnceLock;

    fn manifest() -> &'static CorpusManifest {
        static M: OnceLock<CorpusManifest> = OnceLock::new();
        M.get_or_init(|| build_manifest(&SPLIT_POLICY_V1, test_cell_filter).unwrap())
    }

    fn gates() -> GatesFile {
        GatesFile::load(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../configs/GATES_V1.toml"),
        )
        .unwrap()
    }

    #[test]
    fn the_scorecard_reports_a_deficit_and_a_refusal_not_a_pass() {
        let m = manifest();
        let s = build(m, &gates(), &AuditSeal::sealed(1));

        assert_eq!(s.audit_seal_status, "sealed");
        assert!(s.confidence.refusal.is_some(), "M3 may claim no confidence");
        assert!(s.confidence.residual_model.is_none());
        for row in &s.reliability {
            assert_eq!(row.groups_accepted, 0);
            assert_eq!(row.required_groups, 459);
            assert_eq!(row.group_deficit, 459);
            assert!(!row.contract_met);
            assert_eq!(row.catastrophic_risk_upper, 1.0);
        }
        assert!(s.composition.source_groups >= 55);
        assert!(s.composition.intentionally_ambiguous_groups >= 3);
        assert!(s.composition.inverse_crime_renders > 0);
        assert!(!s.not_yet_produced.is_empty());
    }

    #[test]
    fn the_gate_table_is_true_for_stated_reasons() {
        let m = manifest();
        let s = build(m, &gates(), &AuditSeal::sealed(1));
        let table = s.gate_table(m);
        assert_eq!(table.len(), 5);
        for (name, ok, why) in &table {
            assert!(*ok, "gate {name} is not met: {why}");
            assert!(why.len() > 20, "gate {name} has no stated mechanism");
        }
        // And the table is not vacuously true: an opened seal fails it.
        let opened = AuditSeal::sealed(1).open("c", "p", "g", "note");
        let s2 = build(m, &gates(), &opened);
        let burn_row = s2
            .gate_table(m)
            .into_iter()
            .find(|(n, _, _)| *n == "sealed-audit burn policy active")
            .unwrap();
        assert!(!burn_row.1, "an opened audit must not report as sealed");
    }

    #[test]
    fn the_scorecard_is_deterministic() {
        let m = manifest();
        let a = build(m, &gates(), &AuditSeal::sealed(1)).canonical_json();
        let b = build(m, &gates(), &AuditSeal::sealed(1)).canonical_json();
        assert_eq!(a, b);
    }
}
