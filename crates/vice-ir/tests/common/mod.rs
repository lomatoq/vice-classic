//! Test-support scene builder shared by the vice-ir integration tests.
//!
//! Builds VALID scenes from island descriptions so that negative tests can
//! break exactly one invariant at a time and property tests can generate
//! diverse valid graphs. Test-only code: not part of the library API.

#![allow(dead_code)] // each integration-test binary uses a subset

use vice_geom::Pt;
use vice_ir::{
    Boundary, BoundaryId, Canvas, ChainNode, CurveChain, ExteriorModel, Face, FaceId,
    GlobalFormationHypothesis, GraphVertex, HalfEdge, HalfEdgeId, JoinKind, LinearRgb, Paint,
    PixelFilter, PlanarGraph, QuantizationModel, Segment, VectorScene, VertexId,
};

/// One island: a closed loop of `vertices.len()` boundaries, where
/// `chains[j]` runs from `vertices[j]` to `vertices[(j+1) % k]`.
/// An optional transparent hole (its own face) may sit inside.
pub struct Island {
    pub vertices: Vec<Pt>,
    pub chains: Vec<CurveChain>,
    pub color: LinearRgb,
    pub hole: Option<Ring>,
}

/// A closed ring: used for island holes.
pub struct Ring {
    pub vertices: Vec<Pt>,
    pub chains: Vec<CurveChain>,
}

/// Deterministically wire a valid scene out of islands.
///
/// Faces: 0 = exterior (TransparentExterior), then one opaque face per
/// island, then one TransparentExterior face per hole.
pub fn build_scene(width_px: u32, height_px: u32, islands: &[Island]) -> VectorScene {
    let mut g = PlanarGraph {
        exterior: FaceId(0),
        vertices: Vec::new(),
        boundaries: Vec::new(),
        half_edges: Vec::new(),
        faces: vec![Face {
            loops: Vec::new(),
            paint: Paint::TransparentExterior,
        }],
    };

    // Pre-assign face ids: islands first, then hole faces.
    let island_face_ids: Vec<FaceId> = (0..islands.len()).map(|i| FaceId(1 + i as u32)).collect();
    let mut next_face = 1 + islands.len() as u32;
    let hole_face_ids: Vec<Option<FaceId>> = islands
        .iter()
        .map(|isl| {
            isl.hole.as_ref().map(|_| {
                let id = FaceId(next_face);
                next_face += 1;
                id
            })
        })
        .collect();
    for isl in islands {
        g.faces.push(Face {
            loops: Vec::new(),
            paint: Paint::OpaqueSolid(isl.color),
        });
    }
    for _ in hole_face_ids.iter().flatten() {
        g.faces.push(Face {
            loops: Vec::new(),
            paint: Paint::TransparentExterior,
        });
    }

    for (i, isl) in islands.iter().enumerate() {
        let face = island_face_ids[i];
        add_ring(&mut g, &isl.vertices, &isl.chains, face, FaceId(0));
        if let Some(hole) = &isl.hole {
            let hole_face = hole_face_ids[i].expect("hole face id");
            // Hole ring: left = hole face (its interior), right = island.
            add_ring(&mut g, &hole.vertices, &hole.chains, hole_face, face);
        }
    }

    VectorScene {
        canvas: Canvas {
            width_px,
            height_px,
        },
        graph: g,
        formation: GlobalFormationHypothesis {
            blend_space: vice_ir::BlendSpace::LinearLight,
            pixel_filter: PixelFilter::Box,
            quantization: QuantizationModel::Uint8,
            exterior: ExteriorModel::Transparent,
        },
    }
}

/// Append one closed ring of boundaries. `left` is the face enclosed by the
/// forward walk, `right` the face on the other side.
fn add_ring(
    g: &mut PlanarGraph,
    vertices: &[Pt],
    chains: &[CurveChain],
    left: FaceId,
    right: FaceId,
) {
    assert_eq!(vertices.len(), chains.len(), "ring arity");
    let k = vertices.len();
    let v_base = g.vertices.len() as u32;
    let b_base = g.boundaries.len() as u32;
    let h_base = g.half_edges.len() as u32;

    for &p in vertices {
        g.vertices.push(GraphVertex { pos: p });
    }
    for (j, chain) in chains.iter().enumerate() {
        g.boundaries.push(Boundary {
            left_face: left,
            right_face: right,
            start_vertex: VertexId(v_base + j as u32),
            end_vertex: VertexId(v_base + ((j + 1) % k) as u32),
            closure_join: (k == 1).then_some(JoinKind::Corner),
            curve: chain.clone(),
        });
    }
    // Half-edges: boundary b_base+j -> forward h_base+2j, reverse h_base+2j+1.
    for j in 0..k {
        let jn = (j + 1) % k;
        let jp = (j + k - 1) % k;
        g.half_edges.push(HalfEdge {
            boundary: BoundaryId(b_base + j as u32),
            forward: true,
            twin: HalfEdgeId(h_base + 2 * j as u32 + 1),
            next: HalfEdgeId(h_base + 2 * jn as u32),
            face: left,
        });
        g.half_edges.push(HalfEdge {
            boundary: BoundaryId(b_base + j as u32),
            forward: false,
            twin: HalfEdgeId(h_base + 2 * j as u32),
            next: HalfEdgeId(h_base + 2 * jp as u32 + 1),
            face: right,
        });
    }
    g.faces[left.index()].loops.push(HalfEdgeId(h_base));
    g.faces[right.index()].loops.push(HalfEdgeId(h_base + 1));
}

