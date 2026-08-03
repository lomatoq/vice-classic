use super::*;

pub(super) fn measure_one(
    group_id: &str,
    shape_family: &str,
    truth_scene: &GtScene,
    cell: &DegradationCell,
    equivalence_members: usize,
    config: &CoreConfig,
    execution: MeasurementExecution,
) -> MeasurementRow {
    let started = Instant::now();
    let fixture = match render_cell(truth_scene, cell, equivalence_members) {
        Ok(fixture) => fixture,
        Err(error) => {
            return refusal_row(
                group_id,
                shape_family,
                truth_scene,
                cell,
                "render truth fixture",
                error,
                started,
            )
        }
    };
    let png = match encode_png(fixture.width_px, fixture.height_px, &fixture.rgba8) {
        Ok(bytes) => bytes,
        Err(error) => {
            return refusal_row(
                group_id,
                shape_family,
                truth_scene,
                cell,
                "encode input PNG",
                error,
                started,
            )
        }
    };
    let request = VectorizeRequest {
        preset: execution.preset,
        production: config.is_sealed_production(),
        ..VectorizeRequest::default()
    };
    let run = if execution.capture_baseline {
        vice_core::vectorize_for_calibration(&png, &request, config)
    } else {
        vice_core::vectorize_for_calibration_without_baseline(&png, &request, config)
    };
    let report = run.outcome.report();
    let mut row = MeasurementRow {
        group_id: group_id.to_string(),
        scene_id: truth_scene.id().to_string(),
        shape_family: shape_family.to_string(),
        cell_id: fixture.cell_id,
        size_px: fixture.width_px,
        rasterizer: cell.profile.as_str().to_string(),
        identifiability: fixture.identifiability.as_str().to_string(),
        core_runtime_ms: report.runtime.elapsed_ms,
        runtime_stages: report.runtime.stages.clone(),
        court_runtime_ms: 0,
        row_elapsed_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
        decision_status: format!("{:?}", report.status).to_lowercase(),
        decision_reason: report.reason.as_ref().map(stable_failure_reason),
        production_provenance: report.production,
        production_accepted: matches!(&run.outcome, vice_core::VectorizeOutcome::Success(_)),
        candidate_available: false,
        selected_hypothesis_id: report.selected_hypothesis_id.clone(),
        selected_scene_digest_sha256: None,
        selected_delivery_digest_sha256: None,
        selected_artifact_bundle_sha256: None,
        selected_complexity: None,
        internal_baseline: None,
        internal_baseline_refusals: run
            .baseline_refusals
            .iter()
            .map(|refusal| format!("{:?}: {}", refusal.stage, refusal.detail))
            .collect(),
        pf_oracle: None,
        cost_refusal_histogram: cost_refusal_histogram(report),
        numerical_conditioning: numerical_conditioning(report),
        search_truncated: report.search_mass.as_ref().map(|search| search.truncated),
        explored_mass: report
            .search_mass
            .as_ref()
            .map(|search| search.explored_mass),
        topology_classes_upper_bound: report
            .search_mass
            .as_ref()
            .map(|search| search.topology_classes_upper_bound),
        formation_classes_upper_bound: report
            .search_mass
            .as_ref()
            .map(|search| search.formation_classes_upper_bound),
        top_topology_explored_mass: report
            .search_mass
            .as_ref()
            .and_then(|search| search.topology.first())
            .map(|class| class.explored_mass),
        top_formation_explored_mass: report
            .search_mass
            .as_ref()
            .and_then(|search| search.formation.first())
            .map(|class| class.explored_mass),
        selected_delivery_mass: None,
        retained_normalized_mass: None,
        delivery_classes: report
            .search_mass
            .as_ref()
            .map(|search| search.delivery.len().try_into().unwrap_or(u64::MAX)),
        top2_class_margin_bits: report.search_mass.as_ref().and_then(|search| {
            let top1 = search.delivery.first()?.explored_mass;
            let top2 = search.delivery.get(1)?.explored_mass;
            (top1 > 0.0 && top2 > 0.0)
                .then(|| (top1 / top2).log2())
                .filter(|margin| margin.is_finite())
        }),
        posterior_lower_bound: None,
        posterior_bound_status: "absent".into(),
        unexplored_proxy_hypotheses: unexplored_proxy_hypotheses(report),
        candidate_bytes: report.runtime.candidate_bytes,
        serialized_pixel_bits: None,
        serialized_pixel_bits_per_block: None,
        support_isotopy_displacement_px: None,
        evidence_palette_shift_codes: None,
        palette_support_px: None,
        palette_interval_radius_codes: None,
        paint_calibration_class: None,
        empirical_correlation_length_px: None,
        max_abs_lag1: None,
        topology_entropy_upper_bound: None,
        topology_entropy_bound_status: "absent".into(),
        formation_entropy_upper_bound: None,
        formation_entropy_bound_status: "absent".into(),
        perturbation_stability: None,
        phase_envelope_stable: None,
        sample_step_certificate_stable: None,
        render_tolerance_certificate_stable: None,
        render_tolerance_refusal: None,
        solver_certificate_stable: None,
        topology: None,
        boundary: None,
        max_palette_code_delta: None,
        profile_max_channel_delta: None,
        profile_mean_channel_delta: None,
        internal_to_pure_max_channel_delta: None,
        internal_to_pure_mean_channel_delta: None,
        internal_to_seam_max_channel_delta: None,
        internal_to_seam_mean_channel_delta: None,
        verifier_clean: false,
        measurement_refusal: None,
    };
    if let Some(metrics) = &report.confidence_metrics {
        (
            row.topology_entropy_upper_bound,
            row.topology_entropy_bound_status,
        ) = measured_bound(&metrics.topology_entropy_upper_bound);
        (
            row.formation_entropy_upper_bound,
            row.formation_entropy_bound_status,
        ) = measured_bound(&metrics.formation_entropy_upper_bound);
        row.perturbation_stability = Some(metrics.perturbation_stability.score);
        row.support_isotopy_displacement_px = Some(metrics.support_isotopy_displacement_px);
        row.evidence_palette_shift_codes = Some(metrics.evidence_palette_shift_codes);
        row.palette_support_px = Some(metrics.palette_support_px);
        row.palette_interval_radius_codes = Some(metrics.palette_interval_radius_codes);
        row.paint_calibration_class = Some(metrics.paint_calibration_class.clone());
        row.phase_envelope_stable = Some(metrics.perturbation_stability.phase_envelope_stable);
        row.sample_step_certificate_stable = Some(
            metrics
                .perturbation_stability
                .sample_step_certificate_stable,
        );
        row.render_tolerance_certificate_stable = Some(
            metrics
                .perturbation_stability
                .render_tolerance_certificate_stable,
        );
        row.render_tolerance_refusal = metrics
            .perturbation_stability
            .render_tolerance_refusal
            .clone();
        row.solver_certificate_stable =
            Some(metrics.perturbation_stability.solver_certificate_stable);
    }
    if let Some(best) = report
        .search_mass
        .as_ref()
        .and_then(vice_opt::SearchMassCertificate::best_delivery)
    {
        row.selected_delivery_mass = Some(best.explored_mass);
        row.retained_normalized_mass = Some(best.retained_normalized_mass);
        match best.posterior_lower_bound {
            BoundValue::Certified(value) => {
                row.posterior_lower_bound = Some(value);
                row.posterior_bound_status = "certified".into();
            }
            BoundValue::EmpiricallyCalibrated(value) => {
                row.posterior_lower_bound = Some(value);
                row.posterior_bound_status = "empirically_calibrated".into();
            }
            BoundValue::Unknown => row.posterior_bound_status = "unknown".into(),
        }
    }
    let Some(witness) = run.selected else {
        let reason = report.reason.as_ref().map_or_else(
            || "no selected calibration witness".into(),
            |reason| {
                serde_json::to_string(reason).unwrap_or_else(|_| "unserializable reason".into())
            },
        );
        let candidate_refusals = serde_json::to_string(&report.candidate_refusals)
            .unwrap_or_else(|_| "unserializable candidate refusals".into());
        row.measurement_refusal =
            Some(format!("{reason}; candidate_refusals={candidate_refusals}"));
        return row;
    };
    row.selected_scene_digest_sha256 = Some(witness.candidate.scene_digest_sha256.clone());
    row.selected_delivery_digest_sha256 = Some(witness.candidate.delivery_digest.clone());
    row.selected_artifact_bundle_sha256 = Some(artifact_bundle_digest(&witness));
    row.selected_complexity = scene_complexity(&witness).ok();
    let court_started = Instant::now();
    match judge_witness(truth_scene, cell, &witness) {
        Ok((topology, boundary, paint_delta)) => {
            row.candidate_available = true;
            row.topology = Some(topology);
            row.boundary = Some(boundary);
            row.max_palette_code_delta = Some(paint_delta);
            let candidate = &witness.candidate;
            let seal = &candidate.delivery_seal;
            row.profile_max_channel_delta = Some(seal.profile_comparison.max_channel_delta);
            row.profile_mean_channel_delta = Some(seal.profile_comparison.mean_channel_delta);
            row.internal_to_pure_max_channel_delta =
                Some(seal.internal_to_pure_comparison.max_channel_delta);
            row.internal_to_pure_mean_channel_delta =
                Some(seal.internal_to_pure_comparison.mean_channel_delta);
            row.internal_to_seam_max_channel_delta =
                Some(seal.internal_to_seam_comparison.max_channel_delta);
            row.internal_to_seam_mean_channel_delta =
                Some(seal.internal_to_seam_comparison.mean_channel_delta);
            let diagnostics = &candidate.score.diagnostics;
            row.serialized_pixel_bits = Some(candidate.score.pixel_bits);
            row.serialized_pixel_bits_per_block = (diagnostics.blocks > 0)
                .then_some(candidate.score.pixel_bits / diagnostics.blocks as f64);
            row.empirical_correlation_length_px = Some(diagnostics.empirical_correlation_length_px);
            row.max_abs_lag1 = Some(diagnostics.lag1_x.abs().max(diagnostics.lag1_y.abs()));
            row.verifier_clean = candidate.pre_quantization.worst_g1_spread_rad
                <= config.verification.max_g1_spread_rad;
        }
        Err(error) => row.measurement_refusal = Some(error),
    }
    if let Some(baseline) = run.baseline {
        row.internal_baseline =
            measure_internal_baseline(truth_scene, cell, &baseline, config).ok();
    }
    row.pf_oracle = Some(measure_pf_oracle(
        truth_scene,
        cell,
        &fixture.rgba8,
        &witness,
        config,
    ));
    row.court_runtime_ms = court_started
        .elapsed()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX);
    row.row_elapsed_ms = started.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
    row
}

