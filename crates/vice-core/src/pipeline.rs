use std::collections::BTreeSet;
use std::time::Instant;

use serde::Serialize;
use sha2::{Digest, Sha256};
use vice_evidence::Flat2Outcome;
use vice_ir::{Canvas, PixelFilter};
use vice_opt::{
    posterior_with_search_mass, select_diverse_beam, BeamCandidate, SearchMassInput,
    TransactionKind,
};

use crate::candidate::{
    materialize_candidate, CandidateCache, CandidateModelTransaction, CandidateRequest,
};
use crate::config::{ConfidenceMetrics, CoreConfig, PerturbationStability};
use crate::scene::{topology_arms, TopologyArm};
use crate::types::{
    CalibrationRun, CalibrationWitness, CandidateRefusal, CandidateSummary, DecisionStatus,
    FailureReason, RuntimeSummary, SuccessArtifacts, TopologyArmRefusal, TopologyEnvelopeTrace,
    TransactionInventory, TransactionInventoryRow, VectorizeOutcome, VectorizeReport,
    VectorizeSuccess, CORE_REPORT_SCHEMA,
};
use crate::VectorizeRequest;

const TRANSACTION_DIVERSITY_SEED_CLASSES: usize = 2;
const SINGLE_DELIVERY_CLASS_MARGIN_BITS_V1: f64 = 1024.0;

