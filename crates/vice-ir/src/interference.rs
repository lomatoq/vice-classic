//! Geometric interference between boundary segments (M1/M2 scope: exact
//! for line pairs, conservative-box certificates otherwise).
//!
//! Moved verbatim out of `validate.rs` (which had grown past the 800-LOC
//! module rule, spec §4.1) — behaviour is unchanged; `validate_graph` calls
//! [`check_segment_interference`] exactly as before.

use vice_geom::predicates::{closed_segments_intersect, shared_endpoint_segments_overlap};
use vice_geom::Pt;

use crate::curve::Segment;
use crate::graph::{BoundaryId, PlanarGraph};
use crate::validate::GraphError;

pub(crate) struct PrimSeg<'a> {
    boundary: usize,
    segment: usize,
    p0: Pt,
    p1: Pt,
    seg: &'a Segment,
}

pub(crate) fn collect_prims(g: &PlanarGraph) -> Vec<PrimSeg<'_>> {
    let mut prims = Vec::new();
    for (bi, b) in g.boundaries.iter().enumerate() {
        let pts = b.curve.node_positions(
            g.vertices[b.start_vertex.index()].pos,
            g.vertices[b.end_vertex.index()].pos,
        );
        for (si, s) in b.curve.segments.iter().enumerate() {
            prims.push(PrimSeg {
                boundary: bi,
                segment: si,
                p0: pts[si],
                p1: pts[si + 1],
                seg: s,
            });
        }
    }
    prims
}

/// Shared chain points of two primitive segments.
///
/// SOUNDNESS (REVIEW_M1 M1-N1): position equality here is point IDENTITY,
/// not a heuristic, because `check_chain_point_distinctness` has already
/// rejected any two distinct chain points with equal positions. A shared
/// position therefore always means a genuinely shared node: a common graph
/// vertex, or the shared interior node / loop vertex of consecutive
/// segments within one chain. Two non-adjacent boundaries touching at a
/// coincident-but-distinct node never reach this code — they are rejected
/// earlier as `UnrepresentedJunction`.
fn shared_points(a: &PrimSeg<'_>, b: &PrimSeg<'_>) -> Vec<Pt> {
    [a.p0, a.p1]
        .into_iter()
        .filter(|p| *p == b.p0 || *p == b.p1)
        .collect()
}

pub(crate) fn check_segment_interference(g: &PlanarGraph) -> Result<(), GraphError> {
    let prims = collect_prims(g);
    for i in 0..prims.len() {
        for j in (i + 1)..prims.len() {
            let (a, b) = (&prims[i], &prims[j]);
            if !(a.seg.is_line() && b.seg.is_line()) {
                // Non-line pairs are handled by the certificate split in
                // [`uncertified_interference_pairs`]; M1 cannot exactly
                // reject them (M2+ machinery), only certify disjointness.
                continue;
            }
            match shared_points(a, b).as_slice() {
                [] => {
                    if closed_segments_intersect(a.p0, a.p1, b.p0, b.p1) {
                        return Err(GraphError::BoundariesIntersect {
                            a: BoundaryId(a.boundary as u32),
                            sa: a.segment,
                            b: BoundaryId(b.boundary as u32),
                            sb: b.segment,
                        });
                    }
                }
                [s] => {
                    // Exact collinear-overlap check beyond the shared point.
                    let ea = if a.p0 == *s { a.p1 } else { a.p0 };
                    let eb = if b.p0 == *s { b.p1 } else { b.p0 };
                    if shared_endpoint_segments_overlap(*s, ea, eb) {
                        return Err(GraphError::CollinearOverlap {
                            a: BoundaryId(a.boundary as u32),
                            sa: a.segment,
                            b: BoundaryId(b.boundary as u32),
                            sb: b.segment,
                        });
                    }
                }
                _ => {
                    // Two LINE segments with the same endpoint set are the
                    // same segment: full overlap.
                    return Err(GraphError::CollinearOverlap {
                        a: BoundaryId(a.boundary as u32),
                        sa: a.segment,
                        b: BoundaryId(b.boundary as u32),
                        sb: b.segment,
                    });
                }
            }
        }
    }
    Ok(())
}

/// A segment pair whose non-intersection M1 can NEITHER prove NOR refute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UncertifiedPair {
    pub boundary_a: BoundaryId,
    pub segment_a: usize,
    pub boundary_b: BoundaryId,
    pub segment_b: usize,
}

/// The honest M1 certificate split for the §12 invariant "non-adjacent
/// boundaries do not intersect".
///
/// Returns every NON-adjacent (no shared chain point — identity, see
/// [`shared_points`]) segment pair involving at least one non-line segment
/// whose conservative enclosures overlap. For such pairs M1 has no exact
/// intersection test: they are recorded as UNDETERMINED, not silently
/// assumed disjoint. Line-line pairs are decided exactly during validation
/// and never appear here. The list is the M2+ worklist for certified
/// curve-curve intersection.
///
/// Precondition: `g` passed [`crate::validate::validate_graph`] — in
/// particular the chain-point distinctness invariant, which makes the
/// adjacency test exact (ids and arities are trusted too).
pub fn uncertified_interference_pairs(g: &PlanarGraph) -> Vec<UncertifiedPair> {
    let prims = collect_prims(g);
    let mut out = Vec::new();
    for i in 0..prims.len() {
        for j in (i + 1)..prims.len() {
            let (a, b) = (&prims[i], &prims[j]);
            if a.seg.is_line() && b.seg.is_line() {
                continue; // decided exactly by validation
            }
            if !shared_points(a, b).is_empty() {
                continue; // adjacent pair: outside the §12 non-adjacent invariant
            }
            let boxes_disjoint = a
                .seg
                .conservative_enclosure(a.p0, a.p1)
                .strictly_disjoint(&b.seg.conservative_enclosure(b.p0, b.p1));
            if !boxes_disjoint {
                out.push(UncertifiedPair {
                    boundary_a: BoundaryId(a.boundary as u32),
                    segment_a: a.segment,
                    boundary_b: BoundaryId(b.boundary as u32),
                    segment_b: b.segment,
                });
            }
        }
    }
    out
}
