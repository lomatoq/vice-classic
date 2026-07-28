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
use super::report::TopologyReport;
use crate::gates::GatesFile;

/// A population threshold of a gate row, LOADED from the frozen gate file.
///
/// A newtype with no arithmetic, and the absence is the mechanism (RT45-A10).
/// The previous version compared against `pub const MIN_RECALL_ARMS: u32 = 20`
/// and a scan checked that the constant was registered — so the registration
/// was of the SPELLING. `MIN_RECALL_ARMS / 20`, `MIN_RECALL_ARMS - 16`,
/// `MIN_RECALL_ARMS.saturating_sub(19)` all keep the registered name in the
/// line and compare against 1.
///
/// There is no `Div`, `Sub`, `Add` or `Mul` impl here, so every one of those is
/// a type error rather than a value that passes a text check. There is no
/// public constructor either: the only way to obtain one is
/// [`TopologyGateConfig::from_gates`], which reads the committed file.
///
/// What a newtype cannot stop is arithmetic on the OTHER side —
/// `t.met_by(measured + 4)`. That is why the spelling is not the acceptance
/// criterion at all: `each_gate_row_flips_at_exactly_the_registered_threshold`
/// finds the EFFECTIVE boundary by scanning the measurement until the row
/// changes, and compares that to the file. It is blind to how the comparison is
/// written, on either side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Threshold(u64);

impl Threshold {
    /// True when the measurement meets the threshold.
    pub fn met_by(self, measured: u64) -> bool {
        measured >= self.0
    }

    /// For printing beside the row. Deliberately not `impl From<Threshold> for
    /// u64`: a conversion invites arithmetic at the call site, and the whole
    /// point of the type is that there is none.
    pub fn registered_value(self) -> u64 {
        self.0
    }
}

/// The population thresholds of the §28 M4.5 rows, as the gate file has them.
///
/// Condition 7 asked for the numbers to be registered and C179/C180 registered
/// them. RT45-A10 is that registering a CONSTANT and computing from a constant
/// are different things: the row read `MIN_RECALL_ARMS` and the registration
/// checked `MIN_RECALL_ARMS`, but nothing tied the value the row COMPARED
/// AGAINST to the value in the file. It does now — the row compares against
/// this struct, and this struct has exactly one source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TopologyGateConfig {
    pub min_recall_arms: Threshold,
    pub min_recall_shape_families: Threshold,
    pub min_non_trivial_gt_arms: Threshold,
    pub min_topology_pairs: Threshold,
    pub min_classes_per_retaining_pair: Threshold,
}

/// One registered threshold, the quantity the gate row compares it against, and
/// the same quantity recomputed from the RUN the report was built from.
///
/// RT45-A23 is my own F-0042 question applied to my own delta-3 fix: the
/// aggregate guard was FOUR `assert_eq!`, one per quantity the previous finding
/// had presented — clause 1's four — and clause 2's two thresholds were covered
/// by nothing. Padding `classes_from_a` in `ambiguity.rs` made
/// `gate_min_classes_per_retaining_pair` unfalsifiable with the artifact
/// byte-identical, §27.7 silent and 502 tests green.
///
/// Four asserts is a place where the next finding appends a fifth. This is the
/// property instead, and [`TopologyGateConfig::sites`] is what makes it one.
pub struct ThresholdSite {
    pub name: &'static str,
    pub threshold: Threshold,
    /// What the REPORT says the population is.
    pub reported: fn(&TopologyReport) -> u64,
    /// What the RUN says it is. A disagreement is a threshold moved where
    /// neither the artifact nor §27.7 can see it.
    pub from_run: fn(&TopologyReport) -> u64,
}

impl TopologyGateConfig {
    /// Read the five thresholds out of the frozen `[topology]` section.
    ///
    /// `gate_value` refuses a placeholder section, so a gate row cannot be
    /// computed from a number that is not frozen yet.
    pub fn from_gates(g: &GatesFile) -> Result<TopologyGateConfig, String> {
        let t = |key: &str| -> Result<Threshold, String> {
            let v = g
                .gate_value("topology", key)
                .map_err(|e| format!("gate topology.{key}: {e}"))?;
            let n = v
                .as_integer()
                .ok_or_else(|| format!("gate topology.{key} is not an integer: {v}"))?;
            u64::try_from(n)
                .map(Threshold)
                .map_err(|_| format!("gate topology.{key} is negative: {n}"))
        };
        Ok(TopologyGateConfig {
            min_recall_arms: t("gate_min_recall_arms")?,
            min_recall_shape_families: t("gate_min_recall_shape_families")?,
            min_non_trivial_gt_arms: t("gate_min_non_trivial_gt_arms")?,
            min_topology_pairs: t("gate_min_topology_pairs")?,
            min_classes_per_retaining_pair: t("gate_min_classes_per_retaining_pair")?,
        })
    }

