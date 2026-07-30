use std::collections::BTreeSet;
use std::time::Instant;

use serde::Serialize;
use sha2::{Digest, Sha256};
use vice_evidence::{ChainStatus, Flat2Outcome};
use vice_ir::{Canvas, PixelFilter};
use vice_opt::{posterior_with_search_mass, select_diverse_beam, BeamCandidate, SearchMassInput};

use crate::candidate::{materialize_candidate, CandidateCache, CandidateRequest};
use crate::config::CoreConfig;
use crate::scene::topology_arms;
use crate::types::{
    CandidateRefusal, CandidateSummary, DecisionStatus, FailureReason, RuntimeSummary,
    SuccessArtifacts, VectorizeOutcome, VectorizeReport, VectorizeSuccess, CORE_REPORT_SCHEMA,
};
use crate::VectorizeRequest;

#[derive(Debug, Default)]
struct ReportParts {
    evidence: Option<vice_evidence::Flat2Analysis>,
    fit: Option<vice_fit::ModelRun>,
    beam: Option<vice_opt::BudgetLedger>,
    search_mass: Option<vice_opt::SearchMassCertificate>,
    candidates: Vec<CandidateSummary>,
    candidate_refusals: Vec<CandidateRefusal>,
    selected_hypothesis_id: Option<String>,
    candidate_bytes: u64,
}

fn digest(bytes: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(bytes.as_ref()))
}

#[allow(clippy::too_many_arguments)]
fn make_report(
    status: DecisionStatus,
    reason: Option<FailureReason>,
    request: &VectorizeRequest,
    config: &CoreConfig,
    source_sha256: String,
    production: bool,
    parts: ReportParts,
    started: Instant,
) -> VectorizeReport {
    VectorizeReport {
        schema: CORE_REPORT_SCHEMA,
        status,
        reason,
        request: request.clone(),
        production,
        source_sha256: Some(source_sha256),
        binary_version: env!("CARGO_PKG_VERSION"),
        toolchain: option_env!("VICE_RUSTC_VERSION").unwrap_or("unrecorded"),
        environment: std::env::consts::OS,
        identity: config.identity(),
        calibration: config.confidence.clone(),
        evidence: parts.evidence,
        fit: parts.fit,
        beam: parts.beam,
        search_mass: parts.search_mass,
        runtime: RuntimeSummary {
            elapsed_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
            candidates_scored: parts.candidates.len() as u64,
            candidate_bytes: parts.candidate_bytes,
        },
        candidates: parts.candidates,
        candidate_refusals: parts.candidate_refusals,
        selected_hypothesis_id: parts.selected_hypothesis_id,
    }
}

fn outcome(status: DecisionStatus, report: VectorizeReport) -> VectorizeOutcome {
    match status {
        DecisionStatus::Ambiguous => VectorizeOutcome::Ambiguous(report),
        DecisionStatus::Unsupported => VectorizeOutcome::Unsupported(report),
        DecisionStatus::Failed => VectorizeOutcome::Failed(report),
        DecisionStatus::Success => unreachable!("success needs artifacts"),
    }
}

#[allow(clippy::too_many_arguments)]
fn refuse(
    status: DecisionStatus,
    reason: FailureReason,
    request: &VectorizeRequest,
    config: &CoreConfig,
    source_sha256: String,
    production: bool,
    parts: ReportParts,
    started: Instant,
) -> VectorizeOutcome {
    let report = make_report(
        status,
        Some(reason),
        request,
        config,
        source_sha256,
        production,
        parts,
        started,
    );
    outcome(status, report)
}

fn supported_formations(
    evidence: &vice_evidence::Flat2Evidence,
    report: &vice_evidence::Flat2Analysis,
) -> Result<Vec<vice_ir::GlobalFormationHypothesis>, String> {
    let Flat2Outcome::Supported {
        tied_formations, ..
    } = &report.outcome
    else {
        return Err("evidence outcome is not supported".into());
    };
    let mut ids: BTreeSet<String> = tied_formations.iter().cloned().collect();
    ids.insert(vice_evidence::formation_id(&evidence.formation));
    let enumerated = vice_evidence::enumerate_formations(evidence.formation.exterior);
    let mut found = Vec::new();
    let mut unsupported = Vec::new();
    for formation in enumerated {
        let id = vice_evidence::formation_id(&formation);
        if !ids.contains(&id) {
            continue;
        }
        if formation.pixel_filter == PixelFilter::Box {
            found.push(formation);
        } else {
            unsupported.push(id);
        }
    }
    if !unsupported.is_empty() {
        return Err(format!(
            "explaining formation(s) outside the frozen M7 universe: {}",
            unsupported.join(",")
        ));
    }
    if found.len() != ids.len() {
        return Err("an evidence formation id was not in the declared family".into());
    }
    found.sort_by_key(vice_evidence::formation_id);
    Ok(found)
}

