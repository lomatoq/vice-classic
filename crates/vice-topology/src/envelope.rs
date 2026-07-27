//! The candidate envelope and its three pruning tiers (spec §11.3, §23).
//!
//! ```text
//! 1. invalid topology            -> remove
//! 2. certifiably dominated       -> remove
//! 3. budget pruning              -> only with diversity quotas and an
//!                                   explicit approximation-risk record
//! ```
//!
//! ## This stage does not choose
//!
//! §11.3 is titled "candidate envelope, not an early winner", §32 rule 14
//! forbids a topology winner from a proxy, and the §28 M4.5 gate measures
//! RECALL — whether the admissible answer is IN the envelope — precisely so
//! that a weak score is not asked to pick it. Nothing here sorts to a top-1
//! and nothing here returns a single hypothesis. What it returns is a set,
//! with the reason every removed member was removed.
//!
//! ## Why the tiers are in this order
//!
//! Tier 1 is a property of the labelling alone and costs nothing. Tier 2 is
//! the §5.4 interval rule and can only fire when one candidate's CERTIFIED
//! upper bound is below another's CERTIFIED lower bound — an epsilon is not
//! allowed to choose a topology, so there is no tolerance in it. Tier 3 is
//! the only approximate one, and §11.3 licenses it only under two
//! conditions, both enforced here:
//!
//! - **diversity quotas.** Before the budget is applied, each quota class
//!   keeps its own best: every distinct (components, holes) signature, every
//!   field of §11.1, every connectivity arm, every saddle reading. A budget
//!   that would cut below a quota RAISES ITSELF and records that it did,
//!   because a quota that yields to a budget is not a quota;
//! - **an explicit approximation-risk record.** What was dropped, the worst
//!   lower bound among the dropped, the best among the kept, and an estimate
//!   of the posterior mass lost.
//!
//! ## The mass estimate is not dressed as a bound
//!
//! §23 requires the budget report to state the lost estimated posterior
//! mass; §1.5 forbids serializing a heuristic as `Certified`. Both hold:
//! [`LostMassEstimate`] has a variant for "nothing was removed", which is a
//! FACT, and a variant for a softmax over surrogate costs, which says in its
//! own name and note that it is uncalibrated and is not a posterior. There
//! is no third variant, so there is nowhere to put a number that pretends.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::hypothesis::TopologyHypothesis;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InvalidReason {
    /// Nothing is labelled inside: there is no region at all.
    EmptyForeground,
    /// Everything is labelled inside: there is no visible interface, so the
    /// candidate describes no boundary a vectorizer could carry.
    NoVisibleInterface,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Removed {
    pub digest: String,
    pub components: u32,
    pub holes: u32,
    pub tier: &'static str,
    pub why: String,
}

/// The estimate §23 asks for, typed so that it cannot be read as a bound.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LostMassEstimate {
    /// The budget removed nothing. Not an estimate: a fact.
    NothingRemoved,
    /// A softmax over the SURROGATE lower bounds. Not a posterior, not
    /// calibrated, and §1.5 forbids calling it certified — so it says so.
    UncalibratedSurrogate {
        value: f64,
        scale: f64,
        note: &'static str,
    },
}

