//! Exact common-court selection for M8 multiregion candidates.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use sha2::{Digest, Sha256};
use vice_geom::Pt;
use vice_image::{CanonicalImage, DecodeLimits, ObservationTensor};
use vice_ir::color::srgb_u8_to_linear;
use vice_ir::{scene_digest_sha256, FaceId, LinearRgb, Paint, ValidatedScene, VectorScene};
use vice_opt::{
    model_universe_hash, posterior_with_search_mass, run_exact_alternation, AlternationCandidate,
    AlternationConfig, AlternationError, AlternationResult, BlockLikelihoodConfig, LikelihoodError,
    PaintFitError, PriorCodeLengths, ScoredHypothesis, SearchMassCertificate, SearchMassInput,
    SupportedModelUniverseV1, UnexploredMassInput, MULTIREGION_PAINT_CONFIG_V1,
};
use vice_render::{render_digest_sha256, render_partition, PartitionRender, RenderOptions};

use super::{
    materialize_multiregion_seed, propose_multiregion_seeds, MultiregionMaterializeError,
    MultiregionSeed, MultiregionSeedError,
};

pub const M8_EXACT_SCHEMA: &str = "vice-classic/m8-exact-court/v1";
const M8_PRIOR_SCHEMA: &str = "vice-classic/m8-grid-partition-prior/v1";
const M8_BACKEND_SCHEMA: &str = "vice-classic/m8/backend/v1";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct M8ExactConfig {
    pub likelihood: BlockLikelihoodConfig,
    pub render: RenderOptions,
    pub alternation: AlternationConfig,
    /// Symmetric displacement tried for a shared non-canvas graph vertex.
    pub vertex_step_px: f64,
    /// Deterministic cap per refinement round. Exhaustion remains visible in
    /// the search-mass certificate; wall time never changes membership.
    pub max_vertex_trials_per_round: usize,
}