/// Axis-aligned square island of `size` at `origin`, four line boundaries.
pub fn square_island(origin: Pt, size: f64, color: LinearRgb) -> Island {
    let (x, y, s) = (origin.x, origin.y, size);
    Island {
        vertices: vec![
            Pt::new(x, y),
            Pt::new(x + s, y),
            Pt::new(x + s, y + s),
            Pt::new(x, y + s),
        ],
        chains: vec![
            CurveChain::single(Segment::Line),
            CurveChain::single(Segment::Line),
            CurveChain::single(Segment::Line),
            CurveChain::single(Segment::Line),
        ],
        color,
        hole: None,
    }
}

/// Square island with a smaller transparent square hole inside.
pub fn square_with_hole(origin: Pt, size: f64, margin: f64, color: LinearRgb) -> Island {
    let mut isl = square_island(origin, size, color);
    let (x, y) = (origin.x + margin, origin.y + margin);
    let s = size - 2.0 * margin;
    assert!(s > 0.0, "hole margin too large");
    isl.hole = Some(Ring {
        vertices: vec![
            Pt::new(x, y),
            Pt::new(x + s, y),
            Pt::new(x + s, y + s),
            Pt::new(x, y + s),
        ],
        chains: vec![
            CurveChain::single(Segment::Line),
            CurveChain::single(Segment::Line),
            CurveChain::single(Segment::Line),
            CurveChain::single(Segment::Line),
        ],
    });
    isl
}

/// An island exercising every segment kind and both join kinds: a closed
/// 5-gon whose edges are line, circular arc, quad, cubic and elliptic arc,
/// with a two-segment chain containing an interior smooth node.
pub fn mixed_island(origin: Pt, color: LinearRgb) -> Island {
    let (x, y) = (origin.x, origin.y);
    let v0 = Pt::new(x, y);
    let v1 = Pt::new(x + 8.0, y);
    let v2 = Pt::new(x + 10.0, y + 6.0);
    let v3 = Pt::new(x + 5.0, y + 9.0);
    let v4 = Pt::new(x, y + 6.0);
    Island {
        vertices: vec![v0, v1, v2, v3, v4],
        chains: vec![
            // v0 -> v1: two-segment chain (line then quad) with a smooth
            // interior node carrying a shared tangent parameter.
            CurveChain {
                interior_nodes: vec![ChainNode {
                    pos: Pt::new(x + 4.0, y - 1.0),
                    join: JoinKind::SmoothG1 {
                        tangent_angle_rad: 0.25,
                    },
                }],
                segments: vec![
                    Segment::Line,
                    Segment::Quad {
                        ctrl: Pt::new(x + 6.0, y - 1.0),
                    },
                ],
            },
            // v1 -> v2: circular arc (chord ~ 6.32, r = 4 -> 2r >= chord).
            CurveChain::single(Segment::CircularArc {
                radius_px: 4.0,
                large_arc: false,
                ccw: true,
            }),
            // v2 -> v3: cubic.
            CurveChain::single(Segment::Cubic {
                ctrl1: Pt::new(x + 9.0, y + 8.0),
                ctrl2: Pt::new(x + 7.0, y + 9.5),
            }),
            // v3 -> v4: elliptic arc (chord ~ 5.83; generous radii).
            CurveChain::single(Segment::EllipticArc {
                rx_px: 6.0,
                ry_px: 4.0,
                x_axis_rotation_rad: 0.5,
                large_arc: false,
                ccw: false,
            }),
            // v4 -> v0: line.
            CurveChain::single(Segment::Line),
        ],
        color,
        hole: None,
    }
}

