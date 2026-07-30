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
use std::collections::BTreeSet;
use std::sync::Arc;

use serde::Serialize;
use vice_evidence::{BoundaryChain, BoundarySample};

use crate::code::{ChainCode, GeometryCodeTable};
use crate::grammar::{
    build_edges, k_best_paths_with_closure, materialize_with_closure, path_is_representable,
    GrammarPath,
};
use crate::refit::{closure_g1_spread_rad, g1_readings, RefitChain, RefitRefusal};
use crate::schedule::{FitBudget, Support};
use crate::solve::{joint_constrained_refit, joint_constrained_refit_bounded};
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
    /// Unconstrained sibling before Stage H. M7 compares it against every
    /// admissible constrained delivery under the final serialized likelihood.
    pub stage_h_free_geometry: SelectedBoundaryGeometry,
    pub stage_h_free_code: ChainCode,
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
    /// Shared because this immutable diagnostic catalog is identical across
    /// every M7 variant cloned from the model.
    pub primitives: Arc<Vec<crate::primitive::LoopPrimitiveHypothesis>>,
    /// Index into `primitives` when a whole-loop primitive beat both the free
    /// chain and the best composable relation sibling.
    pub primitive_kept: Option<usize>,
    /// §15 Stage H: every relation hypothesis formed on this model, ACCEPTED
    /// and rejected alike. A list of accepted relations alone says nothing
    /// about how many were considered, and §15's comparison against the
    /// unconstrained sibling is only meaningful when the losing side is
    /// visible.
    /// Shared because constrained variants change only the selected geometry,
    /// code, and kept indices, not the catalog used to derive them.
    pub relations: Arc<Vec<crate::relation::RelationHypothesis>>,
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
    /// Physical observations available to the final constrained solve.
    pub observed_samples: usize,
    /// Observations used only to propose discrete family/breakpoint paths.
    /// A smaller value is explicit search truncation, never a claim that the
    /// omitted observations vanished from the likelihood.
    pub discrete_search_samples: usize,
    /// Deterministic observation counts at which discrete paths were proposed.
    /// More than one level is a finite hierarchical search, not repeated
    /// evidence.
    pub discrete_search_levels: Vec<usize>,
    /// Observations used to form the numerical Jacobian for the continuous
    /// parameter solve. The physical code and final corridor certification are
    /// not truncated with this count.
    pub continuous_solve_samples: usize,
    /// Whether the continuous Jacobian itself used every observation.
    pub full_resolution_refit: bool,
    /// Every published model was physically coded, Stage-H compared and
    /// checked in both corridor directions against all `observed_samples`.
    pub full_resolution_certified: bool,
    /// Opening chosen by canonical-cut search, in the observation order.
    pub selected_cut: usize,
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
    /// Candidate proposal costs refused before the grammar search, counted by
    /// family and stable refusal class. These are deliberately separate from
    /// joint-refit refusals above: M7-33 must show whether a family was absent
    /// because it could not be fitted or because its normal-direction cost was
    /// numerically undefined.
    pub cost_refusals: Vec<(&'static str, &'static str, usize)>,
    /// Largest measured normal-to-Euclidean deviation ratio over proposal
    /// candidates in this run. The accompanying deviation prevents a large
    /// ratio at rounding scale from looking like a material conditioning
    /// failure.
    pub worst_normal_to_euclidean_ratio: Option<crate::RatioReading>,
    /// Population supporting the ratio diagnostic above.
    pub material_cost_samples: usize,
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
    /// Discrete proposal paths that could not survive the full-resolution
    /// forced-path construction, physical code and corridor certification.
    /// Non-zero is observable unexplored search mass.
    pub full_certification_refusals: usize,
    /// Proposal paths not certified after the first bounded level produced the
    /// maximum retained models for this physical chain.
    pub resource_pruned_proposals: usize,
    /// Finer deterministic proposal levels not opened after a coarser level
    /// produced a fully certified model.
    pub proposal_levels_skipped_after_certification: usize,
}

/// Frozen M7 maximum level for discrete path proposal.
///
/// The complete adaptive level schedule is recorded in `ModelRun`. Every
/// retained path is recoded and certified on the complete observation chain.
pub const DISCRETE_PROPOSAL_SAMPLE_CAP_V1: usize = 128;

