//! Procedural GT scene grammar (spec §27.1 source 1).
//!
//! Written separately from any production or test-fixture generator, and
//! deliberately structural: a variant changes the TOPOLOGY, the geometry
//! family or the paint structure, not just a phase or a radius. That
//! distinction is the whole point of §27.4 — "нельзя раздуть n, добавив
//! сотни phase shifts одного логотипа" — so the grammar must not be able to
//! manufacture independence it does not have. Phase and size are handled by
//! the degradation matrix, where they belong, and where they are explicitly
//! CORRELATED renders of one group.
//!
//! Determinism: a fixed splitmix64 stream seeded from the group id, so the
//! corpus is reproducible from a clean checkout with no stored assets.

use std::f64::consts::{PI, TAU};

use vice_geom::Pt;
use vice_ir::{
    BlendSpace, ExteriorModel, GlobalFormationHypothesis, LinearRgb, Paint, PixelFilter,
    QuantizationModel, Segment,
};

use super::build::{regular_polygon, ring_signed_area, SceneBuilder};
use super::{AuthoredTruth, FixtureOrigin, GtScene, GtSourceGroup, SalientFeature};

/// The canonical authoring canvas. Every procedural scene is authored at
/// this size; the degradation matrix produces the actual render sizes, so
/// scene coordinates and render pixels never get confused.
pub const AUTHORING_CANVAS_PX: u32 = 256;

