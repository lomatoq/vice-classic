//! The three §28 M4.5 gate rows.
//!
//! In their own module for the reason §4.1 gives and `oracle/report/gate.rs`
//! already demonstrates: `report.rs` crossed the 800-line rule when the
//! knockout's positive control and the both-conventions subpopulation arrived,
//! and a rule enforced by a test is a rule that moves code rather than a rule
//! that is quoted.
//!
//! Every row is a conjunction with at least one control that CAN fail, and the
//! controls are not decorative — REVIEW_M3_5 M35-N4 is what a row that is green
//! because its population is empty looks like, and this file is written against
//! that. Two findings sharpened it since:
//!
//! - M45-N8 / RT45-A6: a conjunct IMPLIED by its neighbour is not a second
//!   witness, and three of them have been removed for that;
//! - RT45-A12: a control knocked out only in the direction where it FAILS is
//!   half a control, because the other direction — the control going EMPTY —
//!   looks exactly like success from the outside.

use super::ambiguity::AmbiguityRow;
use super::report::{
    TopologyReport, MIN_CLASSES_PER_RETAINING_PAIR, MIN_NON_TRIVIAL_GT_ARMS, MIN_RECALL_ARMS,
    MIN_RECALL_SHAPE_FAMILIES, MIN_TOPOLOGY_PAIRS,
};

impl TopologyReport {
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
        let recall_row = r.arms >= u64::from(MIN_RECALL_ARMS)
            && families >= MIN_RECALL_SHAPE_FAMILIES as usize
            && self.non_trivial_gt_arms >= u64::from(MIN_NON_TRIVIAL_GT_ARMS)
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
            && self.recall_unrelated_field.hits < r.hits
            // And the knockout has to be a KNOCKOUT. RT45-A12: the conjunct
            // above is satisfied trivially by a knockout that measures
            // nothing — `0.3` to `0.0001` on the disk radius empties the
            // unrelated field, takes it to 0 of 100, and this clause stayed
            // MET with its only control switched off. A control that is only
            // knocked out in the direction where it FAILS is half a control.
            //
            // The other direction: a centred disk IS one component and no
            // holes, so on the arms whose truth is exactly that it must
            // SUCCEED. An empty field succeeds nowhere, and now says so here.
            && self.recall_unrelated_field_trivial.hits > 0;
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
        let ambiguity_row = pairs.len() >= MIN_TOPOLOGY_PAIRS as usize
            && pairs.iter().any(|p| both(p))
            && pairs.iter().all(|p| both(p) || excused(p))
            && pairs.iter().filter(|p| both(p)).all(|p| {
                p.classes_from_a.len() >= MIN_CLASSES_PER_RETAINING_PAIR as usize
                    && p.classes_from_b.len() >= MIN_CLASSES_PER_RETAINING_PAIR as usize
            });

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
        // `tie_batches_max > 0` used to be a fifth conjunct and is gone
        // (M45-N8/RT45-A6, second instance). It cannot be false: a tie batch is
        // a level at which two or more pixels share a value, and over 132 arms
        // of 32-128 px renders quantized to 8 bits, pigeonhole guarantees one.
        // A conjunct that cannot be false is not a control, and publishing its
        // value beside the row inflates the count of independent witnesses —
        // which is precisely the finding this clause already absorbed once.
        //
        // RT45-A13 is that it was announced as removed and was not: STATUS_M4_5
        // §9 said "`tie_batches_max > 0` удалён" at C177 while this line stood.
        // The number is still PUBLISHED, in T3d, and T3d says what it is: the
        // size of the input, not evidence.
        let threshold_row = r.arms > 0
            && e.hits == r.hits
            && one_level_is_not_enough
            && self.saddle_alternatives_total > 0;

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
                     part, and a (1, 0) arm is nearly free. SHARPER STILL, and this is the \
                     contrast REVIEW_M4_5 addendum 2 computed before this report published it: on \
                     the {} arms whose GT is non-trivial under BOTH conventions, spread over {} \
                     shape families {:?}, the real envelope is {}/{} and the knockout is {}/{}. \
                     And the knockout's own POSITIVE control, without which the conjunct above is \
                     satisfied by a knockout that measures nothing (RT45-A12): on the trivial \
                     arms, where a centred disk IS the answer, the unrelated field matches {}/{}; \
                     shrink its radius to nothing and this number goes to zero and takes the row \
                     with it. {} audit groups skipped, {} arms refused",
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
                    self.non_trivial_gt_arms_both_conventions,
                    self.non_trivial_both_shape_families.len(),
                    self.non_trivial_both_shape_families,
                    self.recall_non_trivial_both_conventions.hits,
                    self.recall_non_trivial_both_conventions.arms,
                    self.recall_unrelated_field_non_trivial_both.hits,
                    self.recall_unrelated_field_non_trivial_both.arms,
                    self.recall_unrelated_field_trivial.hits,
                    self.recall_unrelated_field_trivial.arms,
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
