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
//! derived set of points — the corner anchors, plus sample zero — solves each
//! cut independently, and returns the best. The cut-invariance TEST is
//! `the_cut_a_closed_chain_is_opened_at_does_not_change_what_is_selected`, and
//! the spread it measures is published rather than asserted to be zero.

use serde::Serialize;
use vice_evidence::{BoundaryChain, BoundarySample};

use crate::code::{ChainCode, GeometryCodeTable};
use crate::grammar::{build_edges, k_best_paths, materialize, path_is_representable, GrammarPath};
use crate::refit::{g1_readings, RefitChain, RefitRefusal};
use crate::schedule::FitBudget;
use crate::solve::joint_constrained_refit;
use crate::span::SpanFamily;
use crate::{span_candidates, FitRefusal};

/// One accepted boundary model: a discrete grammar path that survived the joint
/// solve, with everything a ranking or a gate would need.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BoundaryModel {
    pub chain: RefitChain,
    pub families: Vec<SpanFamily>,
    pub breakpoints: Vec<usize>,
    pub smooth: Vec<bool>,
    /// The code length of the DISCRETE path, from the DP.
    pub code: ChainCode,
    /// §14.4's integral, carried separately and never summed into `code`.
    pub proposal_cost_px: f64,
    /// Worst G1 spread over the model's smooth nodes after lowering, radians.
    /// Exactly what `vice-ir`'s canonical fixture reads 0.4949 rad on.
    pub worst_g1_spread_rad: f64,
    /// Worst `|d_n|` after the solve, px.
    pub worst_normal_deviation_px: f64,
    pub residual_before: f64,
    pub residual_after: f64,
}

/// What one call to [`k_best_boundary_models`] did, including the paths it
/// threw away.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModelRun {
    pub models: Vec<BoundaryModel>,
    pub candidates: usize,
    pub edges: usize,
    pub discrete_paths: usize,
    /// Paths the joint solve refused, by reason. §24 exists because this is not
    /// zero; a run where it is always zero would mean `k` is doing nothing.
    pub refused: Vec<(&'static str, usize)>,
    /// Paths dropped before the solver because the representation cannot carry
    /// them (a quadratic smooth at both ends).
    pub not_representable: usize,
}

fn refusal_name(r: &RefitRefusal) -> &'static str {
    match r {
        RefitRefusal::Malformed => "malformed",
        RefitRefusal::DegenerateSpan { .. } => "degenerate_span",
        RefitRefusal::ArcIsALine { .. } => "arc_is_a_line",
        RefitRefusal::NonFinite { .. } => "non_finite",
        RefitRefusal::OutsideCorridor { .. } => "outside_corridor",
        RefitRefusal::SmoothJoinBetweenTwoLines { .. } => "smooth_join_between_two_lines",
    }
}

/// **§28 M6 bullets 3 and 4 for one chain**, in §24's order.
///
/// `canvas_dim_px` is the code's coordinate range and must be the image's, not
/// a default: it moves every anchor's cost by `2 log2` of the ratio.
pub fn k_best_boundary_models(
    chain: &BoundaryChain,
    budget: &FitBudget,
    table: &GeometryCodeTable,
    canvas_dim_px: f64,
    k: usize,
) -> Result<ModelRun, FitRefusal> {
    let chain = &dedup_coincident(chain);
    let cuts = if chain.closed {
        canonical_cuts(chain)
    } else {
        vec![0usize]
    };
    let mut best: Option<ModelRun> = None;
    for cut in cuts {
        let rotated = rotate(chain, cut);
        let run = models_for_open_chain(&rotated, budget, table, canvas_dim_px, k)?;
        let better = match &best {
            None => true,
            Some(b) => match (b.models.first(), run.models.first()) {
                (_, None) => false,
                (None, Some(_)) => true,
                (Some(x), Some(y)) => y.code.total_bits() < x.code.total_bits(),
            },
        };
        if better {
            best = Some(run);
        }
    }
    best.ok_or(FitRefusal::ChainTooShort {
        samples: chain.samples.len(),
        minimum: crate::MIN_SUPPORT_SAMPLES,
    })
}

