use std::collections::BTreeSet;
use std::time::Instant;

use serde::Serialize;
use sha2::{Digest, Sha256};
use vice_evidence::{ChainStatus, Flat2Outcome};
use vice_ir::{Canvas, Paint, PixelFilter};
use vice_opt::{
    posterior_with_search_mass, select_diverse_beam, BeamCandidate, PriorCodeLengths,
    ScoredHypothesis, SearchMassInput,
};
use vice_svg::{
    build_export_plan, canonical_export_plan_bytes, materialize_svg,
    parse_and_render_independently, SvgProfile,
};
use vice_verify::{quantize_and_verify, seal_delivery};

use crate::config::CoreConfig;
use crate::scene::{build_scene_candidate, optimize_paint, topology_arms};
use crate::types::{
    CandidateSummary, DecisionStatus, FailureReason, RuntimeSummary, SuccessArtifacts,
    VectorizeOutcome, VectorizeReport, VectorizeSuccess, CORE_REPORT_SCHEMA,
};
use crate::VectorizeRequest;

#[derive(Debug)]
struct MaterializedCandidate {
    summary: CandidateSummary,
    score: ScoredHypothesis,
    scene_json: Vec<u8>,
    plan_json: Vec<u8>,
    pure_svg: Vec<u8>,
    seam_svg: Vec<u8>,
    render_png: Vec<u8>,
    seal_json: Vec<u8>,
    estimated_memory_bytes: u64,
}

