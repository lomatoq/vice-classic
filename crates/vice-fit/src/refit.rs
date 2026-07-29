//! §14.3 / §24: the joint constrained chain refit, and **exact G1 by
//! representation**.
//!
//! ## The decision this module makes, and the price of the one not taken
//!
//! §14.3: "Проверка `angle < tolerance` сама по себе **не является G1**." So a
//! tolerance is not available, and there are exactly two ways to have the
//! property:
//!
//! - **as a CHECK** — store geometry and a declared tangent independently and
//!   compare them. This is what `vice_ir::CurveChain` does today, and C294
//!   measured what it is worth: on `vice-ir`'s own canonical VALID fixture a
//!   node typed `SmoothG1` arrives at −14.04°, leaves at 0.00° and declares
//!   +14.32° — a spread of **28.36°**, valid since M1, because `Quad`/`Cubic`
//!   store ABSOLUTE control points and nothing compares them with the
//!   declaration. Making the check real means a tolerance, which §14.3 forbids,
//!   or exact equality of two independently-rounded floats, which is not
//!   achievable.
//! - **as a REPRESENTATION** — store the tangent once and DERIVE both control
//!   points from it, so the disagreement has nowhere to be written down.
//!
//! **This module takes the representation.** [`RefitChain`] has one angle per
//! smooth node and no second copy: [`Handle::Shared`] stores a LENGTH, and the
//! direction comes from the node. G1 at a smooth join is then a property of the
//! type, and a corner is the deliberate absence of sharing.
//!
//! **The price of the choice taken**, stated rather than left to be found:
//! `vice_ir::CurveChain` is unchanged, so it can still express an inconsistent
//! chain, and anything that constructs one outside this module can still write
//! the disagreement down. What is closed is the PRODUCER — [`RefitChain::lower`]
//! is the only path from Stage G to the IR, and the residual it leaves is the
//! floating-point round trip through absolute control points, which is MEASURED
//! (`refit_holds_g1_where_the_ir_fixture_does_not`) rather than assumed
//! negligible.
//!
//! **The price of the choice NOT taken** — making `vice_ir::Segment` store
//! handle lengths instead of absolute control points, which would give the IR
//! itself the property: every serialized scene, the M1 canonical serialization
//! and its digests, the renderer, the validator and `model_universe_hash` all
//! move, and every signed artifact in the repository is re-recorded. That is a
//! §1.5 model-version change with full recalibration, and it is a milestone of
//! its own, not a step inside this one.
//!
//! ## What "joint" means here
//!
//! One parameter vector for the WHOLE chain — node positions, node tangent
//! angles, handle lengths, free corner control points and arc radii — optimised
//! together against the samples. Not per segment: §24's `joint_constrained_refit`
//! exists because a per-segment fit cannot move a shared node, and moving shared
//! nodes is most of what a refit does.

use serde::Serialize;
use vice_geom::Pt;
use vice_ir::{ChainNode, CurveChain, JoinKind, Segment};

/// A control point of a Bezier, at one end of one segment.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum Handle {
    /// This end is a CORNER: the control point is free and stored absolutely.
    Free(Pt),
    /// This end is SMOOTH: the control point is `node.pos ± dir(node.tangent) *
    /// length_px`, and `node.tangent` lives at the node. **There is no second
    /// copy of the direction here, which is the whole mechanism.**
    Shared { length_px: f64 },
}

/// How a circular arc is pinned.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum ArcAnchor {
    /// Free radius and arc flags.
    Radius {
        radius_px: f64,
        large_arc: bool,
        ccw: bool,
    },
    /// The unique circle through both endpoints TANGENT to the head node's
    /// shared direction. The radius is not stored because it is not free: a
    /// circle through two points with a prescribed tangent at one of them is
    /// determined.
    FromHeadTangent,
    /// The same, from the tail node's shared direction.
    FromTailTangent,
}

/// One span of a refit chain. Endpoints come from the adjacent nodes.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum RefitSegment {
    Line,
    Arc(ArcAnchor),
    Quad { ctrl: Handle },
    Cubic { head: Handle, tail: Handle },
}

/// A node of a refit chain.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct RefitNode {
    pub pos: Pt,
    /// `Some(angle)` at a smooth node: the ONE tangent parameter both incident
    /// segments read. `None` at a corner.
    pub tangent_rad: Option<f64>,
}

