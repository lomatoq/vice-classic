//! §24's `k_best_boundary_models`, assembled: candidates, DAG, k-shortest
//! paths, joint refit per path, and the ranking.
//!
//! ## §24's pseudocode, line by line, and where each line went
//!
//! ```text
//! samples  = physical_resample(obs, cfg.sample_step_px)   -> M4, already done
//! breaks   = hierarchical_breakpoint_candidates(...)      -> schedule::anchored_schedule
//! corners  = corner_proposals(...)                        -> corner::corner_proposals
//! spans    = generate_hierarchical_span_candidates(...)   -> span_candidates
//! dag      = build_jet_compatible_grammar_dag(...)        -> grammar::build_edges
//! paths    = k_shortest_paths(dag, cfg.k_discrete_paths)  -> grammar::k_best_paths
//! for path: joint_constrained_refit + exact_g1_and_local_isotopy
//! rank_by_proposal_integral_and_code_length
//! ```
//!
//! `exact_g1_and_local_isotopy` splits in two and only one half is here.
//! **Exact G1 is held by the representation** and is re-measured on every
//! accepted model ([`BoundaryModel::worst_g1_spread_rad`]) rather than assumed.
//! **Local isotopy is NOT checked**, because Stage G's chains are contours of
//! the coverage field and carry no DCEL identity —
//! `vice_topology::curve_replacement_isotopy` still names `fitted_curve` as its
//! first missing capability, and inventing an identity here to satisfy the line
//! would be the fabricated binding STATUS_M6 limitation 57 already prices.
//! That is the honest half of the line and it is stated, not omitted.
//!
//! ## Closed chains
//!
//! §14.3: "Closed loops решаются cyclic k-best search либо несколькими
//! canonical cuts с доказанным cut-invariance test." The second is taken.
//! [`k_best_boundary_models`] cuts a closed chain at each of a structurally
//! derived set of points — a cyclicly canonical root plus persistent-turning
//! corner anchors — solves each cut independently, and returns the best. The
//! cut-invariance TEST is
//! `the_cut_a_closed_chain_is_opened_at_does_not_change_what_is_selected`, and
//! the spread it measures is published rather than asserted to be zero.

use std::cmp::Ordering;

use serde::Serialize;
use vice_evidence::{BoundaryChain, BoundarySample};

use crate::code::{ChainCode, GeometryCodeTable};
use crate::grammar::{
    build_edges, k_best_paths_with_closure, materialize_with_closure, path_is_representable,
    GrammarPath,
};
use crate::refit::{closure_g1_spread_rad, g1_readings, RefitChain, RefitRefusal};
use crate::schedule::{FitBudget, Support};
use crate::solve::joint_constrained_refit;
use crate::span::{NoFit, SpanCandidate, SpanFamily};
use crate::{span_candidates, FitRefusal};

mod closed;
use closed::rotate;
pub use closed::{canonical_cuts, dedup_coincident, DUPLICATE_EPSILON_PX, MAX_CANONICAL_CUTS};

/// The geometry selected by Stage H.
///
/// There is deliberately no parallel free-chain field. A relation winner is
/// the constrained chain itself, and a primitive winner is the canonical
/// primitive itself; downstream code cannot flatten the losing sibling while
/// charging the winner's shorter code.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "selected_geometry", rename_all = "snake_case")]
pub enum SelectedBoundaryGeometry {
    TypedChain {
        chain: RefitChain,
    },
    LoopPrimitive {
        kind: crate::primitive::LoopPrimitiveKind,
        geometry: crate::primitive::LoopPrimitiveGeometry,
        /// The polyline used for the Stage-H residual/corridor comparison.
        /// Export uses the canonical parameters; this preserves the witness
        /// that was actually judged.
        verification_polyline: Vec<vice_geom::Pt>,
    },
}

impl SelectedBoundaryGeometry {
    pub fn typed_chain(&self) -> Option<&RefitChain> {
        match self {
            SelectedBoundaryGeometry::TypedChain { chain } => Some(chain),
            SelectedBoundaryGeometry::LoopPrimitive { .. } => None,
        }
    }

