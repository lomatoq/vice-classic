//! Bounded experimental M8 preview.
//!
//! This path deliberately does not enter the exact alternation or release
//! sealing courts. It produces one visibly non-production candidate from a
//! deterministic proxy so the browser cannot spend minutes evaluating grid
//! refinements merely to show a preview.

use serde::Serialize;
use web_time::Instant;

use vice_evidence::{MulticolorConfig, MULTICOLOR_CONFIG_V1};
use vice_image::{CanonicalImage, ImageError};
use vice_ir::{canonical_scene_bytes, Canvas, ValidatedScene, VectorScene};
use vice_render::{render_partition, PartitionRender, RenderOptions};
use vice_svg::{build_export_plan, canonical_export_plan_bytes, materialize_svg, SvgProfile};

use super::{
    materialize_multiregion_seed, propose_from_image_config, MultiregionMaterializeError,
    MultiregionSeedError,
};

pub const M8_PREVIEW_SCHEMA: &str = "vice-classic/m8-preview/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct M8PreviewConfig {
    pub max_proxy_dimension_px: u32,
    pub max_palette_hypotheses: usize,
    pub max_blend_hypotheses: usize,
    pub geometry_refinement_rounds: usize,
    pub max_vertex_trials: usize,
    pub perform_release_seal: bool,
}

impl M8PreviewConfig {
    pub fn fast() -> Self {
        Self {
            max_proxy_dimension_px: 256,
            max_palette_hypotheses: 1,
            max_blend_hypotheses: 1,
            geometry_refinement_rounds: 0,
            max_vertex_trials: 0,
            perform_release_seal: false,
        }
    }

