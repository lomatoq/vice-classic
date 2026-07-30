//! §15 Stage H relation hypotheses judged by the unconstrained sibling's code.
//!
//! A relation is a hypothesis, not a detector threshold: it pays flag/binding
//! codes, earns only determined scalars, and must both save bits and remain in
//! the evidence corridor. A prior therefore cannot hide topology or residual.
//!
//! Parameters are projected without a constrained re-solve, which belongs to
//! M7. Acceptance is sound and rejection conservative: re-solving may recover
//! a rejection by lowering residual, but cannot undo an accepted saving.
//! Limitation 66.
//!
use serde::Serialize;
use vice_evidence::BoundarySample;
use vice_geom::Pt;

use crate::code::{log2_binomial, GeometryCodeTable};
use crate::models::BoundaryModel;
use crate::refit::{ArcAnchor, RefitChain, RefitSegment};
use crate::solve::flatten_chain;

mod topology;
use topology::closure_matches;
pub use topology::{apply_accepted, apply_relation_sibling};

/// Relation hypotheses are independently evaluated against one free sibling.
/// Selecting more than one would require a newly evaluated joint constrained
/// sibling, so M6 keeps the single best admissible one.
pub const RELATION_COMPOSITION_POLICY: &str = "best_single_constrained_sibling_v1";
/// A relation removes bits from the component that originally encoded the
/// determined scalar. Segment-local arc radii live in geometry; line anchors
/// and whole-loop vertices live in topology.
pub const RELATION_SAVING_OWNERSHIP_POLICY: &str =
    "arc_parameters_geometry_line_and_loop_anchors_topology_v1";

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
    /// Two lines share one direction parameter but retain independent offsets.
    Parallel,
    /// Two line directions differ by exactly one quarter turn.
    Perpendicular,
    /// Two lines lie on the same infinite supporting line.
    SharedBaseline,
    /// A closed line loop is invariant under one reflected node pairing.
    MirrorSymmetry,
    /// One line segment is the translation of another, including its length.
    RepeatedTransform,
}

impl RelationKind {
    /// Every generator this type can name. The relation/universe judge consumes
    /// this value instead of retyping the variants, so adding a variant cannot
    /// silently evade the reverse direction of that judge (M6B-N6).
    pub const ALL: [RelationKind; 7] = [
        RelationKind::EqualRadius,
        RelationKind::Concentric,
        RelationKind::Parallel,
        RelationKind::Perpendicular,
        RelationKind::SharedBaseline,
        RelationKind::MirrorSymmetry,
        RelationKind::RepeatedTransform,
    ];

    pub fn universe_name(self) -> &'static str {
        match self {
            RelationKind::EqualRadius => "equal_radius",
            RelationKind::Concentric => "concentric",
            RelationKind::Parallel | RelationKind::Perpendicular => "parallel_perpendicular",
            RelationKind::SharedBaseline => "shared_baseline",
            RelationKind::MirrorSymmetry => "mirror_symmetry",
            RelationKind::RepeatedTransform => "repeated_transforms",
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
        match self {
            RelationKind::SharedBaseline
            | RelationKind::MirrorSymmetry
            | RelationKind::RepeatedTransform => 2,
            _ => 1,
        }
    }

    pub fn saving_component(self) -> &'static str {
        match self {
            RelationKind::EqualRadius | RelationKind::Concentric => "geometry",
            RelationKind::Parallel
            | RelationKind::Perpendicular
            | RelationKind::SharedBaseline
            | RelationKind::MirrorSymmetry
            | RelationKind::RepeatedTransform => "topology",
        }
    }

    /// Parallel and perpendicular are the two flags of the one
    /// `parallel_perpendicular` universe family named by §15's slash.
    pub fn flag_bits(self) -> f64 {
        match self {
            RelationKind::Parallel | RelationKind::Perpendicular => 1.0,
            _ => 0.0,
        }
    }

    /// An adjacent second line already starts on the first line's endpoint,
    /// so `SharedBaseline` would duplicate `Parallel` without a second saving.
    pub fn identifiable_for_adjacent_segments(self) -> bool {
        self != RelationKind::SharedBaseline
    }
}

