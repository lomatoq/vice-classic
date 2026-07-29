//! **vice-fit — spec v1.3 Stage G/H (§14–§15).**
//!
//! §28 M6 has six bullets. This crate implements the boundary-chain part of all
//! six; only the scene-level symmetry/repetition relations require the
//! scene/partition binding named below:
//!
//! | §28 M6 bullet | here |
//! |---|---|
//! | hierarchical span candidates | [`schedule`] and [`span`] |
//! | candidate-generation budgets | [`schedule::FitBudget`], [`FitRefusal::BudgetExceeded`] |
//! | k-best jet-compatible grammar paths | [`grammar`] and [`models`] |
//! | joint constrained chain refit | [`refit`] and [`solve`] |
//! | explicit code lengths | [`code`] |
//! | primitive/relation hypotheses | closed-chain models in [`primitive`] and constrained relations in [`relation`] |
//!
//! Exact G1 is held by [`RefitChain`]'s representation: a smooth node stores one
//! direction and both incident non-line segments derive their handles from it;
//! unrepresentable line/arc cases are refused before solving. [`g1_readings`]
//! then measures every accepted smooth node as an independent witness. The
//! selector uses the physical-bit [`GeometryCodeTable`], and production entry
//! points always use [`GEOMETRY_CODE_TABLE_V1`].
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

pub mod code;
pub mod corner;
pub mod cost;
pub mod gate;
pub mod grammar;
pub mod models;
pub mod primitive;
pub mod refit;
pub mod relation;
pub mod schedule;
pub mod solve;
pub mod span;

use serde::Serialize;

pub use code::{
    gap_bits, log2_binomial, pricing_surface_v1, residual_bits, ChainCode, GeometryCodeTable,
    GEOMETRY_CODE_TABLE_V1, JOIN_KINDS, REFERENCE_CANVAS_DIM_PX,
};
pub use corner::{corner_anchors, corner_proposals, CornerProposal};
pub use cost::{
    normal_deviation, proposal_cost, CostRefusal, ProposalCost, RatioReading,
    MATERIAL_DEVIATION_FRACTION,
};
pub use gate::{
    GATE_MAX_BREAKPOINT_FRACTION_DELTA, GATE_MAX_CUT_ROTATION_DELTA_BITS, GATE_MAX_G1_SPREAD_RAD,
    GATE_MAX_TRANSLATION_DELTA_BITS, GATE_MIN_CUT_NONTRIVIAL_SPREAD_BITS, GATE_MIN_G1_NODES,
    GATE_MIN_G1_POSITIVE_CONTROL_RAD, GATE_MIN_INVARIANCE_LEGS, GATE_MIN_NO_BIC_EXTRA_SEGMENTS,
};
pub use grammar::{
    build_edges, jet_class, jet_compatible, k_best_paths, materialize, GrammarEdge, GrammarPath,
    JET_CLASSES, K_DISCRETE_PATHS,
};
pub use models::{
    canonical_cuts, fit_forced_boundary_models, k_best_boundary_models, models_at_cut,
    path_families, BoundaryModel, ForcedFitRefusal, ModelRun, DUPLICATE_EPSILON_PX,
};
pub use primitive::{
    apply_best_primitive, loop_primitive_hypotheses, LoopPrimitiveGeometry,
    LoopPrimitiveHypothesis, LoopPrimitiveKind,
};
pub use refit::{
    canonical_angle, g1_readings, ArcAnchor, G1Reading, Handle, RefitChain, RefitNode,
    RefitRefusal, RefitSegment, FEASIBLE_HALFWIDTHS,
};
pub use relation::{apply_accepted, relation_hypotheses, RelationHypothesis, RelationKind};
pub use schedule::{
    anchored_schedule, anchored_schedule_bound, hierarchical_schedule, FitBudget, Support,
    FIT_BUDGET_V1, MIN_SUPPORT_SAMPLES, SUPPORTS_PER_SAMPLE_BOUND,
};
pub use solve::{joint_constrained_refit, RefitOutcome, JOINT_REFIT_PASSES, MAX_JOINT_PARAMETERS};
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
    /// A normal that is not a unit vector.
    ///
    /// `BoundarySample::normal` is documented as a UNIT normal, and every
    /// quantity this crate computes along it — §14.4's `d_n` above all — reads
    /// its length as one. A zero normal passes a finiteness check and then
    /// makes `normal_deviation` return `None` at that sample for EVERY
    /// candidate, so every candidate whose support touches it is silently
    /// removed and the DAG loses a node. Measured: a chain whose first sample
    /// had a zero normal produced 1417 candidates, none of them starting at
    /// sample 0, and the k-best search returned no path at all with no refusal
    /// to explain it. A malformed input has to be refused where it is
    /// malformed, not where its consequences surface.
    ///
    /// The tolerance is on a value the producer DECLARES to be one, so it is a
    /// contract check rather than a geometric threshold.
    NonUnitNormal { sample: usize, length: f64 },
    /// A correlation length that is not strictly positive or not finite.
    ///
    /// **RT6-A2's closure, and the record of why it is here.** The guard above
    /// was F-0088's fix, and it was a fix by ADDRESS: it checked the fields the
    /// defect had used and not the CONTRACT the crate reads. The red team then
    /// drove the same silent-emptiness through the NEIGHBOURING field of the
    /// same struct: `corr_length_px = 0` made `independent_observations`
    /// return `None`, `build_edges` silently dropped every edge, and the run
    /// reported 1604 candidates, 0 paths, 0 models, refused [] — "no path, no
    /// refusal", F-0088's published formula, at zero lines of attack cost.
    /// This guard now covers every field of `BoundarySample` this crate reads:
    /// `p`, `normal`, `weight_ds`, `halfwidth`, `corr_length_px`.
    NonPositiveCorrLength { sample: usize, corr_length_px: f64 },
    /// A public single-cut request named a sample that does not exist.
    CutOutOfRange { cut: usize, samples: usize },
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
    /// with the Euclidean deviation at which it occurred — `None` when no
    /// candidate had a material sample to measure it on (RT6-A5: absence is
    /// not writable as `1.0 at 0.0 px`).
    ///
    /// This is the size of the correction C302 made: `1.0` would mean the
    /// Euclidean lower bound this crate used to integrate was §14.4's `d_n`
    /// all along, and anything above it is what that approximation was
    /// costing.
    pub worst_ratio: Option<cost::RatioReading>,
    /// Material samples over every candidate of the chain — the population
    /// `worst_ratio` stands on.
    pub material_samples: usize,
    /// §14.1's corner proposals for this chain, one per sample with a `±1`
    /// window. A PROPOSAL confidence, published and used to anchor the
    /// schedule; nothing here or downstream compares it against a threshold.
    pub corners: Vec<CornerProposal>,
    /// The samples the schedule anchored its ladder on: the strict local
    /// maxima of the saliency. Published because the schedule's size depends
    /// on this count and a reader must be able to check the bound.
    pub anchors: Vec<usize>,
}

