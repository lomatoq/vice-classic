use std::collections::BTreeMap;

use sha2::{Digest, Sha256};
use vice_evidence::{BoundaryChain, Flat2Evidence};
use vice_fit::BoundaryModel;
use vice_ir::{scene_digest_sha256, Canvas, GlobalFormationHypothesis};
use vice_opt::{
    apply_compound_transaction_traced, CompoundTransaction, OptimizationResult, PriorCodeLengths,
    SceneMutation, ScoredHypothesis, TransactionApplication, TransactionKind,
};
use vice_svg::{
    build_export_plan, canonical_export_plan_bytes, materialize_svg,
    parse_and_render_independently, IndependentlyRenderedSvg, SvgProfile,
};
use vice_verify::{quantize_and_verify, seal_delivery};

use crate::config::CoreConfig;
use crate::scene::{build_scene_candidate, optimize_paint, SceneCandidate, TopologyArm};
use crate::types::{
    CandidateFailureStage, CandidateRefusal, CandidateRelationSolveTrace, CandidateSummary,
};
use crate::Intent;

#[derive(Debug)]
pub(crate) struct MaterializedCandidate {
    pub summary: CandidateSummary,
    pub score: ScoredHypothesis,
    pub bindings: Vec<vice_verify::BoundaryBinding>,
    pub bindings_bytes: u64,
    pub scene_json: Vec<u8>,
    pub plan_json: Vec<u8>,
    pub pure_svg: Vec<u8>,
    pub seam_svg: Vec<u8>,
    pub render_png: Vec<u8>,
    pub seal_json: Vec<u8>,
    pub estimated_memory_bytes: u64,
}

#[derive(Debug, Clone)]
struct SerializedDelivery {
    plan_json: Vec<u8>,
    pure_svg: Vec<u8>,
    seam_svg: Vec<u8>,
    pure_witness: IndependentlyRenderedSvg,
    seam_witness: IndependentlyRenderedSvg,
}

#[derive(Debug, Clone)]
struct OptimizedScene {
    scene: vice_ir::VectorScene,
    optimizer: OptimizationResult,
    transaction: TransactionApplication,
}

#[derive(Debug, Default)]
pub(crate) struct CandidateCache {
    optimized_by_scene_and_prior: BTreeMap<String, OptimizedScene>,
    serialized_by_scene: BTreeMap<String, SerializedDelivery>,
}

#[derive(Debug, Clone)]
pub(crate) struct CandidateModelTransaction {
    pub kind: TransactionKind,
    pub parent_models: Vec<BoundaryModel>,
}

pub(crate) struct CandidateRequest<'a> {
    pub canvas: Canvas,
    pub evidence: &'a Flat2Evidence,
    pub chains: &'a [BoundaryChain],
    pub models: &'a [BoundaryModel],
    pub arm: &'a TopologyArm,
    pub formation: GlobalFormationHypothesis,
    pub model_transactions: &'a [CandidateModelTransaction],
    pub transaction_base_arm: &'a TopologyArm,
    pub transaction_base_chains: &'a [BoundaryChain],
    pub transaction_base_models: &'a [BoundaryModel],
    pub transaction_base_formation: GlobalFormationHypothesis,
    pub hypothesis_id: String,
    pub formation_class: String,
    pub image: &'a vice_image::CanonicalImage,
    pub intent: Intent,
    pub config: &'a CoreConfig,
}

fn digest(bytes: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(bytes.as_ref()))
}

fn selected_relation_solve_trace(models: &[BoundaryModel]) -> Vec<CandidateRelationSolveTrace> {
    let mut trace = Vec::new();
    for (boundary_index, model) in models.iter().enumerate() {
        for &relation_index in &model.relation_kept_indices {
            let Some(hypothesis) = model.relations.get(relation_index) else {
                continue;
            };
            trace.push(CandidateRelationSolveTrace {
                boundary_index,
                relation_index,
                kind: hypothesis.kind,
                segments: hypothesis.segments.clone(),
                continuous_solve_samples: hypothesis.continuous_solve_samples,
                residual_contract:
                    "signed_normal_deviation/halfwidth*sqrt(independent_observations)",
                projected_finite_difference: true,
                rows: hypothesis.solve_trace.clone(),
            });
        }
    }
    trace
}

