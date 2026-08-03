//! Atomic merge, split, and paint transactions over the M8 RAG.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::rag::{RagError, RegionAdjacencyGraph, RegionId, RegionLabelling};

const SCENE_SCHEMA: &str = "vice-classic/region-scene/v1";
const TX_SCHEMA: &str = "vice-classic/rag-transaction/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "rgb", rename_all = "snake_case")]
pub enum QuantizedPaint {
    OpaqueSrgb8([u8; 3]),
    TransparentExterior,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionScene {
    labelling: RegionLabelling,
    paints: BTreeMap<u16, QuantizedPaint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RagEdit {
    Merge {
        keep: RegionId,
        remove: RegionId,
    },
    Split {
        region: RegionId,
        move_pixels: Vec<usize>,
        new_label: u16,
        new_paint: QuantizedPaint,
    },
    AssignPaint {
        label: u16,
        paint: QuantizedPaint,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RagTransactionError {
    #[error(transparent)]
    Graph(#[from] RagError),
    #[error("paint table does not exactly cover the labels in the scene")]
    PaintTableMismatch,
    #[error("transparent paint is only valid for the declared exterior label")]
    TransparentInterior,
    #[error("region {region:?} does not exist")]
    UnknownRegion { region: RegionId },
    #[error("regions {a:?} and {b:?} are not adjacent")]
    NotAdjacent { a: RegionId, b: RegionId },
    #[error("the exterior region cannot be merged or split")]
    EditsExterior,
    #[error("split pixels must be a non-empty strict subset of one region")]
    InvalidSplitSubset,
    #[error("split pixel {pixel} is outside the image or the selected region")]
    SplitPixelOutsideRegion { pixel: usize },
    #[error("new split label {label} is already present")]
    SplitLabelAlreadyPresent { label: u16 },
    #[error("both sides of a split must remain four-connected")]
    DisconnectedSplit,
    #[error("paint label {label} does not exist")]
    UnknownPaintLabel { label: u16 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RagTransactionLedger {
    pub schema: &'static str,
    pub edit: RagEdit,
    pub before_scene_sha256: String,
    pub after_scene_sha256: String,
    pub before_rag_sha256: String,
    pub after_rag_sha256: String,
    pub affected_pixels: Vec<u64>,
    pub exact_rollback_on_refusal: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RagTransactionOutcome {
    pub scene: RegionScene,
    pub graph: RegionAdjacencyGraph,
    pub ledger: RagTransactionLedger,
}

impl RegionScene {
    pub fn new(
        labelling: RegionLabelling,
        paints: BTreeMap<u16, QuantizedPaint>,
    ) -> Result<Self, RagTransactionError> {
        let labels = labelling.labels().iter().copied().collect::<BTreeSet<_>>();
        let paint_labels = paints.keys().copied().collect::<BTreeSet<_>>();
        if labels != paint_labels {
            return Err(RagTransactionError::PaintTableMismatch);
        }
        for (label, paint) in &paints {
            if matches!(paint, QuantizedPaint::TransparentExterior)
                && Some(*label) != labelling.exterior_label()
            {
                return Err(RagTransactionError::TransparentInterior);
            }
        }
        Ok(Self { labelling, paints })
    }

    pub fn labelling(&self) -> &RegionLabelling {
        &self.labelling
    }
    pub fn paints(&self) -> &BTreeMap<u16, QuantizedPaint> {
        &self.paints
    }
    pub fn graph(&self) -> Result<RegionAdjacencyGraph, RagError> {
        RegionAdjacencyGraph::build(&self.labelling)
    }
    pub fn digest_sha256(&self) -> String {
        let mut h = Sha256::new();
        h.update(SCENE_SCHEMA.as_bytes());
        h.update((self.labelling.width_px() as u64).to_le_bytes());
        h.update((self.labelling.height_px() as u64).to_le_bytes());
        h.update(
            self.labelling
                .exterior_label()
                .unwrap_or(u16::MAX)
                .to_le_bytes(),
        );
        for label in self.labelling.labels() {
            h.update(label.to_le_bytes());
        }
        for (label, paint) in &self.paints {
            h.update(label.to_le_bytes());
            match paint {
                QuantizedPaint::OpaqueSrgb8(rgb) => {
                    h.update([0]);
                    h.update(rgb);
                }
                QuantizedPaint::TransparentExterior => h.update([1]),
            }
        }
        hex::encode(h.finalize())
    }
}

pub fn apply_rag_transaction(
    original: &RegionScene,
    edit: RagEdit,
) -> Result<RagTransactionOutcome, RagTransactionError> {
    let before_graph = original.graph()?;
    let before_scene_sha256 = original.digest_sha256();
    let mut labels = original.labelling.labels().to_vec();
    let mut paints = original.paints.clone();
    let mut affected = BTreeSet::new();

    match &edit {
        RagEdit::Merge { keep, remove } => {
            let keep_node = before_graph
                .nodes
                .get(keep.index())
                .ok_or(RagTransactionError::UnknownRegion { region: *keep })?;
            let remove_node = before_graph
                .nodes
                .get(remove.index())
                .ok_or(RagTransactionError::UnknownRegion { region: *remove })?;
            if before_graph.exterior == Some(*remove) || before_graph.exterior == Some(*keep) {
                return Err(RagTransactionError::EditsExterior);
            }
            if !before_graph.neighbours(*keep).contains(remove) {
                return Err(RagTransactionError::NotAdjacent {
                    a: *keep,
                    b: *remove,
                });
            }
            // Canonical RAG palette labels are not the source labels.  Recover
            // the source label at the component anchor instead of confusing
            // the two identity spaces.
            let keep_label = original.labelling.labels()[keep_node.anchor_pixel as usize];
            let remove_label = original.labelling.labels()[remove_node.anchor_pixel as usize];
            for (i, region) in before_graph.region_of_pixel.iter().copied().enumerate() {
                if region == *remove {
                    labels[i] = keep_label;
                    affected.insert(i as u64);
                }
            }
            if !labels.contains(&remove_label) {
                paints.remove(&remove_label);
            }
        }
        RagEdit::Split {
            region,
            move_pixels,
            new_label,
            new_paint,
        } => {
            if before_graph.exterior == Some(*region) {
                return Err(RagTransactionError::EditsExterior);
            }
            let node = before_graph
                .nodes
                .get(region.index())
                .ok_or(RagTransactionError::UnknownRegion { region: *region })?;
            if paints.contains_key(new_label) {
                return Err(RagTransactionError::SplitLabelAlreadyPresent { label: *new_label });
            }
            let selected = move_pixels.iter().copied().collect::<BTreeSet<_>>();
            if selected.is_empty() || selected.len() as u64 >= node.pixels {
                return Err(RagTransactionError::InvalidSplitSubset);
            }
            for &pixel in &selected {
                if before_graph.region_of_pixel.get(pixel).copied() != Some(*region) {
                    return Err(RagTransactionError::SplitPixelOutsideRegion { pixel });
                }
                labels[pixel] = *new_label;
                affected.insert(pixel as u64);
            }
            paints.insert(*new_label, *new_paint);
            if matches!(new_paint, QuantizedPaint::TransparentExterior) {
                return Err(RagTransactionError::TransparentInterior);
            }
            let moved_connected = connected_subset(
                original.labelling.width_px(),
                original.labelling.height_px(),
                &selected,
            );
            let remainder = before_graph
                .region_of_pixel
                .iter()
                .enumerate()
                .filter(|(i, r)| **r == *region && !selected.contains(i))
                .map(|(i, _)| i)
                .collect::<BTreeSet<_>>();
            if !moved_connected
                || !connected_subset(
                    original.labelling.width_px(),
                    original.labelling.height_px(),
                    &remainder,
                )
            {
                return Err(RagTransactionError::DisconnectedSplit);
            }
        }
        RagEdit::AssignPaint { label, paint } => {
            if !paints.contains_key(label) {
                return Err(RagTransactionError::UnknownPaintLabel { label: *label });
            }
            if matches!(paint, QuantizedPaint::TransparentExterior)
                && Some(*label) != original.labelling.exterior_label()
            {
                return Err(RagTransactionError::TransparentInterior);
            }
            paints.insert(*label, *paint);
            for (i, value) in original.labelling.labels().iter().enumerate() {
                if value == label {
                    affected.insert(i as u64);
                }
            }
        }
    }

    let labelling = RegionLabelling::new(
        original.labelling.width_px(),
        original.labelling.height_px(),
        labels,
        original.labelling.exterior_label(),
    )?;
    let scene = RegionScene::new(labelling, paints)?;
    let graph = scene.graph()?;
    let ledger = RagTransactionLedger {
        schema: TX_SCHEMA,
        edit,
        before_scene_sha256,
        after_scene_sha256: scene.digest_sha256(),
        before_rag_sha256: before_graph.digest_sha256,
        after_rag_sha256: graph.digest_sha256.clone(),
        affected_pixels: affected.into_iter().collect(),
        exact_rollback_on_refusal: true,
    };
    Ok(RagTransactionOutcome {
        scene,
        graph,
        ledger,
    })
}

fn connected_subset(width: usize, height: usize, cells: &BTreeSet<usize>) -> bool {
    let Some(&start) = cells.first() else {
        return false;
    };
    let mut seen = BTreeSet::from([start]);
    let mut queue = VecDeque::from([start]);
    while let Some(i) = queue.pop_front() {
        let x = i % width;
        let y = i / width;
        for j in [
            (x > 0).then(|| i - 1),
            (x + 1 < width).then(|| i + 1),
            (y > 0).then(|| i - width),
            (y + 1 < height).then(|| i + width),
        ]
        .into_iter()
        .flatten()
        {
            if cells.contains(&j) && seen.insert(j) {
                queue.push_back(j);
            }
        }
    }
    seen.len() == cells.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene() -> RegionScene {
        let l =
            RegionLabelling::new(4, 3, vec![0, 0, 2, 2, 0, 1, 2, 2, 0, 1, 1, 2], Some(0)).unwrap();
        RegionScene::new(
            l,
            BTreeMap::from([
                (0, QuantizedPaint::TransparentExterior),
                (1, QuantizedPaint::OpaqueSrgb8([200, 10, 10])),
                (2, QuantizedPaint::OpaqueSrgb8([10, 200, 10])),
            ]),
        )
        .unwrap()
    }

    fn region_for_label(scene: &RegionScene, label: u16) -> RegionId {
        let graph = scene.graph().unwrap();
        graph
            .nodes
            .iter()
            .find(|node| scene.labelling.labels()[node.anchor_pixel as usize] == label)
            .unwrap()
            .id
    }

    #[test]
    fn merge_is_atomic_and_records_exact_before_after_identities() {
        let s = scene();
        let before = s.digest_sha256();
        let out = apply_rag_transaction(
            &s,
            RagEdit::Merge {
                keep: region_for_label(&s, 2),
                remove: region_for_label(&s, 1),
            },
        )
        .unwrap();
        assert_eq!(
            s.digest_sha256(),
            before,
            "the input is the rollback witness"
        );
        assert_ne!(
            out.ledger.before_scene_sha256,
            out.ledger.after_scene_sha256
        );
        assert_eq!(out.graph.nodes.len(), 2);
        assert!(out.ledger.exact_rollback_on_refusal);
    }

    #[test]
    fn split_requires_both_connected_sides() {
        let s = scene();
        let region = region_for_label(&s, 2);
        let bad = apply_rag_transaction(
            &s,
            RagEdit::Split {
                region,
                move_pixels: vec![2, 11],
                new_label: 9,
                new_paint: QuantizedPaint::OpaqueSrgb8([1, 2, 3]),
            },
        );
        assert!(matches!(bad, Err(RagTransactionError::DisconnectedSplit)));
        assert_eq!(s.graph().unwrap().nodes.len(), 3);
    }

    #[test]
    fn paint_assignment_changes_no_topology() {
        let s = scene();
        let out = apply_rag_transaction(
            &s,
            RagEdit::AssignPaint {
                label: 1,
                paint: QuantizedPaint::OpaqueSrgb8([8, 9, 10]),
            },
        )
        .unwrap();
        assert_eq!(out.ledger.before_rag_sha256, out.ledger.after_rag_sha256);
        assert_ne!(
            out.ledger.before_scene_sha256,
            out.ledger.after_scene_sha256
        );
    }

    #[test]
    fn transparent_interior_is_unrepresentable_at_creation_and_edit() {
        let s = scene();
        assert!(matches!(
            apply_rag_transaction(
                &s,
                RagEdit::AssignPaint {
                    label: 1,
                    paint: QuantizedPaint::TransparentExterior
                }
            ),
            Err(RagTransactionError::TransparentInterior)
        ));
    }
}