/// A single-vertex island whose loop is one boundary of two quads — the
/// closed-loop (start == end vertex) case.
pub fn loop_island(center: Pt, r: f64, color: LinearRgb) -> Island {
    let v0 = Pt::new(center.x, center.y - r);
    Island {
        vertices: vec![v0],
        chains: vec![CurveChain {
            interior_nodes: vec![ChainNode {
                pos: Pt::new(center.x, center.y + r),
                join: JoinKind::Corner,
            }],
            segments: vec![
                Segment::Quad {
                    ctrl: Pt::new(center.x - 2.0 * r, center.y),
                },
                Segment::Quad {
                    ctrl: Pt::new(center.x + 2.0 * r, center.y),
                },
            ],
        }],
        color,
        hole: None,
    }
}

pub fn red() -> LinearRgb {
    LinearRgb::new(0.8, 0.1, 0.1)
}

pub fn blue() -> LinearRgb {
    LinearRgb::new(0.1, 0.2, 0.9)
}

/// A small standard valid scene: one square island.
pub fn one_square_scene() -> VectorScene {
    build_scene(64, 64, &[square_island(Pt::new(8.0, 8.0), 16.0, red())])
}

/// The empty scene: exterior only.
pub fn empty_scene() -> VectorScene {
    VectorScene {
        canvas: Canvas {
            width_px: 32,
            height_px: 32,
        },
        graph: PlanarGraph::empty(),
        formation: GlobalFormationHypothesis {
            blend_space: vice_ir::BlendSpace::LinearLight,
            pixel_filter: PixelFilter::Box,
            quantization: QuantizationModel::Uint8,
            exterior: ExteriorModel::Transparent,
        },
    }
}

/// Apply index permutations to every entity vector, remapping all
/// references. `perm_*[old] = new`. The result describes the SAME scene
/// with a different construction order.
pub fn apply_permutation(
    scene: &VectorScene,
    perm_v: &[usize],
    perm_b: &[usize],
    perm_h: &[usize],
    perm_f: &[usize],
) -> VectorScene {
    let g = &scene.graph;
    assert_eq!(perm_v.len(), g.vertices.len());
    assert_eq!(perm_b.len(), g.boundaries.len());
    assert_eq!(perm_h.len(), g.half_edges.len());
    assert_eq!(perm_f.len(), g.faces.len());

    let mut vertices = vec![GraphVertex { pos: Pt::ZERO }; g.vertices.len()];
    for (old, v) in g.vertices.iter().enumerate() {
        vertices[perm_v[old]] = *v;
    }
    let placeholder = Boundary {
        left_face: FaceId(0),
        right_face: FaceId(0),
        start_vertex: VertexId(0),
        end_vertex: VertexId(0),
        closure_join: Some(JoinKind::Corner),
        curve: CurveChain::single(Segment::Line),
    };
    let mut boundaries = vec![placeholder; g.boundaries.len()];
    for (old, b) in g.boundaries.iter().enumerate() {
        boundaries[perm_b[old]] = Boundary {
            left_face: FaceId(perm_f[b.left_face.index()] as u32),
            right_face: FaceId(perm_f[b.right_face.index()] as u32),
            start_vertex: VertexId(perm_v[b.start_vertex.index()] as u32),
            end_vertex: VertexId(perm_v[b.end_vertex.index()] as u32),
            closure_join: b.closure_join,
            curve: b.curve.clone(),
        };
    }
    let he_placeholder = HalfEdge {
        boundary: BoundaryId(0),
        forward: true,
        twin: HalfEdgeId(0),
        next: HalfEdgeId(0),
        face: FaceId(0),
    };
    let mut half_edges = vec![he_placeholder; g.half_edges.len()];
    for (old, h) in g.half_edges.iter().enumerate() {
        half_edges[perm_h[old]] = HalfEdge {
            boundary: BoundaryId(perm_b[h.boundary.index()] as u32),
            forward: h.forward,
            twin: HalfEdgeId(perm_h[h.twin.index()] as u32),
            next: HalfEdgeId(perm_h[h.next.index()] as u32),
            face: FaceId(perm_f[h.face.index()] as u32),
        };
    }
    let face_placeholder = Face {
        loops: Vec::new(),
        paint: Paint::TransparentExterior,
    };
    let mut faces = vec![face_placeholder; g.faces.len()];
    for (old, f) in g.faces.iter().enumerate() {
        faces[perm_f[old]] = Face {
            loops: f
                .loops
                .iter()
                .map(|l| HalfEdgeId(perm_h[l.index()] as u32))
                .collect(),
            paint: f.paint,
        };
    }

    VectorScene {
        canvas: scene.canvas,
        graph: PlanarGraph {
            exterior: FaceId(perm_f[g.exterior.index()] as u32),
            vertices,
            boundaries,
            half_edges,
            faces,
        },
        formation: scene.formation,
    }
}
