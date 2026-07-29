//! **vice-fit — spec v1.3 Stage G (§14), the candidate-generation half.**
//!
//! §28 M6 has six bullets. This crate delivers the first two:
//!
//! | §28 M6 bullet | here |
//! |---|---|
//! | hierarchical span candidates | [`schedule`] and [`span`] |
//! | candidate-generation budgets | [`schedule::FitBudget`], [`FitRefusal::BudgetExceeded`] |
//! | k-best jet-compatible grammar paths | **NOT STARTED** |
//! | joint constrained chain refit | **NOT STARTED** |
//! | explicit code lengths | **NOT STARTED** |
//! | primitive/relation hypotheses | **NOT STARTED** |
//!
//! **Nothing here claims exact G1**, and nothing here computes a code length.
//! §14.3 says a DP over families and breakpoints is not G1 and that the
//! guarantee comes from the joint refit; that refit does not exist, so this
//! crate produces INDEPENDENT primitives per interval and says so. A
//! `CurveChain` is never assembled here — assembling one would imply joins,
//! and a join implies a claim about tangents this milestone has not earned.
//! `crates/vice-ir/tests/g1_shared_tangent.rs` measures what the join types
//! currently hold: the tangent is shared as a DECLARATION and is bound to the
//! segment geometry by nothing.
//!
//! ## What it consumes
//!
//! `vice_evidence::BoundaryChain` — the Stage F observation of §13, whose
//! samples are already resampled by physical arclength and carry the corridor
//! halfwidth the proposal cost normalises by. §24's pseudocode opens with
//! `physical_resample(obs, cfg.sample_step_px)`; that resampling already
//! happened in M4, and re-doing it here would be a second opinion about a
//! quantity that has one owner.
//!
//! ## What it is NOT bound to
//!
//! The DCEL. §13 step 4 asks that a chain be tied to its DCEL
//! endpoints/junctions; `BoundaryChain` carries no boundary or vertex id, and
//! this crate invents none. So a candidate here is a candidate for a curve on
//! a contour of the coverage field, not yet for a boundary of the arrangement,
//! and the isotopy conditions of §12 — which `vice_topology`'s
//! `curve_replacement_isotopy` refuses by naming `fitted_curve` as the first
//! missing capability — remain unmet. Stated because it bounds every number
//! this crate produces.

#![forbid(unsafe_code)]

pub mod cost;
pub mod schedule;
pub mod span;

use serde::Serialize;

pub use cost::{
    normal_deviation, proposal_cost, CostRefusal, ProposalCost, MATERIAL_DEVIATION_FRACTION,
};
pub use schedule::{
    hierarchical_schedule, FitBudget, Support, FIT_BUDGET_V1, MIN_SUPPORT_SAMPLES,
    SUPPORTS_PER_SAMPLE_BOUND,
};
pub use span::{
    fit, flattening_error_px, NoFit, SpanCandidate, SpanFamily, DEVIATION_CHORD_TOLERANCE_PX,
    FAMILIES_DELIBERATELY_NOT_FITTED, FITTED_FAMILIES, PARAMETER_REFINEMENT_PASSES,
};

use vice_evidence::BoundaryChain;

/// Why no candidates were produced for a chain.
///
/// Every variant carries the numbers that produced it. A refusal that says
/// only "no" is a refusal a reader must reproduce to understand, and the
/// project has a ledger entry for the value that means both "zero" and "there
/// was nothing" (F-0075).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "refusal", rename_all = "snake_case")]
pub enum FitRefusal {
    /// Shorter than [`MIN_SUPPORT_SAMPLES`]: no support can be judged, so
    /// every family would fit exactly and the residuals would carry nothing.
    ChainTooShort { samples: usize, minimum: usize },
    /// The schedule times the families exceeds the hard cap of §14.2.
    ///
    /// A REFUSAL and not a truncation. Truncating means dropping candidates in
    /// whatever order they were generated, and the order is by scale — so a
    /// truncation silently reinstates exactly the loss of long support that
    /// §14.2's second sentence exists to prevent.
    BudgetExceeded {
        samples: usize,
        supports: usize,
        would_generate: usize,
        cap: usize,
    },
    /// A corridor halfwidth that is not strictly positive. The proposal cost
    /// divides by it, and a floor substituted here would be a magic number
    /// deciding the ranking of every candidate over that sample.
    NonPositiveHalfwidth { sample: usize, halfwidth_px: f64 },
    /// A non-finite sample position, normal, or arclength weight.
    NonFiniteSample { sample: usize },
}

