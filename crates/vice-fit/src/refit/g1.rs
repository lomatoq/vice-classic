//! Independent readings of G1 from lowered canonical geometry.

use serde::Serialize;
use vice_geom::Pt;
use vice_ir::{CurveChain, JoinKind, Segment};

/// Fold an angle into `vice_ir`'s canonical `(-pi, pi]`.
pub fn canonical_angle(angle: f64) -> f64 {
    let two_pi = std::f64::consts::TAU;
    let mut canonical = angle % two_pi;
    if canonical <= -std::f64::consts::PI {
        canonical += two_pi;
    }
    if canonical > std::f64::consts::PI {
        canonical -= two_pi;
    }
    canonical
}

/// What one node of a lowered chain says about its own G1.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct G1Reading {
    pub interior_node: usize,
    pub arrive_rad: f64,
    pub leave_rad: f64,
    pub declared_rad: f64,
    pub spread_rad: f64,
}

/// Measure every `SmoothG1` node from absolute lowered control points.
pub fn g1_readings(chain: &CurveChain, start: Pt, end: Pt) -> Vec<G1Reading> {
    let points = chain.node_positions(start, end);
    let mut readings = Vec::new();
    for (index, node) in chain.interior_nodes.iter().enumerate() {
        let JoinKind::SmoothG1 { tangent_angle_rad } = node.join else {
            continue;
        };
        let arrive = arrive_dir(&chain.segments[index], points[index], points[index + 1]);
        let leave = leave_dir(
            &chain.segments[index + 1],
            points[index + 1],
            points[index + 2],
        );
        let (Some(arrive), Some(leave)) = (arrive, leave) else {
            continue;
        };
        let arrive_rad = arrive.y.atan2(arrive.x);
        let leave_rad = leave.y.atan2(leave.x);
        let delta = |a: f64, b: f64| canonical_angle(a - b).abs();
        readings.push(G1Reading {
            interior_node: index,
            arrive_rad,
            leave_rad,
            declared_rad: tangent_angle_rad,
            spread_rad: delta(arrive_rad, leave_rad)
                .max(delta(arrive_rad, tangent_angle_rad))
                .max(delta(leave_rad, tangent_angle_rad)),
        });
    }
    readings
}

/// Measure the implicit last-to-first join of a closed chain.
pub fn closure_g1_spread_rad(
    chain: &CurveChain,
    start: Pt,
    end: Pt,
    declared_rad: f64,
) -> Option<f64> {
    if chain.segments.len() < 2 || start != end {
        return None;
    }
    let points = chain.node_positions(start, end);
    let last = chain.segments.len() - 1;
    let arrive = arrive_dir(&chain.segments[last], points[last], points[last + 1])?;
    let leave = leave_dir(&chain.segments[0], points[0], points[1])?;
    let arrive_rad = arrive.y.atan2(arrive.x);
    let leave_rad = leave.y.atan2(leave.x);
    let delta = |a: f64, b: f64| canonical_angle(a - b).abs();
    Some(
        delta(arrive_rad, leave_rad)
            .max(delta(arrive_rad, declared_rad))
            .max(delta(leave_rad, declared_rad)),
    )
}

fn nonzero(vector: Pt) -> Option<Pt> {
    (vector.length_sq() > 0.0 && vector.is_finite()).then_some(vector)
}

fn arrive_dir(segment: &Segment, p0: Pt, p1: Pt) -> Option<Pt> {
    match *segment {
        Segment::Line => nonzero(p1 - p0),
        Segment::Quad { ctrl } => nonzero(p1 - ctrl).or_else(|| nonzero(p1 - p0)),
        Segment::Cubic { ctrl2, .. } => nonzero(p1 - ctrl2).or_else(|| nonzero(p1 - p0)),
        Segment::CircularArc {
            radius_px,
            large_arc,
            ccw,
        } => {
            let center = vice_geom::flatten::circular_arc_center(p0, p1, radius_px, large_arc, ccw)
                .ok()?
                .center;
            let radius = p1 - center;
            nonzero(if ccw {
                Pt::new(-radius.y, radius.x)
            } else {
                Pt::new(radius.y, -radius.x)
            })
        }
        Segment::EllipticArc { .. } => None,
    }
}

fn leave_dir(segment: &Segment, p0: Pt, p1: Pt) -> Option<Pt> {
    match *segment {
        Segment::Line => nonzero(p1 - p0),
        Segment::Quad { ctrl } => nonzero(ctrl - p0).or_else(|| nonzero(p1 - p0)),
        Segment::Cubic { ctrl1, .. } => nonzero(ctrl1 - p0).or_else(|| nonzero(p1 - p0)),
        Segment::CircularArc {
            radius_px,
            large_arc,
            ccw,
        } => {
            let center = vice_geom::flatten::circular_arc_center(p0, p1, radius_px, large_arc, ccw)
                .ok()?
                .center;
            let radius = p0 - center;
            nonzero(if ccw {
                Pt::new(-radius.y, radius.x)
            } else {
                Pt::new(radius.y, -radius.x)
            })
        }
        Segment::EllipticArc { .. } => None,
    }
}