    pub fn flatten(&self) -> Result<Vec<vice_geom::Pt>, RefitRefusal> {
        match self {
            SelectedBoundaryGeometry::TypedChain { chain } => crate::solve::flatten_chain(chain),
            SelectedBoundaryGeometry::LoopPrimitive {
                verification_polyline,
                ..
            } if verification_polyline.len() >= 2 => Ok(verification_polyline.clone()),
            SelectedBoundaryGeometry::LoopPrimitive { .. } => Err(RefitRefusal::Malformed),
        }
    }
}

/// One accepted boundary model: a discrete grammar path that survived the joint
/// solve, with everything a ranking or a gate would need.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BoundaryModel {
    pub geometry: SelectedBoundaryGeometry,
    pub families: Vec<SpanFamily>,
    pub breakpoints: Vec<usize>,
    pub smooth: Vec<bool>,
    /// The implicit join at the canonical seam of a closed chain.
    pub closure_smooth: bool,
    /// The code length of the DISCRETE path, from the DP.
    pub code: ChainCode,
    /// §14.4's integral, carried separately and never summed into `code`.
    pub proposal_cost_px: f64,
    /// Worst G1 spread over the model's smooth nodes after lowering, radians.
    /// Exactly what `vice-ir`'s canonical fixture reads 0.4949 rad on.
    pub worst_g1_spread_rad: f64,
    /// Worst `|d_n|` after the solve, px.
    pub worst_normal_deviation_px: f64,
    /// Worst distance in the reverse model-to-evidence direction, px.
    pub worst_model_to_evidence_px: f64,
    pub residual_before: f64,
    pub residual_after: f64,
    /// §15 whole-loop constrained siblings, accepted and rejected alike.
    /// Empty on an open chain.
    pub primitives: Vec<crate::primitive::LoopPrimitiveHypothesis>,
    /// Index into `primitives` when a whole-loop primitive beat both the free
    /// chain and the best composable relation sibling.
    pub primitive_kept: Option<usize>,
    /// §15 Stage H: every relation hypothesis formed on this model, ACCEPTED
    /// and rejected alike. A list of accepted relations alone says nothing
    /// about how many were considered, and §15's comparison against the
    /// unconstrained sibling is only meaningful when the losing side is
    /// visible.
    pub relations: Vec<crate::relation::RelationHypothesis>,
    /// How many of them were accepted, and are therefore folded into `code`.
    pub relations_kept: usize,
    /// Exact indices into `relations`; a count alone cannot identify which
    /// constrained sibling won.
    pub relation_kept_indices: Vec<usize>,
}

/// What one call to [`k_best_boundary_models`] did, including the paths it
/// threw away.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModelRun {
    pub models: Vec<BoundaryModel>,
    /// Canonical openings actually evaluated (one for an open or forced path).
    pub cuts_evaluated: usize,
    /// Candidates generated across every evaluated cut, bounded per physical
    /// chain rather than independently per representation.
    pub candidates: usize,
    pub edges: usize,
    pub discrete_paths: usize,
    /// Paths the joint solve refused, by reason. §24 exists because this is not
    /// zero; a run where it is always zero would mean `k` is doing nothing.
    pub refused: Vec<(&'static str, usize)>,
    /// Paths dropped before the solver because the representation cannot carry
    /// them (a quadratic smooth at both ends, or a smooth join between two
    /// lines).
    pub not_representable: usize,
    /// §15 relation hypotheses formed over every accepted model, and how many
    /// were accepted. Both, because an acceptance rate is the number that says
    /// whether Stage H is deciding anything.
    pub relations_considered: usize,
    pub relations_accepted: usize,
    pub primitives_considered: usize,
    pub primitives_accepted: usize,
}