/// Everything the candidate stage produced for one chain, including the
/// quantities that say how much of it was real.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChainCandidates {
    pub chain_samples: usize,
    pub closed: bool,
    pub supports: usize,
    pub candidates: Vec<SpanCandidate>,
    /// Universe names of the families that actually produced a candidate on
    /// this chain, MEASURED. `FITTED_FAMILIES` is a literal; this is not, and
    /// a family that stops fitting shows here as an absent name rather than as
    /// silence.
    pub families_present: Vec<&'static str>,
    /// `(family, reason)` counts for supports where a family produced nothing.
    pub no_fits: Vec<(&'static str, NoFit, usize)>,
    /// `(family, reason)` counts for supports where a family FITTED and the
    /// §14.4 cost then refused it — overwhelmingly because some sample's
    /// normal line does not meet the candidate at all, which is a candidate
    /// the corridor cannot see. Counted separately from `no_fits` because "no
    /// curve" and "a curve the evidence has no opinion about" are different
    /// facts about the same family.
    pub no_costs: Vec<(&'static str, CostRefusal, usize)>,
    /// How many candidates the cap had left over. Published so that a backstop
    /// quietly becoming a working limit is visible before it binds.
    pub budget_headroom: usize,
    /// The largest `|d_n| / d_euclidean` over every candidate of this chain,
    /// and the Euclidean deviation at which it occurred.
    ///
    /// This is the size of the correction C302 made: `1.0` would mean the
    /// Euclidean lower bound this crate used to integrate was §14.4's `d_n`
    /// all along, and anything above it is what that approximation was
    /// costing. Published rather than argued, at the deviation where it
    /// happened, because a ratio without the magnitude it occurred at is the
    /// shape F-0085 records.
    pub max_normal_to_euclidean_ratio: f64,
    pub ratio_at_deviation_px: f64,
}

/// **§28 M6 bullets 1 and 2 for one chain.**
///
/// Offers every family on every support of the hierarchical schedule, refusing
/// rather than truncating when the budget would bind.
pub fn span_candidates(
    chain: &BoundaryChain,
    budget: &FitBudget,
) -> Result<ChainCandidates, FitRefusal> {
    let samples = &chain.samples;
    let n = samples.len();

    for (i, s) in samples.iter().enumerate() {
        if !s.p.is_finite() || !s.normal.is_finite() || !s.weight_ds.is_finite() {
            return Err(FitRefusal::NonFiniteSample { sample: i });
        }
        if !(s.halfwidth.is_finite() && s.halfwidth > 0.0) {
            return Err(FitRefusal::NonPositiveHalfwidth {
                sample: i,
                halfwidth_px: s.halfwidth,
            });
        }
    }

    if n < MIN_SUPPORT_SAMPLES {
        return Err(FitRefusal::ChainTooShort {
            samples: n,
            minimum: MIN_SUPPORT_SAMPLES,
        });
    }

    let supports = hierarchical_schedule(n);
    let would_generate = supports.len() * FITTED_FAMILIES.len();
    if would_generate > budget.cap() {
        return Err(FitRefusal::BudgetExceeded {
            samples: n,
            supports: supports.len(),
            would_generate,
            cap: budget.cap(),
        });
    }

    let mut candidates = Vec::with_capacity(would_generate);
    let mut no_fits: Vec<(&'static str, NoFit, usize)> = Vec::new();
    let mut no_costs: Vec<(&'static str, CostRefusal, usize)> = Vec::new();
    let mut worst_ratio = 1.0f64;
    let mut worst_ratio_at = 0.0f64;

    for support in &supports {
        for family in FITTED_FAMILIES {
            let seg = match fit(samples, *support, family) {
                Ok(s) => s,
                Err(why) => {
                    bump(&mut no_fits, family.universe_name(), why);
                    continue;
                }
            };
            let cost = match proposal_cost(samples, *support, &seg) {
                Ok(c) => c,
                Err(why) => {
                    bump(&mut no_costs, family.universe_name(), why);
                    continue;
                }
            };
            if cost.max_normal_to_euclidean_ratio > worst_ratio {
                worst_ratio = cost.max_normal_to_euclidean_ratio;
                worst_ratio_at = cost.ratio_at_deviation_px;
            }
            candidates.push(SpanCandidate {
                support: *support,
                family,
                segment: seg,
                cost,
            });
        }
    }

    let mut families_present: Vec<&'static str> = candidates
        .iter()
        .map(|c| c.family.universe_name())
        .collect();
    families_present.sort_unstable();
    families_present.dedup();

    Ok(ChainCandidates {
        chain_samples: n,
        closed: chain.closed,
        supports: supports.len(),
        budget_headroom: budget.cap().saturating_sub(candidates.len()),
        candidates,
        families_present,
        no_fits,
        no_costs,
        max_normal_to_euclidean_ratio: worst_ratio,
        ratio_at_deviation_px: worst_ratio_at,
    })
}

fn bump<T: PartialEq>(acc: &mut Vec<(&'static str, T, usize)>, family: &'static str, why: T) {
    if let Some(e) = acc.iter_mut().find(|e| e.0 == family && e.1 == why) {
        e.2 += 1;
    } else {
        acc.push((family, why, 1));
    }
}