pub(super) fn artifact_bundle_digest(witness: &vice_core::CalibrationWitness) -> String {
    let mut hash = Sha256::new();
    for (name, bytes) in [
        ("scene", witness.scene_json.as_slice()),
        ("export_plan", witness.export_plan_json.as_slice()),
        ("pure_svg", witness.pure_partition_svg.as_slice()),
        ("seam_svg", witness.seam_safe_svg.as_slice()),
        ("render", witness.rendered_png.as_slice()),
        ("seal", witness.seal_json.as_slice()),
    ] {
        hash.update((name.len() as u64).to_be_bytes());
        hash.update(name.as_bytes());
        hash.update((bytes.len() as u64).to_be_bytes());
        hash.update(bytes);
    }
    hex::encode(hash.finalize())
}

pub(super) fn scene_complexity(
    witness: &vice_core::CalibrationWitness,
) -> Result<SceneComplexity, String> {
    let scene = vice_ir::parse_scene(&witness.scene_json)
        .map_err(|error| format!("parse scene for complexity: {error}"))?;
    Ok(SceneComplexity {
        vertices: scene.graph.vertices.len().try_into().unwrap_or(u64::MAX),
        boundaries: scene.graph.boundaries.len().try_into().unwrap_or(u64::MAX),
        curve_segments: scene
            .graph
            .boundaries
            .iter()
            .map(|boundary| u64::try_from(boundary.curve.segments.len()).unwrap_or(u64::MAX))
            .fold(0u64, u64::saturating_add),
        canonical_delivery_bytes: [
            witness.scene_json.len(),
            witness.export_plan_json.len(),
            witness.pure_partition_svg.len(),
            witness.seam_safe_svg.len(),
            witness.rendered_png.len(),
            witness.seal_json.len(),
        ]
        .into_iter()
        .map(|bytes| u64::try_from(bytes).unwrap_or(u64::MAX))
        .fold(0u64, u64::saturating_add),
    })
}

