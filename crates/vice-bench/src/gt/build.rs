//! An independent planar-scene builder for the GT corpus.
//!
//! Written from the §6.1 / §12 contract rather than reused from the
//! renderer's or the IR's test helpers, because spec §27.1 forbids the
//! inverse crime: a corpus assembled by the same code the system under test
//! uses would agree with it for reasons that have nothing to do with
//! correctness. The builder here is region-first (rings separate an inner
//! region from an outer one); the in-tree test helpers are boundary-first
//! (a flat list of boundary specs with faces named per boundary). Same
//! contract, different construction — which is the point.
//!
//! Everything the builder produces goes through `ValidatedScene` and then
//! through `CertifiedMesh`, so a fixture that is not a certified planar
//! embedding cannot enter the corpus at all (debt D-4).

use vice_geom::Pt;
use vice_ir::{
    Boundary, BoundaryId, Canvas, CurveChain, Face, FaceId, GlobalFormationHypothesis, GraphVertex,
    HalfEdge, HalfEdgeId, Paint, PlanarGraph, Segment, ValidatedScene, VectorScene, VertexId,
};

/// Half-edge index of one direction of a boundary. Layout is fixed:
/// forward = `2b`, reverse = `2b + 1`.
fn he_index(boundary: usize, forward: bool) -> usize {
    2 * boundary + usize::from(!forward)
}

#[derive(Debug, Clone)]
struct BoundaryDraft {
    start: usize,
    end: usize,
    chain: CurveChain,
    left: usize,
    right: usize,
}

#[derive(Debug, Clone)]
struct FaceDraft {
    paint: Paint,
    /// Each loop is a cyclic sequence of `(boundary, forward)`.
    loops: Vec<Vec<(usize, bool)>>,
}

/// Region-first builder of a visible planar partition.
#[derive(Debug, Clone)]
pub struct SceneBuilder {
    canvas: Canvas,
    formation: GlobalFormationHypothesis,
    vertices: Vec<Pt>,
    boundaries: Vec<BoundaryDraft>,
    faces: Vec<FaceDraft>,
}

/// Why a draft could not become a scene.
#[derive(Debug, Clone, thiserror::Error)]
pub enum BuildError {
    #[error("ring needs at least 3 points, got {got}")]
    RingTooShort { got: usize },
    #[error("ring has {points} points but {segments} segments")]
    RingArity { points: usize, segments: usize },
    #[error("face index {got} out of range ({faces} faces)")]
    FaceOutOfRange { got: usize, faces: usize },
    #[error("inner and outer region of a ring are the same face {face}")]
    RingSeparatesNothing { face: usize },
    #[error(transparent)]
    Invalid(#[from] vice_ir::SceneError),
}

impl SceneBuilder {
    /// A canvas with a single transparent exterior face (index 0).
    pub fn new(
        width_px: u32,
        height_px: u32,
        formation: GlobalFormationHypothesis,
    ) -> SceneBuilder {
        SceneBuilder {
            canvas: Canvas {
                width_px,
                height_px,
            },
            formation,
            vertices: Vec::new(),
            boundaries: Vec::new(),
            faces: vec![FaceDraft {
                paint: Paint::TransparentExterior,
                loops: Vec::new(),
            }],
        }
    }

    /// Index of the exterior face.
    pub const EXTERIOR: usize = 0;

    pub fn set_exterior_paint(&mut self, paint: Paint) {
        self.faces[Self::EXTERIOR].paint = paint;
    }

    pub fn add_face(&mut self, paint: Paint) -> usize {
        self.faces.push(FaceDraft {
            paint,
            loops: Vec::new(),
        });
        self.faces.len() - 1
    }

    /// Add a shared graph vertex. Positions must stay distinct: the IR
    /// forbids duplicates precisely so that two faces meeting on a line
    /// share ONE boundary instead of authoring it twice.
    pub fn add_vertex(&mut self, p: Pt) -> usize {
        self.vertices.push(p);
        self.vertices.len() - 1
    }

    /// Add one boundary between two existing vertices. `left` is the face on
    /// the algebraic left when traveling start -> end.
    pub fn add_boundary(
        &mut self,
        start: usize,
        end: usize,
        chain: CurveChain,
        left: usize,
        right: usize,
    ) -> Result<usize, BuildError> {
        for f in [left, right] {
            if f >= self.faces.len() {
                return Err(BuildError::FaceOutOfRange {
                    got: f,
                    faces: self.faces.len(),
                });
            }
        }
        self.boundaries.push(BoundaryDraft {
            start,
            end,
            chain,
            left,
            right,
        });
        Ok(self.boundaries.len() - 1)
    }