    pub fn quality() -> Self {
        Self {
            max_proxy_dimension_px: 384,
            max_palette_hypotheses: 2,
            max_blend_hypotheses: 2,
            geometry_refinement_rounds: 0,
            max_vertex_trials: 0,
            perform_release_seal: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct ProductPerformanceTrace {
    pub decode_ms: u64,
    pub proxy_build_ms: u64,
    pub route_flat2_probe_ms: u64,
    pub route_multicolor_probe_ms: u64,
    pub route_line_art_probe_ms: u64,
    pub route_gradient_probe_ms: u64,
    pub seed_generation_ms: u64,
    pub rag_dcel_ms: u64,
    pub boundary_chain_extraction_ms: u64,
    pub curve_fitting_ms: u64,
    pub paint_fitting_ms: u64,
    pub base_candidate_count: u64,
    pub exact_candidate_count: u64,
    pub vertex_trial_count: u64,
    pub render_ms: u64,
    pub likelihood_ms: u64,
    pub delivery_ms: u64,
    pub total_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct M8PreviewGeometry {
    pub source_pixels: u64,
    pub proxy_pixels: u64,
    pub palette_hypotheses: u64,
    pub seed_candidates: u64,
    pub rag_regions: u64,
    pub dcel_boundaries: u64,
    pub grid_vertices: u64,
    pub shared_boundary_chains: u64,
    pub final_segments: u64,
    pub final_anchors: u64,
    pub line_count: u64,
    pub arc_count: u64,
    pub quad_count: u64,
    pub cubic_count: u64,
    pub unit_length_axis_aligned_segment_count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct M8PreviewReport {
    pub schema: &'static str,
    pub source_sha256: String,
    pub production: bool,
    pub experimental: bool,
    pub source_width_px: u32,
    pub source_height_px: u32,
    pub proxy_width_px: u32,
    pub proxy_height_px: u32,
    pub source_to_proxy_scale_x: f64,
    pub source_to_proxy_scale_y: f64,
    pub resampler: &'static str,
    pub selected_seed_id: String,
    pub geometry: M8PreviewGeometry,
    pub performance: ProductPerformanceTrace,
}

#[derive(Debug, Clone, PartialEq)]
pub struct M8PreviewArtifacts {
    pub scene: VectorScene,
    pub proxy_render: PartitionRender,
    pub report: M8PreviewReport,
    pub result_svg: Vec<u8>,
    pub pure_svg: Vec<u8>,
    pub scene_json: Vec<u8>,
    pub plan_json: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum M8PreviewError {
    #[error(transparent)]
    Image(#[from] ImageError),
    #[error(transparent)]
    Seed(#[from] MultiregionSeedError),
    #[error(transparent)]
    Materialize(#[from] MultiregionMaterializeError),
    #[error("preview render failed: {0}")]
    Render(#[from] vice_render::RenderError),
    #[error("preview scene is invalid: {0}")]
    Scene(#[from] vice_ir::SceneError),
    #[error("preview export plan failed: {0}")]
    Export(#[from] vice_svg::ExportPlanError),
    #[error("preview SVG failed: {0}")]
    Svg(#[from] vice_svg::SvgMaterializationError),
    #[error("preview report serialization failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("blank_artifact_guard: nonblank input produced no visible preview pixels")]
    BlankArtifact,
    #[error("M8 preview produced no seed candidate")]
    NoCandidate,
}

pub fn preview_multiregion(
    source: &CanonicalImage,
    cfg: M8PreviewConfig,
    mut performance: ProductPerformanceTrace,
) -> Result<M8PreviewArtifacts, M8PreviewError> {
    let total_started = Instant::now();
    let proxy_started = Instant::now();
    let proxy = proxy_image(source, cfg.max_proxy_dimension_px)?;
    performance.proxy_build_ms = elapsed_ms(proxy_started);

    let seed_started = Instant::now();
    let multicolor_cfg = MulticolorConfig {
        beam_width: cfg.max_palette_hypotheses.max(1),
        ..MULTICOLOR_CONFIG_V1
    };
    let blend_spaces = if cfg.max_blend_hypotheses > 1 {
        &[
            vice_ir::BlendSpace::LinearLight,
            vice_ir::BlendSpace::EncodedSrgb,
        ][..]
    } else {
        &[vice_ir::BlendSpace::LinearLight][..]
    };
    let seeds = propose_from_image_config(
        &proxy,
        &vice_opt::MULTIREGION_PAINT_CONFIG_V1,
        &multicolor_cfg,
        blend_spaces,
    )?;
    performance.seed_generation_ms = elapsed_ms(seed_started);
    let selected = seeds
        .seeds
        .iter()
        .min_by(|a, b| {
            a.palette_score
                .total_proposal_bits
                .total_cmp(&b.palette_score.total_proposal_bits)
                .then_with(|| a.id.cmp(&b.id))
        })
        .ok_or(M8PreviewError::NoCandidate)?;
    performance.base_candidate_count = 1;
    performance.exact_candidate_count = 0;
    performance.vertex_trial_count = 0;

    let grid_vertices = selected
        .dcel
        .boundaries
        .iter()
        .flat_map(|boundary| [boundary.start, boundary.end])
        .collect::<std::collections::BTreeSet<_>>()
        .len() as u64;
    let unit_axis_aligned = selected
        .dcel
        .boundaries
        .iter()
        .filter(|boundary| {
            let dx = boundary.start.0.abs_diff(boundary.end.0);
            let dy = boundary.start.1.abs_diff(boundary.end.1);
            dx + dy == 1
        })
        .count() as u64;

    let render_started = Instant::now();
    let proxy_scene = materialize_multiregion_seed(selected)?;
    let proxy_render = render_partition(&proxy_scene, &RenderOptions::default())?;
    performance.render_ms = elapsed_ms(render_started);
    if source_has_visible_variation(source) && !render_has_visible_variation(&proxy_render) {
        return Err(M8PreviewError::BlankArtifact);
    }

    let delivery_started = Instant::now();
    let scaled = scale_scene_to_source(
        proxy_scene.into_inner(),
        source.width_px(),
        source.height_px(),
    );
    let scaled = ValidatedScene::new(scaled)?;
    let plan = build_export_plan(scaled.scene(), 4, 0.01)?;
    let plan_json = canonical_export_plan_bytes(&plan)?;
    let pure_svg = materialize_svg(&plan, SvgProfile::PurePartition)?;
    let result_svg = materialize_svg(&plan, SvgProfile::SeamSafe)?;
    let scene_json = canonical_scene_bytes(scaled.scene())?;
    performance.delivery_ms = elapsed_ms(delivery_started);

    let boundary_count = selected.dcel.boundaries.len() as u64;
    let geometry = M8PreviewGeometry {
        source_pixels: source.pixel_count() as u64,
        proxy_pixels: proxy.pixel_count() as u64,
        palette_hypotheses: seeds.palette_hypotheses,
        seed_candidates: seeds.seeds.len() as u64,
        rag_regions: selected.rag.nodes.len() as u64,
        dcel_boundaries: boundary_count,
        grid_vertices,
        shared_boundary_chains: 0,
        final_segments: boundary_count,
        final_anchors: grid_vertices,
        line_count: boundary_count,
        arc_count: 0,
        quad_count: 0,
        cubic_count: 0,
        unit_length_axis_aligned_segment_count: unit_axis_aligned,
    };
    performance.total_ms = elapsed_ms(total_started)
        .saturating_add(performance.decode_ms)
        .saturating_add(performance.route_flat2_probe_ms)
        .saturating_add(performance.route_multicolor_probe_ms)
        .saturating_add(performance.route_line_art_probe_ms)
        .saturating_add(performance.route_gradient_probe_ms);
    let report = M8PreviewReport {
        schema: M8_PREVIEW_SCHEMA,
        source_sha256: source.source_sha256().to_string(),
        production: false,
        experimental: true,
        source_width_px: source.width_px(),
        source_height_px: source.height_px(),
        proxy_width_px: proxy.width_px(),
        proxy_height_px: proxy.height_px(),
        source_to_proxy_scale_x: f64::from(proxy.width_px()) / f64::from(source.width_px()),
        source_to_proxy_scale_y: f64::from(proxy.height_px()) / f64::from(source.height_px()),
        resampler: "deterministic_box_rgba8/v1",
        selected_seed_id: selected.id.clone(),
        geometry,
        performance,
    };
    Ok(M8PreviewArtifacts {
        scene: scaled.into_inner(),
        proxy_render,
        report,
        result_svg,
        pure_svg,
        scene_json,
        plan_json,
    })
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
}

fn proxy_image(source: &CanonicalImage, max_dimension: u32) -> Result<CanonicalImage, ImageError> {
    let source_max = source.width_px().max(source.height_px());
    if source_max <= max_dimension {
        return Ok(source.clone());
    }
    let width = ((u64::from(source.width_px()) * u64::from(max_dimension)) / u64::from(source_max))
        .max(1) as u32;
    let height = ((u64::from(source.height_px()) * u64::from(max_dimension))
        / u64::from(source_max))
    .max(1) as u32;
    let mut output = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        let y0 = (u64::from(y) * u64::from(source.height_px()) / u64::from(height)) as u32;
        let y1 = ((u64::from(y + 1) * u64::from(source.height_px())).div_ceil(u64::from(height))
            as u32)
            .max(y0 + 1)
            .min(source.height_px());
        for x in 0..width {
            let x0 = (u64::from(x) * u64::from(source.width_px()) / u64::from(width)) as u32;
            let x1 = ((u64::from(x + 1) * u64::from(source.width_px())).div_ceil(u64::from(width))
                as u32)
                .max(x0 + 1)
                .min(source.width_px());
            let mut sum = [0u64; 4];
            let mut count = 0u64;
            for source_y in y0..y1 {
                for source_x in x0..x1 {
                    let pixel = source.pixel(source.index(source_x, source_y));
                    for channel in 0..4 {
                        sum[channel] += u64::from(pixel[channel]);
                    }
                    count += 1;
                }
            }
            for channel in sum {
                output.push(((channel + count / 2) / count) as u8);
            }
        }
    }
    CanonicalImage::from_straight_srgb8(
        width,
        height,
        output,
        source.source_had_alpha(),
        source.icc_assumption(),
    )
}

fn scale_scene_to_source(mut scene: VectorScene, width: u32, height: u32) -> VectorScene {
    let scale_x = f64::from(width) / f64::from(scene.canvas.width_px);
    let scale_y = f64::from(height) / f64::from(scene.canvas.height_px);
    for vertex in &mut scene.graph.vertices {
        vertex.pos.x *= scale_x;
        vertex.pos.y *= scale_y;
    }
    scene.canvas = Canvas {
        width_px: width,
        height_px: height,
    };
    scene
}

fn source_has_visible_variation(source: &CanonicalImage) -> bool {
    let mut low = [u8::MAX; 4];
    let mut high = [u8::MIN; 4];
    for pixel in source.straight_srgb8().chunks_exact(4) {
        for channel in 0..4 {
            low[channel] = low[channel].min(pixel[channel]);
            high[channel] = high[channel].max(pixel[channel]);
        }
    }
    low.into_iter()
        .zip(high)
        .any(|(low, high)| high.saturating_sub(low) > 4)
}

fn render_has_visible_variation(render: &PartitionRender) -> bool {
    let mut low = [f64::INFINITY; 4];
    let mut high = [f64::NEG_INFINITY; 4];
    for pixel in &render.composite {
        let channels = if pixel.a > 1e-12 {
            [
                pixel.r / pixel.a,
                pixel.g / pixel.a,
                pixel.b / pixel.a,
                pixel.a,
            ]
        } else {
            [0.0, 0.0, 0.0, 0.0]
        };
        for channel in 0..4 {
            low[channel] = low[channel].min(channels[channel]);
            high[channel] = high[channel].max(channels[channel]);
        }
    }
    low.into_iter()
        .zip(high)
        .any(|(low, high)| high - low > 4.0 / 255.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vice_image::IccAssumption;

    #[test]
    fn proxy_never_upscales_and_bounds_the_long_side() {
        let image = CanonicalImage::from_straight_srgb8(
            580,
            387,
            vec![255; 580 * 387 * 4],
            false,
            IccAssumption::NoProfileAssumedSrgb,
        )
        .unwrap();
        let proxy = proxy_image(&image, 256).unwrap();
        assert_eq!((proxy.width_px(), proxy.height_px()), (256, 170));
        let tiny = proxy_image(&proxy, 512).unwrap();
        assert_eq!(tiny, proxy);
    }

    #[test]
    fn preview_budget_cannot_enter_exact_or_vertex_refinement() {
        let cfg = M8PreviewConfig::fast();
        assert_eq!(cfg.max_palette_hypotheses, 1);
        assert_eq!(cfg.max_blend_hypotheses, 1);
        assert_eq!(cfg.geometry_refinement_rounds, 0);
        assert_eq!(cfg.max_vertex_trials, 0);
        assert!(!cfg.perform_release_seal);
    }

    #[test]
    fn blank_guard_requires_visible_output_variation_for_nonblank_input() {
        let image = CanonicalImage::from_straight_srgb8(
            2,
            1,
            vec![255, 255, 255, 255, 20, 40, 60, 255],
            false,
            IccAssumption::NoProfileAssumedSrgb,
        )
        .unwrap();
        assert!(source_has_visible_variation(&image));
        let blank = PartitionRender {
            width_px: 2,
            height_px: 1,
            face_coverage: Vec::new(),
            composite: vec![
                vice_ir::color::PremulRgba {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 1.0,
                };
                2
            ],
        };
        assert!(!render_has_visible_variation(&blank));
    }
}