/// A chain in the shared-parameter representation.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RefitChain {
    /// `segments.len() + 1` nodes, ends included.
    pub nodes: Vec<RefitNode>,
    pub segments: Vec<RefitSegment>,
}

/// Why a chain could not be refitted or lowered.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "refit_refusal", rename_all = "snake_case")]
pub enum RefitRefusal {
    /// Fewer segments than nodes minus one, or no segment at all.
    Malformed,
    /// A segment's two endpoints coincide, so it has no direction.
    DegenerateSpan { segment: usize },
    /// An arc pinned by a tangent that is parallel to its own chord: the circle
    /// through both points tangent to that direction is a straight line, and a
    /// line is a different family.
    ArcIsALine { segment: usize },
    /// A non-finite parameter reached the lowering.
    NonFinite { segment: usize },
    /// A smooth join between two LINES. Their directions are their chords, so
    /// the join is G1 only when the two chords are collinear — and two
    /// collinear lines are one line. There is no shared parameter to store, so
    /// the grammar is refused rather than lowered into a node whose declaration
    /// can agree with at most one side.
    SmoothJoinBetweenTwoLines { node: usize },
    /// A smooth node whose declared tangent one of its incident segments does
    /// not READ — so the declaration and that segment's geometry would be two
    /// independent values, which is exactly the disagreement this
    /// representation exists to make unwritable.
    ///
    /// **This variant is RT6-A1's closure, and the history is the reason it is
    /// a CLASS check rather than another special case.** F-0087 found the hole
    /// for `Line` (its direction is its chord) and was closed by deriving the
    /// angle from the line — an ADDRESS. The red team then measured the same
    /// hole one family over: an arc anchored `FromHeadTangent` reads only its
    /// HEAD node, so an arc smooth at BOTH ends left the tail node's declared
    /// tangent a free parameter, and the standard pipeline produced accepted
    /// models with a G1 spread of 4.224 deg — seven orders over the gate line —
    /// while the DP priced the broken variant identically to the honest ones
    /// (`free_scalars(Arc, true, true) = 0`: the cost function already knew the
    /// tail constraint was not held). The class is "geometry that does not read
    /// the declared tangent at its own end", and this refusal enumerates the
    /// READERS per family and end, exhaustively, so a new family without a
    /// reader is refused rather than silently unbound.
    SmoothNodeUnread { node: usize, segment: usize },
    /// §14.3: exact G1 holds by construction, but the refit could not bring the
    /// chain inside the evidence corridor, so this discrete path is INVALID and
    /// the next one is considered. Carries what it reached.
    OutsideCorridor {
        worst_normal_deviation_px: f64,
        allowed_px: f64,
    },
}

fn dir(rad: f64) -> Pt {
    Pt::new(rad.cos(), rad.sin())
}

/// The circle through `p0` and `p1` tangent to `u` at `p0`, as
/// `(radius, large_arc, ccw)`. `None` when `u` is parallel to the chord.
fn arc_from_tangent(p0: Pt, p1: Pt, u: Pt) -> Option<(f64, bool, bool)> {
    let m = p1 - p0;
    let n = Pt::new(-u.y, u.x);
    let denom = 2.0 * m.dot(n);
    if denom == 0.0 || !denom.is_finite() {
        return None;
    }
    let signed_r = m.length_sq() / denom;
    if !signed_r.is_finite() || signed_r == 0.0 {
        return None;
    }
    let centre = p0 + n * signed_r;
    let ang = |p: Pt| (p.y - centre.y).atan2(p.x - centre.x);
    let two_pi = std::f64::consts::TAU;
    let norm = |a: f64| {
        let mut a = a % two_pi;
        if a < 0.0 {
            a += two_pi;
        }
        a
    };
    // Travelling p0 -> p1 leaving along `u`, the algebraic sweep direction is
    // the SIGN of the signed radius. Derived rather than guessed: with
    // `r = p0 - centre = -n * signed_r`, the counterclockwise velocity
    // `(-r.y, r.x)` equals `signed_r * u`, so it points along `u` exactly when
    // `signed_r > 0`.
    let ccw = signed_r > 0.0;
    let sweep = if ccw {
        norm(ang(p1) - ang(p0))
    } else {
        two_pi - norm(ang(p1) - ang(p0))
    };
    Some((signed_r.abs(), sweep > std::f64::consts::PI, ccw))
}

