//! §15 whole-loop primitive hypotheses.
//!
//! These are constrained siblings of the free typed chain, not shape
//! detectors.  Every hypothesis publishes the complete MDL trade:
//!
//! `free chain structure - primitive structure - residual change`.
//!
//! A primitive is eligible only for a chain that the evidence marked closed,
//! only when the resulting boundary remains inside every evidence corridor,
//! and only when the trade saves bits.  Native SVG emission is deliberately
//! not decided here: §15 additionally requires canonical-boundary identity,
//! shared-neighbour ownership and post-quantization verification, all of which
//! belong to the scene-bound M7 verifier.

use serde::Serialize;
use vice_evidence::BoundarySample;
use vice_geom::Pt;

use crate::code::{independent_observations, residual_bits, GeometryCodeTable};
use crate::models::BoundaryModel;

/// The finite whole-loop primitive universe named by §15.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LoopPrimitiveKind {
    Circle,
    Ellipse,
    Rect,
    RotatedRect,
    RoundedRect,
    Capsule,
    RegularPolygon,
}

impl LoopPrimitiveKind {
    pub const ALL: [LoopPrimitiveKind; 7] = [
        LoopPrimitiveKind::Circle,
        LoopPrimitiveKind::Ellipse,
        LoopPrimitiveKind::Rect,
        LoopPrimitiveKind::RotatedRect,
        LoopPrimitiveKind::RoundedRect,
        LoopPrimitiveKind::Capsule,
        LoopPrimitiveKind::RegularPolygon,
    ];

    pub fn universe_name(self) -> &'static str {
        match self {
            LoopPrimitiveKind::Circle => "circle",
            LoopPrimitiveKind::Ellipse => "ellipse",
            LoopPrimitiveKind::Rect => "rect",
            LoopPrimitiveKind::RotatedRect => "rotated_rect",
            LoopPrimitiveKind::RoundedRect => "rounded_rect",
            LoopPrimitiveKind::Capsule => "capsule",
            LoopPrimitiveKind::RegularPolygon => "regular_polygon",
        }
    }

    /// Coordinate-like continuous parameters coded in pixels.
    pub fn coordinate_parameters(self) -> usize {
        match self {
            LoopPrimitiveKind::Circle => 3,         // cx, cy, r
            LoopPrimitiveKind::Ellipse => 4,        // cx, cy, rx, ry
            LoopPrimitiveKind::Rect => 4,           // cx, cy, hx, hy
            LoopPrimitiveKind::RotatedRect => 4,    // cx, cy, hx, hy
            LoopPrimitiveKind::RoundedRect => 5,    // rotated rect + corner r
            LoopPrimitiveKind::Capsule => 4,        // cx, cy, half-length, r
            LoopPrimitiveKind::RegularPolygon => 3, // cx, cy, circumradius
        }
    }

    /// Angle-like parameters coded in radians.
    pub fn angle_parameters(self) -> usize {
        match self {
            LoopPrimitiveKind::Circle | LoopPrimitiveKind::Rect => 0,
            LoopPrimitiveKind::Ellipse
            | LoopPrimitiveKind::RotatedRect
            | LoopPrimitiveKind::RoundedRect
            | LoopPrimitiveKind::Capsule
            | LoopPrimitiveKind::RegularPolygon => 1,
        }
    }

    pub fn free_parameters(self) -> usize {
        self.coordinate_parameters() + self.angle_parameters()
    }

    /// Discrete parameters inside a primitive family.
    pub fn flag_bits(self) -> f64 {
        match self {
            // Sides 3..=12: ten explicitly searched values.
            LoopPrimitiveKind::RegularPolygon => 10f64.log2(),
            _ => 0.0,
        }
    }

    pub fn code_bits(
        self,
        table: &GeometryCodeTable,
        canvas_dim_px: f64,
        characteristic_radius_px: f64,
    ) -> f64 {
        // Uniform prefix over the seven finite families.  This function is
        // called by `pricing_surface_v1`, so changing either the family count
        // or a parameter count moves the frozen pricing hash.
        let angular_precision_rad = (table.coordinate_precision_px()
            / characteristic_radius_px.max(table.coordinate_precision_px()))
        .min(std::f64::consts::TAU);
        let angle_bits = (std::f64::consts::TAU / angular_precision_rad).log2();
        (Self::ALL.len() as f64).log2()
            + self.coordinate_parameters() as f64 * table.coordinate_bits(canvas_dim_px)
            + self.angle_parameters() as f64 * angle_bits
            + self.flag_bits()
    }
}

