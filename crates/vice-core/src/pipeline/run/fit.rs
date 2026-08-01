use super::*;

#[derive(Clone)]
pub(super) struct ChainFit {
    pub run: vice_fit::ModelRun,
    pub observed_polyline: Option<vice_fit::BoundaryModel>,
}

pub(super) fn fit_chain(
    chain: &vice_evidence::BoundaryChain,
    canvas_dim_px: f64,
    config: &CoreConfig,
    runtime: &mut crate::types::FittingRuntimeSummary,
) -> Result<ChainFit, String> {
    runtime.chain_attempts = runtime.chain_attempts.saturating_add(1);
    let primary = if canvas_dim_px >= 128.0 {
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
    };
    let observed = vice_fit::observed_polyline_rescue_model(chain, canvas_dim_px)
        .map_err(|error| format!("{error:?}"));
    match primary {
        Ok(run) if !run.models.is_empty() => Ok(ChainFit {
            run,
            observed_polyline: observed.ok(),
        }),
        primary => {
            let primary = primary
                .err()
                .unwrap_or_else(|| "typed fitter produced no admissible model".to_string());
            let model = observed.map_err(|rescue| {
                format!("compact grammar refused ({primary}); observed polyline refused ({rescue})")
            })?;
            let segments = model.families.len();
            Ok(ChainFit {
                run: vice_fit::ModelRun {
                    models: vec![model.clone()],
                    observed_samples: chain.samples.len(),
                    discrete_search_samples: chain.samples.len(),
                    discrete_search_levels: vec![chain.samples.len()],
                    continuous_solve_samples: chain.samples.len(),
                    full_resolution_refit: true,
                    full_resolution_certified: true,
                    selected_cut: 0,
                    cuts_evaluated: 1,
                    candidates: 1,
                    edges: segments,
                    discrete_paths: 1,
                    refused: vec![("compact_grammar_refused", 1)],
                    cost_refusals: Vec::new(),
                    worst_normal_to_euclidean_ratio: None,
                    material_cost_samples: chain.samples.len(),
                    not_representable: 0,
                    relations_considered: 0,
                    relations_accepted: 0,
                    primitives_considered: 0,
                    primitives_accepted: 0,
                    full_certification_refusals: 1,
                    resource_pruned_proposals: 0,
                    proposal_levels_skipped_after_certification: 0,
                },
                observed_polyline: Some(model),
            })
        }
    }
}
