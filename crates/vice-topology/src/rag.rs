//! Canonical multiregion labelling and region-adjacency graph (M8).
//!
//! Palette labels are not region identities: disconnected pixels with the
//! same paint are distinct regions. Conversely, all boundary-connected
//! pixels of a declared transparent exterior label are connected through the
//! outside of the canvas. An opaque full-bleed observation has no such pixel
//! label, so the graph creates a zero-pixel synthetic exterior face instead
//! of guessing that one border colour owns infinity.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use sha2::{Digest, Sha256};

pub const RAG_SCHEMA: &str = "vice-classic/region-adjacency-graph/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct RegionId(pub u32);

impl RegionId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionLabelling {
    width_px: usize,
    height_px: usize,
    labels: Vec<u16>,
    exterior_label: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RagError {
    #[error("region labelling dimensions must be non-zero")]
    EmptyDimensions,
    #[error("region labelling has {actual} cells, expected {expected}")]
    ShapeMismatch { expected: usize, actual: usize },
    #[error("region labelling dimensions overflow the address space")]
    DimensionOverflow,
    #[error("declared exterior label {label} has no canvas-border pixel")]
    ExteriorDoesNotTouchBorder { label: u16 },
    #[error("the graph has too many regions for its u32 identity space")]
    RegionSpaceExhausted,
}

impl RegionLabelling {
    pub fn new(
        width_px: usize,
        height_px: usize,
        labels: Vec<u16>,
        exterior_label: Option<u16>,
    ) -> Result<Self, RagError> {
        if width_px == 0 || height_px == 0 {
            return Err(RagError::EmptyDimensions);
        }
        let expected = width_px
            .checked_mul(height_px)
            .ok_or(RagError::DimensionOverflow)?;
        if labels.len() != expected {
            return Err(RagError::ShapeMismatch {
                expected,
                actual: labels.len(),
            });
        }
        if let Some(label) = exterior_label {
            let touches = (0..height_px).any(|y| {
                (0..width_px).any(|x| {
                    (x == 0 || y == 0 || x + 1 == width_px || y + 1 == height_px)
                        && labels[y * width_px + x] == label
                })
            });
            if !touches {
                return Err(RagError::ExteriorDoesNotTouchBorder { label });
            }
        }
        Ok(Self {
            width_px,
            height_px,
            labels,
            exterior_label,
        })
    }

    pub fn width_px(&self) -> usize {
        self.width_px
    }
    pub fn height_px(&self) -> usize {
        self.height_px
    }
    pub fn labels(&self) -> &[u16] {
        &self.labels
    }
    pub fn exterior_label(&self) -> Option<u16> {
        self.exterior_label
    }

    /// Canonicalize arbitrary palette ids.  Exterior is zero; all other ids
    /// are assigned by first raster occurrence.  A pure label permutation
    /// therefore cannot change the serialized graph.
    pub fn canonicalized(&self) -> RegionLabelling {
        let mut next = u16::from(self.exterior_label.is_some());
        let mut map = BTreeMap::new();
        if let Some(ext) = self.exterior_label {
            map.insert(ext, 0);
        }
        let labels = self
            .labels
            .iter()
            .map(|label| {
                if let Some(canonical) = map.get(label) {
                    *canonical
                } else {
                    let canonical = next;
                    next = next.saturating_add(1);
                    map.insert(*label, canonical);
                    canonical
                }
            })
            .collect();
        RegionLabelling {
            width_px: self.width_px,
            height_px: self.height_px,
            labels,
            exterior_label: self.exterior_label.map(|_| 0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RagNode {
    pub id: RegionId,
    pub palette_label: u16,
    pub pixels: u64,
    pub border_segments: u64,
    pub is_exterior: bool,
    /// First member in raster order.  This is the canonical component anchor.
    pub anchor_pixel: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RagEdge {
    pub a: RegionId,
    pub b: RegionId,
    pub shared_grid_segments: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegionAdjacencyGraph {
    pub schema: &'static str,
    pub width_px: usize,
    pub height_px: usize,
    pub nodes: Vec<RagNode>,
    pub edges: Vec<RagEdge>,
    /// Region id at every source pixel; binds the abstract graph to evidence.
    pub region_of_pixel: Vec<RegionId>,
    pub exterior: Option<RegionId>,
    pub digest_sha256: String,
}

#[derive(Debug)]
struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
        }
    }
    fn root(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]];
            x = self.parent[x];
        }
        x
    }
    fn join(&mut self, a: usize, b: usize) {
        let ra = self.root(a);
        let rb = self.root(b);
        if ra != rb {
            let (keep, drop) = if ra < rb { (ra, rb) } else { (rb, ra) };
            self.parent[drop] = keep;
        }
    }
}

impl RegionAdjacencyGraph {
    pub fn build(source: &RegionLabelling) -> Result<Self, RagError> {
        let source = source.canonicalized();
        let (w, h) = (source.width_px, source.height_px);
        let mut uf = UnionFind::new(source.labels.len());
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                if x > 0 && source.labels[i] == source.labels[i - 1] {
                    uf.join(i, i - 1);
                }
                if y > 0 && source.labels[i] == source.labels[i - w] {
                    uf.join(i, i - w);
                }
            }
        }
        // The exterior face is connected outside the finite observation
        // window, even if its visible border pixels appear in separate arcs.
        if let Some(ext) = source.exterior_label {
            let border = (0..source.labels.len())
                .filter(|&i| {
                    let x = i % w;
                    let y = i / w;
                    source.labels[i] == ext && (x == 0 || y == 0 || x + 1 == w || y + 1 == h)
                })
                .collect::<Vec<_>>();
            if let Some((&first, rest)) = border.split_first() {
                for &i in rest {
                    uf.join(first, i);
                }
            }
        }