    /// Every threshold, with both readings of the quantity it gates.
    ///
    /// The DESTRUCTURING is the mechanism. A field added to
    /// `TopologyGateConfig` without a site below does not compile, because the
    /// pattern is exhaustive — so "every registered threshold is checked
    /// against its own input" is enforced by the compiler rather than by
    /// remembering to add a fifth assertion.
    pub fn sites(&self) -> Vec<ThresholdSite> {
        let TopologyGateConfig {
            min_recall_arms,
            min_recall_shape_families,
            min_non_trivial_gt_arms,
            min_topology_pairs,
            min_classes_per_retaining_pair,
        } = *self;
        vec![
            ThresholdSite {
                name: "gate_min_recall_arms",
                threshold: min_recall_arms,
                reported: |r| r.recall_all.arms,
                from_run: |r| r.recall_population().len() as u64,
            },
            ThresholdSite {
                name: "gate_min_recall_shape_families",
                threshold: min_recall_shape_families,
                reported: |r| r.recall_shape_families,
                from_run: |r| {
                    r.recall_population()
                        .iter()
                        .map(|a| a.shape_family.as_str())
                        .collect::<std::collections::BTreeSet<_>>()
                        .len() as u64
                },
            },
            ThresholdSite {
                name: "gate_min_non_trivial_gt_arms",
                threshold: min_non_trivial_gt_arms,
                reported: |r| r.non_trivial_gt_arms,
                from_run: |r| {
                    let plain = |g: super::GtSignature| g.components == 1 && g.holes == 0;
                    r.recall_population()
                        .iter()
                        .filter(|a| !(plain(a.gt_four) && plain(a.gt_eight)))
                        .count() as u64
                },
            },
            ThresholdSite {
                name: "gate_min_topology_pairs",
                threshold: min_topology_pairs,
                reported: |r| r.ambiguity.iter().filter(|p| p.is_topology_pair).count() as u64,
                from_run: |r| {
                    // A pair is a TOPOLOGY pair when its two signatures differ.
                    // Recomputed from the signatures rather than read off the
                    // flag, so a flag set by hand disagrees with its own input.
                    r.ambiguity.iter().filter(|p| p.sig_a != p.sig_b).count() as u64
                },
            },
            ThresholdSite {
                name: "gate_min_classes_per_retaining_pair",
                threshold: min_classes_per_retaining_pair,
                // SUMMED over every published list, not reduced by `min` the
                // way the row reduces it. The row asks "does the weakest pair
                // carry enough readings"; the INPUT question is "is every
                // published reading real", and a sentinel in a non-minimal pair
                // is invisible to a `min`. I measured that: the first version
                // of this site used `min` and the padding knock-out did not
                // fire.
                reported: |r| {
                    r.ambiguity
                        .iter()
                        .filter(|p| p.is_topology_pair)
                        .map(|p| (p.classes_from_a.len() + p.classes_from_b.len()) as u64)
                        .sum()
                },
                from_run: |r| {
                    // WELL-FORMED, DISTINCT readings only. Padding the list with
                    // sentinels is what RT45-A23 did, and a sentinel is not a
                    // topology any envelope produced.
                    let good = |v: &Vec<(u32, u32)>| {
                        let mut seen: Vec<(u32, u32)> = v
                            .iter()
                            .copied()
                            .filter(|(c, h)| {
                                *c != 0 && *c < PLAUSIBLE_CLASS_BOUND && *h < PLAUSIBLE_CLASS_BOUND
                            })
                            .collect();
                        seen.sort_unstable();
                        seen.dedup();
                        seen.len() as u64
                    };
                    r.ambiguity
                        .iter()
                        .filter(|p| p.is_topology_pair)
                        .map(|p| good(&p.classes_from_a) + good(&p.classes_from_b))
                        .sum()
                },
            },
        ]
    }
}

/// A component or hole count above this is not a reading of a 32-512 px render;
/// it is a sentinel someone pushed into a published list.
const PLAUSIBLE_CLASS_BOUND: u32 = 100_000;

impl TopologyReport {
    /// The three §28 M4.5 clauses, as booleans over this report's own data and
    /// the thresholds the frozen gate file carries.
    pub fn gate_table(&self, cfg: &TopologyGateConfig) -> Vec<(&'static str, bool, String)> {
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
        let recall_row = cfg.min_recall_arms.met_by(r.arms)
            && cfg.min_recall_shape_families.met_by(families as u64)
            && cfg
                .min_non_trivial_gt_arms
                .met_by(self.non_trivial_gt_arms)
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
        let ambiguity_row = cfg.min_topology_pairs.met_by(pairs.len() as u64)
            && pairs.iter().any(|p| both(p))
            && pairs.iter().all(|p| both(p) || excused(p))
            && pairs.iter().filter(|p| both(p)).all(|p| {
                cfg.min_classes_per_retaining_pair
                    .met_by(p.classes_from_a.len() as u64)
                    && cfg
                        .min_classes_per_retaining_pair
                        .met_by(p.classes_from_b.len() as u64)
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
