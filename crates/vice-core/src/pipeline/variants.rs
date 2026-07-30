use super::*;

pub(super) fn free_model(selected: &vice_fit::BoundaryModel) -> vice_fit::BoundaryModel {
    let mut free = selected.clone();
    free.geometry = selected.stage_h_free_geometry.clone();
    free.code = selected.stage_h_free_code;
    free.primitive_kept = None;
    free.relations_kept = 0;
    free.relation_kept_indices.clear();
    free
}

fn path_transaction_kinds(
    parent: &vice_fit::BoundaryModel,
    target: &vice_fit::BoundaryModel,
) -> Vec<TransactionKind> {
    let (
        vice_fit::SelectedBoundaryGeometry::TypedChain {
            chain: parent_chain,
        },
        vice_fit::SelectedBoundaryGeometry::TypedChain {
            chain: target_chain,
        },
    ) = (&parent.geometry, &target.geometry)
    else {
        return vec![TransactionKind::JointEscape];
    };
    let mut kinds = Vec::new();
    match target_chain
        .segments
        .len()
        .cmp(&parent_chain.segments.len())
    {
        std::cmp::Ordering::Greater => {
            kinds.push(TransactionKind::AnchorInsert);
            kinds.push(TransactionKind::SpanSplitJointRefit);
        }
        std::cmp::Ordering::Less => {
            kinds.push(TransactionKind::AnchorRemove);
            kinds.push(TransactionKind::SpanMergeJointRefit);
        }
        std::cmp::Ordering::Equal => {}
    }
    let parent_families: Vec<_> = parent_chain
        .segments
        .iter()
        .map(std::mem::discriminant)
        .collect();
    let target_families: Vec<_> = target_chain
        .segments
        .iter()
        .map(std::mem::discriminant)
        .collect();
    if parent_families != target_families {
        kinds.push(TransactionKind::FamilyChange);
    }
    let corners = |chain: &vice_fit::RefitChain| {
        chain
            .nodes
            .iter()
            .filter(|node| node.tangent_rad.is_none())
            .count()
    };
    match corners(target_chain).cmp(&corners(parent_chain)) {
        std::cmp::Ordering::Greater => kinds.push(TransactionKind::CornerActivate),
        std::cmp::Ordering::Less => kinds.push(TransactionKind::CornerDeactivate),
        std::cmp::Ordering::Equal => {}
    }
    if kinds.is_empty() {
        kinds.push(TransactionKind::JointEscape);
    }
    kinds.sort();
    kinds.dedup();
    kinds
}

fn inverse_transaction_kind(kind: TransactionKind) -> TransactionKind {
    match kind {
        TransactionKind::AnchorInsert => TransactionKind::AnchorRemove,
        TransactionKind::AnchorRemove => TransactionKind::AnchorInsert,
        TransactionKind::SpanSplitJointRefit => TransactionKind::SpanMergeJointRefit,
        TransactionKind::SpanMergeJointRefit => TransactionKind::SpanSplitJointRefit,
        TransactionKind::CornerActivate => TransactionKind::CornerDeactivate,
        TransactionKind::CornerDeactivate => TransactionKind::CornerActivate,
        TransactionKind::PrimitivePromote => TransactionKind::PrimitiveDemote,
        TransactionKind::PrimitiveDemote => TransactionKind::PrimitivePromote,
        TransactionKind::RelationPromote => TransactionKind::RelationDemote,
        TransactionKind::RelationDemote => TransactionKind::RelationPromote,
        other => other,
    }
}