pub(super) fn measure_internal_baseline(
    truth_scene: &GtScene,
    cell: &DegradationCell,
    witness: &vice_core::CalibrationWitness,
    config: &CoreConfig,
) -> Result<InternalBaselineMeasurement, String> {
    let (topology, boundary, max_palette_code_delta) = judge_witness(truth_scene, cell, witness)?;
    Ok(InternalBaselineMeasurement {
        hypothesis_id: witness.candidate.hypothesis_id.clone(),
        scene_digest_sha256: witness.candidate.scene_digest_sha256.clone(),
        delivery_digest_sha256: witness.candidate.delivery_digest.clone(),
        artifact_bundle_sha256: artifact_bundle_digest(witness),
        topology,
        boundary,
        max_palette_code_delta,
        profile_max_channel_delta: witness
            .candidate
            .delivery_seal
            .profile_comparison
            .max_channel_delta,
        profile_mean_channel_delta: witness
            .candidate
            .delivery_seal
            .profile_comparison
            .mean_channel_delta,
        internal_to_pure_max_channel_delta: witness
            .candidate
            .delivery_seal
            .internal_to_pure_comparison
            .max_channel_delta,
        internal_to_pure_mean_channel_delta: witness
            .candidate
            .delivery_seal
            .internal_to_pure_comparison
            .mean_channel_delta,
        internal_to_seam_max_channel_delta: witness
            .candidate
            .delivery_seal
            .internal_to_seam_comparison
            .max_channel_delta,
        internal_to_seam_mean_channel_delta: witness
            .candidate
            .delivery_seal
            .internal_to_seam_comparison
            .mean_channel_delta,
        complexity: scene_complexity(witness)?,
        verifier_clean: witness.candidate.pre_quantization.worst_g1_spread_rad
            <= config.verification.max_g1_spread_rad,
    })
}

