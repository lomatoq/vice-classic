//! The M3.5 oracle artifact and its gate table (spec §28 M3.5).
//!
//! The gate sentence is *"no causal deltas across incompatible runs;
//! inverse-crime warning visible"*, and both clauses have the same trap: at
//! this milestone the honest report contains NO causal delta at all, so a
//! check written as "every delta is commensurable" would be green because
//! the set is empty. That is meta-rule M-2 exactly — green because the state
//! belongs to the subclass where the check does not execute — and it is the
//! failure mode this project has repeated inside the milestone that named
//! it.
//!
//! So each gate row here is a conjunction of two things: a property of the
//! artifact's own data, AND a [`MechanismSelftest`] that drives the
//! machinery through the state where the check DOES execute and records the
//! outcome as data a reviewer can read without running anything. The
//! selftest walks the CLASS of key components rather than an example of one
//! (meta-rule M-1), and its inputs are obviously synthetic so they cannot be
//! mistaken for a measurement of the corpus.

pub mod gate;
pub mod selftest;

use serde::Serialize;

use super::ceiling::{CeilingAggregate, CeilingArm, CEILING_METRICS};
use super::crime::InverseCrime;
use super::design::{ArmDeclaration, FormationSource, GArm, PfArm};
use super::effects::{geometry_deltas, pf_effects, FactorialEffects, GeometryDelta};
use super::key::{CommensurableArms, FactorialArm, Reduce};
use super::{OracleConfig, OracleRun, RefusedArm};
use crate::gt::corpus::Platform;
pub use selftest::{KeyComponentCheck, MechanismSelftest};

/// The schema moves with the harness. M4 changed what an arm IS (a second
/// formation source), what its key says (an exhaustive search budget where
/// M3.5 had none) and what the report carries, so an M3.5 artifact and an M4
/// artifact are not the same document and must not share a name.
pub const ORACLE_SCHEMA: &str = "vice-classic/m4-oracle/v1";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OracleReport {
    pub schema: &'static str,
    pub milestone: &'static str,
    /// The Tier A platform these numbers belong to (§5.5, F-0020): every
    /// metric below is a float derived from libm, so an artifact that did
    /// not carry its platform would invite exactly the cross-platform
    /// comparison ADR-0008 §8 forbids.
    pub platform: Platform,
    pub config: OracleConfig,
    pub config_hash: String,
    pub fixture_set_hash: String,
    pub scenes: u64,
    pub arms_measured: u64,
    pub arms_refused: u64,
    pub clean_arms: u64,
    pub inverse_crime_arms: u64,
    pub warnings: Vec<String>,
    pub ceiling: Vec<CeilingAggregate>,
    pub ceiling_arms: Vec<CeilingArm>,
    pub pf_arms: Vec<ArmDeclaration>,
    pub g_arms: Vec<ArmDeclaration>,
    pub factorial: Vec<FactorialEffects>,
    pub geometry_deltas: Vec<GeometryDelta>,
    pub refused: Vec<RefusedArm>,
    /// Arms the factorial could not ASSEMBLE, with the typed reason. Empty
    /// in a healthy run; non-empty would mean two measurements that should
    /// share a key do not.
    pub assembly_refusals: Vec<String>,
    /// (scene, cell) pairs on which BOTH formation sources produced an arm,
    /// and the ones only the ground-truth source did. §27.6 requires the
    /// arms of a factorial to share fixtures, so the factorial runs over the
    /// intersection and the difference is published rather than averaged in.
    pub factorial_common_fixtures: u64,
    pub factorial_dropped_fixtures: u64,
    pub mechanism_selftest: MechanismSelftest,
    pub not_yet_produced: Vec<&'static str>,
}

/// How each metric is reduced over the arms of one factorial arm: the
/// statistic that makes it conservative.
fn reduce_for(metric: &str) -> Reduce {
    match metric {
        "max_abs_code" => Reduce::Max,
        "identical_pixels_frac" => Reduce::Min,
        _ => Reduce::Mean,
    }
}