fn repeated_scene_sibling(
    left: &vice_fit::BoundaryModel,
    right: &vice_fit::BoundaryModel,
    right_chain: &vice_evidence::BoundaryChain,
    canvas_dim_px: f64,
    scene_boundaries: usize,
) -> Option<vice_fit::BoundaryModel> {
    let left_chain = left.stage_h_free_geometry.typed_chain()?;
    let right_free = right.stage_h_free_geometry.typed_chain()?;
    if left_chain.nodes.len() != right_free.nodes.len()
        || left_chain.segments.len() != right_free.segments.len()
        || left_chain
            .segments
            .iter()
            .zip(&right_free.segments)
            .any(|(left, right)| std::mem::discriminant(left) != std::mem::discriminant(right))
    {
        return None;
    }
    let closed = left_chain.start() == left_chain.end() && right_free.start() == right_free.end();
    let unique_nodes = left_chain.nodes.len().saturating_sub(usize::from(closed));
    if unique_nodes < 2 {
        return None;
    }
    let delta = left_chain
        .nodes
        .iter()
        .zip(&right_free.nodes)
        .take(unique_nodes)
        .fold(vice_geom::Pt::ZERO, |sum, (left, right)| {
            sum + (right.pos - left.pos)
        })
        * (1.0 / unique_nodes as f64);
    let mut constrained = left_chain.clone();
    for node in &mut constrained.nodes {
        node.pos += delta;
    }
    if closed {
        let first = constrained.nodes[0].pos;
        let last = constrained.nodes.len() - 1;
        constrained.nodes[last].pos = first;
    }
    let polyline = vice_fit::solve::flatten_chain(&constrained).ok()?;
    let forward = vice_fit::solve::evidence_to_model_corridor(&polyline, &right_chain.samples);
    let reverse = vice_fit::solve::model_to_evidence_corridor(&polyline, &right_chain.samples);
    if !forward.feasible() || !reverse.feasible() {
        return None;
    }
    let table = vice_fit::GEOMETRY_CODE_TABLE_V1;
    let pair_bits = vice_fit::log2_binomial(scene_boundaries, 2);
    let relation_cost = table.bits_per_relation() + pair_bits;
    let saving = (2.0 * table.coordinate_bits(canvas_dim_px)).min(right.code.topology_bits);
    let mut sibling = right.clone();
    sibling.geometry = vice_fit::SelectedBoundaryGeometry::TypedChain { chain: constrained };
    sibling.code.topology_bits -= saving;
    sibling.code.relation_bits += relation_cost;
    sibling.relations_kept += 1;
    sibling.primitive_kept = None;
    sibling.worst_normal_deviation_px = forward.deviation_px;
    sibling.worst_model_to_evidence_px = reverse.deviation_px;
    Some(sibling)
}

