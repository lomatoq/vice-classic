use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn deliver(
    production: bool,
    request: &VectorizeRequest,
    config: &CoreConfig,
    source_sha256: String,
    parts: ReportParts,
    started: Instant,
    selected: &crate::candidate::MaterializedCandidate,
    candidates: &[crate::candidate::MaterializedCandidate],
    candidate_refusals: &[CandidateRefusal],
    best_delivery: &vice_opt::DeliveryPosterior,
    confidence_metrics: &ConfidenceMetrics,
) -> VectorizeOutcome {
    if !production {
        return refuse(
            DecisionStatus::Ambiguous,
            FailureReason::Confidence {
                detail: "oracle, debug, or research override makes this run research_unsealed"
                    .into(),
            },
            request,
            config,
            source_sha256,
            false,
            parts,
            started,
        );
    }
    let Some(calibration) = config.confidence.as_ref() else {
        return refuse(
            DecisionStatus::Ambiguous,
            FailureReason::Confidence {
                detail: "no frozen confidence calibration is installed".into(),
            },
            request,
            config,
            source_sha256,
            production,
            parts,
            started,
        );
    };
    if let Err(detail) = calibration.permits(&config.identity(), best_delivery, confidence_metrics)
    {
        return refuse(
            DecisionStatus::Ambiguous,
            FailureReason::Confidence {
                detail: detail.into(),
            },
            request,
            config,
            source_sha256,
            production,
            parts,
            started,
        );
    }

    let trace_json = build_trace(request, selected, candidates, candidate_refusals);
    let report = make_report(
        DecisionStatus::Success,
        None,
        request,
        config,
        source_sha256,
        true,
        parts,
        started,
    );
    let report_json = serde_json::to_vec(&report).expect("report serializes");
    VectorizeOutcome::Success(VectorizeSuccess {
        report,
        artifacts: SuccessArtifacts {
            result_svg: selected.seam_svg.clone(),
            pure_partition_svg: selected.pure_svg.clone(),
            scene_json: selected.scene_json.clone(),
            export_plan_json: selected.plan_json.clone(),
            report_json,
            render_png: selected.render_png.clone(),
            seal_json: selected.seal_json.clone(),
            trace_json,
        },
    })
}