        let roots = (0..source.labels.len())
            .map(|i| uf.root(i))
            .collect::<Vec<_>>();
        let exterior_root = source.exterior_label.and_then(|ext| {
            roots
                .iter()
                .enumerate()
                .find(|(i, _)| source.labels[*i] == ext)
                .map(|(_, root)| *root)
        });
        let mut anchors = BTreeMap::<usize, usize>::new();
        for (i, root) in roots.iter().copied().enumerate() {
            anchors.entry(root).or_insert(i);
        }
        let mut ordered_roots = anchors.keys().copied().collect::<Vec<_>>();
        ordered_roots.sort_by_key(|root| {
            (
                Some(*root) != exterior_root,
                anchors.get(root).copied().unwrap_or(usize::MAX),
            )
        });
        if ordered_roots.len() > u32::MAX as usize {
            return Err(RagError::RegionSpaceExhausted);
        }
        let synthetic_exterior = exterior_root.is_none();
        let region_offset = u32::from(synthetic_exterior);
        let root_to_region = ordered_roots
            .iter()
            .enumerate()
            .map(|(id, root)| (*root, RegionId(id as u32 + region_offset)))
            .collect::<BTreeMap<_, _>>();
        let region_of_pixel = roots
            .iter()
            .map(|root| root_to_region[root])
            .collect::<Vec<_>>();

        let mut nodes = Vec::with_capacity(ordered_roots.len() + usize::from(synthetic_exterior));
        if synthetic_exterior {
            nodes.push(RagNode {
                id: RegionId(0),
                palette_label: u16::MAX,
                pixels: 0,
                border_segments: (2 * w + 2 * h) as u64,
                is_exterior: true,
                anchor_pixel: u64::MAX,
            });
        }
        nodes.extend(ordered_roots.iter().enumerate().map(|(id, root)| RagNode {
            id: RegionId(id as u32 + region_offset),
            palette_label: source.labels[anchors[root]],
            pixels: 0,
            border_segments: 0,
            is_exterior: Some(*root) == exterior_root,
            anchor_pixel: anchors[root] as u64,
        }));
        for (i, region) in region_of_pixel.iter().copied().enumerate() {
            nodes[region.index()].pixels += 1;
            let x = i % w;
            let y = i / w;
            nodes[region.index()].border_segments += u64::from(x == 0)
                + u64::from(y == 0)
                + u64::from(x + 1 == w)
                + u64::from(y + 1 == h);
        }

        let exterior = Some(
            exterior_root
                .map(|root| root_to_region[&root])
                .unwrap_or(RegionId(0)),
        );
        let mut adjacency = BTreeMap::<(RegionId, RegionId), u64>::new();
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                for j in [(x + 1 < w).then_some(i + 1), (y + 1 < h).then_some(i + w)]
                    .into_iter()
                    .flatten()
                {
                    let a = region_of_pixel[i];
                    let b = region_of_pixel[j];
                    if a != b {
                        let pair = if a < b { (a, b) } else { (b, a) };
                        *adjacency.entry(pair).or_default() += 1;
                    }
                }
            }
        }
        // A non-exterior region on the canvas rim is adjacent to the declared
        // exterior THROUGH the outside ring, even where no exterior pixel is
        // visible inside the finite window.
        if let Some(ext) = exterior {
            for node in &nodes {
                if node.id != ext && node.border_segments > 0 {
                    let pair = if node.id < ext {
                        (node.id, ext)
                    } else {
                        (ext, node.id)
                    };
                    *adjacency.entry(pair).or_default() += node.border_segments;
                }
            }
        }
        let edges = adjacency
            .into_iter()
            .map(|((a, b), shared_grid_segments)| RagEdge {
                a,
                b,
                shared_grid_segments,
            })
            .collect::<Vec<_>>();
        let digest_sha256 = digest(w, h, &nodes, &edges, &region_of_pixel, exterior);
        Ok(Self {
            schema: RAG_SCHEMA,
            width_px: w,
            height_px: h,
            nodes,
            edges,
            region_of_pixel,
            exterior,
            digest_sha256,
        })
    }

    pub fn neighbours(&self, region: RegionId) -> BTreeSet<RegionId> {
        self.edges
            .iter()
            .filter_map(|edge| {
                if edge.a == region {
                    Some(edge.b)
                } else if edge.b == region {
                    Some(edge.a)
                } else {
                    None
                }
            })
            .collect()
    }
}

