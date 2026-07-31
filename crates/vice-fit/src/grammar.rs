//! §14.3's jet-compatible grammar DAG and its k-shortest paths, and §24's loop
//! around the joint refit.
//!
//! ## What the DP decides, and what it explicitly does not
//!
//! §14.3 lists the DP's four outputs — breakpoints, families, corner vs smooth
//! joins, and COARSE tangent compatibility — and then says the fifth thing
//! plainly: "Проверка `angle < tolerance` сама по себе **не является G1**."
//!
//! So [`jet_compatible`] is a bucket comparison and is called what it is: the
//! coarse compatibility of §14.3, a filter on which discrete paths are worth
//! handing to the solver. **It is not G1 and nothing here says it is.** G1 comes
//! from [`crate::refit::RefitChain`], where the tangent is a single shared
//! parameter and the violation cannot be written down.
//!
//! ## The objective is a code length, in bits, and there is no BIC in it
//!
//! Edge and node costs are §14.5 code lengths from the frozen
//! `[geometry_code_table]`, plus the robust quantized residual code. §14.5 bans
//! raw-sample BIC as the final selector and bans `BIC_eff` from promoting
//! anything; there is no `k log n` here and no free multiplier anywhere.
//!
//! **The one deliberate looseness, named with its size.** §14.5's minimal code
//! includes a "combinatorial code breakpoints/corners", which is
//! `log2 C(n-2, k)` and depends on the whole path through `k`. A cost that
//! depends on `k` is not decomposable over edges, so a DP minimising edge sums
//! would be minimising a surrogate and re-ranking afterwards. Instead each
//! segment pays `log2(n-1)` bits for its own LENGTH — a uniform prefix code
//! over the possible gaps, exactly decomposable, and looser than the binomial by
//! about `k log2 k` bits. Looser is the conservative direction for a code
//! length, and the DP is then a true shortest path for the number it reports
//! rather than for a stand-in.
//!
//! ## The proposal cost is used for ORDERING, not for selection
//!
//! §14.4: the boundary integral is "используется для candidate ordering" and is
//! not added to the pixel posterior. It orders the candidates within a support
//! so the DP sees the best of each family first, and it is reported per path by
//! [`GrammarPath::proposal_cost_px`]; it is NOT part of `code`. §24's
//! `rank_by_proposal_integral_and_code_length` therefore has both to rank by,
//! and they are separate numbers.

use serde::Serialize;
use vice_evidence::BoundarySample;

use crate::code::{ChainCode, GeometryCodeTable};
use crate::refit::{ArcAnchor, Handle, RefitChain, RefitNode, RefitSegment};
use crate::span::{SpanCandidate, SpanFamily};

mod closure;
mod control;
mod surface;
use closure::{close_finished_path, ClosureMode, ClosureState};
pub use control::{k_best_proposal_control_paths, ProposalControlPath};
pub use surface::{candidate_jets, free_scalars};
use surface::{
    compare_partial, compare_path_rank, family_ord, validate_candidate, validate_grammar_edges,
    validate_partial_cost, FAMILY_BY_ORD,
};

/// Buckets the endpoint tangent direction is quantized into for the DP state.
///
/// Thirty-two over the full turn: 11.25 degrees each, and compatibility admits
/// the adjacent bucket, so the coarse filter passes joins within about 11
/// degrees. **That is a tolerance and it is named as one** — it is §14.3's
/// "coarse tangent compatibility", a pruning rule on discrete paths, and the
/// exactness comes from the refit. A path the filter rejects is a path whose
/// two spans disagree by more than a bucket; a path it accepts still has to
/// survive the joint solve.
pub const JET_CLASSES: usize = 32;

/// The k in k-shortest paths.
///
/// §24: "K-best нужен, потому что лучший coarse DP path может стать infeasible
/// после exact constraints." Eight, and the number that justifies it is
/// published per run: `GrammarRun::paths_refused_by_the_solver` counts how many
/// of the k the joint refit threw away, so a k that is too small shows up as a
/// run where every path was refused rather than as a silently worse answer.
pub const K_DISCRETE_PATHS: usize = 8;
/// Exact order used at every K truncation, including cyclic seam branches.
pub const PATH_ORDER_POLICY: &str = "physical_code_then_proposal_integral_v1";

/// Which bucket a direction falls in.
pub fn jet_class(dir_rad: f64) -> usize {
    let t = std::f64::consts::TAU;
    let mut a = dir_rad % t;
    if a < 0.0 {
        a += t;
    }
    ((a / t * JET_CLASSES as f64) as usize) % JET_CLASSES
}