impl RefitChain {
    /// The direction the node's shared tangent actually has.
    ///
    /// **This is not simply `dir(tangent_rad)`, and the reason is a defect this
    /// milestone's own G1 instrument found against the first version of this
    /// module.** A `Line` reads no tangent: its direction is the chord between
    /// its two nodes, fixed by positions alone. So at a smooth node with a line
    /// on either side the stored angle is not a parameter — it is DETERMINED —
    /// and storing one produced exactly the disagreement the whole module
    /// exists to make unwritable: a measured spread of **0.0525 rad (3.01°)**
    /// on models the solver had accepted.
    ///
    /// The grammar already priced it that way (`free_scalars(Line, …) = 0`,
    /// `tangent_is_free(Line) = false`), so the code length and the
    /// representation had disagreed about what a line-adjacent smooth node is.
    /// The representation is what moved.
    fn node_dir(&self, i: usize) -> Option<Pt> {
        let unit = |v: Pt| {
            let l = v.length();
            (l > 0.0 && v.is_finite()).then(|| v * (1.0 / l))
        };
        if i > 0 && matches!(self.segments.get(i - 1), Some(RefitSegment::Line)) {
            return unit(self.nodes[i].pos - self.nodes[i - 1].pos);
        }
        if matches!(self.segments.get(i), Some(RefitSegment::Line)) {
            return unit(self.nodes[i + 1].pos - self.nodes[i].pos);
        }
        self.nodes.get(i)?.tangent_rad.map(dir)
    }

    /// Whether segment `k`'s end at a node READS that node's shared direction
    /// (`head = true` for the segment's head node `k`, `false` for its tail
    /// node `k + 1`).
    ///
    /// The judge behind [`RefitRefusal::SmoothNodeUnread`]. A `Line` counts as
    /// reading because the node's angle is DERIVED from its chord
    /// (`node_dir`), so declaration and geometry cannot disagree; every other
    /// family reads only where its parameterisation actually consumes the
    /// node's angle. Exhaustive over families and anchors on purpose: a new
    /// variant fails to compile here rather than silently not reading.
    fn end_reads_node(&self, k: usize, head: bool) -> bool {
        match self.segments[k] {
            RefitSegment::Line => true,
            RefitSegment::Arc(ArcAnchor::FromHeadTangent) => head,
            RefitSegment::Arc(ArcAnchor::FromTailTangent) => !head,
            RefitSegment::Arc(ArcAnchor::Radius { .. }) => false,
            // A quad's one control point is anchored at its HEAD node (see
            // `control`), so only the head is ever read.
            RefitSegment::Quad { ctrl } => head && matches!(ctrl, Handle::Shared { .. }),
            RefitSegment::Cubic { head: h, tail: t } => {
                let handle = if head { h } else { t };
                matches!(handle, Handle::Shared { .. })
            }
        }
    }

    /// The angle a node DECLARES once lowered: the same value its incident
    /// segments were built from, so the declaration cannot drift from the
    /// geometry.
    fn declared_angle(&self, i: usize) -> Option<f64> {
        self.nodes.get(i)?.tangent_rad?;
        let u = self.node_dir(i)?;
        Some(canonical_angle(u.y.atan2(u.x)))
    }

    /// The absolute control point of `handle` at node `i`, offset in `sign`
    /// times the node's shared direction.
    fn control(&self, handle: Handle, i: usize, sign: f64) -> Option<Pt> {
        match handle {
            Handle::Free(p) => Some(p),
            Handle::Shared { length_px } => {
                let u = self.node_dir(i)?;
                Some(self.nodes[i].pos + u * (sign * length_px))
            }
        }
    }

