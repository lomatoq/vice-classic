use super::*;

pub(super) struct BaselineEvidenceRequest<'a> {
    pub enabled: bool,
    pub candidates: &'a [crate::candidate::MaterializedCandidate],
    pub fitted_arms: &'a [FittedTopologyArm],
    pub formations: &'a [vice_ir::GlobalFormationHypothesis],
    pub canvas: Canvas,
    pub evidence: &'a vice_evidence::Flat2Evidence,
    pub image: &'a vice_image::CanonicalImage,
    pub request: &'a VectorizeRequest,
    pub config: &'a CoreConfig,
}

/// Preserve the preregistered free-chain opponent independently of the
/// production posterior. Production ranking may legitimately put every
/// free-chain task outside its bounded prefix; the court still needs either a
/// verified opponent on the same raster/delivery path or typed proof that the
/// opponent itself refused.
pub(super) fn calibration_baseline_evidence(
    request: BaselineEvidenceRequest<'_>,
) -> (
    Option<crate::candidate::MaterializedCandidate>,
    Vec<CandidateRefusal>,
) {
    let BaselineEvidenceRequest {
        enabled,
        candidates,
        fitted_arms,
        formations,
        canvas,
        evidence,
        image,
        request,
        config,
    } = request;
    if !enabled {
        return (None, Vec::new());
    }
    let baseline = candidates
        .iter()
        .filter(|candidate| candidate.score.hypothesis_id.starts_with("baseline-free/"))
        .min_by(|left, right| compare_candidates(left, right))
        .cloned();
    if baseline.is_some() {
        return (baseline, Vec::new());
    }

    let Some(variant) = fitted_arms[0]
        .variants
        .iter()
        .find(|variant| variant.class == "baseline-free")
    else {
        return (
            None,
            vec![CandidateRefusal {
                hypothesis_id: "baseline-free/unavailable".into(),
                stage: CandidateFailureStage::SceneConstruction,
                detail: "canonical topology published no frozen free-chain variant".into(),
            }],
        );
    };

    // Isolated cache/telemetry: these candidates never enter `candidates`,
    // `ReportParts`, the posterior, or delivery selection.
    let bundle = &fitted_arms[0];
    let arm = &bundle.arm;
    let mut cache = CandidateCache::default();
    let mut runtime = crate::types::CandidateRuntimeSummary::default();
    let mut baselines = Vec::new();
    let mut refusals = Vec::new();
    for formation in formations {
        let formation_class = vice_evidence::formation_id(formation);
        let hypothesis_id = format!("{}/t0/{formation_class}", variant.class);
        match materialize_candidate(
            CandidateRequest {
                canvas,
                evidence,
                chains: &arm.chains,
                models: &variant.models,
                arm,
                formation: *formation,
                model_transactions: &variant.model_transactions,
                transaction_base_arm: &fitted_arms[0].arm,
                transaction_base_chains: &fitted_arms[0].arm.chains,
                transaction_base_models: &fitted_arms[0].baseline_models,
                transaction_base_formation: formations[0],
                hypothesis_id,
                formation_class,
                image,
                intent: request.intent,
                config,
            },
            &mut cache,
            &mut runtime,
        ) {
            Ok(candidate) => baselines.push(candidate),
            Err(refusal) => refusals.push(refusal),
        }
    }
    baselines.sort_by(compare_candidates);
    (baselines.into_iter().next(), refusals)
}

fn compare_candidates(
    left: &crate::candidate::MaterializedCandidate,
    right: &crate::candidate::MaterializedCandidate,
) -> std::cmp::Ordering {
    left.score
        .total_bits
        .total_cmp(&right.score.total_bits)
        .then_with(|| {
            left.summary
                .scene_digest_sha256
                .cmp(&right.summary.scene_digest_sha256)
        })
        .then_with(|| left.score.hypothesis_id.cmp(&right.score.hypothesis_id))
}