/// One relation hypothesis, with the whole trade published.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RelationHypothesis {
    pub kind: RelationKind,
    /// Segment indices the relation binds, in order.
    pub segments: Vec<usize>,
    /// The actual constrained sibling whose residual was measured.
    pub constrained_chain: RefitChain,
    /// `bits_per_relation` plus the combinatorial code for which segments.
    pub cost_bits: f64,
    /// `scalars_determined * coordinate_bits`.
    pub saving_bits: f64,
    /// Part of `saving_bits` removed from segment-local parameters.
    pub geometry_saving_bits: f64,
    /// Part of `saving_bits` removed from coded anchors/vertices.
    pub topology_saving_bits: f64,
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
    pub worst_model_to_evidence_px: f64,
    pub allowed_px: f64,
    /// Deterministic projected re-solve performed after the relation is
    /// imposed. Every row is judged on the normal-direction residual used by
    /// Stage G; accepted steps never depend on an unrelated Euclidean proxy.
    pub solve_trace: Vec<RelationSolveTraceRow>,
    pub accepted: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RelationSolveTraceRow {
    pub pass: usize,
    pub parameter: usize,
    pub step_px: f64,
    pub normal_objective_before: f64,
    pub normal_objective_after: f64,
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
    closed: bool,
) -> Vec<RelationHypothesis> {
    let Some(free_chain) = model.geometry.typed_chain() else {
        return Vec::new();
    };
    let cb = table.coordinate_bits(canvas_dim_px);
    let n_seg = free_chain.segments.len();
    if n_seg == 0 || !closure_matches(free_chain, closed) {
        return Vec::new();
    }
    let pair_code = if n_seg >= 2 {
        log2_binomial(n_seg, 2)
    } else {
        0.0
    };
    let base_residual = residual_code(free_chain, samples, table);

    let mut out = Vec::new();
    for (i, a) in free_chain.segments.iter().enumerate() {
        for (j, b) in free_chain.segments.iter().enumerate().skip(i + 1) {
            match (a, b) {
                (RefitSegment::Arc(_), RefitSegment::Arc(_)) => {
                    for kind in [RelationKind::EqualRadius, RelationKind::Concentric] {
                        let mut constrained = free_chain.clone();
                        if !bind_arcs(&mut constrained, i, j, kind) {
                            continue;
                        }
                        out.push(evaluate(
                            kind,
                            vec![i, j],
                            constrained,
                            table.bits_per_relation() + pair_code,
                            cb,
                            kind.scalars_determined(),
                            model.code.geometry_bits,
                            model.code.topology_bits,
                            base_residual,
                            samples,
                            table,
                        ));
                    }
                }
                (RefitSegment::Line, RefitSegment::Line) => {
                    let adjacent = j == i + 1 || (closed && i == 0 && j == n_seg.saturating_sub(1));
                    for kind in [
                        RelationKind::Parallel,
                        RelationKind::Perpendicular,
                        RelationKind::SharedBaseline,
                        RelationKind::RepeatedTransform,
                    ] {
                        if adjacent && !kind.identifiable_for_adjacent_segments() {
                            continue;
                        }
                        let mut constrained = free_chain.clone();
                        let bound = if kind == RelationKind::RepeatedTransform {
                            bind_repeated_line(&mut constrained, i, j)
                        } else {
                            bind_lines(&mut constrained, i, j, kind)
                        };
                        if !bound || !closure_matches(&constrained, closed) {
                            continue;
                        }
                        out.push(evaluate(
                            kind,
                            vec![i, j],
                            constrained,
                            table.bits_per_relation() + pair_code + kind.flag_bits(),
                            cb,
                            kind.scalars_determined(),
                            model.code.geometry_bits,
                            model.code.topology_bits,
                            base_residual,
                            samples,
                            table,
                        ));
                    }
                }
                _ => {}
            }
        }
    }
    if let Some((constrained, determined)) = bind_mirror_loop(free_chain) {
        if closure_matches(&constrained, closed) {
            let segments: Vec<usize> = (0..n_seg).collect();
            out.push(evaluate(
                RelationKind::MirrorSymmetry,
                segments,
                constrained,
                table.bits_per_relation(),
                cb,
                determined,
                model.code.geometry_bits,
                model.code.topology_bits,
                base_residual,
                samples,
                table,
            ));
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
    scalars_determined: usize,
    available_geometry_bits: f64,
    available_topology_bits: f64,
    base_residual: f64,
    samples: &[BoundarySample],
    table: &GeometryCodeTable,
) -> RelationHypothesis {
    let (constrained, solve_trace) = resolve_constrained(kind, &segments, constrained, samples);
    let saving_bits = scalars_determined as f64 * coordinate_bits;
    let (geometry_saving_bits, topology_saving_bits) = match kind.saving_component() {
        "geometry" => (saving_bits, 0.0),
        "topology" => (0.0, saving_bits),
        _ => unreachable!("every relation saving has one owning code component"),
    };
    let after = residual_code(&constrained, samples, table);
    let residual_penalty_bits = after - base_residual;
    let (forward, reverse) = flatten_chain(&constrained).map_or_else(
        |_| {
            let invalid = crate::solve::evidence_to_model_corridor(&[], samples);
            (invalid, invalid)
        },
        |poly| {
            (
                crate::solve::evidence_to_model_corridor(&poly, samples),
                crate::solve::model_to_evidence_corridor(&poly, samples),
            )
        },
    );
    let net_bits = saving_bits - cost_bits - residual_penalty_bits;
    RelationHypothesis {
        kind,
        segments,
        constrained_chain: constrained,
        cost_bits,
        saving_bits,
        geometry_saving_bits,
        topology_saving_bits,
        residual_penalty_bits,
        net_bits,
        worst_normal_deviation_px: forward.deviation_px,
        worst_model_to_evidence_px: reverse.deviation_px,
        allowed_px: forward.allowed_px,
        solve_trace,
        // §15's two conditions, both required: a net saving in bits AND a chain
        // the evidence still supports. A relation that pays for itself by
        // moving the boundary out of its corridor is the "relation prior
        // compensating a salient residual" §15 forbids.
        accepted: net_bits > 0.0
            && forward.feasible()
            && reverse.feasible()
            && after.is_finite()
            && geometry_saving_bits <= available_geometry_bits
            && topology_saving_bits <= available_topology_bits,
    }
}

fn normal_residuals(chain: &RefitChain, samples: &[BoundarySample]) -> Option<Vec<f64>> {
    let poly = flatten_chain(chain).ok()?;
    samples
        .iter()
        .map(|sample| {
            let deviation = crate::cost::normal_deviation(sample.p, sample.normal, &poly)?;
            let independent =
                crate::code::independent_observations(sample.weight_ds, sample.corr_length_px)?;
            (sample.halfwidth.is_finite() && sample.halfwidth > 0.0)
                .then_some(deviation / sample.halfwidth * independent.sqrt())
        })
        .collect()
}

fn normal_objective(chain: &RefitChain, samples: &[BoundarySample]) -> f64 {
    normal_residuals(chain, samples)
        .map(|residuals| residuals.into_iter().map(|value| value * value).sum())
        .unwrap_or(f64::INFINITY)
}

fn project_relation(chain: &mut RefitChain, kind: RelationKind, segments: &[usize]) -> bool {
    let projected = match kind {
        RelationKind::EqualRadius | RelationKind::Concentric if segments.len() == 2 => {
            bind_arcs(chain, segments[0], segments[1], kind)
        }
        RelationKind::Parallel | RelationKind::Perpendicular | RelationKind::SharedBaseline
            if segments.len() == 2 =>
        {
            bind_lines(chain, segments[0], segments[1], kind)
        }
        RelationKind::RepeatedTransform if segments.len() == 2 => {
            bind_repeated_line(chain, segments[0], segments[1])
        }
        RelationKind::MirrorSymmetry => {
            if let Some((projected, _)) = bind_mirror_loop(chain) {
                *chain = projected;
                true
            } else {
                false
            }
        }
        _ => false,
    };
    if projected
        && chain.nodes.first().map(|node| node.pos) == chain.nodes.last().map(|node| node.pos)
    {
        let first = chain.nodes[0].pos;
        let last = chain.nodes.len() - 1;
        chain.nodes[last].pos = first;
    }
    projected
}

fn perturb_projected(
    parent: &RefitChain,
    kind: RelationKind,
    segments: &[usize],
    parameter: usize,
    delta: f64,
) -> Option<RefitChain> {
    let closed =
        parent.nodes.first().map(|node| node.pos) == parent.nodes.last().map(|node| node.pos);
    let unique_nodes = parent.nodes.len().saturating_sub(usize::from(closed));
    let mut child = parent.clone();
    if parameter < unique_nodes * 2 {
        let node = parameter / 2;
        if parameter.is_multiple_of(2) {
            child.nodes[node].pos.x += delta;
        } else {
            child.nodes[node].pos.y += delta;
        }
        if closed && node == 0 {
            let last = child.nodes.len() - 1;
            child.nodes[last].pos = child.nodes[0].pos;
        }
    } else {
        let arc_slot = parameter - unique_nodes * 2;
        let segment = *segments.get(arc_slot)?;
        let RefitSegment::Arc(crate::refit::ArcAnchor::Radius { radius_px, .. }) =
            child.segments.get_mut(segment)?
        else {
            return None;
        };
        *radius_px = (*radius_px + delta).max(1e-9);
    }
    project_relation(&mut child, kind, segments).then_some(child)
}

/// Refit the remaining free coordinates after imposing a relation. The
/// projected finite-difference direction and every accepted child use the
/// same normal-direction residual vector, so the Jacobian and objective
/// cannot silently disagree.
fn resolve_constrained(
    kind: RelationKind,
    segments: &[usize],
    initial: RefitChain,
    samples: &[BoundarySample],
) -> (RefitChain, Vec<RelationSolveTraceRow>) {
    let closed =
        initial.nodes.first().map(|node| node.pos) == initial.nodes.last().map(|node| node.pos);
    let unique_nodes = initial.nodes.len().saturating_sub(usize::from(closed));
    let arc_parameters = segments
        .iter()
        .filter(|&&segment| matches!(initial.segments.get(segment), Some(RefitSegment::Arc(_))))
        .count();
    let parameter_count = unique_nodes * 2 + arc_parameters;
    let mut current = initial;
    let mut current_objective = normal_objective(&current, samples);
    let mut trace = Vec::new();
    for (pass, step) in [0.25, 0.1, 0.04, 0.01].into_iter().enumerate() {
        for parameter in 0..parameter_count {
            let plus = perturb_projected(&current, kind, segments, parameter, step);
            let minus = perturb_projected(&current, kind, segments, parameter, -step);
            let plus_objective = plus
                .as_ref()
                .map(|chain| normal_objective(chain, samples))
                .unwrap_or(f64::INFINITY);
            let minus_objective = minus
                .as_ref()
                .map(|chain| normal_objective(chain, samples))
                .unwrap_or(f64::INFINITY);
            let (candidate, candidate_objective) = if plus_objective <= minus_objective {
                (plus, plus_objective)
            } else {
                (minus, minus_objective)
            };
            let accepted = candidate_objective + 1e-12 < current_objective;
            trace.push(RelationSolveTraceRow {
                pass,
                parameter,
                step_px: step,
                normal_objective_before: current_objective,
                normal_objective_after: candidate_objective,
                accepted,
            });
            if accepted {
                current = candidate.expect("finite candidate objective came from a chain");
                current_objective = candidate_objective;
            }
        }
    }
    (current, trace)
}

/// Make segment `j` the exact translated copy of line segment `i`.
fn bind_repeated_line(chain: &mut RefitChain, i: usize, j: usize) -> bool {
    if !matches!(chain.segments.get(i), Some(RefitSegment::Line))
        || !matches!(chain.segments.get(j), Some(RefitSegment::Line))
    {
        return false;
    }
    let delta = chain.nodes[i + 1].pos - chain.nodes[i].pos;
    if !(delta.is_finite() && delta.length_sq() > 0.0) {
        return false;
    }
    chain.nodes[j + 1].pos = chain.nodes[j].pos + delta;
    true
}

/// Bilaterally symmetrise a closed all-line loop around the axis through its
/// centroid and canonical first vertex.
///
/// The correspondence is finite and cut-explicit: vertex `k` pairs with
/// `n-k`. This is a real constrained sibling (nodes are projected), not a
/// detector flag. Odd loops are supported; the only self-paired vertex is the
/// canonical seam.
fn bind_mirror_loop(chain: &RefitChain) -> Option<(RefitChain, usize)> {
    let n = chain.segments.len();
    if n < 3
        || chain.nodes.len() != n + 1
        || !chain
            .segments
            .iter()
            .all(|segment| matches!(segment, RefitSegment::Line))
        || (chain.nodes[0].pos - chain.nodes[n].pos).length() > 1e-9
    {
        return None;
    }
    let center = chain.nodes[..n]
        .iter()
        .fold(Pt::ZERO, |sum, node| sum + node.pos)
        * (1.0 / n as f64);
    let axis = chain.nodes[0].pos - center;
    let axis_len = axis.length();
    if !(axis_len.is_finite() && axis_len > 0.0) {
        return None;
    }
    let u = axis * (1.0 / axis_len);
    let v = Pt::new(-u.y, u.x);
    let mut constrained = chain.clone();
    let mut pairs = 0usize;
    for k in 0..=n / 2 {
        let j = (n - k) % n;
        if k == j {
            let d = chain.nodes[k].pos - center;
            constrained.nodes[k].pos = center + u * d.dot(u);
            continue;
        }
        let a = chain.nodes[k].pos - center;
        let b = chain.nodes[j].pos - center;
        let along = 0.5 * (a.dot(u) + b.dot(u));
        let across = 0.5 * (a.dot(v) - b.dot(v));
        constrained.nodes[k].pos = center + u * along + v * across;
        constrained.nodes[j].pos = center + u * along - v * across;
        pairs += 1;
    }
    constrained.nodes[n].pos = constrained.nodes[0].pos;
    // Each paired 2D vertex loses two free coordinates. The axis is determined
    // by the centroid and canonical seam, so it is not an unpriced parameter.
    (pairs > 0).then_some((constrained, pairs * 2))
}

fn residual_code(chain: &RefitChain, samples: &[BoundarySample], table: &GeometryCodeTable) -> f64 {
    let Ok(poly) = flatten_chain(chain) else {
        return f64::INFINITY;
    };
    let precision = table.coordinate_precision_px();
    samples
        .iter()
        .try_fold(0.0, |total, s| {
            let dn = crate::cost::normal_deviation(s.p, s.normal, &poly)
                .map_or_else(|| crate::cost::euclidean_deviation(s.p, &poly), f64::abs);
            let w = crate::code::independent_observations(s.weight_ds, s.corr_length_px)?;
            Some(total + w * crate::code::residual_bits(dn, s.halfwidth, precision))
        })
        .unwrap_or(f64::INFINITY)
}

/// Project the second line onto the parallel, perpendicular or shared-baseline
/// sibling of the first while preserving its length and direction of travel.
fn bind_lines(chain: &mut RefitChain, i: usize, j: usize, kind: RelationKind) -> bool {
    let d0 = chain.nodes[i + 1].pos - chain.nodes[i].pos;
    let l0 = d0.length();
    if !l0.is_finite() || l0 <= 0.0 {
        return false;
    }
    let first_u = d0 * (1.0 / l0);
    let old_base = chain.nodes[j].pos;
    let old_d = chain.nodes[j + 1].pos - old_base;
    let len = old_d.length();
    if !len.is_finite() || len <= 0.0 {
        return false;
    }
    let target_u = match kind {
        RelationKind::Parallel | RelationKind::SharedBaseline => first_u,
        RelationKind::Perpendicular => Pt::new(-first_u.y, first_u.x),
        _ => return false,
    };
    let sign = if old_d.dot(target_u) < 0.0 { -1.0 } else { 1.0 };
    let base = if kind == RelationKind::SharedBaseline {
        let first_base = chain.nodes[i].pos;
        first_base + first_u * (old_base - first_base).dot(first_u)
    } else {
        old_base
    };
    chain.nodes[j].pos = base;
    chain.nodes[j + 1].pos = base + target_u * (sign * len);
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
    if kind == RelationKind::Concentric {
        // Fixed arc endpoints do not in general admit an arbitrary prescribed
        // centre. Changing only the radius merely chooses a point on the
        // chord's perpendicular bisector and was previously able to label a
        // visibly non-concentric sibling "concentric". M6 has no constrained
        // endpoint solve, so the sound conservative sibling is the one whose
        // two materialized centres already coincide to roundoff.
        // Compute both centres in one local frame. Using their absolute canvas
        // coordinates here made the roundoff allowance grow with distance
        // from the world origin, so translating the same two arcs could mint
        // a relation candidate.
        let origin = chain.nodes[i].pos;
        let (Some(a), Some(b)) = (
            arc_centre_in_frame(chain, i, origin),
            arc_centre_in_frame(chain, j, origin),
        ) else {
            return false;
        };
        let chord_a = (chain.nodes[i + 1].pos - chain.nodes[i].pos).length();
        let chord_b = (chain.nodes[j + 1].pos - chain.nodes[j].pos).length();
        let scale = ra.abs().max(rb.abs()).max(chord_a).max(chord_b).max(1.0);
        return (a - b).length() <= 32.0 * f64::EPSILON * scale;
    }
    let target = match kind {
        RelationKind::EqualRadius => 0.5 * (ra + rb),
        RelationKind::Concentric => unreachable!("handled above"),
        _ => return false,
    };
    if !(target.is_finite() && target > 0.0) {
        return false;
    }
    for k in [i, j] {
        if let RefitSegment::Arc(ArcAnchor::Radius { radius_px, .. }) = &mut chain.segments[k] {
            *radius_px = target;
        }
    }
    true
}

#[cfg(test)]
fn arc_centre(chain: &RefitChain, seg: usize) -> Option<Pt> {
    arc_centre_in_frame(chain, seg, Pt::ZERO)
}

fn arc_centre_in_frame(chain: &RefitChain, seg: usize, origin: Pt) -> Option<Pt> {
    let RefitSegment::Arc(ArcAnchor::Radius {
        radius_px,
        large_arc,
        ccw,
    }) = chain.segments[seg]
    else {
        return None;
    };
    vice_geom::flatten::circular_arc_center(
        chain.nodes[seg].pos - origin,
        chain.nodes[seg + 1].pos - origin,
        radius_px,
        large_arc,
        ccw,
    )
    .ok()
    .map(|c| c.center)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vice_evidence::BoundarySample;

    #[test]
    fn every_relation_kind_names_a_universe_family() {
        let names: Vec<&str> = RelationKind::ALL
            .iter()
            .map(|k| k.universe_name())
            .collect();
        assert_eq!(
            names,
            vec![
                "equal_radius",
                "concentric",
                "parallel_perpendicular",
                "parallel_perpendicular",
                "shared_baseline",
                "mirror_symmetry",
                "repeated_transforms"
            ]
        );
    }

    #[test]
    fn line_constraints_are_geometrically_distinct() {
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
                    pos: Pt::new(12.0, 3.0),
                    tangent_rad: None,
                },
            ],
            segments: vec![RefitSegment::Line, RefitSegment::Line],
        };
        let before = (c.nodes[2].pos - c.nodes[1].pos).length();
        assert!(bind_lines(&mut c, 0, 1, RelationKind::Parallel));
        let after = c.nodes[2].pos - c.nodes[1].pos;
        assert!((after.length() - before).abs() < 1e-9, "length changed");
        assert!(after.y.abs() < 1e-9, "not parallel with the x axis");
        assert!(after.x > 0.0, "the direction of travel reversed");

        let mut perpendicular = RefitChain {
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
                    pos: Pt::new(12.0, 3.0),
                    tangent_rad: None,
                },
            ],
            segments: vec![RefitSegment::Line, RefitSegment::Line],
        };
        assert!(bind_lines(
            &mut perpendicular,
            0,
            1,
            RelationKind::Perpendicular
        ));
        let d = perpendicular.nodes[2].pos - perpendicular.nodes[1].pos;
        assert!(d.dot(Pt::new(10.0, 0.0)).abs() < 1e-9);

        let mut baseline = RefitChain {
            nodes: vec![
                crate::refit::RefitNode {
                    pos: Pt::new(0.0, 0.0),
                    tangent_rad: None,
                },
                crate::refit::RefitNode {
                    pos: Pt::new(10.0, 2.0),
                    tangent_rad: None,
                },
                crate::refit::RefitNode {
                    pos: Pt::new(12.0, 5.0),
                    tangent_rad: None,
                },
            ],
            segments: vec![RefitSegment::Line, RefitSegment::Line],
        };
        assert!(bind_lines(
            &mut baseline,
            0,
            1,
            RelationKind::SharedBaseline
        ));
        let first = baseline.nodes[1].pos - baseline.nodes[0].pos;
        assert!(
            (baseline.nodes[2].pos - baseline.nodes[0].pos)
                .cross(first)
                .abs()
                < 1e-9
        );
    }

    #[test]
    fn repeated_transform_materializes_the_same_line_vector() {
        let mut chain = RefitChain {
            nodes: vec![
                crate::refit::RefitNode {
                    pos: Pt::new(0.0, 0.0),
                    tangent_rad: None,
                },
                crate::refit::RefitNode {
                    pos: Pt::new(8.0, 2.0),
                    tangent_rad: None,
                },
                crate::refit::RefitNode {
                    pos: Pt::new(20.0, 5.0),
                    tangent_rad: None,
                },
            ],
            segments: vec![RefitSegment::Line, RefitSegment::Line],
        };
        assert!(bind_repeated_line(&mut chain, 0, 1));
        assert_eq!(
            chain.nodes[2].pos - chain.nodes[1].pos,
            chain.nodes[1].pos - chain.nodes[0].pos
        );
    }

    #[test]
    fn concentric_means_the_materialized_arc_centres_coincide() {
        let mut concentric = RefitChain {
            nodes: vec![
                crate::refit::RefitNode {
                    pos: Pt::new(5.0, 0.0),
                    tangent_rad: None,
                },
                crate::refit::RefitNode {
                    pos: Pt::new(0.0, 5.0),
                    tangent_rad: None,
                },
                crate::refit::RefitNode {
                    pos: Pt::new(-5.0, 0.0),
                    tangent_rad: None,
                },
            ],
            segments: vec![
                RefitSegment::Arc(ArcAnchor::Radius {
                    radius_px: 5.0,
                    large_arc: false,
                    ccw: true,
                }),
                RefitSegment::Arc(ArcAnchor::Radius {
                    radius_px: 5.0,
                    large_arc: false,
                    ccw: true,
                }),
            ],
        };
        assert!(bind_arcs(&mut concentric, 0, 1, RelationKind::Concentric));
        assert!(
            (arc_centre(&concentric, 0).unwrap() - arc_centre(&concentric, 1).unwrap()).length()
                <= 32.0 * f64::EPSILON
        );

        // The first arc is the semicircle centred at (1,0). The second
        // chord's perpendicular bisector does not contain that centre. Merely
        // changing its radius can never make the two arcs concentric.
        let mut incompatible = RefitChain {
            nodes: vec![
                crate::refit::RefitNode {
                    pos: Pt::new(2.0, -4.0),
                    tangent_rad: None,
                },
                crate::refit::RefitNode {
                    pos: Pt::new(0.0, 4.0),
                    tangent_rad: None,
                },
                crate::refit::RefitNode {
                    pos: Pt::new(2.0, 6.0),
                    tangent_rad: None,
                },
            ],
            segments: vec![
                RefitSegment::Arc(ArcAnchor::Radius {
                    radius_px: 17.0f64.sqrt(),
                    large_arc: false,
                    ccw: true,
                }),
                RefitSegment::Arc(ArcAnchor::Radius {
                    radius_px: 3.0,
                    large_arc: false,
                    ccw: true,
                }),
            ],
        };
        let before = incompatible.clone();
        assert!(!bind_arcs(
            &mut incompatible,
            0,
            1,
            RelationKind::Concentric
        ));
        assert_eq!(incompatible, before, "a refused projection must be inert");
    }

    #[test]
    fn mirror_loop_projects_a_perturbed_rectangle_to_bilateral_geometry() {
        let chain = RefitChain {
            nodes: vec![
                crate::refit::RefitNode {
                    pos: Pt::new(0.0, 0.0),
                    tangent_rad: None,
                },
                crate::refit::RefitNode {
                    pos: Pt::new(10.0, 1.0),
                    tangent_rad: None,
                },
                crate::refit::RefitNode {
                    pos: Pt::new(10.0, 8.0),
                    tangent_rad: None,
                },
                crate::refit::RefitNode {
                    pos: Pt::new(-1.0, 7.0),
                    tangent_rad: None,
                },
                crate::refit::RefitNode {
                    pos: Pt::new(0.0, 0.0),
                    tangent_rad: None,
                },
            ],
            segments: vec![RefitSegment::Line; 4],
        };
        let (mirrored, saved) = bind_mirror_loop(&chain).expect("closed line loop");
        assert!(saved >= 2);
        assert_eq!(mirrored.nodes[0].pos, mirrored.nodes[4].pos);

        let center = mirrored.nodes[..4]
            .iter()
            .fold(Pt::ZERO, |sum, node| sum + node.pos)
            * 0.25;
        let axis = mirrored.nodes[0].pos - center;
        let u = axis * (1.0 / axis.length());
        let v = Pt::new(-u.y, u.x);
        let a = mirrored.nodes[1].pos - center;
        let b = mirrored.nodes[3].pos - center;
        assert!((a.dot(u) - b.dot(u)).abs() < 1e-12);
        assert!((a.dot(v) + b.dot(v)).abs() < 1e-12);
    }

    fn sample(p: Pt, normal: Pt) -> BoundarySample {
        BoundarySample {
            p,
            normal,
            halfwidth: 0.5,
            confidence: 1.0,
            weight_ds: 0.5,
            corr_length_px: 0.5,
        }
    }

    #[test]
    fn projected_relation_resolve_improves_the_normal_objective() {
        let mut chain = RefitChain {
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
                    pos: Pt::new(11.0, 10.0),
                    tangent_rad: None,
                },
                crate::refit::RefitNode {
                    pos: Pt::new(0.0, 10.0),
                    tangent_rad: None,
                },
                crate::refit::RefitNode {
                    pos: Pt::new(0.0, 0.0),
                    tangent_rad: None,
                },
            ],
            segments: vec![RefitSegment::Line; 4],
        };
        assert!(bind_lines(&mut chain, 0, 2, RelationKind::Parallel));
        let samples = vec![
            sample(Pt::new(2.0, 0.0), Pt::new(0.0, 1.0)),
            sample(Pt::new(8.0, 0.0), Pt::new(0.0, 1.0)),
            sample(Pt::new(2.0, 9.5), Pt::new(0.0, 1.0)),
            sample(Pt::new(8.0, 9.5), Pt::new(0.0, 1.0)),
        ];
        let before = normal_objective(&chain, &samples);
        let (resolved, trace) =
            resolve_constrained(RelationKind::Parallel, &[0, 2], chain, &samples);
        let after = normal_objective(&resolved, &samples);
        assert!(after < before);
        assert!(trace.iter().any(|row| row.accepted));
        let a = resolved.nodes[1].pos - resolved.nodes[0].pos;
        let b = resolved.nodes[3].pos - resolved.nodes[2].pos;
        assert!(a.cross(b).abs() < 1e-9, "projection lost parallelism");
        assert_eq!(resolved.nodes[0].pos, resolved.nodes[4].pos);
    }

    #[test]
    fn finite_difference_matches_the_normal_residual_jacobian() {
        let chain = RefitChain {
            nodes: vec![
                crate::refit::RefitNode {
                    pos: Pt::new(0.0, 0.0),
                    tangent_rad: None,
                },
                crate::refit::RefitNode {
                    pos: Pt::new(10.0, 0.0),
                    tangent_rad: None,
                },
            ],
            segments: vec![RefitSegment::Line],
        };
        let samples = [sample(Pt::new(5.0, 1.0), Pt::new(0.0, 1.0))];
        let eps = 1e-5;
        let shifted = |dy: f64| {
            let mut candidate = chain.clone();
            for node in &mut candidate.nodes {
                node.pos.y += dy;
            }
            normal_residuals(&candidate, &samples).unwrap()[0]
        };
        let jacobian = (shifted(eps) - shifted(-eps)) / (2.0 * eps);
        assert!(
            (jacobian.abs() - 2.0).abs() < 1e-6,
            "normal residual derivative was {jacobian}"
        );
    }
}