/// Judge the primitive and relation constrained siblings against the same free
/// model, then keep exactly the shortest sibling.  This is the §15 comparison
/// and prevents a circle, for example, from also claiming equal-radius savings
/// that its own parameterization already contains.
fn apply_stage_h(
    model: &mut BoundaryModel,
    samples: &[BoundarySample],
    table: &GeometryCodeTable,
    canvas_dim_px: f64,
    closed: bool,
) {
    let relations =
        crate::relation::relation_hypotheses(model, samples, table, canvas_dim_px, closed);
    let primitives =
        crate::primitive::loop_primitive_hypotheses(model, samples, table, canvas_dim_px, closed);

    let mut relation_sibling = model.clone();
    let relations_kept = crate::relation::apply_accepted(&mut relation_sibling, &relations, closed);
    let mut primitive_sibling = model.clone();
    let primitive_kept =
        crate::primitive::apply_best_primitive(&mut primitive_sibling, &primitives);

    if primitive_kept.is_some()
        && primitive_sibling.code.total_bits() < relation_sibling.code.total_bits()
    {
        model.geometry = primitive_sibling.geometry;
        model.code = primitive_sibling.code;
        model.primitive_kept = primitive_kept;
        model.relations_kept = 0;
        model.relation_kept_indices.clear();
    } else {
        model.geometry = relation_sibling.geometry;
        model.code = relation_sibling.code;
        model.primitive_kept = None;
        model.relations_kept = relations_kept;
        model.relation_kept_indices = relation_sibling.relation_kept_indices;
    }
    model.primitives = primitives;
    model.relations = relations;
}

/// Why an oracle-forced discrete path could not be fitted.
///
/// G20 fixes only the GT-equivalent family sequence and breakpoints. It still
/// uses the production proposal fits, joint solver, physical code and
/// relation pass. Every failure is typed because "the oracle arm produced no
/// model" is otherwise indistinguishable from a conveniently empty knockout.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "forced_fit_refusal", rename_all = "snake_case")]
pub enum ForcedFitRefusal {
    Input {
        refusal: FitRefusal,
    },
    ShapeMismatch {
        families: usize,
        breakpoints: usize,
    },
    BreakpointOutOfRange {
        breakpoint: usize,
        previous: usize,
        last_sample: usize,
    },
    FamilyNoFit {
        span: usize,
        family: SpanFamily,
        refusal: NoFit,
    },
    CostRefused {
        span: usize,
        family: SpanFamily,
        refusal: crate::CostRefusal,
    },
    EdgeMissing {
        span: usize,
        family: SpanFamily,
    },
    NoPath,
    NoAcceptedModel {
        paths: usize,
        not_representable: usize,
        refused: Vec<(&'static str, usize)>,
    },
}

fn refusal_name(r: &RefitRefusal) -> &'static str {
    match r {
        RefitRefusal::Input { .. } => "invalid_input",
        RefitRefusal::Malformed => "malformed",
        RefitRefusal::DegenerateSpan { .. } => "degenerate_span",
        RefitRefusal::ArcIsALine { .. } => "arc_is_a_line",
        RefitRefusal::NonFinite { .. } => "non_finite",
        RefitRefusal::NonPositiveSharedHandle { .. } => "non_positive_shared_handle",
        RefitRefusal::G1Violation { .. } => "g1_violation",
        RefitRefusal::OutsideCorridor { .. } => "outside_corridor",
        RefitRefusal::SmoothJoinBetweenTwoLines { .. } => "smooth_join_between_two_lines",
        RefitRefusal::SmoothNodeUnread { .. } => "smooth_node_unread",
        RefitRefusal::TooManyParameters { .. } => "too_many_parameters",
    }
}

/// **§28 M6 bullets 3 and 4 for one chain**, in §24's order.
///
/// `canvas_dim_px` is the code's coordinate range and must be the image's, not
/// a default: it moves every anchor's cost by `2 log2` of the ratio.
pub fn k_best_boundary_models(
    chain: &BoundaryChain,
    budget: &FitBudget,
    canvas_dim_px: f64,
    k: usize,
) -> Result<ModelRun, FitRefusal> {
    crate::validate_canvas_dimension(canvas_dim_px)?;
    k_best_boundary_models_with_table(
        chain,
        budget,
        &crate::GEOMETRY_CODE_TABLE_V1,
        canvas_dim_px,
        k,
    )
}

