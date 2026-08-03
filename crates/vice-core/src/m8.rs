//! Integrated M8 evidence -> RAG -> shared-DCEL -> paint seed path.
//!
//! This is intentionally a seed report, not delivery.  It connects every
//! palette/exterior/formation reading to concrete topology and per-face paint
//! evidence while `production` and `exact_rerendered` remain false.  Only the
//! later exact alternation/verification path may promote one of these seeds.

use serde::Serialize;
use sha2::{Digest, Sha256};
use vice_evidence::{
    interior_confidence, propose_multicolor, MulticolorRefusal, PaletteScore, INTERIOR_CONFIG_V1,
    MULTICOLOR_CONFIG_V1, PALETTE_CONFIG_V1,
};
use vice_image::{CanonicalImage, DecodeLimits, ObservationTensor};
use vice_ir::color::PremulRgba;
use vice_ir::{BlendSpace, FaceId};
use vice_opt::{
    fit_opaque_face_paints, model_universe_hash, MultiregionPaintConfig, PaintFit, PaintFitError,
    SupportedModelUniverseV1, MULTIREGION_PAINT_CONFIG_V1,
};
use vice_render::PartitionRender;
use vice_topology::{
    MultiDcelError, MulticolorDcel, RagError, RegionAdjacencyGraph, RegionLabelling,
};

#[path = "m8/materialize.rs"]
mod materialize;
pub use materialize::{
    materialize_multiregion_seed, multiregion_boundary_bindings, MultiregionMaterializeError,
};
#[path = "m8/production.rs"]
mod production;
pub use production::{
    solve_multiregion_exact, M8CandidateSummary, M8ExactConfig, M8ExactError, M8ExactReport,
    M8SolvedCandidate, M8_EXACT_SCHEMA,
};
#[path = "m8/delivery.rs"]
mod delivery;
pub use delivery::{
    admit_multiregion_delivery, load_committed_m8_production_policy, seal_multiregion_delivery,
    M8DeliveryArtifacts, M8DeliveryConfig, M8DeliveryError, M8DeliveryReport,
    M8ProductionDeliveryGates, M8ProductionPolicy, M8TrustedProductionPolicy, M8_DELIVERY_SCHEMA,
    M8_PRODUCTION_POLICY_SCHEMA,
};

