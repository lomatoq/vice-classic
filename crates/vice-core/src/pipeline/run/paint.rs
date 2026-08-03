use crate::candidate::MaterializedCandidate;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct PaintRiskMetrics {
    pub calibration_class: String,
    pub evidence_palette_shift_codes: u8,
    pub palette_support_px: u64,
    pub palette_interval_radius_codes: u8,
}

fn evidence_kind(color: &vice_evidence::ColorHypothesis) -> &'static str {
    match color {
        vice_evidence::ColorHypothesis::Point { .. } => "point",
        vice_evidence::ColorHypothesis::Interval { .. } => "interval",
    }
}

fn encoded(color: vice_ir::LinearRgb) -> [u8; 3] {
    [color.r, color.g, color.b].map(vice_ir::color::linear_to_srgb_u8)
}

fn max_code_delta(left: [u8; 3], right: [u8; 3]) -> u8 {
    left.into_iter()
        .zip(right)
        .map(|(left, right)| left.abs_diff(right))
        .max()
        .unwrap_or(0)
}

fn interval_radius_codes(color: &vice_evidence::ColorHypothesis) -> u8 {
    match color {
        vice_evidence::ColorHypothesis::Point { .. } => 0,
        vice_evidence::ColorHypothesis::Interval { lo, hi, center, .. } => {
            max_code_delta(encoded(*center), encoded(*lo))
                .max(max_code_delta(encoded(*center), encoded(*hi)))
        }
    }
}

fn delivery_profile(hypothesis_id: &str) -> &str {
    hypothesis_id
        .split_once('/')
        .map_or("unknown", |(_, profile)| profile)
}

fn calibration_class(
    hypothesis_id: &str,
    foreground: &vice_evidence::ColorHypothesis,
    background: &vice_evidence::BackgroundHypothesis,
) -> String {
    let background_kind = match background {
        vice_evidence::BackgroundHypothesis::TransparentExterior => "transparent",
        vice_evidence::BackgroundHypothesis::OpaqueFace(color) => evidence_kind(color),
    };
    format!(
        "{}|delivery:{}|paint:fg-{}+bg-{}",
        crate::selection_calibration_class(hypothesis_id),
        delivery_profile(hypothesis_id),
        evidence_kind(foreground),
        background_kind,
    )
}

fn palette_shift(expected: &[[u8; 3]], actual: &[[u8; 3]]) -> u8 {
    match (expected, actual) {
        ([want], [got]) => max_code_delta(*want, *got),
        ([want_a, want_b], [got_a, got_b]) => {
            let direct = max_code_delta(*want_a, *got_a).max(max_code_delta(*want_b, *got_b));
            let swapped = max_code_delta(*want_a, *got_b).max(max_code_delta(*want_b, *got_a));
            direct.min(swapped)
        }
        _ => u8::MAX,
    }
}

/// Bind final serialized paint to independent input-side palette evidence.
///
/// This court is deliberately production-observable: it uses only the
/// selected canonical scene and the Flat2 evidence that existed before
/// ground truth was consulted. Palette matching is label-swap invariant.
pub(super) fn paint_risk_metrics(
    selected: &MaterializedCandidate,
    evidence: &vice_evidence::Flat2Evidence,
) -> PaintRiskMetrics {
    let foreground = &evidence.hypothesis.foreground;
    let background = match &evidence.hypothesis.background {
        vice_evidence::BackgroundHypothesis::TransparentExterior => None,
        vice_evidence::BackgroundHypothesis::OpaqueFace(color) => Some(color),
    };
    let calibration_class = calibration_class(
        &selected.summary.hypothesis_id,
        foreground,
        &evidence.hypothesis.background,
    );
    let mut expected = vec![encoded(foreground.center())];
    let mut palette_support_px = foreground.support_px();
    let mut palette_interval_radius_codes = interval_radius_codes(foreground);
    if let Some(background) = background {
        expected.push(encoded(background.center()));
        palette_support_px = palette_support_px.min(background.support_px());
        palette_interval_radius_codes =
            palette_interval_radius_codes.max(interval_radius_codes(background));
    }
    let selected_scene = vice_ir::parse_scene(&selected.scene_json);
    let mut actual = selected_scene
        .ok()
        .into_iter()
        .flat_map(|scene| scene.graph.faces.into_iter())
        .filter_map(|face| match face.paint {
            vice_ir::Paint::OpaqueSolid(color) => Some(encoded(color)),
            vice_ir::Paint::TransparentExterior => None,
        })
        .collect::<Vec<_>>();
    actual.sort_unstable();
    actual.dedup();
    expected.sort_unstable();
    expected.dedup();
    let evidence_palette_shift_codes = palette_shift(&expected, &actual);
    PaintRiskMetrics {
        calibration_class,
        evidence_palette_shift_codes,
        palette_support_px,
        palette_interval_radius_codes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(code: u8) -> vice_evidence::ColorHypothesis {
        let linear = vice_ir::color::srgb_u8_to_linear(code);
        vice_evidence::ColorHypothesis::Point {
            color: vice_ir::LinearRgb::new(linear, linear, linear),
            support_px: 16,
        }
    }

    #[test]
    fn max_code_delta_is_channelwise_and_symmetric() {
        assert_eq!(max_code_delta([1, 7, 9], [4, 2, 8]), 5);
        assert_eq!(max_code_delta([4, 2, 8], [1, 7, 9]), 5);
    }

    #[test]
    fn palette_shift_is_label_swap_invariant_and_bounds_perturbations() {
        assert_eq!(palette_shift(&[[10, 20, 30]], &[[12, 20, 29]]), 2);
        assert_eq!(
            palette_shift(
                &[[10, 20, 30], [200, 210, 220]],
                &[[201, 210, 220], [9, 21, 30]]
            ),
            1
        );
        assert_eq!(palette_shift(&[[10, 20, 30]], &[]), u8::MAX);
    }

    #[test]
    fn joint_classes_cover_native_and_general_delivery_profiles() {
        let foreground = point(20);
        let transparent = vice_evidence::BackgroundHypothesis::TransparentExterior;
        let opaque = vice_evidence::BackgroundHypothesis::OpaqueFace(point(220));
        for (id, want) in [
            (
                "c0-path0-primitive-0-circle/t0/srgb/box/u8/opaque",
                "flat2/native-primitive/circle",
            ),
            (
                "c0-path0-primitive-0-ellipse/t0/lin/box/u8/transparent",
                "flat2/native-primitive/ellipse",
            ),
            (
                "c0-path0-primitive-0-regularpolygon/t0/srgb/box/u8/opaque",
                "flat2/native-primitive/regularpolygon",
            ),
            (
                "observed-polyline-rescue/t0/lin/box/u8/transparent",
                "flat2/general",
            ),
        ] {
            assert!(calibration_class(id, &foreground, &opaque).starts_with(want));
        }
        assert!(calibration_class(
            "c0-path0-primitive-0-circle/t0/lin/box/u8/transparent",
            &foreground,
            &transparent,
        )
        .ends_with("paint:fg-point+bg-transparent"));
    }
}
