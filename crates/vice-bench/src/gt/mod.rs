//! Ground-truth corpus with identifiability metadata (spec v1.3 §27.1,
//! §28 M3).
//!
//! Structure, and why it is this one. §27.4 makes the INDEPENDENT UNIT of a
//! reliability trial the source-scene family, not the render, so the corpus
//! is a three-level tree and never a flat list:
//!
//! ```text
//! GtSourceGroup   one independent trial unit; splits move whole groups
//!   GtScene       one visible planar partition (a group has >1 only when
//!                 the group IS an ambiguity pair or an equivalence family)
//!     GtRender    one scene under one degradation cell — where an
//!                 identifiability label actually lives
//! ```
//!
//! Identifiability is a property of the RENDER, not of the scene: a ring at
//! 512 px is `Identifiable` and the same ring at 16 px can be
//! `InformationLost`. Putting the label on the scene would be the mistake
//! that makes a corpus quietly wrong, so the types do not allow it.
//!
//! Every truth field here is either MEASURED from the certified scene or
//! declared as diagnostic-only. `authored_truth` is explicitly the latter
//! (§27.1: authored truth is additional diagnostics, never the target): the
//! recipe that produced a scene is not what a vectorizer is judged against.

pub mod build;
pub mod grammar;
mod recipes;

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use vice_geom::Pt;
use vice_ir::{FaceId, Paint, ValidatedScene};
use vice_render::{CertifiedMesh, RenderOptions};

/// Which of the three independent sources a group came from (§27.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FixtureOrigin {
    /// Procedural scenes from the grammar in [`grammar`].
    Procedural,
    /// Hand-authored SVG committed in this repository.
    Authored,
    /// Adversarial / metamorphic constructions, including ambiguity pairs.
    Adversarial,
}

impl FixtureOrigin {
    pub fn as_str(&self) -> &'static str {
        match self {
            FixtureOrigin::Procedural => "procedural",
            FixtureOrigin::Authored => "authored",
            FixtureOrigin::Adversarial => "adversarial",
        }
    }
}

/// Identifiability of ONE render (spec §1.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentifiabilityClass {
    /// The observed raster determines the visible scene up to the frozen
    /// delivery tolerance.
    Identifiable,
    /// Several scenes are equally correct answers; the correct behaviour is
    /// to deliver any member of the class, not to guess one.
    EquivalentFamily,
    /// A salient feature leaves no trace in the observed bytes. The correct
    /// behaviour is `ambiguous`, and a confident reconstruction is wrong
    /// even when it happens to match the authored truth.
    InformationLost,
}

impl IdentifiabilityClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            IdentifiabilityClass::Identifiable => "identifiable",
            IdentifiabilityClass::EquivalentFamily => "equivalent_family",
            IdentifiabilityClass::InformationLost => "information_lost",
        }
    }
}

/// A feature a correct answer must preserve, and the quantity that decides
/// whether it survived a degradation.
///
/// Each variant carries the number a rasterized image can be measured
/// against — not a name. "Preserve the hole" is not checkable; "the hole is
/// 8 px across in scene units, so at scale 1/8 it is one pixel wide" is.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum SalientFeature {
    /// A hole in a face, with its area in scene px².
    Hole { face: u32, area_px2: f64 },
    /// A connected visible component, with its area in scene px².
    Component { area_px2: f64 },
    /// A corner of the visible partition, with its interior angle.
    Corner {
        at_x: f64,
        at_y: f64,
        angle_deg: f64,
    },
    /// The narrowest place of the visible ink, in scene px.
    ThinFeature { min_width_px: f64 },
    /// Two paints that must stay distinguishable, with their linear-RGB
    /// separation (max abs channel difference).
    PaintPair { separation: f64 },
}

impl SalientFeature {
    /// The characteristic LENGTH of the feature in scene px, or infinity
    /// when the feature has no length. Used by the identifiability rule
    /// together with the render's scale factor.
    pub fn scale_px(&self) -> f64 {
        match self {
            SalientFeature::Hole { area_px2, .. } => area_px2.max(0.0).sqrt(),
            SalientFeature::Component { area_px2 } => area_px2.max(0.0).sqrt(),
            // A corner is a point feature: it survives while the edges
            // meeting there are separable, which the thin-feature scale
            // already governs, so it never triggers the size floor itself.
            SalientFeature::Corner { .. } => f64::INFINITY,
            SalientFeature::ThinFeature { min_width_px } => *min_width_px,
            // Paint separation is not a length; contrast is handled by the
            // quantization floor of the degradation axis.
            SalientFeature::PaintPair { .. } => f64::INFINITY,
        }
    }