pub const M8_SEED_SCHEMA: &str = "vice-classic/m8-seed-report/v1";
const TRANSPARENT_LABEL: u16 = u16::MAX;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MultiregionSeed {
    pub id: String,
    pub palette_id: String,
    pub palette_digest_sha256: String,
    pub palette_cardinality: u64,
    /// Number of spatially supported opaque colour modes observed before
    /// exact model selection. This is evidence, not authored truth.
    pub opaque_modes_seen: u64,
    pub palette_score: PaletteScore,
    pub blend_space: &'static str,
    pub exterior_source_label: Option<u16>,
    pub exterior_is_transparent: bool,
    pub rag: RegionAdjacencyGraph,
    pub dcel: MulticolorDcel,
    pub paint_fit: PaintFit,
    pub exact_rerendered: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MultiregionSeedReport {
    pub schema: &'static str,
    pub source_sha256: String,
    pub model_universe_hash: String,
    pub production: bool,
    pub exact_rerender_required: bool,
    pub palette_hypotheses: u64,
    pub seeds: Vec<MultiregionSeed>,
}

#[derive(Debug, thiserror::Error)]
pub enum MultiregionSeedError {
    #[error("decode failed: {0}")]
    Decode(#[from] vice_image::ImageError),
    #[error("multicolour palette refused: {0}")]
    Palette(#[from] MulticolorRefusal),
    #[error(transparent)]
    Rag(#[from] RagError),
    #[error(transparent)]
    Dcel(#[from] MultiDcelError),
    #[error(transparent)]
    Paint(#[from] PaintFitError),
}

pub fn propose_multiregion_seeds(
    png_bytes: &[u8],
) -> Result<MultiregionSeedReport, MultiregionSeedError> {
    let image = CanonicalImage::decode_png(png_bytes, &DecodeLimits::default())?;
    propose_from_image(&image, &MULTIREGION_PAINT_CONFIG_V1)
}

fn propose_from_image(
    image: &CanonicalImage,
    paint_cfg: &MultiregionPaintConfig,
) -> Result<MultiregionSeedReport, MultiregionSeedError> {
    let linear = ObservationTensor::of(image, BlendSpace::LinearLight);
    let interior = interior_confidence(&linear, &INTERIOR_CONFIG_V1);
    let border = border_indices(linear.width_px() as usize, linear.height_px() as usize);
    let palettes = propose_multicolor(
        &linear,
        &interior,
        &border,
        &PALETTE_CONFIG_V1,
        &MULTICOLOR_CONFIG_V1,
    );
    if let Some(refusal) = palettes.refusal {
        return Err(MultiregionSeedError::Palette(refusal));
    }

    let mut seeds = Vec::new();
    for palette in &palettes.hypotheses {
        let has_transparent = palette.assignments.contains(&TRANSPARENT_LABEL);
        let exterior_labels = if has_transparent {
            vec![Some(TRANSPARENT_LABEL)]
        } else {
            vec![None]
        };
        for exterior_source_label in exterior_labels {
            let labelling = RegionLabelling::new(
                image.width_px() as usize,
                image.height_px() as usize,
                palette.assignments.clone(),
                exterior_source_label,
            )?;
            let rag = RegionAdjacencyGraph::build(&labelling)?;
            let dcel = MulticolorDcel::assemble(&labelling)?;
            for blend_space in [BlendSpace::LinearLight, BlendSpace::EncodedSrgb] {
                let observation = ObservationTensor::of(image, blend_space);
                let proposal_partition = one_hot_partition(&rag);
                let transparent = Some(FaceId(
                    rag.exterior
                        .expect("every RAG has a visible or synthetic exterior")
                        .0,
                ));
                let paint_fit = fit_opaque_face_paints(
                    &observation,
                    &proposal_partition,
                    transparent,
                    paint_cfg,
                )?;
                let blend_name = match blend_space {
                    BlendSpace::LinearLight => "linear_light",
                    BlendSpace::EncodedSrgb => "encoded_srgb",
                };
                let id_material = format!(
                    "{}\u{1f}{}\u{1f}{}\u{1f}{}",
                    palette.digest_sha256,
                    rag.digest_sha256,
                    exterior_source_label
                        .map_or_else(|| "synthetic".to_string(), |label| label.to_string()),
                    blend_name
                );
                let digest = hex::encode(Sha256::digest(id_material.as_bytes()));
                seeds.push(MultiregionSeed {
                    id: format!("M8/seed/{}", &digest[..16]),
                    palette_id: palette.id.clone(),
                    palette_digest_sha256: palette.digest_sha256.clone(),
                    palette_cardinality: palette.colors.len() as u64,
                    opaque_modes_seen: palettes.opaque_modes_seen as u64,
                    palette_score: palette.score,
                    blend_space: blend_name,
                    exterior_source_label,
                    exterior_is_transparent: has_transparent,
                    rag: rag.clone(),
                    dcel: dcel.clone(),
                    paint_fit,
                    exact_rerendered: false,
                });
            }
        }
    }
    seeds.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(MultiregionSeedReport {
        schema: M8_SEED_SCHEMA,
        source_sha256: image.source_sha256().to_string(),
        model_universe_hash: model_universe_hash(&SupportedModelUniverseV1::m8()),
        production: false,
        exact_rerender_required: true,
        palette_hypotheses: palettes.hypotheses.len() as u64,
        seeds,
    })
}

fn border_indices(width: usize, height: usize) -> Vec<usize> {
    (0..width * height)
        .filter(|i| {
            let x = i % width;
            let y = i / width;
            x == 0 || y == 0 || x + 1 == width || y + 1 == height
        })
        .collect()
}

fn one_hot_partition(rag: &RegionAdjacencyGraph) -> PartitionRender {
    let n = rag.width_px * rag.height_px;
    let mut face_coverage = vec![vec![0.0; n]; rag.nodes.len()];
    for (pixel, region) in rag.region_of_pixel.iter().copied().enumerate() {
        face_coverage[region.index()][pixel] = 1.0;
    }
    PartitionRender {
        width_px: rag.width_px as u32,
        height_px: rag.height_px as u32,
        face_coverage,
        composite: vec![
            PremulRgba {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            };
            n
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vice_image::IccAssumption;

    fn three_islands() -> CanonicalImage {
        let (w, h) = (30u32, 14u32);
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        for (x0, rgb) in [(2, [230, 20, 20]), (11, [20, 220, 30]), (20, [20, 40, 230])] {
            for y in 3..11 {
                for x in x0..x0 + 7 {
                    let i = ((y * w + x) * 4) as usize;
                    pixels[i..i + 3].copy_from_slice(&rgb);
                    pixels[i + 3] = 255;
                }
            }
        }
        CanonicalImage::from_straight_srgb8(w, h, pixels, true, IccAssumption::NoProfileAssumedSrgb)
            .unwrap()
    }

    #[test]
    fn every_palette_exterior_and_formation_seed_gets_a_rag_dcel_and_paint_fit() {
        let report = propose_from_image(&three_islands(), &MULTIREGION_PAINT_CONFIG_V1).unwrap();
        assert!(!report.seeds.is_empty());
        assert!(!report.production && report.exact_rerender_required);
        assert_eq!(
            report.model_universe_hash,
            model_universe_hash(&SupportedModelUniverseV1::m8())
        );
        for seed in &report.seeds {
            assert!(seed.exterior_is_transparent);
            assert!(!seed.exact_rerendered);
            assert_eq!(seed.rag.digest_sha256, seed.dcel.rag_sha256);
            assert_eq!(seed.paint_fit.paints.len() + 1, seed.rag.nodes.len());
        }
        assert!(report.seeds.iter().any(|s| s.blend_space == "linear_light"));
        assert!(report.seeds.iter().any(|s| s.blend_space == "encoded_srgb"));
        let scene = materialize_multiregion_seed(&report.seeds[0]).unwrap();
        let render =
            vice_render::render_partition(&scene, &vice_render::RenderOptions::default()).unwrap();
        assert_eq!(
            render.face_coverage,
            one_hot_partition(&report.seeds[0].rag).face_coverage
        );
    }

    #[test]
    fn opaque_full_bleed_materializes_with_a_zero_area_synthetic_exterior() {
        let (w, h) = (30u32, 12u32);
        let colors = [[230, 20, 20], [20, 220, 30], [20, 40, 230]];
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let offset = ((y * w + x) * 4) as usize;
                pixels[offset..offset + 3].copy_from_slice(&colors[(x / 10) as usize]);
                pixels[offset + 3] = 255;
            }
        }
        let image = CanonicalImage::from_straight_srgb8(
            w,
            h,
            pixels,
            true,
            IccAssumption::NoProfileAssumedSrgb,
        )
        .unwrap();
        let report = propose_from_image(&image, &MULTIREGION_PAINT_CONFIG_V1).unwrap();
        assert!(!report.seeds.is_empty());
        for seed in &report.seeds {
            assert_eq!(seed.exterior_source_label, None);
            assert!(!seed.exterior_is_transparent);
            assert_eq!(seed.rag.nodes[0].pixels, 0);
            let scene = materialize_multiregion_seed(seed).unwrap();
            let render =
                vice_render::render_partition(&scene, &vice_render::RenderOptions::default())
                    .unwrap();
            assert!(render.face_coverage[0]
                .iter()
                .all(|coverage| coverage.abs() < 1e-12));
        }
    }

    #[test]
    fn flat2_cannot_enter_the_multiregion_seed_path() {
        // Rebuild a genuine two-colour opaque image; mutating the canonical
        // storage is intentionally unavailable.
        let pixels = vec![200, 10, 10, 255, 10, 20, 220, 255];
        let image = CanonicalImage::from_straight_srgb8(
            2,
            1,
            pixels,
            true,
            IccAssumption::NoProfileAssumedSrgb,
        )
        .unwrap();
        assert!(matches!(
            propose_from_image(&image, &MULTIREGION_PAINT_CONFIG_V1),
            Err(MultiregionSeedError::Palette(
                MulticolorRefusal::TooFewSupportedModes { .. }
            ))
        ));
    }
}