/// Collapse runs of coincident samples into one, summing the arclength each
/// stood for.
///
/// **This is what makes §14.5's "duplicate samples" invariance a property of
/// the code rather than a hope**, and it was written because the property
/// FAILED: a chain with every third sample repeated selected `[Cubic]` where
/// the same boundary without the repeats selected `[CircularArc, Line]`. The
/// residual code was already duplicate-invariant — it counts
/// `weight_ds / corr_length_px`, and a repeat carries no arclength — but the
/// SCHEDULE is over sample INDICES, so repeats move every dyadic support to a
/// different place on the boundary and change which candidates exist at all.
///
/// Collapsing at the entry makes every stage downstream see the physical chain.
/// `observe_boundaries` resamples by arclength and does not normally produce
/// coincident samples, which is exactly why this had to be measured rather than
/// assumed: the defect is invisible on the population the pipeline actually
/// sees, and §14.5 asks for the property anyway.
pub fn dedup_coincident(chain: &BoundaryChain) -> BoundaryChain {
    let mut samples: Vec<BoundarySample> = Vec::with_capacity(chain.samples.len());
    for s in &chain.samples {
        match samples.last_mut() {
            Some(prev) if prev.p == s.p => prev.weight_ds += s.weight_ds,
            _ => samples.push(*s),
        }
    }
    if samples.len() == chain.samples.len() {
        return chain.clone();
    }
    let vertices = samples.len() as u64;
    BoundaryChain {
        samples,
        vertices,
        ..chain.clone()
    }
}

/// The points a closed chain is opened at.
///
/// Sample zero plus the corner anchors: a cut at a corner is the one place a
/// grammar has no join to lose, and sample zero is included so a chain with no
/// corner is still cut somewhere. Derived from the chain, never a fixed list.
pub fn canonical_cuts(chain: &BoundaryChain) -> Vec<usize> {
    let proposals = crate::corner::corner_proposals(&chain.samples);
    let mut cuts = vec![0usize];
    cuts.extend(crate::corner::corner_anchors(
        &proposals,
        crate::CORNER_ANCHOR_HALF_WINDOW,
    ));
    cuts.sort_unstable();
    cuts.dedup();
    cuts
}

/// Rotate a closed chain so that `cut` becomes its first sample, and repeat
/// that sample at the end so the opened chain closes geometrically.
pub fn rotate(chain: &BoundaryChain, cut: usize) -> BoundaryChain {
    if cut == 0 && !chain.closed {
        return chain.clone();
    }
    let n = chain.samples.len();
    let mut samples: Vec<BoundarySample> = (0..n).map(|i| chain.samples[(cut + i) % n]).collect();
    if chain.closed {
        samples.push(chain.samples[cut]);
    }
    BoundaryChain {
        samples,
        ..chain.clone()
    }
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
    let edges = build_edges(&cands.candidates, samples, table, canvas_dim_px);
    let paths = k_best_paths(&edges, samples, table, canvas_dim_px, k);

    let mut models = Vec::new();
    let mut refused: Vec<(&'static str, usize)> = Vec::new();
    let mut not_representable = 0usize;

    for path in &paths {
        let families: Vec<SpanFamily> = path
            .candidates
            .iter()
            .map(|c| cands.candidates[*c].family)
            .collect();
        if !path_is_representable(path, &families) {
            not_representable += 1;
            continue;
        }
        let Some(init) = materialize(path, &edges, &cands.candidates, samples) else {
            not_representable += 1;
            continue;
        };
        match joint_constrained_refit(&init, samples) {
            Ok(out) => {
                let Ok(lowered) = out.chain.lower() else {
                    bump(&mut refused, "malformed");
                    continue;
                };
                let worst_g1 = g1_readings(&lowered, out.chain.start(), out.chain.end())
                    .iter()
                    .map(|r| r.spread_rad)
                    .fold(0.0f64, f64::max);
                models.push(BoundaryModel {
                    chain: out.chain,
                    families,
                    breakpoints: path.breakpoints.clone(),
                    smooth: path.smooth.clone(),
                    code: path.code,
                    proposal_cost_px: path.proposal_cost_px,
                    worst_g1_spread_rad: worst_g1,
                    worst_normal_deviation_px: out.worst_normal_deviation_px,
                    residual_before: out.residual_before,
                    residual_after: out.residual_after,
                });
            }
            Err(why) => bump(&mut refused, refusal_name(&why)),
        }
    }

    // §24's `rank_by_proposal_integral_and_code_length`: code length first
    // because it is the physical-bit score §14.5 names as the selector, the
    // §14.4 integral as the tie-break, because §14.4 says it ORDERS candidates
    // and does not select among final models.
    models.sort_by(|a, b| {
        a.code
            .total_bits()
            .total_cmp(&b.code.total_bits())
            .then(a.proposal_cost_px.total_cmp(&b.proposal_cost_px))
    });
    refused.sort_unstable();

    Ok(ModelRun {
        models,
        candidates: cands.candidates.len(),
        edges: edges.len(),
        discrete_paths: paths.len(),
        refused,
        not_representable,
    })
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
