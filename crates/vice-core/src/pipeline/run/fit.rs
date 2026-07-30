use super::*;

pub(super) fn fit_chain(
    chain: &vice_evidence::BoundaryChain,
    canvas_dim_px: f64,
    config: &CoreConfig,
) -> Result<vice_fit::ModelRun, String> {
    let fit = if canvas_dim_px >= 128.0 {
        vice_fit::k_best_boundary_models_bounded(
            chain,
            &vice_fit::FIT_BUDGET_V1,
            canvas_dim_px,
            config.k_discrete_paths,
            vice_fit::DISCRETE_PROPOSAL_SAMPLE_CAP_V1,
        )
        .or_else(|primary| {
            if config.k_discrete_paths == 1 {
                // Preserve successful one-path Fast latency, then try one
                // bounded four-path recovery after a certified primary miss.
                vice_fit::k_best_boundary_models_bounded(
                    chain,
                    &vice_fit::FIT_BUDGET_V1,
                    canvas_dim_px,
                    4,
                    vice_fit::DISCRETE_PROPOSAL_SAMPLE_CAP_V1,
                )
            } else {
                Err(primary)
            }
        })
        .map_err(|error| format!("{error:?}"))
    } else {
        vice_fit::k_best_boundary_models(
            chain,
            &vice_fit::FIT_BUDGET_V1,
            canvas_dim_px,
            config.k_discrete_paths,
        )
        .map_err(|error| format!("{error:?}"))
    }?;
    (!fit.models.is_empty())
        .then_some(fit)
        .ok_or_else(|| "typed fitter produced no admissible model".to_string())
}