    /// Declare one loop of a face as a cyclic sequence of directed
    /// boundaries. Used when the region is not a single ring — e.g. two
    /// faces abutting on a shared chord, where the chord belongs to both
    /// loops with opposite directions.
    pub fn add_face_loop(
        &mut self,
        face: usize,
        edges: &[(usize, bool)],
    ) -> Result<(), BuildError> {
        if face >= self.faces.len() {
            return Err(BuildError::FaceOutOfRange {
                got: face,
                faces: self.faces.len(),
            });
        }
        self.faces[face].loops.push(edges.to_vec());
        Ok(())
    }

    /// Add a closed ring of boundaries separating `inner` from `outer`.
    ///
    /// `points` are the ring's corner positions in the order that puts
    /// `inner` on the algebraic left; `segments[i]` joins `points[i]` to
    /// `points[i + 1]` cyclically. The ring contributes one loop to each of
    /// the two faces — forward for `inner`, reversed for `outer` — which is
    /// what makes a shared boundary stored exactly once.
    pub fn add_ring(
        &mut self,
        points: &[Pt],
        segments: &[Segment],
        inner: usize,
        outer: usize,
    ) -> Result<(), BuildError> {
        if points.len() < 3 {
            return Err(BuildError::RingTooShort { got: points.len() });
        }
        if points.len() != segments.len() {
            return Err(BuildError::RingArity {
                points: points.len(),
                segments: segments.len(),
            });
        }
        for f in [inner, outer] {
            if f >= self.faces.len() {
                return Err(BuildError::FaceOutOfRange {
                    got: f,
                    faces: self.faces.len(),
                });
            }
        }
        if inner == outer {
            return Err(BuildError::RingSeparatesNothing { face: inner });
        }

        let vs: Vec<usize> = points.iter().map(|p| self.add_vertex(*p)).collect();
        let first_boundary = self.boundaries.len();
        for (i, seg) in segments.iter().enumerate() {
            let j = (i + 1) % vs.len();
            self.boundaries.push(BoundaryDraft {
                start: vs[i],
                end: vs[j],
                chain: CurveChain::single(seg.clone()),
                left: inner,
                right: outer,
            });
        }
        let n = segments.len();
        let inner_loop: Vec<(usize, bool)> = (0..n).map(|i| (first_boundary + i, true)).collect();
        // The outer face walks the same boundaries backwards, in reverse
        // order, so its loop is the mirror cycle and has the opposite sign.
        let outer_loop: Vec<(usize, bool)> =
            (0..n).rev().map(|i| (first_boundary + i, false)).collect();
        self.faces[inner].loops.push(inner_loop);
        self.faces[outer].loops.push(outer_loop);
        Ok(())
    }

    /// Convenience: a straight-edged ring.
    pub fn add_polygon_ring(
        &mut self,
        points: &[Pt],
        inner: usize,
        outer: usize,
    ) -> Result<(), BuildError> {
        let segs = vec![Segment::Line; points.len()];
        self.add_ring(points, &segs, inner, outer)
    }

