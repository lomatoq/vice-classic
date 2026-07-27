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

use serde::Serialize;

use super::ceiling::{CeilingAggregate, CeilingArm, CEILING_METRICS};
use super::crime::InverseCrime;
use super::design::{ArmDeclaration, GArm, PfArm};
use super::effects::{geometry_deltas, pf_effects, FactorialEffects, GeometryDelta};
use super::key::{CandidateBudget, CommensurableArms, CompatibilityKey, KEY_COMPONENTS};
use super::{OracleConfig, OracleRun, RefusedArm};
use crate::gt::corpus::Platform;
use crate::gt::raster::RasterProfile;
use crate::hashing::sha256_hex;

pub const ORACLE_SCHEMA: &str = "vice-classic/m3_5-oracle/v1";

/// One component of the compatibility key, and whether mutating it alone is
/// refused.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct KeyComponentCheck {
    pub component: &'static str,
    pub mismatch_refused: bool,
    pub refusal: String,
}

/// Evidence that the two gate mechanisms execute, produced by running them
/// on SYNTHETIC arms at report time.
///
/// These numbers measure nothing about the corpus and are not arms: the
/// values 1/2/4/8 are chosen to be visibly artificial and the block is named
/// so it cannot be read as data. Its only job is to answer the question a
/// reviewer would otherwise have to take on trust — "does the refusal ever
/// fire, or is everything simply absent?".
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MechanismSelftest {
    pub note: &'static str,
    pub synthetic_arm_values: Vec<(&'static str, f64)>,
    pub effects_produced: u64,
    pub partition_main_effect: f64,
    pub formation_main_effect: f64,
    pub interaction: f64,
    /// The sequential difference §27.6 replaced, printed next to the main
    /// effect it is NOT: if these two were ever equal the harness would be
    /// publishing the ladder under a factorial name.
    pub sequential_pf10_minus_pf00: f64,
    pub key_components: Vec<KeyComponentCheck>,
    pub all_key_components_refuse_a_mismatch: bool,
    pub contaminated_arm_contaminates_the_aggregate: bool,
    pub all_clean_arms_leave_the_aggregate_clean: bool,
}

fn selftest_key() -> CompatibilityKey {
    CompatibilityKey {
        backend_id: "selftest-backend".to_string(),
        config_hash: "selftest-config".to_string(),
        candidate_budget: CandidateBudget::NotApplicable,
        fixture_hash: "selftest-fixture".to_string(),
        intervention_schema_version: super::design::INTERVENTION_SCHEMA_VERSION.to_string(),
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

impl MechanismSelftest {
    pub fn run() -> MechanismSelftest {
        let key = selftest_key();
        let values: Vec<(&'static str, f64)> =
            vec![("PF00", 1.0), ("PF10", 2.0), ("PF01", 4.0), ("PF11", 8.0)];
        let mut set = CommensurableArms::new(key.clone());
        for (arm, v) in &values {
            set.insert(arm, &key, *v, InverseCrime::Clean)
                .expect("synthetic arms share one key");
        }
        let e = pf_effects("selftest", &set);
        let val = |o: &super::design::ArmOutcome<super::key::CausalDelta>| {
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

        // Walk the CLASS of key components: mutate each one alone and
        // record whether the set refuses. A component absent from this walk
        // would be a component the fingerprint does not defend.
        let key_components: Vec<KeyComponentCheck> = KEY_COMPONENTS
            .iter()
            .map(|component| {
                let mut probe = CommensurableArms::new(key.clone());
                probe
                    .insert("PF11", &key, 1.0, InverseCrime::Clean)
                    .expect("same key");
                let outcome =
                    probe.insert("PF10", &mutate(&key, component), 2.0, InverseCrime::Clean);
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

        // The aggregation property, on the state where it matters: one
        // contaminated arm among clean ones.
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
            contaminated_arm_contaminates_the_aggregate: mixed.is_contaminated(),
            all_clean_arms_leave_the_aggregate_clean: !clean.is_contaminated(),
        }
    }
}

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
    pub mechanism_selftest: MechanismSelftest,
    pub not_yet_produced: Vec<&'static str>,
}

/// Aggregate one metric over a set of arms, with the statistic that makes
/// the metric conservative: worst case for the error metrics, worst case for
/// the agreement fraction.
fn aggregate_metric(arms: &[&CeilingArm], metric: &str) -> f64 {
    let vals = arms.iter().filter_map(|a| a.metrics.get(metric));
    match metric {
        "max_abs_code" => vals.fold(f64::NEG_INFINITY, f64::max),
        "identical_pixels_frac" => vals.fold(f64::INFINITY, f64::min),
        _ => {
            let v: Vec<f64> = vals.collect();
            if v.is_empty() {
                f64::NAN
            } else {
                v.iter().sum::<f64>() / v.len() as f64
            }
        }
    }
}

pub fn build(run: &OracleRun) -> OracleReport {
    let arms: Vec<&CeilingArm> = run.arms.iter().collect();

    // Aggregates per (backend, observation cell): the level at which a
    // clean pairing and a contaminated one are both visible.
    let mut ceiling = Vec::new();
    let mut pairs: Vec<(String, String)> = arms
        .iter()
        .map(|a| (a.backend_id.clone(), a.cell_id.clone()))
        .collect();
    pairs.sort();
    pairs.dedup();
    for (backend, cell) in &pairs {
        let group: Vec<&CeilingArm> = arms
            .iter()
            .filter(|a| &a.backend_id == backend && &a.cell_id == cell)
            .copied()
            .collect();
        if let Some(agg) = CeilingAggregate::of(&group) {
            ceiling.push(agg);
        }
    }

    // One factorial per (backend, metric). The fixture side of the key is
    // the whole fixture SET, because §27.6 requires all arms of a factorial
    // to share fixtures - so the arm value is the aggregate over them.
    let mut backends: Vec<String> = arms.iter().map(|a| a.backend_id.clone()).collect();
    backends.sort();
    backends.dedup();
    let mut factorial = Vec::new();
    let mut geometry = Vec::new();
    for backend in &backends {
        let group: Vec<&CeilingArm> = arms
            .iter()
            .filter(|a| &a.backend_id == backend)
            .copied()
            .collect();
        let crime = InverseCrime::fold_all(group.iter().map(|a| &a.inverse_crime));
        let key = CompatibilityKey {
            backend_id: backend.clone(),
            config_hash: run.config_hash.clone(),
            candidate_budget: CandidateBudget::NotApplicable,
            fixture_hash: sha256_hex(
                format!(
                    "{}\u{1f}{}",
                    run.fixture_set_hash,
                    run.config.cells.join(",")
                )
                .as_bytes(),
            ),
            intervention_schema_version: super::design::INTERVENTION_SCHEMA_VERSION.to_string(),
        };
        for metric in CEILING_METRICS {
            let mut set = CommensurableArms::new(key.clone());
            // PF11 == G30: GT partition and GT formation, which is the one
            // arm this milestone can inject honestly.
            set.insert(
                PfArm::Pf11.id(),
                &key,
                aggregate_metric(&group, metric),
                crime.clone(),
            )
            .expect("the arm carries the set's own key");
            factorial.push(pf_effects(metric, &set));
            if *metric == CEILING_METRICS[0] {
                let mut g = CommensurableArms::new(key.clone());
                g.insert(
                    GArm::G30.id(),
                    &key,
                    aggregate_metric(&group, metric),
                    crime.clone(),
                )
                .expect("same key");
                geometry.extend(geometry_deltas(&g));
            }
        }
    }

    let overall = InverseCrime::fold_all(arms.iter().map(|a| &a.inverse_crime));
    let mut warnings: Vec<String> = overall.warnings().iter().map(|w| w.to_string()).collect();
    for agg in ceiling.iter().filter(|a| a.inverse_crime.is_contaminated()) {
        warnings.push(format!(
            "contaminated pairing: backend {} against observation profile {} (cell {})",
            agg.backend_id, agg.observation_profile, agg.cell_id
        ));
    }
    if !factorial.is_empty() && overall.is_contaminated() {
        warnings.push(
            "every factorial arm of this report inherits the contamination above through the \
             aggregation fold; no effect below may be read as evidence about accuracy"
                .to_string(),
        );
    }

    let clean_arms = arms
        .iter()
        .filter(|a| !a.inverse_crime.is_contaminated())
        .count() as u64;

    OracleReport {
        schema: ORACLE_SCHEMA,
        milestone: "M3.5",
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
                    (a.missing().is_empty()).then(|| {
                        "measured as G30: the renderer/serialization ceiling above".to_string()
                    }),
                )
            })
            .collect(),
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
            "PF00/PF01/PF10 and all three factorial effects (auto partition M4.5, formation \
             estimation M4)",
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

    /// The §28 M3.5 gate, as booleans computed from this report's own data.
    pub fn gate_table(&self) -> Vec<(&'static str, bool, String)> {
        // Clause 1. Every published effect is either a delta whose operands
        // carry the factorial's own key fingerprint, or a typed refusal that
        // names what is missing - AND the machinery demonstrably refuses a
        // mismatch on every key component and demonstrably produces effects
        // when the arms ARE commensurable. Without the second half this row
        // would be green because nothing was subtracted (meta-rule M-2).
        let effects_well_formed = self.factorial.iter().all(|f| {
            f.effects().iter().all(|o| match o {
                super::design::ArmOutcome::Measured(d) => {
                    d.key_fingerprint() == f.key_fingerprint && d.terms().len() == 4
                }
                super::design::ArmOutcome::NotYetApplicable(r) => {
                    !r.reason.is_empty() && !r.owner_milestone.is_empty()
                }
            })
        }) && self.geometry_deltas.iter().all(|g| match &g.outcome {
            super::design::ArmOutcome::Measured(d) => !d.terms().is_empty(),
            super::design::ArmOutcome::NotYetApplicable(r) => !r.reason.is_empty(),
        });
        let effect_count_intact = self.factorial.len()
            == self.config.backends.len() * CEILING_METRICS.len()
            && self.factorial.iter().all(|f| f.effects().len() == 3);
        let st = &self.mechanism_selftest;
        let mechanism_live = st.all_key_components_refuse_a_mismatch
            && st.effects_produced == 3
            && (st.partition_main_effect - st.sequential_pf10_minus_pf00).abs() > 1e-9;

        // Clause 2. The warning exists, is attached to the arms that earn
        // it, survives every aggregation, and is not constant.
        let recomputed_ok = self.ceiling.iter().all(|agg| {
            let fold = InverseCrime::fold_all(
                self.ceiling_arms
                    .iter()
                    .filter(|a| a.backend_id == agg.backend_id && a.cell_id == agg.cell_id)
                    .map(|a| &a.inverse_crime),
            );
            fold == agg.inverse_crime
        });
        // Re-derived from the two profile NAMES the artifact records, by the
        // same function that produced them: a reader of the JSON can repeat
        // this without trusting the flag stored next to it.
        let derived_ok = self.ceiling_arms.iter().all(|a| {
            match (
                RasterProfile::from_id(a.backend_rasterizer),
                RasterProfile::from_id(a.observation_profile),
            ) {
                (Some(b), Some(o)) => InverseCrime::of(b, o) == a.inverse_crime,
                _ => false,
            }
        });
        let warning_visible = self.inverse_crime_arms > 0
            && self.clean_arms > 0
            && !self.warnings.is_empty()
            && recomputed_ok
            && derived_ok
            && st.contaminated_arm_contaminates_the_aggregate
            && st.all_clean_arms_leave_the_aggregate_clean;

        vec![
            (
                "no causal deltas across incompatible runs",
                effects_well_formed && effect_count_intact && mechanism_live,
                format!(
                    "{} factorial instances x 3 effects, each a commensurable contrast or a typed \
                     refusal; the selftest shows all {} key components refuse a mismatch and that \
                     four commensurable arms DO yield 3 effects ({} vs the sequential {})",
                    self.factorial.len(),
                    st.key_components.len(),
                    st.partition_main_effect,
                    st.sequential_pf10_minus_pf00
                ),
            ),
            (
                "inverse-crime warning visible",
                warning_visible,
                format!(
                    "{} contaminated arms and {} clean ones; {} warning line(s); every aggregate \
                     status equals the fold of its own arms and every arm status equals the one \
                     derived from its two profiles",
                    self.inverse_crime_arms,
                    self.clean_arms,
                    self.warnings.len()
                ),
            ),
        ]
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
                "backend_id": c["backend_id"],
                "cell_id": c["cell_id"],
                "arms": c["arms"],
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
        assert_eq!(table.len(), 2, "spec 28 M3.5 states two clauses");
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
        assert_eq!(refused.len(), 8, "PF00/PF01/PF10 and G00/G01/G10/G11/G20");
        for a in refused {
            let x = a.outcome.refusal().unwrap();
            assert!(!x.missing.is_empty());
            assert!(["M4", "M4.5", "M6"].contains(&x.owner_milestone));
        }
        assert!(r
            .pf_arms
            .iter()
            .find(|a| a.arm == "PF11")
            .unwrap()
            .outcome
            .measured()
            .is_some());
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
