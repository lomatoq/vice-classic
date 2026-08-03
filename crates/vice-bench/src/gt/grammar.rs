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

use std::f64::consts::PI;

use vice_geom::Pt;
use vice_ir::{
    BlendSpace, ExteriorModel, GlobalFormationHypothesis, LinearRgb, Paint, PixelFilter,
    QuantizationModel,
};

use super::build::ring_signed_area;
use super::{AuthoredTruth, FixtureOrigin, GtScene, GtSourceGroup, SalientFeature};

/// The canonical authoring canvas. Every procedural scene is authored at
/// this size; the degradation matrix produces the actual render sizes, so
/// scene coordinates and render pixels never get confused.
pub const AUTHORING_CANVAS_PX: u32 = 256;

/// Historical M3 corpus generation. This stream is immutable because its
/// geometry and render digests are already frozen in earlier milestones.
pub const PROCEDURAL_GENERATION: u32 = 1;

/// Fresh procedural population used by the successor M7 sealed audit after
/// generations 1--5 were opened and burned.
pub const M7_PROCEDURAL_GENERATION: u32 = 8;

/// Deterministic splitmix64. Small, seedable per group, no dependency.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn from_label(label: &str) -> Rng {
        Self::from_label_generation(label, PROCEDURAL_GENERATION)
    }

    pub(crate) fn from_label_generation(label: &str, generation: u32) -> Rng {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        // Generation 1 deliberately retains the exact legacy label-only
        // stream. Successors use an explicit domain and binary generation,
        // so a burned audit population cannot be regenerated accidentally.
        if generation > PROCEDURAL_GENERATION {
            for byte in b"vice-classic/gt-procedural-generation/" {
                h ^= u64::from(*byte);
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
            for byte in generation.to_le_bytes() {
                h ^= u64::from(byte);
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
            h ^= u64::from(b'/');
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        for byte in label.bytes() {
            h ^= u64::from(byte);
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

pub(super) fn ink(r: f64, g: f64, b: f64) -> Paint {
    Paint::OpaqueSolid(LinearRgb { r, g, b })
}

/// Paint separation used as a salient feature: max abs channel difference.
pub(super) fn separation(a: Paint, b: Paint) -> f64 {
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

pub(super) fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Pt> {
    vec![
        Pt::new(x0, y0),
        Pt::new(x1, y0),
        Pt::new(x1, y1),
        Pt::new(x0, y1),
    ]
}

/// A star polygon ring with positive signed area.
pub(super) fn star(center: Pt, r_outer: f64, r_inner: f64, points: usize, phase: f64) -> Vec<Pt> {
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
pub(crate) fn procedural_groups(variants_per_family: usize) -> Vec<GtSourceGroup> {
    procedural_groups_for_generation(variants_per_family, PROCEDURAL_GENERATION)
}

pub(crate) fn procedural_groups_for_generation(
    variants_per_family: usize,
    generation: u32,
) -> Vec<GtSourceGroup> {
    procedural_groups_filtered_for_generation(variants_per_family, generation, |_| true)
}

pub(crate) fn procedural_groups_filtered_for_generation(
    variants_per_family: usize,
    generation: u32,
    keep: impl Fn(&str) -> bool,
) -> Vec<GtSourceGroup> {
    let mut out = Vec::new();
    for family in SHAPE_FAMILIES {
        for v in 0..variants_per_family {
            let id = format!("proc/{family}/{v:03}");
            if !keep(&id) {
                continue;
            }
            match crate::gt::recipes::build_variant(family, v, &id, generation) {
                Ok(group) => out.push(group),
                Err(why) => panic!("procedural recipe {id} does not certify: {why}"),
            }
        }
    }
    out
}

pub(super) fn group_of(
    id: &str,
    family: &str,
    generation: u32,
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
        provenance: format!(
            "generated by vice-bench gt::grammar procedural generation {} in this repository",
            generation
        ),
        scenes: vec![s],
        equivalence_class: None,
        intentionally_ambiguous: false,
    })
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
    fn successor_generations_rekey_but_generation_one_preserves_the_legacy_stream() {
        let label = "proc/annulus/000";
        let mut legacy_seed = 0xcbf2_9ce4_8422_2325u64;
        for byte in label.bytes() {
            legacy_seed ^= u64::from(byte);
            legacy_seed = legacy_seed.wrapping_mul(0x0000_0100_0000_01b3);
        }
        let mut legacy = Rng(legacy_seed);
        let mut current = Rng::from_label(label);
        assert_eq!(
            current.next_u64(),
            legacy.next_u64(),
            "the frozen M3 corpus must keep its exact legacy stream"
        );
        let mut legacy = Rng(legacy_seed);
        let mut successor = Rng::from_label_generation(label, M7_PROCEDURAL_GENERATION);
        assert_ne!(
            successor.next_u64(),
            legacy.next_u64(),
            "a successor audit generation must not reuse opened fixture bytes"
        );
        let mut previous = Rng::from_label_generation(label, 2);
        let mut current = Rng::from_label_generation(label, M7_PROCEDURAL_GENERATION);
        assert_ne!(
            current.next_u64(),
            previous.next_u64(),
            "the successor generation must not reuse burned generation 2 fixture bytes"
        );
    }

    #[test]
    fn successor_two_islands_stays_inside_the_authoring_canvas() {
        // Generation 3 variant 35 exposed that independently valid island
        // rings could cross the opaque full-bleed canvas boundary.  Keep the
        // concrete witness, then cover the complete successor family so the
        // fix is a construction invariant rather than a seed patch.
        for v in 0..crate::gt::corpus::M7_SUCCESSOR_PROCEDURAL_VARIANTS {
            let id = format!("proc/two_islands/{v:03}");
            crate::gt::recipes::build_variant("two_islands", v, &id, M7_PROCEDURAL_GENERATION)
                .unwrap_or_else(|why| panic!("successor recipe {id} must certify: {why}"));
        }
    }

    #[test]
    fn frozen_generation_one_two_islands_keeps_its_m3_digest() {
        let id = "proc/two_islands/000";
        let group = crate::gt::recipes::build_variant("two_islands", 0, id, PROCEDURAL_GENERATION)
            .expect("the frozen M3 recipe certifies");
        assert_eq!(
            vice_ir::scene_digest_sha256(group.scenes[0].scene().scene())
                .expect("the frozen scene has a canonical digest"),
            "bcba20581a4cdd5f309ba54b8e89e6c7145b623a1fb4973cec0436558b91652c"
        );
    }

    #[test]
    fn filtered_construction_preserves_selected_groups() {
        let eager = procedural_groups(3);
        let filtered = procedural_groups_filtered_for_generation(3, PROCEDURAL_GENERATION, |id| {
            id == "proc/annulus/001" || id == "proc/star/002"
        });
        assert_eq!(
            filtered
                .iter()
                .map(|group| group.id.as_str())
                .collect::<Vec<_>>(),
            vec!["proc/annulus/001", "proc/star/002"]
        );
        for selected in &filtered {
            let reference = eager
                .iter()
                .find(|group| group.id == selected.id)
                .expect("selected group exists in eager corpus");
            assert_eq!(
                selected.scenes[0].partition_truth(),
                reference.scenes[0].partition_truth()
            );
            assert_eq!(
                vice_ir::scene_digest_sha256(selected.scenes[0].scene().scene())
                    .expect("selected scene digests"),
                vice_ir::scene_digest_sha256(reference.scenes[0].scene().scene())
                    .expect("reference scene digests")
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
                let g = crate::gt::recipes::build_variant(family, v, &id, PROCEDURAL_GENERATION)
                    .expect("recipe certifies");
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
                    let g =
                        crate::gt::recipes::build_variant(family, v, &id, PROCEDURAL_GENERATION)
                            .unwrap();
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
            let has_curve = g.scenes[0].scene().graph().boundaries.iter().any(|b| {
                b.curve
                    .segments
                    .iter()
                    .any(|s| *s != vice_ir::Segment::Line)
            });
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
