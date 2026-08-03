use std::collections::BTreeSet;
use std::time::Instant;

use sha2::{Digest, Sha256};
use vice_evidence::Flat2Outcome;
use vice_ir::{Canvas, PixelFilter};
use vice_opt::{
    posterior_with_search_mass, select_diverse_beam, BeamCandidate, SearchMassInput,
    TransactionKind,
};

use crate::candidate::{
    materialize_candidate, score_candidate_proposal, CandidateCache, CandidateModelTransaction,
    CandidateRequest,
};
use crate::config::{ConfidenceMetrics, CoreConfig, PerturbationStability};
use crate::scene::{topology_arms, TopologyArm};
use crate::types::{
    CalibrationRun, CalibrationWitness, CandidateFailureStage, CandidateRefusal, CandidateSummary,
    DecisionStatus, FailureReason, QualityAdmissionWitness, RuntimeStageSummary, RuntimeSummary,
    SuccessArtifacts, TopologyArmRefusal, TopologyEnvelopeTrace, TransactionInventory,
    TransactionInventoryRow, VectorizeOutcome, VectorizeReport, VectorizeSuccess,
    CORE_REPORT_SCHEMA,
};
use crate::{Preset, VectorizeRequest};

const TRANSACTION_DIVERSITY_SEED_CLASSES: usize = 2;
const SINGLE_DELIVERY_CLASS_MARGIN_BITS_V1: f64 = 1024.0;

#[path = "pipeline/run/paint.rs"]
mod paint;
use paint::paint_risk_metrics;
#[path = "pipeline/run/trace.rs"]
mod trace;
use trace::build_trace;

type CalibrationObserver<'a> = dyn FnMut(
        &crate::candidate::MaterializedCandidate,
        Option<&crate::candidate::MaterializedCandidate>,
        &[CandidateRefusal],
    ) + 'a;

#[derive(Debug, Default)]
struct ReportParts {
    evidence: Option<vice_evidence::Flat2Analysis>,
    topology: Option<TopologyEnvelopeTrace>,
    fit_diagnostics: Vec<vice_fit::ModelRun>,
    fits: Vec<vice_fit::ModelRun>,
    beam: Option<vice_opt::BudgetLedger>,
    search_mass: Option<vice_opt::SearchMassCertificate>,
    confidence_metrics: Option<ConfidenceMetrics>,
    quality_admission_witness: Option<QualityAdmissionWitness>,
    candidates: Vec<CandidateSummary>,
    candidate_refusals: Vec<CandidateRefusal>,
    transaction_inventory: Option<TransactionInventory>,
    selected_hypothesis_id: Option<String>,
    selected_boundary_bindings: Vec<vice_verify::BoundaryBinding>,
    candidate_bytes: u64,
    runtime_stages: RuntimeStageSummary,
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
        delivery_policy_sha256: config.delivery_policy_sha256(),
        calibration: config.confidence.clone(),
        evidence: parts.evidence,
        topology: parts.topology,
        fit_diagnostics: parts.fit_diagnostics,
        fits: parts.fits,
        beam: parts.beam,
        search_mass: parts.search_mass,
        confidence_metrics: parts.confidence_metrics,
        quality_admission_witness: parts.quality_admission_witness,
        runtime: RuntimeSummary {
            elapsed_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
            candidates_scored: parts.candidates.len() as u64,
            candidate_bytes: parts.candidate_bytes,
            stages: parts.runtime_stages,
        },
        candidates: parts.candidates,
        candidate_refusals: parts.candidate_refusals,
        transaction_inventory: parts.transaction_inventory,
        selected_hypothesis_id: parts.selected_hypothesis_id,
        selected_boundary_bindings: parts.selected_boundary_bindings,
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
struct FinalSceneVariant {
    class: String,
    models: Vec<vice_fit::BoundaryModel>,
    model_transactions: Vec<CandidateModelTransaction>,
}

#[derive(Debug, Clone)]
struct FittedTopologyArm {
    arm: TopologyArm,
    fits: Vec<vice_fit::ModelRun>,
    baseline_models: Vec<vice_fit::BoundaryModel>,
    variants: Vec<FinalSceneVariant>,
}

mod variants;
use variants::{final_scene_variants, free_model, retain_variant_diversity};

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
    vectorize_with_admission_witness(bytes, request, config, None, false)
}

