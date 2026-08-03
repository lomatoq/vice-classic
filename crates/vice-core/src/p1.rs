//! P1 deterministic partition-correction API.
//!
//! This is a region editor, not a Bézier editor. A script changes the discrete
//! partition, then the affected graph is rebuilt and the resulting scene
//! re-enters the exact M8 render/likelihood court.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use sha2::{Digest, Sha256};
use vice_image::{CanonicalImage, DecodeLimits, ObservationTensor};
use vice_ir::{BlendSpace, FaceId};
use vice_opt::{score_fixed_opaque_face_paints, PaintFitError, MULTIREGION_PAINT_CONFIG_V1};
use vice_render::PartitionRender;
use vice_topology::{
    apply_partition_edit_script, MulticolorDcel, PartitionEditScript, PartitionScriptError,
    PartitionScriptStepLedger, QuantizedPaint, RagError, RagTransactionError, RegionAdjacencyGraph,
    RegionLabelling, RegionScene,
};

use crate::m8::solve_edited_multiregion_seed;
use crate::{
    propose_multiregion_seeds, solve_multiregion_exact, M8ExactConfig, M8ExactError, M8ExactReport,
    M8SolvedCandidate, MultiregionSeed, MultiregionSeedError,
};

pub const P1_SNAPSHOT_SCHEMA: &str = "vice-classic/partition-correction-snapshot/v1";
pub const P1_CORRECTION_SCHEMA: &str = "vice-classic/partition-correction/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct P1PartitionRegion {
    pub label: u16,
    pub region_id: u32,
    pub pixels: u64,
    pub anchor_pixel: u64,
    pub is_exterior: bool,
    pub paint: QuantizedPaint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct P1PartitionSnapshot {
    pub schema: &'static str,
    pub source_sha256: String,
    pub seed_id: String,
    pub base_scene_sha256: String,
    pub base_rag_sha256: String,
    pub width_px: u32,
    pub height_px: u32,
    pub regions: Vec<P1PartitionRegion>,
}