fn mirrored_scene_sibling(
    left: &vice_fit::BoundaryModel,
    right: &vice_fit::BoundaryModel,
    left_observation: &vice_evidence::BoundaryChain,
    right_chain: &vice_evidence::BoundaryChain,
    canvas_dim_px: f64,
    scene_boundaries: usize,
) -> Option<vice_fit::BoundaryModel> {
    let left_chain = left.stage_h_free_geometry.typed_chain()?;
    let right_free = right.stage_h_free_geometry.typed_chain()?;
    let segments = left_chain.segments.len();
    if segments == 0
        || left_chain.nodes.len() != segments + 1
        || left_chain.start() != left_chain.end()
        || right_free.start() != right_free.end()
    {
        return None;
    }
    let lowered = left_chain.lower().ok()?;
    let center = |observation: &vice_evidence::BoundaryChain| {
        let weight = observation
            .samples
            .iter()
            .map(|sample| sample.weight_ds)
            .sum::<f64>();
        (weight.is_finite() && weight > 0.0).then(|| {
            observation
                .samples
                .iter()
                .fold(vice_geom::Pt::ZERO, |sum, sample| {
                    sum + sample.p * sample.weight_ds
                })
                * (1.0 / weight)
        })
    };
    let left_center = center(left_observation)?;
    let right_center = center(right_chain)?;
    let center_delta = right_center - left_center;
    let center_distance = center_delta.length();
    if !(center_distance.is_finite() && center_distance > 1e-9) {
        return None;
    }
    let normal = center_delta * (1.0 / center_distance);
    let midpoint = (left_center + right_center) * 0.5;
    let reflect = |point: vice_geom::Pt| point - normal * (2.0 * (point - midpoint).dot(normal));
    let mut best: Option<(f64, usize)> = None;
    for shift in 0..segments {
        let error = (reflect(left_chain.nodes[shift].pos) - right_free.nodes[0].pos).length_sq();
        if error.is_finite()
            && best.as_ref().is_none_or(|(best_error, best_shift)| {
                error < *best_error || (error == *best_error && shift < *best_shift)
            })
        {
            best = Some((error, shift));
        }
    }
    let (_, shift) = best?;
    let mut constrained = vice_fit::RefitChain {
        nodes: Vec::with_capacity(segments + 1),
        segments: Vec::with_capacity(segments),
    };
    for index in 0..segments {
        let source = (shift + segments - index) % segments;
        constrained.nodes.push(vice_fit::RefitNode {
            pos: reflect(left_chain.nodes[source].pos),
            tangent_rad: None,
        });
        let source_segment = (source + segments - 1) % segments;
        constrained
            .segments
            .push(match lowered.segments[source_segment].clone() {
                vice_ir::Segment::Line => vice_fit::RefitSegment::Line,
                vice_ir::Segment::CircularArc {
                    radius_px,
                    large_arc,
                    ccw,
                } => vice_fit::RefitSegment::Arc(vice_fit::ArcAnchor::Radius {
                    radius_px,
                    large_arc,
                    // Reversing traversal and reflecting each flip sweep, so the
                    // two orientation changes cancel.
                    ccw,
                }),
                vice_ir::Segment::Quad { ctrl } => vice_fit::RefitSegment::Quad {
                    ctrl: vice_fit::Handle::Free(reflect(ctrl)),
                },
                vice_ir::Segment::Cubic { ctrl1, ctrl2 } => vice_fit::RefitSegment::Cubic {
                    head: vice_fit::Handle::Free(reflect(ctrl2)),
                    tail: vice_fit::Handle::Free(reflect(ctrl1)),
                },
                vice_ir::Segment::EllipticArc { .. } => return None,
            });
    }
    constrained.nodes.push(constrained.nodes[0]);
    let polyline = vice_fit::solve::flatten_chain(&constrained).ok()?;
    let forward = vice_fit::solve::evidence_to_model_corridor(&polyline, &right_chain.samples);
    let reverse = vice_fit::solve::model_to_evidence_corridor(&polyline, &right_chain.samples);
    if !forward.feasible() || !reverse.feasible() {
        return None;
    }
    let table = vice_fit::GEOMETRY_CODE_TABLE_V1;
    let relation_cost = table.bits_per_relation() + vice_fit::log2_binomial(scene_boundaries, 2);
    let saving = (2.0 * table.coordinate_bits(canvas_dim_px)).min(right.code.topology_bits);
    let mut sibling = right.clone();
    sibling.geometry = vice_fit::SelectedBoundaryGeometry::TypedChain { chain: constrained };
    sibling.families = sibling
        .geometry
        .typed_chain()?
        .segments
        .iter()
        .map(|segment| match segment {
            vice_fit::RefitSegment::Line => vice_fit::span::SpanFamily::Line,
            vice_fit::RefitSegment::Arc(_) => vice_fit::span::SpanFamily::CircularArc,
            vice_fit::RefitSegment::Quad { .. } => vice_fit::span::SpanFamily::Quad,
            vice_fit::RefitSegment::Cubic { .. } => vice_fit::span::SpanFamily::Cubic,
        })
        .collect();
    sibling.code.topology_bits -= saving;
    sibling.code.relation_bits += relation_cost;
    sibling.relations_kept += 1;
    sibling.primitive_kept = None;
    sibling.worst_normal_deviation_px = forward.deviation_px;
    sibling.worst_model_to_evidence_px = reverse.deviation_px;
    Some(sibling)
}

