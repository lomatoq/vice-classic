//! The §28 M5 report and its four gate rows.
//!
//! Every row is a conjunction, every conjunction has at least one control that
//! CAN fail, and no conjunct is implied by its neighbour. The three rules that
//! shaped this file are the ones M4.5 paid for:
//!
//! - a conjunct implied by another is a paraphrase, not a second witness
//!   (M45-N8, RT45-A6);
//! - a conjunct that cannot be false measures the size of the input
//!   (M45-N5, F-0035);
//! - a control knocked out only where it FAILS is half a control; the other
//!   half is the control going EMPTY, and empty is indistinguishable from
//!   success from the outside (RT45-A12, F-0039).
//!
//! So every row that stands on a population also PUBLISHES that population's
//! size, and the row is false when the population is empty.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use super::{DcelArm, DcelRun, ResolvingPower};
use crate::artifact;

pub const DCEL_REPORT_SCHEMA: &str = "vice-classic/dcel-report/v1";

mod config;
pub use config::{
    DcelGateConfig, MIN_ARMS, MIN_COMPOUND_TRANSACTIONS, MIN_CONVENTION_DEPENDENT_GROUPS,
    MIN_DISTINCT_COMPOUND_DELTAS, MIN_REGISTER_ARMS_WITH_A_LONG_LOOP, MIN_RESOLVING_POWER_PROBES,
    MIN_SLOTS_PERTURBED, MIN_STRUCTURAL_ARMS, MIN_TRANSACTIONS, MIN_TRANSACTION_SHAPES,
    MIN_UNRELATED_CHAIN_POPULATION,
};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DcelReport {
    pub schema: &'static str,
    pub scope: String,
    pub platform: serde_json::Value,
    pub cells: Vec<String>,
    pub scenes: u64,
    pub fixture_set_hash: String,
    pub arms: Vec<DcelArm>,
    pub arms_measured: u64,
    pub corpus_arms: u64,
    pub structural_arms: u64,
    pub arms_refused: u64,
    /// Arms whose labelling has no interface at all, so the arrangement is
    /// valid and EMPTY. Published, and subtracted from the population every
    /// clause stands on: `adv/sliver` is thinner than a pixel and the §5.3
    /// majority rule digitizes it to nothing, and an arm that contains nothing
    /// is evidence for nothing.
    pub arms_with_an_empty_arrangement: u64,
    pub arms_with_a_non_empty_arrangement: u64,
    /// Arms on which the AUDIT REFUSED. Its own number, because an arm the
    /// instrument rejected is not an arm with nothing in it: the error path used
    /// to write zero into `directed_steps`, which is also the definition of
    /// "empty", so F-0058 and meta-rule M-4 shared one counter (RT5-A21).
    pub arms_where_the_audit_refused: u64,
    pub sealed_audit_groups_skipped: u64,

    // --- clause 1 -------------------------------------------------------
    /// Groups (one fixture at one size) and how many distinct classes each
    /// carries in and out of the M5 stage.
    pub groups: u64,
    pub classes_in: u64,
    pub classes_out: u64,
    /// Groups whose two convention arms disagree. This is clause 1's
    /// population: on a group of size one, "the stage did not reduce the set"
    /// is true of any stage at all.
    pub convention_dependent_groups: u64,
    /// Split by SOURCE. The row used to assert "every one of them comes from
    /// the structural register", carrying STATUS_M4_5 limitation 18 onto M5's
    /// population — 444 corpus arms over different cells, not M4.5's 132 —
    /// without recomputing it. The list printed in the same sentence refuted
    /// it: 7 of the 10 are corpus groups (REVIEW_M5_B N1). A prose universal
    /// beside a printed set is a claim COMPUTABLE from that set.
    pub convention_dependent_groups_from_corpus: u64,
    pub convention_dependent_groups_from_register: u64,
    pub convention_dependent_group_names: Vec<String>,

    // --- clause 2 -------------------------------------------------------
    /// Arms where the DCEL's class differs from the class the INDEPENDENT
    /// chain assigns the same labelling.
    pub arms_disagreeing_with_the_independent_chain: u64,
    /// Arms whose Euler identity `V - B + L = 2C` does not hold.
    pub arms_failing_the_euler_identity: u64,
    /// Distinct topological classes the run saw. A run in which every arm is a
    /// disk would satisfy every equality above and measure nothing.
    pub distinct_classes: u64,
    pub classes: Vec<(u32, u32)>,

    // --- clause 3 -------------------------------------------------------
    pub transactions_attempted: u64,
    /// How many distinct edit SHAPES the harness applies per arm. One until
    /// M6, and one shape can only produce the deltas that shape produces:
    /// a filled square yields `(0,0)`, `(-1,0)` or `(+1,0)` and never a
    /// compound edit, which is why `transactions_compound` read zero the first
    /// time it was published (STATUS_M5 limitation 34). The third shape exists
    /// for the same reason one step lower: with two shapes, no declaration in
    /// 960 had a negative hole component although 72 arms carried a hole, so
    /// `hole_fill` was unreachable by SHAPE rather than by construction.
    pub transaction_shapes: u64,
    pub transactions_committed: u64,
    /// Committed transactions on arms whose arrangement is NOT empty. This is
    /// what clause 3 stands on (RT5-A4).
    pub transactions_committed_on_non_empty_arms: u64,
    pub transactions_rolled_back: u64,
    /// Summed over committed transactions: chains lying wholly outside the ROI
    /// plus halo, and how many of them were not found verbatim afterwards.
    pub unrelated_chains_total: u64,
    pub unrelated_chains_that_moved: u64,
    /// Committed transactions that had NO unrelated chain to compare. Published
    /// because on those the clause is a statement about the empty set.
    pub transactions_with_no_unrelated_population: u64,
    /// Arms that had NO transaction attempted, for any reason. Zero exclusions
    /// for being COMPOUND since M6 — the filter that produced 310 of them is
    /// gone (limitations 37, 44) — so what remains is the size guard.
    pub transaction_arms_excluded: u64,
    /// Transactions whose declared edit is worth two or more unit steps: the
    /// subclass §28 M5 names and M5 never attempted. A run in which this is
    /// zero has not tested compound transactions, which is why it is a
    /// published number and a gate floor rather than a remark.
    pub transactions_compound: u64,
    pub transactions_compound_committed: u64,
    pub transactions_unit_step: u64,
    /// Transactions whose declared delta is `(0, 0)`. The signature did not
    /// move; `apply` still has to agree that it did not.
    pub transactions_identity: u64,
    pub max_declared_steps: u64,
    /// Every declared edit name that actually occurred, with its count. M5
    /// exercised two of four possible names; this is derived from the run
    /// rather than from a list, so a name nobody anticipated appears here
    /// without anyone adding a row.
    pub declared_kinds_exercised: BTreeMap<String, u64>,
    /// Refusal variants the run actually produced, counted. MEASURED.
    pub refusals_observed: BTreeMap<&'static str, u64>,
    /// The complement, against every refusal the type can express. A refusal
    /// that never fires is not evidence of correctness, and this is the list
    /// that says which ones those are.
    pub refusals_never_observed: Vec<&'static str>,

    // --- clause 4 -------------------------------------------------------
    /// The population the §12 ORIENTED check stands on: loops long enough to
    /// have an ORDER. Published because a row standing on a population
    /// publishes its size (RT5-A17, M5A-D3-N1).
    pub arms_with_a_loop_of_three_or_more: u64,
    pub arms_with_a_loop_of_three_or_more_from_corpus: u64,
    pub arms_with_a_loop_of_three_or_more_from_register: u64,
    pub longest_loop_seen: u32,
    pub loops_of_three_or_more_total: u64,
    pub arms_failing_the_audit: u64,
    pub arms_that_are_not_their_own_assembly: u64,
    pub audit_resolving_power: ResolvingPower,
}