fn priors(
    models: &[BoundaryModel],
    opaque_paints: usize,
    intent: Intent,
    config: &CoreConfig,
) -> PriorCodeLengths {
    let policy = config.intent_prior(intent);
    PriorCodeLengths {
        topology_bits: models
            .iter()
            .map(|model| {
                (model.code.topology_bits + 1.0) * policy.structural_code_scale
                    + if model.primitive_kept.is_some() || model.relations_kept > 0 {
                        policy.constrained_promotion_extra_bits
                    } else {
                        0.0
                    }
            })
            .sum(),
        geometry_bits: models
            .iter()
            .map(|model| model.code.geometry_bits * policy.structural_code_scale)
            .sum(),
        // Flat2 shares one palette parameter across every same-material face;
        // disconnected components do not pay for duplicate copies.
        paint_bits: 24.0 * opaque_paints as f64,
        relation_bits: models
            .iter()
            .map(|model| model.code.relation_bits * policy.structural_code_scale)
            .sum(),
        formation_bits: 2.0,
    }
}

fn refusal(
    hypothesis_id: &str,
    stage: CandidateFailureStage,
    error: impl ToString,
) -> CandidateRefusal {
    CandidateRefusal {
        hypothesis_id: hypothesis_id.to_owned(),
        stage,
        detail: error.to_string(),
    }
}

fn serialized_delivery(
    scene: &vice_verify::QuantizedVerifiedScene,
    config: &CoreConfig,
) -> Result<SerializedDelivery, (CandidateFailureStage, String)> {
    let plan = build_export_plan(
        scene.scene(),
        config.export_decimal_places,
        config.apron_width_px,
    )
    .map_err(|error| (CandidateFailureStage::ExportPlan, error.to_string()))?;
    let plan_json = canonical_export_plan_bytes(&plan)
        .map_err(|error| (CandidateFailureStage::CanonicalArtifact, error.to_string()))?;
    let pure_svg = materialize_svg(&plan, SvgProfile::PurePartition)
        .map_err(|error| (CandidateFailureStage::SvgMaterialization, error.to_string()))?;
    let seam_svg = materialize_svg(&plan, SvgProfile::SeamSafe)
        .map_err(|error| (CandidateFailureStage::SvgMaterialization, error.to_string()))?;
    let pure_witness = parse_and_render_independently(&pure_svg)
        .map_err(|error| (CandidateFailureStage::IndependentRender, error.to_string()))?;
    let seam_witness = parse_and_render_independently(&seam_svg)
        .map_err(|error| (CandidateFailureStage::IndependentRender, error.to_string()))?;
    Ok(SerializedDelivery {
        plan_json,
        pure_svg,
        seam_svg,
        pure_witness,
        seam_witness,
    })
}

fn memory_bytes(candidate: &MaterializedCandidate) -> u64 {
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
    .sum::<u64>()
        + candidate.bindings_bytes
}

fn apply_expected_transition(
    current: &vice_ir::VectorScene,
    expected: &vice_ir::VectorScene,
    kind: TransactionKind,
    mutations: Vec<SceneMutation>,
) -> Result<(vice_ir::VectorScene, TransactionApplication), String> {
    if mutations.is_empty() {
        return Err(format!("{kind:?} proposal changes no scene state"));
    }
    let (child, application) = apply_compound_transaction_traced(
        current,
        &CompoundTransaction {
            kind,
            expected_parent_digest: scene_digest_sha256(current)
                .map_err(|error| error.to_string())?,
            mutations,
        },
    )
    .map_err(|error| error.to_string())?;
    let expected_digest = scene_digest_sha256(expected).map_err(|error| error.to_string())?;
    if application.child_digest != expected_digest || child != *expected {
        return Err(format!(
            "{kind:?} atomic result differs from its completely refitted target"
        ));
    }
    Ok((child, application))
}

