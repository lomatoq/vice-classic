//! Topology-preserving selection at the public Stage-H application boundary.

use crate::models::{BoundaryModel, SelectedBoundaryGeometry};
use crate::refit::RefitChain;

use super::RelationHypothesis;

/// Whether the shared-parameter chain represents the topology declared by the
/// observation. Equality is exact because a closed chain repeats one node; a
/// relation is not allowed to turn that alias into a near miss.
pub(super) fn closure_matches(chain: &RefitChain, closed: bool) -> bool {
    chain.nodes.len() == chain.segments.len() + 1
        && !chain.segments.is_empty()
        && (chain.nodes.first().map(|node| node.pos) == chain.nodes.last().map(|node| node.pos))
            == closed
}

/// Apply the shortest accepted relation that preserves the observation's
/// declared open/closed topology.
pub fn apply_accepted(
    model: &mut BoundaryModel,
    hypotheses: &[RelationHypothesis],
    closed: bool,
) -> usize {
    let Some(free_chain) = model.geometry.typed_chain() else {
        model.relation_kept_indices.clear();
        return 0;
    };
    if !closure_matches(free_chain, closed) {
        model.relation_kept_indices.clear();
        return 0;
    }
    let best = hypotheses
        .iter()
        .enumerate()
        .filter_map(|(i, hypothesis)| {
            (hypothesis.accepted && closure_matches(&hypothesis.constrained_chain, closed))
                .then_some(i)
        })
        .max_by(|&a, &b| hypotheses[a].net_bits.total_cmp(&hypotheses[b].net_bits));
    let Some(index) = best else {
        model.relation_kept_indices.clear();
        return 0;
    };
    let hypothesis = &hypotheses[index];
    model.code.relation_bits += hypothesis.cost_bits;
    model.code.geometry_bits -= hypothesis.geometry_saving_bits;
    model.code.topology_bits -= hypothesis.topology_saving_bits;
    model.code.residual_bits += hypothesis.residual_penalty_bits;
    model.geometry = SelectedBoundaryGeometry::TypedChain {
        chain: hypothesis.constrained_chain.clone(),
    };
    model.worst_normal_deviation_px = hypothesis.worst_normal_deviation_px;
    model.worst_model_to_evidence_px = hypothesis.worst_model_to_evidence_px;
    model.relation_kept_indices = vec![index];
    1
}