/// Frozen M7 resource envelope for the numerical Jacobian.
///
/// The path, code, Stage-H siblings and two-sided corridor certificate still
/// use every observation. Only the repeated finite-difference solve is bounded.
pub const CONTINUOUS_REFIT_SAMPLE_CAP_V1: usize = 128;

/// Frozen Jacobian cap for each Stage-H relation sibling. Final residual,
/// physical code and both corridor directions remain full-resolution.
pub const RELATION_REFIT_SAMPLE_CAP_V1: usize = 16;

/// Jacobian cap while a decimated chain is used only to propose a discrete
/// path. The proposal's corridor is still certified on every decimated sample.
pub const PROPOSAL_CONTINUOUS_REFIT_SAMPLE_CAP_V1: usize = 64;

/// Maximum fully certified path siblings retained for one physical chain in
/// the bounded M7 search. The grammar still enumerates frozen `k`; omitted
/// paths are published as search truncation.
pub const MAX_CERTIFIED_MODELS_PER_CHAIN_V1: usize = 2;

/// Per-level certification work before the next deterministic proposal
/// resolution is opened.
pub const MAX_CERTIFICATION_ATTEMPTS_PER_LEVEL_V1: usize = 3;

/// The production verifier's frozen boundary tessellation error. Bounded fit
/// uses the same value to reject paths that cannot enter the observed binding
/// tube before expensive Stage-H sibling formation.
pub const BINDING_CERTIFICATION_CHORD_TOLERANCE_PX_V1: f64 = 1.0 / 64.0;

