//! A shared, canonical multicolour DCEL over the pixel-grid arrangement.
//!
//! Each geometric boundary segment is stored once.  Its two half-edges are
//! identities derived from that one record, so twins and two-owner boundaries
//! cannot drift apart.  Face loops are deterministic cyclic sequences and the
//! constructor audits them against the canonical RAG before returning.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::rag::{RagError, RegionAdjacencyGraph, RegionId, RegionLabelling};

pub const MULTICOLOR_DCEL_SCHEMA: &str = "vice-classic/multicolor-dcel/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct MultiBoundaryId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct MultiHalfEdgeId(pub u32);

impl MultiHalfEdgeId {
    pub fn new(boundary: MultiBoundaryId, forward: bool) -> Self {
        Self(boundary.0 * 2 + u32::from(!forward))
    }
    pub fn boundary(self) -> MultiBoundaryId {
        MultiBoundaryId(self.0 / 2)
    }
    pub fn is_forward(self) -> bool {
        self.0.is_multiple_of(2)
    }
    pub fn twin(self) -> Self {
        Self(self.0 ^ 1)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MultiBoundary {
    pub id: MultiBoundaryId,
    pub start: (u32, u32),
    pub end: (u32, u32),
    /// Owner on the left/right of the stored start-to-end orientation.
    pub left: RegionId,
    pub right: RegionId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MultiFace {
    pub region: RegionId,
    pub palette_label: u16,
    pub is_exterior: bool,
    pub loops: Vec<Vec<MultiHalfEdgeId>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MultiJunction {
    pub at: (u32, u32),
    pub incident_boundaries: Vec<MultiBoundaryId>,
    pub incident_regions: Vec<RegionId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MulticolorDcel {
    pub schema: &'static str,
    pub rag_sha256: String,
    pub boundaries: Vec<MultiBoundary>,
    pub faces: Vec<MultiFace>,
    pub junctions: Vec<MultiJunction>,
    pub digest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MultiDcelError {
    #[error(transparent)]
    Graph(#[from] RagError),
    #[error("the arrangement exceeds the u32 boundary identity space")]
    BoundarySpaceExhausted,
    #[error("face {region:?} has an open or ambiguous boundary walk at {at:?}")]
    OpenFaceWalk { region: RegionId, at: (u32, u32) },
    #[error("the DCEL inventory disagrees with its canonical RAG")]
    RagInventoryMismatch,
}

impl MulticolorDcel {
    pub fn assemble(labelling: &RegionLabelling) -> Result<Self, MultiDcelError> {
        let rag = RegionAdjacencyGraph::build(labelling)?;
        let exterior = rag
            .exterior
            .expect("every RAG has a visible or synthetic exterior face");
        let (w, h) = (rag.width_px, rag.height_px);
        let mut raw = Vec::<((u32, u32), (u32, u32), RegionId, RegionId)>::new();
        let mut add = |start, end, left, right| {
            if left != right {
                raw.push((start, end, left, right));
            }
        };
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let here = rag.region_of_pixel[i];
                if x + 1 < w {
                    let right = rag.region_of_pixel[i + 1];
                    add(
                        ((x + 1) as u32, (y + 1) as u32),
                        ((x + 1) as u32, y as u32),
                        right,
                        here,
                    );
                }
                if y + 1 < h {
                    let below = rag.region_of_pixel[i + w];
                    add(
                        (x as u32, (y + 1) as u32),
                        ((x + 1) as u32, (y + 1) as u32),
                        below,
                        here,
                    );
                }
                if here != exterior {
                    if y == 0 {
                        add(((x + 1) as u32, 0), (x as u32, 0), exterior, here);
                    }
                    if y + 1 == h {
                        add(
                            (x as u32, h as u32),
                            ((x + 1) as u32, h as u32),
                            exterior,
                            here,
                        );
                    }
                    if x == 0 {
                        add((0, y as u32), (0, (y + 1) as u32), exterior, here);
                    }
                    if x + 1 == w {
                        add(
                            (w as u32, (y + 1) as u32),
                            (w as u32, y as u32),
                            exterior,
                            here,
                        );
                    }
                }
            }
        }
        raw.sort();
        raw.dedup();
        if raw.len() > u32::MAX as usize {
            return Err(MultiDcelError::BoundarySpaceExhausted);
        }
        let boundaries = raw
            .into_iter()
            .enumerate()
            .map(|(id, (start, end, left, right))| MultiBoundary {
                id: MultiBoundaryId(id as u32),
                start,
                end,
                left,
                right,
            })
            .collect::<Vec<_>>();

        let mut faces = Vec::with_capacity(rag.nodes.len());
        for node in &rag.nodes {
            let mut oriented = Vec::new();
            for boundary in &boundaries {
                if boundary.left == node.id {
                    oriented.push(MultiHalfEdgeId::new(boundary.id, true));
                } else if boundary.right == node.id {
                    oriented.push(MultiHalfEdgeId::new(boundary.id, false));
                }
            }
            let loops = face_loops(node.id, &boundaries, &oriented)?;
            faces.push(MultiFace {
                region: node.id,
                palette_label: node.palette_label,
                is_exterior: node.is_exterior,
                loops,
            });
        }

        let mut at_vertex = BTreeMap::<(u32, u32), Vec<MultiBoundaryId>>::new();
        for boundary in &boundaries {
            at_vertex
                .entry(boundary.start)
                .or_default()
                .push(boundary.id);
            at_vertex.entry(boundary.end).or_default().push(boundary.id);
        }
        let junctions = at_vertex
            .into_iter()
            .filter(|(_, ids)| ids.len() != 2)
            .map(|(at, mut incident_boundaries)| {
                incident_boundaries.sort();
                let incident_regions = incident_boundaries
                    .iter()
                    .flat_map(|id| {
                        let b = &boundaries[id.0 as usize];
                        [b.left, b.right]
                    })
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
                MultiJunction {
                    at,
                    incident_boundaries,
                    incident_regions,
                }
            })
            .collect::<Vec<_>>();

        let mut inventory = BTreeMap::<(RegionId, RegionId), u64>::new();
        for boundary in &boundaries {
            let pair = if boundary.left < boundary.right {
                (boundary.left, boundary.right)
            } else {
                (boundary.right, boundary.left)
            };
            *inventory.entry(pair).or_default() += 1;
        }
        let expected = rag
            .edges
            .iter()
            .map(|edge| ((edge.a, edge.b), edge.shared_grid_segments))
            .collect::<BTreeMap<_, _>>();
        if inventory != expected {
            return Err(MultiDcelError::RagInventoryMismatch);
        }
        let digest_sha256 = digest(&rag.digest_sha256, &boundaries, &faces, &junctions);
        Ok(Self {
            schema: MULTICOLOR_DCEL_SCHEMA,
            rag_sha256: rag.digest_sha256,
            boundaries,
            faces,
            junctions,
            digest_sha256,
        })
    }
}

fn endpoints(boundaries: &[MultiBoundary], h: MultiHalfEdgeId) -> ((u32, u32), (u32, u32)) {
    let b = &boundaries[h.boundary().0 as usize];
    if h.is_forward() {
        (b.start, b.end)
    } else {
        (b.end, b.start)
    }
}

fn direction(a: (u32, u32), b: (u32, u32)) -> u8 {
    match (b.0 as i64 - a.0 as i64, b.1 as i64 - a.1 as i64) {
        (1, 0) => 0,  // east
        (0, -1) => 1, // north
        (-1, 0) => 2, // west
        (0, 1) => 3,  // south
        _ => unreachable!("all multicolour boundaries are unit grid segments"),
    }
}

fn face_loops(
    region: RegionId,
    boundaries: &[MultiBoundary],
    halfedges: &[MultiHalfEdgeId],
) -> Result<Vec<Vec<MultiHalfEdgeId>>, MultiDcelError> {
    let mut outgoing = BTreeMap::<(u32, u32), Vec<MultiHalfEdgeId>>::new();
    for &h in halfedges {
        let (start, _) = endpoints(boundaries, h);
        outgoing.entry(start).or_default().push(h);
    }
    for ids in outgoing.values_mut() {
        ids.sort();
    }
    let mut successor = BTreeMap::new();
    for &h in halfedges {
        let (start, end) = endpoints(boundaries, h);
        let incoming = direction(start, end);
        let candidates = outgoing
            .get(&end)
            .ok_or(MultiDcelError::OpenFaceWalk { region, at: end })?;
        let next = candidates
            .iter()
            .copied()
            .min_by_key(|candidate| {
                let (_, candidate_end) = endpoints(boundaries, *candidate);
                let outgoing_direction = direction(end, candidate_end);
                ((outgoing_direction + 4 - incoming) % 4, candidate.0)
            })
            .ok_or(MultiDcelError::OpenFaceWalk { region, at: end })?;
        successor.insert(h, next);
    }
    // A valid rotation system is a permutation: every halfedge has exactly
    // one predecessor as well as one successor.
    let predecessors = successor.values().copied().collect::<BTreeSet<_>>();
    if predecessors.len() != halfedges.len() {
        let at = halfedges
            .first()
            .map(|h| endpoints(boundaries, *h).0)
            .unwrap_or((0, 0));
        return Err(MultiDcelError::OpenFaceWalk { region, at });
    }
    let mut unseen = halfedges.iter().copied().collect::<BTreeSet<_>>();
    let mut loops = Vec::new();
    while let Some(start) = unseen.first().copied() {
        let mut loop_edges = Vec::new();
        let mut current = start;
        loop {
            if !unseen.remove(&current) {
                if current == start {
                    break;
                }
                return Err(MultiDcelError::OpenFaceWalk {
                    region,
                    at: endpoints(boundaries, current).0,
                });
            }
            loop_edges.push(current);
            current = successor[&current];
        }
        loops.push(loop_edges);
    }
    loops.sort();
    Ok(loops)
}

fn digest(
    rag: &str,
    boundaries: &[MultiBoundary],
    faces: &[MultiFace],
    junctions: &[MultiJunction],
) -> String {
    let mut h = Sha256::new();
    h.update(MULTICOLOR_DCEL_SCHEMA.as_bytes());
    h.update(rag.as_bytes());
    for b in boundaries {
        h.update(b.id.0.to_le_bytes());
        for v in [b.start.0, b.start.1, b.end.0, b.end.1, b.left.0, b.right.0] {
            h.update(v.to_le_bytes());
        }
    }
    for face in faces {
        h.update(face.region.0.to_le_bytes());
        h.update(face.palette_label.to_le_bytes());
        h.update([u8::from(face.is_exterior)]);
        for lp in &face.loops {
            h.update((lp.len() as u64).to_le_bytes());
            for edge in lp {
                h.update(edge.0.to_le_bytes());
            }
        }
    }
    for junction in junctions {
        h.update(junction.at.0.to_le_bytes());
        h.update(junction.at.1.to_le_bytes());
        for id in &junction.incident_boundaries {
            h.update(id.0.to_le_bytes());
        }
        for region in &junction.incident_regions {
            h.update(region.0.to_le_bytes());
        }
    }
    hex::encode(h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(values: Vec<u16>) -> RegionLabelling {
        RegionLabelling::new(3, 3, values, Some(0)).unwrap()
    }

    #[test]
    fn every_shared_segment_has_exactly_two_derived_twins_and_two_faces() {
        let l = labels(vec![0, 0, 0, 0, 1, 2, 0, 3, 2]);
        let d = MulticolorDcel::assemble(&l).unwrap();
        assert!(d.boundaries.iter().all(|b| b.left != b.right));
        for boundary in &d.boundaries {
            let a = MultiHalfEdgeId::new(boundary.id, true);
            let b = MultiHalfEdgeId::new(boundary.id, false);
            assert_eq!(a.twin(), b);
            assert_eq!(b.twin(), a);
            let uses = d
                .faces
                .iter()
                .flat_map(|f| f.loops.iter().flatten())
                .filter(|h| h.boundary() == boundary.id)
                .count();
            assert_eq!(uses, 2);
        }
        assert!(d.junctions.iter().any(|j| j.incident_regions.len() >= 3));
    }

    #[test]
    fn all_face_cycles_are_closed_by_endpoint_identity() {
        let d = MulticolorDcel::assemble(&labels(vec![0, 0, 0, 0, 1, 2, 0, 3, 2])).unwrap();
        for face in &d.faces {
            for lp in &face.loops {
                for i in 0..lp.len() {
                    let (_, end) = endpoints(&d.boundaries, lp[i]);
                    let (next_start, _) = endpoints(&d.boundaries, lp[(i + 1) % lp.len()]);
                    assert_eq!(end, next_start, "face {:?}", face.region);
                }
            }
        }
    }

    #[test]
    fn a_palette_id_permutation_has_identical_multidcel_bytes() {
        let a = labels(vec![0, 0, 0, 0, 7, 9, 0, 3, 9]);
        let b = labels(vec![0, 0, 0, 0, 21, 4, 0, 18, 4]);
        assert_eq!(MulticolorDcel::assemble(&a), MulticolorDcel::assemble(&b));
    }

    #[test]
    fn opaque_full_bleed_uses_a_synthetic_exterior_not_a_border_majority() {
        let l = RegionLabelling::new(2, 2, vec![1, 2, 1, 2], None).unwrap();
        let dcel = MulticolorDcel::assemble(&l).unwrap();
        let rag = RegionAdjacencyGraph::build(&l).unwrap();
        assert_eq!(rag.exterior, Some(RegionId(0)));
        assert_eq!(rag.nodes[0].pixels, 0);
        assert!(rag.nodes[0].is_exterior);
        assert!(dcel.faces[0].is_exterior);
    }
}