fn group_key(a: &DcelArm) -> (String, String) {
    (a.scene_id.clone(), a.cell_id.clone())
}

pub fn build(run: &DcelRun) -> DcelReport {
    let mut in_classes: BTreeMap<(String, String), BTreeSet<(u32, u32)>> = BTreeMap::new();
    let mut out_classes: BTreeMap<(String, String), BTreeSet<(u32, u32)>> = BTreeMap::new();
    for a in &run.arms {
        in_classes
            .entry(group_key(a))
            .or_default()
            .insert((a.dcel_class.components, a.dcel_class.holes));
        out_classes
            .entry(group_key(a))
            .or_default()
            .insert((a.class_out.components, a.class_out.holes));
    }
    // COUNTED from the set, not asserted beside it (REVIEW_M5_B N1).
    let convention_dependent_keys: Vec<(String, String)> = in_classes
        .iter()
        .filter(|(_, v)| v.len() > 1)
        .map(|(k, _)| k.clone())
        .collect();
    let source_of: BTreeMap<(String, String), &'static str> =
        run.arms.iter().map(|a| (group_key(a), a.source)).collect();
    let from_register = convention_dependent_keys
        .iter()
        .filter(|k| source_of.get(*k) == Some(&"structural"))
        .count() as u64;
    let convention_dependent: Vec<String> = convention_dependent_keys
        .iter()
        .map(|k| format!("{}@{}", k.0, k.1))
        .collect();

    let classes: BTreeSet<(u32, u32)> = run
        .arms
        .iter()
        .map(|a| (a.dcel_class.components, a.dcel_class.holes))
        .collect();

    // `audit.is_some()` as well as the loop count: a REFUSED arm still carries a
    // loop profile, because `loop_length_profile` runs whatever the audit says.
    // Harmless today — no refusals on the clean run — but it is the RT5-A21
    // class, an instrument's silence counted as a measurement, and the fix is
    // one predicate (REVIEW_M5_B, delta-6 minor).
    let long: Vec<&DcelArm> = run
        .arms
        .iter()
        .filter(|a| a.audit.is_some() && a.loops_of_three_or_more > 0)
        .collect();
    // BOTH edit shapes. Clause 3 is a claim about transactions, and the
    // annulus transaction is a transaction, so every count below spans both.
    // Counting only the filled square would publish a compound population
    // beside a committed count that excluded it, which is the shape of
    // arithmetic this milestone exists to stop.
    let all: Vec<&super::ArmTransaction> = run
        .arms
        .iter()
        .flat_map(|a| {
            a.transaction
                .iter()
                .chain(a.transaction_ring.iter())
                .chain(a.transaction_hole_fill.iter())
        })
        .collect();
    let tx = |f: fn(&super::ArmTransaction) -> bool| all.iter().filter(|t| f(t)).count() as u64;

    DcelReport {
        schema: DCEL_REPORT_SCHEMA,
        scope: run.scope.to_string(),
        platform: artifact::platform_here(),
        cells: run.cells.clone(),
        scenes: run.scenes,
        fixture_set_hash: run.fixture_set_hash.clone(),
        arms: run.arms.clone(),
        arms_measured: run.arms.len() as u64,
        corpus_arms: run.arms.iter().filter(|a| a.source == "corpus").count() as u64,
        structural_arms: run.arms.iter().filter(|a| a.source == "structural").count() as u64,
        arms_refused: run.refused.len() as u64,
        arms_with_an_empty_arrangement: run
            .arms
            .iter()
            .filter(|a| a.audit.is_some_and(|x| x.directed_steps == 0))
            .count() as u64,
        arms_with_a_non_empty_arrangement: run
            .arms
            .iter()
            .filter(|a| a.audit.is_some_and(|x| x.directed_steps > 0))
            .count() as u64,
        arms_where_the_audit_refused: run.arms.iter().filter(|a| a.audit.is_none()).count() as u64,
        sealed_audit_groups_skipped: run.sealed_audit_groups_skipped,

        groups: in_classes.len() as u64,
        classes_in: in_classes.values().map(|v| v.len() as u64).sum(),
        classes_out: out_classes.values().map(|v| v.len() as u64).sum(),
        convention_dependent_groups: convention_dependent.len() as u64,
        convention_dependent_groups_from_register: from_register,
        convention_dependent_groups_from_corpus: convention_dependent.len() as u64 - from_register,
        convention_dependent_group_names: convention_dependent,

        arms_disagreeing_with_the_independent_chain: run
            .arms
            .iter()
            .filter(|a| !a.agrees_with_the_independent_chain)
            .count() as u64,
        arms_failing_the_euler_identity: run
            .arms
            .iter()
            .filter(|a| a.audit.is_some_and(|x| x.euler_lhs != x.euler_rhs))
            .count() as u64,
        distinct_classes: classes.len() as u64,
        classes: classes.into_iter().collect(),

        // Over BOTH shapes, like every other count in this row. It counted only
        // the filled square for one commit, and the run printed "480 attempted,
        // 678 committed" — arithmetic that is impossible and therefore visible.
        // The rule it broke is the one this project keeps paying for: a
        // denominator and its numerator must come from the same population.
        transactions_attempted: all.len() as u64,
        transaction_shapes: 3,
        transactions_committed: tx(|t| t.committed),
        transactions_committed_on_non_empty_arms: run
            .arms
            .iter()
            .filter(|a| a.audit.is_some_and(|x| x.directed_steps > 0))
            .flat_map(|a| {
                a.transaction
                    .iter()
                    .chain(a.transaction_ring.iter())
                    .chain(a.transaction_hole_fill.iter())
            })
            .filter(|t| t.committed)
            .count() as u64,
        transactions_rolled_back: tx(|t| !t.committed),
        unrelated_chains_total: all
            .iter()
            .filter(|t| t.committed)
            .map(|t| t.unrelated_chains as u64)
            .sum(),
        unrelated_chains_that_moved: all
            .iter()
            .map(|t| t.unrelated_chains_that_moved as u64)
            .sum(),
        transactions_with_no_unrelated_population: tx(|t| t.committed && t.unrelated_chains == 0),
        transaction_arms_excluded: run
            .arms
            .iter()
            .filter(|a| a.excluded_from_transactions)
            .count() as u64,
        transactions_compound: tx(|t| t.declared_steps >= 2),
        transactions_compound_committed: tx(|t| t.declared_steps >= 2 && t.committed),
        transactions_unit_step: tx(|t| t.declared_steps == 1),
        transactions_identity: tx(|t| t.declared_steps == 0),
        max_declared_steps: all.iter().map(|t| t.declared_steps).max().unwrap_or(0),
        declared_kinds_exercised: {
            let mut m: BTreeMap<String, u64> = BTreeMap::new();
            for t in &all {
                *m.entry(t.declared.clone()).or_default() += 1;
            }
            m
        },
        // MEASURED since M6, not argued.
        //
        // This was a pair of hand-written lists with a prose argument for why
        // four of the six refusals could not fire. Two of those four became
        // reachable the moment the compound filter came out — a no-op edit now
        // reaches `apply` instead of being dropped, and `NotTheDeclaredEdit`
        // became a real cross-check once the declaration stopped sharing its
        // provenance with the check. An argument that has to be re-derived
        // whenever the harness moves is not evidence, so what is published now
        // is what the run OBSERVED, and the complement against the refusals the
        // type can express.
        refusals_observed: {
            let mut m: BTreeMap<&'static str, u64> = BTreeMap::new();
            for t in &all {
                if let Some(k) = t.refusal_kind {
                    *m.entry(k).or_default() += 1;
                }
            }
            m
        },
        refusals_never_observed: {
            let seen: BTreeSet<&'static str> = all.iter().filter_map(|t| t.refusal_kind).collect();
            vice_topology::dcel::TransactionRefusal::ALL_NAMES
                .iter()
                .copied()
                .filter(|n| !seen.contains(n))
                .collect()
        },

        arms_with_a_loop_of_three_or_more: long.len() as u64,
        arms_with_a_loop_of_three_or_more_from_corpus: long
            .iter()
            .filter(|a| a.source == "corpus")
            .count() as u64,
        arms_with_a_loop_of_three_or_more_from_register: long
            .iter()
            .filter(|a| a.source == "structural")
            .count() as u64,
        longest_loop_seen: run.arms.iter().map(|a| a.longest_loop).max().unwrap_or(0),
        loops_of_three_or_more_total: run
            .arms
            .iter()
            .map(|a| u64::from(a.loops_of_three_or_more))
            .sum(),
        arms_failing_the_audit: run.arms.iter().filter(|a| !a.audit_ok).count() as u64,
        arms_that_are_not_their_own_assembly: run
            .arms
            .iter()
            .filter(|a| !a.is_its_own_assembly)
            .count() as u64,
        audit_resolving_power: run.audit_resolving_power.clone(),
    }
}