    /// Materialize, validate and return the scene.
    pub fn build(self) -> Result<ValidatedScene, BuildError> {
        let mut half_edges = vec![
            HalfEdge {
                boundary: BoundaryId(0),
                forward: true,
                twin: HalfEdgeId(0),
                next: HalfEdgeId(0),
                face: FaceId(0),
            };
            2 * self.boundaries.len()
        ];
        for (b, draft) in self.boundaries.iter().enumerate() {
            let (f, r) = (he_index(b, true), he_index(b, false));
            half_edges[f] = HalfEdge {
                boundary: BoundaryId(b as u32),
                forward: true,
                twin: HalfEdgeId(r as u32),
                next: HalfEdgeId(f as u32),
                face: FaceId(draft.left as u32),
            };
            half_edges[r] = HalfEdge {
                boundary: BoundaryId(b as u32),
                forward: false,
                twin: HalfEdgeId(f as u32),
                next: HalfEdgeId(r as u32),
                face: FaceId(draft.right as u32),
            };
        }

        let mut faces = Vec::with_capacity(self.faces.len());
        for draft in &self.faces {
            let mut reps = Vec::with_capacity(draft.loops.len());
            for lp in &draft.loops {
                for (k, (b, fwd)) in lp.iter().enumerate() {
                    let (nb, nfwd) = lp[(k + 1) % lp.len()];
                    half_edges[he_index(*b, *fwd)].next = HalfEdgeId(he_index(nb, nfwd) as u32);
                }
                let (b0, f0) = lp[0];
                reps.push(HalfEdgeId(he_index(b0, f0) as u32));
            }
            faces.push(Face {
                loops: reps,
                paint: draft.paint,
            });
        }

        let graph = PlanarGraph {
            exterior: FaceId(Self::EXTERIOR as u32),
            vertices: self
                .vertices
                .iter()
                .map(|p| GraphVertex { pos: *p })
                .collect(),
            boundaries: self
                .boundaries
                .iter()
                .map(|d| Boundary {
                    left_face: FaceId(d.left as u32),
                    right_face: FaceId(d.right as u32),
                    start_vertex: VertexId(d.start as u32),
                    end_vertex: VertexId(d.end as u32),
                    curve: d.chain.clone(),
                })
                .collect(),
            half_edges,
            faces,
        };
        Ok(ValidatedScene::new(VectorScene {
            canvas: self.canvas,
            graph,
            formation: self.formation,
        })?)
    }
}

/// Shoelace signed area of a closed point ring (the ring is NOT repeated;
/// the last point closes to the first). Positive means the region on the
/// algebraic left is enclosed.
pub fn ring_signed_area(points: &[Pt]) -> f64 {
    let n = points.len();
    let mut sum = 0.0;
    for i in 0..n {
        let a = points[i];
        let b = points[(i + 1) % n];
        sum += a.x * b.y - b.x * a.y;
    }
    0.5 * sum
}

/// A regular polygon ring with positive signed area.
pub fn regular_polygon(center: Pt, radius: f64, sides: usize, phase_rad: f64) -> Vec<Pt> {
    let mut pts: Vec<Pt> = (0..sides)
        .map(|i| {
            let t = phase_rad + std::f64::consts::TAU * (i as f64) / (sides as f64);
            Pt::new(center.x + radius * t.cos(), center.y + radius * t.sin())
        })
        .collect();
    if ring_signed_area(&pts) < 0.0 {
        pts.reverse();
    }
    pts
}

/// Reverse a ring, flipping the side its face is on.
pub fn reversed(points: &[Pt]) -> Vec<Pt> {
    let mut v = points.to_vec();
    v.reverse();
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use vice_ir::{BlendSpace, ExteriorModel, LinearRgb, PixelFilter, QuantizationModel};
    use vice_render::{render_partition, CertifiedMesh, RenderOptions};

    fn formation() -> GlobalFormationHypothesis {
        GlobalFormationHypothesis {
            blend_space: BlendSpace::LinearLight,
            pixel_filter: PixelFilter::Box,
            quantization: QuantizationModel::Uint8,
            exterior: ExteriorModel::Transparent,
        }
    }

    fn ink() -> Paint {
        Paint::OpaqueSolid(LinearRgb {
            r: 0.2,
            g: 0.4,
            b: 0.8,
        })
    }

    #[test]
    fn an_island_builds_certifies_and_covers_its_own_area() {
        let mut b = SceneBuilder::new(32, 32, formation());
        let f = b.add_face(ink());
        let square = [
            Pt::new(8.0, 8.0),
            Pt::new(24.0, 8.0),
            Pt::new(24.0, 24.0),
            Pt::new(8.0, 24.0),
        ];
        assert!(ring_signed_area(&square) > 0.0, "positive orientation");
        b.add_polygon_ring(&square, f, SceneBuilder::EXTERIOR)
            .unwrap();
        let scene = b.build().expect("valid scene");
        CertifiedMesh::from_scene(&scene, RenderOptions::default()).expect("certified embedding");

        let render = render_partition(&scene, &RenderOptions::default()).unwrap();
        let area: f64 = render.face_coverage[f].iter().sum();
        assert!((area - 256.0).abs() < 1e-9, "got {area}");
    }

    #[test]
    fn a_ring_with_a_hole_is_three_faces_and_the_hole_is_not_ink() {
        let mut b = SceneBuilder::new(32, 32, formation());
        let ring = b.add_face(ink());
        let hole = b.add_face(Paint::TransparentExterior);
        let outer = [
            Pt::new(4.0, 4.0),
            Pt::new(28.0, 4.0),
            Pt::new(28.0, 28.0),
            Pt::new(4.0, 28.0),
        ];
        let inner = [
            Pt::new(12.0, 12.0),
            Pt::new(20.0, 12.0),
            Pt::new(20.0, 20.0),
            Pt::new(12.0, 20.0),
        ];
        b.add_polygon_ring(&outer, ring, SceneBuilder::EXTERIOR)
            .unwrap();
        // The hole's own region is inside; the ring is outside it.
        b.add_polygon_ring(&inner, hole, ring).unwrap();
        let scene = b.build().expect("valid scene");
        let render = render_partition(&scene, &RenderOptions::default()).unwrap();
        let ring_area: f64 = render.face_coverage[ring].iter().sum();
        let hole_area: f64 = render.face_coverage[hole].iter().sum();
        assert!((hole_area - 64.0).abs() < 1e-9, "hole {hole_area}");
        assert!(
            (ring_area - (576.0 - 64.0)).abs() < 1e-9,
            "ring {ring_area}"
        );
    }

    #[test]
    fn two_faces_can_share_one_boundary_stored_once() {
        // Left and right rectangles meeting on x = 16: the shared segment is
        // one boundary with a face on each side.
        let mut b = SceneBuilder::new(32, 32, formation());
        let left = b.add_face(ink());
        let right = b.add_face(Paint::OpaqueSolid(LinearRgb {
            r: 0.9,
            g: 0.1,
            b: 0.1,
        }));
        let v = |x: f64, y: f64| Pt::new(x, y);
        let a = b.add_vertex(v(8.0, 8.0));
        let c = b.add_vertex(v(16.0, 8.0));
        let d = b.add_vertex(v(24.0, 8.0));
        let e = b.add_vertex(v(24.0, 24.0));
        let g = b.add_vertex(v(16.0, 24.0));
        let h = b.add_vertex(v(8.0, 24.0));
        let edge = |bld: &mut SceneBuilder, s: usize, t: usize, l: usize, r: usize| {
            bld.add_boundary(s, t, CurveChain::single(Segment::Line), l, r)
                .unwrap()
        };
        let ac = edge(&mut b, a, c, left, SceneBuilder::EXTERIOR);
        let cd = edge(&mut b, c, d, right, SceneBuilder::EXTERIOR);
        let de = edge(&mut b, d, e, right, SceneBuilder::EXTERIOR);
        let eg = edge(&mut b, e, g, right, SceneBuilder::EXTERIOR);
        let gh = edge(&mut b, g, h, left, SceneBuilder::EXTERIOR);
        let ha = edge(&mut b, h, a, left, SceneBuilder::EXTERIOR);
        let cg = edge(&mut b, c, g, left, right); // the shared boundary

        b.add_face_loop(left, &[(ac, true), (cg, true), (gh, true), (ha, true)])
            .unwrap();
        b.add_face_loop(right, &[(cd, true), (de, true), (eg, true), (cg, false)])
            .unwrap();
        b.add_face_loop(
            SceneBuilder::EXTERIOR,
            &[
                (ha, false),
                (gh, false),
                (eg, false),
                (de, false),
                (cd, false),
                (ac, false),
            ],
        )
        .unwrap();

        let scene = b.build().expect("valid scene");
        assert_eq!(
            scene.graph().boundaries.len(),
            7,
            "the shared edge is stored once, not twice"
        );
        let render = render_partition(&scene, &RenderOptions::default()).unwrap();
        let la: f64 = render.face_coverage[left].iter().sum();
        let ra: f64 = render.face_coverage[right].iter().sum();
        assert!((la - 128.0).abs() < 1e-9, "left {la}");
        assert!((ra - 128.0).abs() < 1e-9, "right {ra}");
    }

    #[test]
    fn structurally_broken_drafts_are_typed_refusals() {
        let mut b = SceneBuilder::new(16, 16, formation());
        let f = b.add_face(ink());
        assert!(matches!(
            b.add_polygon_ring(&[Pt::new(0.0, 0.0), Pt::new(1.0, 1.0)], f, 0),
            Err(BuildError::RingTooShort { .. })
        ));
        assert!(matches!(
            b.add_ring(
                &[Pt::new(0.0, 0.0), Pt::new(4.0, 0.0), Pt::new(4.0, 4.0)],
                &[Segment::Line],
                f,
                0
            ),
            Err(BuildError::RingArity { .. })
        ));
        assert!(matches!(
            b.add_polygon_ring(
                &[Pt::new(0.0, 0.0), Pt::new(4.0, 0.0), Pt::new(4.0, 4.0)],
                f,
                f
            ),
            Err(BuildError::RingSeparatesNothing { .. })
        ));
        assert!(matches!(
            b.add_polygon_ring(
                &[Pt::new(0.0, 0.0), Pt::new(4.0, 0.0), Pt::new(4.0, 4.0)],
                99,
                0
            ),
            Err(BuildError::FaceOutOfRange { .. })
        ));
    }

    /// A ring wired with the wrong orientation must not become a fixture:
    /// the builder does not "fix" it, the certification refuses it.
    #[test]
    fn a_reversed_ring_is_refused_by_certification_not_silently_corrected() {
        let mut b = SceneBuilder::new(32, 32, formation());
        let f = b.add_face(ink());
        let square = [
            Pt::new(8.0, 8.0),
            Pt::new(24.0, 8.0),
            Pt::new(24.0, 24.0),
            Pt::new(8.0, 24.0),
        ];
        b.add_polygon_ring(&reversed(&square), f, SceneBuilder::EXTERIOR)
            .unwrap();
        let scene = b.build().expect("combinatorially valid");
        assert!(
            CertifiedMesh::from_scene(&scene, RenderOptions::default()).is_err(),
            "an inside-out ring cannot enter the corpus"
        );
    }
}
