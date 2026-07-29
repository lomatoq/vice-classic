//! Non-production differential control for the no-BIC gate.

use serde::Serialize;
use vice_evidence::BoundarySample;

use super::{GrammarEdge, PathObjective};

/// A path ranked by the same admissible grammar's non-negative §14.4 proposal
/// integral alone. It cannot be injected into the production model API.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProposalControlPath {
    pub candidates: Vec<usize>,
    pub breakpoints: Vec<usize>,
    pub smooth: Vec<bool>,
    pub residual_cost_px: f64,
}

/// Remove every geometry/topology code term and rank by a finite non-negative
/// proposal residual. This is the explicit no-BIC knockout, not a code length.
pub fn k_best_proposal_control_paths(
    edges: &[GrammarEdge],
    samples: &[BoundarySample],
    k: usize,
) -> Vec<ProposalControlPath> {
    super::k_best_paths_for_objective(
        edges,
        samples,
        &crate::GEOMETRY_CODE_TABLE_V1,
        crate::REFERENCE_CANVAS_DIM_PX,
        k,
        (PathObjective::ProposalResidual, super::ClosureMode::Open),
        0.0,
    )
    .into_iter()
    .map(|path| ProposalControlPath {
        candidates: path.candidates,
        breakpoints: path.breakpoints,
        smooth: path.smooth,
        residual_cost_px: path.proposal_cost_px,
    })
    .collect()
}
