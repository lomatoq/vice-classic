//! Shared planar graph (spec v1.3 §6.1).
//!
//! One interior shared boundary is stored ONCE; both adjacent faces refer
//! to it. The exterior is a REAL [`FaceId`] pointing into `faces`, not a
//! negative magic label (spec §5.3). Identity of every entity is its index
//! in the owning vector; the id newtypes below exist so indices of
//! different entity kinds cannot be mixed up silently.
//!
//! Deviations from the §6.1 sketch, documented in ADR-0005:
//! - no redundant `id` field inside the structs (identity = index);
//! - no `EvidenceRef`/`SupportRef` — the evidence system starts in M4 and
//!   speculative fields without call sites are forbidden (§32 rule 7).

use serde::{Deserialize, Serialize};
use vice_geom::Pt;

use crate::color::LinearRgb;
use crate::curve::CurveChain;

macro_rules! id_newtype {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub u32);

        impl $name {
            pub fn index(self) -> usize {
                self.0 as usize
            }
        }
    };
}

id_newtype!(
    /// Index into [`PlanarGraph::vertices`].
    VertexId
);
id_newtype!(
    /// Index into [`PlanarGraph::boundaries`].
    BoundaryId
);
id_newtype!(
    /// Index into [`PlanarGraph::half_edges`].
    HalfEdgeId
);
id_newtype!(
    /// Index into [`PlanarGraph::faces`]. The exterior face is a normal
    /// entry of `faces`.
    FaceId
);

/// A graph vertex: the SINGLE storage of a shared endpoint position.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphVertex {
    pub pos: Pt,
}

/// One shared boundary between two faces, directed from `start_vertex` to
/// `end_vertex`. Traversing in that direction, `left_face` lies on the
/// ALGEBRAIC left (see docs/COORDINATE_CONVENTION.md §1 for the y-down
/// caveat). The curve geometry is stored here exactly once.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Boundary {
    pub left_face: FaceId,
    pub right_face: FaceId,
    pub start_vertex: VertexId,
    pub end_vertex: VertexId,
    /// Join at the repeated endpoint of a self-loop boundary. Present
    /// exactly when `start_vertex == end_vertex`, so a closed-chain seam
    /// cannot lose its corner/G1 declaration.
    pub closure_join: Option<crate::curve::JoinKind>,
    pub curve: CurveChain,
}

/// Half of a boundary. `forward = true` traverses the boundary curve from
/// its start vertex to its end vertex and borders `left_face`; the twin
/// traverses backwards and borders `right_face`. `next` walks the face
/// loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HalfEdge {
    pub boundary: BoundaryId,
    pub forward: bool,
    pub twin: HalfEdgeId,
    pub next: HalfEdgeId,
    pub face: FaceId,
}

/// Flat-core paint (spec §6.4). Gradients and true semi-transparent
/// authored layers belong to their own milestones and do NOT exist here.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Paint {
    OpaqueSolid(LinearRgb),
    TransparentExterior,
}

/// A face of the visible planar partition: one representative half-edge per
/// loop, plus paint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Face {
    pub loops: Vec<HalfEdgeId>,
    pub paint: Paint,
}

/// The shared planar graph of a visible scene.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanarGraph {
    pub exterior: FaceId,
    pub vertices: Vec<GraphVertex>,
    pub boundaries: Vec<Boundary>,
    pub half_edges: Vec<HalfEdge>,
    pub faces: Vec<Face>,
}

impl PlanarGraph {
    /// The empty scene: no geometry, a single transparent exterior face.
    pub fn empty() -> PlanarGraph {
        PlanarGraph {
            exterior: FaceId(0),
            vertices: Vec::new(),
            boundaries: Vec::new(),
            half_edges: Vec::new(),
            faces: vec![Face {
                loops: Vec::new(),
                paint: Paint::TransparentExterior,
            }],
        }
    }

    /// Origin vertex of a half-edge (requires valid ids).
    pub fn he_origin(&self, h: HalfEdgeId) -> VertexId {
        let he = &self.half_edges[h.index()];
        let b = &self.boundaries[he.boundary.index()];
        if he.forward {
            b.start_vertex
        } else {
            b.end_vertex
        }
    }

    /// Target vertex of a half-edge (requires valid ids).
    pub fn he_target(&self, h: HalfEdgeId) -> VertexId {
        let he = &self.half_edges[h.index()];
        let b = &self.boundaries[he.boundary.index()];
        if he.forward {
            b.end_vertex
        } else {
            b.start_vertex
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_graph_has_a_real_exterior_face() {
        let g = PlanarGraph::empty();
        assert_eq!(g.exterior, FaceId(0));
        let ext = &g.faces[g.exterior.index()];
        assert_eq!(ext.paint, Paint::TransparentExterior);
        assert!(ext.loops.is_empty());
    }

    #[test]
    fn id_newtypes_do_not_mix() {
        // Compile-time property really; assert the indices work.
        assert_eq!(VertexId(3).index(), 3);
        assert_eq!(FaceId(0).index(), 0);
    }
}
