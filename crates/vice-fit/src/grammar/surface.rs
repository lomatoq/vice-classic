//! Validation for caller-constructible grammar inputs and the small public
//! scalar helpers those inputs use.

use vice_evidence::BoundarySample;

use crate::span::{SpanCandidate, SpanFamily};

use super::{GrammarEdge, JET_CLASSES};

/// Directions a candidate leaves its first sample with and arrives at its last
/// sample with, in radians.
pub fn candidate_jets(candidate: &SpanCandidate, samples: &[BoundarySample]) -> Option<(f64, f64)> {
    if candidate.support.hi() >= samples.len() {
        return None;
    }
    let p0 = samples[candidate.support.lo()].p;
    let p1 = samples[candidate.support.hi()].p;
    let (poly, _) = crate::span::flatten(&candidate.segment, p0, p1)?;
    if poly.len() < 2 {
        return None;
    }
    let entry = poly[1] - poly[0];
    let exit = poly[poly.len() - 1] - poly[poly.len() - 2];
    (entry.length_sq() > 0.0 && exit.length_sq() > 0.0)
        .then(|| (entry.y.atan2(entry.x), exit.y.atan2(exit.x)))
}

/// Scalars a family still has to code, given which of its ends read a shared
/// tangent.
pub fn free_scalars(family: SpanFamily, head_shared: bool, tail_shared: bool) -> usize {
    let shared = usize::from(head_shared) + usize::from(tail_shared);
    match family {
        SpanFamily::Line => 0,
        SpanFamily::CircularArc => usize::from(shared == 0),
        SpanFamily::Quad => 2 - shared.min(2),
        SpanFamily::Cubic => 4 - shared.min(2),
    }
}

pub(super) fn validate_candidate(
    candidate: &SpanCandidate,
    index: usize,
    samples: &[BoundarySample],
) -> Result<(), crate::FitRefusal> {
    if candidate.support.hi() >= samples.len() {
        return Err(crate::FitRefusal::CandidateSupportOutOfRange {
            candidate: index,
            lo: candidate.support.lo(),
            hi: candidate.support.hi(),
            samples: samples.len(),
        });
    }
    if !candidate.proposal_cost_px().is_finite() || candidate.proposal_cost_px() < 0.0 {
        return Err(crate::FitRefusal::InvalidCandidateCost {
            candidate: index,
            proposal_cost_px: candidate.proposal_cost_px(),
        });
    }
    let family_matches = matches!(
        (candidate.family, &candidate.segment),
        (SpanFamily::Line, vice_ir::Segment::Line)
            | (
                SpanFamily::CircularArc,
                vice_ir::Segment::CircularArc { .. }
            )
            | (SpanFamily::Quad, vice_ir::Segment::Quad { .. })
            | (SpanFamily::Cubic, vice_ir::Segment::Cubic { .. })
    );
    if !family_matches {
        return Err(crate::FitRefusal::CandidateFamilyMismatch { candidate: index });
    }
    Ok(())
}

pub(super) fn validate_grammar_edges(
    edges: &[GrammarEdge],
    samples: &[BoundarySample],
) -> Result<(), crate::FitRefusal> {
    let max_path_edges = samples.len().saturating_sub(1).max(1) as f64;
    // Leave half the finite range for first-sample and model-code terms.
    let max_edge_cost = f64::MAX / (2.0 * max_path_edges);
    for (index, edge) in edges.iter().enumerate() {
        if edge.from >= edge.to || edge.to >= samples.len() {
            return Err(crate::FitRefusal::InvalidGrammarEdgeTopology {
                edge: index,
                from: edge.from,
                to: edge.to,
                samples: samples.len(),
            });
        }
        if edge.entry_class >= JET_CLASSES
            || edge.exit_class >= JET_CLASSES
            || !edge.entry_rad.is_finite()
            || !edge.exit_rad.is_finite()
        {
            return Err(crate::FitRefusal::InvalidGrammarEdgeJet {
                edge: index,
                entry_class: edge.entry_class,
                exit_class: edge.exit_class,
                entry_rad: edge.entry_rad,
                exit_rad: edge.exit_rad,
            });
        }
        if !edge.residual_bits.is_finite()
            || edge.residual_bits < 0.0
            || edge.residual_bits > max_edge_cost
            || !edge.proposal_cost_px.is_finite()
            || edge.proposal_cost_px < 0.0
            || edge.proposal_cost_px > max_edge_cost
        {
            return Err(crate::FitRefusal::InvalidGrammarEdgeCost {
                edge: index,
                residual_bits: edge.residual_bits,
                proposal_cost_px: edge.proposal_cost_px,
            });
        }
    }
    Ok(())
}
