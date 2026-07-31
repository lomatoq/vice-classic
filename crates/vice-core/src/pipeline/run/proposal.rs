use super::*;

/// Reorder only the bounded scheduled prefix with the real full-resolution
/// proposal likelihood. Final membership and acceptance remain the serialized
/// court's responsibility.
#[allow(clippy::too_many_arguments)]
pub(super) fn rank_materializations(
    materialization_order: &mut [(usize, usize, usize)],
    diversity_seed_materializations: usize,
    fitted_arms: &[FittedTopologyArm],
    formations: &[vice_ir::GlobalFormationHypothesis],
    canvas: Canvas,
    evidence: &vice_evidence::Flat2Evidence,
    image: &vice_image::CanonicalImage,
    request: &VectorizeRequest,
    config: &CoreConfig,
) -> usize {
    let original_order = materialization_order.to_vec();
    let diversity_seeds = original_order
        .iter()
        .take(diversity_seed_materializations)
        .copied()
        .collect::<Vec<_>>();
    let rank_limit = if fitted_arms.iter().any(|bundle| bundle.arm.chains.len() > 1) {
        config.beam.width.max(16)
    } else {
        config.beam.width
    };
    let rank_count = materialization_order.len().min(rank_limit);
    let mut ranked = Vec::with_capacity(rank_count);
    let mut workspace = crate::candidate::ProposalWorkspace::default();
    for &(topology_index, variant_index, formation_index) in &materialization_order[..rank_count] {
        let bundle = &fitted_arms[topology_index];
        let variant = &bundle.variants[variant_index];
        let arm = &bundle.arm;
        let formation = formations[formation_index];
        let formation_class = vice_evidence::formation_id(&formation);
        let hypothesis_id = format!("{}/t{topology_index}/{formation_class}", variant.class);
        let score = score_candidate_proposal(
            &CandidateRequest {
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
            },
            &mut workspace,
        )
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
    let mut priority = Vec::with_capacity(materialization_order.len());
    let mut prioritized = BTreeSet::new();
    if let Some((task, _)) = ranked.first() {
        priority.push(*task);
        prioritized.insert(*task);
    }
    for prefix in ["scene-repetition-", "scene-mirror-"] {
        if let Some((task, _)) = ranked
            .iter()
            .find(|(task, _)| class_of(task).starts_with(prefix))
        {
            if prioritized.insert(*task) {
                priority.push(*task);
            }
        }
    }
    for task in diversity_seeds {
        if prioritized.insert(task) {
            priority.push(task);
        }
    }
    let mandatory = priority.len();
    for (task, _) in ranked {
        if prioritized.insert(task) {
            priority.push(task);
        }
    }
    for task in original_order {
        if prioritized.insert(task) {
            priority.push(task);
        }
    }
    materialization_order.copy_from_slice(&priority);
    mandatory
}