fn digest(
    width: usize,
    height: usize,
    nodes: &[RagNode],
    edges: &[RagEdge],
    region_of_pixel: &[RegionId],
    exterior: Option<RegionId>,
) -> String {
    let mut h = Sha256::new();
    h.update(RAG_SCHEMA.as_bytes());
    h.update((width as u64).to_le_bytes());
    h.update((height as u64).to_le_bytes());
    h.update(exterior.map(|r| r.0).unwrap_or(u32::MAX).to_le_bytes());
    for node in nodes {
        h.update(node.id.0.to_le_bytes());
        h.update(node.palette_label.to_le_bytes());
        h.update(node.pixels.to_le_bytes());
        h.update(node.border_segments.to_le_bytes());
        h.update([u8::from(node.is_exterior)]);
        h.update(node.anchor_pixel.to_le_bytes());
    }
    for edge in edges {
        h.update(edge.a.0.to_le_bytes());
        h.update(edge.b.0.to_le_bytes());
        h.update(edge.shared_grid_segments.to_le_bytes());
    }
    for region in region_of_pixel {
        h.update(region.0.to_le_bytes());
    }
    hex::encode(h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labelling(rows: &[&[u16]], exterior: Option<u16>) -> RegionLabelling {
        let h = rows.len();
        let w = rows[0].len();
        RegionLabelling::new(
            w,
            h,
            rows.iter().flat_map(|r| r.iter().copied()).collect(),
            exterior,
        )
        .unwrap()
    }

    #[test]
    fn three_stripes_form_a_symmetric_two_edge_path() {
        let l = labelling(&[&[7, 7, 9, 9, 2, 2], &[7, 7, 9, 9, 2, 2]], None);
        let g = RegionAdjacencyGraph::build(&l).unwrap();
        assert_eq!(g.nodes.len(), 4, "three visible stripes plus outside");
        let interior_edges = g
            .edges
            .iter()
            .filter(|edge| edge.a != RegionId(0) && edge.b != RegionId(0))
            .collect::<Vec<_>>();
        assert_eq!(interior_edges.len(), 2);
        assert_eq!(
            g.neighbours(RegionId(2))
                .into_iter()
                .filter(|region| *region != RegionId(0))
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([RegionId(1), RegionId(3)])
        );
        assert!(interior_edges
            .iter()
            .all(|edge| edge.shared_grid_segments == 2));
    }

    #[test]
    fn palette_label_permutation_cannot_change_the_graph_or_digest() {
        let a = labelling(&[&[5, 5, 8], &[5, 3, 8], &[3, 3, 8]], None);
        let b = labelling(&[&[91, 91, 4], &[91, 17, 4], &[17, 17, 4]], None);
        assert_eq!(
            RegionAdjacencyGraph::build(&a),
            RegionAdjacencyGraph::build(&b)
        );
    }

    #[test]
    fn disconnected_equal_paints_are_regions_not_one_colour_node() {
        let l = labelling(&[&[1, 2, 1], &[1, 2, 1]], None);
        let g = RegionAdjacencyGraph::build(&l).unwrap();
        assert_eq!(g.nodes.len(), 4, "three visible regions plus outside");
        assert_eq!(g.nodes.iter().filter(|n| n.palette_label == 0).count(), 2);
    }

    #[test]
    fn exterior_arcs_are_joined_through_the_outside_canvas() {
        let l = labelling(&[&[0, 1, 0], &[1, 1, 1], &[0, 1, 0]], Some(0));
        let g = RegionAdjacencyGraph::build(&l).unwrap();
        assert_eq!(g.nodes.len(), 2);
        assert_eq!(g.exterior, Some(RegionId(0)));
        assert!(g.nodes[0].is_exterior);
        assert_eq!(g.nodes[0].pixels, 4);
    }

    #[test]
    fn malformed_shape_and_hidden_exterior_are_typed_refusals() {
        assert!(matches!(
            RegionLabelling::new(2, 2, vec![1, 2], None),
            Err(RagError::ShapeMismatch { .. })
        ));
        assert!(matches!(
            RegionLabelling::new(3, 3, vec![1, 1, 1, 1, 7, 1, 1, 1, 1], Some(7)),
            Err(RagError::ExteriorDoesNotTouchBorder { label: 7 })
        ));
    }
}