pub(super) fn measure_pf_oracle(
    truth_scene: &GtScene,
    cell: &DegradationCell,
    observed_rgba8: &[u8],
    selected: &vice_core::CalibrationWitness,
    config: &CoreConfig,
) -> PfOracleMeasurement {
    const SCHEMA: &str = "vice-classic/m7-pf-interventions/v1";
    const BACKEND: &str = "vice-svg/independent-parser-renderer/v1";
    let common_config_sha256 = hex::encode(Sha256::digest(
        format!(
            "{SCHEMA}|{BACKEND}|{}|{}|{}",
            config.export_decimal_places,
            config.apron_width_px,
            cell.id()
        )
        .as_bytes(),
    ));
    let mut arms = Vec::new();
    let mut refusals = Vec::new();
    let selected_scene = match vice_ir::parse_scene(&selected.scene_json) {
        Ok(scene) => scene,
        Err(error) => {
            refusals.push(format!("parse selected scene: {error}"));
            return PfOracleMeasurement {
                intervention_schema: SCHEMA.into(),
                common_backend: BACKEND.into(),
                common_config_sha256,
                arms,
                refusals,
                complete: false,
            };
        }
    };
    let gt_formation = vice_ir::GlobalFormationHypothesis {
        blend_space: cell.blend,
        pixel_filter: match cell.psf {
            crate::gt::raster::Psf::Box => vice_ir::PixelFilter::Box,
            crate::gt::raster::Psf::Triangle => vice_ir::PixelFilter::Triangle,
            crate::gt::raster::Psf::Gaussian { sigma_px } => {
                vice_ir::PixelFilter::Gaussian { sigma_px }
            }
        },
        quantization: vice_ir::QuantizationModel::Uint8,
        exterior: truth_scene.scene().scene().formation.exterior,
    };
    let mut variants = vec![("PF00", "automatic", "estimated", selected_scene.clone())];
    let mut pf01 = selected_scene.clone();
    pf01.formation = gt_formation;
    variants.push(("PF01", "automatic", "ground_truth", pf01));
    match scaled_truth_scene(truth_scene, cell) {
        Ok(mut ground_truth) => {
            ground_truth.formation = selected_scene.formation;
            variants.push(("PF10", "ground_truth", "estimated", ground_truth.clone()));
            ground_truth.formation = gt_formation;
            variants.push(("PF11", "ground_truth", "ground_truth", ground_truth));
        }
        Err(error) => refusals.push(format!("construct ground-truth partition: {error}")),
    }
    for (arm, partition_source, formation_source, scene) in variants {
        match measure_pf_arm(
            arm,
            partition_source,
            formation_source,
            &scene,
            observed_rgba8,
            config,
        ) {
            Ok(measurement) => arms.push(measurement),
            Err(error) => refusals.push(format!("{arm}: {error}")),
        }
    }
    arms.sort_by(|left, right| left.arm.cmp(&right.arm));
    let complete = refusals.is_empty()
        && arms.len() == 4
        && ["PF00", "PF01", "PF10", "PF11"]
            .iter()
            .all(|want| arms.iter().any(|arm| arm.arm == *want));
    PfOracleMeasurement {
        intervention_schema: SCHEMA.into(),
        common_backend: BACKEND.into(),
        common_config_sha256,
        arms,
        refusals,
        complete,
    }
}