impl DcelReport {
    pub fn canonical_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("dcel report serializes")
    }

    /// The four §28 M5 clauses, as booleans over this report's own data.
    pub fn gate_table(&self, cfg: &DcelGateConfig) -> Vec<(&'static str, bool, String)> {
        // Clause 1: NO FINAL-TOPOLOGY CLAIM FROM PROXY.
        //
        // The M5 stage carries every topology through: the number of distinct
        // classes per group is the same on both sides. That equality is worth
        // nothing on groups of size one, so the row ALSO requires a population
        // of groups whose two convention arms disagree.
        //
        // This comment used to say that population "exists only because the
        // structural register is here" and that "the corpus alone has none",
        // citing STATUS_M4_5 limitation 18. That is FALSE on M5's population and
        // the evidence string three lines below said so: 7 of the 10 are corpus
        // groups. Limitation 18 was measured on M4.5's 132 arms over different
        // cells, and carrying a limitation between milestones obliges recomputing
        // its number (REVIEW_M5_B N13c). The register's contribution is that it
        // guarantees the population BY CONSTRUCTION at every size and under both
        // arms; the corpus supplies it IN FACT here and need not elsewhere.
        let proxy_row = cfg.min_arms.met_by(self.arms_with_a_non_empty_arrangement)
            && cfg.min_structural_arms.met_by(self.structural_arms)
            && cfg
                .min_convention_dependent_groups
                .met_by(self.convention_dependent_groups)
            && self.classes_out == self.classes_in;

        // Clause 2: CANDIDATE RECALL MAINTAINED AFTER BUDGET PRUNING.
        //
        // What M5 can lose that M4.5 could not: the arrangement could carry a
        // different class from the labelling it was built out of. So the row is
        // agreement with an INDEPENDENT chain — flood fill plus bit-quad Euler,
        // which is neither how production counts nor how this module does — on
        // every arm, plus the intrinsic Euler identity, which shares nothing
        // with either.
        //
        // The two are not the same conjunct: the identity is about the loop
        // extraction and holds for arrangements of any labelling; the agreement
        // is about the face flood fill and would fail on a wrong convention.
        //
        // And the population must carry more than one answer: a run of disks
        // satisfies both equalities and measures nothing.
        let recall_row = self.arms_disagreeing_with_the_independent_chain == 0
            && self.arms_failing_the_euler_identity == 0
            && self.distinct_classes >= 3
            && cfg.min_arms.met_by(self.arms_with_a_non_empty_arrangement);

        // Clause 3: NO UNRELATED GRAPH MUTATION.
        //
        // Committed transactions leave every chain outside the ROI plus halo
        // byte-identical. The population is published beside the verdict,
        // because a transaction whose region swallows the canvas has no
        // unrelated chain and its zero means nothing.
        // RT5-A4: this row used to count `transactions_committed`, which
        // includes the 8 arms whose arrangement is EMPTY — while STATUS_M5 T7
        // said "excluded from every clause" and F-0058's rule said every clause
        // stands on the non-empty population. Three of the four places were
        // done; this was the fourth, and it was a different QUANTITY, so
        // appending `arms_with_a_non_empty_arrangement` in three spots missed
        // it. That is F-0048 Q2 answered "append a line" again, inside the fix
        // for the previous instance.
        // M6 adds the COMPOUND conjuncts. §28 M5's bullet names "local COMPOUND
        // topology transactions", and until M6 this row was green while that
        // subclass was empty — the harness excluded it and mislabelled what it
        // excluded (F-0081). A clause naming a subclass must stand on a
        // population of it, and the three floors are the CAUSE (shapes) and
        // two EFFECTS (count, distinct deltas), because a floor on the count
        // alone is met by one delta repeated.
        // M6 adds the COMPOUND conjuncts. §28 M5's bullet names "local COMPOUND
        // topology transactions", and until M6 this row was green while that
        // subclass was EMPTY: the harness excluded it and mislabelled what it
        // excluded (F-0081). A clause naming a subclass must stand on a
        // population of it. Three conjuncts, not one, because a floor on the
        // count alone is met by one delta repeated and because the count is a
        // consequence of the SHAPE set, which is the cheaper thing to lose.
        let distinct_compound = self
            .declared_kinds_exercised
            .keys()
            .filter(|k| k.starts_with("compound("))
            .count() as u64;
        let mutation_row = cfg
            .min_transactions
            .met_by(self.transactions_committed_on_non_empty_arms)
            && cfg
                .min_unrelated_chain_population
                .met_by(self.unrelated_chains_total)
            && self.unrelated_chains_that_moved == 0
            && cfg
                .min_compound_transactions
                .met_by(self.transactions_compound_committed)
            && cfg.min_distinct_compound_deltas.met_by(distinct_compound)
            && cfg.min_transaction_shapes.met_by(self.transaction_shapes);

        // Clause 4: NO DANGLING/INVALID FACES.
        //
        // The audit is green on every arrangement, and the audit can be red:
        // the mutation walk perturbs every derived slot of a sampled
        // arrangement and counts what the AUDIT catches.
        let p: &ResolvingPower = &self.audit_resolving_power;
        // RT5-A2 removed two conjuncts and replaced the third.
        //
        // `caught_by_neither == 0` and `caught_by_assembly_equality == slots`
        // are ARITHMETIC on a value built by perturbing an assembled one: the
        // perturbed value is by construction not the assembly of its own
        // labelling. The clause therefore required of the audit only
        // `caught_by_audit > 0` — one slot — and the red team reduced `audit()`
        // to range guards plus a single check, deleting the entire seventh §12
        // invariant, with the gate green and 530 tests passing.
        //
        // What stands here now is the complement, and it is a property the
        // audit can fail: EVERY real perturbation of EVERY derived slot must be
        // rejected by the audit ALONE. Remove a check and `uncaught_by_audit`
        // rises off zero.
        // `arms_that_are_not_their_own_assembly == 0` is NOT a conjunct here
        // any more. On this population it cannot be false: every arm IS
        // `assemble(L, c)`, and the check recomputes `assemble(L, c)` and
        // compares — so it measures repeat-determinism of `assemble`, which is
        // worth having and is not what the row said it was (REVIEW_M5_A N7). It
        // is published below under that name. The check earns its keep inside
        // the mutation walk, where `Parts` is genuinely corrupted.
        let faces_row = self.arms_failing_the_audit == 0
            && cfg.min_arms.met_by(self.arms_with_a_non_empty_arrangement)
            && cfg
                .min_resolving_power_probes
                .met_by(u64::from(p.arrangements_probed))
            && cfg.min_slots_perturbed.met_by(p.slots_perturbed)
            && p.uncaught_by_audit == 0
            && p.no_ops == 0
            // REVIEW_M5_B N15. The branch probe was published and gated by
            // NOTHING, so a commit deleting it and re-recording the artifact
            // (which is not under §27.7) would silently return this row to
            // stride-dependence — and stride-dependence is what made its green
            // a property of arm ORDER in the first place. It is a conjunct now.
            //
            // EVERY branch the judge reported must have been probed, and there
            // must be at least two, so a run that only ever took one branch
            // cannot satisfy it by having nothing to miss.
            && p.branches_seen.len() >= 2
            && p.branches_seen.iter().all(|b| b.arms_probed > 0)
            // RT5-A17 / M5A-D3-N1. §12's ORIENTED half is only exercised by a
            // loop long enough to HAVE an order, and delta-3 gave that clause a
            // fixture and no floor - the fourth population of this gate to need
            // one, and the only one that did not get it. A fixture makes a check
            // exercised today; a floor makes it exercised tomorrow.
            && cfg
                .min_register_arms_with_a_long_loop
                .met_by(self.arms_with_a_loop_of_three_or_more_from_register);

        vec![
            (
                "no final-topology claim from proxy",
                proxy_row,
                format!(
                    "The M5 stage carries topologies; it does not choose among them, and \
                     `dcel::transaction::apply` has no parameter through which a cost, a bound or \
                     a score could arrive. MEASURED over {} groups (one fixture at one size): {} \
                     distinct classes entering the stage, {} leaving. That equality is empty on a \
                     group of size one, so the row stands on the {} group(s) whose two \
                     complementary-connectivity arms DISAGREE, of which {} come from the CORPUS \
                     and {} from the structural register: {:?}. Those two counts are computed from \
                     the printed set rather than asserted beside it, which is delta-1's correction \
                     of this row: it used to say `every one of them comes from the structural \
                     register`, carrying STATUS_M4_5 limitation 18 (`zero of 132 arms`) onto M5's \
                     444-arm population over different cells without recomputing it, and the list \
                     printed in the same sentence refuted it (REVIEW_M5_B N1). What the register \
                     supplies is the population BY CONSTRUCTION, at every size and under both \
                     arms; the corpus supplies it IN FACT, on this cell set, and would not \
                     necessarily on another. Structural arms are {} of {}. The knockout that makes \
                     this row false is in the tree and runs under `--ignored`: \
                     `ProxyKnockout::Select` gives each group its first class and the row goes NOT \
                     MET",
                    self.groups,
                    self.classes_in,
                    self.classes_out,
                    self.convention_dependent_groups,
                    self.convention_dependent_groups_from_corpus,
                    self.convention_dependent_groups_from_register,
                    self.convention_dependent_group_names,
                    self.structural_arms,
                    self.arms_measured
                ),
            ),
            (
                "candidate recall maintained after budget pruning",
                recall_row,
                format!(
                    "What M5 can lose is the CLASS: an arrangement built from a labelling could \
                     carry a topology the labelling does not have. Measured on all {} arms ({} \
                     corpus, {} structural) against an INDEPENDENT chain - flood fill for \
                     components, bit-quad Euler for holes - which is neither how production counts \
                     nor how this module does: {} arm(s) disagree. Beside it, and sharing nothing \
                     with it, the intrinsic identity V - B + L = 2C: {} arm(s) fail it. The two \
                     are not one conjunct - the identity is about the loop extraction and the \
                     agreement is about the face flood fill, and a wrong convention moves the \
                     second while leaving the first true. The population carries {} distinct \
                     topological classes {:?}; a run of disks would satisfy both equalities and \
                     measure nothing. NOTE ON SCOPE, so the row is not read as more: this \
                     population is the ORACLE observation - exact ink coverage digitized by the \
                     majority rule - not the estimated-evidence path. Budget pruning of the \
                     envelope is measured where the envelope is built, by the M4.5 clause, and \
                     that number is `arms_where_budget_pruning_lost_the_last_gt_candidate` in \
                     TOPOLOGY_M4_5.json. {} audit groups skipped, {} arms refused",
                    self.arms_measured,
                    self.corpus_arms,
                    self.structural_arms,
                    self.arms_disagreeing_with_the_independent_chain,
                    self.arms_failing_the_euler_identity,
                    self.distinct_classes,
                    self.classes,
                    self.sealed_audit_groups_skipped,
                    self.arms_refused
                ),
            ),
            (
                "no unrelated graph mutation",
                mutation_row,
                format!(
                    "{} transaction(s) attempted, {} committed, {} rolled back. On the committed \
                     ones, the boundary chains lying WHOLLY outside the ROI plus its {} px halo \
                     are compared by their lattice paths rather than by id, because an edit that \
                     merges two faces legitimately renumbers faces and comparing ids would report \
                     the edit itself as collateral damage. Population: {} such chains, of which {} \
                     were not found verbatim afterwards. The population is published because it is \
                     the thing that makes zero mean something: {} committed transaction(s) had NO \
                     unrelated chain at all - their region plus halo covered everything - and on \
                     those the clause is a statement about the empty set. The knockout is in the \
                     tree: `RoiKnockout::Reach` adds one pixel outside the declared ROI and every \
                     transaction is refused",
                    self.transactions_attempted,
                    self.transactions_committed,
                    self.transactions_rolled_back,
                    vice_topology::TX_CONFIG_V1.halo_px,
                    self.unrelated_chains_total,
                    self.unrelated_chains_that_moved,
                    self.transactions_with_no_unrelated_population
                ),
            ),
            (
                "no dangling/invalid faces",
                faces_row,
                format!(
                    "§12 lists seven invariants and one of them - face cycles closed AND oriented - \
                     is a conjunction held two different ways, so the table splits it into eight \
                     rows. SIX are held by the representation: the twin is a bit flip, the owners \
                     are a pair, a loop is a cyclic sequence, a crack is not constructible, \
                     segments are integer-lattice unit steps and the exterior is an ordinary face - \
                     so there is nothing there to measure and nothing there is claimed. TWO are \
                     computations: the Euler/cubical signature, and - since delta-3 - whether the \
                     face cycles are ORIENTED, which the cyclic-sequence argument never \
                     established. This row is about both: {} of {} non-empty arms failed the audit, and {} were not the assembly of \
                     their own labelling. A further {} arm(s) carry a valid but EMPTY arrangement \
                     - `adv/sliver` is thinner than a pixel and the §5.3 majority rule digitizes \
                     it to nothing - and they are audited like the rest while no clause is allowed \
                     to stand on them, because an arm that contains nothing is evidence for \
                     nothing. SEPARATELY from those, {} arm(s) are ones the AUDIT REFUSED, which \
                     is a different thing and until delta-6 was not: the error path wrote zero \
                     into the step count, and zero is also how an empty labelling is recognised, \
                     so an arm the instrument rejected was counted as an arm with nothing in it \
                     and this sentence described it as a sliver thinner than a pixel (RT5-A21). \
                     An error path may not write a value that elsewhere means the subject was \
                     absent. What makes those zeros evidence rather than silence is the \
                     MUTATION WALK, which is the world in which the audit is red: on {} sampled \
                     arrangements out of {} arms seen it perturbed {} derived slots one at a time \
                     - the walk is an exhaustive destructuring of the structure, so a field added \
                     without a site does not compile - and the AUDIT ALONE rejected {} of them, \
                     ACCEPTED {}, with {} that changed nothing. Delta-1 changed what this row \
                     asks. It required `caught_by_neither == 0`, which REDTEAM_M5 RT5-A2 proved is \
                     ARITHMETIC - a perturbed value is by construction not the assembly of its own \
                     labelling - so the audit was asked for ONE caught slot, and an audit reduced \
                     to a single check kept this row green with 530 tests passing. It now requires \
                     the complement to be zero: every real perturbation of every derived slot \
                     rejected by the audit alone. The number that made this visible was already \
                     printed and was misread - 5648 of 155 160 was offered as an honest weakness \
                     and what it said is that 96.4 % of the structure was checked by nothing, \
                     including `face_of_padded_px`, the largest field, which no predicate read at \
                     all until the third construction of delta-1 (RT5-A1). WHAT THIS NUMBER DOES \
                     NOT COVER, said plainly because REDTEAM_M5 RT5-A14 measured it: switching the \
                     LABELLING ANCHOR off entirely moves no slot count, no `by_family` entry and \
                     no artifact byte. That is a property of the INSTRUMENT rather than a defect \
                     in the anchor - this walk is made of perturbations of a CORRECT structure, \
                     and the anchor's whole domain is defects INSIDE `assemble`, which are not \
                     perturbations of anything (F-0066). The anchor is guarded by KNOCKOUTS rather \
                     than measured here: RT5-A1 and RT5-A9 are both in the tree as gate-level \
                     controls required to redden this row, and RT5-A9 is the corruption that \
                     passed the whole of delta-1 with a byte-identical artifact. What the walk DOES \
                     measure of the two map checks is that they are not redundant - on a 13x13 \
                     annulus the rebuild alone catches 153 slots the anchor does not, the anchor \
                     alone catches 3 the rebuild does not, and 160 fall to both - so citing both is \
                     not citing one twice. The §12 ORIENTED clause is a third check, added in \
                     delta-3: loops re-derived from the labelling, which is what a reordering of \
                     one loop moves and neither of the other two can see (RT5-A13). WHAT THIS \
                     ROW DOES NOT CERTIFY, in the same tone as the anchor sentence above: it \
                     certifies the fields the structure has TODAY, not a property of the \
                     structure. A field ADDED by a later commit whose type serialises to \
                     nothing - a newtype writing `serialize_none()` - is invisible to the \
                     perturbation walk, to the leaf count that guards the walk's completeness \
                     and to this artifact, and it can carry a systematically wrong value \
                     behind a public accessor with all four clauses MET (REDTEAM_M5 RT5-A16, \
                     nine lines). The leaf count is keyed on the serialization, so a type that \
                     serialises to nothing moves the ruler rather than the measurement. \
                     Closing the CLASS needs a proc-macro deriving the perturbation sites and \
                     the leaf count from ONE definition, which is a new crate and is owned by \
                     M6; this row states the boundary instead of implying it is not there. \
                     ITS \
                     POPULATION, which delta-3 published nowhere: {} arm(s) carry a face \
                     loop of three or more half-edges - {} from the corpus and {} from the \
                     structural register - the longest is {} half-edges, and {} such loops \
                     exist in total. A loop of one or two has no reordering that changes \
                     it, so those are the arms on which the ORIENTED check is a test at \
                     all. The floor is on the REGISTER's share because that is where the \
                     population is guaranteed BY CONSTRUCTION at every size under both \
                     convention arms; the corpus's share is incidental and is published \
                     rather than relied on",
                    self.arms_failing_the_audit,
                    self.arms_with_a_non_empty_arrangement,
                    self.arms_that_are_not_their_own_assembly,
                    self.arms_with_an_empty_arrangement,
                    self.arms_where_the_audit_refused,
                    p.arrangements_probed,
                    p.arms_seen,
                    p.slots_perturbed,
                    p.caught_by_audit,
                    p.uncaught_by_audit,
                    p.no_ops,
                    self.arms_with_a_loop_of_three_or_more,
                    self.arms_with_a_loop_of_three_or_more_from_corpus,
                    self.arms_with_a_loop_of_three_or_more_from_register,
                    self.longest_loop_seen,
                    self.loops_of_three_or_more_total
                ),
            ),
        ]
    }
}

