//! §14.3 / §24: the joint constrained chain refit, and **exact G1 by
//! representation**.
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
//! **The price of the choice taken**, stated rather than left to be found:
//! `vice_ir::CurveChain` is unchanged, so it can still express an inconsistent
//! chain, and anything that constructs one outside this module can still write
//! the disagreement down. What is closed is the PRODUCER — [`RefitChain::lower`]
//! is the only path from Stage G to the IR, and the residual it leaves is the
//! floating-point round trip through absolute control points, which is MEASURED
//! (`refit_holds_g1_where_the_ir_fixture_does_not`) rather than assumed
//! negligible.
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

mod g1;
pub use g1::{canonical_angle, closure_g1_spread_rad, g1_readings, G1Reading};

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
    /// A shared Bezier handle has no representable positive length.
    ///
    /// A zero handle has a zero derivative. Treating its chord as a fallback
    /// direction would make a declared smooth join writable with a visible
    /// kink, so lowering refuses it before a canonical curve exists.
    NonPositiveSharedHandle { segment: usize, length_px: f64 },
    /// A lowered smooth join does not agree with its shared declaration.
    ///
    /// This is a representation assertion, not a tolerance-based way to
    /// manufacture G1. The threshold only covers floating-point roundoff after
    /// the two incident controls were derived from the same parameter.
    G1Violation { node: usize, spread_rad: f64 },
    /// More free scalars than [`crate::solve::MAX_JOINT_PARAMETERS`].
    ///
    /// Its own name, because until delta-1 this case was reported as
    /// `Malformed` — and a correct chain of forty-one segments is not
    /// malformed, it is bigger than the solver's backstop, and a report that
    /// calls it malformed misdirects whoever reads it (REDTEAM_M6 §4).
    TooManyParameters { parameters: usize, cap: usize },
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
        worst_deviation_px: f64,
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
    /// Whether the repeated endpoint of a closed chain shares one tangent
    /// parameter with its canonical start.
    pub fn has_closed_tangent_alias(&self) -> bool {
        self.nodes.len() >= 2
            && self
                .nodes
                .first()
                .is_some_and(|first| self.nodes.last().is_some_and(|last| first.pos == last.pos))
            && self.nodes[0].tangent_rad.is_some()
            && self.nodes[self.nodes.len() - 1].tangent_rad.is_some()
    }

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
        let last = self.nodes.len().checked_sub(1)?;
        if i == last && self.has_closed_tangent_alias() {
            return self.node_dir(0);
        }
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
                if !(length_px.is_finite() && length_px > 0.0) {
                    return None;
                }
                let u = self.node_dir(i)?;
                let control = self.nodes[i].pos + u * (sign * length_px);
                (control.is_finite() && control != self.nodes[i].pos).then_some(control)
            }
        }
    }

    fn validate_shared_handle(
        &self,
        handle: Handle,
        node: usize,
        segment: usize,
    ) -> Result<(), RefitRefusal> {
        let Handle::Shared { length_px } = handle else {
            return Ok(());
        };
        let control = self.control(handle, node, 1.0);
        if control.is_none() {
            return Err(RefitRefusal::NonPositiveSharedHandle { segment, length_px });
        }
        Ok(())
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
            match *seg {
                RefitSegment::Quad { ctrl } => {
                    self.validate_shared_handle(ctrl, k, k)?;
                }
                RefitSegment::Cubic { head, tail } => {
                    self.validate_shared_handle(head, k, k)?;
                    self.validate_shared_handle(tail, k + 1, k)?;
                }
                RefitSegment::Line | RefitSegment::Arc(_) => {}
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
        let closure_angle = if self.nodes[0].pos != self.nodes[self.nodes.len() - 1].pos {
            None
        } else {
            match (
                self.nodes[0].tangent_rad,
                self.nodes[self.nodes.len() - 1].tangent_rad,
            ) {
                (None, None) => None,
                (Some(first), Some(last)) => {
                    let spread = canonical_angle(first - last).abs();
                    if spread > crate::GATE_MAX_G1_SPREAD_RAD {
                        return Err(RefitRefusal::G1Violation {
                            node: 0,
                            spread_rad: spread,
                        });
                    }
                    let last_segment = self.segments.len() - 1;
                    if matches!(self.segments[0], RefitSegment::Line)
                        && matches!(self.segments[last_segment], RefitSegment::Line)
                    {
                        return Err(RefitRefusal::SmoothJoinBetweenTwoLines { node: 0 });
                    }
                    if !self.end_reads_node(last_segment, false) {
                        return Err(RefitRefusal::SmoothNodeUnread {
                            node: 0,
                            segment: last_segment,
                        });
                    }
                    if !self.end_reads_node(0, true) {
                        return Err(RefitRefusal::SmoothNodeUnread {
                            node: 0,
                            segment: 0,
                        });
                    }
                    Some(
                        self.declared_angle(0)
                            .ok_or(RefitRefusal::NonFinite { segment: 0 })?,
                    )
                }
                _ => return Err(RefitRefusal::Malformed),
            }
        };
        let lowered = CurveChain {
            interior_nodes,
            segments,
        };
        if let Some(reading) = g1_readings(&lowered, self.start(), self.end())
            .into_iter()
            .find(|reading| reading.spread_rad > crate::GATE_MAX_G1_SPREAD_RAD)
        {
            return Err(RefitRefusal::G1Violation {
                node: reading.interior_node + 1,
                spread_rad: reading.spread_rad,
            });
        }
        if let Some(declared) = closure_angle {
            let spread = closure_g1_spread_rad(&lowered, self.start(), self.end(), declared)
                .ok_or(RefitRefusal::G1Violation {
                    node: 0,
                    spread_rad: f64::INFINITY,
                })?;
            if spread > crate::GATE_MAX_G1_SPREAD_RAD {
                return Err(RefitRefusal::G1Violation {
                    node: 0,
                    spread_rad: spread,
                });
            }
        }
        Ok(lowered)
    }

    pub fn start(&self) -> Pt {
        self.nodes[0].pos
    }

    pub fn end(&self) -> Pt {
        self.nodes[self.nodes.len() - 1].pos
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

    /// A zero shared handle has no tangent. The old witness silently replaced
    /// that derivative with the span chord, so an independently chosen chord
    /// could disagree with the declared angle while `lower()` still accepted
    /// the chain.
    #[test]
    fn a_zero_shared_handle_is_not_a_smooth_representation() {
        let mut c = cubic_chain(0.6, -0.3);
        c.segments[0] = RefitSegment::Cubic {
            head: Handle::Free(Pt::new(3.0, -2.0)),
            tail: Handle::Shared { length_px: 0.0 },
        };
        assert!(matches!(
            c.lower(),
            Err(RefitRefusal::NonPositiveSharedHandle {
                segment: 0,
                length_px: 0.0
            })
        ));
    }

    #[test]
    fn a_closed_smooth_seam_aliases_one_tangent_and_is_measured() {
        let mut chain = RefitChain {
            nodes: vec![
                RefitNode {
                    pos: Pt::new(0.0, 0.0),
                    tangent_rad: Some(0.0),
                },
                RefitNode {
                    pos: Pt::new(10.0, 10.0),
                    tangent_rad: Some(std::f64::consts::PI),
                },
                RefitNode {
                    pos: Pt::new(0.0, 0.0),
                    tangent_rad: Some(0.0),
                },
            ],
            segments: vec![
                RefitSegment::Cubic {
                    head: Handle::Shared { length_px: 3.0 },
                    tail: Handle::Shared { length_px: 3.0 },
                },
                RefitSegment::Cubic {
                    head: Handle::Shared { length_px: 3.0 },
                    tail: Handle::Shared { length_px: 3.0 },
                },
            ],
        };
        let lowered = chain.lower().expect("closed shared seam lowers");
        let spread = closure_g1_spread_rad(&lowered, chain.start(), chain.end(), 0.0)
            .expect("closure witness");
        assert!(spread < crate::GATE_MAX_G1_SPREAD_RAD);

        chain.nodes[2].tangent_rad = Some(0.2);
        assert!(matches!(
            chain.lower(),
            Err(RefitRefusal::G1Violation { node: 0, .. })
        ));
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