/// Canonical parameters of one fitted hypothesis.  The fields have one
/// interpretation for all families, keeping the serialized witness compact:
///
/// - `(half_width, half_height)` are radii/extents in the rotated local frame;
/// - `corner_radius` is non-zero only for rounded rectangles/capsules;
/// - `sides` is present only for regular polygons.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct LoopPrimitiveGeometry {
    pub center: Pt,
    pub axis_angle_rad: f64,
    pub half_width_px: f64,
    pub half_height_px: f64,
    pub corner_radius_px: f64,
    pub sides: Option<u8>,
}

/// One constrained whole-loop sibling and its complete comparison with the
/// free typed chain.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LoopPrimitiveHypothesis {
    pub kind: LoopPrimitiveKind,
    pub geometry: LoopPrimitiveGeometry,
    /// The exact tessellated witness used for residual/corridor evaluation.
    pub verification_polyline: Vec<Pt>,
    pub primitive_bits: f64,
    pub free_structure_bits: f64,
    pub residual_penalty_bits: f64,
    pub net_bits: f64,
    pub worst_normal_deviation_px: f64,
    pub worst_model_to_evidence_px: f64,
    pub allowed_px: f64,
    pub accepted: bool,
}

/// Form every §15 whole-loop primitive hypothesis for a closed chain.
///
/// Polygon side counts are separate hypotheses because the integer is a real
/// discrete decision.  Rejected hypotheses are retained alongside winners.
pub fn loop_primitive_hypotheses(
    model: &BoundaryModel,
    samples: &[BoundarySample],
    table: &GeometryCodeTable,
    canvas_dim_px: f64,
    closed: bool,
) -> Vec<LoopPrimitiveHypothesis> {
    if !closed || samples.len() < 4 {
        return Vec::new();
    }
    let points = unique_loop_points(samples);
    if points.len() < 3 {
        return Vec::new();
    }
    let Ok(free_poly) = model.geometry.flatten() else {
        return Vec::new();
    };
    let free_residual = polyline_residual(&free_poly, samples, table);
    let free_structure_bits = model.code.geometry_bits + model.code.topology_bits;
    let mut candidates = Vec::new();

    if let Some(g) = fit_circle(&points) {
        candidates.push((LoopPrimitiveKind::Circle, g));
    }
    if let Some(g) = fit_box(&points, principal_angle(&points), 0.0, None) {
        candidates.push((LoopPrimitiveKind::Ellipse, g));
        candidates.push((LoopPrimitiveKind::RotatedRect, g));
    }
    if let Some(g) = fit_box(&points, 0.0, 0.0, None) {
        candidates.push((LoopPrimitiveKind::Rect, g));
    }
    if let Some(g) = fit_rounded_rect(&points) {
        candidates.push((LoopPrimitiveKind::RoundedRect, g));
    }
    if let Some(g) = fit_capsule(&points) {
        candidates.push((LoopPrimitiveKind::Capsule, g));
    }
    for sides in 3u8..=12 {
        if let Some(g) = fit_regular_polygon(&points, sides, samples, table) {
            candidates.push((LoopPrimitiveKind::RegularPolygon, g));
        }
    }

    candidates
        .into_iter()
        .filter_map(|(kind, geometry)| {
            let poly = primitive_polyline(kind, geometry, table.coordinate_precision_px())?;
            let after = polyline_residual(&poly, samples, table);
            let residual_penalty_bits = after - free_residual;
            let characteristic_radius_px = geometry.half_width_px.max(geometry.half_height_px);
            let primitive_bits = kind.code_bits(table, canvas_dim_px, characteristic_radius_px);
            let net_bits = free_structure_bits - primitive_bits - residual_penalty_bits;
            let forward = crate::solve::evidence_to_model_corridor(&poly, samples);
            let reverse = crate::solve::model_to_evidence_corridor(&poly, samples);
            Some(LoopPrimitiveHypothesis {
                kind,
                geometry,
                verification_polyline: poly,
                primitive_bits,
                free_structure_bits,
                residual_penalty_bits,
                net_bits,
                worst_normal_deviation_px: forward.deviation_px,
                worst_model_to_evidence_px: reverse.deviation_px,
                allowed_px: forward.allowed_px,
                accepted: net_bits > 0.0
                    && forward.feasible()
                    && reverse.feasible()
                    && after.is_finite()
                    && primitive_bits.is_finite(),
            })
        })
        .collect()
}

