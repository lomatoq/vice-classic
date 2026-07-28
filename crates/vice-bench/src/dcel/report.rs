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
use crate::gates::GatesFile;
use crate::topology::gate::Threshold;

pub const DCEL_REPORT_SCHEMA: &str = "vice-classic/dcel-report/v1";

/// The population thresholds of the §28 M5 rows, LOADED from the frozen gate
/// file.
///
/// The same shape `TopologyGateConfig` uses and for the same reason (RT45-A10):
/// the row compares against this struct, this struct has exactly one source,
/// and `Threshold` has no arithmetic so `t / 20` is a type error rather than a
/// value that passes a text check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DcelGateConfig {
    pub min_arms: Threshold,
    pub min_structural_arms: Threshold,
    pub min_convention_dependent_groups: Threshold,
    pub min_transactions: Threshold,
    pub min_unrelated_chain_population: Threshold,
    pub min_resolving_power_probes: Threshold,
}

impl DcelGateConfig {
    pub fn from_gates(g: &GatesFile) -> Result<DcelGateConfig, String> {
        let t = |key: &str| Threshold::from_gates(g, "dcel", key);
        Ok(DcelGateConfig {
            min_arms: t("gate_min_arms")?,
            min_structural_arms: t("gate_min_structural_arms")?,
            min_convention_dependent_groups: t("gate_min_convention_dependent_groups")?,
            min_transactions: t("gate_min_transactions")?,
            min_unrelated_chain_population: t("gate_min_unrelated_chain_population")?,
            min_resolving_power_probes: t("gate_min_resolving_power_probes")?,
        })
    }
}

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
    pub transactions_committed: u64,
    pub transactions_rolled_back: u64,
    /// Summed over committed transactions: chains lying wholly outside the ROI
    /// plus halo, and how many of them were not found verbatim afterwards.
    pub unrelated_chains_total: u64,
    pub unrelated_chains_that_moved: u64,
    /// Committed transactions that had NO unrelated chain to compare. Published
    /// because on those the clause is a statement about the empty set.
    pub transactions_with_no_unrelated_population: u64,

    // --- clause 4 -------------------------------------------------------
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
    let convention_dependent: Vec<String> = in_classes
        .iter()
        .filter(|(_, v)| v.len() > 1)
        .map(|(k, _)| format!("{}@{}", k.0, k.1))
        .collect();

    let classes: BTreeSet<(u32, u32)> = run
        .arms
        .iter()
        .map(|a| (a.dcel_class.components, a.dcel_class.holes))
        .collect();

    let tx = |f: fn(&super::ArmTransaction) -> bool| {
        run.arms
            .iter()
            .filter_map(|a| a.transaction.as_ref())
            .filter(|t| f(t))
            .count() as u64
    };

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
        sealed_audit_groups_skipped: run.sealed_audit_groups_skipped,

        groups: in_classes.len() as u64,
        classes_in: in_classes.values().map(|v| v.len() as u64).sum(),
        classes_out: out_classes.values().map(|v| v.len() as u64).sum(),
        convention_dependent_groups: convention_dependent.len() as u64,
        convention_dependent_group_names: convention_dependent,

        arms_disagreeing_with_the_independent_chain: run
            .arms
            .iter()
            .filter(|a| !a.agrees_with_the_independent_chain)
            .count() as u64,
        arms_failing_the_euler_identity: run
            .arms
            .iter()
            .filter(|a| a.euler_lhs != a.euler_rhs)
            .count() as u64,
        distinct_classes: classes.len() as u64,
        classes: classes.into_iter().collect(),

        transactions_attempted: run.arms.iter().filter(|a| a.transaction.is_some()).count() as u64,
        transactions_committed: tx(|t| t.committed),
        transactions_rolled_back: tx(|t| !t.committed),
        unrelated_chains_total: run
            .arms
            .iter()
            .filter_map(|a| a.transaction.as_ref())
            .filter(|t| t.committed)
            .map(|t| t.unrelated_chains as u64)
            .sum(),
        unrelated_chains_that_moved: run
            .arms
            .iter()
            .filter_map(|a| a.transaction.as_ref())
            .map(|t| t.unrelated_chains_that_moved as u64)
            .sum(),
        transactions_with_no_unrelated_population: tx(|t| t.committed && t.unrelated_chains == 0),

        arms_failing_the_audit: run.arms.iter().filter(|a| !a.audit_ok).count() as u64,
        arms_that_are_not_their_own_assembly: run
            .arms
            .iter()
            .filter(|a| !a.is_its_own_assembly)
            .count() as u64,
        audit_resolving_power: run.audit_resolving_power,
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
        // of groups whose two convention arms disagree — and that population
        // exists only because the structural register is here. The corpus alone
        // has none (STATUS_M4_5 limitation 18: zero of 132 arms).
        let proxy_row = cfg.min_arms.met_by(self.arms_measured)
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
            && cfg.min_arms.met_by(self.arms_measured);

        // Clause 3: NO UNRELATED GRAPH MUTATION.
        //
        // Committed transactions leave every chain outside the ROI plus halo
        // byte-identical. The population is published beside the verdict,
        // because a transaction whose region swallows the canvas has no
        // unrelated chain and its zero means nothing.
        let mutation_row = cfg.min_transactions.met_by(self.transactions_committed)
            && cfg
                .min_unrelated_chain_population
                .met_by(self.unrelated_chains_total)
            && self.unrelated_chains_that_moved == 0;

        // Clause 4: NO DANGLING/INVALID FACES.
        //
        // The audit is green on every arrangement, and the audit can be red:
        // the mutation walk perturbs every derived slot of a sampled
        // arrangement and counts what each check catches. `caught_by_neither`
        // being zero is the property; `slots_perturbed` being large is what
        // stops the row from being satisfied by a walk that visited nothing.
        let p: &ResolvingPower = &self.audit_resolving_power;
        let faces_row = self.arms_failing_the_audit == 0
            && self.arms_that_are_not_their_own_assembly == 0
            && cfg
                .min_resolving_power_probes
                .met_by(u64::from(p.arrangements_probed))
            && p.slots_perturbed > 0
            && p.caught_by_neither == 0
            && p.no_ops == 0
            // Both directions. The walk must find slots the audit itself
            // catches, or "the audit is green" would rest entirely on the
            // assembly comparison and the row would be citing a check it does
            // not use.
            && p.caught_by_audit > 0;

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
                     complementary-connectivity arms DISAGREE: {:?}. Every one of them comes from \
                     the STRUCTURAL register ({} of {} arms), and that is not an accident of \
                     fixture choice - STATUS_M4_5 limitation 18 records that ZERO of the corpus's \
                     arms have a convention-dependent class, so on the corpus alone this clause \
                     would be a statement about singletons. The knockout that makes it false is in \
                     the tree and runs under `--ignored`: `ProxyKnockout::Select` gives the group \
                     its first class and the row goes NOT MET",
                    self.groups,
                    self.classes_in,
                    self.classes_out,
                    self.convention_dependent_groups,
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
                    "Six of the seven §12 invariants have no failure mode in this representation - \
                     the twin is a bit flip, the owners are a pair, a loop is a cyclic sequence, a \
                     crack is not constructible, segments are integer-lattice unit steps and the \
                     exterior is an ordinary face - so there is nothing there to measure and \
                     nothing there is claimed. The seventh IS a computation, and this row is about \
                     it: {} of {} arms failed the audit, {} were not the assembly of their own \
                     labelling. What makes those zeros evidence rather than silence is the \
                     MUTATION WALK, which is the world in which the audit is red: on {} sampled \
                     arrangements out of {} arms seen it perturbed {} derived slots one at a time \
                     - the walk is an exhaustive destructuring of the structure, so a field added \
                     without a site does not compile - and caught {} by the audit, {} by assembly \
                     equality, {} by neither, with {} perturbations that changed nothing. The \
                     audit's own share is the honest number here and it is not the whole: most \
                     slots are face ids inside a uniform face, where no construction invariant can \
                     see the change and only the assembly comparison does. Both checks are cited \
                     because a row citing one would claim a resolving power that one does not have",
                    self.arms_failing_the_audit,
                    self.arms_measured,
                    self.arms_that_are_not_their_own_assembly,
                    p.arrangements_probed,
                    p.arms_seen,
                    p.slots_perturbed,
                    p.caught_by_audit,
                    p.caught_by_assembly_equality,
                    p.caught_by_neither,
                    p.no_ops
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
        "transactions_rolled_back",
        "unrelated_chains_total",
        "unrelated_chains_that_moved",
        "transactions_with_no_unrelated_population",
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