const MASS_NOTE: &str = "softmax over surrogate lower-bound costs at a fixed scale: it is NOT a \
                         posterior and NOT calibrated (spec 1.5 forbids serializing a heuristic \
                         as certified). It is published so that budget pruning has a size, not \
                         so that it has a guarantee";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ApproximationRisk {
    pub removed_by_budget: usize,
    pub kept: usize,
    pub lost_mass: LostMassEstimate,
    /// The best (smallest) certified lower bound among the candidates the
    /// budget dropped, and the best among those it kept. If the first is
    /// below the second, the budget dropped something that could still have
    /// been the answer, and that is exactly what the number is for.
    pub best_removed_lower_bound: Option<f64>,
    pub best_kept_lower_bound: Option<f64>,
    /// Set when a diversity quota forced the budget up.
    pub budget_raised_by_quota_to: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PruningRecord {
    pub proposed: usize,
    pub invalid_removed: usize,
    pub duplicates_removed: usize,
    pub dominated_removed: usize,
    pub budget_removed: usize,
    pub quota_classes: usize,
    pub approximation_risk: ApproximationRisk,
    pub removed: Vec<Removed>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Envelope {
    pub hypotheses: Vec<TopologyHypothesis>,
    pub pruning: PruningRecord,
}

impl Envelope {
    /// How many surviving candidates came from each complementary-connectivity
    /// arm, as `(foreground 4, foreground 8)`.
    ///
    /// This is the measurement RT45-A1 asked for, and it is not decoration.
    /// The recall clause relaxes its success condition by reference to a
    /// mechanism — "§5.3 gives two admissible conventions, so a candidate
    /// matching EITHER truth counts" — and that relaxation is only justified
    /// while the envelope actually carries both. Nothing measured it: the red
    /// team deleted one arm from the generator, and the gate table, the
    /// config hash and the structural projection were all unmoved.
    ///
    /// A metric that weakens its own success condition by pointing at a
    /// mechanism has to measure that the mechanism is there.
    pub fn candidates_by_arm(&self) -> (usize, usize) {
        let four = self
            .hypotheses
            .iter()
            .filter(|h| h.provenance.foreground_connectivity == "4")
            .count();
        (four, self.hypotheses.len() - four)
    }

    /// Distinct (components, holes) readings the envelope still carries.
    pub fn signature_classes(&self) -> BTreeSet<(u32, u32)> {
        self.hypotheses
            .iter()
            .map(|h| (h.signature.components, h.signature.holes))
            .collect()
    }
    pub fn contains_signature(&self, components: u32, holes: u32) -> bool {
        self.hypotheses
            .iter()
            .any(|h| h.signature.components == components && h.signature.holes == holes)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct EnvelopeConfig {
    /// Candidates kept after tier 3, before quotas raise it.
    pub budget: usize,
    /// Members each quota class keeps regardless of the budget.
    pub per_quota_class: usize,
    /// Scale of the surrogate softmax, in the units of the lower bound
    /// (squared coverage summed over pixels).
    pub mass_scale: f64,
}

pub const ENVELOPE_CONFIG_V1: EnvelopeConfig = EnvelopeConfig {
    budget: 48,
    per_quota_class: 2,
    mass_scale: 8.0,
};

fn invalid(h: &TopologyHypothesis) -> Option<InvalidReason> {
    let n = h.labelling.inside().len();
    let inside = h.labelling.count_inside();
    if inside == 0 {
        Some(InvalidReason::EmptyForeground)
    } else if inside == n {
        Some(InvalidReason::NoVisibleInterface)
    } else {
        None
    }
}

/// Apply the three tiers of §11.3 to a proposed candidate set.
pub fn prune(mut proposed: Vec<TopologyHypothesis>, cfg: &EnvelopeConfig) -> Envelope {
    let n_proposed = proposed.len();
    let mut removed: Vec<Removed> = Vec::new();

    // Deterministic input order: the whole record below is a function of the
    // candidate set, not of the order the generator happened to emit.
    proposed.sort_by(|a, b| {
        a.signature
            .digest
            .cmp(&b.signature.digest)
            .then(a.provenance.field.cmp(&b.provenance.field))
            .then(a.provenance.saddle.cmp(&b.provenance.saddle))
            .then(a.provenance.level.total_cmp(&b.provenance.level))
            .then(a.provenance.palette_id.cmp(&b.provenance.palette_id))
            .then(a.provenance.formation_id.cmp(&b.provenance.formation_id))
    });

    // --- Tier 1: invalid topology, and exact duplicates ---------------------
    let mut kept: Vec<TopologyHypothesis> = Vec::new();
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut invalid_removed = 0;
    let mut duplicates_removed = 0;
    for h in proposed {
        if let Some(why) = invalid(&h) {
            invalid_removed += 1;
            removed.push(Removed {
                digest: h.signature.digest.clone(),
                components: h.signature.components,
                holes: h.signature.holes,
                tier: "invalid",
                why: format!("{why:?}"),
            });
            continue;
        }
        match seen.get(&h.signature.digest).copied() {
            Some(i) => {
                duplicates_removed += 1;
                // The duplicate is the SAME labelling under the SAME
                // convention — the digest includes the convention, so a
                // cross-arm collision cannot occur (M45-N7).
                if !kept[i].ambiguity.level_from_fixed_probe_only {
                    // already event-driven, nothing to learn
                } else if !h.ambiguity.level_from_fixed_probe_only {
                    // The same labelling is ALSO reachable without a fixed
                    // probe: that fact belongs to the survivor, or the
                    // no-magic-threshold measurement would undercount.
                    kept[i].ambiguity.level_from_fixed_probe_only = false;
                    kept[i].provenance.level_from_fixed_probe_only = false;
                }
            }
            None => {
                seen.insert(h.signature.digest.clone(), kept.len());
                kept.push(h);
            }
        }
    }

    // --- Tier 2: certifiably dominated (§5.4 interval rule) -----------------
    let best_upper = kept
        .iter()
        .filter_map(|h| h.cost.achievable)
        .fold(f64::INFINITY, f64::min);
    let mut survivors: Vec<TopologyHypothesis> = Vec::new();
    let mut dominated_removed = 0;
    for h in kept {
        if best_upper < h.cost.lower {
            dominated_removed += 1;
            removed.push(Removed {
                digest: h.signature.digest.clone(),
                components: h.signature.components,
                holes: h.signature.holes,
                tier: "certifiably_dominated",
                why: format!(
                    "its certified lower bound {:.6} is above another candidate's certified \
                     achievable cost {best_upper:.6}",
                    h.cost.lower
                ),
            });
            continue;
        }
        survivors.push(h);
    }

    // --- Tier 3: budget, under diversity quotas -----------------------------
    let quotas = quota_keep(&survivors, cfg);
    let quota_classes = quota_class_count(&survivors);
    let mut order: Vec<usize> = (0..survivors.len()).collect();
    order.sort_by(|a, b| {
        survivors[*a]
            .cost
            .lower
            .total_cmp(&survivors[*b].cost.lower)
            .then(
                survivors[*a]
                    .signature
                    .digest
                    .cmp(&survivors[*b].signature.digest),
            )
    });
    let mut keep_set: BTreeSet<usize> = quotas.clone();
    let raised = if keep_set.len() > cfg.budget {
        Some(keep_set.len())
    } else {
        None
    };
    for i in &order {
        // The quotas may already have filled or overrun the budget, in which
        // case nothing more is added and the overrun is recorded above. A
        // quota that yielded to the budget would not be a quota.
        if keep_set.len() >= cfg.budget {
            break;
        }
        keep_set.insert(*i);
    }

    let mut best_removed = f64::INFINITY;
    let mut best_kept = f64::INFINITY;
    let mut budget_removed = 0;
    let mut out: Vec<TopologyHypothesis> = Vec::new();
    let mut dropped_costs: Vec<f64> = Vec::new();
    let mut all_costs: Vec<f64> = Vec::new();
    for (i, h) in survivors.into_iter().enumerate() {
        all_costs.push(h.cost.lower);
        if keep_set.contains(&i) {
            best_kept = best_kept.min(h.cost.lower);
            out.push(h);
        } else {
            budget_removed += 1;
            best_removed = best_removed.min(h.cost.lower);
            dropped_costs.push(h.cost.lower);
            removed.push(Removed {
                digest: h.signature.digest.clone(),
                components: h.signature.components,
                holes: h.signature.holes,
                tier: "budget",
                why: format!(
                    "outside the budget of {} after diversity quotas; lower bound {:.6}",
                    cfg.budget, h.cost.lower
                ),
            });
        }
    }

    out.sort_by(|a, b| {
        a.cost
            .lower
            .total_cmp(&b.cost.lower)
            .then(a.signature.digest.cmp(&b.signature.digest))
    });
    removed.sort_by(|a, b| a.tier.cmp(b.tier).then(a.digest.cmp(&b.digest)));

    Envelope {
        pruning: PruningRecord {
            proposed: n_proposed,
            invalid_removed,
            duplicates_removed,
            dominated_removed,
            budget_removed,
            quota_classes,
            approximation_risk: ApproximationRisk {
                removed_by_budget: budget_removed,
                kept: out.len(),
                lost_mass: lost_mass(&all_costs, &dropped_costs, cfg.mass_scale),
                best_removed_lower_bound: finite(best_removed),
                best_kept_lower_bound: finite(best_kept),
                budget_raised_by_quota_to: raised,
            },
            removed,
        },
        hypotheses: out,
    }
}

fn finite(v: f64) -> Option<f64> {
    v.is_finite().then_some(v)
}

/// The quota keys of §11.3's "diversity quotas": one class per distinct
/// topological reading, per restored field, per connectivity arm and per
/// saddle reading. A budget may not cut a class below `per_quota_class`.
fn quota_key(h: &TopologyHypothesis) -> Vec<String> {
    vec![
        format!("sig:{}:{}", h.signature.components, h.signature.holes),
        format!("field:{}", h.provenance.field.as_str()),
        format!("conn:{}", h.provenance.foreground_connectivity),
        format!("saddle:{}", h.provenance.saddle.as_str()),
    ]
}

fn quota_class_count(hs: &[TopologyHypothesis]) -> usize {
    let mut set = BTreeSet::new();
    for h in hs {
        for k in quota_key(h) {
            set.insert(k);
        }
    }
    set.len()
}

fn quota_keep(hs: &[TopologyHypothesis], cfg: &EnvelopeConfig) -> BTreeSet<usize> {
    let mut by_class: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, h) in hs.iter().enumerate() {
        for k in quota_key(h) {
            by_class.entry(k).or_default().push(i);
        }
    }
    let mut out = BTreeSet::new();
    for members in by_class.values() {
        let mut m = members.clone();
        m.sort_by(|a, b| {
            hs[*a]
                .cost
                .lower
                .total_cmp(&hs[*b].cost.lower)
                .then(hs[*a].signature.digest.cmp(&hs[*b].signature.digest))
        });
        for i in m.into_iter().take(cfg.per_quota_class) {
            out.insert(i);
        }
    }
    out
}

fn lost_mass(all: &[f64], dropped: &[f64], scale: f64) -> LostMassEstimate {
    if dropped.is_empty() {
        return LostMassEstimate::NothingRemoved;
    }
    let min = all.iter().copied().fold(f64::INFINITY, f64::min);
    let w = |c: f64| (-(c - min) / scale).exp();
    let total: f64 = all.iter().map(|c| w(*c)).sum();
    let lost: f64 = dropped.iter().map(|c| w(*c)).sum();
    LostMassEstimate::UncalibratedSurrogate {
        value: if total > 0.0 { lost / total } else { 0.0 },
        scale,
        note: MASS_NOTE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cubical::{Labelling, SaddleResolution};
    use crate::events::LevelOrigin;
    use crate::field::FieldKind;
    use crate::hypothesis::{from_events, CandidateContext};
    use vice_ir::ComplementaryConnectivity;

    fn arm() -> ComplementaryConnectivity {
        ComplementaryConnectivity::arms()[0]
    }

    struct Spec<'a> {
        w: usize,
        h: usize,
        alpha: &'a [f64],
        level: f64,
        field: FieldKind,
        saddle: SaddleResolution,
        conn: ComplementaryConnectivity,
    }

    fn candidate(inside: Vec<bool>, s: &Spec<'_>) -> TopologyHypothesis {
        from_events(
            Labelling::new(s.w, s.h, inside),
            &CandidateContext {
                conn: s.conn,
                alpha: s.alpha,
                level: s.level,
                level_origins: &[LevelOrigin::PersistencePlateau],
                persistence: 0.3,
                kernel: None,
                palette_id: "pal",
                formation_id: "form",
                field: s.field,
                saddle: s.saddle,
            },
        )
    }

    fn square(w: usize, h: usize, x0: usize, x1: usize) -> Vec<bool> {
        (0..w * h).map(|i| (x0..x1).contains(&(i % w))).collect()
    }

    /// Tier 1 removes exactly the two degenerate labellings and names which.
    #[test]
    fn tier_one_removes_the_labellings_that_describe_no_boundary() {
        let (w, h) = (6usize, 6usize);
        let alpha = vec![0.6f64; w * h];
        let good = candidate(
            square(w, h, 1, 4),
            &Spec {
                w,
                h,
                alpha: &alpha,
                level: 0.5,
                field: FieldKind::RawCoverage,
                saddle: SaddleResolution::Thresholded,
                conn: arm(),
            },
        );
        let empty = candidate(
            vec![false; w * h],
            &Spec {
                w,
                h,
                alpha: &alpha,
                level: 0.5,
                field: FieldKind::RawCoverage,
                saddle: SaddleResolution::Thresholded,
                conn: arm(),
            },
        );
        let full = candidate(
            vec![true; w * h],
            &Spec {
                w,
                h,
                alpha: &alpha,
                level: 0.5,
                field: FieldKind::RawCoverage,
                saddle: SaddleResolution::Thresholded,
                conn: arm(),
            },
        );
        let e = prune(vec![good, empty, full], &ENVELOPE_CONFIG_V1);
        assert_eq!(e.hypotheses.len(), 1);
        assert_eq!(e.pruning.invalid_removed, 2);
        let whys: Vec<&str> = e
            .pruning
            .removed
            .iter()
            .filter(|r| r.tier == "invalid")
            .map(|r| r.why.as_str())
            .collect();
        assert!(whys.contains(&"EmptyForeground"));
        assert!(whys.contains(&"NoVisibleInterface"));
    }

    /// A labelling proposed twice under the SAME convention collapses to one
    /// candidate.
    ///
    /// The previous version of this test asserted that a cross-arm duplicate
    /// keeps a `both_connectivity_arms_agree` flag — and reached that branch
    /// by overwriting the digest and the convention by hand. M45-N7: the
    /// branch is unreachable on the production path, because the digest
    /// includes the convention, so the test was proving a property of a state
    /// `propose` cannot build (meta-rule M-2). The flag is gone; what the two
    /// arms contribute is counted by `candidates_by_arm`, which is asserted
    /// below on candidates the generator actually produced.
    #[test]
    fn an_exact_duplicate_collapses_and_both_arms_are_counted() {
        let (w, h) = (6usize, 6usize);
        let alpha = vec![0.6f64; w * h];
        let [four, eight] = ComplementaryConnectivity::arms();
        let a = candidate(
            square(w, h, 1, 4),
            &Spec {
                w,
                h,
                alpha: &alpha,
                level: 0.5,
                field: FieldKind::RawCoverage,
                saddle: SaddleResolution::Thresholded,
                conn: four,
            },
        );
        let b = candidate(
            square(w, h, 1, 4),
            &Spec {
                w,
                h,
                alpha: &alpha,
                level: 0.5,
                field: FieldKind::RawCoverage,
                saddle: SaddleResolution::Thresholded,
                conn: eight,
            },
        );
        // Two candidates under DIFFERENT arms are two candidates: the digest
        // carries the convention, so there is no collision and nothing to
        // deduplicate. That is the fact M45-N7 established, asserted here
        // rather than worked around.
        assert_ne!(
            a.signature.digest, b.signature.digest,
            "the digest must separate the two conventions, or a cross-arm duplicate would              silently erase one arm"
        );
        let both = prune(vec![a.clone(), b.clone()], &ENVELOPE_CONFIG_V1);
        assert_eq!(both.hypotheses.len(), 2);
        assert_eq!(both.pruning.duplicates_removed, 0);
        assert_eq!(
            both.candidates_by_arm(),
            (1, 1),
            "both complementary arms must be counted, because the recall clause relaxes its              success condition by pointing at them"
        );

        // A genuine duplicate - same labelling, same convention - still
        // collapses, which is what the tier exists for.
        let e = prune(vec![a.clone(), a], &ENVELOPE_CONFIG_V1);
        assert_eq!(e.hypotheses.len(), 1);
        assert_eq!(e.pruning.duplicates_removed, 1);
        assert_eq!(e.candidates_by_arm(), (1, 0));
    }

    /// Tier 3 respects the quotas even when they exceed the budget, and says
    /// so; and a run that drops nothing publishes a FACT rather than an
    /// estimate.
    #[test]
    fn the_budget_yields_to_the_quotas_and_records_that_it_did() {
        let (w, h) = (8usize, 8usize);
        let alpha: Vec<f64> = (0..w * h).map(|i| (i % 7) as f64 / 7.0).collect();
        let mut hs = Vec::new();
        for k in 1..7usize {
            hs.push(candidate(
                square(w, h, 0, k),
                &Spec {
                    w,
                    h,
                    alpha: &alpha,
                    level: 0.5,
                    field: FieldKind::ALL[k % FieldKind::ALL.len()],
                    saddle: SaddleResolution::ALL[k % 3],
                    conn: arm(),
                },
            ));
        }
        let tight = EnvelopeConfig {
            budget: 1,
            per_quota_class: 1,
            mass_scale: 8.0,
        };
        let e = prune(hs.clone(), &tight);
        assert!(
            e.hypotheses.len() > 1,
            "a budget of one may not cut below the diversity quotas: kept {}",
            e.hypotheses.len()
        );
        assert_eq!(
            e.pruning.approximation_risk.budget_raised_by_quota_to,
            Some(e.hypotheses.len()),
            "the raise must be RECORDED, or a quota that overrides a budget is invisible"
        );
        assert!(e.pruning.quota_classes > 1);

        let roomy = prune(hs, &ENVELOPE_CONFIG_V1);
        assert_eq!(roomy.pruning.budget_removed, 0);
        assert_eq!(
            roomy.pruning.approximation_risk.lost_mass,
            LostMassEstimate::NothingRemoved,
            "when nothing is dropped the record is a fact, not a heuristic"
        );
    }

    /// The lost-mass estimate is typed so it cannot be read as a bound, and
    /// it is non-zero exactly when something was dropped.
    #[test]
    fn the_lost_mass_estimate_says_what_it_is() {
        let m = lost_mass(&[0.0, 1.0, 2.0], &[2.0], 8.0);
        match m {
            LostMassEstimate::UncalibratedSurrogate { value, scale, note } => {
                assert!(value > 0.0 && value < 1.0, "{value}");
                assert!((scale - 8.0).abs() < 1e-12);
                assert!(note.contains("NOT a posterior"));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            lost_mass(&[0.0, 1.0], &[], 8.0),
            LostMassEstimate::NothingRemoved
        );
    }

    /// Pruning is a function of the SET, not of the order it arrives in.
    #[test]
    fn the_envelope_does_not_depend_on_proposal_order() {
        let (w, h) = (8usize, 8usize);
        let alpha: Vec<f64> = (0..w * h).map(|i| (i % 5) as f64 / 5.0).collect();
        let mut hs = Vec::new();
        for k in 1..6usize {
            hs.push(candidate(
                square(w, h, 0, k),
                &Spec {
                    w,
                    h,
                    alpha: &alpha,
                    level: 0.5,
                    field: FieldKind::ALL[k % FieldKind::ALL.len()],
                    saddle: SaddleResolution::ALL[k % 3],
                    conn: arm(),
                },
            ));
        }
        let a = prune(hs.clone(), &ENVELOPE_CONFIG_V1);
        hs.reverse();
        let b = prune(hs, &ENVELOPE_CONFIG_V1);
        assert_eq!(a, b);
    }

    /// Tier 2 is the interval rule and nothing else: a candidate whose
    /// CERTIFIED lower bound is above another's CERTIFIED achievable cost
    /// goes, and one whose interval merely overlaps stays.
    #[test]
    fn tier_two_removes_only_what_the_interval_rule_removes() {
        let (w, h) = (10usize, 10usize);
        // The observation IS one of the candidates, so that candidate has a
        // zero lower bound and a small achievable cost; a very different
        // labelling owes a lot.
        let alpha: Vec<f64> = square(w, h, 2, 8)
            .into_iter()
            .map(|b| if b { 1.0 } else { 0.0 })
            .collect();
        let right = candidate(
            square(w, h, 2, 8),
            &Spec {
                w,
                h,
                alpha: &alpha,
                level: 0.5,
                field: FieldKind::RawCoverage,
                saddle: SaddleResolution::Thresholded,
                conn: arm(),
            },
        );
        let wrong = candidate(
            square(w, h, 0, 1),
            &Spec {
                w,
                h,
                alpha: &alpha,
                level: 0.5,
                field: FieldKind::RawCoverage,
                saddle: SaddleResolution::Thresholded,
                conn: arm(),
            },
        );
        assert!(right.cost.achievable.is_some());
        assert!(wrong.cost.lower > 0.0);
        let e = prune(vec![right, wrong], &ENVELOPE_CONFIG_V1);
        assert_eq!(e.pruning.dominated_removed, 1, "{:?}", e.pruning.removed);
        assert_eq!(e.hypotheses.len(), 1);
        assert!(e.contains_signature(1, 0));
    }
}