/// §14.3's COARSE tangent compatibility: the same bucket or an adjacent one.
pub fn jet_compatible(a: usize, b: usize) -> bool {
    let d = a.abs_diff(b);
    d <= 1 || d == JET_CLASSES - 1
}

/// Whether a family's endpoint tangent can take an arbitrary value.
///
/// A line's cannot: it is the chord, fixed by the two nodes. So a smooth join
/// with a line on one side does not introduce a free angle — the angle is
/// DETERMINED — and charging for one would be charging for a parameter the
/// scene does not carry.
fn tangent_is_free(f: SpanFamily) -> bool {
    !matches!(f, SpanFamily::Line)
}

/// Whether a smooth transition can be materialized by [`RefitChain`].
///
/// This is checked in the DP, before an unrepresentable path can consume one
/// of the frozen K slots. The post-DP [`path_is_representable`] check remains
/// as a defence for paths supplied by other callers.
fn smooth_transition_is_representable(
    incoming: SpanFamily,
    incoming_head_shared: bool,
    outgoing: SpanFamily,
) -> bool {
    match incoming {
        SpanFamily::Quad => false,
        SpanFamily::CircularArc => !incoming_head_shared,
        SpanFamily::Line => outgoing != SpanFamily::Line,
        SpanFamily::Cubic => true,
    }
}

/// One edge of the grammar DAG: a candidate with its jets resolved.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct GrammarEdge {
    /// Index into the candidate list handed to [`k_best_paths`].
    pub candidate: usize,
    pub from: usize,
    pub to: usize,
    pub family: SpanFamily,
    pub entry_class: usize,
    pub exit_class: usize,
    pub entry_rad: f64,
    pub exit_rad: f64,
    /// The robust quantized residual code over the samples this edge OWNS —
    /// `(from, to]`, so no sample is charged twice when spans meet at a
    /// breakpoint (§17.2).
    pub residual_bits: f64,
    /// §14.4's proposal integral, carried through for ranking.
    pub proposal_cost_px: f64,
}

/// A complete grammar path over one chain, with its code length.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GrammarPath {
    /// Candidate indices in order.
    pub candidates: Vec<usize>,
    /// Sample indices of the interior breakpoints.
    pub breakpoints: Vec<usize>,
    /// `true` where the interior node is a SMOOTH join. One entry per
    /// breakpoint.
    pub smooth: Vec<bool>,
    /// Whether the path covers a cyclic chain with a seam join.
    pub closed: bool,
    /// The seam join chosen by the cyclic DP. False means a corner when
    /// `closed`, and has no meaning for an open path.
    pub closure_smooth: bool,
    pub code: ChainCode,
    /// Sum of §14.4's integrals over the path's spans. Separate from `code`
    /// and never added to it (§14.4).
    pub proposal_cost_px: f64,
}

impl GrammarPath {
    pub fn total_bits(&self) -> f64 {
        self.code.total_bits()
    }
}

/// Build the DAG's edges from a candidate list.
pub fn build_edges(
    candidates: &[SpanCandidate],
    samples: &[BoundarySample],
    table: &GeometryCodeTable,
    canvas_dim_px: f64,
) -> Result<Vec<GrammarEdge>, crate::FitRefusal> {
    crate::validate_samples(samples)?;
    crate::validate_canvas_dimension(canvas_dim_px)?;
    let precision = table.coordinate_precision_px();
    let mut out = Vec::with_capacity(candidates.len());
    for (i, c) in candidates.iter().enumerate() {
        validate_candidate(c, i, samples)?;
        let Some((entry_rad, exit_rad)) = candidate_jets(c, samples) else {
            continue;
        };
        let Some((poly, _)) = crate::span::flatten(
            &c.segment,
            samples[c.support.lo()].p,
            samples[c.support.hi()].p,
        ) else {
            continue;
        };
        // The samples this edge OWNS: `(lo, hi]`. The chain's first sample is
        // charged once, by the source transition in `k_best_paths`.
        let mut residual = 0.0f64;
        let mut ok = true;
        for (offset, s) in samples[c.support.lo() + 1..=c.support.hi()]
            .iter()
            .enumerate()
        {
            if s.weight_ds == 0.0 {
                continue;
            }
            let Some(dn) = crate::cost::normal_deviation(s.p, s.normal, &poly) else {
                ok = false;
                break;
            };
            residual = crate::code::accumulate_residual_bits(
                residual,
                s,
                c.support.lo() + 1 + offset,
                dn,
                precision,
            )?;
        }
        if !ok {
            continue;
        }
        let _ = canvas_dim_px;
        out.push(GrammarEdge {
            candidate: i,
            from: c.support.lo(),
            to: c.support.hi(),
            family: c.family,
            entry_class: jet_class(entry_rad),
            exit_class: jet_class(exit_rad),
            entry_rad,
            exit_rad,
            residual_bits: residual,
            proposal_cost_px: c.proposal_cost_px(),
        });
    }
    Ok(out)
}