pub(super) fn final_scene_variants(
    fits: &[vice_fit::ModelRun],
    chains: &[vice_evidence::BoundaryChain],
    canvas_dim_px: f64,
) -> Vec<FinalSceneVariant> {
    let baseline: Vec<_> = fits.iter().map(|fit| free_model(&fit.models[0])).collect();
    let mut variants = vec![FinalSceneVariant {
        class: "baseline-free".into(),
        models: baseline.clone(),
        model_transactions: Vec::new(),
    }];
    for (chain_index, fit) in fits.iter().enumerate() {
        for (path_index, selected) in fit.models.iter().enumerate() {
            let free = free_model(selected);
            let mut path_models = baseline.clone();
            path_models[chain_index] = free.clone();
            if path_index != 0 {
                for kind in path_transaction_kinds(&baseline[chain_index], &free) {
                    variants.push(FinalSceneVariant {
                        class: format!("c{chain_index}-path{path_index}-{kind:?}").to_lowercase(),
                        models: path_models.clone(),
                        model_transactions: vec![CandidateModelTransaction {
                            kind,
                            parent_models: baseline.clone(),
                        }],
                    });
                    variants.push(FinalSceneVariant {
                        class: format!("c{chain_index}-path{path_index}-{kind:?}-reverse")
                            .to_lowercase(),
                        models: baseline.clone(),
                        model_transactions: vec![CandidateModelTransaction {
                            kind: inverse_transaction_kind(kind),
                            parent_models: path_models.clone(),
                        }],
                    });
                }
            }
            for (index, hypothesis) in selected.relations.iter().enumerate() {
                let mut sibling = free.clone();
                if vice_fit::apply_relation_sibling(&mut sibling, hypothesis, index, true) {
                    let mut models = path_models.clone();
                    models[chain_index] = sibling;
                    variants.push(FinalSceneVariant {
                        class: format!(
                            "c{chain_index}-path{path_index}-relation-{index}-{:?}",
                            hypothesis.kind
                        )
                        .to_lowercase(),
                        models: models.clone(),
                        model_transactions: vec![CandidateModelTransaction {
                            kind: TransactionKind::RelationPromote,
                            parent_models: path_models.clone(),
                        }],
                    });
                    variants.push(FinalSceneVariant {
                        class: format!(
                            "c{chain_index}-path{path_index}-relation-{index}-{:?}-demote",
                            hypothesis.kind
                        )
                        .to_lowercase(),
                        models: path_models.clone(),
                        model_transactions: vec![CandidateModelTransaction {
                            kind: TransactionKind::RelationDemote,
                            parent_models: models,
                        }],
                    });
                }
            }
            for (index, hypothesis) in selected.primitives.iter().enumerate() {
                let mut sibling = free.clone();
                if vice_fit::apply_primitive_sibling(&mut sibling, hypothesis, index) {
                    let mut models = path_models.clone();
                    models[chain_index] = sibling;
                    variants.push(FinalSceneVariant {
                        class: format!(
                            "c{chain_index}-path{path_index}-primitive-{index}-{:?}",
                            hypothesis.kind
                        )
                        .to_lowercase(),
                        models: models.clone(),
                        model_transactions: vec![CandidateModelTransaction {
                            kind: TransactionKind::PrimitivePromote,
                            parent_models: path_models.clone(),
                        }],
                    });
                    variants.push(FinalSceneVariant {
                        class: format!(
                            "c{chain_index}-path{path_index}-primitive-{index}-{:?}-demote",
                            hypothesis.kind
                        )
                        .to_lowercase(),
                        models: path_models.clone(),
                        model_transactions: vec![CandidateModelTransaction {
                            kind: TransactionKind::PrimitiveDemote,
                            parent_models: models,
                        }],
                    });
                }
            }
        }
    }
    for left in 0..baseline.len() {
        for right in left + 1..baseline.len() {
            if let Some(sibling) = repeated_scene_sibling(
                &baseline[left],
                &baseline[right],
                &chains[right],
                canvas_dim_px,
                baseline.len(),
            ) {
                let mut models = baseline.clone();
                models[right] = sibling;
                variants.push(FinalSceneVariant {
                    class: format!("scene-repetition-c{left}-c{right}"),
                    models: models.clone(),
                    model_transactions: vec![CandidateModelTransaction {
                        kind: TransactionKind::RelationPromote,
                        parent_models: baseline.clone(),
                    }],
                });
                variants.push(FinalSceneVariant {
                    class: format!("scene-repetition-c{left}-c{right}-demote"),
                    models: baseline.clone(),
                    model_transactions: vec![CandidateModelTransaction {
                        kind: TransactionKind::RelationDemote,
                        parent_models: models,
                    }],
                });
            }
            if let Some(sibling) = mirrored_scene_sibling(
                &baseline[left],
                &baseline[right],
                &chains[left],
                &chains[right],
                canvas_dim_px,
                baseline.len(),
            ) {
                let mut models = baseline.clone();
                models[right] = sibling;
                variants.push(FinalSceneVariant {
                    class: format!("scene-mirror-c{left}-c{right}"),
                    models: models.clone(),
                    model_transactions: vec![CandidateModelTransaction {
                        kind: TransactionKind::RelationPromote,
                        parent_models: baseline.clone(),
                    }],
                });
                variants.push(FinalSceneVariant {
                    class: format!("scene-mirror-c{left}-c{right}-demote"),
                    models: baseline.clone(),
                    model_transactions: vec![CandidateModelTransaction {
                        kind: TransactionKind::RelationDemote,
                        parent_models: models,
                    }],
                });
            }
        }
    }
    variants.sort_by(|left, right| {
        let left_bits: f64 = left
            .models
            .iter()
            .map(|model| model.code.total_bits())
            .sum();
        let right_bits: f64 = right
            .models
            .iter()
            .map(|model| model.code.total_bits())
            .sum();
        left_bits
            .total_cmp(&right_bits)
            .then_with(|| left.class.cmp(&right.class))
    });
    let mut merged: Vec<FinalSceneVariant> = Vec::new();
    for mut variant in variants {
        if let Some(existing) = merged
            .iter_mut()
            .find(|existing| existing.models == variant.models)
        {
            if variant.class == "baseline-free" {
                existing.class = variant.class;
            }
            for transaction in variant.model_transactions.drain(..) {
                if !existing.model_transactions.iter().any(|present| {
                    present.kind == transaction.kind
                        && present.parent_models == transaction.parent_models
                }) {
                    existing.model_transactions.push(transaction);
                }
            }
        } else {
            merged.push(variant);
        }
    }
    merged
}

