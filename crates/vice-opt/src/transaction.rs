//! Atomic compound edits over canonical scenes.

use serde::Serialize;
use thiserror::Error;
use vice_geom::Pt;
use vice_ir::{
    scene_digest_sha256, BoundaryId, CurveChain, FaceId, GlobalFormationHypothesis, JoinKind,
    Paint, PlanarGraph, SceneError, VectorScene, VertexId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionKind {
    AnchorInsert,
    AnchorRemove,
    SpanSplitJointRefit,
    SpanMergeJointRefit,
    FamilyChange,
    CornerActivate,
    CornerDeactivate,
    PrimitivePromote,
    PrimitiveDemote,
    RelationPromote,
    RelationDemote,
    TopologyMerge,
    TopologySplit,
    TopologyBridge,
    TopologyHole,
    PaintChange,
    ExteriorChange,
    FormationChange,
    JointEscape,
}

impl TransactionKind {
    pub const ALL: [TransactionKind; 19] = [
        TransactionKind::AnchorInsert,
        TransactionKind::AnchorRemove,
        TransactionKind::SpanSplitJointRefit,
        TransactionKind::SpanMergeJointRefit,
        TransactionKind::FamilyChange,
        TransactionKind::CornerActivate,
        TransactionKind::CornerDeactivate,
        TransactionKind::PrimitivePromote,
        TransactionKind::PrimitiveDemote,
        TransactionKind::RelationPromote,
        TransactionKind::RelationDemote,
        TransactionKind::TopologyMerge,
        TransactionKind::TopologySplit,
        TransactionKind::TopologyBridge,
        TransactionKind::TopologyHole,
        TransactionKind::PaintChange,
        TransactionKind::ExteriorChange,
        TransactionKind::FormationChange,
        TransactionKind::JointEscape,
    ];
}

#[derive(Debug, Clone, PartialEq)]
pub enum SceneMutation {
    ReplaceBoundaryGeometry {
        boundary: BoundaryId,
        curve: CurveChain,
        /// The closure join is part of self-loop geometry and must change in
        /// the same transaction as its segments/nodes.
        closure_join: Option<JoinKind>,
    },
    ReplaceVertexPosition {
        vertex: VertexId,
        position: Pt,
    },
    ReplaceFacePaint {
        face: FaceId,
        paint: Paint,
    },
    ReplaceGraph(PlanarGraph),
    ReplaceFormation(GlobalFormationHypothesis),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompoundTransaction {
    pub kind: TransactionKind,
    pub expected_parent_digest: String,
    /// Every mutation is applied to a private clone. Validation happens only
    /// after the complete logical operation, so an invalid half-operation is
    /// never published or scored.
    pub mutations: Vec<SceneMutation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TransactionApplication {
    pub kind: TransactionKind,
    pub parent_digest: String,
    pub child_digest: String,
    pub mutations_applied: usize,
    pub atomic: bool,
}

#[derive(Debug, Error)]
pub enum TransactionError {
    #[error("parent digest changed before transaction")]
    StaleParent,
    #[error("compound transaction has no mutations")]
    Empty,
    #[error("transaction references missing boundary or face")]
    MissingEntity,
    #[error("transaction kind and mutation payload disagree")]
    KindPayloadMismatch,
    #[error("complete transaction is invalid: {0}")]
    InvalidScene(#[from] SceneError),
}

pub fn apply_compound_transaction(
    parent: &VectorScene,
    tx: &CompoundTransaction,
) -> Result<VectorScene, TransactionError> {
    apply_compound_transaction_traced(parent, tx).map(|(scene, _receipt)| scene)
}

pub fn apply_compound_transaction_traced(
    parent: &VectorScene,
    tx: &CompoundTransaction,
) -> Result<(VectorScene, TransactionApplication), TransactionError> {
    let parent_digest = scene_digest_sha256(parent)?;
    if parent_digest != tx.expected_parent_digest {
        return Err(TransactionError::StaleParent);
    }
    if tx.mutations.is_empty() {
        return Err(TransactionError::Empty);
    }
    let mut child = parent.clone();
    for mutation in &tx.mutations {
        let allowed = match tx.kind {
            TransactionKind::PaintChange => {
                matches!(mutation, SceneMutation::ReplaceFacePaint { .. })
            }
            TransactionKind::ExteriorChange => matches!(
                mutation,
                SceneMutation::ReplaceFacePaint { .. }
                    | SceneMutation::ReplaceGraph(_)
                    | SceneMutation::ReplaceFormation(_)
            ),
            TransactionKind::FormationChange => {
                matches!(mutation, SceneMutation::ReplaceFormation(_))
            }
            TransactionKind::TopologyMerge
            | TransactionKind::TopologySplit
            | TransactionKind::TopologyBridge
            | TransactionKind::TopologyHole => matches!(mutation, SceneMutation::ReplaceGraph(_)),
            TransactionKind::JointEscape => true,
            TransactionKind::AnchorInsert
            | TransactionKind::AnchorRemove
            | TransactionKind::SpanSplitJointRefit
            | TransactionKind::SpanMergeJointRefit
            | TransactionKind::FamilyChange
            | TransactionKind::CornerActivate
            | TransactionKind::CornerDeactivate
            | TransactionKind::PrimitivePromote
            | TransactionKind::PrimitiveDemote
            | TransactionKind::RelationPromote
            | TransactionKind::RelationDemote => {
                matches!(
                    mutation,
                    SceneMutation::ReplaceBoundaryGeometry { .. }
                        | SceneMutation::ReplaceVertexPosition { .. }
                )
            }
        };
        if !allowed {
            return Err(TransactionError::KindPayloadMismatch);
        }
        match mutation {
            SceneMutation::ReplaceBoundaryGeometry {
                boundary,
                curve,
                closure_join,
            } => {
                let Some(slot) = child.graph.boundaries.get_mut(boundary.index()) else {
                    return Err(TransactionError::MissingEntity);
                };
                slot.curve = curve.clone();
                slot.closure_join = *closure_join;
            }
            SceneMutation::ReplaceVertexPosition { vertex, position } => {
                let Some(slot) = child.graph.vertices.get_mut(vertex.index()) else {
                    return Err(TransactionError::MissingEntity);
                };
                slot.pos = *position;
            }
            SceneMutation::ReplaceFacePaint { face, paint } => {
                let Some(slot) = child.graph.faces.get_mut(face.index()) else {
                    return Err(TransactionError::MissingEntity);
                };
                slot.paint = *paint;
            }
            SceneMutation::ReplaceGraph(graph) => child.graph = graph.clone(),
            SceneMutation::ReplaceFormation(formation) => child.formation = *formation,
        }
    }
    vice_ir::validate_scene(&child)?;
    let child_digest = scene_digest_sha256(&child)?;
    Ok((
        child,
        TransactionApplication {
            kind: tx.kind,
            parent_digest,
            child_digest,
            mutations_applied: tx.mutations.len(),
            atomic: true,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vice_ir::{
        BlendSpace, Canvas, ExteriorModel, GlobalFormationHypothesis, PixelFilter,
        QuantizationModel,
    };

    fn scene() -> VectorScene {
        VectorScene {
            canvas: Canvas {
                width_px: 10,
                height_px: 10,
            },
            graph: PlanarGraph::empty(),
            formation: GlobalFormationHypothesis {
                blend_space: BlendSpace::LinearLight,
                pixel_filter: PixelFilter::Box,
                quantization: QuantizationModel::Uint8,
                exterior: ExteriorModel::Transparent,
            },
        }
    }

    #[test]
    fn stale_parent_is_refused_before_any_mutation() {
        let parent = scene();
        let tx = CompoundTransaction {
            kind: TransactionKind::FormationChange,
            expected_parent_digest: "wrong".into(),
            mutations: vec![SceneMutation::ReplaceFormation(parent.formation)],
        };
        assert!(matches!(
            apply_compound_transaction(&parent, &tx),
            Err(TransactionError::StaleParent)
        ));
    }

    #[test]
    fn complete_valid_transaction_is_atomic() {
        let parent = scene();
        let mut formation = parent.formation;
        formation.blend_space = BlendSpace::EncodedSrgb;
        let tx = CompoundTransaction {
            kind: TransactionKind::FormationChange,
            expected_parent_digest: scene_digest_sha256(&parent).unwrap(),
            mutations: vec![SceneMutation::ReplaceFormation(formation)],
        };
        let child = apply_compound_transaction(&parent, &tx).unwrap();
        assert_eq!(child.formation.blend_space, BlendSpace::EncodedSrgb);
        assert_eq!(parent.formation.blend_space, BlendSpace::LinearLight);
    }

    #[test]
    fn a_late_failure_cannot_publish_an_earlier_half_operation() {
        let parent = scene();
        let mut formation = parent.formation;
        formation.blend_space = BlendSpace::EncodedSrgb;
        let before = scene_digest_sha256(&parent).unwrap();
        let tx = CompoundTransaction {
            kind: TransactionKind::JointEscape,
            expected_parent_digest: before.clone(),
            mutations: vec![
                SceneMutation::ReplaceFormation(formation),
                SceneMutation::ReplaceBoundaryGeometry {
                    boundary: BoundaryId(0),
                    curve: CurveChain {
                        interior_nodes: Vec::new(),
                        segments: Vec::new(),
                    },
                    closure_join: None,
                },
            ],
        };
        assert!(matches!(
            apply_compound_transaction(&parent, &tx),
            Err(TransactionError::MissingEntity)
        ));
        assert_eq!(scene_digest_sha256(&parent).unwrap(), before);
        assert_eq!(parent.formation.blend_space, BlendSpace::LinearLight);
    }

    #[test]
    fn the_transaction_inventory_is_total_and_duplicate_free() {
        let kinds: std::collections::BTreeSet<_> = TransactionKind::ALL.into_iter().collect();
        assert_eq!(kinds.len(), TransactionKind::ALL.len());
        assert_eq!(kinds.len(), 19);
    }
}