    /// Lower to the canonical IR: absolute control points, and the node's ONE
    /// angle as the declared `SmoothG1` tangent.
    ///
    /// This is the only place a shared tangent becomes two absolute control
    /// points, so it is the only place a G1 disagreement could enter — and it
    /// enters only as the floating-point round trip, because both control
    /// points are built from the same `dir(angle)` value **and every smooth
    /// node is checked to be READ by both incident ends** (`end_reads_node`).
    /// Before that check the sentence above was false as a class statement:
    /// RT6-A1 wrote the disagreement down through an arc anchored at its other
    /// end, and the solver accepted it.
    pub fn lower(&self) -> Result<CurveChain, RefitRefusal> {
        if self.segments.is_empty() || self.nodes.len() != self.segments.len() + 1 {
            return Err(RefitRefusal::Malformed);
        }
        let mut segments = Vec::with_capacity(self.segments.len());
        for (k, seg) in self.segments.iter().enumerate() {
            let (p0, p1) = (self.nodes[k].pos, self.nodes[k + 1].pos);
            if (p1 - p0).length_sq() <= 0.0 {
                return Err(RefitRefusal::DegenerateSpan { segment: k });
            }
            let out = match *seg {
                RefitSegment::Line => Segment::Line,
                RefitSegment::Arc(ArcAnchor::Radius {
                    radius_px,
                    large_arc,
                    ccw,
                }) => Segment::CircularArc {
                    radius_px,
                    large_arc,
                    ccw,
                },
                RefitSegment::Arc(ArcAnchor::FromHeadTangent) => {
                    let u = self
                        .node_dir(k)
                        .ok_or(RefitRefusal::NonFinite { segment: k })?;
                    let (radius_px, large_arc, ccw) = arc_from_tangent(p0, p1, u)
                        .ok_or(RefitRefusal::ArcIsALine { segment: k })?;
                    Segment::CircularArc {
                        radius_px,
                        large_arc,
                        ccw,
                    }
                }
                RefitSegment::Arc(ArcAnchor::FromTailTangent) => {
                    let u = self
                        .node_dir(k + 1)
                        .ok_or(RefitRefusal::NonFinite { segment: k })?;
                    // The circle through p1 and p0 leaving p1 along -u. That
                    // arc is this one traversed backwards, so the radius and
                    // the large-arc flag survive and the sweep direction flips.
                    let (radius_px, large_arc, ccw) = arc_from_tangent(p1, p0, u * -1.0)
                        .ok_or(RefitRefusal::ArcIsALine { segment: k })?;
                    Segment::CircularArc {
                        radius_px,
                        large_arc,
                        ccw: !ccw,
                    }
                }
                RefitSegment::Quad { ctrl } => Segment::Quad {
                    ctrl: self
                        .control(ctrl, k, 1.0)
                        .ok_or(RefitRefusal::NonFinite { segment: k })?,
                },
                RefitSegment::Cubic { head, tail } => Segment::Cubic {
                    ctrl1: self
                        .control(head, k, 1.0)
                        .ok_or(RefitRefusal::NonFinite { segment: k })?,
                    ctrl2: self
                        .control(tail, k + 1, -1.0)
                        .ok_or(RefitRefusal::NonFinite { segment: k })?,
                },
            };
            segments.push(out);
        }
        let mut interior_nodes = Vec::with_capacity(self.nodes.len().saturating_sub(2));
        for i in 1..self.nodes.len() - 1 {
            let join = if self.nodes[i].tangent_rad.is_some() {
                // Two lines meeting smoothly are collinear, and two collinear
                // lines are one line. Refused rather than lowered into a node
                // whose declaration can match at most one of its two chords.
                if matches!(self.segments[i - 1], RefitSegment::Line)
                    && matches!(self.segments[i], RefitSegment::Line)
                {
                    return Err(RefitRefusal::SmoothJoinBetweenTwoLines { node: i });
                }
                // RT6-A1: BOTH incident ends must read this node's angle, or
                // the declaration and the unreading side's geometry are two
                // independent values and the violation is writable.
                if !self.end_reads_node(i - 1, false) {
                    return Err(RefitRefusal::SmoothNodeUnread {
                        node: i,
                        segment: i - 1,
                    });
                }
                if !self.end_reads_node(i, true) {
                    return Err(RefitRefusal::SmoothNodeUnread {
                        node: i,
                        segment: i,
                    });
                }
                JoinKind::SmoothG1 {
                    tangent_angle_rad: self
                        .declared_angle(i)
                        .ok_or(RefitRefusal::NonFinite { segment: i })?,
                }
            } else {
                JoinKind::Corner
            };
            interior_nodes.push(ChainNode {
                pos: self.nodes[i].pos,
                join,
            });
        }
        Ok(CurveChain {
            interior_nodes,
            segments,
        })
    }