#[derive(Debug, Clone)]
struct FinalModelVariant {
    path_index: usize,
    class: String,
    model: vice_fit::BoundaryModel,
}

fn final_model_variants(fit: &vice_fit::ModelRun) -> Vec<FinalModelVariant> {
    let mut variants = Vec::new();
    for (path_index, selected) in fit.models.iter().enumerate() {
        let mut free = selected.clone();
        free.geometry = selected.stage_h_free_geometry.clone();
        free.code = selected.stage_h_free_code;
        free.primitive_kept = None;
        free.relations_kept = 0;
        free.relation_kept_indices.clear();
        variants.push(FinalModelVariant {
            path_index,
            class: "free".into(),
            model: free.clone(),
        });
        for (index, hypothesis) in selected.relations.iter().enumerate() {
            let mut sibling = free.clone();
            if vice_fit::apply_relation_sibling(&mut sibling, hypothesis, index, true) {
                variants.push(FinalModelVariant {
                    path_index,
                    class: format!("relation-{index}-{:?}", hypothesis.kind).to_lowercase(),
                    model: sibling,
                });
            }
        }
        for (index, hypothesis) in selected.primitives.iter().enumerate() {
            let mut sibling = free.clone();
            if vice_fit::apply_primitive_sibling(&mut sibling, hypothesis, index) {
                variants.push(FinalModelVariant {
                    path_index,
                    class: format!("primitive-{index}-{:?}", hypothesis.kind).to_lowercase(),
                    model: sibling,
                });
            }
        }
    }
    variants.sort_by(|left, right| {
        left.model
            .code
            .total_bits()
            .total_cmp(&right.model.code.total_bits())
            .then_with(|| left.path_index.cmp(&right.path_index))
            .then_with(|| left.class.cmp(&right.class))
    });
    variants
}

/// Standard M7 entry point using the repository's installed configuration.
pub fn vectorize(bytes: &[u8], request: &VectorizeRequest) -> VectorizeOutcome {
    vectorize_with_config(bytes, request, &CoreConfig::development_for(request.preset))
}

