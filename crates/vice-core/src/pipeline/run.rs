use super::*;

mod input;
use input::{prepare_input, PreparedInput};
mod trace;
use trace::build_trace;

pub(super) fn vectorize_impl(
    bytes: &[u8],
    request: &VectorizeRequest,
    config: &CoreConfig,
    mut calibration_observer: Option<&mut CalibrationObserver<'_>>,
) -> VectorizeOutcome {
    let input = match prepare_input(bytes, request, config) {
        Ok(input) => input,
        Err(outcome) => return *outcome,
    };
    let PreparedInput {
        started,
        source_sha256,
        production,
        image,
        evidence,
        formations,
        mut parts,
    } = input;
    let topology = topology_arms(&evidence);
    let proposal = topology.proposal;
    let topology_classes_upper_bound = proposal
        .envelope
        .hypotheses
        .iter()
        .map(|hypothesis| (hypothesis.signature.components, hypothesis.signature.holes))
        .collect::<BTreeSet<_>>()
        .len() as u64;
    let formation_classes_upper_bound = formations
        .iter()
        .map(vice_evidence::formation_id)
        .collect::<BTreeSet<_>>()
        .len() as u64;
    let mut topology_traces = topology.traces;
    let mut topology_refusals = topology.refusals;
    let arms = topology.arms;
    if arms.is_empty() {
        parts.topology = Some(TopologyEnvelopeTrace {
            proposal,
            materialized_arms: topology_traces,
            materialization_refusals: topology_refusals,
            prefit_budget_pruned_arms: Vec::new(),
        });
        return refuse(
            DecisionStatus::Unsupported,
            FailureReason::Topology {
                detail: "no M4.5 envelope hypothesis produced an audited closed-boundary DCEL \
                         that bound every observed chain"
                    .into(),
            },
            request,
            config,
            source_sha256,
            production,
            parts,
            started,
        );
    }
    let max_topology_arms = config
        .beam
        .budget
        .max_candidates_considered
        .checked_div(formations.len())
        .unwrap_or(0)
        .min(config.beam.width);
    if max_topology_arms == 0 {
        parts.topology = Some(TopologyEnvelopeTrace {
            proposal,
            materialized_arms: topology_traces,
            materialization_refusals: topology_refusals,
            prefit_budget_pruned_arms: Vec::new(),
        });
        return refuse(
            DecisionStatus::Ambiguous,
            FailureReason::SearchTruncated {
                detail: "candidate budget cannot fit and score one topology/formation seed".into(),
            },
            request,
            config,
            source_sha256,
            production,
            parts,
            started,
        );
    }
    let topology_budget_truncated = arms.len() > max_topology_arms;
    let topology_budget_pruned_arms = arms
        .iter()
        .skip(max_topology_arms)
        .map(|arm| arm.trace.clone())
        .collect::<Vec<_>>();
    let canvas_dim_px = f64::from(image.width_px().max(image.height_px()));
    let mut fitted_arms = Vec::new();
    let mut fit_cache =
        std::collections::BTreeMap::<String, Result<vice_fit::ModelRun, String>>::new();
    for mut arm in arms.into_iter().take(max_topology_arms) {
        let mut fits = Vec::with_capacity(arm.chains.len());
        let mut fit_refusal = None;
        for (chain_index, chain) in arm.chains.iter().enumerate() {
            let fit_key = match serde_json::to_vec(chain) {
                Ok(bytes) => digest(bytes),
                Err(error) => {
                    fit_refusal = Some(format!(
                        "chain {chain_index} identity serialization: {error}"
                    ));
                    break;
                }
            };
            let fit = if let Some(cached) = fit_cache.get(&fit_key) {
                cached.clone()
            } else {
                let fit = if canvas_dim_px >= 128.0 {
                    vice_fit::k_best_boundary_models_bounded(
                        chain,
                        &vice_fit::FIT_BUDGET_V1,
                        canvas_dim_px,
                        config.k_discrete_paths,
                        vice_fit::DISCRETE_PROPOSAL_SAMPLE_CAP_V1,
                    )
                    .map_err(|error| format!("{error:?}"))
                } else {
                    vice_fit::k_best_boundary_models(
                        chain,
                        &vice_fit::FIT_BUDGET_V1,
                        canvas_dim_px,
                        config.k_discrete_paths,
                    )
                    .map_err(|error| format!("{error:?}"))
                }
                .and_then(|fit| {
                    (!fit.models.is_empty())
                        .then_some(fit)
                        .ok_or_else(|| "typed fitter produced no admissible model".to_string())
                });
                fit_cache.insert(fit_key, fit.clone());
                fit
            };
            match fit {
                Ok(fit) => fits.push(fit),
                Err(error) => {
                    fit_refusal = Some(format!("chain {chain_index}: {error}"));
                    break;
                }
            }
        }
        if let Some(detail) = fit_refusal {
            topology_refusals.push(TopologyArmRefusal {
                topology_class: arm.topology_class.clone(),
                signature_sha256: arm.trace.signature_sha256.clone(),
                foreground_connectivity: arm.trace.foreground_connectivity.clone(),
                field: arm.trace.field,
                saddle: arm.trace.saddle,
                extraction_level: arm.trace.extraction_level,
                detail: format!("complete topology refit refused: {detail}"),
            });
            continue;
        }
        arm.trace.fit_models_per_chain =
            fits.iter().map(|fit| fit.models.len()).collect::<Vec<_>>();
        if let Some(trace) = topology_traces
            .iter_mut()
            .find(|trace| trace.class == arm.class)
        {
            *trace = arm.trace.clone();
        }
        let baseline_models = fits.iter().map(|fit| free_model(&fit.models[0])).collect();
        let variants = final_scene_variants(&fits, &arm.chains, canvas_dim_px);
        let variant_count = variants.len();
        let variants = retain_variant_diversity(variants, variant_count, canvas_dim_px >= 128.0);
        fitted_arms.push(FittedTopologyArm {
            arm,
            fits,
            baseline_models,
            variants,
        });
    }
    parts.topology = Some(TopologyEnvelopeTrace {
        proposal,
        materialized_arms: topology_traces,
        materialization_refusals: topology_refusals,
        prefit_budget_pruned_arms: topology_budget_pruned_arms,
    });
    if fitted_arms.is_empty() {
        let first = parts
            .topology
            .as_ref()
            .and_then(|topology| topology.materialization_refusals.last())
            .map_or_else(
                || "no topology arm published a more specific refusal".to_string(),
                |refusal| refusal.detail.clone(),
            );
        return refuse(
            DecisionStatus::Unsupported,
            FailureReason::Fitting {
                detail: format!(
                    "every materializable topology envelope arm refused its complete typed \
                     boundary refit; last: {first}"
                ),
            },
            request,
            config,
            source_sha256,
            production,
            parts,
            started,
        );
    }
    parts.fits = fitted_arms[0].fits.clone();
    parts.fit_diagnostics = fitted_arms
        .iter()
        .flat_map(|bundle| bundle.fits.iter().cloned())
        .collect();
    let fit_truncated = fitted_arms.iter().any(|bundle| {
        bundle.fits.iter().any(|fit| {
            fit.models.len() >= config.k_discrete_paths
                || fit.discrete_search_samples < fit.observed_samples
                || fit.continuous_solve_samples < fit.observed_samples
                || fit.full_certification_refusals > 0
                || fit.resource_pruned_proposals > 0
                || fit.proposal_levels_skipped_after_certification > 0
        })
    });
    let canvas = Canvas {
        width_px: image.width_px(),
        height_px: image.height_px(),
    };
    let mut candidates = Vec::new();
    let mut candidate_refusals = Vec::new();
    let mut candidate_cache = CandidateCache::default();
    let mut proposed_transactions = std::collections::BTreeMap::<TransactionKind, u64>::new();
    let planned_materializations = fitted_arms
        .iter()
        .map(|bundle| bundle.variants.len().saturating_mul(formations.len()))
        .sum::<usize>();
    let mut scheduled = BTreeSet::new();
    let mut materialization_order = Vec::with_capacity(planned_materializations);
    for topology_index in 0..fitted_arms.len().min(config.beam.min_topology_classes) {
        let task = (topology_index, 0, 0);
        if scheduled.insert(task) {
            materialization_order.push(task);
        }
    }
    for formation_index in 0..formations.len().min(config.beam.min_formation_classes) {
        let task = (0, 0, formation_index);
        if scheduled.insert(task) {
            materialization_order.push(task);
        }
    }
    for variant_index in 0..fitted_arms[0]
        .variants
        .len()
        .min(TRANSACTION_DIVERSITY_SEED_CLASSES)
    {
        let task = (0, variant_index, 0);
        if scheduled.insert(task) {
            materialization_order.push(task);
        }
    }
    let diversity_seed_materializations = materialization_order.len();
    for (topology_index, bundle) in fitted_arms.iter().enumerate() {
        for variant_index in 0..bundle.variants.len() {
            for formation_index in 0..formations.len() {
                let task = (topology_index, variant_index, formation_index);
                if scheduled.insert(task) {
                    materialization_order.push(task);
                }
            }
        }
    }
    let unmaterialized_by_candidate_budget = materialization_order
        .len()
        .saturating_sub(config.beam.budget.max_candidates_considered);
    let evaluation_truncated = unmaterialized_by_candidate_budget > 0;
    materialization_order.truncate(config.beam.budget.max_candidates_considered);
    let scheduled_materializations = materialization_order.len();
    // A time budget cannot be subordinated to a quota: materialize one
    // deterministic seed so the run has a candidate, then enforce the
    // deadline before every further topology/formation/transaction seed.
    // Unreached diversity slots remain explicit unexplored mass.
    let mandatory_diversity_materializations =
        usize::from(diversity_seed_materializations > 0 && scheduled_materializations > 0);
    let mut attempted_materializations = 0usize;
    let mut time_truncated = false;
    'materialization: for (topology_index, variant_index, formation_index) in materialization_order
    {
        let bundle = &fitted_arms[topology_index];
        let variant = &bundle.variants[variant_index];
        let arm = &bundle.arm;
        let formation = &formations[formation_index];
        if attempted_materializations >= mandatory_diversity_materializations
            && started.elapsed().as_millis() >= u128::from(config.beam.budget.max_elapsed_ms)
        {
            time_truncated = true;
            break 'materialization;
        }
        attempted_materializations += 1;
        for transaction in &variant.model_transactions {
            *proposed_transactions.entry(transaction.kind).or_default() += 1;
        }
        if arm.class != fitted_arms[0].arm.class {
            let kind = if arm.dcel.holes() != fitted_arms[0].arm.dcel.holes() {
                TransactionKind::TopologyHole
            } else if arm.dcel.foreground_faces() < fitted_arms[0].arm.dcel.foreground_faces() {
                TransactionKind::TopologyBridge
            } else if arm.dcel.foreground_faces() > fitted_arms[0].arm.dcel.foreground_faces() {
                TransactionKind::TopologySplit
            } else if arm.dcel.boundaries().len() < fitted_arms[0].arm.dcel.boundaries().len() {
                TransactionKind::TopologyMerge
            } else {
                TransactionKind::TopologySplit
            };
            *proposed_transactions.entry(kind).or_default() += 1;
        }
        if *formation != formations[0] {
            let kind = if formation.exterior != formations[0].exterior {
                TransactionKind::ExteriorChange
            } else {
                TransactionKind::FormationChange
            };
            *proposed_transactions.entry(kind).or_default() += 1;
        }
        *proposed_transactions
            .entry(TransactionKind::PaintChange)
            .or_default() += 1;
        let formation_class = vice_evidence::formation_id(formation);
        let hypothesis_id = format!("{}/t{topology_index}/{formation_class}", variant.class);
        let built = materialize_candidate(
            CandidateRequest {
                canvas,
                evidence: &evidence,
                chains: &arm.chains,
                models: &variant.models,
                arm,
                formation: *formation,
                model_transactions: &variant.model_transactions,
                transaction_base_arm: &fitted_arms[0].arm,
                transaction_base_chains: &fitted_arms[0].arm.chains,
                transaction_base_models: &fitted_arms[0].baseline_models,
                transaction_base_formation: formations[0],
                hypothesis_id: hypothesis_id.clone(),
                formation_class,
                image: &image,
                intent: request.intent,
                config,
            },
            &mut candidate_cache,
        );
        match built {
            Ok(candidate) => candidates.push(candidate),
            Err(error) => candidate_refusals.push(error),
        }
    }
    candidates.sort_by(|left, right| {
        left.score
            .total_bits
            .total_cmp(&right.score.total_bits)
            .then_with(|| {
                left.summary
                    .scene_digest_sha256
                    .cmp(&right.summary.scene_digest_sha256)
            })
            .then_with(|| left.score.hypothesis_id.cmp(&right.score.hypothesis_id))
    });
    parts.candidate_bytes = candidates
        .iter()
        .map(|candidate| candidate.estimated_memory_bytes)
        .sum();
    parts.candidates = candidates
        .iter()
        .map(|candidate| candidate.summary.clone())
        .collect();
    parts.candidate_refusals = candidate_refusals.clone();
    let mut applied_transactions = std::collections::BTreeMap::<TransactionKind, u64>::new();
    for candidate in &candidates {
        for transaction in &candidate.summary.transactions {
            *applied_transactions.entry(transaction.kind).or_default() += 1;
        }
    }
    let rows = TransactionKind::ALL
        .into_iter()
        .map(|kind| {
            let proposed = proposed_transactions.get(&kind).copied().unwrap_or(0);
            let atomic_applied = applied_transactions.get(&kind).copied().unwrap_or(0);
            TransactionInventoryRow {
                kind,
                proposed,
                atomic_applied,
                verified_and_exact_scored: atomic_applied,
                refused_before_score: proposed.saturating_sub(atomic_applied),
                not_applicable: proposed == 0,
            }
        })
        .collect::<Vec<_>>();
    parts.transaction_inventory = Some(TransactionInventory {
        complete_kind_enumeration: rows.len() == TransactionKind::ALL.len()
            && rows
                .iter()
                .zip(TransactionKind::ALL)
                .all(|(row, kind)| row.kind == kind),
        rows,
    });
    if candidates.is_empty() {
        let detail = if candidate_refusals.is_empty() {
            "no candidate entered the verifier".into()
        } else {
            format!(
                "all candidates failed verification/delivery; first: {}: {}",
                candidate_refusals[0].hypothesis_id, candidate_refusals[0].detail
            )
        };
        return refuse(
            DecisionStatus::Ambiguous,
            FailureReason::NoVerifiedCandidate { detail },
            request,
            config,
            source_sha256,
            production,
            parts,
            started,
        );
    }
    let beam_candidates: Vec<_> = candidates
        .iter()
        .map(|candidate| BeamCandidate {
            score: candidate.score.clone(),
            canonical_scene_digest: candidate.summary.scene_digest_sha256.clone(),
            estimated_memory_bytes: candidate.estimated_memory_bytes,
        })
        .collect();
    let mut selection = match select_diverse_beam(
        beam_candidates,
        config.beam,
        started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
    ) {
        Ok(selection) => selection,
        Err(error) => {
            return refuse(
                DecisionStatus::Ambiguous,
                FailureReason::SearchTruncated {
                    detail: error.to_string(),
                },
                request,
                config,
                source_sha256,
                production,
                parts,
                started,
            )
        }
    };
    selection.ledger.time_budget_exhausted |= time_truncated;
    selection.ledger.unmaterialized_by_candidate_budget = unmaterialized_by_candidate_budget
        .try_into()
        .unwrap_or(u64::MAX);
    selection.ledger.unmaterialized_by_time_budget = if time_truncated {
        scheduled_materializations
            .saturating_sub(attempted_materializations)
            .try_into()
            .unwrap_or(u64::MAX)
    } else {
        0
    };
    parts.beam = Some(selection.ledger.clone());
    let budget_ids: BTreeSet<_> = selection
        .budget_pruned
        .iter()
        .map(|candidate| candidate.score.hypothesis_id.as_str())
        .collect();
    let mut explored_kept = Vec::new();
    let mut budget_pruned = Vec::new();
    for candidate in &candidates {
        if budget_ids.contains(candidate.score.hypothesis_id.as_str()) {
            budget_pruned.push(candidate.score.clone());
        } else {
            explored_kept.push(candidate.score.clone());
        }
    }
    let search_mass = match posterior_with_search_mass(SearchMassInput {
        identity: config.identity(),
        explored_kept,
        budget_pruned,
        topology_classes_upper_bound,
        formation_classes_upper_bound,
        unexplored: if topology_budget_truncated
            || fit_truncated
            || evaluation_truncated
            || time_truncated
        {
            config
                .confidence
                .as_ref()
                .and_then(|calibration| calibration.empirical_unexplored_relative_mass_upper_bound)
                .map_or(vice_opt::UnexploredMassInput::Unknown, |upper_bound| {
                    vice_opt::UnexploredMassInput::EmpiricallyCalibrated {
                        relative_mass_upper_bound: upper_bound,
                    }
                })
        } else {
            vice_opt::UnexploredMassInput::Complete
        },
    }) {
        Ok(certificate) => certificate,
        Err(error) => {
            return refuse(
                DecisionStatus::Failed,
                FailureReason::Internal {
                    detail: error.to_string(),
                },
                request,
                config,
                source_sha256,
                production,
                parts,
                started,
            )
        }
    };
    let Some(best_delivery) = search_mass.best_delivery() else {
        return refuse(
            DecisionStatus::Failed,
            FailureReason::Internal {
                detail: "posterior certificate has no delivery class".into(),
            },
            request,
            config,
            source_sha256,
            production,
            parts,
            started,
        );
    };
    let best_delivery_digest = best_delivery.delivery_digest.clone();
    let best_delivery = best_delivery.clone();
    let selected_index = candidates
        .iter()
        .position(|candidate| candidate.score.delivery_digest == best_delivery_digest)
        .expect("posterior delivery is formed from candidates");
    parts.selected_hypothesis_id = Some(candidates[selected_index].score.hypothesis_id.clone());
    parts.search_mass = Some(search_mass.clone());
    let selected = &candidates[selected_index];
    let top2_class_margin_bits =
        search_mass
            .delivery
            .get(1)
            .map_or(SINGLE_DELIVERY_CLASS_MARGIN_BITS_V1, |runner_up| {
                if runner_up.explored_mass > 0.0 {
                    (best_delivery.explored_mass / runner_up.explored_mass)
                        .log2()
                        .clamp(0.0, SINGLE_DELIVERY_CLASS_MARGIN_BITS_V1)
                } else {
                    SINGLE_DELIVERY_CLASS_MARGIN_BITS_V1
                }
            });
    let diagnostics = &selected.summary.score.diagnostics;
    let predictive_bits_per_block = if diagnostics.blocks == 0 {
        f64::MAX
    } else {
        selected.summary.score.pixel_bits / diagnostics.blocks as f64
    };
    let max_abs_residual_lag1 = diagnostics.lag1_x.abs().max(diagnostics.lag1_y.abs());
    let phase_envelope_stable = parts.topology.as_ref().is_some_and(|topology| {
        let same = |class: &str| class == selected.score.topology_class;
        !topology.materialized_arms.is_empty()
            && topology
                .materialized_arms
                .iter()
                .all(|arm| same(&arm.topology_class))
            && topology
                .prefit_budget_pruned_arms
                .iter()
                .all(|arm| same(&arm.topology_class))
            && topology
                .materialization_refusals
                .iter()
                .all(|arm| same(&arm.topology_class))
    });
    let sample_step_certificate_stable = fitted_arms
        .iter()
        .find(|bundle| bundle.arm.class == selected.summary.topology_arm)
        .is_some_and(|bundle| {
            !bundle.fits.is_empty()
                && bundle.fits.iter().all(|fit| {
                    fit.full_resolution_certified
                        && fit.observed_samples > 0
                        && fit.discrete_search_samples > 0
                        && fit.continuous_solve_samples > 0
                        && !fit.discrete_search_levels.is_empty()
                        && !fit.models.is_empty()
                })
        });
    let canonical_binding_check = (|| {
        let scene = vice_ir::parse_scene(&selected.scene_json)
            .map_err(|error| format!("parse selected scene: {error}"))?;
        let scene = vice_ir::ValidatedScene::new(scene)
            .map_err(|error| format!("validate selected scene: {error}"))?;
        let roundtrip_topology = vice_verify::topology_signature_sha256(scene.scene())
            .map_err(|error| format!("roundtrip topology: {error}"))?;
        if roundtrip_topology != selected.summary.post_quantization.topology_signature_sha256 {
            return Err(format!(
                "canonical scene roundtrip changed topology: report={} roundtrip={}",
                selected.summary.post_quantization.topology_signature_sha256, roundtrip_topology
            ));
        }
        if scene.scene().graph.boundaries.len() != selected.bindings.len() {
            return Err(format!(
                "canonical scene roundtrip changed boundary count: scene={} bindings={}",
                scene.scene().graph.boundaries.len(),
                selected.bindings.len()
            ));
        }
        let bindings = vice_verify::rebind_scene_bindings(
            scene.scene(),
            &selected.bindings,
            config.verification,
        )
        .map_err(|error| format!("canonical binding remap: {error}"))?;
        Ok((scene, bindings))
    })();
    if let Ok((_, bindings)) = &canonical_binding_check {
        parts.selected_boundary_bindings = bindings.clone();
    }
    let tighter_tolerance = config
        .verification
        .render_options
        .budget
        .chord_tolerance
        .px()
        / 2.0;
    let tighter_render_check = canonical_binding_check
        .as_ref()
        .map_err(Clone::clone)
        .and_then(|(scene, bindings)| {
            let budget =
                vice_render::TessellationBudget::with_chord_tolerance_px(tighter_tolerance)
                    .ok_or_else(|| "invalid tighter tessellation budget".to_string())?;
            let mut verification = config.verification;
            verification.render_options = verification.render_options.with_budget(budget);
            vice_verify::preseal_scene(scene.scene(), bindings, verification)
                .map(|_| ())
                .map_err(|error| error.to_string())
        });
    let render_tolerance_certificate_stable = tighter_render_check.is_ok();
    let accepted_local = selected
        .summary
        .optimizer
        .trace
        .iter()
        .filter(|row| row.accepted && !row.full_check)
        .count();
    let full_checks = selected
        .summary
        .optimizer
        .trace
        .iter()
        .filter(|row| row.full_check)
        .count();
    let local_scopes_closed = !selected.summary.optimizer.block_plan.is_empty()
        && selected.summary.optimizer.full_check_every_accepted_blocks == 1
        && selected.summary.optimizer.block_plan.iter().all(|block| {
            !block.scope.global && block.scope.roi.is_some() && block.scope.halo_px > 0
        })
        && selected
            .summary
            .optimizer
            .trace
            .iter()
            .filter(|row| row.accepted && !row.full_check)
            .all(|row| !row.scope.global && row.scope.roi.is_some() && row.scope.halo_px > 0);
    let solver_certificate_stable = local_scopes_closed
        && full_checks >= accepted_local
        && selected.summary.optimizer.trace.iter().all(|row| {
            row.child_bits.is_finite()
                && row.parent_bits.is_finite()
                && (!row.full_check || (row.accepted && !row.rolled_back_to_verified))
        });
    let mut perturbation_stability = PerturbationStability::from_legs(
        phase_envelope_stable,
        sample_step_certificate_stable,
        render_tolerance_certificate_stable,
        solver_certificate_stable,
    );
    perturbation_stability.render_tolerance_refusal = tighter_render_check.err();
    let confidence_metrics = ConfidenceMetrics {
        top2_class_margin_bits,
        posterior_predictive_bits_per_block: if predictive_bits_per_block.is_finite() {
            predictive_bits_per_block
        } else {
            f64::MAX
        },
        max_abs_residual_lag1: if max_abs_residual_lag1.is_finite() {
            max_abs_residual_lag1
        } else {
            f64::MAX
        },
        topology_entropy_upper_bound: search_mass.topology_entropy_upper_bound.clone(),
        formation_entropy_upper_bound: search_mass.formation_entropy_upper_bound.clone(),
        perturbation_stability,
    };
    parts.confidence_metrics = Some(confidence_metrics.clone());
    if let Some(observer) = calibration_observer.as_mut() {
        let baseline = candidates
            .iter()
            .find(|candidate| candidate.score.hypothesis_id.starts_with("baseline-free/"));
        observer(selected, baseline);
    }
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
    if let Err(detail) =
        calibration.permits(&config.identity(), &best_delivery, &confidence_metrics)
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

    let trace_json = build_trace(request, selected, &candidates, &candidate_refusals);
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