/// Fit models while forcing the GT-equivalent family sequence and breakpoints.
///
/// This is the G20 injection point from §27.6. It deliberately does **not**
/// accept segments, parameters, joins, a code table or a selector:
///
/// - each forced span is fitted from the same observations as production;
/// - corner/smooth remains an automatic discrete choice subject to the same
///   jet compatibility and representability rules;
/// - parameters come from the same joint constrained solve;
/// - code and relations use the frozen production table.
///
/// Thus the arm removes discrete family/breakpoint search and nothing else.
pub fn fit_forced_boundary_models(
    chain: &BoundaryChain,
    families: &[SpanFamily],
    breakpoints: &[usize],
    canvas_dim_px: f64,
    k: usize,
) -> Result<ModelRun, ForcedFitRefusal> {
    crate::validate_chain(chain).map_err(|refusal| ForcedFitRefusal::Input { refusal })?;
    crate::validate_canvas_dimension(canvas_dim_px)
        .map_err(|refusal| ForcedFitRefusal::Input { refusal })?;
    let closed = chain.closed;
    let base_chain = dedup_coincident(chain);
    // The forced breakpoints are expressed from GT's canonical start. Open
    // that same physical loop exactly once; unlike G00 there is no cut search
    // to perform, but Stage H and the seam still need to know it is a loop.
    let opened_chain = if closed {
        rotate(&base_chain, 0)
    } else {
        base_chain
    };
    if families.len() != breakpoints.len() + 1 {
        return Err(ForcedFitRefusal::ShapeMismatch {
            families: families.len(),
            breakpoints: breakpoints.len(),
        });
    }
    let samples = &opened_chain.samples;
    let last = samples.len() - 1;
    let mut previous = 0usize;
    for &breakpoint in breakpoints {
        if breakpoint <= previous || breakpoint >= last {
            return Err(ForcedFitRefusal::BreakpointOutOfRange {
                breakpoint,
                previous,
                last_sample: last,
            });
        }
        previous = breakpoint;
    }

    let mut bounds = Vec::with_capacity(breakpoints.len() + 2);
    bounds.push(0usize);
    bounds.extend_from_slice(breakpoints);
    bounds.push(last);

    let mut candidates = Vec::with_capacity(families.len());
    for (span, (window, &family)) in bounds.windows(2).zip(families).enumerate() {
        let support =
            Support::new(window[0], window[1]).ok_or(ForcedFitRefusal::BreakpointOutOfRange {
                breakpoint: window[1],
                previous: window[0],
                last_sample: last,
            })?;
        let segment = crate::fit(samples, support, family).map_err(|refusal| {
            ForcedFitRefusal::FamilyNoFit {
                span,
                family,
                refusal,
            }
        })?;
        let cost = crate::proposal_cost(samples, support, &segment).map_err(|refusal| {
            ForcedFitRefusal::CostRefused {
                span,
                family,
                refusal,
            }
        })?;
        candidates.push(SpanCandidate {
            support,
            family,
            segment,
            cost,
        });
    }

    let table = &crate::GEOMETRY_CODE_TABLE_V1;
    let edges = build_edges(&candidates, samples, table, canvas_dim_px)
        .map_err(|refusal| ForcedFitRefusal::Input { refusal })?;
    if edges.len() != candidates.len() {
        let missing = candidates
            .iter()
            .enumerate()
            .find(|(candidate, _)| !edges.iter().any(|e| e.candidate == *candidate))
            .expect("edge count differs, so a candidate is absent");
        return Err(ForcedFitRefusal::EdgeMissing {
            span: missing.0,
            family: missing.1.family,
        });
    }
    let paths = k_best_paths_with_closure(&edges, samples, table, canvas_dim_px, k, closed)
        .map_err(|refusal| ForcedFitRefusal::Input { refusal })?;
    if paths.is_empty() {
        return Err(ForcedFitRefusal::NoPath);
    }

    let mut models = Vec::new();
    let mut refused = Vec::new();
    let mut not_representable = 0usize;
    for path in &paths {
        let closure_smooth = path.closure_smooth;
        if !path_is_representable(path, families) {
            not_representable += 1;
            continue;
        }
        let Some(init) = materialize_with_closure(path, &edges, &candidates, samples) else {
            not_representable += 1;
            continue;
        };
        let closure_is_represented = init.has_closed_tangent_alias();
        match joint_constrained_refit(&init, samples) {
            Ok(out) => {
                let Ok(lowered) = out.chain.lower() else {
                    bump(&mut refused, "malformed");
                    continue;
                };
                let mut worst_g1 = g1_readings(&lowered, out.chain.start(), out.chain.end())
                    .iter()
                    .map(|r| r.spread_rad)
                    .fold(0.0f64, f64::max);
                if closure_smooth && closure_is_represented {
                    let Some(declared) = out.chain.nodes[0].tangent_rad else {
                        bump(&mut refused, "closure_tangent_missing");
                        continue;
                    };
                    let Some(spread) = closure_g1_spread_rad(
                        &lowered,
                        out.chain.start(),
                        out.chain.end(),
                        declared,
                    ) else {
                        bump(&mut refused, "closure_g1_unread");
                        continue;
                    };
                    worst_g1 = worst_g1.max(spread);
                }
                let mut code = path.code;
                code.residual_bits = crate::code::chain_residual_bits(&out.chain, samples, table);
                if !code.residual_bits.is_finite() {
                    bump(&mut refused, "non_finite_post_refit_code");
                    continue;
                }
                let mut model = BoundaryModel {
                    geometry: SelectedBoundaryGeometry::TypedChain { chain: out.chain },
                    families: families.to_vec(),
                    breakpoints: path.breakpoints.clone(),
                    smooth: path.smooth.clone(),
                    closure_smooth,
                    code,
                    proposal_cost_px: path.proposal_cost_px,
                    worst_g1_spread_rad: worst_g1,
                    worst_normal_deviation_px: out.worst_normal_deviation_px,
                    worst_model_to_evidence_px: out.worst_model_to_evidence_px,
                    residual_before: out.residual_before,
                    residual_after: out.residual_after,
                    primitives: Vec::new(),
                    primitive_kept: None,
                    relations: Vec::new(),
                    relations_kept: 0,
                    relation_kept_indices: Vec::new(),
                };
                apply_stage_h(&mut model, samples, table, canvas_dim_px, closed);
                if closure_smooth
                    && !closure_is_represented
                    && matches!(model.geometry, SelectedBoundaryGeometry::TypedChain { .. })
                {
                    bump(&mut refused, "smooth_closure_unrepresented");
                    continue;
                }
                models.push(model);
            }
            Err(why) => bump(&mut refused, refusal_name(&why)),
        }
    }
    if models.is_empty() {
        return Err(ForcedFitRefusal::NoAcceptedModel {
            paths: paths.len(),
            not_representable,
            refused,
        });
    }
    models.sort_by(compare_model_rank);
    refused.sort_unstable();
    let relations_considered = models.iter().map(|m| m.relations.len()).sum();
    let relations_accepted = models.iter().map(|m| m.relations_kept).sum();
    let primitives_considered = models.iter().map(|m| m.primitives.len()).sum();
    let primitives_accepted = models.iter().filter(|m| m.primitive_kept.is_some()).count();
    Ok(ModelRun {
        relations_considered,
        relations_accepted,
        primitives_considered,
        primitives_accepted,
        models,
        cuts_evaluated: 1,
        candidates: candidates.len(),
        edges: edges.len(),
        discrete_paths: paths.len(),
        refused,
        not_representable,
    })
}

