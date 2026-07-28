//! The recall report and the three §28 M4.5 gate rows.
//!
//! Every row is a conjunction with at least one control that CAN fail, and
//! the controls are not decorative — REVIEW_M3_5 M35-N4 is what a row that
//! is green because its population is empty looks like, and this file is
//! written against that.

use serde::Serialize;

use super::ambiguity::AmbiguityRow;
use super::{TopologyArm, TopologyRun};
use crate::gt::corpus::Platform;

pub const TOPOLOGY_REPORT_SCHEMA: &str = "vice-classic/topology-report/v1";

/// The population thresholds the CODE expects, and the anchor of the claim that
/// ties them to `configs/GATES_V1.toml`.
///
/// These are no longer what the gate rows compare against — that is
/// [`super::gate::TopologyGateConfig`], loaded from the file (RT45-A10). Their
/// job now is one half of a ratchet, and the halves are deliberately in
/// different files:
///
/// - change the constant without the file, and the claims check in
///   `gates/mod.rs` fails because code and file disagree;
/// - change the file without the constant, and it fails for the same reason;
/// - change both, and §27.7 refuses the commit, because a gate file and
///   production code may not move together.
///
/// RT45-A5 lowered `MIN_RECALL_ARMS` to 1 and shrank the envelope budget in ONE
/// commit and `gates-check` returned exit 0: §27.7's second sentence had
/// nothing to act on. It does now, and RT45-A10's follow-up — that the row read
/// the constant rather than the file, so the registration was of the SPELLING —
/// is why the row no longer reads these at all.
pub const MIN_RECALL_ARMS: u32 = 20;
/// Breadth in SHAPE FAMILIES, the unit §27.1 keeps splits by (M45-N3).
pub const MIN_RECALL_SHAPE_FAMILIES: u32 = 5;
pub const MIN_NON_TRIVIAL_GT_ARMS: u32 = 5;
pub const MIN_TOPOLOGY_PAIRS: u32 = 2;
pub const MIN_CLASSES_PER_RETAINING_PAIR: u32 = 2;

/// Recall over one population.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Recall {
    pub arms: u64,
    pub hits: u64,
    pub fraction: f64,
}

fn recall(arms: &[&TopologyArm], f: impl Fn(&TopologyArm) -> bool) -> Recall {
    let hits = arms.iter().filter(|a| f(a)).count() as u64;
    Recall {
        arms: arms.len() as u64,
        hits,
        fraction: if arms.is_empty() {
            0.0
        } else {
            hits as f64 / arms.len() as f64
        },
    }
}