    /// The linear-RGB separation this feature requires, or 1.0 when it does
    /// not constrain contrast.
    pub fn required_separation(&self) -> f64 {
        match self {
            SalientFeature::PaintPair { separation } => *separation,
            _ => 1.0,
        }
    }
}

/// The recipe that produced a scene. DIAGNOSTIC ONLY (§27.1): never a
/// scoring target, because a different recipe producing the same visible
/// partition is an equally correct answer.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AuthoredTruth {
    pub construction: String,
    pub parameters: BTreeMap<String, f64>,
}

impl AuthoredTruth {
    pub fn new(construction: impl Into<String>, params: &[(&str, f64)]) -> AuthoredTruth {
        AuthoredTruth {
            construction: construction.into(),
            parameters: params.iter().map(|(k, v)| ((*k).to_string(), *v)).collect(),
        }
    }
}

/// Measured facts about the visible partition. Every field is computed from
/// the certified scene, not declared by the generator — a generator that
/// declares its own truth cannot detect its own bugs.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PartitionTruth {
    pub visible_faces: u32,
    pub holes: u32,
    pub components: u32,
    /// Distinct opaque paints, canonically ordered.
    pub palette: Vec<[f64; 3]>,
    /// The formation's declared exterior model.
    pub exterior_model: &'static str,
    /// How much of the canvas the TRUE exterior face actually covers,
    /// MEASURED by rendering. An opaque-exterior scene is a full-bleed
    /// background FACE (the exterior face's paint is `TransparentExterior`
    /// by IR contract), so this must be ~0 there; a fixture that declares
    /// `opaque` while leaving the exterior visible is inconsistent, and the
    /// only way to notice is to measure rather than to read the label.
    pub exterior_visible_px2: f64,
    /// Signed area of each face in scene px², indexed by `FaceId`.
    pub face_area_px2: Vec<f64>,
    pub total_ink_px2: f64,
}

impl PartitionTruth {
    /// Measure the partition of a certified scene.
    ///
    /// Areas come from the shoelace of the CERTIFIED mesh polylines, not
    /// from a render: an area read off a raster would be a property of the
    /// degradation, and this is the scene's own truth.
    pub fn measure(scene: &ValidatedScene, certified: &CertifiedMesh) -> PartitionTruth {
        let g = scene.graph();
        let mesh = certified.mesh();
        let exterior = g.exterior.index();

        let mut face_area_px2 = Vec::with_capacity(mesh.face_loops.len());
        let mut holes = 0u32;
        for (fi, loops) in mesh.face_loops.iter().enumerate() {
            let mut area = 0.0;
            for lp in loops {
                let a = polyline_signed_area(&lp.points);
                if a < 0.0 && fi != exterior {
                    holes += 1;
                }
                area += a;
            }
            face_area_px2.push(area);
        }

        let mut palette: Vec<[f64; 3]> = Vec::new();
        let mut seen: BTreeSet<[u64; 3]> = BTreeSet::new();
        for (fi, f) in g.faces.iter().enumerate() {
            if fi == exterior {
                continue;
            }
            if let Paint::OpaqueSolid(c) = f.paint {
                let key = [c.r.to_bits(), c.g.to_bits(), c.b.to_bits()];
                if seen.insert(key) {
                    palette.push([c.r, c.g, c.b]);
                }
            }
        }
        palette.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let visible_faces = (g.faces.len() - 1) as u32;
        let total_ink_px2 = g
            .faces
            .iter()
            .enumerate()
            .filter(|(fi, f)| *fi != exterior && matches!(f.paint, Paint::OpaqueSolid(_)))
            .map(|(fi, _)| face_area_px2[fi])
            .sum();

        let render = vice_render::render_mesh_partition(certified)
            .expect("a certified mesh renders; the certificate is what guarantees it");
        let exterior_visible_px2 = render.face_coverage[exterior].iter().sum();

        PartitionTruth {
            visible_faces,
            holes,
            components: count_components(scene),
            palette,
            exterior_model: match scene.scene().formation.exterior {
                vice_ir::ExteriorModel::Transparent => "transparent",
                vice_ir::ExteriorModel::Opaque => "opaque",
            },
            exterior_visible_px2,
            face_area_px2,
            total_ink_px2,
        }
    }
}

