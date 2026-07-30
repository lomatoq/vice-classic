//! Typed curve grammar (spec v1.3 §6.2).
//!
//! A [`CurveChain`] is the geometry of ONE shared boundary. Shared
//! parameters are stored exactly once (spec §5.5):
//! - the chain's two ENDPOINTS live in the graph vertices referenced by the
//!   owning [`crate::graph::Boundary`] — they are never duplicated here;
//! - interior nodes between segments are stored once in `interior_nodes`;
//! - a smooth join stores its shared tangent parameter once, at the node;
//! - a self-loop's repeated-endpoint join lives once on its owning
//!   [`crate::graph::Boundary`].
//!
//! Exact G1 arises from parameterization/joint solve (M6); M1 carries the
//! shared tangent parameter in the types but does not yet enforce
//! tangent/geometry consistency (documented limitation, ADR-0005).

use serde::{Deserialize, Serialize};
use vice_geom::{Aabb, Pt};

/// Join classification at a chain node.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum JoinKind {
    /// Free corner: incident segments meet with independent tangents.
    Corner,
    /// G1-smooth node. The tangent direction is the SHARED parameter of
    /// both incident segments, stored once. Canonical range: `(-pi, pi]`
    /// radians, measured in the frame (x right, y down), atan2 convention.
    SmoothG1 { tangent_angle_rad: f64 },
}

/// Interior node of a chain: a position plus the join at that node.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChainNode {
    pub pos: Pt,
    pub join: JoinKind,
}

/// One typed span between two consecutive chain points. Endpoints are NOT
/// stored in the segment (they are shared chain/graph parameters); only the
/// segment-interior parameters live here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Segment {
    /// Straight line between the two adjacent chain points.
    Line,
    /// Circular arc between the two adjacent chain points, endpoint
    /// parameterization: positive radius, arc-choice flags (analogous to
    /// the SVG arc flags). Requires `2 * radius_px >= chord length`.
    CircularArc {
        radius_px: f64,
        large_arc: bool,
        /// Sweep direction in the ALGEBRAIC sense (counterclockwise with
        /// y up); on screen (y down) `ccw = true` looks clockwise.
        ccw: bool,
    },
    /// Elliptic arc between the two adjacent chain points, endpoint
    /// parameterization. `x_axis_rotation_rad` canonical range: `[0, pi)`.
    EllipticArc {
        rx_px: f64,
        ry_px: f64,
        x_axis_rotation_rad: f64,
        large_arc: bool,
        ccw: bool,
    },
    /// Quadratic Bezier with one absolute control point.
    Quad { ctrl: Pt },
    /// Cubic Bezier with two absolute control points.
    Cubic { ctrl1: Pt, ctrl2: Pt },
}

impl Segment {
    /// A CERTIFIED conservative axis-aligned enclosure of the segment with
    /// endpoints `p0`, `p1`.
    ///
    /// - Line/Quad/Cubic: control-polygon box (convex-hull property, exact).
    /// - CircularArc: endpoint box inflated outward by `2 * radius_px`
    ///   (every point of a circle is within `2r` of any other point of the
    ///   same circle), with a one-ulp outward guard against rounding.
    /// - EllipticArc: endpoint box inflated outward by `2 * max(rx, ry)`.
    ///
    /// These enclosures may be very loose; they are only used as
    /// NON-intersection certificates (disjoint boxes ⇒ disjoint segments),
    /// never as intersection proofs.
    pub fn conservative_enclosure(&self, p0: Pt, p1: Pt) -> Aabb {
        let ends = Aabb::from_points(&[p0, p1]).expect("two points");
        match *self {
            Segment::Line => ends,
            Segment::Quad { ctrl } => Aabb::from_points(&[p0, p1, ctrl]).expect("points"),
            Segment::Cubic { ctrl1, ctrl2 } => {
                Aabb::from_points(&[p0, p1, ctrl1, ctrl2]).expect("points")
            }
            Segment::CircularArc { radius_px, .. } => ends.inflate_outward(2.0 * radius_px),
            Segment::EllipticArc { rx_px, ry_px, .. } => {
                ends.inflate_outward(2.0 * rx_px.max(ry_px))
            }
        }
    }

    pub fn is_line(&self) -> bool {
        matches!(self, Segment::Line)
    }
}

/// The geometry of one shared boundary: `segments.len()` typed spans with
/// `segments.len() - 1` interior nodes between them. Endpoints come from
/// the owning boundary's start/end graph vertices.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurveChain {
    pub interior_nodes: Vec<ChainNode>,
    pub segments: Vec<Segment>,
}

impl CurveChain {
    /// A single-segment chain (no interior nodes).
    pub fn single(segment: Segment) -> CurveChain {
        CurveChain {
            interior_nodes: Vec::new(),
            segments: vec![segment],
        }
    }

    /// All chain point positions in order: `[start, interior..., end]`.
    /// `start`/`end` are the owning boundary's vertex positions.
    pub fn node_positions(&self, start: Pt, end: Pt) -> Vec<Pt> {
        let mut out = Vec::with_capacity(self.interior_nodes.len() + 2);
        out.push(start);
        out.extend(self.interior_nodes.iter().map(|n| n.pos));
        out.push(end);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_positions_orders_start_interior_end() {
        let chain = CurveChain {
            interior_nodes: vec![ChainNode {
                pos: Pt::new(1.0, 1.0),
                join: JoinKind::Corner,
            }],
            segments: vec![Segment::Line, Segment::Line],
        };
        assert_eq!(
            chain.node_positions(Pt::new(0.0, 0.0), Pt::new(2.0, 0.0)),
            vec![Pt::new(0.0, 0.0), Pt::new(1.0, 1.0), Pt::new(2.0, 0.0)]
        );
    }

    #[test]
    fn bezier_enclosures_use_control_polygon() {
        let p0 = Pt::new(0.0, 0.0);
        let p1 = Pt::new(4.0, 0.0);
        let q = Segment::Quad {
            ctrl: Pt::new(2.0, 3.0),
        };
        let b = q.conservative_enclosure(p0, p1);
        assert_eq!(b.min(), Pt::new(0.0, 0.0));
        assert_eq!(b.max(), Pt::new(4.0, 3.0));

        let c = Segment::Cubic {
            ctrl1: Pt::new(-1.0, 1.0),
            ctrl2: Pt::new(5.0, -2.0),
        };
        let bc = c.conservative_enclosure(p0, p1);
        assert_eq!(bc.min(), Pt::new(-1.0, -2.0));
        assert_eq!(bc.max(), Pt::new(5.0, 1.0));
    }

    #[test]
    fn arc_enclosures_cover_the_whole_circle_conservatively() {
        let p0 = Pt::new(0.0, 0.0);
        let p1 = Pt::new(2.0, 0.0);
        let arc = Segment::CircularArc {
            radius_px: 1.0,
            large_arc: true,
            ccw: true,
        };
        let b = arc.conservative_enclosure(p0, p1);
        // Circle through (0,0),(2,0) with r=1 is centered at (1,0); the
        // farthest circle point from the endpoint box is at distance <= 2r.
        assert!(b.contains(Pt::new(1.0, 1.0)));
        assert!(b.contains(Pt::new(1.0, -1.0)));
        assert!(b.min().x <= -2.0 && b.max().x >= 4.0);
    }
}
