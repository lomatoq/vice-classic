use super::*;

pub(super) fn fit_chain(
    chain: &vice_evidence::BoundaryChain,
    canvas_dim_px: f64,
    config: &CoreConfig,
    runtime: &mut crate::types::FittingRuntimeSummary,
) -> Result<vice_fit::ModelRun, String> {
    runtime.chain_attempts = runtime.chain_attempts.saturating_add(1);
    let fit = if canvas_dim_px >= 128.0 {
        let started = Instant::now();
        let primary = vice_fit::k_best_boundary_models_bounded(
            chain,
            &vice_fit::FIT_BUDGET_V1,
            canvas_dim_px,
            config.k_discrete_paths,
            vice_fit::DISCRETE_PROPOSAL_SAMPLE_CAP_V1,
        );
        runtime.primary_attempt_ms = runtime
            .primary_attempt_ms
            .saturating_add(started.elapsed().as_millis().try_into().unwrap_or(u64::MAX));
        match primary {
            Ok(fit) => Ok(fit),
            Err(primary) => {
                if config.k_discrete_paths < 2 * vice_fit::K_DISCRETE_PATHS {
                    // A certified miss opens exactly one wider deterministic
                    // level. This is recovery inside the same finite grammar, not
                    // a fallback to a different fitter or representation.
                    runtime.recovery_attempts = runtime.recovery_attempts.saturating_add(1);
                    let started = Instant::now();
                    let recovery = vice_fit::k_best_boundary_models_bounded(
                        chain,
                        &vice_fit::FIT_BUDGET_V1,
                        canvas_dim_px,
                        2 * vice_fit::K_DISCRETE_PATHS,
                        vice_fit::DISCRETE_PROPOSAL_SAMPLE_CAP_V1,
                    );
                    runtime.recovery_attempt_ms = runtime.recovery_attempt_ms.saturating_add(
                        started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
                    );
                    recovery
                } else {
                    Err(primary)
                }
            }
        }
        .map_err(|error| format!("{error:?}"))
    } else {
        let started = Instant::now();
        let fit = vice_fit::k_best_boundary_models(
            chain,
            &vice_fit::FIT_BUDGET_V1,
            canvas_dim_px,
            config.k_discrete_paths,
        );
        runtime.primary_attempt_ms = runtime
            .primary_attempt_ms
            .saturating_add(started.elapsed().as_millis().try_into().unwrap_or(u64::MAX));
        fit.map_err(|error| format!("{error:?}"))
    }?;
    (!fit.models.is_empty())
        .then_some(fit)
        .ok_or_else(|| "typed fitter produced no admissible model".to_string())
}