/// The warnings a report MUST carry, derived from its arms and aggregates.
///
/// Shared by `build` and by the gate row, so clause 2 compares the published
/// warnings with a derived SET rather than with emptiness — the second of
/// the two vacuum gaps REVIEW_M3_5 M35-N4 found ("the weather is fine"
/// satisfied `!warnings.is_empty()`).
pub fn derived_warnings(
    arms: &[CeilingArm],
    aggregates: &[CeilingAggregate],
    has_factorial: bool,
) -> Vec<String> {
    let overall = InverseCrime::fold_all(arms.iter().map(|a| &a.inverse_crime));
    let mut out: Vec<String> = overall.warnings().iter().map(|w| w.to_string()).collect();
    for agg in aggregates
        .iter()
        .filter(|a| a.inverse_crime.is_contaminated())
    {
        out.push(format!(
            "contaminated pairing: backend {} against observation profile {} (cell {}, arm {})",
            agg.backend_id, agg.observation_profile, agg.cell_id, agg.arm
        ));
    }
    if has_factorial && overall.is_contaminated() {
        out.push(
            "every factorial arm of this report inherits the contamination above through the \
             aggregation fold; no effect below may be read as evidence about accuracy"
                .to_string(),
        );
    }
    out
}

pub fn build(run: &OracleRun) -> OracleReport {
    let arms: Vec<&CeilingArm> = run.arms.iter().collect();

    // Aggregates per (arm, backend, observation cell): the level at which a
    // clean pairing and a contaminated one are both visible, now split by
    // which formation the arm rendered with.
    let mut ceiling = Vec::new();
    let mut pairs: Vec<(&'static str, String, String)> = arms
        .iter()
        .map(|a| (a.arm, a.backend_id.clone(), a.cell_id.clone()))
        .collect();
    pairs.sort();
    pairs.dedup();
    for (arm_id, backend, cell) in &pairs {
        let group: Vec<&CeilingArm> = arms
            .iter()
            .filter(|a| a.arm == *arm_id && &a.backend_id == backend && &a.cell_id == cell)
            .copied()
            .collect();
        if let Some(agg) = CeilingAggregate::of(&group) {
            ceiling.push(agg);
        }
    }

    // One factorial per (backend, metric). Each arm is DERIVED from the
    // measurements it aggregates — the key comes from them, not from here
    // (condition B1 / REVIEW_M3_5 M35-N3) — so a factorial cannot be
    // assembled out of arms that were not measured under one key.
    let mut backends: Vec<String> = arms.iter().map(|a| a.backend_id.clone()).collect();
    backends.sort();
    backends.dedup();
    let mut factorial = Vec::new();
    let mut geometry = Vec::new();
    let mut assembly_refusals: Vec<String> = Vec::new();
    let mut common_fixtures = 0u64;
    let mut dropped_fixtures = 0u64;
    for backend in &backends {
        // The two arms of a factorial must range over the SAME fixtures
        // (§27.6), and they do not automatically: an estimated-formation arm
        // is refused wherever the estimate is not realizable by the backend.
        // So the factorial runs over the INTERSECTION, and the arms the
        // intersection drops are counted rather than quietly averaged in.
        //
        // Nothing enforces this by convention: the derived key's fixture
        // component is a hash over the members' own, so two arms over
        // different fixture sets are refused at `insert` — which is how this
        // was found (condition B1 doing its work on the first run).
        let present = |source: FormationSource| -> std::collections::BTreeSet<(String, String)> {
            arms.iter()
                .filter(|a| &a.backend_id == backend && a.formation_source == source)
                .map(|a| (a.scene_id.clone(), a.cell_id.clone()))
                .collect()
        };
        let gt = present(FormationSource::GroundTruth);
        let est = present(FormationSource::Estimated);
        let shared: std::collections::BTreeSet<(String, String)> =
            gt.intersection(&est).cloned().collect();
        common_fixtures += shared.len() as u64;
        dropped_fixtures += (gt.len() - shared.len()) as u64;
        for metric in CEILING_METRICS {
            let mut set = CommensurableArms::new();
            for (pf, source) in [
                (PfArm::Pf10, FormationSource::Estimated),
                (PfArm::Pf11, FormationSource::GroundTruth),
            ] {
                let members: Vec<&CeilingArm> = arms
                    .iter()
                    .filter(|a| {
                        &a.backend_id == backend
                            && a.formation_source == source
                            && shared.contains(&(a.scene_id.clone(), a.cell_id.clone()))
                    })
                    .copied()
                    .collect();
                match FactorialArm::aggregate(pf.id(), metric, &members, reduce_for(metric)) {
                    Ok(a) => {
                        if let Err(e) = set.insert(&a) {
                            assembly_refusals.push(e.to_string());
                        }
                    }
                    Err(e) => assembly_refusals.push(e.to_string()),
                }
            }
            factorial.push(pf_effects(metric, &set));
            if *metric == CEILING_METRICS[0] {
                let members: Vec<&CeilingArm> = arms
                    .iter()
                    .filter(|a| {
                        &a.backend_id == backend
                            && a.formation_source == FormationSource::GroundTruth
                            && shared.contains(&(a.scene_id.clone(), a.cell_id.clone()))
                    })
                    .copied()
                    .collect();
                let mut g = CommensurableArms::new();
                if let Ok(a) =
                    FactorialArm::aggregate(GArm::G30.id(), metric, &members, reduce_for(metric))
                {
                    let _ = g.insert(&a);
                }
                geometry.extend(geometry_deltas(&g));
            }
        }
    }

    let warnings = derived_warnings(&run.arms, &ceiling, !factorial.is_empty());
    let clean_arms = arms
        .iter()
        .filter(|a| !a.inverse_crime.is_contaminated())
        .count() as u64;

    OracleReport {
        schema: ORACLE_SCHEMA,
        milestone: "M4",
        platform: Platform::current(),
        config: run.config.clone(),
        config_hash: run.config_hash.clone(),
        fixture_set_hash: run.fixture_set_hash.clone(),
        scenes: run.scenes,
        arms_measured: run.arms.len() as u64,
        arms_refused: run.refused.len() as u64,
        clean_arms,
        inverse_crime_arms: run.arms.len() as u64 - clean_arms,
        warnings,
        ceiling,
        ceiling_arms: run.arms.clone(),
        pf_arms: PfArm::ALL
            .iter()
            .map(|a| {
                ArmDeclaration::pf(
                    *a,
                    (a.missing().is_empty()).then(|| match a {
                        PfArm::Pf11 => "measured: GT partition + GT formation, which is also G30 \
                                         (the renderer/serialization ceiling above)"
                            .to_string(),
                        _ => "measured: GT partition + the formation ESTIMATED from the \
                              observation by vice-evidence (M4)"
                            .to_string(),
                    }),
                )
            })
            .collect(),
        assembly_refusals,
        factorial_common_fixtures: common_fixtures,
        factorial_dropped_fixtures: dropped_fixtures,
        g_arms: GArm::ALL
            .iter()
            .map(|a| {
                ArmDeclaration::g(
                    *a,
                    (a.missing().is_empty()).then(|| "measured: see `ceiling` above".to_string()),
                )
            })
            .collect(),
        factorial,
        geometry_deltas: geometry,
        refused: run.refused.clone(),
        mechanism_selftest: MechanismSelftest::run(),
        not_yet_produced: vec![
            "PF00/PF01 and all three factorial effects (auto partition, M4.5). PF10 joined the \
             measured arms in M4",
            "G00/G01/G10/G11/G20 and the geometry ladder deltas (M6)",
            "paint oracle (M8) and formation-expansion oracle (M9), per §27.6",
            "boundary/topology/primitive metrics: there is no vectorizer output to measure (M6+)",
            "the resize-chain cells of §27.2: the edge mask is defined at work resolution only",
        ],
    }
}

impl OracleReport {
    pub fn canonical_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("oracle report serializes")
    }
}