fn geometry_mutations(
    current: &vice_ir::VectorScene,
    expected: &vice_ir::VectorScene,
) -> Result<Vec<SceneMutation>, String> {
    if current.canvas != expected.canvas
        || current.formation != expected.formation
        || current.graph.vertices.len() != expected.graph.vertices.len()
        || current.graph.boundaries.len() != expected.graph.boundaries.len()
        || current.graph.half_edges != expected.graph.half_edges
        || current.graph.faces != expected.graph.faces
        || current.graph.exterior != expected.graph.exterior
    {
        return Err("geometry transaction changed non-geometry scene structure".into());
    }
    let mut mutations = Vec::new();
    for (index, (before, after)) in current
        .graph
        .vertices
        .iter()
        .zip(&expected.graph.vertices)
        .enumerate()
    {
        if before != after {
            mutations.push(SceneMutation::ReplaceVertexPosition {
                vertex: vice_ir::VertexId(index as u32),
                position: after.pos,
            });
        }
    }
    for (index, (before, after)) in current
        .graph
        .boundaries
        .iter()
        .zip(&expected.graph.boundaries)
        .enumerate()
    {
        if before.left_face != after.left_face
            || before.right_face != after.right_face
            || before.start_vertex != after.start_vertex
            || before.end_vertex != after.end_vertex
        {
            return Err("geometry transaction changed boundary ownership or incidence".into());
        }
        if before.curve != after.curve || before.closure_join != after.closure_join {
            mutations.push(SceneMutation::ReplaceBoundaryGeometry {
                boundary: vice_ir::BoundaryId(index as u32),
                curve: after.curve.clone(),
                closure_join: after.closure_join,
            });
        }
    }
    Ok(mutations)
}

fn topology_transaction_kind(base: &TopologyArm, target: &TopologyArm) -> TransactionKind {
    let base_components = base.dcel.foreground_faces();
    let target_components = target.dcel.foreground_faces();
    if target.dcel.holes() != base.dcel.holes() {
        TransactionKind::TopologyHole
    } else if target_components < base_components {
        TransactionKind::TopologyBridge
    } else if target_components > base_components {
        TransactionKind::TopologySplit
    } else if target.dcel.boundaries().len() < base.dcel.boundaries().len() {
        TransactionKind::TopologyMerge
    } else {
        TransactionKind::TopologySplit
    }
}