    pub fn start(&self) -> Pt {
        self.nodes[0].pos
    }

    pub fn end(&self) -> Pt {
        self.nodes[self.nodes.len() - 1].pos
    }
}

/// Fold an angle into `vice_ir`'s canonical `(-pi, pi]`.
///
/// `validate` rejects anything outside it, and a refit that produced `pi + eps`
/// would be rejected by the IR for a reason that has nothing to do with the
/// geometry.
pub fn canonical_angle(a: f64) -> f64 {
    let two_pi = std::f64::consts::TAU;
    let mut x = a % two_pi;
    if x <= -std::f64::consts::PI {
        x += two_pi;
    }
    if x > std::f64::consts::PI {
        x -= two_pi;
    }
    x
}

/// What one node of a LOWERED chain says about its own G1, measured from the
/// geometry rather than from the declaration.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct G1Reading {
    pub interior_node: usize,
    /// Direction the incoming segment ARRIVES with, radians.
    pub arrive_rad: f64,
    /// Direction the outgoing segment LEAVES with, radians.
    pub leave_rad: f64,
    /// The angle the node DECLARES, radians.
    pub declared_rad: f64,
    /// The largest pairwise difference among the three, radians. Zero is exact
    /// G1; the M1 IR fixture reads 0.4949 rad (28.36 deg).
    pub spread_rad: f64,
}

/// Measure G1 at every `SmoothG1` node of a lowered chain, from the absolute
/// control points.
///
/// The instrument is deliberately blind to how the chain was built: it is the
/// same function that reads 28.36° on `vice-ir`'s canonical fixture and reads
/// the floating-point floor on a lowered [`RefitChain`], which is what makes
/// the second number a measurement rather than a tautology.
pub fn g1_readings(chain: &CurveChain, start: Pt, end: Pt) -> Vec<G1Reading> {
    let pts = chain.node_positions(start, end);
    let mut out = Vec::new();
    for (i, node) in chain.interior_nodes.iter().enumerate() {
        let JoinKind::SmoothG1 { tangent_angle_rad } = node.join else {
            continue;
        };
        let arrive = arrive_dir(&chain.segments[i], pts[i], pts[i + 1]);
        let leave = leave_dir(&chain.segments[i + 1], pts[i + 1], pts[i + 2]);
        let (Some(a), Some(l)) = (arrive, leave) else {
            continue;
        };
        let (ar, lr) = (a.y.atan2(a.x), l.y.atan2(l.x));
        let d = |x: f64, y: f64| canonical_angle(x - y).abs();
        let spread = d(ar, lr)
            .max(d(ar, tangent_angle_rad))
            .max(d(lr, tangent_angle_rad));
        out.push(G1Reading {
            interior_node: i,
            arrive_rad: ar,
            leave_rad: lr,
            declared_rad: tangent_angle_rad,
            spread_rad: spread,
        });
    }
    out
}

fn nonzero(v: Pt) -> Option<Pt> {
    (v.length_sq() > 0.0 && v.is_finite()).then_some(v)
}

/// Direction a segment arrives at `p1` with.
fn arrive_dir(seg: &Segment, p0: Pt, p1: Pt) -> Option<Pt> {
    match *seg {
        Segment::Line => nonzero(p1 - p0),
        Segment::Quad { ctrl } => nonzero(p1 - ctrl).or_else(|| nonzero(p1 - p0)),
        Segment::Cubic { ctrl2, .. } => nonzero(p1 - ctrl2).or_else(|| nonzero(p1 - p0)),
        Segment::CircularArc {
            radius_px,
            large_arc,
            ccw,
        } => {
            let c = vice_geom::flatten::circular_arc_center(p0, p1, radius_px, large_arc, ccw)
                .ok()?
                .center;
            let r = p1 - c;
            nonzero(if ccw {
                Pt::new(-r.y, r.x)
            } else {
                Pt::new(r.y, -r.x)
            })
        }
        Segment::EllipticArc { .. } => None,
    }
}

