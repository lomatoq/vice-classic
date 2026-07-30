use serde::Serialize;

use super::*;

pub(super) fn build_trace(
    request: &VectorizeRequest,
    selected: &crate::candidate::MaterializedCandidate,
    candidates: &[crate::candidate::MaterializedCandidate],
    candidate_refusals: &[CandidateRefusal],
) -> Option<Vec<u8>> {
    if !request.trace && request.dump_candidates == 0 {
        return None;
    }
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
            candidate_refusals,
        })
        .expect("trace serializes"),
    )
}
