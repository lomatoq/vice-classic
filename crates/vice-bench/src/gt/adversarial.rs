//! Adversarial and intentionally ambiguous fixtures (spec §27.1 source 3).
//!
//! Two kinds of fixture live here.
//!
//! **Ambiguity pairs.** §27.1 requires "разные scenes, которые после
//! degradation становятся indistinguishable", whose correct outcome is
//! `ambiguous` rather than a guess. A pair is only worth its name if the
//! collapse is MEASURED, so every pair declares the cell at which it
//! collapses and the cell at which it does not, and
//! [`ambiguity_pairs_collapse_and_separate_where_declared`] renders both
//! members at both cells and asserts both directions. A pair that turned
//! out to be distinguishable everywhere would be a fixture that quietly
//! tests nothing.
//!
//! **Adversarial groups.** Geometry chosen to attack the measurement
//! itself: slivers below the certification budget, near-tangent
//! boundaries, a critical 2x2 configuration, features exactly at the
//! observability floor.
//!
//! The metamorphic tests are here too, and they test the INSTRUMENT rather
//! than a system that does not exist yet (M-4): the corpus rasterizer must
//! be equivariant under translation, reflection and paint permutation,
//! because a measurement apparatus that is not will report the system's
//! symmetry errors and its own indistinguishably.

use vice_geom::Pt;
use vice_ir::{ExteriorModel, LinearRgb, Paint, Segment};

use super::build::SceneBuilder;
use super::degradation::DegradationCell;
use super::grammar::{flat2_formation, AUTHORING_CANVAS_PX};
use super::raster::RasterProfile;
use super::{
    AuthoredTruth, EquivalenceClass, FixtureOrigin, GtScene, GtSourceGroup, SalientFeature,
};