pub(super) fn scaled_truth_scene(
    truth_scene: &GtScene,
    cell: &DegradationCell,
) -> Result<vice_ir::VectorScene, String> {
    if cell.resize != crate::gt::degradation::ResizeChain::None {
        return Err("PF scene transform requires a no-resize cell".into());
    }
    let mut scene = truth_scene.scene().scene().clone();
    let scale = f64::from(cell.size_px) / f64::from(AUTHORING_CANVAS_PX);
    let transform = |point: Pt| {
        Pt::new(
            point.x * scale + cell.subpixel_dx,
            point.y * scale + cell.subpixel_dy,
        )
    };
    for vertex in &mut scene.graph.vertices {
        vertex.pos = transform(vertex.pos);
    }
    for boundary in &mut scene.graph.boundaries {
        for node in &mut boundary.curve.interior_nodes {
            node.pos = transform(node.pos);
        }
        for segment in &mut boundary.curve.segments {
            match segment {
                vice_ir::Segment::Line => {}
                vice_ir::Segment::CircularArc { radius_px, .. } => *radius_px *= scale,
                vice_ir::Segment::EllipticArc { rx_px, ry_px, .. } => {
                    *rx_px *= scale;
                    *ry_px *= scale;
                }
                vice_ir::Segment::Quad { ctrl } => *ctrl = transform(*ctrl),
                vice_ir::Segment::Cubic { ctrl1, ctrl2 } => {
                    *ctrl1 = transform(*ctrl1);
                    *ctrl2 = transform(*ctrl2);
                }
            }
        }
    }
    scene.canvas = vice_ir::Canvas {
        width_px: cell.size_px,
        height_px: cell.size_px,
    };
    vice_ir::validate_scene(&scene).map_err(|error| error.to_string())?;
    Ok(scene)
}

pub(super) fn measure_pf_arm(
    arm: &str,
    partition_source: &str,
    formation_source: &str,
    scene: &vice_ir::VectorScene,
    observed_rgba8: &[u8],
    config: &CoreConfig,
) -> Result<PfArmMeasurement, String> {
    let plan =
        vice_svg::build_export_plan(scene, config.export_decimal_places, config.apron_width_px)
            .map_err(|error| error.to_string())?;
    let svg = vice_svg::materialize_svg(&plan, vice_svg::SvgProfile::PurePartition)
        .map_err(|error| error.to_string())?;
    let rendered =
        vice_svg::parse_and_render_independently(&svg).map_err(|error| error.to_string())?;
    let actual = rendered.premultiplied_rgba8();
    if actual.len() != observed_rgba8.len() {
        return Err(format!(
            "render length {} does not match observation {}",
            actual.len(),
            observed_rgba8.len()
        ));
    }
    let mut max_delta = 0.0f64;
    let mut sum = 0.0f64;
    let mut identical = 0u64;
    for (expected, actual) in observed_rgba8.chunks_exact(4).zip(actual.chunks_exact(4)) {
        let deltas = crate::gt::colour::premultiplied_deltas(expected, actual);
        let pixel_max = deltas.iter().copied().fold(0.0, f64::max);
        max_delta = max_delta.max(pixel_max);
        sum += deltas.iter().sum::<f64>();
        identical += u64::from(pixel_max == 0.0);
    }
    let pixels = u64::try_from(observed_rgba8.len() / 4).unwrap_or(u64::MAX);
    Ok(PfArmMeasurement {
        arm: arm.into(),
        partition_source: partition_source.into(),
        formation_source: formation_source.into(),
        scene_digest_sha256: vice_ir::scene_digest_sha256(scene)
            .map_err(|error| error.to_string())?,
        serialized_svg_sha256: hex::encode(Sha256::digest(&svg)),
        max_premultiplied_code_delta: max_delta,
        mean_premultiplied_code_delta: if pixels == 0 {
            0.0
        } else {
            sum / (pixels * 4) as f64
        },
        identical_pixels_fraction: if pixels == 0 {
            1.0
        } else {
            identical as f64 / pixels as f64
        },
    })
}