/// Load a digest-pinned production configuration and execute the production
/// path. A missing, tampered, stale, or pre-freeze configuration becomes a
/// typed `failed` report; it never falls back to development thresholds.
pub fn vectorize_with_production_config(
    bytes: &[u8],
    request: &VectorizeRequest,
    path: &std::path::Path,
) -> VectorizeOutcome {
    match CoreConfig::load_production_for(request.preset, path) {
        Ok(config) => vectorize_with_admission_witness(bytes, request, &config, None, false),
        Err(error) => {
            let config = CoreConfig::development_for(request.preset);
            // Decode failures are faults in the input, independent of release
            // configuration availability. Preserve that typed outcome while
            // still making every decodable input fail closed on the config.
            if vice_image::CanonicalImage::decode_png(bytes, &vice_image::DecodeLimits::default())
                .is_err()
            {
                return vectorize_impl(bytes, request, &config, None, false, None);
            }
            refuse(
                DecisionStatus::Failed,
                FailureReason::Internal {
                    detail: format!("production configuration refused: {error}"),
                },
                request,
                &config,
                digest(bytes),
                false,
                ReportParts::default(),
                Instant::now(),
            )
        }
    }
}

/// Run the exact production candidate path but retain the selected canonical
/// witness for the held-out GT court. The outcome remains non-success under
/// a development config; this API cannot set the private production seal.
pub fn vectorize_for_calibration(
    bytes: &[u8],
    request: &VectorizeRequest,
    config: &CoreConfig,
) -> CalibrationRun {
    vectorize_for_calibration_impl(bytes, request, config, true)
}

/// Retain the selected court witness without constructing the paired baseline.
/// Calibration thresholds do not consume baseline evidence; the sealed release
/// measurement uses `vectorize_for_calibration` and remains fully paired.
pub fn vectorize_for_calibration_without_baseline(
    bytes: &[u8],
    request: &VectorizeRequest,
    config: &CoreConfig,
) -> CalibrationRun {
    vectorize_for_calibration_impl(bytes, request, config, false)
}

fn vectorize_for_calibration_impl(
    bytes: &[u8],
    request: &VectorizeRequest,
    config: &CoreConfig,
    capture_baseline: bool,
) -> CalibrationRun {
    let mut selected = None;
    let mut baseline = None;
    let mut baseline_refusals = Vec::new();
    let witness = |candidate: &crate::candidate::MaterializedCandidate| CalibrationWitness {
        candidate: candidate.summary.clone(),
        scene_json: candidate.scene_json.clone(),
        export_plan_json: candidate.plan_json.clone(),
        pure_partition_svg: candidate.pure_svg.clone(),
        seam_safe_svg: candidate.seam_svg.clone(),
        rendered_png: candidate.render_png.clone(),
        seal_json: candidate.seal_json.clone(),
    };
    let mut capture = |candidate: &crate::candidate::MaterializedCandidate,
                       baseline_candidate: Option<&crate::candidate::MaterializedCandidate>,
                       refusals: &[CandidateRefusal]| {
        selected = Some(witness(candidate));
        baseline = baseline_candidate.map(witness);
        baseline_refusals = refusals.to_vec();
    };
    let outcome = vectorize_with_admission_witness(
        bytes,
        request,
        config,
        Some(&mut capture),
        capture_baseline,
    );
    CalibrationRun {
        outcome,
        selected,
        baseline,
        baseline_refusals,
    }
}

