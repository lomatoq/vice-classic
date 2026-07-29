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

/// Directions a candidate leaves its first sample with and arrives at its last
/// sample with, in radians.
pub fn candidate_jets(c: &SpanCandidate, samples: &[BoundarySample]) -> Option<(f64, f64)> {
    let p0 = samples[c.support.lo()].p;
    let p1 = samples[c.support.hi()].p;
    let (poly, _) = crate::span::flatten(&c.segment, p0, p1)?;
    if poly.len() < 2 {
        return None;
    }
    let entry = poly[1] - poly[0];
    let exit = poly[poly.len() - 1] - poly[poly.len() - 2];
    (entry.length_sq() > 0.0 && exit.length_sq() > 0.0)
        .then(|| (entry.y.atan2(entry.x), exit.y.atan2(exit.x)))
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

/// Scalars a family still has to code, given which of its ends read a shared
/// tangent.
///
/// Exact, family by family, rather than a saving applied uniformly:
/// - a line has none either way;
/// - a circle through two points with a prescribed tangent at EITHER end is
///   determined, so one shared end takes its radius to zero free scalars and a
///   second one takes nothing further;
/// - a quadratic's single control point loses one scalar per shared end, and
///   with both shared it is the intersection of the two tangent lines;
/// - a cubic loses one scalar per shared end, keeping a handle length.
pub fn free_scalars(f: SpanFamily, head_shared: bool, tail_shared: bool) -> usize {
    let shared = usize::from(head_shared) + usize::from(tail_shared);
    match f {
        SpanFamily::Line => 0,
        SpanFamily::CircularArc => usize::from(shared == 0),
        SpanFamily::Quad => 2 - shared.min(2),
        SpanFamily::Cubic => 4 - shared.min(2),
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
) -> Vec<GrammarEdge> {
    let precision = table.coordinate_precision_px();
    let mut out = Vec::with_capacity(candidates.len());
    for (i, c) in candidates.iter().enumerate() {
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
        for s in &samples[c.support.lo() + 1..=c.support.hi()] {
            let Some(dn) = crate::cost::normal_deviation(s.p, s.normal, &poly) else {
                ok = false;
                break;
            };
            let Some(w) = crate::code::independent_observations(s.weight_ds, s.corr_length_px)
            else {
                ok = false;
                break;
            };
            residual += w * crate::code::residual_bits(dn, s.halfwidth, precision);
        }
        if !ok || !residual.is_finite() {
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
    out
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
}

fn family_ord(f: SpanFamily) -> usize {
    match f {
        SpanFamily::Line => 0,
        SpanFamily::CircularArc => 1,
        SpanFamily::Quad => 2,
        SpanFamily::Cubic => 3,
    }
}

const FAMILY_BY_ORD: [SpanFamily; 4] = [
    SpanFamily::Line,
    SpanFamily::CircularArc,
    SpanFamily::Quad,
    SpanFamily::Cubic,
];

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
) -> Vec<GrammarPath> {
    let n = samples.len();
    if n < 2 || edges.is_empty() || k == 0 {
        return Vec::new();
    }
    let cb = table.coordinate_bits(canvas_dim_px);
    let anchor = table.anchor_bits(canvas_dim_px);
    let join_bits = (crate::code::JOIN_KINDS as f64).log2();
    let gap_bits = ((n - 1) as f64).log2();
    let precision = table.coordinate_precision_px();
    // The chain's first sample, charged once so it is not free and not double
    // counted (§17.2). Its deviation is zero for every candidate — both
    // endpoints are held at sample positions — so this is the code's
    // normalising constant, identical for every path.
    let first_sample_bits =
        crate::code::independent_observations(samples[0].weight_ds, samples[0].corr_length_px)
            .unwrap_or(0.0)
            * crate::code::residual_bits(0.0, samples[0].halfwidth, precision);

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
        table.bits_per_segment_family()
            + f.flag_bits()
            + free_scalars(f, head, false) as f64 * cb
            + gap_bits
    };

    for &ei in &by_from[0] {
        let e = edges[ei];
        let g = seg_bits(e.family, false);
        let p = Partial {
            bits: g + e.residual_bits + first_sample_bits,
            geometry: g,
            topology: 0.0,
            residual: e.residual_bits + first_sample_bits,
            proposal: e.proposal_cost_px,
            edge: ei,
            smooth_here: false,
            prev: None,
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
                    let angle_free = smooth && tangent_is_free(f_in) && tangent_is_free(e.family);
                    let node_bits = join_bits
                        + anchor
                        + if smooth {
                            f64::from(u8::from(angle_free)) * cb - tail_saving * cb
                        } else {
                            0.0
                        };
                    let g = seg_bits(e.family, smooth);
                    for &pi in &partials {
                        let p = arena[pi];
                        let q = Partial {
                            bits: p.bits + g + node_bits + e.residual_bits,
                            geometry: p.geometry + g,
                            topology: p.topology + node_bits,
                            residual: p.residual + e.residual_bits,
                            proposal: p.proposal + e.proposal_cost_px,
                            edge: ei,
                            smooth_here: smooth,
                            prev: Some(pi),
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

    finished.sort_by(|a, b| arena[*a].bits.total_cmp(&arena[*b].bits));
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
            GrammarPath {
                candidates: walk.iter().map(|x| edges[x.edge].candidate).collect(),
                breakpoints: walk[1..].iter().map(|x| edges[x.edge].from).collect(),
                smooth: walk[1..].iter().map(|x| x.smooth_here).collect(),
                code: ChainCode {
                    geometry_bits: p.geometry,
                    topology_bits: p.topology,
                    relation_bits: 0.0,
                    residual_bits: p.residual,
                },
                proposal_cost_px: p.proposal,
            }
        })
        .collect()
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
    let idx = arena.len();
    arena.push(p);
    if e.to == n - 1 {
        finished.push(idx);
        return;
    }
    let key = StateKey {
        exit_class: e.exit_class,
        family_ord: family_ord(e.family),
        head_shared,
    };
    let v = slot.entry(key).or_default();
    v.push(idx);
    v.sort_by(|a, b| arena[*a].bits.total_cmp(&arena[*b].bits));
    v.truncate(k);
}

/// Turn a discrete path into the shared-parameter representation the joint
/// refit optimises.
///
/// The initial tangent angle at a smooth node is the MEAN of what the two
/// spans arrive and leave with — an initialisation, not a decision: the solve
/// moves it, and whatever it moves to, both incident control points follow it
/// because there is only one of it.
pub fn materialize(
    path: &GrammarPath,
    edges: &[GrammarEdge],
    candidates: &[SpanCandidate],
    samples: &[BoundarySample],
) -> Option<RefitChain> {
    let ids: Vec<usize> = path
        .candidates
        .iter()
        .map(|c| edges.iter().position(|e| e.candidate == *c))
        .collect::<Option<Vec<_>>>()?;
    let es: Vec<&GrammarEdge> = ids.iter().map(|i| &edges[*i]).collect();

    let mut nodes: Vec<RefitNode> = Vec::with_capacity(es.len() + 1);
    nodes.push(RefitNode {
        pos: samples[es[0].from].p,
        tangent_rad: None,
    });
    for (i, e) in es.iter().enumerate() {
        let smooth = i + 1 < es.len() && path.smooth.get(i).copied().unwrap_or(false);
        let tangent = if smooth {
            let a = e.exit_rad;
            let b = es[i + 1].entry_rad;
            Some(crate::refit::canonical_angle(
                a + crate::refit::canonical_angle(b - a) * 0.5,
            ))
        } else {
            None
        };
        nodes.push(RefitNode {
            pos: samples[e.to].p,
            tangent_rad: tangent,
        });
    }

    let mut segments = Vec::with_capacity(es.len());
    for (i, e) in es.iter().enumerate() {
        let head_shared = i > 0 && nodes[i].tangent_rad.is_some();
        let tail_shared = nodes[i + 1].tangent_rad.is_some();
        let cand = &candidates[e.candidate];
        let (p0, p1) = (samples[e.from].p, samples[e.to].p);
        segments.push(match cand.segment {
            vice_ir::Segment::Line => RefitSegment::Line,
            vice_ir::Segment::CircularArc {
                radius_px,
                large_arc,
                ccw,
            } => RefitSegment::Arc(if head_shared {
                ArcAnchor::FromHeadTangent
            } else if tail_shared {
                ArcAnchor::FromTailTangent
            } else {
                ArcAnchor::Radius {
                    radius_px,
                    large_arc,
                    ccw,
                }
            }),
            vice_ir::Segment::Quad { ctrl } => RefitSegment::Quad {
                ctrl: if head_shared {
                    Handle::Shared {
                        length_px: (ctrl - p0).length(),
                    }
                } else if tail_shared {
                    Handle::Shared {
                        length_px: (ctrl - p1).length(),
                    }
                } else {
                    Handle::Free(ctrl)
                },
            },
            vice_ir::Segment::Cubic { ctrl1, ctrl2 } => RefitSegment::Cubic {
                head: if head_shared {
                    Handle::Shared {
                        length_px: (ctrl1 - p0).length(),
                    }
                } else {
                    Handle::Free(ctrl1)
                },
                tail: if tail_shared {
                    Handle::Shared {
                        length_px: (ctrl2 - p1).length(),
                    }
                } else {
                    Handle::Free(ctrl2)
                },
            },
            vice_ir::Segment::EllipticArc { .. } => return None,
        });
    }
    Some(RefitChain { nodes, segments })
}

/// Discrete paths the shared-tangent representation cannot carry.
///
/// Two shapes, each for a reason about the representation rather than about
/// the evidence:
///
/// - a QUADRATIC smooth at both ends: its single control point is then the
///   intersection of the two tangent lines, which is not a handle length;
/// - a smooth join between two LINES: their directions are their chords, so
///   there is no shared parameter, and the join is G1 only when the two chords
///   are collinear.
pub fn path_is_representable(path: &GrammarPath, families: &[SpanFamily]) -> bool {
    let quad_ok = families.iter().enumerate().all(|(i, f)| {
        let head = i > 0 && path.smooth.get(i - 1).copied().unwrap_or(false);
        let tail = path.smooth.get(i).copied().unwrap_or(false);
        !(matches!(f, SpanFamily::Quad) && head && tail)
    });
    let lines_ok = path.smooth.iter().enumerate().all(|(i, s)| {
        !(*s && matches!(families.get(i), Some(SpanFamily::Line))
            && matches!(families.get(i + 1), Some(SpanFamily::Line)))
    });
    quad_ok && lines_ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jet_classes_wrap_and_neighbours_are_compatible() {
        assert_eq!(jet_class(0.0), 0);
        assert_eq!(jet_class(std::f64::consts::TAU), 0);
        assert!(jet_compatible(0, JET_CLASSES - 1), "the wrap is compatible");
        assert!(jet_compatible(5, 6));
        assert!(!jet_compatible(5, 7));
        // A right angle is never compatible, at any rotation.
        for a in 0..JET_CLASSES {
            let b = (a + JET_CLASSES / 4) % JET_CLASSES;
            assert!(!jet_compatible(a, b), "{a} and {b}");
        }
    }

    /// The exact scalar counts, family by family, in both directions.
    #[test]
    fn sharing_a_tangent_removes_exactly_the_scalars_it_determines() {
        use SpanFamily::*;
        assert_eq!(free_scalars(Line, false, false), 0);
        assert_eq!(free_scalars(Line, true, true), 0);
        assert_eq!(free_scalars(CircularArc, false, false), 1);
        assert_eq!(free_scalars(CircularArc, true, false), 0);
        assert_eq!(free_scalars(CircularArc, true, true), 0);
        assert_eq!(free_scalars(Quad, false, false), 2);
        assert_eq!(free_scalars(Quad, true, false), 1);
        assert_eq!(free_scalars(Quad, true, true), 0);
        assert_eq!(free_scalars(Cubic, false, false), 4);
        assert_eq!(free_scalars(Cubic, true, false), 3);
        assert_eq!(free_scalars(Cubic, true, true), 2);
    }
}
