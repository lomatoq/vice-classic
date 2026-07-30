use super::*;

pub(super) fn run_recovery(
    fixture_id: &str,
    mode: &'static str,
    initial: &vice_fit::RefitChain,
    samples: &[vice_evidence::BoundarySample],
    truth: &[Pt],
    perturbation_px: f64,
) -> M7RecoveryRow {
    let perturbed = perturb_chain(initial, perturbation_px);
    let before_error = vice_fit::solve::flatten_chain(&perturbed)
        .ok()
        .and_then(|poly| geometry_error_polylines(&poly, truth).ok())
        .map(|error| error.symmetric_max_px);
    match vice_fit::joint_constrained_refit(&perturbed, samples) {
        Ok(result) => {
            let after_error = vice_fit::solve::flatten_chain(&result.chain)
                .ok()
                .and_then(|poly| geometry_error_polylines(&poly, truth).ok())
                .map(|error| error.symmetric_max_px);
            let (normal_objective_recovered, truth_distance_improved) = classify_recovery(
                result.residual_before,
                result.residual_after,
                before_error,
                after_error,
            );
            M7RecoveryRow {
                fixture_id: fixture_id.into(),
                mode,
                status: "measured",
                perturbation_px,
                residual_before: Some(result.residual_before),
                residual_after: Some(result.residual_after),
                symmetric_max_before_px: before_error,
                symmetric_max_after_px: after_error,
                pass_kept: Some(result.pass_kept),
                normal_objective_recovered,
                truth_distance_improved,
                refusal: None,
            }
        }
        Err(error) => recovery_refusal(
            fixture_id,
            mode,
            perturbation_px,
            &format!("joint recovery solve refused: {error:?}"),
        ),
    }
}

pub(super) fn classify_recovery(
    residual_before: f64,
    residual_after: f64,
    truth_before: Option<f64>,
    truth_after: Option<f64>,
) -> (bool, Option<bool>) {
    (
        residual_after + f64::EPSILON < residual_before,
        truth_before
            .zip(truth_after)
            .map(|(before, after)| after < before),
    )
}

pub(super) fn recovery_refusal(
    fixture_id: &str,
    mode: &'static str,
    perturbation_px: f64,
    reason: &str,
) -> M7RecoveryRow {
    M7RecoveryRow {
        fixture_id: fixture_id.into(),
        mode,
        status: "refused",
        perturbation_px,
        residual_before: None,
        residual_after: None,
        symmetric_max_before_px: None,
        symmetric_max_after_px: None,
        pass_kept: None,
        normal_objective_recovered: false,
        truth_distance_improved: None,
        refusal: Some(reason.into()),
    }
}

fn perturb_chain(chain: &vice_fit::RefitChain, delta: f64) -> vice_fit::RefitChain {
    let mut out = chain.clone();
    let closed =
        out.nodes.len() >= 2 && out.nodes.first().unwrap().pos == out.nodes.last().unwrap().pos;
    let unique_nodes = out.nodes.len().saturating_sub(usize::from(closed));
    for (index, node) in out.nodes.iter_mut().take(unique_nodes).enumerate() {
        let sign = if index.is_multiple_of(2) { 1.0 } else { -1.0 };
        node.pos.x += sign * delta;
        node.pos.y -= sign * delta * 0.5;
        if let Some(tangent) = &mut node.tangent_rad {
            *tangent = vice_fit::canonical_angle(*tangent + sign * 0.01);
        }
    }
    if closed {
        let first = out.nodes[0];
        let last = out.nodes.len() - 1;
        out.nodes[last] = first;
    }
    for (index, segment) in out.segments.iter_mut().enumerate() {
        let sign = if index.is_multiple_of(2) { 1.0 } else { -1.0 };
        match segment {
            vice_fit::RefitSegment::Line
            | vice_fit::RefitSegment::Arc(
                vice_fit::ArcAnchor::FromHeadTangent | vice_fit::ArcAnchor::FromTailTangent,
            ) => {}
            vice_fit::RefitSegment::Arc(vice_fit::ArcAnchor::Radius { radius_px, .. }) => {
                *radius_px = (*radius_px + sign * delta).max(delta);
            }
            vice_fit::RefitSegment::Quad { ctrl } => perturb_handle(ctrl, sign, delta),
            vice_fit::RefitSegment::Cubic { head, tail } => {
                perturb_handle(head, sign, delta);
                perturb_handle(tail, -sign, delta);
            }
        }
    }
    out
}

fn perturb_handle(handle: &mut vice_fit::Handle, sign: f64, delta: f64) {
    match handle {
        vice_fit::Handle::Free(point) => {
            point.x += sign * delta;
            point.y -= sign * delta * 0.5;
        }
        vice_fit::Handle::Shared { length_px } => {
            *length_px = (*length_px + sign * delta).max(delta);
        }
    }
}

