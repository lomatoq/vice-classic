//! §15 Stage H: primitive and relation hypotheses, judged by the same code
//! length that judged the unconstrained model.
//!
//! ## §15's rule, and why it decides the whole shape of this module
//!
//! > Каждый constrained model сравнивается с unconstrained sibling через тот же
//! > exact posterior/MDL. Relation prior не может компенсировать topology defect
//! > или salient residual.
//!
//! So a relation is not a detector with a threshold. It is a HYPOTHESIS that
//! pays `bits_per_relation` plus the combinatorial code for which segments it
//! binds, and earns back the scalars it determines. It is accepted only when
//! that trade is a NET SAVING in bits, and it is refused outright when
//! enforcing it pushes the chain outside the evidence corridor — which is the
//! second sentence, as a mechanism rather than as a warning.
//!
//! ## The approximation, named at its exact price
//!
//! A relation is evaluated at the PROJECTED parameters — both radii set to
//! their mean, a line snapped to its nearest axis — **without a constrained
//! re-solve**. The optimiser has no constraint machinery and adding one is
//! §28 M7's trust-region work.
//!
//! That makes acceptance SOUND and rejection CONSERVATIVE: a constrained
//! re-solve can only lower the constrained model's residual, so a relation
//! accepted here would still be accepted after one, and a relation rejected
//! here might not be. The error is one-directional and it is in the safe
//! direction — the direction that does not promote a relation the evidence does
//! not support. Limitation 66.
//!
//! ## What is NOT here
//!
//! §15's `mirror_symmetry` and `repetition` are properties of a SCENE — "a
//! reflection maps the scene to itself", "a translation maps a sub-scene to
//! itself". Stage G is handed one chain at a time and has no scene. They are
//! not detected here, and the universe records their owner as the milestone with
//! a scene-level search rather than leaving them pointed at this one.

use serde::Serialize;
use vice_evidence::BoundarySample;
use vice_geom::Pt;

use crate::code::{log2_binomial, GeometryCodeTable};
use crate::models::BoundaryModel;
use crate::refit::{ArcAnchor, RefitChain, RefitSegment};
use crate::solve::flatten_chain;

/// The relation families of §15, named exactly as the model universe names
/// them.
///
/// Exhaustive `universe_name`: a variant nobody named stops compiling, which is
/// the compiler judging rather than a reviewer noticing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    /// Two arcs share one radius parameter.
    EqualRadius,
    /// Two arcs share one centre.
    Concentric,
    /// A line is horizontal or vertical.
    AxisAligned,
    /// Two lines share one direction parameter.
    Collinear,
}

impl RelationKind {
    pub fn universe_name(self) -> &'static str {
        match self {
            RelationKind::EqualRadius => "equal_radius",
            RelationKind::Concentric => "concentric",
            RelationKind::AxisAligned => "axis_aligned",
            RelationKind::Collinear => "collinear",
        }
    }

    /// Scalars the relation determines, so the constrained model no longer
    /// codes them.
    ///
    /// One each, and each for a stated reason rather than by a uniform rule:
    /// two arcs sharing a radius code one radius instead of two; two arcs
    /// sharing a centre determine the second radius from the first centre and
    /// the second arc's own endpoints; an axis-aligned line ties one coordinate
    /// of its two anchors together; two collinear lines tie one coordinate of
    /// the second's far anchor to the first's direction.
    pub fn scalars_determined(self) -> usize {
        1
    }
}

/// One relation hypothesis, with the whole trade published.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RelationHypothesis {
    pub kind: RelationKind,
    /// Segment indices the relation binds, in order.
    pub segments: Vec<usize>,
    /// `bits_per_relation` plus the combinatorial code for which segments.
    pub cost_bits: f64,
    /// `scalars_determined * coordinate_bits`.
    pub saving_bits: f64,
    /// Residual code of the constrained chain minus that of the unconstrained
    /// one. Non-negative in practice: constraining cannot improve a fit that
    /// was already optimised without the constraint.
    pub residual_penalty_bits: f64,
    /// `saving - cost - residual_penalty`. Positive means the constrained
    /// model is SHORTER, which is the only reason to accept it.
    pub net_bits: f64,
    /// Worst `|d_n|` of the constrained chain, px, and what the corridor
    /// allowed. §15's "relation prior не может компенсировать salient residual"
    /// as a number.
    pub worst_normal_deviation_px: f64,
    pub allowed_px: f64,
    pub accepted: bool,
}