fn build_transactional_candidate(
    request: &CandidateRequest<'_>,
) -> Result<(SceneCandidate, Vec<TransactionApplication>), String> {
    let final_candidate = build_scene_candidate(
        request.canvas,
        request.evidence,
        request.chains,
        request.models,
        request.arm,
        request.formation,
    )?;
    let base_target = build_scene_candidate(
        request.canvas,
        request.evidence,
        request.transaction_base_chains,
        request.transaction_base_models,
        request.transaction_base_arm,
        request.transaction_base_formation,
    )?;
    let mut current = base_target.scene;
    let mut applications = Vec::new();

    let target_parent_models = request
        .model_transactions
        .first()
        .map_or(request.models, |transaction| {
            transaction.parent_models.as_slice()
        });
    if request.arm.class != request.transaction_base_arm.class {
        let rebuilt = build_scene_candidate(
            request.canvas,
            request.evidence,
            request.chains,
            target_parent_models,
            request.arm,
            request.transaction_base_formation,
        )?;
        let kind = topology_transaction_kind(request.transaction_base_arm, request.arm);
        let (child, application) = apply_expected_transition(
            &current,
            &rebuilt.scene,
            kind,
            vec![SceneMutation::ReplaceGraph(rebuilt.scene.graph.clone())],
        )?;
        current = child;
        applications.push(application);
    }

    for transaction in request.model_transactions {
        let parent = build_scene_candidate(
            request.canvas,
            request.evidence,
            request.chains,
            &transaction.parent_models,
            request.arm,
            request.transaction_base_formation,
        )?;
        let expected = build_scene_candidate(
            request.canvas,
            request.evidence,
            request.chains,
            request.models,
            request.arm,
            request.transaction_base_formation,
        )?;
        let mutations = geometry_mutations(&parent.scene, &expected.scene)?;
        let (child, application) =
            apply_expected_transition(&parent.scene, &expected.scene, transaction.kind, mutations)?;
        if current == parent.scene {
            current = child;
        } else if current != expected.scene {
            return Err(format!(
                "{:?} parent is not the current completely refitted topology",
                transaction.kind
            ));
        }
        applications.push(application);
    }

    if current != final_candidate.scene {
        let exterior_changed =
            current.formation.exterior != final_candidate.scene.formation.exterior;
        let kind = if exterior_changed {
            TransactionKind::ExteriorChange
        } else {
            TransactionKind::FormationChange
        };
        let mut mutations = Vec::new();
        if current.graph != final_candidate.scene.graph {
            mutations.push(SceneMutation::ReplaceGraph(
                final_candidate.scene.graph.clone(),
            ));
        }
        if current.formation != final_candidate.scene.formation {
            mutations.push(SceneMutation::ReplaceFormation(
                final_candidate.scene.formation,
            ));
        }
        let (child, application) =
            apply_expected_transition(&current, &final_candidate.scene, kind, mutations)?;
        current = child;
        applications.push(application);
    }

    if current != final_candidate.scene {
        return Err("compound transaction lineage did not reach the requested scene".into());
    }
    Ok((
        SceneCandidate {
            scene: current,
            bindings: final_candidate.bindings,
            paint_layout: final_candidate.paint_layout,
        },
        applications,
    ))
}

