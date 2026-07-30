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
    let g =
        fit_regular_polygon(&points, 6, &s, &crate::GEOMETRY_CODE_TABLE_V1).expect("hexagon fit");
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

#[test]
fn every_whole_loop_family_lowers_to_native_closed_ir() {
    for kind in LoopPrimitiveKind::ALL {
        let mut geometry = LoopPrimitiveGeometry {
            center: Pt::new(20.0, 30.0),
            axis_angle_rad: 0.3,
            half_width_px: 9.0,
            half_height_px: 5.0,
            corner_radius_px: 2.0,
            sides: (kind == LoopPrimitiveKind::RegularPolygon).then_some(6),
        };
        if kind == LoopPrimitiveKind::Circle {
            geometry.half_height_px = geometry.half_width_px;
        }
        let lowered = lower_loop_primitive(kind, geometry).expect("native primitive");
        assert!(!lowered.boundary.curve.segments.is_empty());
        assert_eq!(
            lowered.boundary.curve.interior_nodes.len() + 1,
            lowered.boundary.curve.segments.len()
        );
        if matches!(
            kind,
            LoopPrimitiveKind::Circle
                | LoopPrimitiveKind::Ellipse
                | LoopPrimitiveKind::RoundedRect
                | LoopPrimitiveKind::Capsule
        ) {
            assert!(matches!(
                lowered.boundary.closure_join,
                Some(JoinKind::SmoothG1 { .. })
            ));
            let readings =
                crate::g1_readings(&lowered.boundary.curve, lowered.start, lowered.start);
            assert!(
                readings
                    .iter()
                    .all(|reading| reading.spread_rad <= crate::GATE_MAX_G1_SPREAD_RAD),
                "{kind:?}: {readings:?}"
            );
            let Some(JoinKind::SmoothG1 { tangent_angle_rad }) = lowered.boundary.closure_join
            else {
                unreachable!()
            };
            if kind != LoopPrimitiveKind::Ellipse {
                assert!(
                    crate::closure_g1_spread_rad(
                        &lowered.boundary.curve,
                        lowered.start,
                        lowered.start,
                        tangent_angle_rad,
                    )
                    .expect("closure tangent")
                        <= crate::GATE_MAX_G1_SPREAD_RAD
                );
            }
        }
    }
}