/// Apply the shortest individually admissible primitive sibling.
///
/// The caller compares the resulting code with the relation-constrained
/// sibling before keeping either, so primitive and relation savings cannot be
/// counted twice.
pub fn apply_best_primitive(
    model: &mut BoundaryModel,
    hypotheses: &[LoopPrimitiveHypothesis],
) -> Option<usize> {
    let (index, best) = hypotheses
        .iter()
        .enumerate()
        .filter(|(_, h)| h.accepted)
        .max_by(|(_, a), (_, b)| a.net_bits.total_cmp(&b.net_bits))?;
    model.code.geometry_bits = best.primitive_bits;
    model.code.topology_bits = 0.0;
    model.code.relation_bits = 0.0;
    model.code.residual_bits += best.residual_penalty_bits;
    model.geometry = crate::models::SelectedBoundaryGeometry::LoopPrimitive {
        kind: best.kind,
        geometry: best.geometry,
        verification_polyline: best.verification_polyline.clone(),
    };
    model.worst_normal_deviation_px = best.worst_normal_deviation_px;
    model.worst_model_to_evidence_px = best.worst_model_to_evidence_px;
    Some(index)
}

fn unique_loop_points(samples: &[BoundarySample]) -> Vec<Pt> {
    let mut points: Vec<Pt> = samples.iter().map(|s| s.p).collect();
    if points.len() > 1 && (points[0] - points[points.len() - 1]).length() <= 1e-9 {
        points.pop();
    }
    points
}

fn mean(points: &[Pt]) -> Pt {
    points.iter().copied().fold(Pt::ZERO, |sum, p| sum + p) * (1.0 / points.len() as f64)
}

fn principal_angle(points: &[Pt]) -> f64 {
    let c = mean(points);
    let (xx, xy, yy) = points.iter().fold((0.0, 0.0, 0.0), |(xx, xy, yy), p| {
        let d = *p - c;
        (xx + d.x * d.x, xy + d.x * d.y, yy + d.y * d.y)
    });
    0.5 * (2.0 * xy).atan2(xx - yy)
}

fn axes(angle: f64) -> (Pt, Pt) {
    let u = Pt::new(angle.cos(), angle.sin());
    (u, Pt::new(-u.y, u.x))
}

fn fit_circle(points: &[Pt]) -> Option<LoopPrimitiveGeometry> {
    let center = mean(points);
    let radius = points.iter().map(|p| (*p - center).length()).sum::<f64>() / points.len() as f64;
    (radius.is_finite() && radius > 0.0).then_some(LoopPrimitiveGeometry {
        center,
        axis_angle_rad: 0.0,
        half_width_px: radius,
        half_height_px: radius,
        corner_radius_px: 0.0,
        sides: None,
    })
}

fn fit_box(
    points: &[Pt],
    angle: f64,
    corner_radius_px: f64,
    sides: Option<u8>,
) -> Option<LoopPrimitiveGeometry> {
    let origin = mean(points);
    let (u, v) = axes(angle);
    let mut min_u = f64::INFINITY;
    let mut max_u = f64::NEG_INFINITY;
    let mut min_v = f64::INFINITY;
    let mut max_v = f64::NEG_INFINITY;
    for p in points {
        let d = *p - origin;
        let x = d.dot(u);
        let y = d.dot(v);
        min_u = min_u.min(x);
        max_u = max_u.max(x);
        min_v = min_v.min(y);
        max_v = max_v.max(y);
    }
    let hw = 0.5 * (max_u - min_u);
    let hh = 0.5 * (max_v - min_v);
    if !(hw.is_finite() && hh.is_finite() && hw > 0.0 && hh > 0.0) {
        return None;
    }
    Some(LoopPrimitiveGeometry {
        center: origin + u * (0.5 * (min_u + max_u)) + v * (0.5 * (min_v + max_v)),
        axis_angle_rad: angle,
        half_width_px: hw,
        half_height_px: hh,
        corner_radius_px,
        sides,
    })
}

fn fit_rounded_rect(points: &[Pt]) -> Option<LoopPrimitiveGeometry> {
    let mut g = fit_box(points, principal_angle(points), 0.0, None)?;
    // Radius is fitted on a finite deterministic grid.  It is a proposal
    // mechanism only; the exact MDL/corridor judge below makes the decision.
    let limit = g.half_width_px.min(g.half_height_px);
    let mut best = (f64::INFINITY, 0.0);
    for step in 1..=10 {
        let r = limit * step as f64 / 20.0;
        g.corner_radius_px = r;
        if let Some(poly) = primitive_polyline(
            LoopPrimitiveKind::RoundedRect,
            g,
            crate::GEOMETRY_CODE_TABLE_V1.coordinate_precision_px(),
        ) {
            let err = points
                .iter()
                .map(|p| crate::cost::euclidean_deviation(*p, &poly))
                .sum::<f64>();
            if err < best.0 {
                best = (err, r);
            }
        }
    }
    g.corner_radius_px = best.1;
    (best.0.is_finite() && best.1 > 0.0).then_some(g)
}