/// How much each §11.1 field contributed.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FieldContribution {
    pub field: &'static str,
    /// Arms where this field produced a matching candidate.
    pub matched: u64,
    /// Arms where it was the ONLY field that did. This is the number that
    /// says a field earns its place: a field that is never the sole source
    /// of the answer is one whose neighbours already cover it.
    pub sole_source: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Bucket {
    pub axis: &'static str,
    pub value: String,
    pub recall: Recall,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PruningTotals {
    pub arms: u64,
    pub arms_with_budget_pruning: u64,
    pub budget_removed: u64,
    pub dominated_removed: u64,
    /// Arms where the budget removed a candidate CARRYING the GT reading —
    /// whether or not another one survived.
    ///
    /// The near-miss counter, and the one that is not a paraphrase: it can be
    /// non-zero in a world where recall is 100 %, which is what M45-N8 asked
    /// of it.
    pub arms_where_budget_removed_a_gt_class_candidate: u64,
    /// Arms where the budget removed a GT-carrying candidate and NONE
    /// survived — the §36 stop condition, computed from the removal record
    /// (before tier 3) against the envelope (after it).
    ///
    /// Still implied by `hits == arms`, and therefore no longer a conjunct of
    /// the clause: a number implied by its neighbour is not a second witness.
    /// It stays published because when recall does drop it says WHY — lost to
    /// pruning, or never generated at all.
    pub arms_where_budget_pruning_lost_the_last_gt_candidate: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ContinuationTotals {
    pub plans: u64,
    /// Steps of the §11.4 compound operation that half-exist. There is no
    /// `executed` count any more, and its absence is the point: M4.5 applies
    /// no topology edit at all (M45-N12).
    pub partially_executed_steps: u64,
    pub refused_steps: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TopologyReport {
    pub schema: &'static str,
    pub milestone: &'static str,
    /// Tier A platform (§5.5, F-0020).
    pub platform: Platform,
    pub config: super::TopologyConfigRecord,
    pub config_hash: String,
    pub fixture_set_hash: String,
    pub scenes: u64,
    pub arms_measured: u64,
    pub arms_refused: u64,
    pub sealed_audit_groups_skipped: u64,
    /// Ambiguity pairs the sealed-audit filter removed from clause 2.
    pub ambiguity_pairs_in_sealed_audit_skipped: u64,
    /// Arms excluded from the recall clause because the scene has an OPAQUE
    /// exterior. Named, counted, and NOT silently absent.
    pub opaque_exterior_arms_excluded: u64,
    /// The recall population: identifiable, supported, transparent exterior.
    pub identifiable_supported_arms: u64,
    /// Independent SOURCE GROUPS the recall population spans. §27.4 makes
    /// the source-scene family the unit of a reliability trial, so the arm
    /// count alone would overstate how wide the population is; this is the
    /// number a gate row may honestly quote about breadth.
    pub recall_source_groups: u64,
    /// SHAPE FAMILIES the recall population spans — the unit §27.1 keeps
    /// splits by and §27.4 calls the trial unit.
    ///
    /// Published beside `recall_source_groups` and cited by the gate row in
    /// preference to it: 18 group ids are 8 families, and `proc/annulus/{000,
    /// 001,003}` are three variants of one family that `split.rs` explains are
    /// not independent (M45-N3).
    pub recall_shape_families: u64,
    /// Renders the corpus labels `identifiable` that the M4 evidence stage
    /// refused before the topology stage ran, and the families they came from.
    ///
    /// The exclusion's COMPOSITION, not its size. The excluding predicate is
    /// the same pipeline whose output the clause checks, so difficulty
    /// correlates with refusal and the size alone is not disclosure.
    pub identifiable_arms_refused_before_topology: u64,
    /// Shape families present among the refused arms and ABSENT from the
    /// recall population entirely.
    pub families_absent_from_recall_population: Vec<String>,
    pub recall_all: Recall,
    pub recall_events_only: Recall,
    pub recall_fixed_only: Recall,
    /// Arms whose GT topology is NOT the trivial one component and no holes.
    /// Without these the recall clause would be asking whether the envelope
    /// contains a disk, which it always does.
    pub non_trivial_gt_arms: u64,
    pub recall_non_trivial: Recall,
    /// The KNOCKOUT: recall of an envelope built from a field unrelated to the
    /// scene, scored against the real ground truth (condition 4).
    ///
    /// This is the number that says how much of the population is a TEST. On
    /// the trivial arms it is close to the real recall, because (1, 0) is what
    /// almost any blob reads as; the non-trivial figure beside it is where the
    /// metric discriminates.
    pub recall_unrelated_field: Recall,
    pub recall_unrelated_field_non_trivial: Recall,
    /// The knockout's POSITIVE control, and the reason the clause can no longer
    /// be green with a knockout that measures nothing.
    ///
    /// RT45-A12: `recall_unrelated_field.hits < recall_all.hits` is satisfied
    /// TRIVIALLY when the knockout is zeroed. Shrinking the disk radius from
    /// `0.3` to `0.0001` empties the unrelated field, takes its recall to 0 of
    /// 100, and the clause stays MET while its only control has stopped
    /// discriminating. The control was knocked out in the direction where it
    /// FAILS and never in the direction where it goes EMPTY.
    ///
    /// A centred disk IS the trivial topology, so on arms whose ground truth is
    /// one component and no holes the knockout is supposed to SUCCEED. It does,
    /// on all of them; an empty field succeeds nowhere. The clause requires
    /// this number to be non-zero, so both directions are now live.
    pub recall_unrelated_field_trivial: Recall,
    /// Arms whose ground truth is non-trivial under BOTH complementary
    /// conventions, and the two recalls over them (condition 17).
    ///
    /// `non_trivial_gt_arms` counts arms non-trivial under AT LEAST ONE
    /// convention, and a reader may reasonably take it for the harder question.
    /// This is the harder question, and REVIEW_M4_5 addendum 2 computed it
    /// before I published it: 24 arms, real recall 24 of 24, knockout 0 of 24.
    /// It is the sharpest contrast in this report and it was missing from it.
    pub non_trivial_gt_arms_both_conventions: u64,
    pub recall_non_trivial_both_conventions: Recall,
    pub recall_unrelated_field_non_trivial_both: Recall,
    /// Shape families of the both-conventions non-trivial subpopulation
    /// (RT45-A8 remainder). Breadth of the part of the corpus that carries the
    /// clause's discriminating power, in the unit §27.1 keeps splits by.
    pub non_trivial_both_shape_families: Vec<String>,
    /// The same breadth as a scalar, so a gate row can be bound to it POSITION
    /// BY POSITION. A list is not a declared value; a count is.
    pub non_trivial_both_shape_families_count: u64,
    /// Arms whose envelope carries candidates from BOTH complementary arms,
    /// and arms where one of the two contributed nothing.
    ///
    /// The second number is the one that matters: it is zero today, and it is
    /// what makes the clause-1 relaxation ("a candidate matching either
    /// convention counts") a measured statement rather than a promise
    /// (RT45-A1).
    pub arms_with_both_connectivity_arms: u64,
    pub arms_missing_a_connectivity_arm: u64,
    pub field_contributions: Vec<FieldContribution>,
    pub buckets: Vec<Bucket>,
    pub pruning: PruningTotals,
    pub continuation: ContinuationTotals,
    pub tie_batches_max: u32,
    pub largest_batch_pixels: u32,
    pub saddle_alternatives_total: u64,
    pub ambiguity: Vec<AmbiguityRow>,
    /// Intentionally ambiguous pairs the corpus carries, of which
    /// `topology_pairs` are about topology. Published so a gate row can
    /// quote both numbers as measurements rather than one as a constant.
    pub ambiguity_pairs: u64,
    pub topology_pairs: u64,
    /// Topology pairs that actually CARRY clause 2 — both readings retained
    /// from both renders — as opposed to being excused.
    ///
    /// The conjunct `topology_pairs >= 2` was written with the comment "or the
    /// row would be a statement about one fixture" and did not prevent exactly
    /// that, because it counts pairs BEFORE the excuse is applied (M45-N4).
    /// The number after the excuse is published here, the row prints it, and
    /// STATUS records that clause 2 stands on n = 1. Raising the conjunct to
    /// this number would turn the row red on a corpus that has only two
    /// topology pairs to begin with; naming the weakness is the honest move,
    /// inflating the corpus to hide it is not.
    pub topology_pairs_carrying_the_row: u64,
    pub arms: Vec<TopologyArm>,
    pub refused: Vec<super::RefusedArm>,
    pub warnings: Vec<String>,
}

fn is_recall_population(a: &TopologyArm) -> bool {
    a.identifiability == "identifiable" && a.outcome.starts_with("supported")
}

fn plain(g: super::GtSignature) -> bool {
    g.components == 1 && g.holes == 0
}

/// Non-trivial under AT LEAST ONE convention.
fn non_trivial(a: &TopologyArm) -> bool {
    !(plain(a.gt_four) && plain(a.gt_eight))
}

/// Non-trivial under BOTH conventions — the harder question (condition 17).
fn non_trivial_under_both(a: &TopologyArm) -> bool {
    !plain(a.gt_four) && !plain(a.gt_eight)
}

pub fn build(run: &TopologyRun) -> TopologyReport {
    let pop: Vec<&TopologyArm> = run
        .arms
        .iter()
        .filter(|a| is_recall_population(a))
        .collect();
    let nontrivial: Vec<&TopologyArm> = pop.iter().copied().filter(|a| non_trivial(a)).collect();
    // The two subpopulations the knockout is measured over separately: arms
    // non-trivial under BOTH conventions (where a disk cannot possibly be the
    // answer) and arms trivial under both (where a disk is exactly the answer,
    // which is what makes the knockout's success there a positive control).
    let nontrivial_both: Vec<&TopologyArm> = pop
        .iter()
        .copied()
        .filter(|a| non_trivial_under_both(a))
        .collect();
    let trivial: Vec<&TopologyArm> = pop.iter().copied().filter(|a| !non_trivial(a)).collect();
    let mut nt_both_families: Vec<String> = nontrivial_both
        .iter()
        .map(|a| a.shape_family.clone())
        .collect();
    nt_both_families.sort();
    nt_both_families.dedup();

    let mut field_contributions = Vec::new();
    for field in vice_topology::FieldKind::ALL {
        let name = field.as_str();
        let matched = pop
            .iter()
            .filter(|a| a.matching_fields.contains(&name))
            .count() as u64;
        let sole = pop.iter().filter(|a| a.unique_field == Some(name)).count() as u64;
        field_contributions.push(FieldContribution {
            field: name,
            matched,
            sole_source: sole,
        });
    }

    let mut axis_values: Vec<(&'static str, String)> = Vec::new();
    for a in &pop {
        for v in [
            ("profile", a.profile.to_string()),
            ("resolution_px", a.size_px.to_string()),
            ("split", a.split.to_string()),
        ] {
            if !axis_values.contains(&v) {
                axis_values.push(v);
            }
        }
    }
    axis_values.sort();
    let buckets: Vec<Bucket> = axis_values
        .into_iter()
        .map(|(axis, value)| {
            let subset: Vec<&TopologyArm> = pop
                .iter()
                .copied()
                .filter(|a| match axis {
                    "profile" => a.profile == value,
                    "resolution_px" => a.size_px.to_string() == value,
                    _ => a.split == value,
                })
                .collect();
            Bucket {
                axis,
                value,
                recall: recall(&subset, |a| a.gt_in_envelope),
            }
        })
        .collect();

    let families: std::collections::BTreeSet<&str> =
        pop.iter().map(|a| a.shape_family.as_str()).collect();
    let refused_families: std::collections::BTreeSet<&str> = run
        .refused
        .iter()
        .map(|r| r.shape_family.as_str())
        .collect();
    let absent: Vec<String> = refused_families
        .difference(&families)
        .map(|f| (*f).to_string())
        .collect();

    let mut warnings = Vec::new();
    if pop.is_empty() {
        warnings.push("the recall population is empty; every recall row below is vacuous".into());
    }
    if nontrivial.is_empty() {
        warnings.push(
            "no arm in the recall population has a non-trivial GT topology; the clause would be \
             asking whether a disk is in the envelope"
                .into(),
        );
    }

    let topology_pairs = run.ambiguity.iter().filter(|r| r.is_topology_pair).count() as u64;

    TopologyReport {
        schema: TOPOLOGY_REPORT_SCHEMA,
        milestone: "M4.5",
        platform: Platform::current(),
        config: run.config.clone(),
        config_hash: run.config_hash.clone(),
        fixture_set_hash: run.fixture_set_hash.clone(),
        scenes: run.scenes,
        arms_measured: run.arms.len() as u64,
        arms_refused: run.refused.len() as u64,
        sealed_audit_groups_skipped: run.sealed_audit_groups_skipped,
        ambiguity_pairs_in_sealed_audit_skipped: run.ambiguity_pairs_in_sealed_audit_skipped,
        opaque_exterior_arms_excluded: run.opaque_exterior_arms,
        identifiable_supported_arms: pop.len() as u64,
        recall_source_groups: pop
            .iter()
            .map(|a| a.group_id.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len() as u64,
        recall_shape_families: families.len() as u64,
        identifiable_arms_refused_before_topology: run
            .refused
            .iter()
            .filter(|r| r.identifiability == "identifiable")
            .count() as u64,
        families_absent_from_recall_population: absent,
        recall_all: recall(&pop, |a| a.gt_in_envelope),
        recall_events_only: recall(&pop, |a| a.gt_in_envelope_events_only),
        recall_fixed_only: recall(&pop, |a| a.gt_in_envelope_fixed_only),
        non_trivial_gt_arms: nontrivial.len() as u64,
        recall_non_trivial: recall(&nontrivial, |a| a.gt_in_envelope),
        recall_unrelated_field: recall(&pop, |a| a.gt_in_envelope_unrelated_field),
        recall_unrelated_field_non_trivial: recall(&nontrivial, |a| {
            a.gt_in_envelope_unrelated_field
        }),
        recall_unrelated_field_trivial: recall(&trivial, |a| a.gt_in_envelope_unrelated_field),
        non_trivial_gt_arms_both_conventions: nontrivial_both.len() as u64,
        recall_non_trivial_both_conventions: recall(&nontrivial_both, |a| a.gt_in_envelope),
        recall_unrelated_field_non_trivial_both: recall(&nontrivial_both, |a| {
            a.gt_in_envelope_unrelated_field
        }),
        non_trivial_both_shape_families_count: nt_both_families.len() as u64,
        non_trivial_both_shape_families: nt_both_families,
        arms_with_both_connectivity_arms: pop
            .iter()
            .filter(|a| a.candidates_by_arm.0 > 0 && a.candidates_by_arm.1 > 0)
            .count() as u64,
        arms_missing_a_connectivity_arm: pop
            .iter()
            .filter(|a| a.candidates_by_arm.0 == 0 || a.candidates_by_arm.1 == 0)
            .count() as u64,
        field_contributions,
        buckets,
        pruning: PruningTotals {
            arms: run.arms.len() as u64,
            arms_with_budget_pruning: run.arms.iter().filter(|a| a.budget_removed > 0).count()
                as u64,
            budget_removed: run.arms.iter().map(|a| a.budget_removed as u64).sum(),
            dominated_removed: run.arms.iter().map(|a| a.dominated_removed as u64).sum(),
            arms_where_budget_removed_a_gt_class_candidate: pop
                .iter()
                .filter(|a| a.budget_removed_gt_class_candidates > 0)
                .count() as u64,
            arms_where_budget_pruning_lost_the_last_gt_candidate: pop
                .iter()
                .filter(|a| a.budget_removed_the_last_gt_class_candidate)
                .count() as u64,
        },
        continuation: ContinuationTotals {
            plans: run.arms.iter().map(|a| a.continuation_plans as u64).sum(),
            partially_executed_steps: run
                .arms
                .iter()
                .map(|a| a.continuation_partial_steps as u64)
                .sum(),
            refused_steps: run
                .arms
                .iter()
                .map(|a| a.continuation_refused_steps as u64)
                .sum(),
        },
        tie_batches_max: run.arms.iter().map(|a| a.tie_batches).max().unwrap_or(0),
        largest_batch_pixels: run
            .arms
            .iter()
            .map(|a| a.largest_batch_pixels)
            .max()
            .unwrap_or(0),
        saddle_alternatives_total: run.arms.iter().map(|a| a.saddle_alternatives as u64).sum(),
        ambiguity_pairs: run.ambiguity.len() as u64,
        topology_pairs_carrying_the_row: run
            .ambiguity
            .iter()
            .filter(|p| {
                p.is_topology_pair
                    && p.both_retained_from_a == Some(true)
                    && p.both_retained_from_b == Some(true)
            })
            .count() as u64,
        ambiguity: run.ambiguity.clone(),
        topology_pairs,
        arms: run.arms.clone(),
        refused: run.refused.clone(),
        warnings,
    }
}

impl TopologyReport {
    /// The arms a recall clause is computed over, recomputed from the arms the
    /// report carries rather than read off an aggregate.
    ///
    /// The gate's threshold sites use this as the INPUT side, so an aggregate
    /// that disagrees with the arms it was built from is a finding (RT45-A16,
    /// RT45-A23).
    pub fn recall_population(&self) -> Vec<&TopologyArm> {
        self.arms
            .iter()
            .filter(|a| is_recall_population(a))
            .collect()
    }

    pub fn canonical_json(&self) -> String {
        serde_json::to_string(self).expect("topology report serializes")
    }
}

/// The platform-INDEPENDENT projection: composition, arm identities,
/// outcomes, GT signatures and the recall booleans — every one of which is a
/// count or a label, and none of which is a float.
///
/// F-0022 in force: `fixture_set_hash` is a sha256 over scene digests, hence
/// a function of libm, and it does not survive. `config_hash` does — its
/// inputs are the schema, the scope, cell ids and integer/lliteral config
/// constants of this source tree.
///
/// The signatures DO survive, and that is the point of this projection
/// rather than an oversight: a component count is an integer that a
/// different libm cannot move without moving the topology, so a drift big
/// enough to change an answer is exactly what the structural mode sees.
pub fn structural_projection(v: &serde_json::Value) -> serde_json::Value {
    let arms: Vec<serde_json::Value> = v["arms"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|a| {
            serde_json::json!({
                "scene_id": a["scene_id"],
                "group_id": a["group_id"],
                "shape_family": a["shape_family"],
                "cell_id": a["cell_id"],
                "split": a["split"],
                "profile": a["profile"],
                "identifiability": a["identifiability"],
                "exterior_truth": a["exterior_truth"],
                "outcome": a["outcome"],
                "gt_four": a["gt_four"],
                "gt_eight": a["gt_eight"],
                "gt_in_envelope": a["gt_in_envelope"],
                "gt_in_envelope_events_only": a["gt_in_envelope_events_only"],
                "gt_in_envelope_fixed_only": a["gt_in_envelope_fixed_only"],
                "gt_in_envelope_unrelated_field": a["gt_in_envelope_unrelated_field"],
                "candidates_by_arm": a["candidates_by_arm"],
                "matching_fields": a["matching_fields"],
            })
        })
        .collect();
    let ambiguity: Vec<serde_json::Value> = v["ambiguity"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|p| {
            serde_json::json!({
                "group_id": p["group_id"],
                "family": p["family"],
                "collapse_cell": p["collapse_cell"],
                "separate_cell": p["separate_cell"],
                "sig_a": p["sig_a"],
                "sig_b": p["sig_b"],
                "is_topology_pair": p["is_topology_pair"],
                "both_retained_from_a": p["both_retained_from_a"],
                "both_retained_from_b": p["both_retained_from_b"],
                "identifiability_at_collapse_a": p["identifiability_at_collapse_a"],
                "identifiability_at_collapse_b": p["identifiability_at_collapse_b"],
                "both_retained_fixed_only_from_a": p["both_retained_fixed_only_from_a"],
                "both_retained_fixed_only_from_b": p["both_retained_fixed_only_from_b"],
            })
        })
        .collect();
    serde_json::json!({
        "schema": v["schema"],
        "milestone": v["milestone"],
        "config_hash": v["config_hash"],
        "scenes": v["scenes"],
        "arms_measured": v["arms_measured"],
        "arms_refused": v["arms_refused"],
        "sealed_audit_groups_skipped": v["sealed_audit_groups_skipped"],
        "ambiguity_pairs_in_sealed_audit_skipped": v["ambiguity_pairs_in_sealed_audit_skipped"],
        "opaque_exterior_arms_excluded": v["opaque_exterior_arms_excluded"],
        "identifiable_supported_arms": v["identifiable_supported_arms"],
        "recall_source_groups": v["recall_source_groups"],
        "recall_shape_families": v["recall_shape_families"],
        "identifiable_arms_refused_before_topology": v["identifiable_arms_refused_before_topology"],
        "families_absent_from_recall_population": v["families_absent_from_recall_population"],
        "recall_all": {"arms": v["recall_all"]["arms"], "hits": v["recall_all"]["hits"]},
        "recall_events_only": {
            "arms": v["recall_events_only"]["arms"],
            "hits": v["recall_events_only"]["hits"]
        },
        "recall_fixed_only": {
            "arms": v["recall_fixed_only"]["arms"],
            "hits": v["recall_fixed_only"]["hits"]
        },
        "non_trivial_gt_arms": v["non_trivial_gt_arms"],
        "recall_unrelated_field": {
            "arms": v["recall_unrelated_field"]["arms"],
            "hits": v["recall_unrelated_field"]["hits"]
        },
        "recall_unrelated_field_non_trivial": {
            "arms": v["recall_unrelated_field_non_trivial"]["arms"],
            "hits": v["recall_unrelated_field_non_trivial"]["hits"]
        },
        "recall_unrelated_field_trivial": {
            "arms": v["recall_unrelated_field_trivial"]["arms"],
            "hits": v["recall_unrelated_field_trivial"]["hits"]
        },
        "non_trivial_gt_arms_both_conventions": v["non_trivial_gt_arms_both_conventions"],
        "recall_non_trivial_both_conventions": {
            "arms": v["recall_non_trivial_both_conventions"]["arms"],
            "hits": v["recall_non_trivial_both_conventions"]["hits"]
        },
        "recall_unrelated_field_non_trivial_both": {
            "arms": v["recall_unrelated_field_non_trivial_both"]["arms"],
            "hits": v["recall_unrelated_field_non_trivial_both"]["hits"]
        },
        "non_trivial_both_shape_families": v["non_trivial_both_shape_families"],
        "non_trivial_both_shape_families_count": v["non_trivial_both_shape_families_count"],
        "arms_with_both_connectivity_arms": v["arms_with_both_connectivity_arms"],
        "arms_missing_a_connectivity_arm": v["arms_missing_a_connectivity_arm"],
        "field_contributions": v["field_contributions"],
        "ambiguity_pairs": v["ambiguity_pairs"],
        "topology_pairs": v["topology_pairs"],
        "topology_pairs_carrying_the_row": v["topology_pairs_carrying_the_row"],
        "arms": arms,
        "ambiguity": ambiguity,
        "refused": v["refused"],
        "warnings": v["warnings"],
    })
}