/// Deterministic splitmix64. Small, seedable per group, no dependency.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn from_label(label: &str) -> Rng {
        // FNV-1a of the label: the seed is a function of the group id, so
        // a group's geometry does not shift when another group is added.
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in label.as_bytes() {
            h ^= u64::from(*b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        Rng(h)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in `[lo, hi)`.
    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        let u = (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
        lo + (hi - lo) * u
    }

    pub fn pick<T: Copy>(&mut self, xs: &[T]) -> T {
        xs[(self.next_u64() % xs.len() as u64) as usize]
    }
}

pub fn flat2_formation(exterior: ExteriorModel) -> GlobalFormationHypothesis {
    GlobalFormationHypothesis {
        blend_space: BlendSpace::LinearLight,
        pixel_filter: PixelFilter::Box,
        quantization: QuantizationModel::Uint8,
        exterior,
    }
}

fn ink(r: f64, g: f64, b: f64) -> Paint {
    Paint::OpaqueSolid(LinearRgb { r, g, b })
}

/// Paint separation used as a salient feature: max abs channel difference.
fn separation(a: Paint, b: Paint) -> f64 {
    match (a, b) {
        (Paint::OpaqueSolid(x), Paint::OpaqueSolid(y)) => (x.r - y.r)
            .abs()
            .max((x.g - y.g).abs())
            .max((x.b - y.b).abs()),
        // Against a transparent exterior the separation is alpha, which is
        // total.
        _ => 1.0,
    }
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Pt> {
    vec![
        Pt::new(x0, y0),
        Pt::new(x1, y0),
        Pt::new(x1, y1),
        Pt::new(x0, y1),
    ]
}

/// A star polygon ring with positive signed area.
fn star(center: Pt, r_outer: f64, r_inner: f64, points: usize, phase: f64) -> Vec<Pt> {
    let mut pts = Vec::with_capacity(points * 2);
    for i in 0..points * 2 {
        let r = if i % 2 == 0 { r_outer } else { r_inner };
        let t = phase + PI * (i as f64) / (points as f64);
        pts.push(Pt::new(center.x + r * t.cos(), center.y + r * t.sin()));
    }
    if ring_signed_area(&pts) < 0.0 {
        pts.reverse();
    }
    pts
}

/// The structural families of the grammar. Each is a distinct SHAPE FAMILY
/// for split purposes (§27.1: splits hold whole shape families).
pub const SHAPE_FAMILIES: &[&str] = &[
    "polygon",
    "annulus",
    "nested_island",
    "two_islands",
    "shared_edge",
    "arc_disk",
    "bezier_blob",
    "thin_bridge",
    "dot_cluster",
    "l_shape",
    "triple_junction",
    "star",
];

/// Build every procedural source group.
///
/// `variants_per_family` structural variants of each family; each variant
/// is a separate independent group.
pub fn procedural_groups(variants_per_family: usize) -> Vec<GtSourceGroup> {
    let mut out = Vec::new();
    for family in SHAPE_FAMILIES {
        for v in 0..variants_per_family {
            let id = format!("proc/{family}/{v:03}");
            match build_variant(family, v, &id) {
                Ok(group) => out.push(group),
                Err(why) => panic!("procedural recipe {id} does not certify: {why}"),
            }
        }
    }
    out
}

fn group_of(
    id: &str,
    family: &str,
    scene: vice_ir::ValidatedScene,
    truth: AuthoredTruth,
    salient: Vec<SalientFeature>,
) -> Result<GtSourceGroup, String> {
    let scene_id = format!("{id}#a");
    let s = GtScene::new(scene_id, id, scene, truth, salient).map_err(|e| e.to_string())?;
    Ok(GtSourceGroup {
        id: id.to_string(),
        origin: FixtureOrigin::Procedural,
        shape_family: family.to_string(),
        provenance: "generated by vice-bench gt::grammar in this repository".to_string(),
        scenes: vec![s],
        equivalence_class: None,
        intentionally_ambiguous: false,
    })
}

fn build_variant(family: &str, v: usize, id: &str) -> Result<GtSourceGroup, String> {
    let mut rng = Rng::from_label(id);
    let c = f64::from(AUTHORING_CANVAS_PX);
    let exterior = if v % 3 == 2 {
        ExteriorModel::Opaque
    } else {
        ExteriorModel::Transparent
    };
    let mut b = SceneBuilder::new(
        AUTHORING_CANVAS_PX,
        AUTHORING_CANVAS_PX,
        flat2_formation(exterior),
    );
    let err = |e: super::build::BuildError| e.to_string();
    // An OPAQUE exterior is a full-bleed background FACE covering the
    // canvas, not a painted exterior face: §5.3 / ADR-0005 keep the
    // exterior face's paint TransparentExterior by contract, and §9.2 calls
    // the opaque case a full-bleed hypothesis. Shapes then sit on the
    // background face rather than on the exterior, which is also the
    // structure a vectorizer has to recover.
    let bg_paint = ink(0.95, 0.95, 0.92);
    let outer = if exterior == ExteriorModel::Opaque {
        let bg = b.add_face(bg_paint);
        b.add_polygon_ring(&rect(0.0, 0.0, c, c), bg, SceneBuilder::EXTERIOR)
            .map_err(err)?;
        bg
    } else {
        SceneBuilder::EXTERIOR
    };
    let bg_paint = if exterior == ExteriorModel::Opaque {
        bg_paint
    } else {
        Paint::TransparentExterior
    };
    let fg = ink(
        rng.range(0.02, 0.35),
        rng.range(0.05, 0.45),
        rng.range(0.10, 0.70),
    );
    let center = Pt::new(c / 2.0, c / 2.0);

    let (scene, truth, salient) = match family {
        "polygon" => {
            let sides = 3 + v % 6;
            let radius = c * rng.range(0.22, 0.42);
            let phase = rng.range(0.0, TAU);
            let f = b.add_face(fg);
            let ring = regular_polygon(center, radius, sides, phase);
            b.add_polygon_ring(&ring, f, outer).map_err(err)?;
            let area = ring_signed_area(&ring);
            let interior = 180.0 - 360.0 / sides as f64;
            (
                b.build().map_err(err)?,
                AuthoredTruth::new(
                    "regular polygon island",
                    &[
                        ("sides", sides as f64),
                        ("radius_px", radius),
                        ("phase_rad", phase),
                    ],
                ),
                vec![
                    SalientFeature::Component { area_px2: area },
                    SalientFeature::Corner {
                        at_x: ring[0].x,
                        at_y: ring[0].y,
                        angle_deg: interior,
                    },
                    SalientFeature::PaintPair {
                        separation: separation(fg, bg_paint),
                    },
                ],
            )
        }
        "annulus" => {
            let sides = 4 + v % 5;
            let outer_r = c * rng.range(0.28, 0.44);
            let inner_r = outer_r * rng.range(0.30, 0.62);
            let ring_face = b.add_face(fg);
            let hole_face = b.add_face(bg_paint);
            let outer_ring = regular_polygon(center, outer_r, sides, 0.0);
            let inner_ring = regular_polygon(center, inner_r, sides, 0.0);
            b.add_polygon_ring(&outer_ring, ring_face, outer)
                .map_err(err)?;
            b.add_polygon_ring(&inner_ring, hole_face, ring_face)
                .map_err(err)?;
            let hole_area = ring_signed_area(&inner_ring);
            (
                b.build().map_err(err)?,
                AuthoredTruth::new(
                    "annulus with a polygonal hole",
                    &[
                        ("sides", sides as f64),
                        ("outer_radius_px", outer_r),
                        ("inner_radius_px", inner_r),
                    ],
                ),
                vec![
                    SalientFeature::Hole {
                        face: hole_face as u32,
                        area_px2: hole_area,
                    },
                    SalientFeature::ThinFeature {
                        min_width_px: outer_r - inner_r,
                    },
                    SalientFeature::PaintPair {
                        separation: separation(fg, bg_paint),
                    },
                ],
            )
        }
        "nested_island" => {
            let outer_r = c * rng.range(0.32, 0.44);
            let inner_r = outer_r * 0.66;
            let core_r = inner_r * rng.range(0.35, 0.62);
            let ring_face = b.add_face(fg);
            let hole_face = b.add_face(bg_paint);
            let core = b.add_face(ink(0.85, rng.range(0.1, 0.4), 0.15));
            b.add_polygon_ring(
                &regular_polygon(center, outer_r, 5 + v % 3, 0.3),
                ring_face,
                outer,
            )
            .map_err(err)?;
            b.add_polygon_ring(
                &regular_polygon(center, inner_r, 5 + v % 3, 0.3),
                hole_face,
                ring_face,
            )
            .map_err(err)?;
            b.add_polygon_ring(&regular_polygon(center, core_r, 4, 0.0), core, hole_face)
                .map_err(err)?;
            (
                b.build().map_err(err)?,
                AuthoredTruth::new(
                    "island inside the hole of an annulus",
                    &[
                        ("outer_radius_px", outer_r),
                        ("hole_radius_px", inner_r),
                        ("core_radius_px", core_r),
                    ],
                ),
                vec![
                    SalientFeature::Hole {
                        face: hole_face as u32,
                        area_px2: ring_signed_area(&regular_polygon(
                            center,
                            inner_r,
                            5 + v % 3,
                            0.3,
                        )),
                    },
                    SalientFeature::ThinFeature {
                        min_width_px: (inner_r - core_r).min(outer_r - inner_r),
                    },
                    SalientFeature::PaintPair { separation: 0.5 },
                ],
            )
        }
        "two_islands" => {
            let r = c * rng.range(0.12, 0.20);
            let gap = c * rng.range(0.05, 0.22);
            let a = b.add_face(fg);
            let d = b.add_face(ink(0.8, 0.55, 0.05));
            let ca = Pt::new(c / 2.0 - r - gap / 2.0, c / 2.0);
            let cb = Pt::new(c / 2.0 + r + gap / 2.0, c / 2.0);
            b.add_polygon_ring(&regular_polygon(ca, r, 4 + v % 4, 0.2), a, outer)
                .map_err(err)?;
            b.add_polygon_ring(&regular_polygon(cb, r, 4 + v % 4, 0.9), d, outer)
                .map_err(err)?;
            (
                b.build().map_err(err)?,
                AuthoredTruth::new("two disjoint islands", &[("radius_px", r), ("gap_px", gap)]),
                vec![
                    SalientFeature::Component {
                        area_px2: PI * r * r,
                    },
                    SalientFeature::ThinFeature { min_width_px: gap },
                    SalientFeature::PaintPair {
                        separation: separation(fg, ink(0.8, 0.55, 0.05)),
                    },
                ],
            )
        }
        "shared_edge" => {
            let (x0, x1) = (c * rng.range(0.08, 0.20), c * rng.range(0.78, 0.92));
            let (y0, y1) = (c * rng.range(0.15, 0.32), c * rng.range(0.66, 0.86));
            let split_x = x0 + (x1 - x0) * rng.range(0.30, 0.70);
            let left = b.add_face(fg);
            let right = b.add_face(ink(0.9, 0.25, 0.2));
            // Authored with the low-level API on purpose: two rings would
            // duplicate the split line's endpoints, and the IR rightly
            // refuses duplicate vertex positions — a boundary shared by two
            // faces exists ONCE. This is the structure §6.1 is about, so
            // the corpus must contain it rather than route around it.
            let chain = || vice_ir::CurveChain::single(Segment::Line);
            let va = b.add_vertex(Pt::new(x0, y0));
            let vb = b.add_vertex(Pt::new(split_x, y0));
            let vc = b.add_vertex(Pt::new(x1, y0));
            let vd = b.add_vertex(Pt::new(x1, y1));
            let ve = b.add_vertex(Pt::new(split_x, y1));
            let vf = b.add_vertex(Pt::new(x0, y1));
            let e_ab = b.add_boundary(va, vb, chain(), left, outer).map_err(err)?;
            let e_bc = b.add_boundary(vb, vc, chain(), right, outer).map_err(err)?;
            let e_cd = b.add_boundary(vc, vd, chain(), right, outer).map_err(err)?;
            let e_de = b.add_boundary(vd, ve, chain(), right, outer).map_err(err)?;
            let e_ef = b.add_boundary(ve, vf, chain(), left, outer).map_err(err)?;
            let e_fa = b.add_boundary(vf, va, chain(), left, outer).map_err(err)?;
            let e_be = b.add_boundary(vb, ve, chain(), left, right).map_err(err)?;
            b.add_face_loop(
                left,
                &[(e_ab, true), (e_be, true), (e_ef, true), (e_fa, true)],
            )
            .map_err(err)?;
            b.add_face_loop(
                right,
                &[(e_bc, true), (e_cd, true), (e_de, true), (e_be, false)],
            )
            .map_err(err)?;
            b.add_face_loop(
                outer,
                &[
                    (e_fa, false),
                    (e_ef, false),
                    (e_de, false),
                    (e_cd, false),
                    (e_bc, false),
                    (e_ab, false),
                ],
            )
            .map_err(err)?;
            (
                b.build().map_err(err)?,
                AuthoredTruth::new(
                    "two rectangles abutting on a common line",
                    &[
                        ("split_x_px", split_x),
                        ("width_px", x1 - x0),
                        ("height_px", y1 - y0),
                    ],
                ),
                vec![
                    SalientFeature::PaintPair {
                        separation: separation(fg, ink(0.9, 0.25, 0.2)),
                    },
                    SalientFeature::Corner {
                        at_x: split_x,
                        at_y: y0,
                        angle_deg: 90.0,
                    },
                    // The narrower of the two faces is what a degradation
                    // can lose first; it also gives the group a length
                    // scale, without which identifiability is undecidable.
                    SalientFeature::Component {
                        area_px2: (split_x - x0).min(x1 - split_x) * (y1 - y0),
                    },
                    SalientFeature::ThinFeature {
                        min_width_px: (split_x - x0).min(x1 - split_x),
                    },
                ],
            )
        }
        "arc_disk" => {
            let r = c * rng.range(0.24, 0.40);
            let f = b.add_face(fg);
            let pts = regular_polygon(center, r, 4, 0.0);
            let segs = vec![
                Segment::CircularArc {
                    radius_px: r,
                    large_arc: false,
                    ccw: true,
                };
                4
            ];
            b.add_ring(&pts, &segs, f, outer).map_err(err)?;
            (
                b.build().map_err(err)?,
                AuthoredTruth::new("disk from four circular arcs", &[("radius_px", r)]),
                vec![
                    SalientFeature::Component {
                        area_px2: PI * r * r,
                    },
                    SalientFeature::PaintPair {
                        separation: separation(fg, bg_paint),
                    },
                ],
            )
        }
        "bezier_blob" => {
            let r = c * rng.range(0.24, 0.38);
            let f = b.add_face(fg);
            let pts = regular_polygon(center, r, 4, rng.range(0.0, 0.4));
            let bulge = r * rng.range(0.30, 0.60);
            let mut segs = Vec::with_capacity(4);
            for i in 0..4 {
                let a = pts[i];
                let d = pts[(i + 1) % 4];
                let mid = Pt::new((a.x + d.x) / 2.0, (a.y + d.y) / 2.0);
                let outward = Pt::new(mid.x - center.x, mid.y - center.y);
                let len = (outward.x * outward.x + outward.y * outward.y)
                    .sqrt()
                    .max(1e-9);
                let ctrl = Pt::new(
                    mid.x + outward.x / len * bulge,
                    mid.y + outward.y / len * bulge,
                );
                segs.push(if i % 2 == 0 {
                    Segment::Quad { ctrl }
                } else {
                    Segment::Cubic {
                        ctrl1: Pt::new(a.x + (ctrl.x - a.x) * 0.66, a.y + (ctrl.y - a.y) * 0.66),
                        ctrl2: Pt::new(d.x + (ctrl.x - d.x) * 0.66, d.y + (ctrl.y - d.y) * 0.66),
                    }
                });
            }
            b.add_ring(&pts, &segs, f, outer).map_err(err)?;
            (
                b.build().map_err(err)?,
                AuthoredTruth::new(
                    "blob of alternating quadratic and cubic segments",
                    &[("radius_px", r), ("bulge_px", bulge)],
                ),
                vec![
                    SalientFeature::Component {
                        area_px2: PI * r * r,
                    },
                    SalientFeature::PaintPair {
                        separation: separation(fg, bg_paint),
                    },
                ],
            )
        }
        "thin_bridge" => {
            let neck = c * rng.range(0.012, 0.045);
            let lobe = c * rng.range(0.13, 0.20);
            let f = b.add_face(fg);
            let cy = c / 2.0;
            let dx = c * 0.22;
            // A dumbbell authored as one ring: left lobe, neck, right lobe.
            let ring = vec![
                Pt::new(c / 2.0 - dx - lobe, cy - lobe),
                Pt::new(c / 2.0 - dx + lobe, cy - lobe),
                Pt::new(c / 2.0 + dx - lobe, cy - neck / 2.0),
                Pt::new(c / 2.0 + dx - lobe, cy - lobe),
                Pt::new(c / 2.0 + dx + lobe, cy - lobe),
                Pt::new(c / 2.0 + dx + lobe, cy + lobe),
                Pt::new(c / 2.0 + dx - lobe, cy + lobe),
                Pt::new(c / 2.0 + dx - lobe, cy + neck / 2.0),
                Pt::new(c / 2.0 - dx + lobe, cy + neck / 2.0),
                Pt::new(c / 2.0 - dx + lobe, cy + lobe),
                Pt::new(c / 2.0 - dx - lobe, cy + lobe),
            ];
            let mut ring = ring;
            // The point that makes the neck on the upper side.
            ring.insert(2, Pt::new(c / 2.0 - dx + lobe, cy - neck / 2.0));
            if ring_signed_area(&ring) < 0.0 {
                ring.reverse();
            }
            b.add_polygon_ring(&ring, f, outer).map_err(err)?;
            (
                b.build().map_err(err)?,
                AuthoredTruth::new(
                    "dumbbell: two lobes joined by a thin neck",
                    &[("neck_px", neck), ("lobe_px", lobe)],
                ),
                vec![
                    SalientFeature::ThinFeature { min_width_px: neck },
                    SalientFeature::Component {
                        area_px2: 4.0 * lobe * lobe,
                    },
                    SalientFeature::PaintPair {
                        separation: separation(fg, bg_paint),
                    },
                ],
            )
        }
        "dot_cluster" => {
            let n = 3 + v % 4;
            let r = c * rng.range(0.012, 0.035);
            let spread = c * 0.30;
            let mut smallest = f64::INFINITY;
            for i in 0..n {
                let t = TAU * (i as f64) / (n as f64);
                let cc = Pt::new(center.x + spread * t.cos(), center.y + spread * t.sin());
                let rr = r * (0.7 + 0.3 * (i as f64) / (n as f64));
                smallest = smallest.min(PI * rr * rr);
                let f = b.add_face(ink(0.1 + 0.2 * i as f64 / n as f64, 0.2, 0.6));
                b.add_polygon_ring(&regular_polygon(cc, rr, 6, 0.0), f, outer)
                    .map_err(err)?;
            }
            (
                b.build().map_err(err)?,
                AuthoredTruth::new(
                    "cluster of small components",
                    &[("count", n as f64), ("radius_px", r)],
                ),
                vec![
                    SalientFeature::Component { area_px2: smallest },
                    SalientFeature::PaintPair {
                        separation: separation(fg, bg_paint),
                    },
                ],
            )
        }
        "l_shape" => {
            let arm = c * rng.range(0.10, 0.18);
            let span = c * rng.range(0.42, 0.60);
            let (x0, y0) = (c * 0.2, c * 0.2);
            let f = b.add_face(fg);
            let ring = vec![
                Pt::new(x0, y0),
                Pt::new(x0 + span, y0),
                Pt::new(x0 + span, y0 + arm),
                Pt::new(x0 + arm, y0 + arm),
                Pt::new(x0 + arm, y0 + span),
                Pt::new(x0, y0 + span),
            ];
            let ring = if ring_signed_area(&ring) < 0.0 {
                super::build::reversed(&ring)
            } else {
                ring
            };
            b.add_polygon_ring(&ring, f, outer).map_err(err)?;
            (
                b.build().map_err(err)?,
                AuthoredTruth::new("L shape", &[("arm_px", arm), ("span_px", span)]),
                vec![
                    SalientFeature::ThinFeature { min_width_px: arm },
                    SalientFeature::Corner {
                        at_x: x0 + arm,
                        at_y: y0 + arm,
                        angle_deg: 270.0,
                    },
                    SalientFeature::PaintPair {
                        separation: separation(fg, bg_paint),
                    },
                ],
            )
        }
        "triple_junction" => {
            // Three wedges of one disk meeting at the centre: three faces
            // and one point where all three and the exterior meet.
            let r = c * rng.range(0.28, 0.40);
            let phase = rng.range(0.0, TAU / 3.0);
            let paints = [fg, ink(0.9, 0.3, 0.1), ink(0.15, 0.6, 0.25)];
            let wedges: Vec<usize> = paints.iter().map(|p| b.add_face(*p)).collect();
            // Shared spokes: the three wedges meet at ONE graph vertex and
            // each spoke is one boundary with a wedge on either side. Three
            // separate rings would duplicate the centre and the rim points,
            // which the IR refuses - and rightly, since a triple junction
            // whose spokes are stored twice is not a shared planar graph.
            let at = |ang: f64| Pt::new(center.x + r * ang.cos(), center.y + r * ang.sin());
            let chain = || vice_ir::CurveChain::single(Segment::Line);
            let vc = b.add_vertex(center);
            let ang = |k: usize| phase + TAU * (k as f64) / 3.0;
            let rim: Vec<usize> = (0..3).map(|k| b.add_vertex(at(ang(k)))).collect();
            let mid: Vec<usize> = (0..3)
                .map(|k| b.add_vertex(at((ang(k) + ang(k + 1)) / 2.0)))
                .collect();
            // spoke[k]: centre -> rim[k]; wedge k on the left, wedge k-1 on
            // the right.
            let mut spoke = Vec::with_capacity(3);
            for k in 0..3 {
                spoke.push(
                    b.add_boundary(vc, rim[k], chain(), wedges[k], wedges[(k + 2) % 3])
                        .map_err(err)?,
                );
            }
            let mut rim_edges = Vec::with_capacity(6);
            for k in 0..3 {
                rim_edges.push(
                    b.add_boundary(rim[k], mid[k], chain(), wedges[k], outer)
                        .map_err(err)?,
                );
                rim_edges.push(
                    b.add_boundary(mid[k], rim[(k + 1) % 3], chain(), wedges[k], outer)
                        .map_err(err)?,
                );
            }
            for k in 0..3 {
                b.add_face_loop(
                    wedges[k],
                    &[
                        (spoke[k], true),
                        (rim_edges[2 * k], true),
                        (rim_edges[2 * k + 1], true),
                        (spoke[(k + 1) % 3], false),
                    ],
                )
                .map_err(err)?;
            }
            let mut outer_loop: Vec<(usize, bool)> =
                rim_edges.iter().rev().map(|e| (*e, false)).collect();
            outer_loop.rotate_left(0);
            b.add_face_loop(outer, &outer_loop).map_err(err)?;
            (
                b.build().map_err(err)?,
                AuthoredTruth::new(
                    "three wedges meeting at one point",
                    &[("radius_px", r), ("phase_rad", phase)],
                ),
                vec![
                    SalientFeature::Corner {
                        at_x: center.x,
                        at_y: center.y,
                        angle_deg: 120.0,
                    },
                    SalientFeature::PaintPair {
                        separation: separation(paints[0], paints[2]),
                    },
                    // Each wedge is a visible component of the partition;
                    // once a wedge is smaller than a pixel the junction is
                    // no longer observable, so this is the length scale.
                    SalientFeature::Component {
                        area_px2: PI * r * r / 3.0,
                    },
                ],
            )
        }
        "star" => {
            let points = 5 + v % 4;
            let r_out = c * rng.range(0.30, 0.44);
            let r_in = r_out * rng.range(0.32, 0.55);
            let f = b.add_face(fg);
            let ring = star(center, r_out, r_in, points, rng.range(0.0, 0.5));
            b.add_polygon_ring(&ring, f, outer).map_err(err)?;
            (
                b.build().map_err(err)?,
                AuthoredTruth::new(
                    "star polygon",
                    &[
                        ("points", points as f64),
                        ("outer_radius_px", r_out),
                        ("inner_radius_px", r_in),
                    ],
                ),
                vec![
                    SalientFeature::Corner {
                        at_x: ring[0].x,
                        at_y: ring[0].y,
                        angle_deg: 360.0 / points as f64,
                    },
                    SalientFeature::ThinFeature {
                        min_width_px: 2.0 * r_in * (PI / points as f64).sin(),
                    },
                    SalientFeature::PaintPair {
                        separation: separation(fg, bg_paint),
                    },
                ],
            )
        }
        other => return Err(format!("unknown shape family {other:?}")),
    };

    group_of(id, family, scene, truth, salient)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn every_recipe_certifies_and_is_reproducible() {
        let a = procedural_groups(4);
        let b = procedural_groups(4);
        assert_eq!(a.len(), SHAPE_FAMILIES.len() * 4);
        for (x, y) in a.iter().zip(&b) {
            assert_eq!(x.id, y.id);
            assert_eq!(
                x.scenes[0].partition_truth(),
                y.scenes[0].partition_truth(),
                "the grammar must be deterministic across processes and runs"
            );
        }
    }

    #[test]
    fn variants_of_one_family_differ_structurally_not_only_in_phase() {
        // The point of §27.4: variants must not be phase shifts of one
        // logo. Structural signature = (faces, holes, components).
        for family in SHAPE_FAMILIES {
            let mut sigs = BTreeSet::new();
            let mut areas = BTreeSet::new();
            for v in 0..6 {
                let id = format!("proc/{family}/{v:03}");
                let g = build_variant(family, v, &id).expect("recipe certifies");
                let t = g.scenes[0].partition_truth();
                sigs.insert((t.visible_faces, t.holes, t.components));
                areas.insert(t.total_ink_px2.to_bits());
            }
            // Not every family can vary its topology (a polygon island is
            // always one face), so the requirement is stated honestly: the
            // variants must differ in SOMETHING measurable, and at least
            // half the families must differ topologically.
            assert!(
                areas.len() >= 5,
                "family {family}: variants are not measurably different"
            );
        }
        let topologically_varying = SHAPE_FAMILIES
            .iter()
            .filter(|family| {
                let mut sigs = BTreeSet::new();
                for v in 0..6 {
                    let id = format!("proc/{family}/{v:03}");
                    let g = build_variant(family, v, &id).unwrap();
                    let t = g.scenes[0].partition_truth();
                    sigs.insert((t.visible_faces, t.holes, t.components));
                }
                sigs.len() > 1
            })
            .count();
        assert!(
            topologically_varying >= 2,
            "at least some families must vary topology across variants, got {topologically_varying}"
        );
    }

    #[test]
    fn the_corpus_covers_the_structural_axes_it_claims_to_cover() {
        let groups = procedural_groups(4);
        let mut with_holes = 0;
        let mut multi_component = 0;
        let mut multi_paint = 0;
        let mut opaque_exterior = 0;
        let mut curved = 0;
        for g in &groups {
            let t = g.scenes[0].partition_truth();
            if t.holes > 0 {
                with_holes += 1;
            }
            if t.components > 1 {
                multi_component += 1;
            }
            if t.palette.len() > 1 {
                multi_paint += 1;
            }
            if t.exterior_model == "opaque" {
                opaque_exterior += 1;
                assert!(
                    t.exterior_visible_px2.abs() < 1e-9,
                    "{}: declares an opaque exterior but leaves {} px2 of the true                      exterior visible - the label and the pixels must agree",
                    g.id,
                    t.exterior_visible_px2
                );
            }
            let has_curve = g.scenes[0]
                .scene()
                .graph()
                .boundaries
                .iter()
                .any(|b| b.curve.segments.iter().any(|s| *s != Segment::Line));
            if has_curve {
                curved += 1;
            }
        }
        assert!(with_holes >= 4, "holes: {with_holes}");
        assert!(multi_component >= 4, "components: {multi_component}");
        assert!(multi_paint >= 4, "palettes: {multi_paint}");
        assert!(opaque_exterior >= 4, "opaque exteriors: {opaque_exterior}");
        assert!(curved >= 4, "curved boundaries: {curved}");
    }

    #[test]
    fn every_group_declares_at_least_one_salient_feature() {
        for g in procedural_groups(3) {
            assert!(
                !g.scenes[0].salient_features().is_empty(),
                "{}: a fixture with no salient feature cannot be scored",
                g.id
            );
            assert!(
                g.scenes[0].min_salient_scale_px().is_finite(),
                "{}: at least one feature must have a length scale",
                g.id
            );
        }
    }
}