fn fit_capsule(points: &[Pt]) -> Option<LoopPrimitiveGeometry> {
    let mut g = fit_box(points, principal_angle(points), 0.0, None)?;
    if g.half_height_px > g.half_width_px {
        std::mem::swap(&mut g.half_width_px, &mut g.half_height_px);
        g.axis_angle_rad += std::f64::consts::FRAC_PI_2;
    }
    g.corner_radius_px = g.half_height_px;
    Some(g)
}

fn fit_regular_polygon(
    points: &[Pt],
    sides: u8,
    samples: &[BoundarySample],
    table: &GeometryCodeTable,
) -> Option<LoopPrimitiveGeometry> {
    let center = mean(points);
    let radius = points.iter().map(|p| (*p - center).length()).sum::<f64>() / points.len() as f64;
    if !(radius.is_finite() && radius > 0.0) {
        return None;
    }
    let period = std::f64::consts::TAU / f64::from(sides);
    let mut best: Option<(f64, LoopPrimitiveGeometry)> = None;
    for step in 0..32 {
        let phase = period * step as f64 / 32.0;
        let g = LoopPrimitiveGeometry {
            center,
            axis_angle_rad: phase,
            half_width_px: radius,
            half_height_px: radius,
            corner_radius_px: 0.0,
            sides: Some(sides),
        };
        let poly = primitive_polyline(
            LoopPrimitiveKind::RegularPolygon,
            g,
            table.coordinate_precision_px(),
        )?;
        let score = polyline_residual(&poly, samples, table);
        if best.as_ref().is_none_or(|(s, _)| score < *s) {
            best = Some((score, g));
        }
    }
    best.map(|(_, g)| g)
}

fn primitive_polyline(
    kind: LoopPrimitiveKind,
    g: LoopPrimitiveGeometry,
    precision_px: f64,
) -> Option<Vec<Pt>> {
    let valid = [
        g.center.x,
        g.center.y,
        g.axis_angle_rad,
        g.half_width_px,
        g.half_height_px,
        g.corner_radius_px,
    ]
    .iter()
    .all(|v| v.is_finite())
        && g.half_width_px > 0.0
        && g.half_height_px > 0.0;
    if !valid {
        return None;
    }
    match kind {
        LoopPrimitiveKind::Circle | LoopPrimitiveKind::Ellipse => {
            let steps = arc_steps(g.half_width_px.max(g.half_height_px), precision_px * 0.05);
            Some(
                (0..=steps)
                    .map(|i| {
                        let a = std::f64::consts::TAU * i as f64 / steps as f64;
                        local_to_world(
                            g,
                            Pt::new(g.half_width_px * a.cos(), g.half_height_px * a.sin()),
                        )
                    })
                    .collect(),
            )
        }
        LoopPrimitiveKind::Rect | LoopPrimitiveKind::RotatedRect => {
            let mut p = vec![
                Pt::new(g.half_width_px, g.half_height_px),
                Pt::new(-g.half_width_px, g.half_height_px),
                Pt::new(-g.half_width_px, -g.half_height_px),
                Pt::new(g.half_width_px, -g.half_height_px),
            ];
            p.push(p[0]);
            Some(p.into_iter().map(|q| local_to_world(g, q)).collect())
        }
        LoopPrimitiveKind::RoundedRect | LoopPrimitiveKind::Capsule => {
            let r = g
                .corner_radius_px
                .min(g.half_width_px)
                .min(g.half_height_px);
            if r <= 0.0 {
                return None;
            }
            let per_corner = (arc_steps(r, precision_px * 0.05) / 4).max(2);
            let centers = [
                Pt::new(g.half_width_px - r, g.half_height_px - r),
                Pt::new(-g.half_width_px + r, g.half_height_px - r),
                Pt::new(-g.half_width_px + r, -g.half_height_px + r),
                Pt::new(g.half_width_px - r, -g.half_height_px + r),
            ];
            let starts = [
                0.0,
                std::f64::consts::FRAC_PI_2,
                std::f64::consts::PI,
                3.0 * std::f64::consts::FRAC_PI_2,
            ];
            let mut out = Vec::with_capacity(4 * per_corner + 1);
            for (center, start) in centers.into_iter().zip(starts) {
                for i in 0..per_corner {
                    let a = start + std::f64::consts::FRAC_PI_2 * i as f64 / per_corner as f64;
                    out.push(local_to_world(
                        g,
                        center + Pt::new(r * a.cos(), r * a.sin()),
                    ));
                }
            }
            out.push(out[0]);
            Some(out)
        }
        LoopPrimitiveKind::RegularPolygon => {
            let sides = usize::from(g.sides?);
            if !(3..=12).contains(&sides) {
                return None;
            }
            Some(
                (0..=sides)
                    .map(|i| {
                        let a = g.axis_angle_rad + std::f64::consts::TAU * i as f64 / sides as f64;
                        g.center + Pt::new(g.half_width_px * a.cos(), g.half_width_px * a.sin())
                    })
                    .collect(),
            )
        }
    }
}