/// Shoelace area of a CLOSED polyline (first point repeated at the end, as
/// `LoopPolygon` guarantees).
pub fn polyline_signed_area(points: &[Pt]) -> f64 {
    let mut sum = 0.0;
    for w in points.windows(2) {
        sum += w[0].x * w[1].y - w[1].x * w[0].y;
    }
    0.5 * sum
}

/// Connected components of the non-exterior faces under boundary adjacency.
///
/// Definition chosen deliberately: a donut's ring and its hole are ONE
/// component, because they are one connected planar region of the visible
/// partition. Counting "ink blobs" instead would make the count depend on
/// paint, which is a different axis and is measured separately.
fn count_components(scene: &ValidatedScene) -> u32 {
    let g = scene.graph();
    let n = g.faces.len();
    let exterior = g.exterior.index();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], x: usize) -> usize {
        let mut x = x;
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    for b in &g.boundaries {
        let (l, r) = (b.left_face.index(), b.right_face.index());
        if l == exterior || r == exterior {
            continue;
        }
        let (a, c) = (find(&mut parent, l), find(&mut parent, r));
        if a != c {
            parent[a] = c;
        }
    }
    let mut roots = BTreeSet::new();
    for f in 0..n {
        if f == exterior {
            continue;
        }
        roots.insert(find(&mut parent, f));
    }
    roots.len() as u32
}

/// One visible planar partition of the corpus.
///
/// The scene is private and reachable only together with its certificate:
/// constructing a `GtScene` runs `CertifiedMesh::from_scene`, so a fixture
/// that is not a certified planar embedding cannot exist (debt D-4). This
/// is the first non-rendering consumer the reviews were talking about.
#[derive(Debug, Clone)]
pub struct GtScene {
    id: String,
    group_id: String,
    scene: ValidatedScene,
    certified: CertifiedMesh,
    authored_truth: AuthoredTruth,
    salient_features: Vec<SalientFeature>,
    partition_truth: PartitionTruth,
}

