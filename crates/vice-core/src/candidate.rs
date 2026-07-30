use std::collections::BTreeMap;

use sha2::{Digest, Sha256};
use vice_evidence::{BoundaryChain, Flat2Evidence};
use vice_fit::BoundaryModel;
use vice_ir::{Canvas, GlobalFormationHypothesis, Paint};
use vice_opt::{OptimizationResult, PriorCodeLengths, ScoredHypothesis};
use vice_svg::{
    build_export_plan, canonical_export_plan_bytes, materialize_svg,
    parse_and_render_independently, IndependentlyRenderedSvg, SvgProfile,
};
use vice_verify::{quantize_and_verify, seal_delivery};

use crate::config::CoreConfig;
use crate::scene::{build_scene_candidate, optimize_paint, TopologyArm};
use crate::types::{CandidateFailureStage, CandidateRefusal, CandidateSummary};
use crate::Intent;

#[derive(Debug)]
pub(crate) struct MaterializedCandidate {
    pub summary: CandidateSummary,
    pub score: ScoredHypothesis,
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
}

#[derive(Debug, Default)]
pub(crate) struct CandidateCache {
    optimized_by_scene_and_prior: BTreeMap<String, OptimizedScene>,
    serialized_by_scene: BTreeMap<String, SerializedDelivery>,
}

pub(crate) struct CandidateRequest<'a> {
    pub canvas: Canvas,
    pub evidence: &'a Flat2Evidence,
    pub chain: &'a BoundaryChain,
    pub model: &'a BoundaryModel,
    pub arm: &'a TopologyArm,
    pub formation: GlobalFormationHypothesis,
    pub hypothesis_id: String,
    pub formation_class: String,
    pub image: &'a vice_image::CanonicalImage,
    pub intent: Intent,
    pub config: &'a CoreConfig,
}

fn digest(bytes: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(bytes.as_ref()))
}

fn priors(
    model: &BoundaryModel,
    opaque_faces: usize,
    intent: Intent,
    config: &CoreConfig,
) -> PriorCodeLengths {
    let policy = config.intent_prior(intent);
    let promoted = model.primitive_kept.is_some() || model.relations_kept > 0;
    PriorCodeLengths {
        topology_bits: (model.code.topology_bits + 1.0) * policy.structural_code_scale
            + if promoted {
                policy.constrained_promotion_extra_bits
            } else {
                0.0
            },
        geometry_bits: model.code.geometry_bits * policy.structural_code_scale,
        paint_bits: 24.0 * opaque_faces as f64,
        relation_bits: model.code.relation_bits * policy.structural_code_scale,
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
    .sum()
}

pub(crate) fn materialize_candidate(
    request: CandidateRequest<'_>,
    cache: &mut CandidateCache,
) -> Result<MaterializedCandidate, CandidateRefusal> {
    let hypothesis_id = request.hypothesis_id.clone();
    let mut candidate = build_scene_candidate(
        request.canvas,
        request.evidence,
        request.chain,
        request.model,
        request.arm,
        request.formation,
    )
    .map_err(|error| {
        refusal(
            &hypothesis_id,
            CandidateFailureStage::SceneConstruction,
            error,
        )
    })?;
    let opaque_faces = candidate
        .scene
        .graph
        .faces
        .iter()
        .filter(|face| matches!(face.paint, Paint::OpaqueSolid(_)))
        .count();
    let prior = priors(request.model, opaque_faces, request.intent, request.config);
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
        cached.optimizer.clone()
    } else {
        let base = vice_verify::preseal_scene(
            &candidate.scene,
            &candidate.bindings,
            request.config.verification,
        )
        .map_err(|error| refusal(&hypothesis_id, CandidateFailureStage::Preseal, error))?;
        let (optimized, optimizer): (_, OptimizationResult) = optimize_paint(
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
        cache.optimized_by_scene_and_prior.insert(
            optimization_key,
            OptimizedScene {
                scene: candidate.scene.clone(),
                optimizer: optimizer.clone(),
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
        topology_class: request.arm.class.clone(),
        formation_class: request.formation_class.clone(),
        total_bits: score.total_bits,
    };
    let summary = CandidateSummary {
        hypothesis_id: request.hypothesis_id,
        topology_class: request.arm.class.clone(),
        formation_class: request.formation_class,
        scene_digest_sha256,
        delivery_digest,
        score,
        pre_quantization: verified.pre_quantization_certificate().clone(),
        post_quantization: verified.post_quantization_certificate().clone(),
        delivery_seal: seal.clone(),
        optimizer,
    };
    let seal_json = serde_json::to_vec(&seal).map_err(|error| {
        refusal(
            &hypothesis_id,
            CandidateFailureStage::CanonicalArtifact,
            error,
        )
    })?;
    let mut candidate = MaterializedCandidate {
        summary,
        score: scored,
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