fn local_to_world(g: LoopPrimitiveGeometry, p: Pt) -> Pt {
    let (u, v) = axes(g.axis_angle_rad);
    g.center + u * p.x + v * p.y
}

fn arc_steps(radius: f64, tolerance: f64) -> usize {
    if radius <= tolerance {
        return 32;
    }
    let half_angle = (1.0 - tolerance / radius).clamp(-1.0, 1.0).acos();
    (std::f64::consts::PI / half_angle).ceil().max(32.0) as usize
}

fn polyline_residual(poly: &[Pt], samples: &[BoundarySample], table: &GeometryCodeTable) -> f64 {
    let precision = table.coordinate_precision_px();
    samples
        .iter()
        .try_fold(0.0, |total, s| {
            let dn = crate::cost::normal_deviation(s.p, s.normal, poly)
                .map_or_else(|| crate::cost::euclidean_deviation(s.p, poly), f64::abs);
            let w = independent_observations(s.weight_ds, s.corr_length_px)?;
            Some(total + w * residual_bits(dn, s.halfwidth, precision))
        })
        .unwrap_or(f64::INFINITY)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples(points: &[Pt]) -> Vec<BoundarySample> {
        let center = mean(points);
        points
            .iter()
            .map(|p| BoundarySample {
                p: *p,
                normal: (*p - center) * (1.0 / (*p - center).length()),
                halfwidth: 0.5,
                confidence: 1.0,
                weight_ds: 1.0,
                corr_length_px: 1.0,
            })
            .collect()
    }

    #[test]
    fn every_kind_has_a_finite_positive_price() {
        for kind in LoopPrimitiveKind::ALL {
            let bits = kind.code_bits(&crate::GEOMETRY_CODE_TABLE_V1, 256.0, 64.0);
            assert!(bits.is_finite() && bits > 0.0, "{kind:?}: {bits}");
        }
    }

    #[test]
    fn a_regular_hexagon_fit_keeps_the_side_count_explicit() {
        let points: Vec<Pt> = (0..6)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / 6.0;
                Pt::new(20.0 + 8.0 * a.cos(), 30.0 + 8.0 * a.sin())
            })
            .collect();
        let s = samples(&points);
        let g = fit_regular_polygon(&points, 6, &s, &crate::GEOMETRY_CODE_TABLE_V1)
            .expect("hexagon fit");
        assert_eq!(g.sides, Some(6));
        let poly = primitive_polyline(
            LoopPrimitiveKind::RegularPolygon,
            g,
            crate::GEOMETRY_CODE_TABLE_V1.coordinate_precision_px(),
        )
        .unwrap();
        assert_eq!(poly.len(), 7);
    }

    #[test]
    fn capsule_and_rounded_rect_are_distinct_finite_hypotheses() {
        let points = vec![
            Pt::new(-10.0, -4.0),
            Pt::new(10.0, -4.0),
            Pt::new(14.0, 0.0),
            Pt::new(10.0, 4.0),
            Pt::new(-10.0, 4.0),
            Pt::new(-14.0, 0.0),
        ];
        let capsule = fit_capsule(&points).unwrap();
        let rounded = fit_rounded_rect(&points).unwrap();
        assert!(capsule.corner_radius_px > 0.0);
        assert!(rounded.corner_radius_px > 0.0);
        assert!(primitive_polyline(
            LoopPrimitiveKind::Capsule,
            capsule,
            crate::GEOMETRY_CODE_TABLE_V1.coordinate_precision_px()
        )
        .is_some());
        assert!(primitive_polyline(
            LoopPrimitiveKind::RoundedRect,
            rounded,
            crate::GEOMETRY_CODE_TABLE_V1.coordinate_precision_px()
        )
        .is_some());
    }
}