#[derive(Debug, Default)]
struct ReportParts {
    evidence: Option<vice_evidence::Flat2Analysis>,
    fit: Option<vice_fit::ModelRun>,
    beam: Option<vice_opt::BudgetLedger>,
    search_mass: Option<vice_opt::SearchMassCertificate>,
    candidates: Vec<CandidateSummary>,
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

fn priors(model: &vice_fit::BoundaryModel, opaque_faces: usize) -> PriorCodeLengths {
    PriorCodeLengths {
        topology_bits: model.code.topology_bits + 1.0,
        geometry_bits: model.code.geometry_bits,
        paint_bits: 24.0 * opaque_faces as f64,
        relation_bits: model.code.relation_bits,
        formation_bits: 2.0,
    }
}

fn candidate_memory(candidate: &MaterializedCandidate) -> u64 {
    [
        candidate.scene_json.len(),
        candidate.plan_json.len(),
        candidate.pure_svg.len(),
        candidate.seam_svg.len(),
        candidate.render_png.len(),
        candidate.seal_json.len(),
    ]
    .into_iter()
    .map(|value| value as u64)
    .sum()
}

/// Standard M7 entry point using the repository's installed configuration.
pub fn vectorize(bytes: &[u8], request: &VectorizeRequest) -> VectorizeOutcome {
    vectorize_with_config(bytes, request, &CoreConfig::development())
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
    if fit.models.len() >= config.k_discrete_paths {
        return refuse(
            DecisionStatus::Ambiguous,
            FailureReason::SearchTruncated {
                detail: format!(
                    "k-best frontier reached k={}; unexplored geometry mass has no bound",
                    config.k_discrete_paths
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
    let supported_count = fit
        .models
        .len()
        .saturating_mul(arms.len())
        .saturating_mul(formations.len());
    if supported_count > config.beam.budget.max_candidates_considered {
        return refuse(
            DecisionStatus::Ambiguous,
            FailureReason::SearchTruncated {
                detail: format!(
                    "{supported_count} supported hypotheses exceed the declared candidate budget {}",
                    config.beam.budget.max_candidates_considered
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

    let canvas = Canvas {
        width_px: image.width_px(),
        height_px: image.height_px(),
    };
    let mut candidates = Vec::new();
    let mut candidate_failures = Vec::new();
    for (model_index, model) in fit.models.iter().enumerate() {
        for (topology_index, arm) in arms.iter().enumerate() {
            for formation in &formations {
                let formation_class = vice_evidence::formation_id(formation);
                let hypothesis_id = format!("m{model_index}/t{topology_index}/{formation_class}");
                let built = (|| -> Result<MaterializedCandidate, String> {
                    let candidate =
                        build_scene_candidate(canvas, &evidence, &chain, model, arm, *formation)?;
                    let opaque_faces = candidate
                        .scene
                        .graph
                        .faces
                        .iter()
                        .filter(|face| matches!(face.paint, Paint::OpaqueSolid(_)))
                        .count();
                    let prior = priors(model, opaque_faces);
                    let base = vice_verify::preseal_scene(
                        &candidate.scene,
                        &candidate.bindings,
                        config.verification,
                    )
                    .map_err(|error| error.to_string())?;
                    let (candidate, optimizer) = optimize_paint(
                        candidate,
                        &image,
                        base.render(),
                        config.likelihood,
                        prior,
                        config.trust_region,
                    )?;
                    let verified = quantize_and_verify(
                        &candidate.scene,
                        &candidate.bindings,
                        config.verification,
                        config.quantization,
                    )
                    .map_err(|error| error.to_string())?;
                    let score = vice_opt::score_full_resolution(
                        verified.scene(),
                        &image,
                        verified.render(),
                        config.likelihood,
                        prior,
                    )
                    .map_err(|error| error.to_string())?;
                    let plan = build_export_plan(
                        verified.scene(),
                        config.export_decimal_places,
                        config.apron_width_px,
                    )
                    .map_err(|error| error.to_string())?;
                    let plan_json =
                        canonical_export_plan_bytes(&plan).map_err(|error| error.to_string())?;
                    let pure_svg = materialize_svg(&plan, SvgProfile::PurePartition)
                        .map_err(|error| error.to_string())?;
                    let seam_svg = materialize_svg(&plan, SvgProfile::SeamSafe)
                        .map_err(|error| error.to_string())?;
                    let pure_witness = parse_and_render_independently(&pure_svg)
                        .map_err(|error| error.to_string())?;
                    let seam_witness = parse_and_render_independently(&seam_svg)
                        .map_err(|error| error.to_string())?;
                    let seal =
                        seal_delivery(&verified, &plan, &pure_witness, &seam_witness, config.seal)
                            .map_err(|error| error.to_string())?;
                    let scene_json = vice_ir::canonical_scene_bytes(verified.scene())
                        .map_err(|error| error.to_string())?;
                    let scene_digest_sha256 = verified
                        .post_quantization_certificate()
                        .post_scene_digest_sha256
                        .clone();
                    let delivery_digest = digest(
                        format!(
                            "{}|{}",
                            pure_witness.render_digest_sha256(),
                            seam_witness.render_digest_sha256()
                        )
                        .as_bytes(),
                    );
                    let scored = ScoredHypothesis {
                        hypothesis_id: hypothesis_id.clone(),
                        delivery_digest: delivery_digest.clone(),
                        topology_class: arm.class.clone(),
                        formation_class: formation_class.clone(),
                        total_bits: score.total_bits,
                    };
                    let summary = CandidateSummary {
                        hypothesis_id: hypothesis_id.clone(),
                        topology_class: arm.class.clone(),
                        formation_class,
                        scene_digest_sha256,
                        delivery_digest,
                        score,
                        pre_quantization: verified.pre_quantization_certificate().clone(),
                        post_quantization: verified.post_quantization_certificate().clone(),
                        delivery_seal: seal.clone(),
                        optimizer,
                    };
                    let seal_json = serde_json::to_vec(&seal).map_err(|error| error.to_string())?;
                    let mut candidate = MaterializedCandidate {
                        summary,
                        score: scored,
                        scene_json,
                        plan_json,
                        pure_svg,
                        seam_svg,
                        render_png: seam_witness.png_bytes().to_vec(),
                        seal_json,
                        estimated_memory_bytes: 0,
                    };
                    candidate.estimated_memory_bytes = candidate_memory(&candidate);
                    Ok(candidate)
                })();
                match built {
                    Ok(candidate) => candidates.push(candidate),
                    Err(error) => candidate_failures.push(format!("{hypothesis_id}: {error}")),
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
    if candidates.is_empty() {
        let detail = if candidate_failures.is_empty() {
            "no candidate entered the verifier".into()
        } else {
            format!(
                "all candidates failed verification/delivery; first: {}",
                candidate_failures[0]
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
        supported_hypotheses: candidates.len() as u64,
        explored_kept,
        budget_pruned,
        unexplored_bound: None,
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
    let posterior_lower_bound = best_delivery.posterior_lower_bound;
    let selected_index = candidates
        .iter()
        .position(|candidate| candidate.score.delivery_digest == best_delivery_digest)
        .expect("posterior delivery is formed from candidates");
    parts.selected_hypothesis_id = Some(candidates[selected_index].score.hypothesis_id.clone());
    parts.search_mass = Some(search_mass.clone());
    if search_mass.truncated {
        return refuse(
            DecisionStatus::Ambiguous,
            FailureReason::SearchTruncated {
                detail: "resource-pruned posterior mass prevents a production decision".into(),
            },
            request,
            config,
            source_sha256,
            production,
            parts,
            started,
        );
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
    if let Err(detail) = calibration.permits(&config.identity(), posterior_lower_bound) {
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
            candidate_failures: &'a [String],
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
                candidate_failures: &candidate_failures,
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