impl P1PartitionSnapshot {
    pub fn edit_script(&self) -> PartitionEditScript {
        PartitionEditScript {
            schema: vice_topology::PARTITION_EDIT_SCRIPT_SCHEMA.into(),
            base_scene_sha256: self.base_scene_sha256.clone(),
            edits: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct P1CorrectionReport {
    pub schema: &'static str,
    pub source_sha256: String,
    pub base_seed_id: String,
    pub base_scene_sha256: String,
    pub base_rag_sha256: String,
    pub script_sha256: String,
    pub steps: Vec<PartitionScriptStepLedger>,
    pub protected_labels: Vec<u16>,
    pub final_partition_scene_sha256: String,
    pub final_vector_scene_sha256: String,
    pub final_rag_sha256: String,
    pub affected_pixels: Vec<u64>,
    pub core_rerun_performed: bool,
    pub exact_rerendered: bool,
    pub production_admitted: bool,
    pub exact: M8ExactReport,
}

#[derive(Debug, Clone)]
pub struct P1CorrectionOutcome {
    pub solved: M8SolvedCandidate,
    pub report: P1CorrectionReport,
}

#[derive(Debug, thiserror::Error)]
pub enum P1CorrectionError {
    #[error(transparent)]
    Decode(#[from] vice_image::ImageError),
    #[error(transparent)]
    Exact(#[from] M8ExactError),
    #[error(transparent)]
    Seed(#[from] MultiregionSeedError),
    #[error(transparent)]
    Script(#[from] PartitionScriptError),
    #[error(transparent)]
    Rag(#[from] RagError),
    #[error(transparent)]
    Transaction(#[from] RagTransactionError),
    #[error(transparent)]
    Dcel(#[from] vice_topology::MultiDcelError),
    #[error(transparent)]
    Paint(#[from] PaintFitError),
    #[error("the exact M8 winner seed {seed_id} is absent from the reconstructed seed set")]
    MissingWinnerSeed { seed_id: String },
    #[error("the partition has too many regions for stable P1 labels")]
    RegionLabelSpaceExhausted,
    #[error("visible region {region} has no opaque fitted paint")]
    MissingPaint { region: u32 },
    #[error("P1 cannot score unknown blend space {0}")]
    UnknownBlendSpace(String),
}

pub fn inspect_multiregion_partition(
    png_bytes: &[u8],
    cfg: &M8ExactConfig,
) -> Result<P1PartitionSnapshot, P1CorrectionError> {
    let (image, seed, scene) = selected_region_scene(png_bytes, cfg)?;
    let graph = scene.graph()?;
    let regions = graph
        .nodes
        .iter()
        .filter(|node| node.pixels > 0)
        .map(|node| {
            let label = scene.labelling().labels()[node.anchor_pixel as usize];
            let paint = scene
                .paints()
                .get(&label)
                .copied()
                .ok_or(P1CorrectionError::MissingPaint { region: node.id.0 })?;
            Ok(P1PartitionRegion {
                label,
                region_id: node.id.0,
                pixels: node.pixels,
                anchor_pixel: node.anchor_pixel,
                is_exterior: node.is_exterior,
                paint,
            })
        })
        .collect::<Result<Vec<_>, P1CorrectionError>>()?;
    Ok(P1PartitionSnapshot {
        schema: P1_SNAPSHOT_SCHEMA,
        source_sha256: image.source_sha256().to_string(),
        seed_id: seed.id,
        base_scene_sha256: scene.digest_sha256(),
        base_rag_sha256: graph.digest_sha256,
        width_px: image.width_px(),
        height_px: image.height_px(),
        regions,
    })
}

pub fn correct_multiregion_partition(
    png_bytes: &[u8],
    script: &PartitionEditScript,
    cfg: &M8ExactConfig,
) -> Result<P1CorrectionOutcome, P1CorrectionError> {
    let (image, seed, base_scene) = selected_region_scene(png_bytes, cfg)?;
    let base_graph = base_scene.graph()?;
    let edited = apply_partition_edit_script(&base_scene, script)?;
    let affected_pixels = edited
        .steps
        .iter()
        .flat_map(|step| step.affected_pixels.iter().copied())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let edited_seed = edited_seed(
        &image,
        &seed,
        &edited.scene,
        &edited.graph,
        &edited.script_sha256,
    )?;
    let solved = solve_edited_multiregion_seed(&image, &edited_seed, cfg)?;
    let report = P1CorrectionReport {
        schema: P1_CORRECTION_SCHEMA,
        source_sha256: image.source_sha256().to_string(),
        base_seed_id: seed.id,
        base_scene_sha256: base_scene.digest_sha256(),
        base_rag_sha256: base_graph.digest_sha256,
        script_sha256: edited.script_sha256,
        steps: edited.steps,
        protected_labels: edited.protected_labels.into_iter().collect(),
        final_partition_scene_sha256: edited.scene.digest_sha256(),
        final_vector_scene_sha256: solved.report.selected.scene_digest_sha256.clone(),
        final_rag_sha256: edited.graph.digest_sha256,
        affected_pixels,
        core_rerun_performed: true,
        exact_rerendered: solved.report.selected.exact_rerendered,
        production_admitted: solved.report.production_admitted,
        exact: solved.report.clone(),
    };
    Ok(P1CorrectionOutcome { solved, report })
}

fn selected_region_scene(
    png_bytes: &[u8],
    cfg: &M8ExactConfig,
) -> Result<(CanonicalImage, MultiregionSeed, RegionScene), P1CorrectionError> {
    let image = CanonicalImage::decode(png_bytes, &DecodeLimits::default())?;
    let solved = solve_multiregion_exact(png_bytes, cfg)?;
    let seed_id = solved.report.selected.seed_id;
    let seed = propose_multiregion_seeds(png_bytes)?
        .seeds
        .into_iter()
        .find(|seed| seed.id == seed_id)
        .ok_or(P1CorrectionError::MissingWinnerSeed { seed_id })?;
    let scene = region_scene_from_seed(&seed)?;
    Ok((image, seed, scene))
}

fn region_scene_from_seed(seed: &MultiregionSeed) -> Result<RegionScene, P1CorrectionError> {
    if seed.rag.nodes.len() > usize::from(u16::MAX) {
        return Err(P1CorrectionError::RegionLabelSpaceExhausted);
    }
    let labels = seed
        .rag
        .region_of_pixel
        .iter()
        .map(|region| {
            u16::try_from(region.0).map_err(|_| P1CorrectionError::RegionLabelSpaceExhausted)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let exterior_label = seed.rag.exterior.and_then(|region| {
        seed.rag.nodes[region.index()]
            .pixels
            .gt(&0)
            .then_some(region.0 as u16)
    });
    let fitted = seed
        .paint_fit
        .paints
        .iter()
        .map(|paint| (paint.face.0, paint.quantized_srgb8))
        .collect::<BTreeMap<_, _>>();
    let mut paints = BTreeMap::new();
    for node in seed.rag.nodes.iter().filter(|node| node.pixels > 0) {
        let label =
            u16::try_from(node.id.0).map_err(|_| P1CorrectionError::RegionLabelSpaceExhausted)?;
        let paint = if node.is_exterior {
            QuantizedPaint::TransparentExterior
        } else {
            QuantizedPaint::OpaqueSrgb8(
                fitted
                    .get(&node.id.0)
                    .copied()
                    .ok_or(P1CorrectionError::MissingPaint { region: node.id.0 })?,
            )
        };
        paints.insert(label, paint);
    }
    let labelling = RegionLabelling::new(
        seed.rag.width_px,
        seed.rag.height_px,
        labels,
        exterior_label,
    )?;
    RegionScene::new(labelling, paints).map_err(P1CorrectionError::from)
}

fn edited_seed(
    image: &CanonicalImage,
    base: &MultiregionSeed,
    scene: &RegionScene,
    graph: &RegionAdjacencyGraph,
    script_sha256: &str,
) -> Result<MultiregionSeed, P1CorrectionError> {
    let dcel = MulticolorDcel::assemble(scene.labelling())?;
    let partition = one_hot_partition(graph);
    let blend_space = match base.blend_space {
        "linear_light" => BlendSpace::LinearLight,
        "encoded_srgb" => BlendSpace::EncodedSrgb,
        other => return Err(P1CorrectionError::UnknownBlendSpace(other.into())),
    };
    let observation = ObservationTensor::of(image, blend_space);
    let exterior = graph.exterior.map(|region| FaceId(region.0));
    let fixed = graph
        .nodes
        .iter()
        .filter(|node| !node.is_exterior)
        .map(|node| {
            let label = scene.labelling().labels()[node.anchor_pixel as usize];
            match scene.paints().get(&label) {
                Some(QuantizedPaint::OpaqueSrgb8(rgb)) => Ok((FaceId(node.id.0), *rgb)),
                _ => Err(P1CorrectionError::MissingPaint { region: node.id.0 }),
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    let paint_fit = score_fixed_opaque_face_paints(
        &observation,
        &partition,
        exterior,
        &fixed,
        &MULTIREGION_PAINT_CONFIG_V1,
    )?;
    let palette_cardinality = fixed
        .iter()
        .map(|(_, rgb)| *rgb)
        .collect::<BTreeSet<_>>()
        .len() as u64;
    let identity = format!(
        "{}{}{}{}",
        P1_CORRECTION_SCHEMA, base.id, script_sha256, graph.digest_sha256
    );
    let digest = hex::encode(Sha256::digest(identity.as_bytes()));
    Ok(MultiregionSeed {
        id: format!("P1/seed/{}", &digest[..16]),
        palette_id: format!("P1/palette/{}", &paint_fit.digest_sha256[..16]),
        palette_digest_sha256: paint_fit.digest_sha256.clone(),
        palette_cardinality,
        opaque_modes_seen: base.opaque_modes_seen,
        palette_score: base.palette_score,
        blend_space: base.blend_space,
        exterior_source_label: scene.labelling().exterior_label(),
        exterior_is_transparent: base.exterior_is_transparent,
        rag: graph.clone(),
        dcel,
        paint_fit,
        exact_rerendered: false,
    })
}

fn one_hot_partition(graph: &RegionAdjacencyGraph) -> PartitionRender {
    let pixels = graph.width_px * graph.height_px;
    let mut face_coverage = vec![vec![0.0; pixels]; graph.nodes.len()];
    for (pixel, region) in graph.region_of_pixel.iter().copied().enumerate() {
        face_coverage[region.index()][pixel] = 1.0;
    }
    PartitionRender {
        width_px: graph.width_px as u32,
        height_px: graph.height_px as u32,
        face_coverage,
        composite: vec![
            vice_ir::color::PremulRgba {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            };
            pixels
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vice_topology::{PartitionScriptEdit, PARTITION_EDIT_SCRIPT_SCHEMA};

    fn png() -> Vec<u8> {
        let (width, height) = (12u32, 6u32);
        let colors = [[230, 20, 20], [20, 220, 30], [20, 40, 230]];
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        for y in 0..height {
            for x in 0..width {
                let offset = ((y * width + x) * 4) as usize;
                pixels[offset..offset + 3].copy_from_slice(&colors[(x / 4) as usize]);
                pixels[offset + 3] = 255;
            }
        }
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&pixels).unwrap();
        }
        bytes
    }

    #[test]
    fn snapshot_script_reruns_exact_core_and_preserves_assignment() {
        let bytes = png();
        let cfg = M8ExactConfig::default();
        let snapshot = inspect_multiregion_partition(&bytes, &cfg).unwrap();
        let target = snapshot
            .regions
            .iter()
            .find(|region| !region.is_exterior)
            .unwrap();
        let mut script = snapshot.edit_script();
        script.edits.push(PartitionScriptEdit::Assign {
            label: target.label,
            paint: QuantizedPaint::OpaqueSrgb8([12, 34, 56]),
        });
        script.edits.push(PartitionScriptEdit::Protect {
            label: target.label,
        });
        let outcome = correct_multiregion_partition(&bytes, &script, &cfg).unwrap();
        assert!(outcome.report.core_rerun_performed);
        assert!(outcome.report.exact_rerendered);
        assert!(!outcome.report.production_admitted);
        assert_eq!(outcome.report.protected_labels, vec![target.label]);
        assert_eq!(outcome.report.steps.len(), 2);
        assert_eq!(outcome.solved.report.selected.paint_digest_sha256.len(), 64);
        let vice_ir::Paint::OpaqueSolid(rgb) =
            outcome.solved.scene.graph.faces[target.region_id as usize].paint
        else {
            panic!("assigned region lost its opaque paint");
        };
        let codes = [rgb.r, rgb.g, rgb.b]
            .map(|value| (vice_ir::color::linear_to_srgb_encoded(value) * 255.0).round() as u8);
        assert_eq!(
            codes,
            [12, 34, 56],
            "core rerun must not refit an explicit assignment away"
        );
    }

    #[test]
    fn stale_snapshot_and_protected_mutation_fail_before_delivery() {
        let bytes = png();
        let cfg = M8ExactConfig::default();
        let snapshot = inspect_multiregion_partition(&bytes, &cfg).unwrap();
        let label = snapshot
            .regions
            .iter()
            .find(|region| !region.is_exterior)
            .unwrap()
            .label;
        let protected = PartitionEditScript {
            schema: PARTITION_EDIT_SCRIPT_SCHEMA.into(),
            base_scene_sha256: snapshot.base_scene_sha256,
            edits: vec![
                PartitionScriptEdit::Protect { label },
                PartitionScriptEdit::Assign {
                    label,
                    paint: QuantizedPaint::OpaqueSrgb8([1, 2, 3]),
                },
            ],
        };
        assert!(matches!(
            correct_multiregion_partition(&bytes, &protected, &cfg),
            Err(P1CorrectionError::Script(
                PartitionScriptError::ProtectedLabel { .. }
            ))
        ));
        let stale = PartitionEditScript {
            schema: PARTITION_EDIT_SCRIPT_SCHEMA.into(),
            base_scene_sha256: "0".repeat(64),
            edits: vec![],
        };
        assert!(matches!(
            correct_multiregion_partition(&bytes, &stale, &cfg),
            Err(P1CorrectionError::Script(
                PartitionScriptError::BaseSceneMismatch
            ))
        ));
    }

    #[test]
    fn split_rebuilds_the_graph_and_reenters_the_exact_court() {
        let bytes = png();
        let cfg = M8ExactConfig::default();
        let snapshot = inspect_multiregion_partition(&bytes, &cfg).unwrap();
        let (_, _, base_scene) = selected_region_scene(&bytes, &cfg).unwrap();
        let base_graph = base_scene.graph().unwrap();
        let target = snapshot
            .regions
            .iter()
            .find(|region| !region.is_exterior && region.pixels >= 8)
            .unwrap();
        let width = base_scene.labelling().width_px();
        let move_pixels = base_scene
            .labelling()
            .labels()
            .iter()
            .enumerate()
            .filter(|(pixel, label)| **label == target.label && pixel / width < 3)
            .map(|(pixel, _)| pixel as u64)
            .collect::<Vec<_>>();
        let mut script = snapshot.edit_script();
        script.edits.push(PartitionScriptEdit::Split {
            label: target.label,
            move_pixels,
            new_label: 99,
            new_paint: QuantizedPaint::OpaqueSrgb8([90, 80, 70]),
        });
        let outcome = correct_multiregion_partition(&bytes, &script, &cfg).unwrap();
        assert_ne!(
            outcome.report.base_rag_sha256,
            outcome.report.final_rag_sha256
        );
        assert_eq!(
            outcome.solved.report.selected.visible_faces,
            base_graph.nodes.len() as u64
        );
        assert!(outcome.report.affected_pixels.len() >= 4);
        assert!(outcome.report.exact_rerendered);
    }
}