/// A partial path in the k-best DP.
///
/// The path itself is a BACKPOINTER, not a vector. The first version carried a
/// `Vec<usize>` in every partial and cloned it at every relaxation; on a
/// 305-sample chain that was 160 seconds for one closed chain's canonical cuts.
/// The arena below is the same algorithm with the same result and no allocation
/// in the inner loop.
#[derive(Debug, Clone, Copy)]
struct Partial {
    bits: f64,
    geometry: f64,
    topology: f64,
    residual: f64,
    proposal: f64,
    edge: usize,
    /// `true` when the join at the node this partial's last edge STARTS from
    /// was smooth. `false` for the seed, which starts at the chain end.
    smooth_here: bool,
    closure: Option<ClosureState>,
    prev: Option<usize>,
}

/// The DP state key at a node.
///
/// The suffix of a path depends on the prefix only through these three things —
/// the exit bucket (for compatibility), the last family and whether its head
/// was shared (for the exact scalar count at the coming node). So keeping the
/// `k` best per KEY is exact k-best, where keeping the `k` best per node would
/// not be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct StateKey {
    exit_class: usize,
    family_ord: usize,
    head_shared: bool,
    closure: Option<ClosureState>,
}

/// **§28 M6 bullet 3.** The k best jet-compatible grammar paths over one chain.
///
/// Exact k-shortest paths over the DAG under the code-length objective, with
/// the state carrying enough of the prefix that the suffix cost is independent
/// of the rest of it.
pub fn k_best_paths(
    edges: &[GrammarEdge],
    samples: &[BoundarySample],
    table: &GeometryCodeTable,
    canvas_dim_px: f64,
    k: usize,
) -> Result<Vec<GrammarPath>, crate::FitRefusal> {
    let first_sample_bits = crate::code::first_sample_residual_bits(samples, table, canvas_dim_px)?;
    validate_grammar_edges(edges, samples)?;
    k_best_paths_for_objective(
        edges,
        samples,
        table,
        canvas_dim_px,
        k,
        (PathObjective::PhysicalCode, ClosureMode::Open),
        first_sample_bits,
    )
}