/// Internal injection point for the no-BIC knockout. Production callers cannot
/// replace the frozen table with a feature-local one (M6B-N5).
fn k_best_boundary_models_with_table(
    chain: &BoundaryChain,
    budget: &FitBudget,
    table: &GeometryCodeTable,
    canvas_dim_px: f64,
    k: usize,
) -> Result<ModelRun, FitRefusal> {
    // M6B-N3: an empty CLOSED chain reached `rotate`, which indexes
    // `samples[cut]`, and PANICKED — while every other degenerate input is
    // refused by name. The refusal has to precede `canonical_cuts`/`rotate`,
    // which is F-0088's rule: an input is refused where it is malformed, not
    // where its consequences surface. `< MIN_SUPPORT_SAMPLES` rather than
    // `is_empty`, because `span_candidates` would refuse those lengths anyway
    // and a rotate on a two-sample closed chain has nothing to select over.
    if chain.samples.len() < crate::MIN_SUPPORT_SAMPLES {
        return Err(FitRefusal::ChainTooShort {
            samples: chain.samples.len(),
            minimum: crate::MIN_SUPPORT_SAMPLES,
        });
    }
    let chain = &dedup_coincident(chain);
    let cuts = if chain.closed {
        canonical_cuts(chain)
    } else {
        vec![0usize]
    };
    let mut best: Option<ModelRun> = None;
    let mut candidates_across_cuts = 0usize;
    let mut cuts_evaluated = 0usize;
    for cut in cuts {
        let rotated = rotate(chain, cut);
        let run = models_for_open_chain(&rotated, budget, table, canvas_dim_px, k)?;
        cuts_evaluated += 1;
        candidates_across_cuts = candidates_across_cuts.saturating_add(run.candidates);
        if candidates_across_cuts > budget.cap() {
            return Err(FitRefusal::CutSearchBudgetExceeded {
                cuts_evaluated,
                candidates: candidates_across_cuts,
                cap: budget.cap(),
            });
        }
        let better = match &best {
            None => true,
            Some(b) => match (b.models.first(), run.models.first()) {
                (_, None) => false,
                (None, Some(_)) => true,
                (Some(x), Some(y)) => compare_model_rank(y, x).is_lt(),
            },
        };
        if better {
            best = Some(run);
        }
    }
    let mut best = best.ok_or(FitRefusal::ChainTooShort {
        samples: chain.samples.len(),
        minimum: crate::MIN_SUPPORT_SAMPLES,
    })?;
    best.cuts_evaluated = cuts_evaluated;
    best.candidates = candidates_across_cuts;
    Ok(best)
}