/// One frozen fitter chord certificate around the observed binding tube.
/// It covers the bounded-refit tessellation comparison and is carried into
/// the verifier binding; larger misses open the next discrete proposal level.
pub const BINDING_RELATION_RESCUE_MARGIN_PX_V1: f64 = crate::solve::REFIT_CHORD_TOLERANCE_PX;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "bounded_fit_refusal", rename_all = "snake_case")]
pub enum BoundedFitRefusal {
    Proposal {
        refusal: FitRefusal,
    },
    NoProposalModels {
        levels: Vec<usize>,
        candidates: usize,
        discrete_paths: usize,
        refused: Vec<(&'static str, usize)>,
        not_representable: usize,
    },
    AllFullResolutionCertificationsRefused {
        proposals: usize,
        refusals: Vec<ForcedFitRefusal>,
    },
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
    continuous_sample_cap: Option<usize>,
) {
    let relations = match continuous_sample_cap {
        Some(cap) => crate::relation::relation_hypotheses_bounded(
            model,
            samples,
            table,
            canvas_dim_px,
            closed,
            cap.min(RELATION_REFIT_SAMPLE_CAP_V1),
        ),
        None => crate::relation::relation_hypotheses(model, samples, table, canvas_dim_px, closed),
    };
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
    model.primitives = Arc::new(primitives);
    model.relations = Arc::new(relations);
    model.worst_g1_spread_rad =
        selected_worst_g1_spread(&model.geometry, model.closure_smooth).unwrap_or(f64::INFINITY);
}

fn selected_worst_g1_spread(
    geometry: &SelectedBoundaryGeometry,
    closure_smooth: bool,
) -> Option<f64> {
    let SelectedBoundaryGeometry::TypedChain { chain } = geometry else {
        // Canonical whole-loop primitives carry no composable smooth joins.
        return Some(0.0);
    };
    let lowered = chain.lower().ok()?;
    let mut worst = g1_readings(&lowered, chain.start(), chain.end())
        .iter()
        .map(|reading| reading.spread_rad)
        .fold(0.0f64, f64::max);
    if closure_smooth && chain.has_closed_tangent_alias() {
        let declared = chain.nodes.first()?.tangent_rad?;
        worst = worst.max(closure_g1_spread_rad(
            &lowered,
            chain.start(),
            chain.end(),
            declared,
        )?);
    }
    Some(worst)
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
    BindingIsotopy {
        displacement_px: f64,
        allowed_px: f64,
    },
    NoAcceptedModel {
        paths: usize,
        not_representable: usize,
        refused: Vec<(&'static str, usize)>,
    },
}

fn point_segment_distance(point: vice_geom::Pt, a: vice_geom::Pt, b: vice_geom::Pt) -> f64 {
    let direction = b - a;
    let length_sq = direction.length_sq();
    let t = if length_sq > 0.0 && length_sq.is_finite() {
        ((point - a).dot(direction) / length_sq).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (point - (a + direction * t)).length()
}

fn directed_polyline_distance(points: &[vice_geom::Pt], target: &[vice_geom::Pt]) -> f64 {
    points
        .iter()
        .map(|point| {
            target
                .windows(2)
                .map(|segment| point_segment_distance(*point, segment[0], segment[1]))
                .fold(f64::INFINITY, f64::min)
        })
        .fold(0.0, f64::max)
}

fn observed_support_polyline(samples: &[BoundarySample], closed: bool) -> Vec<vice_geom::Pt> {
    let mut support = samples.iter().map(|sample| sample.p).collect::<Vec<_>>();
    // Open chains retain their literal support. A closed observation owns the
    // seam even though physical resampling does not duplicate its first
    // sample; omitting that edge makes a valid closure look non-isotopic.
    if closed && support.len() >= 2 && support.first() != support.last() {
        support.push(support[0]);
    }
    support
}

fn observed_binding_isotopy(
    geometry: &SelectedBoundaryGeometry,
    samples: &[BoundarySample],
    closed: bool,
) -> Option<(f64, f64)> {
    let fitted = match geometry {
        SelectedBoundaryGeometry::TypedChain { chain } => crate::solve::flatten_chain_at_tolerance(
            chain,
            vice_geom::ChordTolerancePx::new(BINDING_CERTIFICATION_CHORD_TOLERANCE_PX_V1)?,
        )
        .ok()?,
        SelectedBoundaryGeometry::LoopPrimitive {
            verification_polyline,
            ..
        } => verification_polyline.clone(),
    };
    let support = observed_support_polyline(samples, closed);
    if fitted.len() < 2 || support.len() < 2 {
        return None;
    }
    let displacement_px = directed_polyline_distance(&fitted, &support)
        .max(directed_polyline_distance(&support, &fitted))
        + BINDING_CERTIFICATION_CHORD_TOLERANCE_PX_V1;
    let allowed_px = samples
        .iter()
        .map(|sample| sample.halfwidth)
        .fold(0.0f64, f64::max)
        + 0.5
            * samples
                .iter()
                .map(|sample| sample.weight_ds)
                .fold(0.0f64, f64::max);
    let allowed_px = allowed_px + BINDING_CERTIFICATION_CHORD_TOLERANCE_PX_V1;
    (displacement_px.is_finite() && allowed_px.is_finite() && allowed_px > 0.0)
        .then_some((displacement_px, allowed_px))
}

fn retain_binding_certified_stage_h(
    model: &mut BoundaryModel,
    samples: &[BoundarySample],
    closed: bool,
) -> Result<(), (f64, f64)> {
    let selected =
        observed_binding_isotopy(&model.geometry, samples, closed).unwrap_or((f64::INFINITY, 0.0));
    let free = observed_binding_isotopy(&model.stage_h_free_geometry, samples, closed)
        .unwrap_or((f64::INFINITY, 0.0));
    if selected.0 <= selected.1 + BINDING_RELATION_RESCUE_MARGIN_PX_V1
        && selected.0 <= free.0 + BINDING_RELATION_RESCUE_MARGIN_PX_V1
    {
        return Ok(());
    }
    if free.0 > free.1 + BINDING_RELATION_RESCUE_MARGIN_PX_V1 {
        return Err(
            if free.0 / free.1.max(f64::MIN_POSITIVE)
                > selected.0 / selected.1.max(f64::MIN_POSITIVE)
            {
                free
            } else {
                selected
            },
        );
    }

    // Stage-H primitives and relations are optional MDL siblings of the
    // already certified free chain. A shorter sibling that leaves the
    // observed binding tube or materially regresses the free sibling's
    // observed-support displacement is inadmissible; it must not discard the
    // valid free reconstruction along with itself.
    model.geometry = model.stage_h_free_geometry.clone();
    model.code = model.stage_h_free_code;
    model.primitive_kept = None;
    model.relations_kept = 0;
    model.relation_kept_indices.clear();
    model.worst_g1_spread_rad =
        selected_worst_g1_spread(&model.geometry, model.closure_smooth).unwrap_or(f64::INFINITY);
    Ok(())
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
        true,
    )
}

/// Bounded hierarchical proposal followed by full-observation certification.
///
/// Dense physical resampling is evidence, not a mandate to repeat the same
/// family/breakpoint search at every subpixel. The proposal chain preserves a
/// uniform spine, the strongest persistent corners, and the total physical
/// observation weight. Levels 32/64/96/128 open deterministically until one
/// produces a binding-certifiable model; every skipped level and path remains
/// explicit search truncation. Retained paths are forced onto the original
/// chain. Only their finite-difference Jacobians are bounded: final residual
/// code, Stage-H comparison, binding isotopy and both corridor directions use
/// every observation.
pub fn k_best_boundary_models_bounded(
    chain: &BoundaryChain,
    budget: &FitBudget,
    canvas_dim_px: f64,
    k: usize,
    proposal_sample_cap: usize,
) -> Result<ModelRun, BoundedFitRefusal> {
    let base = dedup_coincident(chain);
    if base.samples.len() <= proposal_sample_cap {
        return k_best_boundary_models(&base, budget, canvas_dim_px, k)
            .map_err(|refusal| BoundedFitRefusal::Proposal { refusal });
    }
    let mut level_caps = vec![proposal_sample_cap];
    for level in [32, 64, 96] {
        if proposal_sample_cap > level {
            level_caps.push(level);
        }
    }
    level_caps.sort_unstable();
    level_caps.dedup();
    let level_count = level_caps.len();
    let mut aggregate_run: Option<ModelRun> = None;
    let mut evaluated_levels = Vec::with_capacity(level_count);
    let mut models = Vec::<(usize, BoundaryModel)>::new();
    let mut refusals = Vec::new();
    let mut continuous_solve_samples = 0usize;
    let mut proposal_count = 0usize;
    let mut resource_pruned_proposals = 0usize;
    let mut proposal_levels_skipped_after_certification = 0usize;
    for (level_index, cap) in level_caps.into_iter().enumerate() {
        let (proposal, original_indices) = discrete_proposal_chain(&base, cap);
        let proposal_run = k_best_boundary_models_with_table(
            &proposal,
            budget,
            &crate::GEOMETRY_CODE_TABLE_V1,
            canvas_dim_px,
            k,
            false,
        )
        .map_err(|refusal| BoundedFitRefusal::Proposal { refusal })?;
        evaluated_levels.push(proposal.samples.len());
        let run = aggregate_run.get_or_insert_with(|| {
            let mut aggregate = proposal_run.clone();
            aggregate.models.clear();
            aggregate.candidates = 0;
            aggregate.edges = 0;
            aggregate.discrete_paths = 0;
            aggregate.cuts_evaluated = 0;
            aggregate.refused.clear();
            aggregate.cost_refusals.clear();
            aggregate.worst_normal_to_euclidean_ratio = None;
            aggregate.material_cost_samples = 0;
            aggregate.not_representable = 0;
            aggregate
        });
        run.candidates = run.candidates.saturating_add(proposal_run.candidates);
        run.edges = run.edges.saturating_add(proposal_run.edges);
        run.discrete_paths = run
            .discrete_paths
            .saturating_add(proposal_run.discrete_paths);
        run.cuts_evaluated = run
            .cuts_evaluated
            .saturating_add(proposal_run.cuts_evaluated);
        run.not_representable = run
            .not_representable
            .saturating_add(proposal_run.not_representable);
        for &(name, count) in &proposal_run.refused {
            match run.refused.iter_mut().find(|entry| entry.0 == name) {
                Some(entry) => entry.1 = entry.1.saturating_add(count),
                None => run.refused.push((name, count)),
            }
        }
        for &(family, reason, count) in &proposal_run.cost_refusals {
            match run
                .cost_refusals
                .iter_mut()
                .find(|entry| entry.0 == family && entry.1 == reason)
            {
                Some(entry) => entry.2 = entry.2.saturating_add(count),
                None => run.cost_refusals.push((family, reason, count)),
            }
        }
        run.material_cost_samples = run
            .material_cost_samples
            .saturating_add(proposal_run.material_cost_samples);
        if proposal_run
            .worst_normal_to_euclidean_ratio
            .is_some_and(|reading| {
                run.worst_normal_to_euclidean_ratio
                    .is_none_or(|current| reading.ratio > current.ratio)
            })
        {
            run.worst_normal_to_euclidean_ratio = proposal_run.worst_normal_to_euclidean_ratio;
        }
        proposal_count = proposal_count.saturating_add(proposal_run.models.len());
        let proposal_cut = proposal_run.selected_cut;
        let full_cut = original_indices[proposal_cut];
        let full_chain = rotate_unopened(&base, full_cut);
        let mut certified_at_level = 0usize;
        for (proposal_index, proposal_model) in proposal_run.models.iter().enumerate() {
            if certified_at_level >= MAX_CERTIFIED_MODELS_PER_CHAIN_V1
                || proposal_index >= MAX_CERTIFICATION_ATTEMPTS_PER_LEVEL_V1
            {
                resource_pruned_proposals = resource_pruned_proposals
                    .saturating_add(proposal_run.models.len() - proposal_index);
                break;
            }
            let breakpoints = proposal_model
                .breakpoints
                .iter()
                .map(|breakpoint| {
                    let original =
                        original_indices[(proposal_cut + breakpoint) % original_indices.len()];
                    (original + base.samples.len() - full_cut) % base.samples.len()
                })
                .collect::<Vec<_>>();
            match fit_forced_boundary_models_impl(
                &full_chain,
                &proposal_model.families,
                &breakpoints,
                canvas_dim_px,
                1,
                Some(CONTINUOUS_REFIT_SAMPLE_CAP_V1),
            ) {
                Ok(forced) => {
                    continuous_solve_samples =
                        continuous_solve_samples.max(forced.continuous_solve_samples);
                    certified_at_level = certified_at_level.saturating_add(forced.models.len());
                    models.extend(forced.models.into_iter().map(|model| (full_cut, model)));
                }
                Err(refusal) => refusals.push(refusal),
            }
        }
        if certified_at_level > 0 {
            proposal_levels_skipped_after_certification =
                level_count.saturating_sub(level_index + 1);
            break;
        }
    }
    let mut run = aggregate_run.expect("at least one proposal level");
    if proposal_count == 0 {
        return Err(BoundedFitRefusal::NoProposalModels {
            levels: evaluated_levels,
            candidates: run.candidates,
            discrete_paths: run.discrete_paths,
            refused: run.refused,
            not_representable: run.not_representable,
        });
    }
    if models.is_empty() {
        return Err(BoundedFitRefusal::AllFullResolutionCertificationsRefused {
            proposals: proposal_count,
            refusals,
        });
    }
    models.sort_by(|left, right| compare_model_rank(&left.1, &right.1));
    models.dedup_by(|left, right| left.1 == right.1);
    models.truncate(k);
    run.selected_cut = models[0].0;
    run.models = models.into_iter().map(|(_, model)| model).collect();
    run.observed_samples = base.samples.len();
    run.discrete_search_samples = evaluated_levels.iter().copied().max().unwrap_or(0);
    run.discrete_search_levels = evaluated_levels;
    run.continuous_solve_samples = continuous_solve_samples;
    run.full_resolution_refit = continuous_solve_samples >= base.samples.len();
    run.full_resolution_certified = true;
    run.full_certification_refusals = refusals.len();
    run.resource_pruned_proposals = resource_pruned_proposals;
    run.proposal_levels_skipped_after_certification = proposal_levels_skipped_after_certification;
    run.relations_considered = run.models.iter().map(|model| model.relations.len()).sum();
    run.relations_accepted = run.models.iter().map(|model| model.relations_kept).sum();
    run.primitives_considered = run.models.iter().map(|model| model.primitives.len()).sum();
    run.primitives_accepted = run
        .models
        .iter()
        .filter(|model| model.primitive_kept.is_some())
        .count();
    Ok(run)
}

mod search;
#[cfg(test)]
use search::compare_rank_values;
use search::{
    compare_model_rank, discrete_proposal_chain, fit_forced_boundary_models_impl,
    k_best_boundary_models_with_table, rotate_unopened,
};
pub use search::{fit_forced_boundary_models, models_at_cut};

/// The families of a path, for callers that want the discrete answer without
/// the solve.
pub fn path_families(
    path: &GrammarPath,
    candidates: &[crate::SpanCandidate],
) -> Result<Vec<SpanFamily>, FitRefusal> {
    path.candidates
        .iter()
        .enumerate()
        .map(|(path_candidate, &candidate)| {
            candidates
                .get(candidate)
                .map(|candidate| candidate.family)
                .ok_or(FitRefusal::PathCandidateOutOfRange {
                    path_candidate,
                    candidate,
                    candidates: candidates.len(),
                })
        })
        .collect()
}

#[cfg(test)]
mod bounded_tests;
#[cfg(test)]
mod ranking_tests;
