//! Deterministic P1 partition-correction scripts over the M8 region scene.
//!
//! The script operates on stable source labels, never ephemeral RAG indices.
//! Every step is digest-bound, protection is explicit, restore addresses a
//! prior checkpoint, and an error returns no partial scene.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::rag::{RegionAdjacencyGraph, RegionId};
use crate::rag_transaction::{
    apply_rag_transaction, QuantizedPaint, RagEdit, RagTransactionError, RegionScene,
};

pub const PARTITION_EDIT_SCRIPT_SCHEMA: &str = "vice-classic/partition-edit-script/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartitionEditScript {
    pub schema: String,
    pub base_scene_sha256: String,
    pub edits: Vec<PartitionScriptEdit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PartitionScriptEdit {
    Merge {
        keep_label: u16,
        remove_label: u16,
    },
    Split {
        label: u16,
        move_pixels: Vec<u64>,
        new_label: u16,
        new_paint: QuantizedPaint,
    },
    Assign {
        label: u16,
        paint: QuantizedPaint,
    },
    Protect {
        label: u16,
    },
    /// Restore the state after `checkpoint` prior steps. Checkpoint zero is
    /// the immutable base scene.
    Restore {
        checkpoint: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PartitionScriptStepLedger {
    pub step: u32,
    pub edit: PartitionScriptEdit,
    pub before_scene_sha256: String,
    pub after_scene_sha256: String,
    pub before_rag_sha256: String,
    pub after_rag_sha256: String,
    pub affected_pixels: Vec<u64>,
    pub protected_labels: Vec<u16>,
    pub restored_checkpoint: Option<u32>,
    pub core_rerun_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionScriptOutcome {
    pub scene: RegionScene,
    pub graph: RegionAdjacencyGraph,
    pub protected_labels: BTreeSet<u16>,
    pub script_sha256: String,
    pub steps: Vec<PartitionScriptStepLedger>,
    pub core_rerun_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PartitionScriptError {
    #[error("partition edit script schema {found:?} is not supported")]
    InvalidSchema { found: String },
    #[error("partition edit script base scene does not match the supplied scene")]
    BaseSceneMismatch,
    #[error("label {label} does not identify a region")]
    UnknownLabel { label: u16 },
    #[error("label {label} identifies more than one disconnected region")]
    AmbiguousLabel { label: u16 },
    #[error("step {step} attempts to change protected label {label}")]
    ProtectedLabel { step: u32, label: u16 },
    #[error("step {step} restores unavailable or future checkpoint {checkpoint}")]
    InvalidRestore { step: u32, checkpoint: u32 },
    #[error("pixel index {pixel} does not fit this platform")]
    PixelIndexOverflow { pixel: u64 },
    #[error("partition transaction at step {step} refused: {source}")]
    Transaction {
        step: u32,
        #[source]
        source: RagTransactionError,
    },
}

#[derive(Clone)]
struct Snapshot {
    scene: RegionScene,
    protected: BTreeSet<u16>,
}

pub fn apply_partition_edit_script(
    original: &RegionScene,
    script: &PartitionEditScript,
) -> Result<PartitionScriptOutcome, PartitionScriptError> {
    if script.schema != PARTITION_EDIT_SCRIPT_SCHEMA {
        return Err(PartitionScriptError::InvalidSchema {
            found: script.schema.clone(),
        });
    }
    if script.base_scene_sha256 != original.digest_sha256() {
        return Err(PartitionScriptError::BaseSceneMismatch);
    }

    let script_sha256 = hex::encode(Sha256::digest(
        serde_json::to_vec(script).expect("typed partition script serializes"),
    ));
    let mut current = original.clone();
    let mut protected = BTreeSet::new();
    let mut snapshots = vec![Snapshot {
        scene: current.clone(),
        protected: protected.clone(),
    }];
    let mut ledgers = Vec::with_capacity(script.edits.len());

    for (index, edit) in script.edits.iter().cloned().enumerate() {
        let step = u32::try_from(index).unwrap_or(u32::MAX);
        let before_scene_sha256 = current.digest_sha256();
        let before_graph = current
            .graph()
            .map_err(|source| PartitionScriptError::Transaction {
                step,
                source: source.into(),
            })?;
        let before_labels = current.labelling().labels().to_vec();
        let mut affected_pixels = Vec::new();
        let mut restored_checkpoint = None;

        match &edit {
            PartitionScriptEdit::Merge {
                keep_label,
                remove_label,
            } => {
                require_unprotected(step, *keep_label, &protected)?;
                require_unprotected(step, *remove_label, &protected)?;
                let keep = unique_region_for_label(&current, &before_graph, *keep_label)?;
                let remove = unique_region_for_label(&current, &before_graph, *remove_label)?;
                let outcome = apply_rag_transaction(&current, RagEdit::Merge { keep, remove })
                    .map_err(|source| PartitionScriptError::Transaction { step, source })?;
                affected_pixels = outcome.ledger.affected_pixels;
                current = outcome.scene;
            }
            PartitionScriptEdit::Split {
                label,
                move_pixels,
                new_label,
                new_paint,
            } => {
                require_unprotected(step, *label, &protected)?;
                require_unprotected(step, *new_label, &protected)?;
                let region = unique_region_for_label(&current, &before_graph, *label)?;
                let move_pixels = move_pixels
                    .iter()
                    .map(|&pixel| {
                        usize::try_from(pixel)
                            .map_err(|_| PartitionScriptError::PixelIndexOverflow { pixel })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let outcome = apply_rag_transaction(
                    &current,
                    RagEdit::Split {
                        region,
                        move_pixels,
                        new_label: *new_label,
                        new_paint: *new_paint,
                    },
                )
                .map_err(|source| PartitionScriptError::Transaction { step, source })?;
                affected_pixels = outcome.ledger.affected_pixels;
                current = outcome.scene;
            }
            PartitionScriptEdit::Assign { label, paint } => {
                require_unprotected(step, *label, &protected)?;
                if !current.paints().contains_key(label) {
                    return Err(PartitionScriptError::UnknownLabel { label: *label });
                }
                let outcome = apply_rag_transaction(
                    &current,
                    RagEdit::AssignPaint {
                        label: *label,
                        paint: *paint,
                    },
                )
                .map_err(|source| PartitionScriptError::Transaction { step, source })?;
                affected_pixels = outcome.ledger.affected_pixels;
                current = outcome.scene;
            }
            PartitionScriptEdit::Protect { label } => {
                if !current.paints().contains_key(label) {
                    return Err(PartitionScriptError::UnknownLabel { label: *label });
                }
                protected.insert(*label);
            }
            PartitionScriptEdit::Restore { checkpoint } => {
                let Some(snapshot) = snapshots.get(*checkpoint as usize).cloned() else {
                    return Err(PartitionScriptError::InvalidRestore {
                        step,
                        checkpoint: *checkpoint,
                    });
                };
                affected_pixels = differing_pixels(
                    current.labelling().labels(),
                    snapshot.scene.labelling().labels(),
                );
                current = snapshot.scene;
                protected = snapshot.protected;
                restored_checkpoint = Some(*checkpoint);
            }
        }

        let after_graph = current
            .graph()
            .map_err(|source| PartitionScriptError::Transaction {
                step,
                source: source.into(),
            })?;
        let after_scene_sha256 = current.digest_sha256();
        let core_rerun_required = before_scene_sha256 != after_scene_sha256
            || before_graph.digest_sha256 != after_graph.digest_sha256
            || before_labels != current.labelling().labels();
        ledgers.push(PartitionScriptStepLedger {
            step,
            edit,
            before_scene_sha256,
            after_scene_sha256,
            before_rag_sha256: before_graph.digest_sha256,
            after_rag_sha256: after_graph.digest_sha256.clone(),
            affected_pixels,
            protected_labels: protected.iter().copied().collect(),
            restored_checkpoint,
            core_rerun_required,
        });
        snapshots.push(Snapshot {
            scene: current.clone(),
            protected: protected.clone(),
        });
    }

    let graph = current
        .graph()
        .map_err(|source| PartitionScriptError::Transaction {
            step: u32::try_from(script.edits.len()).unwrap_or(u32::MAX),
            source: source.into(),
        })?;
    let core_rerun_required = ledgers.iter().any(|step| step.core_rerun_required);
    Ok(PartitionScriptOutcome {
        scene: current,
        graph,
        protected_labels: protected,
        script_sha256,
        steps: ledgers,
        core_rerun_required,
    })
}

fn require_unprotected(
    step: u32,
    label: u16,
    protected: &BTreeSet<u16>,
) -> Result<(), PartitionScriptError> {
    if protected.contains(&label) {
        Err(PartitionScriptError::ProtectedLabel { step, label })
    } else {
        Ok(())
    }
}

fn unique_region_for_label(
    scene: &RegionScene,
    graph: &RegionAdjacencyGraph,
    label: u16,
) -> Result<RegionId, PartitionScriptError> {
    let regions = graph
        .nodes
        .iter()
        .filter(|node| {
            node.pixels > 0 && scene.labelling().labels()[node.anchor_pixel as usize] == label
        })
        .map(|node| node.id)
        .collect::<Vec<_>>();
    match regions.as_slice() {
        [] => Err(PartitionScriptError::UnknownLabel { label }),
        [region] => Ok(*region),
        _ => Err(PartitionScriptError::AmbiguousLabel { label }),
    }
}

fn differing_pixels(before: &[u16], after: &[u16]) -> Vec<u64> {
    before
        .iter()
        .zip(after)
        .enumerate()
        .filter(|(_, (before, after))| before != after)
        .map(|(index, _)| index as u64)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::rag::RegionLabelling;

    fn scene() -> RegionScene {
        RegionScene::new(
            RegionLabelling::new(4, 3, vec![0, 0, 2, 2, 0, 1, 2, 2, 0, 1, 1, 2], Some(0)).unwrap(),
            BTreeMap::from([
                (0, QuantizedPaint::TransparentExterior),
                (1, QuantizedPaint::OpaqueSrgb8([200, 10, 10])),
                (2, QuantizedPaint::OpaqueSrgb8([10, 200, 10])),
            ]),
        )
        .unwrap()
    }

    fn script(base: &RegionScene, edits: Vec<PartitionScriptEdit>) -> PartitionEditScript {
        PartitionEditScript {
            schema: PARTITION_EDIT_SCRIPT_SCHEMA.into(),
            base_scene_sha256: base.digest_sha256(),
            edits,
        }
    }

    #[test]
    fn split_assign_protect_and_restore_are_deterministic() {
        let base = scene();
        let edits = vec![
            PartitionScriptEdit::Split {
                label: 2,
                move_pixels: vec![2, 3],
                new_label: 9,
                new_paint: QuantizedPaint::OpaqueSrgb8([1, 2, 3]),
            },
            PartitionScriptEdit::Assign {
                label: 9,
                paint: QuantizedPaint::OpaqueSrgb8([4, 5, 6]),
            },
            PartitionScriptEdit::Protect { label: 9 },
            PartitionScriptEdit::Restore { checkpoint: 2 },
        ];
        let script = script(&base, edits);
        let a = apply_partition_edit_script(&base, &script).unwrap();
        let b = apply_partition_edit_script(&base, &script).unwrap();
        assert_eq!(a.script_sha256, b.script_sha256);
        assert_eq!(a.scene.digest_sha256(), b.scene.digest_sha256());
        assert_eq!(a.steps, b.steps);
        assert!(!a.protected_labels.contains(&9));
        assert_eq!(a.steps[3].restored_checkpoint, Some(2));
        assert!(a.core_rerun_required);
    }

    #[test]
    fn protection_refuses_the_whole_script_without_partial_output() {
        let base = scene();
        let before = base.digest_sha256();
        let script = script(
            &base,
            vec![
                PartitionScriptEdit::Protect { label: 2 },
                PartitionScriptEdit::Merge {
                    keep_label: 1,
                    remove_label: 2,
                },
            ],
        );
        assert!(matches!(
            apply_partition_edit_script(&base, &script),
            Err(PartitionScriptError::ProtectedLabel { step: 1, label: 2 })
        ));
        assert_eq!(base.digest_sha256(), before);
    }

    #[test]
    fn stale_base_and_future_restore_fail_closed() {
        let base = scene();
        let mut stale = script(&base, vec![]);
        stale.base_scene_sha256 = "0".repeat(64);
        assert!(matches!(
            apply_partition_edit_script(&base, &stale),
            Err(PartitionScriptError::BaseSceneMismatch)
        ));
        let future = script(&base, vec![PartitionScriptEdit::Restore { checkpoint: 2 }]);
        assert!(matches!(
            apply_partition_edit_script(&base, &future),
            Err(PartitionScriptError::InvalidRestore { .. })
        ));
    }

    #[test]
    fn script_json_round_trips_and_unknown_fields_are_refused() {
        let base = scene();
        let script = script(
            &base,
            vec![PartitionScriptEdit::Assign {
                label: 1,
                paint: QuantizedPaint::OpaqueSrgb8([7, 8, 9]),
            }],
        );
        let json = serde_json::to_vec(&script).unwrap();
        assert_eq!(
            serde_json::from_slice::<PartitionEditScript>(&json).unwrap(),
            script
        );
        let mut value = serde_json::to_value(&script).unwrap();
        value["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<PartitionEditScript>(value).is_err());
    }

    #[test]
    fn merge_uses_stable_labels_and_restore_returns_exact_base_bytes() {
        let base = scene();
        let script = script(
            &base,
            vec![
                PartitionScriptEdit::Merge {
                    keep_label: 1,
                    remove_label: 2,
                },
                PartitionScriptEdit::Restore { checkpoint: 0 },
            ],
        );
        let outcome = apply_partition_edit_script(&base, &script).unwrap();
        assert_eq!(outcome.scene.digest_sha256(), base.digest_sha256());
        assert_eq!(
            outcome.graph.digest_sha256,
            base.graph().unwrap().digest_sha256
        );
        assert_eq!(outcome.steps[1].restored_checkpoint, Some(0));
        assert!(outcome.steps[0].core_rerun_required);
        assert!(outcome.steps[1].core_rerun_required);
    }
}
