use super::*;

macro_rules! refuse_input {
    ($($argument:tt)*) => {
        return Err(Box::new(refuse($($argument)*)))
    };
}

pub(super) struct PreparedInput {
    pub(super) started: Instant,
    pub(super) source_sha256: String,
    pub(super) production: bool,
    pub(super) image: vice_image::CanonicalImage,
    pub(super) evidence: vice_evidence::Flat2Evidence,
    pub(super) formations: Vec<vice_ir::GlobalFormationHypothesis>,
    pub(super) parts: ReportParts,
}

pub(super) fn prepare_input(
    bytes: &[u8],
    request: &VectorizeRequest,
    config: &CoreConfig,
) -> Result<PreparedInput, Box<VectorizeOutcome>> {
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
                refuse_input!(
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
        refuse_input!(
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

    let analysis = vice_evidence::analyze_full_for_filters(
        &image,
        &vice_evidence::ANALYSIS_CONFIG_V1,
        request.oracle_override.clone(),
        &[vice_ir::PixelFilter::Box],
    );
    let production = provenance_production && analysis.report.production;
    let parts = ReportParts {
        evidence: Some(analysis.report.clone()),
        ..ReportParts::default()
    };
    match &analysis.report.outcome {
        Flat2Outcome::Ambiguous { note, .. } => {
            refuse_input!(
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
            refuse_input!(
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
        refuse_input!(
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
            refuse_input!(
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
        refuse_input!(
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
    if boundary_observation.chains.is_empty()
        || boundary_observation
            .chains
            .iter()
            .any(|chain| !chain.closed)
    {
        refuse_input!(
            DecisionStatus::Ambiguous,
            FailureReason::BoundaryOutsideSelectiveCore {
                detail: "M7 selective core requires one or more closed boundaries; critical \
                         saddle readings remain explicit M4.5 topology hypotheses"
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
    Ok(PreparedInput {
        started,
        source_sha256,
        production,
        image,
        evidence,
        formations,
        parts,
    })
}