fn vectorize_with_admission_witness(
    bytes: &[u8],
    request: &VectorizeRequest,
    config: &CoreConfig,
    calibration_observer: Option<&mut CalibrationObserver<'_>>,
    capture_baseline: bool,
) -> VectorizeOutcome {
    if request.preset != config.preset() {
        return refuse(
            DecisionStatus::Failed,
            FailureReason::Internal {
                detail: format!(
                    "request preset {:?} does not match supplied {:?} core configuration",
                    request.preset,
                    config.preset()
                ),
            },
            request,
            config,
            digest(bytes),
            false,
            ReportParts::default(),
            Instant::now(),
        );
    }
    if request.preset != Preset::Quality || !config.requires_fast_admission_witness() {
        return vectorize_impl(
            bytes,
            request,
            config,
            calibration_observer,
            capture_baseline,
            None,
        );
    }

    let started = Instant::now();
    let mut witness_request = request.clone();
    witness_request.preset = Preset::Fast;
    let witness_config = CoreConfig::development_for(Preset::Fast);
    let mut selected_witness = None;
    let mut capture = |candidate: &crate::candidate::MaterializedCandidate,
                       _: Option<&crate::candidate::MaterializedCandidate>,
                       _: &[CandidateRefusal]| {
        selected_witness = Some(candidate.summary.clone());
    };
    let witness_outcome = vectorize_impl(
        bytes,
        &witness_request,
        &witness_config,
        Some(&mut capture),
        false,
        None,
    );
    let prefix_elapsed_ms = started.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
    let witness_report = witness_outcome.report();
    let witness_runtime = witness_report.runtime.clone();
    let source_sha256 = witness_report
        .source_sha256
        .clone()
        .unwrap_or_else(|| digest(bytes));
    let certificate = QualityAdmissionWitness {
        schema: "vice-classic/m7-quality-fast-admission-witness/v1",
        source_sha256: source_sha256.clone(),
        intent: request.intent,
        witness_preset: Preset::Fast,
        witness_identity: witness_config.identity(),
        decision_status: witness_report.status,
        decision_reason: witness_report.reason.clone(),
        candidate_available: selected_witness.is_some(),
        selected_hypothesis_id: selected_witness
            .as_ref()
            .map(|candidate| candidate.hypothesis_id.clone()),
        selected_scene_digest_sha256: selected_witness
            .as_ref()
            .map(|candidate| candidate.scene_digest_sha256.clone()),
        selected_delivery_digest: selected_witness
            .as_ref()
            .map(|candidate| candidate.delivery_digest.clone()),
        runtime: witness_runtime.clone(),
    };

    if selected_witness.is_none() {
        let preserve_input_or_fault = matches!(
            witness_report.reason,
            Some(
                FailureReason::Decode { .. }
                    | FailureReason::Evidence { .. }
                    | FailureReason::FormationOutsideUniverse { .. }
                    | FailureReason::BoundaryOutsideSelectiveCore { .. }
                    | FailureReason::Internal { .. }
            )
        );
        let status = if preserve_input_or_fault {
            witness_report.status
        } else {
            DecisionStatus::Ambiguous
        };
        let reason = if preserve_input_or_fault {
            witness_report
                .reason
                .clone()
                .unwrap_or_else(|| FailureReason::Internal {
                    detail: "Fast admission witness stopped without a typed reason".into(),
                })
        } else {
            FailureReason::BoundaryOutsideSelectiveCore {
                detail: format!(
                    "Quality delivery requires a verified protected Fast-lane witness on the \
                     same source and intent; the witness lane returned {:?}: {}",
                    witness_report.status,
                    witness_report.reason.as_ref().map_or_else(
                        || "no typed reason".to_string(),
                        |reason| format!("{reason:?}")
                    )
                ),
            }
        };
        let production = request.production
            && config.is_sealed_production()
            && !request.research_override
            && request.milestone_debug.is_none()
            && request.oracle_override.is_none()
            && witness_report
                .evidence
                .as_ref()
                .is_none_or(|evidence| evidence.production);
        let mut parts = ReportParts {
            evidence: witness_report.evidence.clone(),
            quality_admission_witness: Some(certificate),
            ..ReportParts::default()
        };
        parts.runtime_stages.quality_admission_witness_ms = prefix_elapsed_ms;
        return refuse(
            status,
            reason,
            request,
            config,
            source_sha256,
            production,
            parts,
            started,
        );
    }

    let mut outcome = vectorize_impl(
        bytes,
        request,
        config,
        calibration_observer,
        capture_baseline,
        Some(certificate),
    );
    let report = match &mut outcome {
        VectorizeOutcome::Success(success) => &mut success.report,
        VectorizeOutcome::Ambiguous(report)
        | VectorizeOutcome::Unsupported(report)
        | VectorizeOutcome::Failed(report) => report,
    };
    report.runtime.elapsed_ms = report.runtime.elapsed_ms.saturating_add(prefix_elapsed_ms);
    report.runtime.stages.quality_admission_witness_ms = report
        .runtime
        .stages
        .quality_admission_witness_ms
        .saturating_add(prefix_elapsed_ms);
    report.runtime.candidates_scored = report
        .runtime
        .candidates_scored
        .saturating_add(witness_runtime.candidates_scored);
    report.runtime.candidate_bytes = report
        .runtime
        .candidate_bytes
        .max(witness_runtime.candidate_bytes);
    if let VectorizeOutcome::Success(success) = &mut outcome {
        success.artifacts.report_json =
            serde_json::to_vec(&success.report).expect("report serializes");
    }
    outcome
}

mod run;
use run::vectorize_impl;
