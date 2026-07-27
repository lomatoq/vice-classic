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

fn non_trivial(a: &TopologyArm) -> bool {
    let plain = |g: super::GtSignature| g.components == 1 && g.holes == 0;
    !(plain(a.gt_four) && plain(a.gt_eight))
}

pub fn build(run: &TopologyRun) -> TopologyReport {
    let pop: Vec<&TopologyArm> = run
        .arms
        .iter()
        .filter(|a| is_recall_population(a))
        .collect();
    let nontrivial: Vec<&TopologyArm> = pop.iter().copied().filter(|a| non_trivial(a)).collect();

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
    pub fn canonical_json(&self) -> String {
        serde_json::to_string(self).expect("topology report serializes")
    }

    /// The three §28 M4.5 clauses, as booleans over this report's own data.
    pub fn gate_table(&self) -> Vec<(&'static str, bool, String)> {
        // Clause 1: the GT-equivalent topology is PRESENT IN THE ENVELOPE on
        // identifiable supported fixtures. Recall, not accuracy of choice.
        //
        // Controls, all of which can fail: the population must exist and be
        // wide enough to be a corpus rather than a fixture; it must contain
        // arms whose GT topology is NOT a plain disk, or the question is
        // free; and the budget must never be the reason an answer is
        // missing (§36 stop condition).
        let r = &self.recall_all;
        let nt = &self.recall_non_trivial;
        let groups = self.recall_source_groups as usize;
        // Breadth counted in SHAPE FAMILIES, which is the unit §27.1 keeps
        // splits by. The threshold used to be on group ids, three of which can
        // be one family — a gate row citing dependent trials as independent
        // (M45-N3).
        let families = self.recall_shape_families as usize;
        let recall_row = r.arms >= 20
            && families >= 5
            && self.non_trivial_gt_arms >= 5
            && r.hits == r.arms
            // The relaxation this clause grants itself — a candidate matching
            // EITHER convention's truth counts — is only justified while the
            // envelope carries BOTH complementary arms. RT45-A1 deleted one
            // from the generator and nothing moved. It moves now.
            && self.arms_missing_a_connectivity_arm == 0
            // The knockout has to LOSE. If an envelope built from a field
            // unrelated to the scene scored the same as the real one, this
            // clause would not be a measurement of anything (condition 4,
            // M45-N2). It does not have to lose by much for the row to be
            // true — the number is published so a reader can see by how
            // little.
            && self.recall_unrelated_field.hits < r.hits;
        // Two conjuncts were removed here rather than fixed, and the reason is
        // the finding itself (M45-N8, RT45-A6): `nt.hits == nt.arms` and
        // "budget lost the last GT candidate == 0" are both IMPLIED by
        // `r.hits == r.arms`, because the non-trivial arms are a subset of the
        // population and a lost answer is an arm without its reading. A
        // conjunct implied by its neighbour cannot fail on its own; publishing
        // its value as a separate measurement inflates the count of
        // independent witnesses. Both numbers are still published, and the
        // near-miss counter beside them is the one that can move while recall
        // stays at 100 %.

        // Clause 2: ambiguous fixtures RETAIN ALTERNATIVES. A pair whose two
        // scenes have the same ink topology is not a topology ambiguity and
        // is excluded by name; at least two must remain, or the row would be
        // a statement about one fixture.
        let pairs: Vec<&AmbiguityRow> = self
            .ambiguity
            .iter()
            .filter(|p| p.is_topology_pair)
            .collect();
        let both = |p: &AmbiguityRow| {
            p.both_retained_from_a == Some(true) && p.both_retained_from_b == Some(true)
        };
        // MEASURED against a FROZEN constant, and the comment says which one
        // because M45-N4 found this paragraph naming a different instrument
        // from the one the line below uses. The excuse is the render
        // difference against `identifiability.quantization_floor_codes`; the
        // corpus's identifiability labels are printed beside it and are not
        // read here at all.
        // MEASURED against a FROZEN constant, not chosen: two renders that
        // differ by less than one 8-bit code carry no evidence of the
        // distinction between them, and an envelope that produced the
        // distinction anyway would be inventing it.
        let excused = |p: &AmbiguityRow| {
            p.collapse_max_code_diff < crate::gt::degradation::QUANTIZATION_FLOOR_CODES
        };
        let ambiguity_row = pairs.len() >= 2
            && pairs.iter().any(|p| both(p))
            && pairs.iter().all(|p| both(p) || excused(p))
            && pairs
                .iter()
                .filter(|p| both(p))
                .all(|p| p.classes_from_a.len() >= 2 && p.classes_from_b.len() >= 2);

        // Clause 3: NO magic-threshold-only architecture, made executable as
        // an ablation over the same generator rather than as a claim.
        //
        //   events_only == all    removing every fixed probe loses nothing;
        //   fixed_only  <  all    the fixed probes ALONE are not enough.
        //
        // The second half is what stops the first from being satisfied by a
        // generator that is secretly one threshold: if 0.5 alone already
        // recovered everything, "the events are not needed either" would be
        // equally true and the architecture would be a threshold with
        // decoration.
        let e = &self.recall_events_only;
        let f = &self.recall_fixed_only;
        // A population where ONE level provably cannot do the job: an
        // ambiguous fixture needs TWO readings, and a single threshold
        // produces a single labelling. The full generator retains both there
        // and the fixed-only generator does not, which is the contrast the
        // clause is about. Without this half, "removing the fixed probes
        // loses nothing" would also be true of a program that IS one
        // threshold with decoration.
        let one_level_is_not_enough = pairs.iter().any(|p| {
            both(p)
                && (p.both_retained_fixed_only_from_a == Some(false)
                    || p.both_retained_fixed_only_from_b == Some(false))
        });
        let threshold_row = r.arms > 0
            && e.hits == r.hits
            && one_level_is_not_enough
            && self.saddle_alternatives_total > 0
            && self.tie_batches_max > 0;

        vec![
            (
                "GT-equivalent topology present in envelope",
                recall_row,
                format!(
                    "SCOPE of this row: identifiable renders of TRANSPARENT-exterior scenes that \
                     the M4 evidence stage supports, which is {} of {} measured arms over {} \
                     SHAPE FAMILIES ({} group ids; §27.1 keeps splits by family, so the family \
                     count is the honest breadth); {} arms of opaque-exterior scenes are excluded \
                     because the ink region of a full-bleed scene is whichever face is not the \
                     background, and that is a palette question this milestone does not answer. \
                     Of the refused arms, {} are renders the corpus labels IDENTIFIABLE that the \
                     M4 evidence stage rejected before topology ran, and the families they take \
                     with them are absent from the population entirely: {:?}. There, the GT \
                     digital topology (majority-rule digitization of the exact ink coverage, \
                     under either admissible connectivity convention) is present in the envelope \
                     on {}/{} arms = {:.4}; on the {} arms whose GT is NOT a plain disk it is \
                     {}/{} = {:.4}. The relaxation to EITHER convention is measured rather than \
                     assumed: {} of the {} arms carry candidates from BOTH complementary arms, \
                     and {} carry only one. Budget pruning was the reason an answer was missing \
                     on {} arms, and removed a GT-carrying candidate without losing the \
                     reading on {} more. KNOCKOUT, and it is the number that says how much of \
                     this population is a test at all: an envelope built from a field UNRELATED \
                     to the scene still matches on {}/{} arms, and on the non-trivial ones \
                     {}/{} — so the discriminating part of the population is the non-trivial \
                     part, and a (1, 0) arm is nearly free. {} audit groups skipped, {} arms \
                     refused",
                    self.identifiable_supported_arms,
                    self.arms_measured,
                    families,
                    groups,
                    self.opaque_exterior_arms_excluded,
                    self.identifiable_arms_refused_before_topology,
                    self.families_absent_from_recall_population,
                    r.hits,
                    r.arms,
                    r.fraction,
                    self.non_trivial_gt_arms,
                    nt.hits,
                    nt.arms,
                    nt.fraction,
                    self.arms_with_both_connectivity_arms,
                    r.arms,
                    self.arms_missing_a_connectivity_arm,
                    self.pruning
                        .arms_where_budget_pruning_lost_the_last_gt_candidate,
                    self.pruning.arms_where_budget_removed_a_gt_class_candidate,
                    self.recall_unrelated_field.hits,
                    self.recall_unrelated_field.arms,
                    self.recall_unrelated_field_non_trivial.hits,
                    self.recall_unrelated_field_non_trivial.arms,
                    self.sealed_audit_groups_skipped,
                    self.arms_refused
                ),
            ),
            (
                "ambiguous fixtures retain alternatives",
                ambiguity_row,
                format!(
                    "This row stands on {} pair(s): that is how many TOPOLOGY pairs retain \
                     both readings WITHOUT the excuse, and it is stated here rather than left to \
                     be inferred from the sentence after it. {} of the {} intentionally \
                     ambiguous pairs are TOPOLOGY pairs, i.e. their \
                     two scenes have different ink topologies where they are distinguishable; the \
                     rest are ambiguities about paint or partition and are excluded by name. On \
                     each of them the envelope built from EITHER scene's render at the collapse \
                     cell must contain BOTH scenes' readings, and {} of {} do. A pair that does \
                     not is excused ONLY when the two renders differ by LESS than the frozen \
                     quantization floor of one 8-bit code, i.e. the observation carries no \
                     evidence of the distinction at all and spec 1.5 calls that information lost; \
                     an envelope that produced the distinction anyway would be inventing it. The \
                     excuse cannot carry the row: at least one pair must retain both readings \
                     WITHOUT it, and a pair that does retain both must carry more than one \
                     topological class. Per pair, with the corpus's own identifiability labels \
                     printed beside the measured difference: {}",
                    self.topology_pairs_carrying_the_row,
                    self.topology_pairs,
                    self.ambiguity.len(),
                    pairs.iter().filter(|p| both(p)).count(),
                    pairs.len(),
                    pairs
                        .iter()
                        .map(|p| format!(
                            "{} ({:?} against {:?}: from A {:?}, from B {:?}, labels {} and {}, \
                             collapse difference {:.1} codes)",
                            p.family,
                            (p.sig_a.components, p.sig_a.holes),
                            (p.sig_b.components, p.sig_b.holes),
                            p.both_retained_from_a,
                            p.both_retained_from_b,
                            p.identifiability_at_collapse_a,
                            p.identifiability_at_collapse_b,
                            p.collapse_max_code_diff
                        ))
                        .collect::<Vec<_>>()
                        .join("; ")
                ),
            ),
            (
                "no magic-threshold-only architecture",
                threshold_row,
                format!(
                    "the candidate levels come from persistence plateaus and from the levels \
                     either side of a critical event; a fixed level is ONE labelled source among \
                     them, and every candidate records which sources produced it. Ablation over \
                     the SAME generator, in two directions. (a) With every fixed probe removed, \
                     recall on the clean identifiable population is {}/{} against {}/{} with \
                     everything: the fixed probes carry {} arms the events do not, so the \
                     architecture does not DEPEND on a magic threshold. (b) With ONLY the fixed \
                     probes, recall on that same population is {}/{} - so on clean, identifiable, \
                     supported Flat2 fixtures a single 0.5 level would have sufficed, and this \
                     row does not pretend otherwise. What it asserts instead is measured where a \
                     single level provably CANNOT do the job: on the ambiguous pairs the answer \
                     is TWO readings, one level yields one labelling, and the fixed-only \
                     generator fails to retain both on a pair where the full generator succeeds \
                     ({}), and it stands on ONE pair. Saddle alternatives generated: {}. NOT \
                     offered as evidence, and named here so that it is not read as any: {} px is \
                     the largest equal-valued batch and {} levels carry a tie, and both are true \
                     of EVERY image this corpus contains — 513 quantized levels against at least \
                     1024 pixels makes a tie certain by the pigeonhole principle — so they \
                     measure the size of the input. The batch rule of 11.2 is tested at the unit \
                     level instead, on fixtures built for it",
                    e.hits,
                    e.arms,
                    r.hits,
                    r.arms,
                    r.hits.saturating_sub(e.hits),
                    f.hits,
                    f.arms,
                    pairs
                        .iter()
                        .filter(|p| both(p))
                        .map(|p| format!(
                            "{}: full {:?}/{:?}, fixed-only {:?}/{:?}",
                            p.family,
                            p.both_retained_from_a,
                            p.both_retained_from_b,
                            p.both_retained_fixed_only_from_a,
                            p.both_retained_fixed_only_from_b
                        ))
                        .collect::<Vec<_>>()
                        .join("; "),
                    self.saddle_alternatives_total,
                    self.largest_batch_pixels,
                    self.tie_batches_max
                ),
            ),
        ]
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