impl Default for M8ExactConfig {
    fn default() -> Self {
        Self {
            // M8 has its own clean multiregion court. Reusing M7's broad
            // Flat2 residual scale made deleting an entire fourth face
            // cheaper than describing it; the expanded universe must carry
            // its own likelihood identity and calibration.
            likelihood: BlockLikelihoodConfig::new(2, 2.0, [4.0 / 255.0; 4], 4.0)
                .expect("static M8 likelihood"),
            render: RenderOptions::default(),
            alternation: AlternationConfig {
                max_rounds: 3,
                min_exact_improvement_bits: 1e-9,
            },
            vertex_step_px: 0.25,
            max_vertex_trials_per_round: 64,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct M8CandidateSummary {
    pub id: String,
    pub seed_id: String,
    pub palette_cardinality: u64,
    pub opaque_modes_seen: u64,
    pub palette_digest_sha256: String,
    pub rag_digest_sha256: String,
    pub scene_digest_sha256: String,
    pub render_digest_sha256: String,
    pub paint_digest_sha256: String,
    pub blend_space: String,
    pub exact_total_bits: f64,
    pub exact_pixel_bits: f64,
    pub exact_blocks: u64,
    pub pixel_bits_per_block: f64,
    pub visible_faces: u64,
    pub visible_components: u64,
    pub junctions: u64,
    pub selection_class: String,
    pub exact_rerendered: bool,
    pub geometry_refinement_depth: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct M8ExactReport {
    pub schema: &'static str,
    pub source_sha256: String,
    pub model_universe_hash: String,
    pub config_sha256: String,
    pub pricing_sha256: String,
    pub backend_sha256: String,
    /// Exact selection is production arithmetic, but release admission stays
    /// false until M8 calibration and delivery sealing are attached.
    pub production_admitted: bool,
    pub admission_authority_sha256: Option<String>,
    pub seed_candidates: u64,
    pub exact_candidates_evaluated: u64,
    pub selected: M8CandidateSummary,
    pub alternation: AlternationResult,
    pub search_mass: SearchMassCertificate,
}

#[derive(Debug, Clone, PartialEq)]
pub struct M8SolvedCandidate {
    pub scene: VectorScene,
    pub render: PartitionRender,
    pub report: M8ExactReport,
}

#[derive(Debug, thiserror::Error)]
pub enum M8ExactError {
    #[error(transparent)]
    Decode(#[from] vice_image::ImageError),
    #[error(transparent)]
    Seed(#[from] MultiregionSeedError),
    #[error(transparent)]
    Materialize(#[from] MultiregionMaterializeError),
    #[error(transparent)]
    Render(#[from] vice_render::RenderError),
    #[error(transparent)]
    Paint(#[from] PaintFitError),
    #[error(transparent)]
    Likelihood(#[from] LikelihoodError),
    #[error("exact M8 scene is invalid: {0}")]
    Scene(#[from] vice_ir::SceneError),
    #[error(transparent)]
    Alternation(#[from] AlternationError),
    #[error(transparent)]
    Posterior(#[from] vice_opt::PosteriorError),
    #[error("M8 exact configuration is malformed")]
    InvalidConfig,
    #[error("M8 exact court produced no candidate")]
    NoCandidate,
    #[error("alternation selected an unavailable exact scene")]
    MissingSelectedScene,
}

#[derive(Debug, Clone)]
struct ExactCandidate {
    scene: VectorScene,
    render: PartitionRender,
    summary: M8CandidateSummary,
    topology_class: String,
    formation_class: String,
}

impl ExactCandidate {
    fn alternation(&self, universe_hash: &str) -> AlternationCandidate {
        AlternationCandidate {
            id: self.summary.id.clone(),
            universe_hash: universe_hash.to_string(),
            palette_digest: self.summary.palette_digest_sha256.clone(),
            partition_digest: self.summary.render_digest_sha256.clone(),
            paint_digest: self.summary.paint_digest_sha256.clone(),
            exact_total_bits: self.summary.exact_total_bits,
            exact_rerendered: true,
        }
    }

    fn scored(&self) -> ScoredHypothesis {
        ScoredHypothesis {
            hypothesis_id: self.summary.id.clone(),
            delivery_digest: self.summary.scene_digest_sha256.clone(),
            topology_class: self.topology_class.clone(),
            formation_class: self.formation_class.clone(),
            total_bits: self.summary.exact_total_bits,
        }
    }
}

pub fn solve_multiregion_exact(
    png_bytes: &[u8],
    cfg: &M8ExactConfig,
) -> Result<M8SolvedCandidate, M8ExactError> {
    validate_config(cfg)?;
    let image = CanonicalImage::decode(png_bytes, &DecodeLimits::default())?;
    let seed_report = propose_multiregion_seeds(png_bytes)?;
    let universe_hash = model_universe_hash(&SupportedModelUniverseV1::m8());
    let config_sha256 = config_digest(cfg);
    let pricing_sha256 = hex::encode(Sha256::digest(M8_PRIOR_SCHEMA.as_bytes()));
    let backend_sha256 = hex::encode(Sha256::digest(
        format!(
            "{}|{}|{}",
            M8_BACKEND_SCHEMA,
            vice_render::RENDER_DIGEST_SCHEMA,
            env!("CARGO_PKG_VERSION")
        )
        .as_bytes(),
    ));

    let mut candidates = BTreeMap::<String, ExactCandidate>::new();
    let mut base_ids = Vec::new();
    for seed in &seed_report.seeds {
        let scene = materialize_multiregion_seed(seed)?.into_inner();
        let candidate = exact_refit_and_score(scene, seed, &image, cfg, 0)?;
        base_ids.push(candidate.summary.id.clone());
        candidates.insert(candidate.summary.id.clone(), candidate);
    }
    if base_ids.is_empty() {
        return Err(M8ExactError::NoCandidate);
    }
    base_ids.sort();

    // A genuine exact candidate is the initial state. Picking the worst base
    // makes round zero compare the complete palette/formation seed court; no
    // fabricated sentinel score enters the trace.
    let initial_id = base_ids
        .iter()
        .max_by(|a, b| {
            candidates[*a]
                .summary
                .exact_total_bits
                .total_cmp(&candidates[*b].summary.exact_total_bits)
                .then_with(|| a.cmp(b))
        })
        .cloned()
        .ok_or(M8ExactError::NoCandidate)?;
    let initial = candidates[&initial_id].alternation(&universe_hash);

    let alternation = run_exact_alternation(initial, cfg.alternation, |parent, round| {
        let parent_candidate = candidates
            .get(&parent.id)
            .cloned()
            .ok_or_else(|| "current exact scene is absent".to_string())?;
        if round == 0 && base_ids.len() > 1 {
            return Ok(base_ids
                .iter()
                .map(|id| candidates[id].alternation(&universe_hash))
                .collect());
        }
        let seed = seed_report
            .seeds
            .iter()
            .find(|seed| seed.id == parent_candidate.summary.seed_id)
            .ok_or_else(|| "selected seed is absent".to_string())?;
        let refined = refine_shared_vertices(&parent_candidate, seed, &image, cfg)
            .map_err(|error| error.to_string())?;
        let rows = refined
            .into_iter()
            .map(|candidate| {
                let row = candidate.alternation(&universe_hash);
                candidates
                    .entry(candidate.summary.id.clone())
                    .or_insert(candidate);
                row
            })
            .collect();
        Ok(rows)
    })?;

    let selected = candidates
        .get(&alternation.winner.id)
        .cloned()
        .ok_or(M8ExactError::MissingSelectedScene)?;
    let identity = vice_opt::ModelIdentity::new(
        universe_hash.clone(),
        pricing_sha256.clone(),
        backend_sha256.clone(),
        config_sha256.clone(),
    )?;
    let search_mass = posterior_with_search_mass(SearchMassInput {
        identity,
        explored_kept: candidates.values().map(ExactCandidate::scored).collect(),
        budget_pruned: Vec::new(),
        topology_classes_upper_bound: candidates
            .values()
            .map(|candidate| candidate.topology_class.as_str())
            .collect::<BTreeSet<_>>()
            .len() as u64,
        formation_classes_upper_bound: candidates
            .values()
            .map(|candidate| candidate.formation_class.as_str())
            .collect::<BTreeSet<_>>()
            .len() as u64,
        // Merge/split and finer geometry proposals are deliberately visible,
        // not silently assigned zero posterior mass before M8 calibration.
        unexplored: UnexploredMassInput::Unknown,
    })?;
    let report = M8ExactReport {
        schema: M8_EXACT_SCHEMA,
        source_sha256: image.source_sha256().to_string(),
        model_universe_hash: universe_hash,
        config_sha256,
        pricing_sha256,
        backend_sha256,
        production_admitted: false,
        admission_authority_sha256: None,
        seed_candidates: seed_report.seeds.len() as u64,
        exact_candidates_evaluated: candidates.len() as u64,
        selected: selected.summary.clone(),
        alternation,
        search_mass,
    };
    Ok(M8SolvedCandidate {
        scene: selected.scene,
        render: selected.render,
        report,
    })
}

/// Re-enter the exact M8 court after a P1 script has rebuilt the partition.
/// The edited seed's explicit paints are scored as written; they are not
/// silently replaced by a fresh proposal fit.
pub(crate) fn solve_edited_multiregion_seed(
    image: &CanonicalImage,
    seed: &MultiregionSeed,
    cfg: &M8ExactConfig,
) -> Result<M8SolvedCandidate, M8ExactError> {
    validate_config(cfg)?;
    let scene = materialize_multiregion_seed(seed)?.into_inner();
    let selected = exact_score_with_paint_fit(scene, seed, image, cfg, seed.paint_fit.clone(), 0)?;
    let universe_hash = model_universe_hash(&SupportedModelUniverseV1::m8());
    let config_sha256 = config_digest(cfg);
    let pricing_sha256 = hex::encode(Sha256::digest(M8_PRIOR_SCHEMA.as_bytes()));
    let backend_sha256 = hex::encode(Sha256::digest(
        format!(
            "{}|{}|{}",
            M8_BACKEND_SCHEMA,
            vice_render::RENDER_DIGEST_SCHEMA,
            env!("CARGO_PKG_VERSION")
        )
        .as_bytes(),
    ));
    let alternation = run_exact_alternation(
        selected.alternation(&universe_hash),
        cfg.alternation,
        |_, _| Ok(Vec::new()),
    )?;
    let identity = vice_opt::ModelIdentity::new(
        universe_hash.clone(),
        pricing_sha256.clone(),
        backend_sha256.clone(),
        config_sha256.clone(),
    )?;
    let search_mass = posterior_with_search_mass(SearchMassInput {
        identity,
        explored_kept: vec![selected.scored()],
        budget_pruned: Vec::new(),
        topology_classes_upper_bound: 1,
        formation_classes_upper_bound: 1,
        unexplored: UnexploredMassInput::Unknown,
    })?;
    let report = M8ExactReport {
        schema: M8_EXACT_SCHEMA,
        source_sha256: image.source_sha256().to_string(),
        model_universe_hash: universe_hash,
        config_sha256,
        pricing_sha256,
        backend_sha256,
        production_admitted: false,
        admission_authority_sha256: None,
        seed_candidates: 1,
        exact_candidates_evaluated: 1,
        selected: selected.summary.clone(),
        alternation,
        search_mass,
    };
    Ok(M8SolvedCandidate {
        scene: selected.scene,
        render: selected.render,
        report,
    })
}

fn validate_config(cfg: &M8ExactConfig) -> Result<(), M8ExactError> {
    if cfg.alternation.max_rounds == 0
        || !cfg.alternation.min_exact_improvement_bits.is_finite()
        || cfg.alternation.min_exact_improvement_bits < 0.0
        || !cfg.vertex_step_px.is_finite()
        || !(0.0..=0.5).contains(&cfg.vertex_step_px)
        || cfg.vertex_step_px == 0.0
        || cfg.max_vertex_trials_per_round == 0
    {
        return Err(M8ExactError::InvalidConfig);
    }
    Ok(())
}

fn exact_refit_and_score(
    scene: VectorScene,
    seed: &MultiregionSeed,
    image: &CanonicalImage,
    cfg: &M8ExactConfig,
    geometry_refinement_depth: u32,
) -> Result<ExactCandidate, M8ExactError> {
    let validated = ValidatedScene::new(scene)?;
    let proposal_render = render_partition(&validated, &cfg.render)?;
    let observation = ObservationTensor::of(image, validated.scene().formation.blend_space);
    let interior =
        vice_evidence::interior_confidence(&observation, &vice_evidence::INTERIOR_CONFIG_V1);
    let evidence_weights = (0..observation.len())
        .map(|pixel| interior.weight(pixel))
        .collect::<Vec<_>>();
    let paint_fit = vice_opt::fit_opaque_face_paints_weighted(
        &observation,
        &proposal_render,
        Some(FaceId(0)),
        &evidence_weights,
        &MULTIREGION_PAINT_CONFIG_V1,
    )?;
    exact_score_with_paint_fit(
        validated.into_inner(),
        seed,
        image,
        cfg,
        paint_fit,
        geometry_refinement_depth,
    )
}

fn exact_score_with_paint_fit(
    mut fitted: VectorScene,
    seed: &MultiregionSeed,
    image: &CanonicalImage,
    cfg: &M8ExactConfig,
    paint_fit: vice_opt::PaintFit,
    geometry_refinement_depth: u32,
) -> Result<ExactCandidate, M8ExactError> {
    for paint in &paint_fit.paints {
        let rgb = paint.quantized_srgb8;
        fitted.graph.faces[paint.face.index()].paint = Paint::OpaqueSolid(LinearRgb::new(
            srgb_u8_to_linear(rgb[0]),
            srgb_u8_to_linear(rgb[1]),
            srgb_u8_to_linear(rgb[2]),
        ));
    }
    let validated = ValidatedScene::new(fitted)?;
    let render = render_partition(&validated, &cfg.render)?;
    let priors = partition_priors(seed, &validated, &paint_fit);
    let score =
        vice_opt::score_full_resolution(validated.scene(), image, &render, cfg.likelihood, priors)?;
    let scene = validated.into_inner();
    let scene_digest = scene_digest_sha256(&scene)?;
    let render_digest = render_digest_sha256(&render);
    let id_material = format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}",
        M8_EXACT_SCHEMA, seed.id, scene_digest, paint_fit.digest_sha256
    );
    let id_digest = hex::encode(Sha256::digest(id_material.as_bytes()));
    let formation_class = format!(
        "{}:{}",
        seed.blend_space,
        if seed.exterior_is_transparent {
            "transparent"
        } else {
            "opaque"
        }
    );
    let visible_faces = seed.rag.nodes.len().saturating_sub(1) as u64;
    let visible_components = visible_component_count(seed);
    let junctions = seed
        .dcel
        .junctions
        .iter()
        .filter(|junction| {
            junction
                .incident_regions
                .iter()
                .filter(|region| region.0 != 0)
                .collect::<BTreeSet<_>>()
                .len()
                >= 3
        })
        .count() as u64;
    let selection_class = format!(
        "m{}/k{}/f{visible_faces}/c{visible_components}/j{junctions}/{}",
        seed.opaque_modes_seen, seed.palette_cardinality, formation_class
    );
    Ok(ExactCandidate {
        scene,
        render,
        summary: M8CandidateSummary {
            id: format!("M8/exact/{}", &id_digest[..16]),
            seed_id: seed.id.clone(),
            palette_cardinality: seed.palette_cardinality,
            opaque_modes_seen: seed.opaque_modes_seen,
            palette_digest_sha256: seed.palette_digest_sha256.clone(),
            rag_digest_sha256: seed.rag.digest_sha256.clone(),
            scene_digest_sha256: scene_digest,
            render_digest_sha256: render_digest,
            paint_digest_sha256: paint_fit.digest_sha256,
            blend_space: seed.blend_space.to_string(),
            exact_total_bits: score.total_bits,
            exact_pixel_bits: score.pixel_bits,
            exact_blocks: score.diagnostics.blocks,
            pixel_bits_per_block: score.pixel_bits / score.diagnostics.blocks.max(1) as f64,
            visible_faces,
            visible_components,
            junctions,
            selection_class,
            exact_rerendered: true,
            geometry_refinement_depth,
        },
        topology_class: seed.rag.digest_sha256.clone(),
        formation_class,
    })
}

fn visible_component_count(seed: &MultiregionSeed) -> u64 {
    let visible = seed
        .rag
        .nodes
        .iter()
        .filter(|node| !node.is_exterior)
        .map(|node| node.id)
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    let mut components = 0u64;
    for &start in &visible {
        if !seen.insert(start) {
            continue;
        }
        components += 1;
        let mut stack = vec![start];
        while let Some(region) = stack.pop() {
            for neighbour in seed.rag.neighbours(region) {
                if visible.contains(&neighbour) && seen.insert(neighbour) {
                    stack.push(neighbour);
                }
            }
        }
    }
    components
}

fn partition_priors(
    seed: &MultiregionSeed,
    scene: &ValidatedScene,
    paint_fit: &vice_opt::PaintFit,
) -> PriorCodeLengths {
    // A deterministic row-major prefix description: first region plus one
    // same/change bit per later cell and a changed-region symbol. It prices
    // fragmented partitions without reusing the proposal reconstruction loss.
    let k = seed.rag.nodes.len().max(2) as f64;
    let mut topology_bits = k.log2();
    for labels in seed.rag.region_of_pixel.windows(2) {
        topology_bits += 1.0;
        if labels[0] != labels[1] {
            topology_bits += (k - 1.0).log2();
        }
    }
    PriorCodeLengths {
        topology_bits,
        // Shared grid topology determines the initial coordinates. Every
        // independently movable non-canvas vertex is explicitly charged.
        geometry_bits: scene
            .graph()
            .vertices
            .iter()
            .filter(|vertex| !on_canvas(vertex.pos, scene.scene()))
            .count() as f64
            * 32.0,
        paint_bits: paint_fit.total_paint_code_bits,
        relation_bits: 0.0,
        formation_bits: 2.0,
    }
}

fn refine_shared_vertices(
    parent: &ExactCandidate,
    seed: &MultiregionSeed,
    image: &CanonicalImage,
    cfg: &M8ExactConfig,
) -> Result<Vec<ExactCandidate>, M8ExactError> {
    let mut out = Vec::new();
    let mut trials = 0usize;
    for vertex in 0..parent.scene.graph.vertices.len() {
        let p = parent.scene.graph.vertices[vertex].pos;
        if on_canvas(p, &parent.scene) {
            continue;
        }
        for delta in [
            Pt::new(-cfg.vertex_step_px, 0.0),
            Pt::new(cfg.vertex_step_px, 0.0),
            Pt::new(0.0, -cfg.vertex_step_px),
            Pt::new(0.0, cfg.vertex_step_px),
        ] {
            if trials >= cfg.max_vertex_trials_per_round {
                return Ok(out);
            }
            trials += 1;
            let mut proposed = parent.scene.clone();
            proposed.graph.vertices[vertex].pos = p + delta;
            let Ok(validated) = ValidatedScene::new(proposed) else {
                continue;
            };
            match exact_refit_and_score(
                validated.into_inner(),
                seed,
                image,
                cfg,
                parent.summary.geometry_refinement_depth + 1,
            ) {
                Ok(candidate) => out.push(candidate),
                Err(M8ExactError::Render(_) | M8ExactError::Scene(_)) => {}
                Err(error) => return Err(error),
            }
        }
    }
    Ok(out)
}

fn on_canvas(point: Pt, scene: &VectorScene) -> bool {
    point.x == 0.0
        || point.y == 0.0
        || point.x == f64::from(scene.canvas.width_px)
        || point.y == f64::from(scene.canvas.height_px)
}

#[derive(Serialize)]
struct ConfigIdentity {
    likelihood: BlockLikelihoodConfig,
    chord_tolerance_px: f64,
    render_sum_abs_tol: f64,
    render_range_tol: f64,
    max_abs_coord_px: f64,
    max_canvas_dim_px: u32,
    alternation: AlternationConfig,
    vertex_step_px: f64,
    max_vertex_trials_per_round: usize,
}

fn config_digest(cfg: &M8ExactConfig) -> String {
    let tolerances = cfg.render.tolerances();
    let domain = cfg.render.domain();
    let identity = ConfigIdentity {
        likelihood: cfg.likelihood,
        chord_tolerance_px: cfg.render.budget.chord_tolerance.px(),
        render_sum_abs_tol: tolerances.sum_abs_tol,
        render_range_tol: tolerances.range_tol,
        max_abs_coord_px: domain.max_abs_coord_px,
        max_canvas_dim_px: domain.max_canvas_dim_px,
        alternation: cfg.alternation,
        vertex_step_px: cfg.vertex_step_px,
        max_vertex_trials_per_round: cfg.max_vertex_trials_per_round,
    };
    hex::encode(Sha256::digest(
        serde_json::to_vec(&identity).expect("M8 config serializes"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png() -> Vec<u8> {
        let (w, h) = (12u32, 6u32);
        let colors = [[230, 20, 20], [20, 220, 30], [20, 40, 230]];
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                pixels[i..i + 3].copy_from_slice(&colors[(x / 4) as usize]);
                pixels[i + 3] = 255;
            }
        }
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, w, h);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&pixels).unwrap();
        }
        bytes
    }

    #[test]
    fn exact_court_scores_serialized_paints_and_keeps_unknown_mass_visible() {
        let result = solve_multiregion_exact(&png(), &M8ExactConfig::default()).unwrap();
        assert!(result.report.selected.exact_rerendered);
        assert!(result.report.selected.exact_total_bits.is_finite());
        assert!(!result.report.production_admitted);
        assert!(matches!(
            result.report.search_mass.unexplored_mass_upper_bound,
            vice_opt::BoundValue::Unknown
        ));
        assert_eq!(
            result.report.selected.scene_digest_sha256,
            scene_digest_sha256(&result.scene).unwrap()
        );
    }

    #[test]
    fn zero_work_budget_is_a_typed_refusal() {
        let cfg = M8ExactConfig {
            max_vertex_trials_per_round: 0,
            ..M8ExactConfig::default()
        };
        assert!(matches!(
            solve_multiregion_exact(&png(), &cfg),
            Err(M8ExactError::InvalidConfig)
        ));
    }

    #[test]
    fn multicolor_paint_transaction_roi_matches_the_full_exact_court() {
        let bytes = png();
        let cfg = M8ExactConfig::default();
        let result = solve_multiregion_exact(&bytes, &cfg).unwrap();
        let parent = ValidatedScene::new(result.scene.clone()).unwrap();
        let mut child_scene = result.scene.clone();
        child_scene.graph.faces[1].paint = Paint::OpaqueSolid(LinearRgb::new(0.5, 0.1, 0.1));
        let child = ValidatedScene::new(child_scene).unwrap();
        let affected = result.render.face_coverage[1]
            .iter()
            .enumerate()
            .filter(|(_, coverage)| **coverage != 0.0)
            .map(|(pixel, _)| pixel as u64)
            .collect::<Vec<_>>();
        let zero_priors = PriorCodeLengths {
            topology_bits: 0.0,
            geometry_bits: 0.0,
            paint_bits: 0.0,
            relation_bits: 0.0,
            formation_bits: 0.0,
        };
        let image = CanonicalImage::decode_png(&bytes, &DecodeLimits::default()).unwrap();
        let certificate = vice_opt::certify_exact_roi_transaction(
            &parent,
            &child,
            &image,
            &affected,
            &cfg.render,
            cfg.likelihood,
            zero_priors,
            zero_priors,
            1,
        )
        .unwrap();
        assert!(certificate.roi_render_matches_full_slice);
        assert!(certificate.preference_matches_full);
        assert!((certificate.full_delta_bits - certificate.roi_delta_bits).abs() <= 1e-9);
    }
}