/// Direction a segment leaves `p0` with.
fn leave_dir(seg: &Segment, p0: Pt, p1: Pt) -> Option<Pt> {
    match *seg {
        Segment::Line => nonzero(p1 - p0),
        Segment::Quad { ctrl } => nonzero(ctrl - p0).or_else(|| nonzero(p1 - p0)),
        Segment::Cubic { ctrl1, .. } => nonzero(ctrl1 - p0).or_else(|| nonzero(p1 - p0)),
        Segment::CircularArc {
            radius_px,
            large_arc,
            ccw,
        } => {
            let c = vice_geom::flatten::circular_arc_center(p0, p1, radius_px, large_arc, ccw)
                .ok()?
                .center;
            let r = p0 - c;
            nonzero(if ccw {
                Pt::new(-r.y, r.x)
            } else {
                Pt::new(r.y, -r.x)
            })
        }
        Segment::EllipticArc { .. } => None,
    }
}

/// How many samples' worth of corridor a refitted chain may miss by before the
/// discrete path is declared INVALID (§14.3).
///
/// Three corridor halfwidths, and what it hides is stated: a chain whose worst
/// `d_n` is between one and three halfwidths is accepted here and pays for the
/// excess in the residual code, which is where a soft miss belongs. What the
/// test excludes is a path whose geometry is somewhere else entirely — the
/// case §14.3 means by "evidence feasibility недостижимы". It is a bound on
/// GROSS infeasibility, not a quality threshold, and no candidate is selected
/// by it.
pub const FEASIBLE_HALFWIDTHS: f64 = 3.0;

#[cfg(test)]
mod tests {
    use super::*;

    fn cubic_chain(t0: f64, t1: f64) -> RefitChain {
        RefitChain {
            nodes: vec![
                RefitNode {
                    pos: Pt::new(0.0, 0.0),
                    tangent_rad: None,
                },
                RefitNode {
                    pos: Pt::new(10.0, 0.0),
                    tangent_rad: Some(t0),
                },
                RefitNode {
                    pos: Pt::new(20.0, 5.0),
                    tangent_rad: Some(t1),
                },
            ],
            segments: vec![
                RefitSegment::Cubic {
                    head: Handle::Free(Pt::new(3.0, -2.0)),
                    tail: Handle::Shared { length_px: 3.0 },
                },
                RefitSegment::Cubic {
                    head: Handle::Shared { length_px: 4.0 },
                    tail: Handle::Free(Pt::new(18.0, 6.0)),
                },
            ],
        }
    }

    /// **The property this module exists for, with the positive control that
    /// makes it a measurement.**
    ///
    /// The same instrument reads the refit chain and `vice-ir`'s canonical
    /// fixture. If it read zero on both it would prove nothing.
    #[test]
    fn refit_holds_g1_where_the_ir_fixture_does_not() {
        let c = cubic_chain(0.6, -0.3);
        let lowered = c.lower().expect("lowers");
        let readings = g1_readings(&lowered, c.start(), c.end());
        assert_eq!(readings.len(), 1, "one smooth interior node");
        let worst = readings.iter().map(|r| r.spread_rad).fold(0.0f64, f64::max);
        println!(
            "refit chain worst G1 spread {worst:.3e} rad ({:.3e} deg)",
            worst.to_degrees()
        );
        assert!(
            worst < 1e-12,
            "the refit representation lowered to a spread of {worst} rad; the control points are \
             supposed to be built from ONE angle"
        );

        // The positive control: the same instrument on a chain that stores its
        // control points independently of its declaration.
        let broken = CurveChain {
            interior_nodes: vec![ChainNode {
                pos: Pt::new(10.0, 0.0),
                join: JoinKind::SmoothG1 {
                    tangent_angle_rad: 0.25,
                },
            }],
            segments: vec![
                Segment::Quad {
                    ctrl: Pt::new(5.0, 2.5),
                },
                Segment::Quad {
                    ctrl: Pt::new(15.0, 0.0),
                },
            ],
        };
        let control = g1_readings(&broken, Pt::new(0.0, 0.0), Pt::new(20.0, 0.0));
        assert!(
            control[0].spread_rad > 0.1,
            "the instrument reads {} rad on a chain whose declaration and geometry disagree; then \
             the zero above is a property of the instrument, not of the representation",
            control[0].spread_rad
        );
    }