/// Configurable M7 entry point for sealed calibration and research harnesses.
/// It is selective by construction: any missing calibration, incomplete
/// search mass, failed verifier, or delivery-court check returns a typed
/// non-success carrying no SVG bytes.
pub fn vectorize_with_config(
    bytes: &[u8],
    request: &VectorizeRequest,
    config: &CoreConfig,
) -> VectorizeOutcome {
    let started = Instant::now();
    let source_sha256 = digest(bytes);
    let provenance_production = request.production
        && config.is_sealed_production()
        && !request.research_override
        && request.milestone_debug.is_none()
        && request.oracle_override.is_none();
    let image =
        match vice_image::CanonicalImage::decode_png(bytes, &vice_image::DecodeLimits::default()) {
            Ok(image) => image,
            Err(error) => {
                return refuse(
                    DecisionStatus::Failed,
                    FailureReason::Decode {
                        detail: error.to_string(),
                    },
                    request,
                    config,
                    source_sha256,
                    false,
                    ReportParts::default(),
                    started,
                )
            }
        };
    if request.strict && image.icc_assumption().is_assumed() {
        return refuse(
            DecisionStatus::Unsupported,
            FailureReason::Evidence {
                detail: "strict mode requires a declared sRGB source".into(),
            },
            request,
            config,
            source_sha256,
            provenance_production,
            ReportParts::default(),
            started,
        );
    }

    let analysis = vice_evidence::analyze_full(
        &image,
        &vice_evidence::ANALYSIS_CONFIG_V1,
        request.oracle_override.clone(),
    );
    let production = provenance_production && analysis.report.production;
    let mut parts = ReportParts {
        evidence: Some(analysis.report.clone()),
        ..ReportParts::default()
    };
    match &analysis.report.outcome {
        Flat2Outcome::Ambiguous { note, .. } => {
            return refuse(
                DecisionStatus::Ambiguous,
                FailureReason::Evidence {
                    detail: (*note).into(),
                },
                request,
                config,
                source_sha256,
                production,
                parts,
                started,
            )
        }
        Flat2Outcome::Unsupported(reason) => {
            return refuse(
                DecisionStatus::Unsupported,
                FailureReason::Evidence {
                    detail: format!("{reason:?}"),
                },
                request,
                config,
                source_sha256,
                production,
                parts,
                started,
            )
        }
        Flat2Outcome::Supported { .. } => {}
    }
    let Some(evidence) = analysis.chosen else {
        return refuse(
            DecisionStatus::Failed,
            FailureReason::Internal {
                detail: "supported evidence had no selected tensor".into(),
            },
            request,
            config,
            source_sha256,
            production,
            parts,
            started,
        );
    };
    let formations = match supported_formations(&evidence, &analysis.report) {
        Ok(formations) => formations,
        Err(detail) => {
            return refuse(
                DecisionStatus::Unsupported,
                FailureReason::FormationOutsideUniverse { detail },
                request,
                config,
                source_sha256,
                production,
                parts,
                started,
            )
        }
    };
    let Some(boundary_observation) = analysis.report.boundary.as_ref() else {
        return refuse(
            DecisionStatus::Unsupported,
            FailureReason::BoundaryOutsideSelectiveCore {
                detail: analysis
                    .report
                    .boundary_refusal
                    .clone()
                    .unwrap_or_else(|| "boundary evidence missing".into()),
            },
            request,
            config,
            source_sha256,
            production,
            parts,
            started,
        );
    };
    if !matches!(boundary_observation.status, ChainStatus::WellFormed)
        || boundary_observation.chains.len() != 1
        || !boundary_observation.chains[0].closed
    {
        return refuse(
            DecisionStatus::Ambiguous,
            FailureReason::BoundaryOutsideSelectiveCore {
                detail: "M7 selective core requires one unambiguous closed boundary".into(),
            },
            request,
            config,
            source_sha256,
            production,
            parts,
            started,
        );
    }
    let chain = boundary_observation.chains[0].clone();
    let arms = match topology_arms(&evidence) {
        Ok(arms) => arms,
        Err(detail) => {
            return refuse(
                DecisionStatus::Unsupported,
                FailureReason::Topology { detail },
                request,
                config,
                source_sha256,
                production,
                parts,
                started,
            )
        }
    };
    let fit = match vice_fit::k_best_boundary_models(
        &chain,
        &vice_fit::FIT_BUDGET_V1,
        f64::from(image.width_px().max(image.height_px())),
        config.k_discrete_paths,
    ) {
        Ok(fit) => fit,
        Err(error) => {
            return refuse(
                DecisionStatus::Unsupported,
                FailureReason::Fitting {
                    detail: format!("{error:?}"),
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
    parts.fit = Some(fit.clone());
    if fit.models.is_empty() {
        return refuse(
            DecisionStatus::Unsupported,
            FailureReason::Fitting {
                detail: "typed fitter produced no admissible model".into(),
            },
            request,
            config,
            source_sha256,
            production,
            parts,
            started,
        );
    }
    let fit_truncated = fit.models.len() >= config.k_discrete_paths;
    let mut model_variants = final_model_variants(&fit);
    let combinations_per_variant = arms.len().saturating_mul(formations.len());
    let max_variants = config
        .beam
        .budget
        .max_candidates_considered
        .checked_div(combinations_per_variant)
        .unwrap_or(0);
    if max_variants == 0 {
        return refuse(
            DecisionStatus::Ambiguous,
            FailureReason::SearchTruncated {
                detail: "candidate budget cannot score one complete model/topology/formation cross-product"
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
    let variants_before_budget = model_variants.len();
    model_variants.truncate(max_variants);
    let variant_truncated = model_variants.len() < variants_before_budget;
    let canvas = Canvas {
        width_px: image.width_px(),
        height_px: image.height_px(),
    };
    let mut candidates = Vec::new();
    let mut candidate_refusals = Vec::new();
    let mut candidate_cache = CandidateCache::default();
    for variant in &model_variants {
        let model = &variant.model;
        for (topology_index, arm) in arms.iter().enumerate() {
            for formation in &formations {
                let formation_class = vice_evidence::formation_id(formation);
                let hypothesis_id = format!(
                    "m{}-{}/t{topology_index}/{formation_class}",
                    variant.path_index, variant.class
                );
                let built = materialize_candidate(
                    CandidateRequest {
                        canvas,
                        evidence: &evidence,
                        chain: &chain,
                        model,
                        arm,
                        formation: *formation,
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
    let selection = match select_diverse_beam(
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
        unexplored: if fit_truncated || variant_truncated {
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
    if let Err(detail) = calibration.permits(&config.identity(), &best_delivery) {
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

    let selected = &candidates[selected_index];
    let trace_json = if request.trace || request.dump_candidates > 0 {
        #[derive(Serialize)]
        struct Trace<'a> {
            selected_hypothesis_id: &'a str,
            optimizer_trace: &'a [vice_opt::OptimizationTraceRow],
            candidate_summaries: Vec<&'a CandidateSummary>,
            candidate_refusals: &'a [CandidateRefusal],
        }
        Some(
            serde_json::to_vec(&Trace {
                selected_hypothesis_id: &selected.score.hypothesis_id,
                optimizer_trace: &selected.summary.optimizer.trace,
                candidate_summaries: candidates
                    .iter()
                    .take(request.dump_candidates)
                    .map(|candidate| &candidate.summary)
                    .collect(),
                candidate_refusals: &candidate_refusals,
            })
            .expect("trace serializes"),
        )
    } else {
        None
    };
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