pub(crate) fn k_best_paths_with_closure(
    edges: &[GrammarEdge],
    samples: &[BoundarySample],
    table: &GeometryCodeTable,
    canvas_dim_px: f64,
    k: usize,
    closed: bool,
) -> Result<Vec<GrammarPath>, crate::FitRefusal> {
    let first_sample_bits = crate::code::first_sample_residual_bits(samples, table, canvas_dim_px)?;
    validate_grammar_edges(edges, samples)?;
    if !closed {
        return k_best_paths_for_objective(
            edges,
            samples,
            table,
            canvas_dim_px,
            k,
            (PathObjective::PhysicalCode, ClosureMode::Open),
            first_sample_bits,
        );
    }
    let mut paths = k_best_paths_for_objective(
        edges,
        samples,
        table,
        canvas_dim_px,
        k,
        (PathObjective::PhysicalCode, ClosureMode::Corner),
        first_sample_bits,
    )?;
    paths.extend(k_best_paths_for_objective(
        edges,
        samples,
        table,
        canvas_dim_px,
        k,
        (PathObjective::PhysicalCode, ClosureMode::Smooth),
        first_sample_bits,
    )?);
    paths.sort_by(compare_path_rank);
    paths.truncate(k);
    Ok(paths)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PathObjective {
    PhysicalCode,
    ProposalResidual,
}

fn k_best_paths_for_objective(
    edges: &[GrammarEdge],
    samples: &[BoundarySample],
    table: &GeometryCodeTable,
    canvas_dim_px: f64,
    k: usize,
    (objective, closure_mode): (PathObjective, ClosureMode),
    first_sample_bits: f64,
) -> Result<Vec<GrammarPath>, crate::FitRefusal> {
    let n = samples.len();
    if n < 2 || edges.is_empty() || k == 0 {
        return Ok(Vec::new());
    }
    let physical = objective == PathObjective::PhysicalCode;
    let cb = if physical {
        table.coordinate_bits(canvas_dim_px)
    } else {
        0.0
    };
    let anchor = if physical {
        table.anchor_bits(canvas_dim_px)
    } else {
        0.0
    };
    let join_bits = if physical {
        (crate::code::JOIN_KINDS as f64).log2()
    } else {
        0.0
    };
    let gap_bits = if physical {
        crate::code::gap_bits(n)
    } else {
        0.0
    };
    // The chain's first sample, charged once so it is not free and not double
    // counted (§17.2). Its deviation is zero for every candidate — both
    // endpoints are held at sample positions — so this is the code's
    // normalising constant, identical for every path.
    let mut by_from: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, e) in edges.iter().enumerate() {
        by_from[e.from].push(i);
    }

    let mut arena: Vec<Partial> = Vec::new();
    // `states[v]` maps a key to arena indices of the best partials reaching `v`.
    let mut states: Vec<std::collections::BTreeMap<StateKey, Vec<usize>>> =
        vec![Default::default(); n];
    let mut finished: Vec<usize> = Vec::new();

    let seg_bits = |f: SpanFamily, head: bool| {
        if physical {
            table.bits_per_segment_family()
                + f.flag_bits()
                + free_scalars(f, head, false) as f64 * cb
                + gap_bits
        } else {
            0.0
        }
    };
    let edge_objective = |edge: GrammarEdge| {
        if physical {
            edge.residual_bits
        } else {
            edge.proposal_cost_px
        }
    };

    for &ei in &by_from[0] {
        let e = edges[ei];
        let g = seg_bits(e.family, false);
        let residual = edge_objective(e);
        let p = Partial {
            bits: g + residual + first_sample_bits,
            geometry: g,
            topology: 0.0,
            residual: if physical {
                e.residual_bits + first_sample_bits
            } else {
                0.0
            },
            proposal: e.proposal_cost_px,
            edge: ei,
            smooth_here: false,
            closure: closure_mode.state_for_seed(e),
            prev: None,
        };
        let p = validate_partial_cost(p)?;
        let Some(p) = validate_complete_partial(p, e.to, n, closure_mode, edges, cb, join_bits)?
        else {
            continue;
        };
        push_state(
            &mut arena,
            &mut states[e.to],
            &e,
            false,
            p,
            k,
            &mut finished,
            n,
        );
    }

    for v in 1..n {
        let keyed: Vec<(StateKey, Vec<usize>)> =
            std::mem::take(&mut states[v]).into_iter().collect();
        if v == n - 1 {
            for (_, ps) in keyed {
                finished.extend(ps);
            }
            continue;
        }
        for (key, partials) in keyed {
            let f_in = FAMILY_BY_ORD[key.family_ord];
            // What the incoming segment saves if this node shares its tangent.
            let tail_saving = free_scalars(f_in, key.head_shared, false) as f64
                - free_scalars(f_in, key.head_shared, true) as f64;
            for &ei in &by_from[v] {
                let e = edges[ei];
                for smooth in [false, true] {
                    if smooth && !jet_compatible(key.exit_class, e.entry_class) {
                        continue;
                    }
                    if smooth
                        && !smooth_transition_is_representable(f_in, key.head_shared, e.family)
                    {
                        continue;
                    }
                    let angle_free = smooth && tangent_is_free(f_in) && tangent_is_free(e.family);
                    let node_bits = join_bits + anchor + f64::from(u8::from(angle_free)) * cb;
                    let g = seg_bits(e.family, smooth);
                    let geometry_delta = g - if smooth { tail_saving * cb } else { 0.0 };
                    let residual = edge_objective(e);
                    for &pi in &partials {
                        let p = arena[pi];
                        let q = Partial {
                            bits: p.bits + geometry_delta + node_bits + residual,
                            geometry: p.geometry + geometry_delta,
                            topology: p.topology + node_bits,
                            residual: if physical {
                                p.residual + e.residual_bits
                            } else {
                                0.0
                            },
                            proposal: p.proposal + e.proposal_cost_px,
                            edge: ei,
                            smooth_here: smooth,
                            closure: p
                                .closure
                                .map(|state| state.after_join(p.prev.is_none(), smooth)),
                            prev: Some(pi),
                        };
                        let q = validate_partial_cost(q)?;
                        let Some(q) = validate_complete_partial(
                            q,
                            e.to,
                            n,
                            closure_mode,
                            edges,
                            cb,
                            join_bits,
                        )?
                        else {
                            continue;
                        };
                        push_state(
                            &mut arena,
                            &mut states[e.to],
                            &e,
                            smooth,
                            q,
                            k,
                            &mut finished,
                            n,
                        );
                    }
                }
            }
        }
    }

    finished.sort_by(|a, b| compare_partial(&arena[*a], &arena[*b]));
    finished.truncate(k);
    finished
        .into_iter()
        .map(|end| {
            let mut walk: Vec<Partial> = Vec::new();
            let mut cur = Some(end);
            while let Some(i) = cur {
                walk.push(arena[i]);
                cur = arena[i].prev;
            }
            walk.reverse();
            let p = arena[end];
            let path = GrammarPath {
                candidates: walk.iter().map(|x| edges[x.edge].candidate).collect(),
                breakpoints: walk[1..].iter().map(|x| edges[x.edge].from).collect(),
                smooth: walk[1..].iter().map(|x| x.smooth_here).collect(),
                closed: closure_mode.is_closed(),
                closure_smooth: closure_mode.is_smooth(),
                code: ChainCode {
                    geometry_bits: p.geometry,
                    topology_bits: p.topology,
                    relation_bits: 0.0,
                    residual_bits: p.residual,
                },
                proposal_cost_px: p.proposal,
            };
            if path.total_bits().is_finite() {
                Ok(path)
            } else {
                Err(crate::FitRefusal::InvalidGrammarPathCost {
                    edge: p.edge,
                    total_bits: path.total_bits(),
                    geometry_bits: path.code.geometry_bits,
                    topology_bits: path.code.topology_bits,
                    residual_bits: path.code.residual_bits,
                    proposal_cost_px: path.proposal_cost_px,
                })
            }
        })
        .collect()
}