/// How far apart two corner anchors must be, in samples.
///
/// `MIN_SUPPORT_SAMPLES - 1`, and it is DERIVED rather than picked: two anchors
/// closer together than one minimum support cannot both terminate a support
/// this crate is willing to judge, so a second anchor inside that window buys
/// no candidate. Change `MIN_SUPPORT_SAMPLES` and this moves with it.
pub const CORNER_ANCHOR_HALF_WINDOW: usize = MIN_SUPPORT_SAMPLES - 1;

/// How far `BoundarySample::normal` may be from unit length before the chain is
/// refused.
///
/// A contract check on a value its producer declares to be a unit vector, not a
/// geometric tolerance: `1e-9` is far above the rounding of a normalisation and
/// far below anything that would make `d_n` mean something different.
pub const UNIT_NORMAL_TOLERANCE: f64 = 1e-9;

/// **§28 M6 bullets 1 and 2 for one chain.**
///
/// Offers every family on every support of the schedule — the dyadic ladder of
/// §14.2 anchored at the corner proposals of §14.1 — refusing rather than
/// truncating when the budget would bind.
pub fn span_candidates(
    chain: &BoundaryChain,
    budget: &FitBudget,
) -> Result<ChainCandidates, FitRefusal> {
    let samples = &chain.samples;
    let n = samples.len();

    validate_chain(chain)?;

    let corners = corner_proposals(samples);
    let anchors = corner_anchors(&corners, CORNER_ANCHOR_HALF_WINDOW);
    let supports = anchored_schedule(n, &anchors);
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
    let mut worst_ratio: Option<cost::RatioReading> = None;
    let mut material_samples = 0usize;

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
            material_samples += cost.material_samples;
            if let Some(r) = cost.worst_ratio {
                if worst_ratio.is_none_or(|w| r.ratio > w.ratio) {
                    worst_ratio = Some(r);
                }
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
        worst_ratio,
        material_samples,
        corners,
        anchors,
    })
}

/// Validate the complete public input contract before either automatic or
/// oracle-forced candidate generation. Keeping one guard is important: G20
/// must not reopen RT6-A2 merely because it bypasses the automatic schedule.
pub(crate) fn validate_chain(chain: &BoundaryChain) -> Result<(), FitRefusal> {
    let samples = &chain.samples;
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
        let len = s.normal.length();
        if (len - 1.0).abs() > UNIT_NORMAL_TOLERANCE {
            return Err(FitRefusal::NonUnitNormal {
                sample: i,
                length: len,
            });
        }
        if !(s.corr_length_px.is_finite() && s.corr_length_px > 0.0) {
            return Err(FitRefusal::NonPositiveCorrLength {
                sample: i,
                corr_length_px: s.corr_length_px,
            });
        }
    }

    if samples.len() < MIN_SUPPORT_SAMPLES {
        return Err(FitRefusal::ChainTooShort {
            samples: samples.len(),
            minimum: MIN_SUPPORT_SAMPLES,
        });
    }
    Ok(())
}

fn bump<T: PartialEq>(acc: &mut Vec<(&'static str, T, usize)>, family: &'static str, why: T) {
    if let Some(e) = acc.iter_mut().find(|e| e.0 == family && e.1 == why) {
        e.2 += 1;
    } else {
        acc.push((family, why, 1));
    }
}