#[derive(Debug, Default)]
struct ReportParts {
    evidence: Option<vice_evidence::Flat2Analysis>,
    topology: Option<TopologyEnvelopeTrace>,
    fits: Vec<vice_fit::ModelRun>,
    beam: Option<vice_opt::BudgetLedger>,
    search_mass: Option<vice_opt::SearchMassCertificate>,
    confidence_metrics: Option<ConfidenceMetrics>,
    candidates: Vec<CandidateSummary>,
    candidate_refusals: Vec<CandidateRefusal>,
    transaction_inventory: Option<TransactionInventory>,
    selected_hypothesis_id: Option<String>,
    selected_boundary_bindings: Vec<vice_verify::BoundaryBinding>,
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
        delivery_policy_sha256: config.delivery_policy_sha256(),
        calibration: config.confidence.clone(),
        evidence: parts.evidence,
        topology: parts.topology,
        fits: parts.fits,
        beam: parts.beam,
        search_mass: parts.search_mass,
        confidence_metrics: parts.confidence_metrics,
        runtime: RuntimeSummary {
            elapsed_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
            candidates_scored: parts.candidates.len() as u64,
            candidate_bytes: parts.candidate_bytes,
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

fn free_model(selected: &vice_fit::BoundaryModel) -> vice_fit::BoundaryModel {
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

fn final_scene_variants(
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

fn retain_variant_diversity(
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
    vectorize_impl(bytes, request, config, None)
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
        Ok(config) => vectorize_impl(bytes, request, &config, None),
        Err(error) => {
            let config = CoreConfig::development_for(request.preset);
            // Decode failures are faults in the input, independent of release
            // configuration availability. Preserve that typed outcome while
            // still making every decodable input fail closed on the config.
            if vice_image::CanonicalImage::decode_png(bytes, &vice_image::DecodeLimits::default())
                .is_err()
            {
                return vectorize_impl(bytes, request, &config, None);
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
    let mut selected = None;
    let mut baseline = None;
    let witness = |candidate: &crate::candidate::MaterializedCandidate| CalibrationWitness {
        candidate: candidate.summary.clone(),
        scene_json: candidate.scene_json.clone(),
        export_plan_json: candidate.plan_json.clone(),
        pure_partition_svg: candidate.pure_svg.clone(),
        seam_safe_svg: candidate.seam_svg.clone(),
        rendered_png: candidate.render_png.clone(),
        seal_json: candidate.seal_json.clone(),
    };
    let mut capture =
        |candidate: &crate::candidate::MaterializedCandidate,
         baseline_candidate: Option<&crate::candidate::MaterializedCandidate>| {
            selected = Some(witness(candidate));
            baseline = baseline_candidate.map(witness);
        };
    let outcome = vectorize_impl(bytes, request, config, Some(&mut capture));
    CalibrationRun {
        outcome,
        selected,
        baseline,
    }
}

fn vectorize_impl(
    bytes: &[u8],
    request: &VectorizeRequest,
    config: &CoreConfig,
    mut calibration_observer: Option<
        &mut dyn FnMut(
            &crate::candidate::MaterializedCandidate,
            Option<&crate::candidate::MaterializedCandidate>,
        ),
    >,
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
    if boundary_observation.chains.is_empty()
        || boundary_observation
            .chains
            .iter()
            .any(|chain| !chain.closed)
    {
        return refuse(
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
    let topology = topology_arms(&evidence);
    let proposal = topology.proposal;
    let topology_classes_upper_bound = proposal
        .envelope
        .hypotheses
        .iter()
        .map(|hypothesis| (hypothesis.signature.components, hypothesis.signature.holes))
        .collect::<BTreeSet<_>>()
        .len() as u64;
    let formation_classes_upper_bound = formations
        .iter()
        .map(vice_evidence::formation_id)
        .collect::<BTreeSet<_>>()
        .len() as u64;
    let mut topology_traces = topology.traces;
    let mut topology_refusals = topology.refusals;
    let arms = topology.arms;
    if arms.is_empty() {
        parts.topology = Some(TopologyEnvelopeTrace {
            proposal,
            materialized_arms: topology_traces,
            materialization_refusals: topology_refusals,
            prefit_budget_pruned_arms: Vec::new(),
        });
        return refuse(
            DecisionStatus::Unsupported,
            FailureReason::Topology {
                detail: "no M4.5 envelope hypothesis produced an audited closed-boundary DCEL \
                         that bound every observed chain"
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
    let max_topology_arms = config
        .beam
        .budget
        .max_candidates_considered
        .checked_div(formations.len())
        .unwrap_or(0)
        .min(config.beam.width);
    if max_topology_arms == 0 {
        parts.topology = Some(TopologyEnvelopeTrace {
            proposal,
            materialized_arms: topology_traces,
            materialization_refusals: topology_refusals,
            prefit_budget_pruned_arms: Vec::new(),
        });
        return refuse(
            DecisionStatus::Ambiguous,
            FailureReason::SearchTruncated {
                detail: "candidate budget cannot fit and score one topology/formation seed".into(),
            },
            request,
            config,
            source_sha256,
            production,
            parts,
            started,
        );
    }
    let topology_budget_truncated = arms.len() > max_topology_arms;
    let topology_budget_pruned_arms = arms
        .iter()
        .skip(max_topology_arms)
        .map(|arm| arm.trace.clone())
        .collect::<Vec<_>>();
    let canvas_dim_px = f64::from(image.width_px().max(image.height_px()));
    let mut fitted_arms = Vec::new();
    let mut fit_cache =
        std::collections::BTreeMap::<String, Result<vice_fit::ModelRun, String>>::new();
    for mut arm in arms.into_iter().take(max_topology_arms) {
        let mut fits = Vec::with_capacity(arm.chains.len());
        let mut fit_refusal = None;
        for (chain_index, chain) in arm.chains.iter().enumerate() {
            let fit_key = match serde_json::to_vec(chain) {
                Ok(bytes) => digest(bytes),
                Err(error) => {
                    fit_refusal = Some(format!(
                        "chain {chain_index} identity serialization: {error}"
                    ));
                    break;
                }
            };
            let fit = if let Some(cached) = fit_cache.get(&fit_key) {
                cached.clone()
            } else {
                let fit = if canvas_dim_px >= 128.0 {
                    vice_fit::k_best_boundary_models_bounded(
                        chain,
                        &vice_fit::FIT_BUDGET_V1,
                        canvas_dim_px,
                        config.k_discrete_paths,
                        vice_fit::DISCRETE_PROPOSAL_SAMPLE_CAP_V1,
                    )
                    .map_err(|error| format!("{error:?}"))
                } else {
                    vice_fit::k_best_boundary_models(
                        chain,
                        &vice_fit::FIT_BUDGET_V1,
                        canvas_dim_px,
                        config.k_discrete_paths,
                    )
                    .map_err(|error| format!("{error:?}"))
                }
                .and_then(|fit| {
                    (!fit.models.is_empty())
                        .then_some(fit)
                        .ok_or_else(|| "typed fitter produced no admissible model".to_string())
                });
                fit_cache.insert(fit_key, fit.clone());
                fit
            };
            match fit {
                Ok(fit) => fits.push(fit),
                Err(error) => {
                    fit_refusal = Some(format!("chain {chain_index}: {error}"));
                    break;
                }
            }
        }
        if let Some(detail) = fit_refusal {
            topology_refusals.push(TopologyArmRefusal {
                topology_class: arm.topology_class.clone(),
                signature_sha256: arm.trace.signature_sha256.clone(),
                foreground_connectivity: arm.trace.foreground_connectivity.clone(),
                field: arm.trace.field,
                saddle: arm.trace.saddle,
                extraction_level: arm.trace.extraction_level,
                detail: format!("complete topology refit refused: {detail}"),
            });
            continue;
        }
        arm.trace.fit_models_per_chain =
            fits.iter().map(|fit| fit.models.len()).collect::<Vec<_>>();
        if let Some(trace) = topology_traces
            .iter_mut()
            .find(|trace| trace.class == arm.class)
        {
            *trace = arm.trace.clone();
        }
        let baseline_models = fits.iter().map(|fit| free_model(&fit.models[0])).collect();
        let variants = final_scene_variants(&fits, &arm.chains, canvas_dim_px);
        let variant_count = variants.len();
        let variants = retain_variant_diversity(variants, variant_count, canvas_dim_px >= 128.0);
        fitted_arms.push(FittedTopologyArm {
            arm,
            fits,
            baseline_models,
            variants,
        });
    }
    parts.topology = Some(TopologyEnvelopeTrace {
        proposal,
        materialized_arms: topology_traces,
        materialization_refusals: topology_refusals,
        prefit_budget_pruned_arms: topology_budget_pruned_arms,
    });
    if fitted_arms.is_empty() {
        let first = parts
            .topology
            .as_ref()
            .and_then(|topology| topology.materialization_refusals.last())
            .map_or_else(
                || "no topology arm published a more specific refusal".to_string(),
                |refusal| refusal.detail.clone(),
            );
        return refuse(
            DecisionStatus::Unsupported,
            FailureReason::Fitting {
                detail: format!(
                    "every materializable topology envelope arm refused its complete typed \
                     boundary refit; last: {first}"
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
    parts.fits = fitted_arms[0].fits.clone();
    let fit_truncated = fitted_arms.iter().any(|bundle| {
        bundle.fits.iter().any(|fit| {
            fit.models.len() >= config.k_discrete_paths
                || fit.discrete_search_samples < fit.observed_samples
                || fit.continuous_solve_samples < fit.observed_samples
                || fit.full_certification_refusals > 0
                || fit.resource_pruned_proposals > 0
                || fit.proposal_levels_skipped_after_certification > 0
        })
    });
    let canvas = Canvas {
        width_px: image.width_px(),
        height_px: image.height_px(),
    };
    let mut candidates = Vec::new();
    let mut candidate_refusals = Vec::new();
    let mut candidate_cache = CandidateCache::default();
    let mut proposed_transactions = std::collections::BTreeMap::<TransactionKind, u64>::new();
    let planned_materializations = fitted_arms
        .iter()
        .map(|bundle| bundle.variants.len().saturating_mul(formations.len()))
        .sum::<usize>();
    let mut scheduled = BTreeSet::new();
    let mut materialization_order = Vec::with_capacity(planned_materializations);
    for topology_index in 0..fitted_arms.len().min(config.beam.min_topology_classes) {
        let task = (topology_index, 0, 0);
        if scheduled.insert(task) {
            materialization_order.push(task);
        }
    }
    for formation_index in 0..formations.len().min(config.beam.min_formation_classes) {
        let task = (0, 0, formation_index);
        if scheduled.insert(task) {
            materialization_order.push(task);
        }
    }
    for variant_index in 0..fitted_arms[0]
        .variants
        .len()
        .min(TRANSACTION_DIVERSITY_SEED_CLASSES)
    {
        let task = (0, variant_index, 0);
        if scheduled.insert(task) {
            materialization_order.push(task);
        }
    }
    let diversity_seed_materializations = materialization_order.len();
    for (topology_index, bundle) in fitted_arms.iter().enumerate() {
        for variant_index in 0..bundle.variants.len() {
            for formation_index in 0..formations.len() {
                let task = (topology_index, variant_index, formation_index);
                if scheduled.insert(task) {
                    materialization_order.push(task);
                }
            }
        }
    }
    let unmaterialized_by_candidate_budget = materialization_order
        .len()
        .saturating_sub(config.beam.budget.max_candidates_considered);
    let evaluation_truncated = unmaterialized_by_candidate_budget > 0;
    materialization_order.truncate(config.beam.budget.max_candidates_considered);
    let scheduled_materializations = materialization_order.len();
    // A time budget cannot be subordinated to a quota: materialize one
    // deterministic seed so the run has a candidate, then enforce the
    // deadline before every further topology/formation/transaction seed.
    // Unreached diversity slots remain explicit unexplored mass.
    let mandatory_diversity_materializations =
        usize::from(diversity_seed_materializations > 0 && scheduled_materializations > 0);
    let mut attempted_materializations = 0usize;
    let mut time_truncated = false;
    'materialization: for (topology_index, variant_index, formation_index) in materialization_order
    {
        let bundle = &fitted_arms[topology_index];
        let variant = &bundle.variants[variant_index];
        let arm = &bundle.arm;
        let formation = &formations[formation_index];
        if attempted_materializations >= mandatory_diversity_materializations
            && started.elapsed().as_millis() >= u128::from(config.beam.budget.max_elapsed_ms)
        {
            time_truncated = true;
            break 'materialization;
        }
        attempted_materializations += 1;
        for transaction in &variant.model_transactions {
            *proposed_transactions.entry(transaction.kind).or_default() += 1;
        }
        if arm.class != fitted_arms[0].arm.class {
            let kind = if arm.dcel.holes() != fitted_arms[0].arm.dcel.holes() {
                TransactionKind::TopologyHole
            } else if arm.dcel.foreground_faces() < fitted_arms[0].arm.dcel.foreground_faces() {
                TransactionKind::TopologyBridge
            } else if arm.dcel.foreground_faces() > fitted_arms[0].arm.dcel.foreground_faces() {
                TransactionKind::TopologySplit
            } else if arm.dcel.boundaries().len() < fitted_arms[0].arm.dcel.boundaries().len() {
                TransactionKind::TopologyMerge
            } else {
                TransactionKind::TopologySplit
            };
            *proposed_transactions.entry(kind).or_default() += 1;
        }
        if *formation != formations[0] {
            let kind = if formation.exterior != formations[0].exterior {
                TransactionKind::ExteriorChange
            } else {
                TransactionKind::FormationChange
            };
            *proposed_transactions.entry(kind).or_default() += 1;
        }
        *proposed_transactions
            .entry(TransactionKind::PaintChange)
            .or_default() += 1;
        let formation_class = vice_evidence::formation_id(formation);
        let hypothesis_id = format!("{}/t{topology_index}/{formation_class}", variant.class);
        let built = materialize_candidate(
            CandidateRequest {
                canvas,
                evidence: &evidence,
                chains: &arm.chains,
                models: &variant.models,
                arm,
                formation: *formation,
                model_transactions: &variant.model_transactions,
                transaction_base_arm: &fitted_arms[0].arm,
                transaction_base_chains: &fitted_arms[0].arm.chains,
                transaction_base_models: &fitted_arms[0].baseline_models,
                transaction_base_formation: formations[0],
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
    let mut applied_transactions = std::collections::BTreeMap::<TransactionKind, u64>::new();
    for candidate in &candidates {
        for transaction in &candidate.summary.transactions {
            *applied_transactions.entry(transaction.kind).or_default() += 1;
        }
    }
    let rows = TransactionKind::ALL
        .into_iter()
        .map(|kind| {
            let proposed = proposed_transactions.get(&kind).copied().unwrap_or(0);
            let atomic_applied = applied_transactions.get(&kind).copied().unwrap_or(0);
            TransactionInventoryRow {
                kind,
                proposed,
                atomic_applied,
                verified_and_exact_scored: atomic_applied,
                refused_before_score: proposed.saturating_sub(atomic_applied),
                not_applicable: proposed == 0,
            }
        })
        .collect::<Vec<_>>();
    parts.transaction_inventory = Some(TransactionInventory {
        complete_kind_enumeration: rows.len() == TransactionKind::ALL.len()
            && rows
                .iter()
                .zip(TransactionKind::ALL)
                .all(|(row, kind)| row.kind == kind),
        rows,
    });
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
    let mut selection = match select_diverse_beam(
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
    selection.ledger.time_budget_exhausted |= time_truncated;
    selection.ledger.unmaterialized_by_candidate_budget = unmaterialized_by_candidate_budget
        .try_into()
        .unwrap_or(u64::MAX);
    selection.ledger.unmaterialized_by_time_budget = if time_truncated {
        scheduled_materializations
            .saturating_sub(attempted_materializations)
            .try_into()
            .unwrap_or(u64::MAX)
    } else {
        0
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
        topology_classes_upper_bound,
        formation_classes_upper_bound,
        unexplored: if topology_budget_truncated
            || fit_truncated
            || evaluation_truncated
            || time_truncated
        {
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
    let selected = &candidates[selected_index];
    let top2_class_margin_bits =
        search_mass
            .delivery
            .get(1)
            .map_or(SINGLE_DELIVERY_CLASS_MARGIN_BITS_V1, |runner_up| {
                if runner_up.explored_mass > 0.0 {
                    (best_delivery.explored_mass / runner_up.explored_mass)
                        .log2()
                        .clamp(0.0, SINGLE_DELIVERY_CLASS_MARGIN_BITS_V1)
                } else {
                    SINGLE_DELIVERY_CLASS_MARGIN_BITS_V1
                }
            });
    let diagnostics = &selected.summary.score.diagnostics;
    let predictive_bits_per_block = if diagnostics.blocks == 0 {
        f64::MAX
    } else {
        selected.summary.score.pixel_bits / diagnostics.blocks as f64
    };
    let max_abs_residual_lag1 = diagnostics.lag1_x.abs().max(diagnostics.lag1_y.abs());
    let phase_envelope_stable = parts.topology.as_ref().is_some_and(|topology| {
        let same = |class: &str| class == selected.score.topology_class;
        !topology.materialized_arms.is_empty()
            && topology
                .materialized_arms
                .iter()
                .all(|arm| same(&arm.topology_class))
            && topology
                .prefit_budget_pruned_arms
                .iter()
                .all(|arm| same(&arm.topology_class))
            && topology
                .materialization_refusals
                .iter()
                .all(|arm| same(&arm.topology_class))
    });
    let sample_step_certificate_stable = fitted_arms
        .iter()
        .find(|bundle| bundle.arm.class == selected.summary.topology_arm)
        .is_some_and(|bundle| {
            !bundle.fits.is_empty()
                && bundle.fits.iter().all(|fit| {
                    fit.full_resolution_certified
                        && fit.observed_samples > 0
                        && fit.discrete_search_samples > 0
                        && fit.continuous_solve_samples > 0
                        && !fit.discrete_search_levels.is_empty()
                        && !fit.models.is_empty()
                })
        });
    let canonical_binding_check = (|| {
        let scene = vice_ir::parse_scene(&selected.scene_json)
            .map_err(|error| format!("parse selected scene: {error}"))?;
        let scene = vice_ir::ValidatedScene::new(scene)
            .map_err(|error| format!("validate selected scene: {error}"))?;
        let roundtrip_topology = vice_verify::topology_signature_sha256(scene.scene())
            .map_err(|error| format!("roundtrip topology: {error}"))?;
        if roundtrip_topology != selected.summary.post_quantization.topology_signature_sha256 {
            return Err(format!(
                "canonical scene roundtrip changed topology: report={} roundtrip={}",
                selected.summary.post_quantization.topology_signature_sha256, roundtrip_topology
            ));
        }
        if scene.scene().graph.boundaries.len() != selected.bindings.len() {
            return Err(format!(
                "canonical scene roundtrip changed boundary count: scene={} bindings={}",
                scene.scene().graph.boundaries.len(),
                selected.bindings.len()
            ));
        }
        let bindings = vice_verify::rebind_scene_bindings(
            scene.scene(),
            &selected.bindings,
            config.verification,
        )
        .map_err(|error| format!("canonical binding remap: {error}"))?;
        Ok((scene, bindings))
    })();
    if let Ok((_, bindings)) = &canonical_binding_check {
        parts.selected_boundary_bindings = bindings.clone();
    }
    let tighter_tolerance = config
        .verification
        .render_options
        .budget
        .chord_tolerance
        .px()
        / 2.0;
    let tighter_render_check = canonical_binding_check
        .as_ref()
        .map_err(Clone::clone)
        .and_then(|(scene, bindings)| {
            let budget =
                vice_render::TessellationBudget::with_chord_tolerance_px(tighter_tolerance)
                    .ok_or_else(|| "invalid tighter tessellation budget".to_string())?;
            let mut verification = config.verification;
            verification.render_options = verification.render_options.with_budget(budget);
            vice_verify::preseal_scene(scene.scene(), bindings, verification)
                .map(|_| ())
                .map_err(|error| error.to_string())
        });
    let render_tolerance_certificate_stable = tighter_render_check.is_ok();
    let accepted_local = selected
        .summary
        .optimizer
        .trace
        .iter()
        .filter(|row| row.accepted && !row.full_check)
        .count();
    let full_checks = selected
        .summary
        .optimizer
        .trace
        .iter()
        .filter(|row| row.full_check)
        .count();
    let solver_certificate_stable = full_checks >= accepted_local
        && selected.summary.optimizer.trace.iter().all(|row| {
            row.child_bits.is_finite()
                && row.parent_bits.is_finite()
                && (!row.full_check || (row.accepted && !row.rolled_back_to_verified))
        });
    let mut perturbation_stability = PerturbationStability::from_legs(
        phase_envelope_stable,
        sample_step_certificate_stable,
        render_tolerance_certificate_stable,
        solver_certificate_stable,
    );
    perturbation_stability.render_tolerance_refusal = tighter_render_check.err();
    let confidence_metrics = ConfidenceMetrics {
        top2_class_margin_bits,
        posterior_predictive_bits_per_block: if predictive_bits_per_block.is_finite() {
            predictive_bits_per_block
        } else {
            f64::MAX
        },
        max_abs_residual_lag1: if max_abs_residual_lag1.is_finite() {
            max_abs_residual_lag1
        } else {
            f64::MAX
        },
        topology_entropy_upper_bound: search_mass.topology_entropy_upper_bound.clone(),
        formation_entropy_upper_bound: search_mass.formation_entropy_upper_bound.clone(),
        perturbation_stability,
    };
    parts.confidence_metrics = Some(confidence_metrics.clone());
    if let Some(observer) = calibration_observer.as_mut() {
        let baseline = candidates
            .iter()
            .find(|candidate| candidate.score.hypothesis_id.starts_with("baseline-free/"));
        observer(selected, baseline);
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
    if let Err(detail) =
        calibration.permits(&config.identity(), &best_delivery, &confidence_metrics)
    {
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