pub(super) fn flatten_truth_segment(
    segment: &Segment,
    p0: Pt,
    p1: Pt,
    tolerance: ChordTolerancePx,
) -> Result<Vec<Pt>, String> {
    let points = match *segment {
        Segment::Line => vec![p0, p1],
        Segment::Quad { ctrl } => vice_geom::flatten::flatten_quad(p0, ctrl, p1, tolerance).points,
        Segment::Cubic { ctrl1, ctrl2 } => {
            vice_geom::flatten::flatten_cubic(p0, ctrl1, ctrl2, p1, tolerance).points
        }
        Segment::CircularArc {
            radius_px,
            large_arc,
            ccw,
        } => {
            vice_geom::flatten::flatten_circular_arc(p0, p1, radius_px, large_arc, ccw, tolerance)
                .map_err(|e| format!("circular arc does not flatten: {e}"))?
                .points
        }
        Segment::EllipticArc { .. } => return Err("elliptic arc is not an M6 fit family".into()),
    };
    (points.len() >= 2)
        .then_some(points)
        .ok_or_else(|| "truth segment flattened to fewer than two points".to_string())
}

pub(super) fn measure_boundary(
    observation: &RasterBoundObservation,
    config: &GeometryOracleConfig,
    config_hash: &str,
    fixture_set_hash: &str,
) -> Result<(GeometryBoundaryRow, Option<vice_fit::RefitChain>), String> {
    let auto = k_best_boundary_models(
        &observation.chain,
        &FIT_BUDGET_V1,
        config.canvas_dim_px,
        config.k_discrete_paths,
    )
    .map_err(|e| format!("G00 automatic candidate generation refused: {e:?}"))?;
    if auto.models.is_empty() {
        return Err(format!(
            "G00 produced no accepted model: candidates {}, paths {}, refused {:?}",
            auto.candidates, auto.discrete_paths, auto.refused
        ));
    }
    let forced = fit_forced_boundary_models(
        &observation.forced_chain,
        &observation.gt_families,
        &observation.gt_breakpoints,
        config.canvas_dim_px,
        config.k_discrete_paths,
    )
    .map_err(|e| format!("G20 forced discrete fit refused: {e:?}"))?;
    if forced.models.is_empty() {
        return Err("G20 produced no accepted model".to_string());
    }
    if forced.models.iter().any(|model| {
        model.families != observation.gt_families || model.breakpoints != observation.gt_breakpoints
    }) {
        return Err("G20 changed a forced family or breakpoint".to_string());
    }

    let auto_first = &auto.models[0];
    let forced_first = &forced.models[0];
    let auto_oracle = oracle_select(&auto.models, &observation.truth)?;
    let forced_oracle = oracle_select(&forced.models, &observation.truth)?;
    let (union_first, union_source) = auto
        .models
        .iter()
        .map(|model| (model, "automatic"))
        .chain(forced.models.iter().map(|model| (model, "forced_gt")))
        .min_by(|(a, _), (b, _)| {
            a.code
                .total_bits()
                .total_cmp(&b.code.total_bits())
                .then(a.proposal_cost_px.total_cmp(&b.proposal_cost_px))
        })
        .ok_or_else(|| "G10 union is empty".to_string())?;
    let key = compatibility_key(config, config_hash, fixture_set_hash);
    let context = ArmContext {
        key: &key,
        truth: &observation.truth,
    };

    let arms = vec![
        arm_result("G00", "automatic", auto_first, auto.models.len(), &context)?,
        arm_result(
            "G10",
            union_source,
            union_first,
            auto.models.len() + forced.models.len(),
            &context,
        )?,
        arm_result("G01", "automatic", auto_oracle, auto.models.len(), &context)?,
        arm_result(
            "G11",
            "forced_gt",
            forced_oracle,
            forced.models.len(),
            &context,
        )?,
        arm_result(
            "G20",
            "forced_gt",
            forced_first,
            forced.models.len(),
            &context,
        )?,
    ];
    if arms[2].error.symmetric_max_px > arms[0].error.symmetric_max_px + f64::EPSILON {
        return Err("G01 oracle selector is worse than G00 on the same candidate set".to_string());
    }
    if arms[3].error.symmetric_max_px > arms[4].error.symmetric_max_px + f64::EPSILON {
        return Err("G11 oracle selector is worse than G20 on the same candidate set".to_string());
    }

    let oracle_selector_changed = arms[2].geometry_sha256 != arms[0].geometry_sha256;
    let injection_selector_changed = arms[1].geometry_sha256 != arms[0].geometry_sha256;
    let forced_selector_changed = arms[3].geometry_sha256 != arms[4].geometry_sha256;
    let g20_chain = forced_first.geometry.typed_chain().cloned();
    Ok((
        GeometryBoundaryRow {
            fixture_id: observation.fixture_id.clone(),
            scene_id: observation.scene_id.clone(),
            boundary_id: observation.boundary_id,
            samples: observation.chain.samples.len(),
            gt_families: observation
                .gt_families
                .iter()
                .map(|family| family.universe_name())
                .collect(),
            gt_breakpoints: observation.gt_breakpoints.clone(),
            stage_f_truth_match_px: observation.stage_f_truth_match_px,
            render_cell: observation.render_cell.clone(),
            injected_models: forced.models.len(),
            oracle_selector_changed,
            injection_selector_changed,
            forced_selector_changed,
            arms,
        },
        g20_chain,
    ))
}
