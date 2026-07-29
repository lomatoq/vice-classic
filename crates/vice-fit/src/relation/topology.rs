//! Topology-preserving selection at the public Stage-H application boundary.

use crate::models::{BoundaryModel, SelectedBoundaryGeometry};
use crate::refit::{RefitChain, RefitSegment};

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

fn finite_nonnegative(value: f64) -> bool {
    value.is_finite() && value >= 0.0
}

fn same_segment_family(a: &RefitSegment, b: &RefitSegment) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
}

fn safe_application(
    model: &BoundaryModel,
    free_chain: &RefitChain,
    hypothesis: &RelationHypothesis,
    closed: bool,
) -> Option<(f64, crate::ChainCode)> {
    if !hypothesis.accepted
        || !closure_matches(&hypothesis.constrained_chain, closed)
        || free_chain.lower().is_err()
        || hypothesis.constrained_chain.lower().is_err()
        || free_chain.nodes.len() != hypothesis.constrained_chain.nodes.len()
        || free_chain.segments.len() != hypothesis.constrained_chain.segments.len()
        || !free_chain
            .segments
            .iter()
            .zip(&hypothesis.constrained_chain.segments)
            .all(|(a, b)| same_segment_family(a, b))
        || hypothesis.segments.is_empty()
        || !hypothesis
            .segments
            .iter()
            .all(|&segment| segment < free_chain.segments.len())
        || !hypothesis.segments.windows(2).all(|pair| pair[0] < pair[1])
    {
        return None;
    }

    let published = [
        hypothesis.cost_bits,
        hypothesis.saving_bits,
        hypothesis.geometry_saving_bits,
        hypothesis.topology_saving_bits,
        hypothesis.worst_normal_deviation_px,
        hypothesis.worst_model_to_evidence_px,
        hypothesis.allowed_px,
    ];
    let code_before = [
        model.code.geometry_bits,
        model.code.topology_bits,
        model.code.relation_bits,
        model.code.residual_bits,
    ];
    if !published.into_iter().all(finite_nonnegative)
        || !code_before.into_iter().all(finite_nonnegative)
        || !hypothesis.residual_penalty_bits.is_finite()
        || !hypothesis.net_bits.is_finite()
    {
        return None;
    }

    let saving = hypothesis.geometry_saving_bits + hypothesis.topology_saving_bits;
    let net = saving - hypothesis.cost_bits - hypothesis.residual_penalty_bits;
    if !saving.is_finite()
        || !net.is_finite()
        || saving != hypothesis.saving_bits
        || net != hypothesis.net_bits
        || net <= 0.0
        || hypothesis.worst_normal_deviation_px > hypothesis.allowed_px
    {
        return None;
    }

    let code = crate::ChainCode {
        geometry_bits: model.code.geometry_bits - hypothesis.geometry_saving_bits,
        topology_bits: model.code.topology_bits - hypothesis.topology_saving_bits,
        relation_bits: model.code.relation_bits + hypothesis.cost_bits,
        residual_bits: model.code.residual_bits + hypothesis.residual_penalty_bits,
    };
    let after = [
        code.geometry_bits,
        code.topology_bits,
        code.relation_bits,
        code.residual_bits,
        code.total_bits(),
    ];
    after
        .into_iter()
        .all(finite_nonnegative)
        .then_some((net, code))
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
        .filter_map(|(index, hypothesis)| {
            safe_application(model, free_chain, hypothesis, closed)
                .map(|(net_bits, code)| (index, net_bits, code))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1));
    let Some((index, _, code)) = best else {
        model.relation_kept_indices.clear();
        return 0;
    };
    let hypothesis = &hypotheses[index];
    model.code = code;
    model.geometry = SelectedBoundaryGeometry::TypedChain {
        chain: hypothesis.constrained_chain.clone(),
    };
    model.worst_normal_deviation_px = hypothesis.worst_normal_deviation_px;
    model.worst_model_to_evidence_px = hypothesis.worst_model_to_evidence_px;
    model.relation_kept_indices = vec![index];
    1
}
