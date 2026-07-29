//! Atomic compound edits over canonical scenes.

use serde::Serialize;
use thiserror::Error;
use vice_ir::{
    scene_digest_sha256, BoundaryId, CurveChain, FaceId, GlobalFormationHypothesis, Paint,
    PlanarGraph, SceneError, VectorScene,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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

#[derive(Debug, Clone, PartialEq)]
pub enum SceneMutation {
    ReplaceBoundaryCurve {
        boundary: BoundaryId,
        curve: CurveChain,
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
    if scene_digest_sha256(parent)? != tx.expected_parent_digest {
        return Err(TransactionError::StaleParent);
    }
    if tx.mutations.is_empty() {
        return Err(TransactionError::Empty);
    }
    let mut child = parent.clone();
    for mutation in &tx.mutations {
        let allowed = match tx.kind {
            TransactionKind::PaintChange | TransactionKind::ExteriorChange => {
                matches!(mutation, SceneMutation::ReplaceFacePaint { .. })
            }
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
                matches!(mutation, SceneMutation::ReplaceBoundaryCurve { .. })
            }
        };
        if !allowed {
            return Err(TransactionError::KindPayloadMismatch);
        }
        match mutation {
            SceneMutation::ReplaceBoundaryCurve { boundary, curve } => {
                let Some(slot) = child.graph.boundaries.get_mut(boundary.index()) else {
                    return Err(TransactionError::MissingEntity);
                };
                slot.curve = curve.clone();
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
    Ok(child)
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
}