fn validate_complete_partial(
    mut partial: Partial,
    to: usize,
    sample_count: usize,
    closure_mode: ClosureMode,
    edges: &[GrammarEdge],
    coordinate_bits: f64,
    join_bits: f64,
) -> Result<Option<Partial>, crate::FitRefusal> {
    if to != sample_count - 1 || !closure_mode.is_closed() {
        return Ok(Some(partial));
    }
    if !close_finished_path(
        &mut partial,
        edges,
        coordinate_bits,
        join_bits,
        closure_mode.is_smooth(),
    ) {
        return Ok(None);
    }
    validate_partial_cost(partial).map(Some)
}

#[allow(clippy::too_many_arguments)]
fn push_state(
    arena: &mut Vec<Partial>,
    slot: &mut std::collections::BTreeMap<StateKey, Vec<usize>>,
    e: &GrammarEdge,
    head_shared: bool,
    p: Partial,
    k: usize,
    finished: &mut Vec<usize>,
    n: usize,
) {
    if e.to == n - 1 {
        if finished.len() >= k
            && finished
                .last()
                .is_some_and(|worst| !compare_partial(&p, &arena[*worst]).is_lt())
        {
            return;
        }
        let idx = arena.len();
        arena.push(p);
        finished.push(idx);
        finished.sort_by(|a, b| compare_partial(&arena[*a], &arena[*b]));
        finished.truncate(k);
        return;
    }
    let key = StateKey {
        exit_class: e.exit_class,
        family_ord: family_ord(e.family),
        head_shared,
        closure: p.closure,
    };
    let v = slot.entry(key).or_default();
    if v.len() >= k
        && v.last()
            .is_some_and(|worst| !compare_partial(&p, &arena[*worst]).is_lt())
    {
        return;
    }
    let idx = arena.len();
    arena.push(p);
    v.push(idx);
    v.sort_by(|a, b| compare_partial(&arena[*a], &arena[*b]));
    v.truncate(k);
}

/// Turn a discrete path into the shared-parameter representation the joint
/// refit optimises.
///
/// The initial tangent angle at a smooth node is the MEAN of what the two
/// spans arrive and leave with — an initialisation, not a decision: the solve
/// moves it, and whatever it moves to, both incident control points follow it
/// because there is only one of it.
mod materialize;
pub(crate) use materialize::materialize_with_closure;
pub use materialize::{materialize, path_is_representable};

#[cfg(test)]
mod tests;