pub(super) fn measured_bound(bound: &BoundValue<f64>) -> (Option<f64>, String) {
    match bound {
        BoundValue::Certified(value) => (Some(*value), "certified".into()),
        BoundValue::EmpiricallyCalibrated(value) => (Some(*value), "empirically_calibrated".into()),
        BoundValue::Unknown => (None, "unknown".into()),
    }
}

pub(super) fn stable_failure_reason(reason: &vice_core::FailureReason) -> String {
    match reason {
        vice_core::FailureReason::Evidence { .. } => "evidence",
        vice_core::FailureReason::FormationOutsideUniverse { .. } => "formation_outside_universe",
        vice_core::FailureReason::BoundaryOutsideSelectiveCore { .. } => {
            "boundary_outside_selective_core"
        }
        vice_core::FailureReason::Topology { .. } => "topology",
        vice_core::FailureReason::Fitting { .. } => "fitting",
        vice_core::FailureReason::SearchTruncated { .. } => "search_truncated",
        vice_core::FailureReason::NoVerifiedCandidate { .. } => "no_verified_candidate",
        vice_core::FailureReason::Confidence { .. } => "confidence",
        vice_core::FailureReason::Decode { .. } => "decode",
        vice_core::FailureReason::Internal { .. } => "internal",
    }
    .into()
}

pub(super) fn cost_refusal_histogram(report: &vice_core::VectorizeReport) -> Vec<CostRefusalCount> {
    let mut counts = BTreeMap::<(String, String), u64>::new();
    for fit in &report.fit_diagnostics {
        for &(family, reason, count) in &fit.cost_refusals {
            let entry = counts
                .entry((family.to_string(), reason.to_string()))
                .or_default();
            *entry = entry.saturating_add(count.try_into().unwrap_or(u64::MAX));
        }
    }
    counts
        .into_iter()
        .map(|((family, reason), count)| CostRefusalCount {
            family,
            reason,
            count,
        })
        .collect()
}

pub(super) fn numerical_conditioning(
    report: &vice_core::VectorizeReport,
) -> NumericalConditioningDiagnostics {
    let evidence_conditioning = report
        .evidence
        .as_ref()
        .into_iter()
        .flat_map(|analysis| {
            analysis
                .evidences
                .iter()
                .map(|evidence| evidence.conditioning)
        })
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    let worst_fit_ratio = report
        .fit_diagnostics
        .iter()
        .filter_map(|fit| fit.worst_normal_to_euclidean_ratio)
        .max_by(|left, right| left.ratio.total_cmp(&right.ratio));
    NumericalConditioningDiagnostics {
        evidence_pairs: evidence_conditioning.len().try_into().unwrap_or(u64::MAX),
        evidence_pairs_refused: report
            .evidence
            .as_ref()
            .map_or(0, |analysis| analysis.refused.len())
            .try_into()
            .unwrap_or(u64::MAX),
        evidence_conditioning_min: evidence_conditioning.iter().copied().min_by(f64::total_cmp),
        evidence_conditioning_max: evidence_conditioning.iter().copied().max_by(f64::total_cmp),
        fit_runs: report.fit_diagnostics.len().try_into().unwrap_or(u64::MAX),
        fit_material_cost_samples: report.fit_diagnostics.iter().fold(0u64, |total, fit| {
            total.saturating_add(fit.material_cost_samples.try_into().unwrap_or(u64::MAX))
        }),
        fit_worst_normal_to_euclidean_ratio: worst_fit_ratio.map(|reading| reading.ratio),
        fit_worst_ratio_at_deviation_px: worst_fit_ratio.map(|reading| reading.at_deviation_px),
    }
}