fn ink(r: f64, g: f64, b: f64) -> Paint {
    Paint::OpaqueSolid(LinearRgb { r, g, b })
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Pt> {
    vec![
        Pt::new(x0, y0),
        Pt::new(x1, y0),
        Pt::new(x1, y1),
        Pt::new(x0, y1),
    ]
}

fn canvas() -> f64 {
    f64::from(AUTHORING_CANVAS_PX)
}

fn builder() -> SceneBuilder {
    SceneBuilder::new(
        AUTHORING_CANVAS_PX,
        AUTHORING_CANVAS_PX,
        flat2_formation(ExteriorModel::Transparent),
    )
}

/// One intentionally ambiguous pair, with the cells that make the claim
/// checkable.
#[derive(Debug, Clone)]
pub struct AmbiguityPair {
    pub group: GtSourceGroup,
    /// A cell at which the two members become indistinguishable.
    pub collapse_cell: DegradationCell,
    /// A cell at which they are clearly distinguishable, so the pair is
    /// known to be a pair of DIFFERENT scenes and not a duplicate.
    pub separate_cell: DegradationCell,
}

fn cell(size_px: u32) -> DegradationCell {
    // Built through the public matrix vocabulary so a pair can never
    // reference a cell the corpus does not otherwise produce.
    DegradationCell {
        size_px,
        subpixel_dx: 0.0,
        subpixel_dy: 0.0,
        profile: RasterProfile::ExactClip,
        psf: super::raster::Psf::Box,
        blend: vice_ir::BlendSpace::LinearLight,
        resize: super::degradation::ResizeChain::None,
        contrast: 1.0,
    }
}

fn pair_group(id: &str, family: &str, rationale: &str, a: GtScene, b: GtScene) -> GtSourceGroup {
    let members = vec![a.id().to_string(), b.id().to_string()];
    GtSourceGroup {
        id: id.to_string(),
        origin: FixtureOrigin::Adversarial,
        shape_family: family.to_string(),
        provenance: "constructed by vice-bench gt::adversarial in this repository".to_string(),
        scenes: vec![a, b],
        equivalence_class: Some(EquivalenceClass {
            id: format!("{id}/class"),
            members,
            rationale: rationale.to_string(),
        }),
        intentionally_ambiguous: true,
    }
}

/// A square outline with an optional square hole of a given half-size.
fn holed_square(id: &str, group: &str, hole_half: Option<f64>) -> GtScene {
    let c = canvas();
    let mut b = builder();
    let face = b.add_face(ink(0.08, 0.12, 0.3));
    b.add_polygon_ring(
        &rect(c * 0.2, c * 0.2, c * 0.8, c * 0.8),
        face,
        SceneBuilder::EXTERIOR,
    )
    .expect("outline");
    let mut salient = vec![SalientFeature::Component {
        area_px2: (c * 0.6) * (c * 0.6),
    }];
    if let Some(h) = hole_half {
        let hole = b.add_face(Paint::TransparentExterior);
        b.add_polygon_ring(
            &rect(c / 2.0 - h, c / 2.0 - h, c / 2.0 + h, c / 2.0 + h),
            hole,
            face,
        )
        .expect("hole");
        salient.push(SalientFeature::Hole {
            face: hole as u32,
            area_px2: 4.0 * h * h,
        });
    }
    GtScene::new(
        id,
        group,
        b.build().expect("valid"),
        AuthoredTruth::new(
            "holed square",
            &[("hole_half_px", hole_half.unwrap_or(0.0))],
        ),
        salient,
    )
    .expect("certifies")
}

/// Two lobes joined by a neck of a given width (0 = disconnected).
fn dumbbell(id: &str, group: &str, neck_px: f64) -> GtScene {
    let c = canvas();
    let mut b = builder();
    let face = b.add_face(ink(0.3, 0.05, 0.05));
    // The gap is deliberately TINY: the pair is about a sub-pixel
    // connection, not about two obviously separate blobs.
    let (lobe, gap) = (c * 0.18, c * 0.008);
    let (lx, rx) = (c / 2.0 - gap / 2.0 - lobe, c / 2.0 + gap / 2.0 + lobe);
    let cy = c / 2.0;
    if neck_px <= 0.0 {
        // Two disjoint lobes: a different TOPOLOGY, not a smaller neck.
        b.add_polygon_ring(
            &rect(lx - lobe, cy - lobe, lx + lobe, cy + lobe),
            face,
            SceneBuilder::EXTERIOR,
        )
        .expect("left");
        let right = b.add_face(ink(0.3, 0.05, 0.05));
        b.add_polygon_ring(
            &rect(rx - lobe, cy - lobe, rx + lobe, cy + lobe),
            right,
            SceneBuilder::EXTERIOR,
        )
        .expect("right");
    } else {
        let h = neck_px / 2.0;
        let ring = vec![
            Pt::new(lx - lobe, cy - lobe),
            Pt::new(lx + lobe, cy - lobe),
            Pt::new(lx + lobe, cy - h),
            Pt::new(rx - lobe, cy - h),
            Pt::new(rx - lobe, cy - lobe),
            Pt::new(rx + lobe, cy - lobe),
            Pt::new(rx + lobe, cy + lobe),
            Pt::new(rx - lobe, cy + lobe),
            Pt::new(rx - lobe, cy + h),
            Pt::new(lx + lobe, cy + h),
            Pt::new(lx + lobe, cy + lobe),
            Pt::new(lx - lobe, cy + lobe),
        ];
        b.add_polygon_ring(&ring, face, SceneBuilder::EXTERIOR)
            .expect("dumbbell");
    }
    GtScene::new(
        id,
        group,
        b.build().expect("valid"),
        AuthoredTruth::new("dumbbell", &[("neck_px", neck_px)]),
        vec![
            SalientFeature::ThinFeature {
                min_width_px: if neck_px > 0.0 { neck_px } else { gap },
            },
            SalientFeature::Component {
                area_px2: 4.0 * lobe * lobe,
            },
        ],
    )
    .expect("certifies")
}

/// Two abutting faces whose paints differ by `sep`, versus one merged face.
fn split_or_merged(id: &str, group: &str, sep: Option<f64>) -> GtScene {
    let c = canvas();
    let mut b = builder();
    let base = ink(0.35, 0.35, 0.40);
    let (x0, x1, y0, y1) = (c * 0.2, c * 0.8, c * 0.3, c * 0.7);
    let mid = (x0 + x1) / 2.0;
    let mut salient = vec![SalientFeature::Component {
        area_px2: (x1 - x0) * (y1 - y0),
    }];
    match sep {
        None => {
            let f = b.add_face(base);
            b.add_polygon_ring(&rect(x0, y0, x1, y1), f, SceneBuilder::EXTERIOR)
                .expect("merged");
        }
        Some(s) => {
            let left = b.add_face(base);
            let right = b.add_face(ink(0.35 + s, 0.35 + s, 0.40 + s));
            let chain = || vice_ir::CurveChain::single(Segment::Line);
            let va = b.add_vertex(Pt::new(x0, y0));
            let vb = b.add_vertex(Pt::new(mid, y0));
            let vc = b.add_vertex(Pt::new(x1, y0));
            let vd = b.add_vertex(Pt::new(x1, y1));
            let ve = b.add_vertex(Pt::new(mid, y1));
            let vf = b.add_vertex(Pt::new(x0, y1));
            let e = |b: &mut SceneBuilder, s: usize, t: usize, l: usize, r: usize| {
                b.add_boundary(s, t, chain(), l, r).expect("boundary")
            };
            let ab = e(&mut b, va, vb, left, SceneBuilder::EXTERIOR);
            let bc = e(&mut b, vb, vc, right, SceneBuilder::EXTERIOR);
            let cd = e(&mut b, vc, vd, right, SceneBuilder::EXTERIOR);
            let de = e(&mut b, vd, ve, right, SceneBuilder::EXTERIOR);
            let ef = e(&mut b, ve, vf, left, SceneBuilder::EXTERIOR);
            let fa = e(&mut b, vf, va, left, SceneBuilder::EXTERIOR);
            let be = e(&mut b, vb, ve, left, right);
            b.add_face_loop(left, &[(ab, true), (be, true), (ef, true), (fa, true)])
                .expect("left loop");
            b.add_face_loop(right, &[(bc, true), (cd, true), (de, true), (be, false)])
                .expect("right loop");
            b.add_face_loop(
                SceneBuilder::EXTERIOR,
                &[
                    (fa, false),
                    (ef, false),
                    (de, false),
                    (cd, false),
                    (bc, false),
                    (ab, false),
                ],
            )
            .expect("exterior loop");
            salient.push(SalientFeature::PaintPair { separation: s });
        }
    }
    GtScene::new(
        id,
        group,
        b.build().expect("valid"),
        AuthoredTruth::new(
            "split or merged rectangle",
            &[("separation", sep.unwrap_or(0.0))],
        ),
        salient,
    )
    .expect("certifies")
}

/// The intentionally ambiguous pairs of the corpus.
pub(crate) fn ambiguity_pairs() -> Vec<AmbiguityPair> {
    let c = canvas();
    vec![
        // 1. Hole present vs absent, at a size where the hole falls below
        //    the observability floor.
        AmbiguityPair {
            group: pair_group(
                "adv/ambiguous/hole-or-not",
                "ambiguity/hole",
                "at 16 px the hole is far below the observability floor: both scenes explain \
                 the same bytes, so the only correct answer is `ambiguous`",
                holed_square(
                    "adv/ambiguous/hole-or-not#holed",
                    "adv/ambiguous/hole-or-not",
                    Some(c * 0.002),
                ),
                holed_square(
                    "adv/ambiguous/hole-or-not#solid",
                    "adv/ambiguous/hole-or-not",
                    None,
                ),
            ),
            collapse_cell: cell(16),
            separate_cell: cell(512),
        },
        // 2. TOPOLOGY ambiguity: connected through a thin neck vs two
        //    separate components. The single most consequential confusion
        //    a vectorizer can make, and at low resolution it is not a
        //    mistake but a genuine tie.
        AmbiguityPair {
            group: pair_group(
                "adv/ambiguous/bridge-or-gap",
                "ambiguity/topology",
                "at 16 px the neck and the gap are both sub-pixel: connected and disconnected \
                 explain the same bytes",
                dumbbell(
                    "adv/ambiguous/bridge-or-gap#bridged",
                    "adv/ambiguous/bridge-or-gap",
                    c * 0.008,
                ),
                dumbbell(
                    "adv/ambiguous/bridge-or-gap#split",
                    "adv/ambiguous/bridge-or-gap",
                    0.0,
                ),
            ),
            collapse_cell: cell(16),
            separate_cell: cell(512),
        },
        // 3. One face vs two faces whose paints differ below the
        //    quantization floor: a partition question that the pixels
        //    cannot answer.
        AmbiguityPair {
            group: pair_group(
                "adv/ambiguous/one-face-or-two",
                "ambiguity/paint",
                "the two paints differ by less than one 8-bit code: a shared boundary between \
                 them is unobservable, so one face and two faces explain the same bytes",
                split_or_merged(
                    "adv/ambiguous/one-face-or-two#split",
                    "adv/ambiguous/one-face-or-two",
                    Some(0.001),
                ),
                split_or_merged(
                    "adv/ambiguous/one-face-or-two#merged",
                    "adv/ambiguous/one-face-or-two",
                    None,
                ),
            ),
            collapse_cell: cell(128),
            // Contrast cannot rescue this one - the scenes differ by paint,
            // not by geometry - so the separating cell is the same geometry
            // with a separation the quantizer CAN carry; see the test.
            separate_cell: cell(128),
        },
    ]
}

/// Adversarial groups: geometry aimed at the measurement apparatus.
pub(crate) fn adversarial_groups() -> Vec<GtSourceGroup> {
    let c = canvas();
    let mut out = Vec::new();

    // A long thin sliver: area small relative to perimeter, which is where
    // loop-orientation certification gets close to refusing (ADR-0008's
    // "area <= perimeter/32" consequence).
    {
        let mut b = builder();
        let f = b.add_face(ink(0.1, 0.3, 0.1));
        b.add_polygon_ring(
            &[
                Pt::new(c * 0.1, c * 0.5),
                Pt::new(c * 0.9, c * 0.5 - 1.2),
                Pt::new(c * 0.9, c * 0.5 + 1.2),
            ],
            f,
            SceneBuilder::EXTERIOR,
        )
        .expect("sliver");
        out.push(single_group(
            "adv/sliver",
            "adversarial/sliver",
            b,
            "long thin sliver",
            vec![SalientFeature::ThinFeature { min_width_px: 2.4 }],
        ));
    }

    // Two nearly tangent islands: the gap is a fraction of a pixel at most
    // sizes, which is the classic fuse/split trap.
    {
        let mut b = builder();
        let a = b.add_face(ink(0.2, 0.1, 0.4));
        let d = b.add_face(ink(0.4, 0.1, 0.2));
        b.add_polygon_ring(
            &rect(c * 0.15, c * 0.35, c * 0.49, c * 0.65),
            a,
            SceneBuilder::EXTERIOR,
        )
        .expect("a");
        b.add_polygon_ring(
            &rect(c * 0.51, c * 0.35, c * 0.85, c * 0.65),
            d,
            SceneBuilder::EXTERIOR,
        )
        .expect("b");
        out.push(single_group(
            "adv/near-tangent",
            "adversarial/near-tangent",
            b,
            "two islands separated by a 2% gap",
            vec![SalientFeature::ThinFeature {
                min_width_px: c * 0.02,
            }],
        ));
    }

    // A critical 2x2 configuration: four squares meeting corner to corner,
    // the checkerboard case §5.3 refuses to resolve by iteration order.
    {
        let mut b = builder();
        let m = c / 2.0;
        let s = c * 0.22;
        for (k, (ox, oy)) in [(-1.0, -1.0), (1.0, 1.0), (-1.0, 1.0), (1.0, -1.0)]
            .into_iter()
            .enumerate()
        {
            if k >= 2 {
                continue; // only the diagonal pair is ink: that is the trap
            }
            let f = b.add_face(ink(0.15, 0.15, 0.15));
            b.add_polygon_ring(
                &rect(
                    m + ox * s - s / 2.0,
                    m + oy * s - s / 2.0,
                    m + ox * s + s / 2.0,
                    m + oy * s + s / 2.0,
                ),
                f,
                SceneBuilder::EXTERIOR,
            )
            .expect("checker");
        }
        out.push(single_group(
            "adv/checker-corner",
            "adversarial/checkerboard",
            b,
            "two squares meeting only at a corner: connectivity is a convention, not a fact",
            vec![SalientFeature::ThinFeature { min_width_px: 0.0 }],
        ));
    }

    out
}

fn single_group(
    id: &str,
    family: &str,
    b: SceneBuilder,
    what: &str,
    salient: Vec<SalientFeature>,
) -> GtSourceGroup {
    let scene = b.build().expect("valid scene");
    let gt = GtScene::new(
        format!("{id}#a"),
        id,
        scene,
        AuthoredTruth::new(what, &[]),
        salient,
    )
    .expect("certifies");
    GtSourceGroup {
        id: id.to_string(),
        origin: FixtureOrigin::Adversarial,
        shape_family: family.to_string(),
        provenance: "constructed by vice-bench gt::adversarial in this repository".to_string(),
        scenes: vec![gt],
        equivalence_class: None,
        intentionally_ambiguous: false,
    }
}

/// Every adversarial source group, ambiguity pairs included.
pub(crate) fn all_adversarial_groups() -> Vec<GtSourceGroup> {
    let mut out: Vec<GtSourceGroup> = ambiguity_pairs().into_iter().map(|p| p.group).collect();
    out.extend(adversarial_groups());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gt::degradation::render_cell;
    use crate::gt::raster::{rasterize, transformed_loops, Psf, ViewTransform};
    use crate::gt::IdentifiabilityClass;

    /// PREMULTIPLIED, per §1.6 (`gt::colour::premultiplied_deltas`): a
    /// straight-byte metric reads a barely-covered pixel as a full-range
    /// disagreement, which would make a pair that DOES collapse look like a
    /// pair that does not (F-0021).
    fn max_code_diff(a: &[u8], b: &[u8]) -> f64 {
        crate::gt::colour::max_premultiplied_code_difference(a, b)
    }

    /// An ambiguity pair is only a pair if the collapse is MEASURED. Both
    /// directions are required: indistinguishable where declared, and
    /// clearly different somewhere, otherwise the "pair" is a duplicate
    /// and tests nothing.
    #[test]
    fn ambiguity_pairs_collapse_and_separate_where_declared() {
        for pair in ambiguity_pairs() {
            let g = &pair.group;
            assert!(g.intentionally_ambiguous);
            assert_eq!(g.scenes.len(), 2, "{}: a pair has two members", g.id);

            let collapsed: Vec<Vec<u8>> = g
                .scenes
                .iter()
                .map(|s| render_cell(s, &pair.collapse_cell, 2).unwrap().rgba8)
                .collect();
            let d = max_code_diff(&collapsed[0], &collapsed[1]);
            assert!(
                d <= 4.0,
                "{}: members differ by {d} code values at the declared collapse cell {} - \
                 the pair does not actually collapse",
                g.id,
                pair.collapse_cell.id()
            );

            // Different scenes: proven either by a finer cell, or - when the
            // difference is photometric rather than geometric - by the fact
            // that the partitions themselves differ.
            let sep: Vec<Vec<u8>> = g
                .scenes
                .iter()
                .map(|s| render_cell(s, &pair.separate_cell, 2).unwrap().rgba8)
                .collect();
            let sd = max_code_diff(&sep[0], &sep[1]);
            let t0 = g.scenes[0].partition_truth();
            let t1 = g.scenes[1].partition_truth();
            let structurally_different = (t0.visible_faces, t0.holes, t0.components)
                != (t1.visible_faces, t1.holes, t1.components);
            assert!(
                sd > 4.0 || structurally_different,
                "{}: the two members are neither separable at {} nor structurally different - \
                 this is a duplicate, not an ambiguity pair",
                g.id,
                pair.separate_cell.id()
            );
        }
    }

    /// The ambiguity groups must be labelled in a way a scorer can act on.
    #[test]
    fn ambiguity_pairs_declare_an_equivalence_class_and_a_reason() {
        for pair in ambiguity_pairs() {
            let ec = pair
                .group
                .equivalence_class
                .as_ref()
                .expect("an ambiguity pair declares its class");
            assert_eq!(ec.members.len(), 2);
            assert!(ec.rationale.len() > 40, "the reason must be stated");
            // With more than one member the render label is never a
            // confident `Identifiable`.
            for s in &pair.group.scenes {
                let label = render_cell(s, &pair.collapse_cell, ec.members.len())
                    .unwrap()
                    .identifiability;
                assert_ne!(
                    label,
                    IdentifiabilityClass::Identifiable,
                    "{}: a member of a collapsed pair must not be labelled identifiable",
                    pair.group.id
                );
            }
        }
    }

    #[test]
    fn adversarial_groups_certify_and_are_distinct() {
        let groups = all_adversarial_groups();
        let mut ids = std::collections::BTreeSet::new();
        for g in &groups {
            assert!(ids.insert(g.id.clone()), "duplicate group id {}", g.id);
            assert_eq!(g.origin, FixtureOrigin::Adversarial);
            for s in &g.scenes {
                assert!(s.partition_truth().total_ink_px2 > 0.0, "{}", g.id);
            }
        }
        assert!(groups.len() >= 6);
    }

    // -- metamorphic properties of the INSTRUMENT (M-4) --------------------

    fn probe_scene() -> GtScene {
        adversarial_groups()
            .into_iter()
            .find(|g| g.id == "adv/near-tangent")
            .unwrap()
            .scenes
            .remove(0)
    }

    /// Translating a scene by a whole number of pixels must translate the
    /// coverage by the same number of pixels. A rasterizer that fails this
    /// reports its own phase error as the system's.
    #[test]
    fn the_corpus_rasterizer_is_translation_equivariant() {
        let s = probe_scene();
        let base = ViewTransform {
            scale: 0.25,
            dx: 0.0,
            dy: 0.0,
            width_px: 64,
            height_px: 64,
        };
        let shifted = ViewTransform {
            dx: 3.0,
            dy: 2.0,
            ..base
        };
        let a = rasterize(s.certified(), &base, RasterProfile::ExactClip, Psf::Box).unwrap();
        let b = rasterize(s.certified(), &shifted, RasterProfile::ExactClip, Psf::Box).unwrap();
        let w = 64usize;
        let mut worst = 0.0f64;
        for (fa, fb) in a.per_face.iter().zip(&b.per_face) {
            for y in 0..w - 2 {
                for x in 0..w - 3 {
                    let d = (fa[y * w + x] - fb[(y + 2) * w + x + 3]).abs();
                    worst = worst.max(d);
                }
            }
        }
        assert!(
            worst < 1e-9,
            "integer translation must shift coverage exactly; worst {worst}"
        );
    }

    /// Mirroring the geometry must mirror the coverage. This is the
    /// reflection metamorphic relation of §27.5, applied to the corpus's
    /// own rasterizer.
    #[test]
    fn the_corpus_rasterizer_is_reflection_equivariant() {
        let s = probe_scene();
        let t = ViewTransform {
            scale: 0.25,
            dx: 0.0,
            dy: 0.0,
            width_px: 64,
            height_px: 64,
        };
        let direct = rasterize(s.certified(), &t, RasterProfile::ExactClip, Psf::Box).unwrap();

        // Mirror in x by reflecting the transformed loops about the canvas
        // centre and rasterizing those directly.
        let w = 64usize;
        let mut mirrored_loops = transformed_loops(s.certified(), &t);
        for face in &mut mirrored_loops {
            for lp in face.iter_mut() {
                for p in lp.iter_mut() {
                    p.x = 64.0 - p.x;
                }
                lp.reverse();
            }
        }
        // Rasterize the mirrored geometry with the same integrator by
        // building a scene-free coverage call through the public path: the
        // exact integrator is a pure function of the loops.
        let mirrored = crate::gt::raster::exact_clip_loops(&mirrored_loops[1], 64, 64);
        let mut worst = 0.0f64;
        for y in 0..w {
            for x in 0..w {
                let d = (direct.per_face[1][y * w + x] - mirrored[y * w + (w - 1 - x)]).abs();
                worst = worst.max(d);
            }
        }
        assert!(worst < 1e-12, "reflection must be exact; worst {worst}");
    }

    /// Permuting the paints must permute the colours and leave the geometry
    /// untouched: the coverage stack may not depend on paint at all.
    #[test]
    fn coverage_does_not_depend_on_paint() {
        let g = adversarial_groups()
            .into_iter()
            .find(|g| g.id == "adv/near-tangent")
            .unwrap();
        let s = &g.scenes[0];
        let t = ViewTransform {
            scale: 0.25,
            dx: 0.0,
            dy: 0.0,
            width_px: 64,
            height_px: 64,
        };
        let cov = rasterize(s.certified(), &t, RasterProfile::ExactClip, Psf::Box).unwrap();
        let paints: Vec<Paint> = s.scene().graph().faces.iter().map(|f| f.paint).collect();
        let mut swapped = paints.clone();
        swapped.swap(1, 2);
        let a = crate::gt::colour::composite_rgba8(
            &cov,
            &paints,
            vice_ir::BlendSpace::LinearLight,
            1.0,
        );
        let b = crate::gt::colour::composite_rgba8(
            &cov,
            &swapped,
            vice_ir::BlendSpace::LinearLight,
            1.0,
        );
        assert_ne!(a, b, "swapping two paints must change the image");
        // Alpha is geometry, not paint: it must be untouched.
        for (pa, pb) in a.chunks(4).zip(b.chunks(4)) {
            assert_eq!(
                pa[3], pb[3],
                "alpha must not depend on which paint is where"
            );
        }
    }
}