/// Every relation hypothesis this module can form on one model, evaluated.
///
/// REJECTED hypotheses are returned too. A list of accepted relations says
/// nothing about how many were considered, and §15's comparison is only
/// meaningful if the losing side is visible.
pub fn relation_hypotheses(
    model: &BoundaryModel,
    samples: &[BoundarySample],
    table: &GeometryCodeTable,
    canvas_dim_px: f64,
) -> Vec<RelationHypothesis> {
    let cb = table.coordinate_bits(canvas_dim_px);
    let n_seg = model.chain.segments.len();
    if n_seg == 0 {
        return Vec::new();
    }
    let pair_code = if n_seg >= 2 {
        log2_binomial(n_seg, 2)
    } else {
        0.0
    };
    let single_code = (n_seg as f64).log2();
    let base_residual = residual_code(&model.chain, samples, table);

    let mut out = Vec::new();
    for (i, a) in model.chain.segments.iter().enumerate() {
        // Single-segment relations.
        if matches!(a, RefitSegment::Line) {
            let mut constrained = model.chain.clone();
            snap_to_axis(&mut constrained, i);
            out.push(evaluate(
                RelationKind::AxisAligned,
                vec![i],
                constrained,
                table.bits_per_relation() + single_code,
                cb,
                base_residual,
                samples,
                table,
            ));
        }
        for (j, b) in model.chain.segments.iter().enumerate().skip(i + 1) {
            match (a, b) {
                (RefitSegment::Arc(_), RefitSegment::Arc(_)) => {
                    for kind in [RelationKind::EqualRadius, RelationKind::Concentric] {
                        let mut constrained = model.chain.clone();
                        if !bind_arcs(&mut constrained, i, j, kind) {
                            continue;
                        }
                        out.push(evaluate(
                            kind,
                            vec![i, j],
                            constrained,
                            table.bits_per_relation() + pair_code,
                            cb,
                            base_residual,
                            samples,
                            table,
                        ));
                    }
                }
                (RefitSegment::Line, RefitSegment::Line) => {
                    let mut constrained = model.chain.clone();
                    if !make_collinear(&mut constrained, i, j) {
                        continue;
                    }
                    out.push(evaluate(
                        RelationKind::Collinear,
                        vec![i, j],
                        constrained,
                        table.bits_per_relation() + pair_code,
                        cb,
                        base_residual,
                        samples,
                        table,
                    ));
                }
                _ => {}
            }
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn evaluate(
    kind: RelationKind,
    segments: Vec<usize>,
    constrained: RefitChain,
    cost_bits: f64,
    coordinate_bits: f64,
    base_residual: f64,
    samples: &[BoundarySample],
    table: &GeometryCodeTable,
) -> RelationHypothesis {
    let saving_bits = kind.scalars_determined() as f64 * coordinate_bits;
    let after = residual_code(&constrained, samples, table);
    let residual_penalty_bits = after - base_residual;
    let (worst, allowed) = worst_deviation(&constrained, samples);
    let net_bits = saving_bits - cost_bits - residual_penalty_bits;
    RelationHypothesis {
        kind,
        segments,
        cost_bits,
        saving_bits,
        residual_penalty_bits,
        net_bits,
        worst_normal_deviation_px: worst,
        allowed_px: allowed,
        // §15's two conditions, both required: a net saving in bits AND a chain
        // the evidence still supports. A relation that pays for itself by
        // moving the boundary out of its corridor is the "relation prior
        // compensating a salient residual" §15 forbids.
        accepted: net_bits > 0.0 && worst <= allowed && after.is_finite(),
    }
}

fn residual_code(chain: &RefitChain, samples: &[BoundarySample], table: &GeometryCodeTable) -> f64 {
    let Ok(poly) = flatten_chain(chain) else {
        return f64::INFINITY;
    };
    let precision = table.coordinate_precision_px();
    samples
        .iter()
        .map(|s| {
            let dn = crate::cost::normal_deviation(s.p, s.normal, &poly)
                .map_or_else(|| crate::cost::euclidean_deviation(s.p, &poly), f64::abs);
            let w =
                crate::code::independent_observations(s.weight_ds, s.corr_length_px).unwrap_or(0.0);
            w * crate::code::residual_bits(dn, s.halfwidth, precision)
        })
        .sum()
}

fn worst_deviation(chain: &RefitChain, samples: &[BoundarySample]) -> (f64, f64) {
    let Ok(poly) = flatten_chain(chain) else {
        return (f64::INFINITY, 0.0);
    };
    let mut worst = 0.0f64;
    let mut allowed = 0.0f64;
    for s in samples {
        let dn = crate::cost::normal_deviation(s.p, s.normal, &poly)
            .map_or_else(|| crate::cost::euclidean_deviation(s.p, &poly), f64::abs);
        if dn > worst {
            worst = dn;
            allowed = crate::refit::FEASIBLE_HALFWIDTHS * s.halfwidth;
        }
    }
    (worst, allowed)
}

/// Move a line's two anchors onto a common horizontal or vertical, whichever is
/// nearer, by averaging the coordinate they must share.
fn snap_to_axis(chain: &mut RefitChain, seg: usize) {
    let (a, b) = (chain.nodes[seg].pos, chain.nodes[seg + 1].pos);
    let d = b - a;
    if d.x.abs() >= d.y.abs() {
        let y = 0.5 * (a.y + b.y);
        chain.nodes[seg].pos = Pt::new(a.x, y);
        chain.nodes[seg + 1].pos = Pt::new(b.x, y);
    } else {
        let x = 0.5 * (a.x + b.x);
        chain.nodes[seg].pos = Pt::new(x, a.y);
        chain.nodes[seg + 1].pos = Pt::new(x, b.y);
    }
}

/// Rotate the second line's far anchor about its near anchor so that its
/// direction equals the first line's.
fn make_collinear(chain: &mut RefitChain, i: usize, j: usize) -> bool {
    let d0 = chain.nodes[i + 1].pos - chain.nodes[i].pos;
    let l0 = d0.length();
    if !l0.is_finite() || l0 <= 0.0 {
        return false;
    }
    let u = d0 * (1.0 / l0);
    let base = chain.nodes[j].pos;
    let len = (chain.nodes[j + 1].pos - base).length();
    // Keep the far anchor on the same side, so "collinear" does not silently
    // reverse the chain's direction.
    let sign = if (chain.nodes[j + 1].pos - base).dot(u) < 0.0 {
        -1.0
    } else {
        1.0
    };
    chain.nodes[j + 1].pos = base + u * (sign * len);
    true
}

/// Bind two arcs by radius or by centre. `false` when either is pinned by a
/// shared tangent, in which case its radius is not a free parameter and there
/// is nothing to bind.
fn bind_arcs(chain: &mut RefitChain, i: usize, j: usize, kind: RelationKind) -> bool {
    let (
        RefitSegment::Arc(ArcAnchor::Radius { radius_px: ra, .. }),
        RefitSegment::Arc(ArcAnchor::Radius { radius_px: rb, .. }),
    ) = (chain.segments[i], chain.segments[j])
    else {
        return false;
    };
    let target = match kind {
        RelationKind::EqualRadius => 0.5 * (ra + rb),
        RelationKind::Concentric => {
            // With both endpoint pairs held, two concentric arcs are two arcs
            // whose centres coincide. The centre of each is determined by its
            // radius; equating them and solving is a one-parameter problem, and
            // the projection used here is the radius that puts the second
            // centre nearest the first.
            let Some(ca) = arc_centre(chain, i) else {
                return false;
            };
            let (p0, p1) = (chain.nodes[j].pos, chain.nodes[j + 1].pos);
            let mid = p0 + (p1 - p0) * 0.5;
            let r = (ca - mid).length().hypot((p1 - p0).length() * 0.5);
            if !(r.is_finite() && r > 0.0) {
                return false;
            }
            r
        }
        _ => return false,
    };
    if !(target.is_finite() && target > 0.0) {
        return false;
    }
    for k in [i, j] {
        if let RefitSegment::Arc(ArcAnchor::Radius { radius_px, .. }) = &mut chain.segments[k] {
            if kind == RelationKind::EqualRadius || k == j {
                *radius_px = target;
            }
        }
    }
    true
}

fn arc_centre(chain: &RefitChain, seg: usize) -> Option<Pt> {
    let RefitSegment::Arc(ArcAnchor::Radius {
        radius_px,
        large_arc,
        ccw,
    }) = chain.segments[seg]
    else {
        return None;
    };
    vice_geom::flatten::circular_arc_center(
        chain.nodes[seg].pos,
        chain.nodes[seg + 1].pos,
        radius_px,
        large_arc,
        ccw,
    )
    .ok()
    .map(|c| c.center)
}

/// The relations a model actually keeps, and what they do to its code length.
///
/// Only ACCEPTED hypotheses enter `relation_bits`, and a model with none pays
/// nothing — a `L_relations` of zero is a real statement about a chain with no
/// relation, not an absence.
pub fn apply_accepted(model: &mut BoundaryModel, hypotheses: &[RelationHypothesis]) -> usize {
    let mut kept = 0usize;
    for h in hypotheses.iter().filter(|h| h.accepted) {
        model.code.relation_bits += h.cost_bits;
        model.code.geometry_bits -= h.saving_bits;
        model.code.residual_bits += h.residual_penalty_bits;
        kept += 1;
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_relation_kind_names_a_universe_family() {
        let names: Vec<&str> = [
            RelationKind::EqualRadius,
            RelationKind::Concentric,
            RelationKind::AxisAligned,
            RelationKind::Collinear,
        ]
        .iter()
        .map(|k| k.universe_name())
        .collect();
        assert_eq!(
            names,
            vec!["equal_radius", "concentric", "axis_aligned", "collinear"]
        );
    }

    /// Snapping a nearly-horizontal line to the axis puts both anchors on one
    /// `y`, and snapping a nearly-vertical one puts both on one `x`. The wrong
    /// axis would be a relation that always loses, which is invisible from the
    /// accepted list alone.
    #[test]
    fn a_line_snaps_to_the_axis_it_is_nearest_to() {
        let mut c = RefitChain {
            nodes: vec![
                crate::refit::RefitNode {
                    pos: Pt::new(0.0, 0.0),
                    tangent_rad: None,
                },
                crate::refit::RefitNode {
                    pos: Pt::new(10.0, 0.2),
                    tangent_rad: None,
                },
            ],
            segments: vec![RefitSegment::Line],
        };
        snap_to_axis(&mut c, 0);
        assert!((c.nodes[0].pos.y - c.nodes[1].pos.y).abs() < 1e-12);
        assert!((c.nodes[0].pos.x - 0.0).abs() < 1e-12);

        let mut v = RefitChain {
            nodes: vec![
                crate::refit::RefitNode {
                    pos: Pt::new(0.0, 0.0),
                    tangent_rad: None,
                },
                crate::refit::RefitNode {
                    pos: Pt::new(0.2, 10.0),
                    tangent_rad: None,
                },
            ],
            segments: vec![RefitSegment::Line],
        };
        snap_to_axis(&mut v, 0);
        assert!((v.nodes[0].pos.x - v.nodes[1].pos.x).abs() < 1e-12);
    }

    #[test]
    fn collinear_keeps_the_length_and_the_direction_of_travel() {
        let mut c = RefitChain {
            nodes: vec![
                crate::refit::RefitNode {
                    pos: Pt::new(0.0, 0.0),
                    tangent_rad: None,
                },
                crate::refit::RefitNode {
                    pos: Pt::new(10.0, 0.0),
                    tangent_rad: None,
                },
                crate::refit::RefitNode {
                    pos: Pt::new(19.0, 3.0),
                    tangent_rad: None,
                },
            ],
            segments: vec![RefitSegment::Line, RefitSegment::Line],
        };
        let before = (c.nodes[2].pos - c.nodes[1].pos).length();
        assert!(make_collinear(&mut c, 0, 1));
        let after = c.nodes[2].pos - c.nodes[1].pos;
        assert!((after.length() - before).abs() < 1e-9, "length changed");
        assert!(after.y.abs() < 1e-9, "not collinear with the x axis");
        assert!(after.x > 0.0, "the direction of travel reversed");
    }
}