impl GtScene {
    pub fn new(
        id: impl Into<String>,
        group_id: impl Into<String>,
        scene: ValidatedScene,
        authored_truth: AuthoredTruth,
        salient_features: Vec<SalientFeature>,
    ) -> Result<GtScene, vice_render::RenderError> {
        let certified = CertifiedMesh::from_scene(&scene, RenderOptions::default())?;
        let partition_truth = PartitionTruth::measure(&scene, &certified);
        Ok(GtScene {
            id: id.into(),
            group_id: group_id.into(),
            scene,
            certified,
            authored_truth,
            salient_features,
            partition_truth,
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn group_id(&self) -> &str {
        &self.group_id
    }
    pub fn scene(&self) -> &ValidatedScene {
        &self.scene
    }
    pub fn certified(&self) -> &CertifiedMesh {
        &self.certified
    }
    pub fn authored_truth(&self) -> &AuthoredTruth {
        &self.authored_truth
    }
    pub fn salient_features(&self) -> &[SalientFeature] {
        &self.salient_features
    }
    pub fn partition_truth(&self) -> &PartitionTruth {
        &self.partition_truth
    }

    /// The smallest salient LENGTH in scene px: what a degradation has to
    /// preserve for this scene to stay identifiable.
    pub fn min_salient_scale_px(&self) -> f64 {
        self.salient_features
            .iter()
            .map(|f| f.scale_px())
            .fold(f64::INFINITY, f64::min)
    }

    /// The tightest paint separation any salient feature requires.
    pub fn min_salient_separation(&self) -> f64 {
        self.salient_features
            .iter()
            .map(|f| f.required_separation())
            .fold(1.0, f64::min)
    }

    pub fn face_paint(&self, f: FaceId) -> Paint {
        self.scene.graph().faces[f.index()].paint
    }
}

/// Why several scenes are equally correct answers (§27.1 "допустимая scene
/// equivalence class").
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EquivalenceClass {
    pub id: String,
    /// Scene ids that are all correct answers for any member's raster.
    pub members: Vec<String>,
    pub rationale: String,
}

/// One independent trial unit (§27.4).
#[derive(Debug, Clone)]
pub struct GtSourceGroup {
    pub id: String,
    pub origin: FixtureOrigin,
    /// Shape family. Splits move whole families, so this is recorded per
    /// group and used by the split assignment.
    pub shape_family: String,
    /// Where this group legally comes from.
    pub provenance: String,
    pub scenes: Vec<GtScene>,
    /// Present when the group's scenes form an equivalence class or an
    /// intentionally ambiguous pair.
    pub equivalence_class: Option<EquivalenceClass>,
    /// True when the group exists to test correct abstention: its scenes
    /// are DIFFERENT and become indistinguishable after degradation, so the
    /// only correct answer is `ambiguous` (§27.1).
    pub intentionally_ambiguous: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gt::build::SceneBuilder;
    use vice_ir::{
        BlendSpace, ExteriorModel, GlobalFormationHypothesis, LinearRgb, PixelFilter,
        QuantizationModel,
    };

    fn formation() -> GlobalFormationHypothesis {
        GlobalFormationHypothesis {
            blend_space: BlendSpace::LinearLight,
            pixel_filter: PixelFilter::Box,
            quantization: QuantizationModel::Uint8,
            exterior: ExteriorModel::Transparent,
        }
    }

    fn ink(r: f64, g: f64, b: f64) -> Paint {
        Paint::OpaqueSolid(LinearRgb { r, g, b })
    }

    fn sq(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Pt> {
        vec![
            Pt::new(x0, y0),
            Pt::new(x1, y0),
            Pt::new(x1, y1),
            Pt::new(x0, y1),
        ]
    }

    #[test]
    fn partition_truth_is_measured_from_the_scene_not_declared() {
        // A donut plus a separate island: 3 visible faces, 1 hole,
        // 2 components, 2 distinct paints.
        let mut b = SceneBuilder::new(64, 64, formation());
        let ring = b.add_face(ink(0.1, 0.2, 0.3));
        let hole = b.add_face(Paint::TransparentExterior);
        let island = b.add_face(ink(0.9, 0.1, 0.1));
        b.add_polygon_ring(&sq(4.0, 4.0, 28.0, 28.0), ring, SceneBuilder::EXTERIOR)
            .unwrap();
        b.add_polygon_ring(&sq(12.0, 12.0, 20.0, 20.0), hole, ring)
            .unwrap();
        b.add_polygon_ring(&sq(40.0, 40.0, 56.0, 56.0), island, SceneBuilder::EXTERIOR)
            .unwrap();
        let scene = b.build().unwrap();
        let certified = CertifiedMesh::from_scene(&scene, RenderOptions::default()).unwrap();
        let t = PartitionTruth::measure(&scene, &certified);

        assert_eq!(t.visible_faces, 3);
        assert_eq!(t.holes, 1, "the ring carries exactly one negative loop");
        assert_eq!(t.components, 2, "donut (ring+hole) and island");
        assert_eq!(t.palette.len(), 2, "the transparent hole is not a paint");
        assert_eq!(t.exterior_model, "transparent");
        // 64*64 canvas minus the donut (576) and the island (256).
        assert!(
            (t.exterior_visible_px2 - (4096.0 - 576.0 - 256.0)).abs() < 1e-9,
            "{}",
            t.exterior_visible_px2
        );
        // Ring 576 - 64 = 512; island 256; the hole is not ink.
        assert!(
            (t.total_ink_px2 - 768.0).abs() < 1e-9,
            "{}",
            t.total_ink_px2
        );
        assert!((t.face_area_px2[hole] - 64.0).abs() < 1e-9);
    }

    #[test]
    fn a_scene_that_is_not_a_certified_embedding_cannot_become_a_fixture() {
        let mut b = SceneBuilder::new(32, 32, formation());
        let f = b.add_face(ink(0.5, 0.5, 0.5));
        let inside_out = crate::gt::build::reversed(&sq(8.0, 8.0, 24.0, 24.0));
        b.add_polygon_ring(&inside_out, f, SceneBuilder::EXTERIOR)
            .unwrap();
        let scene = b.build().expect("combinatorially valid");
        assert!(
            GtScene::new(
                "x",
                "g",
                scene,
                AuthoredTruth::new("inside-out square", &[]),
                Vec::new()
            )
            .is_err(),
            "the GT loader is a D-4 consumer: no certificate, no fixture"
        );
    }
}