pub(super) fn retain_variant_diversity(
    variants: Vec<FinalSceneVariant>,
    limit: usize,
    baseline_first: bool,
) -> Vec<FinalSceneVariant> {
    let limit = limit.min(variants.len());
    let mut selected = Vec::with_capacity(limit);
    let mut used = vec![false; variants.len()];
    let predicates: [fn(&str) -> bool; 6] = if baseline_first {
        [
            |class: &str| class == "baseline-free",
            |class: &str| class.starts_with("scene-repetition-"),
            |class: &str| class.starts_with("scene-mirror-"),
            |class: &str| class.contains("-primitive-"),
            |class: &str| class.contains("-relation-"),
            |class: &str| {
                class.contains("-path")
                    && !class.contains("-primitive-")
                    && !class.contains("-relation-")
            },
        ]
    } else {
        [
            |class: &str| class.starts_with("scene-repetition-"),
            |class: &str| class.starts_with("scene-mirror-"),
            |class: &str| class.contains("-primitive-"),
            |class: &str| class.contains("-relation-"),
            |class: &str| class == "baseline-free",
            |class: &str| {
                class.contains("-path")
                    && !class.contains("-primitive-")
                    && !class.contains("-relation-")
            },
        ]
    };
    for predicate in predicates {
        if selected.len() == limit {
            break;
        }
        if let Some((index, _)) = variants
            .iter()
            .enumerate()
            .find(|(index, variant)| !used[*index] && predicate(&variant.class))
        {
            used[index] = true;
            selected.push(variants[index].clone());
        }
    }
    for (index, variant) in variants.into_iter().enumerate() {
        if selected.len() == limit {
            break;
        }
        if !used[index] {
            selected.push(variant);
        }
    }
    selected
}