/// The models for ONE canonical cut, without re-cutting.
///
/// Exposed because the cut-invariance test needs to measure what a SINGLE cut
/// selects: calling the whole pipeline on a rotated chain re-cuts it and
/// measures the pipeline again, which is the property under test rather than
/// the thing that makes it non-trivial.
pub fn models_at_cut(
    chain: &BoundaryChain,
    cut: usize,
    budget: &FitBudget,
    canvas_dim_px: f64,
    k: usize,
) -> Result<ModelRun, FitRefusal> {
    crate::validate_chain(chain)?;
    crate::validate_canvas_dimension(canvas_dim_px)?;
    let chain = dedup_coincident(chain);
    if cut >= chain.samples.len() {
        return Err(FitRefusal::CutOutOfRange {
            cut,
            samples: chain.samples.len(),
        });
    }
    let rotated = rotate(&chain, cut);
    models_for_open_chain(
        &rotated,
        budget,
        &crate::GEOMETRY_CODE_TABLE_V1,
        canvas_dim_px,
        k,
    )
}

fn models_for_open_chain(
    chain: &BoundaryChain,
    budget: &FitBudget,
    table: &GeometryCodeTable,
    canvas_dim_px: f64,
    k: usize,
) -> Result<ModelRun, FitRefusal> {
    let cands = span_candidates(chain, budget)?;
    let samples = &chain.samples;
    let edges = build_edges(&cands.candidates, samples, table, canvas_dim_px)?;
    let paths = k_best_paths_with_closure(&edges, samples, table, canvas_dim_px, k, chain.closed)?;

    let mut models = Vec::new();
    let mut refused: Vec<(&'static str, usize)> = Vec::new();
    let mut not_representable = 0usize;

    for path in &paths {
        let closure_smooth = path.closure_smooth;
        let families: Vec<SpanFamily> = path
            .candidates
            .iter()
            .map(|c| cands.candidates[*c].family)
            .collect();
        if !path_is_representable(path, &families) {
            not_representable += 1;
            continue;
        }
        let Some(init) = materialize_with_closure(path, &edges, &cands.candidates, samples) else {
            not_representable += 1;
            continue;
        };
        let closure_is_represented = init.has_closed_tangent_alias();
        match joint_constrained_refit(&init, samples) {
            Ok(out) => {
                let Ok(lowered) = out.chain.lower() else {
                    bump(&mut refused, "malformed");
                    continue;
                };
                let mut worst_g1 = g1_readings(&lowered, out.chain.start(), out.chain.end())
                    .iter()
                    .map(|r| r.spread_rad)
                    .fold(0.0f64, f64::max);
                if closure_smooth && closure_is_represented {
                    let Some(declared) = out.chain.nodes[0].tangent_rad else {
                        bump(&mut refused, "closure_tangent_missing");
                        continue;
                    };
                    let Some(spread) = closure_g1_spread_rad(
                        &lowered,
                        out.chain.start(),
                        out.chain.end(),
                        declared,
                    ) else {
                        bump(&mut refused, "closure_g1_unread");
                        continue;
                    };
                    worst_g1 = worst_g1.max(spread);
                }
                let mut code = path.code;
                code.residual_bits = crate::code::chain_residual_bits(&out.chain, samples, table);
                if !code.residual_bits.is_finite() {
                    bump(&mut refused, "non_finite_post_refit_code");
                    continue;
                }
                let mut m = BoundaryModel {
                    geometry: SelectedBoundaryGeometry::TypedChain { chain: out.chain },
                    families,
                    breakpoints: path.breakpoints.clone(),
                    smooth: path.smooth.clone(),
                    closure_smooth,
                    code,
                    proposal_cost_px: path.proposal_cost_px,
                    worst_g1_spread_rad: worst_g1,
                    worst_normal_deviation_px: out.worst_normal_deviation_px,
                    worst_model_to_evidence_px: out.worst_model_to_evidence_px,
                    residual_before: out.residual_before,
                    residual_after: out.residual_after,
                    primitives: Vec::new(),
                    primitive_kept: None,
                    relations: Vec::new(),
                    relations_kept: 0,
                    relation_kept_indices: Vec::new(),
                };
                apply_stage_h(&mut m, samples, table, canvas_dim_px, chain.closed);
                if closure_smooth
                    && !closure_is_represented
                    && matches!(m.geometry, SelectedBoundaryGeometry::TypedChain { .. })
                {
                    bump(&mut refused, "smooth_closure_unrepresented");
                    continue;
                }
                models.push(m);
            }
            Err(why) => bump(&mut refused, refusal_name(&why)),
        }
    }

    // §24's `rank_by_proposal_integral_and_code_length`: code length first
    // because it is the physical-bit score §14.5 names as the selector, the
    // §14.4 integral as the tie-break, because §14.4 says it ORDERS candidates
    // and does not select among final models.
    models.sort_by(compare_model_rank);
    refused.sort_unstable();

    let relations_considered = models.iter().map(|m| m.relations.len()).sum();
    let relations_accepted = models.iter().map(|m| m.relations_kept).sum();
    let primitives_considered = models.iter().map(|m| m.primitives.len()).sum();
    let primitives_accepted = models.iter().filter(|m| m.primitive_kept.is_some()).count();
    Ok(ModelRun {
        relations_considered,
        relations_accepted,
        primitives_considered,
        primitives_accepted,
        models,
        cuts_evaluated: 1,
        candidates: cands.candidates.len(),
        edges: edges.len(),
        discrete_paths: paths.len(),
        refused,
        not_representable,
    })
}

/// §24's declared ordering, in one function the knockout test can exercise.
///
/// The physical-bit code is the final chain selector. The §14.4 proposal
/// integral is the deterministic order within an exact code tie; it is not a
/// second likelihood. Keeping both arguments here makes deleting the proposal
/// leg mechanically red instead of leaving `rank_by_proposal_integral...`
/// guarded only by prose (RT6-A4).
fn compare_rank_values(a_code: f64, a_proposal: f64, b_code: f64, b_proposal: f64) -> Ordering {
    a_code
        .total_cmp(&b_code)
        .then(a_proposal.total_cmp(&b_proposal))
}

fn compare_model_rank(a: &BoundaryModel, b: &BoundaryModel) -> Ordering {
    compare_rank_values(
        a.code.total_bits(),
        a.proposal_cost_px,
        b.code.total_bits(),
        b.proposal_cost_px,
    )
}

fn bump(acc: &mut Vec<(&'static str, usize)>, name: &'static str) {
    match acc.iter_mut().find(|e| e.0 == name) {
        Some(e) => e.1 += 1,
        None => acc.push((name, 1)),
    }
}

/// The families of a path, for callers that want the discrete answer without
/// the solve.
pub fn path_families(path: &GrammarPath, candidates: &[crate::SpanCandidate]) -> Vec<SpanFamily> {
    path.candidates
        .iter()
        .map(|c| candidates[*c].family)
        .collect()
}

#[cfg(test)]
mod ranking_tests;
