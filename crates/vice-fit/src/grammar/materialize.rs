use super::*;

pub fn materialize(
    path: &GrammarPath,
    edges: &[GrammarEdge],
    candidates: &[SpanCandidate],
    samples: &[BoundarySample],
) -> Option<RefitChain> {
    if path.candidates.is_empty()
        || path.breakpoints.len() + 1 != path.candidates.len()
        || path.smooth.len() != path.breakpoints.len()
        || (path.closure_smooth && !path.closed)
    {
        return None;
    }
    materialize_with_closure(path, edges, candidates, samples)
}

/// Materialize one path and, when requested, make the repeated endpoint of a
/// closed chain read the same tangent parameter at both incident segments.
pub(crate) fn materialize_with_closure(
    path: &GrammarPath,
    edges: &[GrammarEdge],
    candidates: &[SpanCandidate],
    samples: &[BoundarySample],
) -> Option<RefitChain> {
    let closure_smooth = path.closure_smooth;
    let ids: Vec<usize> = path
        .candidates
        .iter()
        .map(|c| edges.iter().position(|e| e.candidate == *c))
        .collect::<Option<Vec<_>>>()?;
    if ids.is_empty() {
        return None;
    }
    let es: Vec<&GrammarEdge> = ids.iter().map(|i| &edges[*i]).collect();
    if es.iter().enumerate().any(|(i, edge)| {
        edge.from >= edge.to
            || edge.to >= samples.len()
            || edge.candidate >= candidates.len()
            || validate_candidate(&candidates[edge.candidate], edge.candidate, samples).is_err()
            || candidates[edge.candidate].support.lo() != edge.from
            || candidates[edge.candidate].support.hi() != edge.to
            || candidates[edge.candidate].family != edge.family
            || (i > 0 && es[i - 1].to != edge.from)
            || (i > 0 && path.breakpoints[i - 1] != edge.from)
    }) || es.first()?.from != 0
        || es.last()?.to + 1 != samples.len()
    {
        return None;
    }

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
    if closure_smooth {
        if nodes.first()?.pos != nodes.last()?.pos || es.len() < 2 {
            return None;
        }
        let arrive = es.last()?.exit_rad;
        let leave = es.first()?.entry_rad;
        let shared = crate::refit::canonical_angle(
            arrive + crate::refit::canonical_angle(leave - arrive) * 0.5,
        );
        nodes.first_mut()?.tangent_rad = Some(shared);
        nodes.last_mut()?.tangent_rad = Some(shared);
    }

    let mut segments = Vec::with_capacity(es.len());
    for (i, e) in es.iter().enumerate() {
        let head_shared = nodes[i].tangent_rad.is_some();
        let tail_shared = nodes[i + 1].tangent_rad.is_some();
        let cand = &candidates[e.candidate];
        let (p0, p1) = (samples[e.from].p, samples[e.to].p);
        segments.push(match cand.segment {
            vice_ir::Segment::Line => RefitSegment::Line,
            vice_ir::Segment::CircularArc {
                radius_px,
                large_arc,
                ccw,
            } => RefitSegment::Arc(if head_shared && tail_shared {
                // Not representable: an anchor reads one end (RT6-A1). The
                // caller filters via `path_is_representable`; this is the
                // defence for callers that do not.
                return None;
            } else if head_shared {
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
            vice_ir::Segment::Quad { ctrl } => {
                if tail_shared {
                    // The representation anchors a quad's control point at its
                    // HEAD node only; a tail-smooth quad would not read the
                    // node that claims it (RT6-A1's class). Not representable.
                    return None;
                }
                RefitSegment::Quad {
                    ctrl: if head_shared {
                        Handle::Shared {
                            length_px: (ctrl - p0).length(),
                        }
                    } else {
                        Handle::Free(ctrl)
                    },
                }
            }
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
/// Per shape, the reason is about the REPRESENTATION rather than the evidence,
/// and the list mirrors `RefitChain::end_reads_node` — a segment end that does
/// not read its smooth node's angle would leave the declaration unbound
/// (RT6-A1):
///
/// - a QUADRATIC smooth at its tail: its one control point is anchored at the
///   head node, so a tail-smooth quad reads nothing at the node that claims it
///   (smooth at both ends is the special case where the control point would be
///   the intersection of two tangent lines, which is not a handle length);
/// - an ARC smooth at BOTH ends: an anchor reads exactly one end, and the
///   arrival direction at the other is determined by the circle, not by the
///   node — the configuration the red team drove to a 4.224 deg accepted G1
///   violation through this very function;
/// - a smooth join between two LINES: their directions are their chords, and
///   two collinear lines are one line.
pub fn path_is_representable(path: &GrammarPath, families: &[SpanFamily]) -> bool {
    closure::path_is_representable(path, families)
}