/// One effect or geometry delta, with every platform-dependent value gone.
///
/// A `CausalDelta` carries a `value` (a float) and a `key_fingerprint` (a
/// hash over a scene digest); both are dropped. Its ARMS and COEFFICIENTS
/// survive, because "which arms this contrast is over" is a fact about the
/// experimental design and not about libm.
fn outcome_projection(e: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "outcome": e["outcome"],
        "what": e["detail"]["what"],
        "missing": e["detail"]["missing"],
        "owner_milestone": e["detail"]["owner_milestone"],
        "reason": e["detail"]["reason"],
        "label": e["detail"]["label"],
        "terms": e["detail"]["terms"],
        "inverse_crime": e["detail"]["inverse_crime"],
    })
}

/// The platform-INDEPENDENT projection of an oracle report: composition, arm
/// identities, contamination status, refusal reasons and the design of every
/// contrast — and nothing that is a function of a float.
///
/// "A function of a float" is wider than "a float", and the first version of
/// this projection got that wrong (F-0022). `fixture_set_hash` is a sha256
/// over SCENE DIGESTS, and a scene digest is the canonical form of geometry
/// built with `sin`/`cos`; `key_fingerprint` contains that same hash. Both
/// are hex strings and neither looks like a number, but both change with the
/// platform, so a projection carrying them is not platform-independent no
/// matter what its doc comment says. `config_hash` is kept, because its
/// inputs — schema, scope, backend ids, cell ids, metric names — contain no
/// float at all.
pub fn structural_projection(v: &serde_json::Value) -> serde_json::Value {
    let arms: Vec<serde_json::Value> = v["ceiling_arms"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|a| {
            serde_json::json!({
                "arm": a["arm"],
                "scene_id": a["scene_id"],
                "cell_id": a["cell_id"],
                "backend_id": a["backend_id"],
                "observation_profile": a["observation_profile"],
                "backend_rasterizer": a["backend_rasterizer"],
                "arm": a["arm"],
                "formation_source": a["formation_source"],
                "formation": a["formation"],
                "formation_matches_gt": a["formation_matches_gt"],
                "inverse_crime": a["inverse_crime"],
                "serialization_digest_identical": a["metrics"]["serialization_digest_identical"],
            })
        })
        .collect();
    let factorial: Vec<serde_json::Value> = v["factorial"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|f| {
            serde_json::json!({
                "metric": f["metric"],
                "present_arms": f["present_arms"],
                "partition_main_effect": outcome_projection(&f["partition_main_effect"]),
                "formation_main_effect": outcome_projection(&f["formation_main_effect"]),
                "interaction": outcome_projection(&f["interaction"]),
            })
        })
        .collect();
    let geometry: Vec<serde_json::Value> = v["geometry_deltas"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|g| {
            serde_json::json!({
                "name": g["name"],
                "isolates": g["isolates"],
                "outcome": outcome_projection(&g["outcome"]),
            })
        })
        .collect();
    let ceiling: Vec<serde_json::Value> = v["ceiling"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|c| {
            serde_json::json!({
                "arm": c["arm"],
                "backend_id": c["backend_id"],
                "cell_id": c["cell_id"],
                "arms": c["arms"],
                "formation_matches_gt": c["formation_matches_gt"],
                "inverse_crime": c["inverse_crime"],
                "all_serialization_identical": c["all_serialization_identical"],
            })
        })
        .collect();
    serde_json::json!({
        "schema": v["schema"],
        "milestone": v["milestone"],
        "config": v["config"],
        // Kept: its inputs are schema, scope, backend ids, cell ids and
        // metric names - not one float among them.
        "config_hash": v["config_hash"],
        // NOT kept: `fixture_set_hash` and every `key_fingerprint`. They are
        // hashes OVER SCENE DIGESTS, hence functions of libm (F-0022).
        "scenes": v["scenes"],
        "arms_measured": v["arms_measured"],
        "arms_refused": v["arms_refused"],
        "clean_arms": v["clean_arms"],
        "inverse_crime_arms": v["inverse_crime_arms"],
        "warnings": v["warnings"],
        "pf_arms": v["pf_arms"],
        "g_arms": v["g_arms"],
        "factorial": factorial,
        "geometry_deltas": geometry,
        "refused": v["refused"],
        "assembly_refusals": v["assembly_refusals"],
        "factorial_common_fixtures": v["factorial_common_fixtures"],
        "factorial_dropped_fixtures": v["factorial_dropped_fixtures"],
        "ceiling": ceiling,
        "ceiling_arms": arms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;

    fn report() -> &'static OracleReport {
        static R: OnceLock<OracleReport> = OnceLock::new();
        R.get_or_init(|| build(super::super::tests::test_run()))
    }

    #[test]
    fn both_gate_rows_are_met_and_name_their_mechanism() {
        let r = report();
        let table = r.gate_table();
        assert_eq!(
            table.len(),
            3,
            "two clauses from 28 M3.5 and the factorial clause of 28 M4"
        );
        for (name, ok, why) in &table {
            assert!(*ok, "gate {name} not met: {why}");
            assert!(why.len() > 40, "gate {name} states no mechanism");
        }
    }

    /// Clause 1 is green ONLY because the machinery works, not because the
    /// delta set is empty. Each half is knocked out in turn.
    #[test]
    fn the_no_incompatible_delta_row_is_not_vacuous() {
        let row = |r: &OracleReport| r.gate_table()[0].1;
        assert!(row(report()));

        // (a) a key component that stops refusing.
        let mut broken = report().clone();
        broken.mechanism_selftest.key_components[0].mismatch_refused = false;
        broken
            .mechanism_selftest
            .all_key_components_refuse_a_mismatch = false;
        assert!(
            !row(&broken),
            "a key component that admits a mismatch must fail the gate"
        );

        // (b) a machinery that can never produce an effect at all would
        // satisfy "no incompatible deltas" trivially; the selftest is what
        // rules it out.
        let mut inert = report().clone();
        inert.mechanism_selftest.effects_produced = 0;
        assert!(!row(&inert));

        // (c) a factorial instance silently dropped.
        let mut short = report().clone();
        short.factorial.pop();
        assert!(!row(&short));

        // (c2) and the empty case, which is the one that would otherwise
        // pass by having nothing to check: `all()` over an empty set is
        // true and `0 == 0 * 3` holds. A report that publishes no effects
        // has not demonstrated the clause.
        let mut none = report().clone();
        none.factorial.clear();
        assert!(
            !row(&none),
            "a report with no factorial at all must not satisfy the clause"
        );

        // (d) the ladder published under a factorial name: if the main
        // effect equalled the sequential difference the harness would be
        // reporting the order-dependent quantity 27.6 replaced.
        let mut ladder = report().clone();
        ladder.mechanism_selftest.sequential_pf10_minus_pf00 =
            ladder.mechanism_selftest.partition_main_effect;
        assert!(!row(&ladder));
    }

    /// Clause 2 is green because the warning is attached, derivable and not
    /// constant. Each of those is knocked out in turn — including the one
    /// that matters most, a recorded flag that disagrees with the profiles
    /// it claims to describe.
    #[test]
    fn the_inverse_crime_row_is_not_vacuous() {
        let row = |r: &OracleReport| r.gate_table()[1].1;
        let r = report();
        assert!(row(r));
        assert!(r.inverse_crime_arms > 0 && r.clean_arms > 0);

        let mut silent = r.clone();
        silent.warnings.clear();
        assert!(
            !row(&silent),
            "a contaminated report with no warning must fail"
        );

        let mut lying = r.clone();
        let victim = lying
            .ceiling_arms
            .iter_mut()
            .find(|a| a.inverse_crime.is_contaminated())
            .expect("the run must contain a contaminated arm");
        victim.inverse_crime = InverseCrime::Clean;
        assert!(
            !row(&lying),
            "a flag that disagrees with the two profiles it describes must fail"
        );

        let mut lost = r.clone();
        lost.ceiling
            .iter_mut()
            .for_each(|a| a.inverse_crime = InverseCrime::Clean);
        assert!(
            !row(&lost),
            "an aggregate that dropped its arms' contamination must fail"
        );

        let mut constant = r.clone();
        constant.clean_arms = 0;
        assert!(
            !row(&constant),
            "with no clean arm the flag is constant and proves nothing"
        );
    }

    /// The warning survives every aggregation level present in the report,
    /// and the clean pairings are genuinely clean — the control without
    /// which the previous test would pass on a flag that is always set.
    #[test]
    fn contamination_reaches_every_aggregation_level_and_clean_pairings_exist() {
        let r = report();
        assert!(r.ceiling.iter().any(|a| !a.inverse_crime.is_contaminated()));
        assert!(r.ceiling.iter().any(|a| a.inverse_crime.is_contaminated()));
        for agg in r
            .ceiling
            .iter()
            .filter(|a| a.inverse_crime.is_contaminated())
        {
            assert!(r
                .ceiling_arms
                .iter()
                .filter(|a| a.backend_id == agg.backend_id && a.cell_id == agg.cell_id)
                .any(|a| a.inverse_crime.is_contaminated()));
        }
        // Every factorial arm inherits it through the fold, and the report
        // says so in words rather than leaving the reader to infer it.
        assert!(r.warnings.iter().any(|w| w.contains("INVERSE CRIME")));
        assert!(r
            .warnings
            .iter()
            .any(|w| w.contains("contaminated pairing")));
    }

    /// The absent arms are DATA: named, with an owner, and carrying no
    /// number anywhere in the artifact.
    #[test]
    fn absent_arms_are_named_refusals_and_never_zeros() {
        let r = report();
        let refused: Vec<&ArmDeclaration> = r
            .pf_arms
            .iter()
            .chain(&r.g_arms)
            .filter(|a| a.outcome.refusal().is_some())
            .collect();
        assert_eq!(
            refused.len(),
            7,
            "PF00/PF01 and G00/G01/G10/G11/G20 - PF10 became measurable in M4"
        );
        for a in refused {
            let x = a.outcome.refusal().unwrap();
            assert!(!x.missing.is_empty());
            assert!(["M4.5", "M6"].contains(&x.owner_milestone));
        }
        for arm in ["PF10", "PF11"] {
            assert!(
                r.pf_arms
                    .iter()
                    .find(|a| a.arm == arm)
                    .unwrap()
                    .outcome
                    .measured()
                    .is_some(),
                "{arm} must be measured in M4"
            );
        }
        assert!(r
            .g_arms
            .iter()
            .find(|a| a.arm == "G30")
            .unwrap()
            .outcome
            .measured()
            .is_some());
        assert!(!r.not_yet_produced.is_empty());
    }

    /// The Tier A rule applied to the NEW artifact, not only to the corpus:
    /// these metrics are libm-derived floats, so the report carries the
    /// platform and the platform is part of what a reader compares.
    #[test]
    fn the_report_carries_its_tier_a_platform_and_projects_without_floats() {
        let r = report();
        assert_eq!(r.platform, Platform::current());
        let v: serde_json::Value = serde_json::from_str(&r.canonical_json()).unwrap();
        let p = structural_projection(&v);
        // No libm-derived VALUE survives: neither the per-arm metrics block
        // nor the aggregate statistics. Metric NAMES do survive, and must —
        // dropping them would drop the identity of the columns.
        for arm in p["ceiling_arms"].as_array().unwrap() {
            assert!(arm.get("metrics").is_none());
            // The fingerprint is DROPPED: it is a hash over a scene digest,
            // hence a function of libm (F-0022). Identity across platforms
            // is the (scene, cell, backend) triple, which is text.
            assert!(arm.get("key_fingerprint").is_none());
            assert!(arm.get("scene_id").is_some());
            assert!(arm.get("cell_id").is_some());
            assert!(arm.get("backend_id").is_some());
            assert!(arm.get("inverse_crime").is_some());
        }
        for agg in p["ceiling"].as_array().unwrap() {
            for float_field in [
                "max_abs_code",
                "edge_mean_abs_code_mean",
                "edge_mean_abs_code_max",
                "identical_pixels_frac_min",
            ] {
                assert!(agg.get(float_field).is_none(), "{float_field} survived");
            }
            assert!(agg.get("inverse_crime").is_some());
        }
        assert!(
            !p["platform"].is_object(),
            "the projection drops the platform"
        );
        // And it is not empty of content: the same projection of a report
        // with one arm removed differs, so it can still catch composition.
        let mut fewer = r.clone();
        fewer.ceiling_arms.pop();
        let v2: serde_json::Value = serde_json::from_str(&fewer.canonical_json()).unwrap();
        assert_ne!(structural_projection(&v2), p);
    }

    /// F-0022: "a function of a float" is wider than "a float".
    ///
    /// The first projection kept `fixture_set_hash` and every
    /// `key_fingerprint`. Both are hex strings and neither looks like a
    /// number, but both are sha256 OVER SCENE DIGESTS, and a scene digest is
    /// the canonical form of geometry built with `sin`/`cos`. So the
    /// "platform-independent projection" changed with the platform, and CI
    /// found it on the first cross-platform run - while the corpus's own
    /// structural step, which drops scene digests, passed beside it.
    ///
    /// The walk enumerates the derived values from the report ITSELF rather
    /// than listing field names, so a hash added later is covered without
    /// anyone having to remember it.
    #[test]
    fn the_projection_carries_no_value_that_is_a_function_of_a_float() {
        let r = report();
        let v: serde_json::Value = serde_json::from_str(&r.canonical_json()).unwrap();
        let text = serde_json::to_string(&structural_projection(&v)).unwrap();

        let mut derived: std::collections::BTreeSet<String> = Default::default();
        derived.insert(r.fixture_set_hash.clone());
        for a in &r.ceiling_arms {
            derived.insert(a.key_fingerprint.clone());
        }
        for f in &r.factorial {
            derived.insert(f.key_fingerprint.clone());
        }
        assert!(
            derived.len() > 2,
            "the walk found nothing to check and would pass on any projection"
        );
        for h in &derived {
            assert!(
                !text.contains(h.as_str()),
                "the projection carries {h}, which is a hash over scene digests"
            );
        }
        // Control in the other direction: a hash whose inputs contain no
        // float MUST survive, or the projection loses the identity of what
        // it is comparing.
        assert!(
            text.contains(&r.config_hash),
            "config_hash has no float input and must stay comparable across platforms"
        );
    }

    #[test]
    fn the_report_is_deterministic() {
        let run = super::super::tests::test_run();
        assert_eq!(build(run).canonical_json(), build(run).canonical_json());
    }
}