pub(super) fn unexplored_proxy_hypotheses(report: &vice_core::VectorizeReport) -> Option<u64> {
    let search = report.search_mass.as_ref()?;
    if !search.truncated {
        return Some(0);
    }
    let topology = report.topology.as_ref().map_or(0u64, |topology| {
        topology
            .prefit_budget_pruned_arms
            .len()
            .try_into()
            .unwrap_or(u64::MAX)
    });
    let fit = report.fit_diagnostics.iter().fold(0u64, |total, fit| {
        let skipped_levels = u64::try_from(fit.proposal_levels_skipped_after_certification)
            .unwrap_or(u64::MAX)
            .saturating_mul(vice_fit::K_DISCRETE_PATHS.try_into().unwrap_or(u64::MAX));
        total
            .saturating_add(fit.resource_pruned_proposals.try_into().unwrap_or(u64::MAX))
            .saturating_add(skipped_levels)
    });
    let materialization = report.beam.as_ref().map_or(0u64, |beam| {
        beam.unmaterialized_by_candidate_budget
            .saturating_add(beam.unmaterialized_by_materialization_budget)
            .saturating_add(beam.unmaterialized_by_time_budget)
    });
    Some(topology.saturating_add(fit).saturating_add(materialization))
}

pub(super) fn refusal_row(
    group_id: &str,
    shape_family: &str,
    scene: &GtScene,
    cell: &DegradationCell,
    stage: &str,
    detail: String,
    started: Instant,
) -> MeasurementRow {
    MeasurementRow {
        group_id: group_id.to_string(),
        scene_id: scene.id().to_string(),
        shape_family: shape_family.to_string(),
        cell_id: cell.id(),
        size_px: cell.size_px,
        rasterizer: cell.profile.as_str().to_string(),
        identifiability: "measurement_refused".into(),
        core_runtime_ms: 0,
        runtime_stages: vice_core::RuntimeStageSummary::default(),
        court_runtime_ms: 0,
        row_elapsed_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
        decision_status: "measurement_refused".into(),
        decision_reason: Some("measurement_failure".into()),
        production_provenance: false,
        production_accepted: false,
        candidate_available: false,
        selected_hypothesis_id: None,
        selected_scene_digest_sha256: None,
        selected_delivery_digest_sha256: None,
        selected_artifact_bundle_sha256: None,
        selected_complexity: None,
        internal_baseline: None,
        internal_baseline_refusals: Vec::new(),
        pf_oracle: None,
        cost_refusal_histogram: Vec::new(),
        numerical_conditioning: NumericalConditioningDiagnostics::default(),
        search_truncated: None,
        explored_mass: None,
        topology_classes_upper_bound: None,
        formation_classes_upper_bound: None,
        top_topology_explored_mass: None,
        top_formation_explored_mass: None,
        selected_delivery_mass: None,
        retained_normalized_mass: None,
        delivery_classes: None,
        top2_class_margin_bits: None,
        posterior_lower_bound: None,
        posterior_bound_status: "absent".into(),
        unexplored_proxy_hypotheses: None,
        candidate_bytes: 0,
        serialized_pixel_bits: None,
        serialized_pixel_bits_per_block: None,
        support_isotopy_displacement_px: None,
        evidence_palette_shift_codes: None,
        palette_support_px: None,
        palette_interval_radius_codes: None,
        paint_calibration_class: None,
        empirical_correlation_length_px: None,
        max_abs_lag1: None,
        topology_entropy_upper_bound: None,
        topology_entropy_bound_status: "absent".into(),
        formation_entropy_upper_bound: None,
        formation_entropy_bound_status: "absent".into(),
        perturbation_stability: None,
        phase_envelope_stable: None,
        sample_step_certificate_stable: None,
        render_tolerance_certificate_stable: None,
        render_tolerance_refusal: None,
        solver_certificate_stable: None,
        topology: None,
        boundary: None,
        max_palette_code_delta: None,
        profile_max_channel_delta: None,
        profile_mean_channel_delta: None,
        internal_to_pure_max_channel_delta: None,
        internal_to_pure_mean_channel_delta: None,
        internal_to_seam_max_channel_delta: None,
        internal_to_seam_mean_channel_delta: None,
        verifier_clean: false,
        measurement_refusal: Some(format!("{stage}: {detail}")),
    }
}