    /// Changing the ONE angle moves BOTH control points, which is what "stored
    /// once" means operationally.
    #[test]
    fn one_angle_moves_both_incident_control_points() {
        let a = cubic_chain(0.6, -0.3).lower().expect("lowers");
        let b = cubic_chain(0.9, -0.3).lower().expect("lowers");
        let (Segment::Cubic { ctrl2: a_in, .. }, Segment::Cubic { ctrl1: a_out, .. }) =
            (&a.segments[0], &a.segments[1])
        else {
            panic!("cubics");
        };
        let (Segment::Cubic { ctrl2: b_in, .. }, Segment::Cubic { ctrl1: b_out, .. }) =
            (&b.segments[0], &b.segments[1])
        else {
            panic!("cubics");
        };
        assert!(
            (*a_in - *b_in).length() > 1e-6 && (*a_out - *b_out).length() > 1e-6,
            "changing the node angle moved only one side: there are two copies of the direction"
        );
    }

    /// A corner node is the deliberate absence of sharing, and the instrument
    /// does not report on it at all — there is nothing to be consistent with.
    #[test]
    fn a_corner_node_declares_no_tangent_and_is_not_measured() {
        let mut c = cubic_chain(0.6, -0.3);
        c.nodes[1].tangent_rad = None;
        c.segments[0] = RefitSegment::Cubic {
            head: Handle::Free(Pt::new(3.0, -2.0)),
            tail: Handle::Free(Pt::new(7.0, -4.0)),
        };
        c.segments[1] = RefitSegment::Cubic {
            head: Handle::Free(Pt::new(13.0, 4.0)),
            tail: Handle::Free(Pt::new(18.0, 6.0)),
        };
        let lowered = c.lower().expect("lowers");
        assert_eq!(lowered.interior_nodes[0].join, JoinKind::Corner);
        assert!(g1_readings(&lowered, c.start(), c.end()).is_empty());
    }

    /// An arc pinned by a shared tangent is G1 at that node by construction
    /// too: its radius is not stored, it is derived from the same angle.
    #[test]
    fn an_arc_pinned_by_a_shared_tangent_is_g1_at_that_node() {
        let c = RefitChain {
            nodes: vec![
                RefitNode {
                    pos: Pt::new(0.0, 0.0),
                    tangent_rad: None,
                },
                RefitNode {
                    pos: Pt::new(10.0, 0.0),
                    tangent_rad: Some(0.4),
                },
                RefitNode {
                    pos: Pt::new(18.0, 9.0),
                    tangent_rad: None,
                },
            ],
            segments: vec![
                RefitSegment::Cubic {
                    head: Handle::Free(Pt::new(3.0, -1.0)),
                    tail: Handle::Shared { length_px: 3.0 },
                },
                RefitSegment::Arc(ArcAnchor::FromHeadTangent),
            ],
        };
        let lowered = c.lower().expect("lowers");
        let r = g1_readings(&lowered, c.start(), c.end());
        assert_eq!(r.len(), 1);
        assert!(
            r[0].spread_rad < 1e-9,
            "cubic-to-arc smooth join reads {} rad",
            r[0].spread_rad
        );
    }

    /// An arc whose prescribed tangent is along its own chord is a straight
    /// line, and is refused rather than given an enormous radius.
    #[test]
    fn an_arc_tangent_to_its_own_chord_is_refused() {
        let c = RefitChain {
            nodes: vec![
                RefitNode {
                    pos: Pt::new(0.0, 0.0),
                    tangent_rad: Some(0.0),
                },
                RefitNode {
                    pos: Pt::new(10.0, 0.0),
                    tangent_rad: None,
                },
            ],
            segments: vec![RefitSegment::Arc(ArcAnchor::FromHeadTangent)],
        };
        assert_eq!(c.lower(), Err(RefitRefusal::ArcIsALine { segment: 0 }));
    }

    #[test]
    fn the_canonical_angle_range_is_the_irs() {
        for a in [-7.0f64, -3.2, -std::f64::consts::PI, 0.0, 3.2, 7.0, 100.0] {
            let x = canonical_angle(a);
            assert!(
                x > -std::f64::consts::PI && x <= std::f64::consts::PI,
                "{a} folded to {x}"
            );
            assert!((canonical_angle(x - a) % std::f64::consts::TAU).abs() < 1e-9);
        }
    }
}
