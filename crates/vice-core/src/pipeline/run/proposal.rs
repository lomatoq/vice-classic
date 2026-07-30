use super::*;

/// Reorder only the bounded scheduled prefix with the real full-resolution
/// proposal likelihood. Final membership and acceptance remain the serialized
/// court's responsibility.
#[allow(clippy::too_many_arguments)]
pub(super) fn rank_materializations(
    materialization_order: &mut [(usize, usize, usize)],
    fitted_arms: &[FittedTopologyArm],
    formations: &[vice_ir::GlobalFormationHypothesis],
    canvas: Canvas,
    evidence: &vice_evidence::Flat2Evidence,
    image: &vice_image::CanonicalImage,
    request: &VectorizeRequest,
    config: &CoreConfig,
) -> usize {
    let rank_limit = if fitted_arms.iter().any(|bundle| bundle.arm.chains.len() > 1) {
        config.beam.width.max(16)
    } else {
        config.beam.width
    };
    let rank_count = materialization_order.len().min(rank_limit);
    let mut ranked = Vec::with_capacity(rank_count);
    for &(topology_index, variant_index, formation_index) in &materialization_order[..rank_count] {
        let bundle = &fitted_arms[topology_index];
        let variant = &bundle.variants[variant_index];
        let arm = &bundle.arm;
        let formation = formations[formation_index];
        let formation_class = vice_evidence::formation_id(&formation);
        let hypothesis_id = format!("{}/t{topology_index}/{formation_class}", variant.class);
        let score = score_candidate_proposal(&CandidateRequest {
            canvas,
            evidence,
            chains: &arm.chains,
            models: &variant.models,
            arm,
            formation,
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
        })
        .ok();
        ranked.push(((topology_index, variant_index, formation_index), score));
    }
    ranked.sort_by(|(left_task, left), (right_task, right)| {
        left.as_ref()
            .map_or(f64::INFINITY, |score| score.total_bits)
            .total_cmp(
                &right
                    .as_ref()
                    .map_or(f64::INFINITY, |score| score.total_bits),
            )
            .then_with(|| {
                left.as_ref()
                    .map(|score| score.scene_digest_sha256.as_str())
                    .unwrap_or("")
                    .cmp(
                        right
                            .as_ref()
                            .map(|score| score.scene_digest_sha256.as_str())
                            .unwrap_or(""),
                    )
            })
            .then_with(|| left_task.cmp(right_task))
    });
    let class_of =
        |task: &(usize, usize, usize)| fitted_arms[task.0].variants[task.1].class.as_str();
    let mut mandatory = usize::from(!ranked.is_empty());
    for prefix in ["scene-repetition-", "scene-mirror-"] {
        if ranked
            .iter()
            .take(mandatory)
            .any(|(task, _)| class_of(task).starts_with(prefix))
        {
            continue;
        }
        if let Some(index) = ranked
            .iter()
            .position(|(task, _)| class_of(task).starts_with(prefix))
        {
            let relation = ranked.remove(index);
            ranked.insert(mandatory, relation);
            mandatory += 1;
        }
    }
    for (slot, (task, _)) in materialization_order
        .iter_mut()
        .take(rank_count)
        .zip(ranked)
    {
        *slot = task;
    }
    mandatory
}
