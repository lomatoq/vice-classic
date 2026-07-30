//! Canonical IR to shared-parameter lifting for the G30 recovery intervention.

use vice_geom::Pt;
use vice_ir::{CurveChain, JoinKind, Segment};

use crate::refit::{
    canonical_angle, ArcAnchor, Handle, RefitChain, RefitNode, RefitRefusal, RefitSegment,
};

/// Lift one canonical IR boundary into the shared-parameter representation.
///
/// Smooth joins remain shared by construction. An IR combination that the
/// fitter cannot encode is refused instead of being silently cornered.
pub fn refit_chain_from_ir(
    start: Pt,
    end: Pt,
    curve: &CurveChain,
    closure_join: Option<JoinKind>,
    forward: bool,
) -> Result<RefitChain, RefitRefusal> {
    if curve.segments.is_empty()
        || curve.interior_nodes.len() + 1 != curve.segments.len()
        || !start.is_finite()
        || !end.is_finite()
    {
        return Err(RefitRefusal::Malformed);
    }
    let mut points = curve.node_positions(start, end);
    let mut joins = curve
        .interior_nodes
        .iter()
        .map(|node| node.join)
        .collect::<Vec<_>>();
    let mut segments = curve.segments.clone();
    if !forward {
        points.reverse();
        joins.reverse();
        joins = joins.into_iter().map(reverse_join).collect();
        segments = segments.into_iter().rev().map(reverse_segment).collect();
    }
    let closure_tangent =
        closure_join.and_then(
            |join| match if forward { join } else { reverse_join(join) } {
                JoinKind::Corner => None,
                JoinKind::SmoothG1 { tangent_angle_rad } => {
                    Some(canonical_angle(tangent_angle_rad))
                }
            },
        );
    if closure_tangent.is_some() && points.first() != points.last() {
        return Err(RefitRefusal::Malformed);
    }
    let nodes = points
        .iter()
        .enumerate()
        .map(|(index, &pos)| {
            let tangent_rad = if index == 0 || index + 1 == points.len() {
                closure_tangent
            } else {
                match joins[index - 1] {
                    JoinKind::Corner => None,
                    JoinKind::SmoothG1 { tangent_angle_rad } => {
                        Some(canonical_angle(tangent_angle_rad))
                    }
                }
            };
            RefitNode { pos, tangent_rad }
        })
        .collect::<Vec<_>>();
    let mut lifted = Vec::with_capacity(segments.len());
    for (index, segment) in segments.into_iter().enumerate() {
        let p0 = nodes[index].pos;
        let p1 = nodes[index + 1].pos;
        let head_shared = nodes[index].tangent_rad.is_some();
        let tail_shared = nodes[index + 1].tangent_rad.is_some();
        lifted.push(match segment {
            Segment::Line => RefitSegment::Line,
            Segment::CircularArc {
                radius_px,
                large_arc,
                ccw,
            } => RefitSegment::Arc(if head_shared && tail_shared {
                return Err(RefitRefusal::SmoothNodeUnread {
                    node: index,
                    segment: index,
                });
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
            Segment::Quad { ctrl } => {
                if tail_shared {
                    return Err(RefitRefusal::SmoothNodeUnread {
                        node: index + 1,
                        segment: index,
                    });
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
            Segment::Cubic { ctrl1, ctrl2 } => RefitSegment::Cubic {
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
            Segment::EllipticArc { .. } => return Err(RefitRefusal::Malformed),
        });
    }
    let chain = RefitChain {
        nodes,
        segments: lifted,
    };
    chain.lower_boundary_geometry()?;
    Ok(chain)
}

fn reverse_join(join: JoinKind) -> JoinKind {
    match join {
        JoinKind::Corner => JoinKind::Corner,
        JoinKind::SmoothG1 { tangent_angle_rad } => JoinKind::SmoothG1 {
            tangent_angle_rad: canonical_angle(tangent_angle_rad + std::f64::consts::PI),
        },
    }
}

fn reverse_segment(segment: Segment) -> Segment {
    match segment {
        Segment::Line => Segment::Line,
        Segment::CircularArc {
            radius_px,
            large_arc,
            ccw,
        } => Segment::CircularArc {
            radius_px,
            large_arc,
            ccw: !ccw,
        },
        Segment::EllipticArc {
            rx_px,
            ry_px,
            x_axis_rotation_rad,
            large_arc,
            ccw,
        } => Segment::EllipticArc {
            rx_px,
            ry_px,
            x_axis_rotation_rad,
            large_arc,
            ccw: !ccw,
        },
        Segment::Quad { ctrl } => Segment::Quad { ctrl },
        Segment::Cubic { ctrl1, ctrl2 } => Segment::Cubic {
            ctrl1: ctrl2,
            ctrl2: ctrl1,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cornered_canonical_chain_round_trips_through_shared_parameters() {
        let curve = CurveChain {
            interior_nodes: vec![vice_ir::ChainNode {
                pos: Pt::new(1.0, 0.0),
                join: JoinKind::Corner,
            }],
            segments: vec![
                Segment::Line,
                Segment::Quad {
                    ctrl: Pt::new(1.5, 1.0),
                },
            ],
        };
        let lifted =
            refit_chain_from_ir(Pt::new(0.0, 0.0), Pt::new(2.0, 0.0), &curve, None, true).unwrap();
        assert_eq!(lifted.lower().unwrap(), curve);
    }
}