pub(crate) fn materialize_candidate(
    request: CandidateRequest<'_>,
    cache: &mut CandidateCache,
) -> Result<MaterializedCandidate, CandidateRefusal> {
    let hypothesis_id = request.hypothesis_id.clone();
    let (mut candidate, mut transactions) =
        build_transactional_candidate(&request).map_err(|error| {
            refusal(
                &hypothesis_id,
                CandidateFailureStage::SceneConstruction,
                error,
            )
        })?;
    let opaque_paints = 1 + usize::from(!candidate.paint_layout.background.is_empty());
    let prior = priors(
        request.models,
        opaque_paints,
        request.intent,
        request.config,
    );
    let optimization_key = format!(
        "{}|{}",
        vice_ir::scene_digest_sha256(&candidate.scene).map_err(|error| {
            refusal(
                &hypothesis_id,
                CandidateFailureStage::CanonicalArtifact,
                error,
            )
        })?,
        digest(serde_json::to_vec(&prior).map_err(|error| {
            refusal(
                &hypothesis_id,
                CandidateFailureStage::CanonicalArtifact,
                error,
            )
        })?)
    );
    let optimizer = if let Some(cached) = cache.optimized_by_scene_and_prior.get(&optimization_key)
    {
        candidate.scene = cached.scene.clone();
        transactions.push(cached.transaction.clone());
        cached.optimizer.clone()
    } else {
        let base = vice_verify::preseal_scene(
            &candidate.scene,
            &candidate.bindings,
            request.config.verification,
        )
        .map_err(|error| refusal(&hypothesis_id, CandidateFailureStage::Preseal, error))?;
        let (optimized, optimizer, paint_transaction): (
            _,
            OptimizationResult,
            TransactionApplication,
        ) = optimize_paint(
            candidate,
            request.image,
            base.render(),
            prior,
            request.config,
        )
        .map_err(|error| {
            refusal(
                &hypothesis_id,
                CandidateFailureStage::PaintOptimization,
                error,
            )
        })?;
        candidate = optimized;
        transactions.push(paint_transaction.clone());
        cache.optimized_by_scene_and_prior.insert(
            optimization_key,
            OptimizedScene {
                scene: candidate.scene.clone(),
                optimizer: optimizer.clone(),
                transaction: paint_transaction,
            },
        );
        optimizer
    };
    let verified = quantize_and_verify(
        &candidate.scene,
        &candidate.bindings,
        request.config.verification,
        request.config.quantization,
    )
    .map_err(|error| refusal(&hypothesis_id, CandidateFailureStage::Quantization, error))?;
    let scene_digest_sha256 = verified
        .post_quantization_certificate()
        .post_scene_digest_sha256
        .clone();
    let delivery = if let Some(delivery) = cache.serialized_by_scene.get(&scene_digest_sha256) {
        delivery.clone()
    } else {
        let delivery = serialized_delivery(&verified, request.config)
            .map_err(|(stage, error)| refusal(&hypothesis_id, stage, error))?;
        cache
            .serialized_by_scene
            .insert(scene_digest_sha256.clone(), delivery.clone());
        delivery
    };
    let score = vice_opt::score_serialized_full_resolution(
        verified.scene(),
        request.image,
        delivery.seam_witness.premultiplied_rgba8(),
        delivery.seam_witness.width_px(),
        delivery.seam_witness.height_px(),
        request.config.likelihood,
        prior,
    )
    .map_err(|error| {
        refusal(
            &hypothesis_id,
            CandidateFailureStage::SerializedLikelihood,
            error,
        )
    })?;
    let plan = vice_svg::build_export_plan(
        verified.scene(),
        request.config.export_decimal_places,
        request.config.apron_width_px,
    )
    .map_err(|error| refusal(&hypothesis_id, CandidateFailureStage::ExportPlan, error))?;
    let seal = seal_delivery(
        &verified,
        &plan,
        &delivery.pure_witness,
        &delivery.seam_witness,
        request.config.seal,
    )
    .map_err(|error| refusal(&hypothesis_id, CandidateFailureStage::DeliverySeal, error))?;
    let scene_json = vice_ir::canonical_scene_bytes(verified.scene()).map_err(|error| {
        refusal(
            &hypothesis_id,
            CandidateFailureStage::CanonicalArtifact,
            error,
        )
    })?;
    let delivery_digest = digest(
        format!(
            "{}|{}",
            delivery.pure_witness.render_digest_sha256(),
            delivery.seam_witness.render_digest_sha256()
        )
        .as_bytes(),
    );
    let scored = ScoredHypothesis {
        hypothesis_id: request.hypothesis_id.clone(),
        delivery_digest: delivery_digest.clone(),
        topology_class: request.arm.topology_class.clone(),
        formation_class: request.formation_class.clone(),
        total_bits: score.total_bits,
    };
    let summary = CandidateSummary {
        hypothesis_id: request.hypothesis_id,
        topology_arm: request.arm.class.clone(),
        topology_class: request.arm.topology_class.clone(),
        formation_class: request.formation_class,
        scene_digest_sha256,
        delivery_digest,
        score,
        pre_quantization: verified.pre_quantization_certificate().clone(),
        post_quantization: verified.post_quantization_certificate().clone(),
        delivery_seal: seal.clone(),
        optimizer,
        intra_boundary_relation_solve_trace: selected_relation_solve_trace(request.models),
        transactions,
    };
    let seal_json = serde_json::to_vec(&seal).map_err(|error| {
        refusal(
            &hypothesis_id,
            CandidateFailureStage::CanonicalArtifact,
            error,
        )
    })?;
    let bindings = verified.bindings().to_vec();
    let bindings_bytes = serde_json::to_vec(&bindings)
        .map_err(|error| {
            refusal(
                &hypothesis_id,
                CandidateFailureStage::CanonicalArtifact,
                error,
            )
        })?
        .len() as u64;
    let mut candidate = MaterializedCandidate {
        summary,
        score: scored,
        bindings,
        bindings_bytes,
        scene_json,
        plan_json: delivery.plan_json,
        pure_svg: delivery.pure_svg,
        seam_svg: delivery.seam_svg,
        render_png: delivery.seam_witness.png_bytes().to_vec(),
        seal_json,
        estimated_memory_bytes: 0,
    };
    candidate.estimated_memory_bytes = memory_bytes(&candidate);
    Ok(candidate)
}