/// The cross-platform projection: everything that is an integer or a name.
///
/// Wider than the corridor and oracle projections and for the same reason the
/// M4.5 one is: the quantities here are counts and classes, which a different
/// libm cannot move without moving the topology itself. It is NOT Tier B and is
/// not offered as it — A7.1 remains open with owner M12.
pub fn structural_projection(v: &serde_json::Value) -> serde_json::Value {
    let mut out = serde_json::Map::new();
    for k in [
        "schema",
        "scope",
        "cells",
        "scenes",
        "arms_measured",
        "corpus_arms",
        "structural_arms",
        "arms_refused",
        "arms_with_an_empty_arrangement",
        "arms_with_a_non_empty_arrangement",
        "arms_where_the_audit_refused",
        "sealed_audit_groups_skipped",
        "groups",
        "classes_in",
        "classes_out",
        "convention_dependent_groups",
        "convention_dependent_group_names",
        "arms_disagreeing_with_the_independent_chain",
        "arms_failing_the_euler_identity",
        "distinct_classes",
        "classes",
        "transactions_attempted",
        "transactions_committed",
        "transactions_committed_on_non_empty_arms",
        "transactions_rolled_back",
        "unrelated_chains_total",
        "unrelated_chains_that_moved",
        "transactions_with_no_unrelated_population",
        "transaction_arms_excluded_as_compound",
        "reachable_refusals_on_this_population",
        "unreachable_refusals_on_this_population",
        "arms_with_a_loop_of_three_or_more",
        "arms_with_a_loop_of_three_or_more_from_corpus",
        "arms_with_a_loop_of_three_or_more_from_register",
        "longest_loop_seen",
        "loops_of_three_or_more_total",
        "arms_failing_the_audit",
        "arms_that_are_not_their_own_assembly",
        "audit_resolving_power",
    ] {
        if let Some(x) = v.get(k) {
            out.insert(k.to_string(), x.clone());
        }
    }
    serde_json::Value::Object(out)
}
