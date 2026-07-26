//! Degradation matrix and the identifiability rule (spec §27.2, §1.5).
//!
//! A degradation CELL is one point of the observation model: output size,
//! subpixel phase, rasterizer profile, PSF, blend space, resize chain and
//! contrast. A cell applied to a scene produces one render, and a render is
//! where an identifiability label belongs.
//!
//! The identifiability rule is a MEASURED threshold, not an opinion. §1.5
//! defines `InformationLost` as detail that is physically unrecoverable, so
//! the test asks whether the feature's PARAMETERS survive: a hole and a
//! hole 1.4x larger are rendered at a sweep of scales, and the floor is the
//! size at which the two stop being distinguishable after 8-bit
//! quantization. Both directions are asserted, because a threshold with
//! only sensitivity or only specificity proven is not a threshold (F-0010).
//! Two earlier formulations of this test were refuted by their own
//! measurements; the refutations are kept next to the test, because they
//! are the reason the label claims what it claims and nothing more.

use serde::Serialize;
use vice_ir::BlendSpace;

use super::colour::composite_rgba8;
use super::grammar::AUTHORING_CANVAS_PX;
use super::raster::{rasterize, Psf, RasterProfile, ViewTransform};
use super::{GtScene, IdentifiabilityClass};

/// Resampling applied after rasterization (§27.2 "resize chains").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResizeChain {
    None,
    /// Render at 2x and box-downsample: the common "supersampled export".
    DownFrom2x,
    /// Render at half size and bilinearly upsample: the common "someone
    /// enlarged a small asset" degradation, which destroys detail
    /// irreversibly and is exactly the case abstention exists for.
    UpFromHalf,
}

impl ResizeChain {
    pub fn as_str(&self) -> &'static str {
        match self {
            ResizeChain::None => "none",
            ResizeChain::DownFrom2x => "down-from-2x",
            ResizeChain::UpFromHalf => "up-from-half",
        }
    }

    /// How much this chain multiplies the smallest resolvable length. A
    /// half-size intermediate throws away a factor of two before anything
    /// downstream can see it.
    pub fn detail_penalty(&self) -> f64 {
        match self {
            ResizeChain::None => 1.0,
            ResizeChain::DownFrom2x => 1.0,
            ResizeChain::UpFromHalf => 2.0,
        }
    }
}

/// One point of the observation model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DegradationCell {
    pub size_px: u32,
    pub subpixel_dx: f64,
    pub subpixel_dy: f64,
    pub profile: RasterProfile,
    pub psf: Psf,
    pub blend: BlendSpace,
    pub resize: ResizeChain,
    /// Multiplier pulling paints toward mid grey; 1.0 = authored contrast.
    pub contrast: f64,
}

impl DegradationCell {
    /// Stable identifier. Deliberately readable: a cell id appears in every
    /// per-render row of a report, and an opaque hash there would make the
    /// report unusable for diagnosis.
    pub fn id(&self) -> String {
        let psf = match self.psf {
            Psf::Gaussian { sigma_px } => format!("gauss{sigma_px:.2}"),
            other => other.as_str().to_string(),
        };
        let blend = match self.blend {
            BlendSpace::LinearLight => "lin",
            BlendSpace::EncodedSrgb => "srgb",
        };
        format!(
            "s{}_p{}_{}_{}_{}_dx{:.2}dy{:.2}_c{:.2}",
            self.size_px,
            self.profile.as_str(),
            psf,
            blend,
            self.resize.as_str(),
            self.subpixel_dx,
            self.subpixel_dy,
            self.contrast
        )
    }

    /// A cell is realizable when its profile can produce its PSF.
    pub fn is_realizable(&self) -> bool {
        self.profile.supports(self.psf)
    }

    /// Renders produced by the inverse-crime profile are diagnostics only.
    pub fn is_inverse_crime(&self) -> bool {
        self.profile.is_inverse_crime()
    }

    fn base(size_px: u32, profile: RasterProfile) -> DegradationCell {
        DegradationCell {
            size_px,
            subpixel_dx: 0.0,
            subpixel_dy: 0.0,
            profile,
            psf: Psf::Box,
            blend: BlendSpace::LinearLight,
            resize: ResizeChain::None,
            contrast: 1.0,
        }
    }
}

/// Sizes of the size axis (§27.2: 16..512).
pub const SIZES_PX: &[u32] = &[16, 32, 64, 128, 256, 512];

/// Above this size the supersampling integrator is too expensive to run
/// over the whole corpus (it costs `k² = 576` inside-tests per pixel), so
/// the PSF axis is exercised at small sizes only. Stated as a limit of the
/// INSTRUMENT rather than presented as a design choice about the axis.
pub const MAX_SUPERSAMPLED_SIZE_PX: u32 = 64;

/// The frozen V1 degradation matrix.
///
/// Design: a full factorial over seven axes would be tens of thousands of
/// cells and would spend almost all of it on combinations that vary
/// nothing interesting. Instead the matrix is a SPINE (every size x every
/// profile, at the authored phase and formation) plus ONE-AXIS-AT-A-TIME
/// excursions anchored at two sizes. Every axis of §27.2 therefore appears,
/// every axis is separable from the others in analysis, and the count stays
/// small enough to regenerate from a clean checkout.
pub fn matrix_v1() -> Vec<DegradationCell> {
    let mut cells = Vec::new();

    // Spine: size x profile.
    for &size in SIZES_PX {
        for &profile in RasterProfile::ALL {
            if profile == RasterProfile::Supersample && size > MAX_SUPERSAMPLED_SIZE_PX {
                continue;
            }
            cells.push(DegradationCell::base(size, profile));
        }
    }

    // Excursions, anchored where the interesting behaviour lives: one size
    // where features are comfortable (128) and one where they are not (32).
    for &anchor in &[32u32, 128u32] {
        // Subpixel phase: the classic source of correlated near-duplicates,
        // which is why it is an axis of the DEGRADATION and never a way to
        // make a new source group (§27.4).
        for (dx, dy) in [(1.0 / 3.0, 0.5), (0.5, 0.25)] {
            let mut c = DegradationCell::base(anchor, RasterProfile::ExactClip);
            c.subpixel_dx = dx;
            c.subpixel_dy = dy;
            cells.push(c);
        }
        // Blend space.
        let mut c = DegradationCell::base(anchor, RasterProfile::ExactClip);
        c.blend = BlendSpace::EncodedSrgb;
        cells.push(c);
        // Contrast.
        for contrast in [0.5, 0.15] {
            let mut c = DegradationCell::base(anchor, RasterProfile::ExactClip);
            c.contrast = contrast;
            cells.push(c);
        }
        // Resize chains.
        for resize in [ResizeChain::DownFrom2x, ResizeChain::UpFromHalf] {
            let mut c = DegradationCell::base(anchor, RasterProfile::ExactClip);
            c.resize = resize;
            cells.push(c);
        }
    }

    // PSF excursions, on the only profile that can realize them and only
    // at sizes the instrument can afford.
    for &anchor in &[32u32, 64u32] {
        for psf in [
            Psf::Triangle,
            Psf::Gaussian { sigma_px: 0.5 },
            Psf::Gaussian { sigma_px: 1.0 },
        ] {
            let mut c = DegradationCell::base(anchor, RasterProfile::Supersample);
            c.psf = psf;
            cells.push(c);
        }
    }

    cells
}

// ---------------------------------------------------------------------------
// Identifiability
// ---------------------------------------------------------------------------

/// The smallest salient LENGTH, in render pixels, whose PARAMETERS are
/// still recoverable from the observed bytes.
///
/// Calibrated by measurement, not chosen. The discriminability of a 1.4x
/// change in feature size, measured at 64 px (see
/// `observability_floor_is_calibrated_against_a_rival_member`):
///
/// ```text
/// feature length   0.20   0.40   0.60   1.00   2.00   4.00   render px
/// max code delta      2     10     22     61    102    204
/// ```
///
/// The floor sits in the only gap the data offers: 0.20 px is at the
/// quantization noise, 0.40 px is an order of magnitude above it.
///
/// Worth stating because it is not the obvious number: the floor is well
/// BELOW one pixel. Antialiased coverage carries sub-pixel area
/// information, which is exactly why subpixel deblurring is possible at
/// all; a floor of "one pixel" would have declared recoverable detail
/// unrecoverable and quietly excused the system from finding it.
pub const OBSERVABILITY_FLOOR_PX: f64 = 0.35;

/// The smallest 8-bit code difference that survives quantization.
pub const QUANTIZATION_FLOOR_CODES: f64 = 1.0;

/// Label one (scene, cell) pair.
///
/// The rule, in order:
/// 1. a salient LENGTH below the observability floor after scaling, blur
///    and resize penalty means the feature is not in the bytes at all;
/// 2. a paint separation that quantizes to less than one code value means
///    the colours are not distinguishable in the bytes;
/// 3. a scene belonging to a declared equivalence class of more than one
///    member is `EquivalentFamily`;
/// 4. otherwise `Identifiable`.
///
/// Note the order: information loss OUTRANKS equivalence, because a member
/// of an equivalence class whose distinguishing feature has been destroyed
/// is not "several correct answers", it is "no way to tell" — and the
/// correct behaviour differs (`ambiguous` either way, but for a reason the
/// report must not confuse).
pub fn identifiability(
    scene: &GtScene,
    cell: &DegradationCell,
    equivalence_members: usize,
) -> IdentifiabilityClass {
    let scale = f64::from(cell.size_px) / f64::from(AUTHORING_CANVAS_PX);
    let blur_penalty = match cell.psf {
        Psf::Box => 1.0,
        Psf::Triangle => 2.0,
        Psf::Gaussian { sigma_px } => 1.0 + 2.0 * sigma_px,
    };
    let effective_px =
        scene.min_salient_scale_px() * scale / (blur_penalty * cell.resize.detail_penalty());
    if effective_px < OBSERVABILITY_FLOOR_PX {
        return IdentifiabilityClass::InformationLost;
    }
    // Contrast acts on the linear paint separation; what survives is the
    // 8-bit code difference after the transfer function. Evaluated at the
    // separation's own midpoint, which is where the transfer is flattest
    // for dark inks and therefore the conservative place to ask.
    let sep = scene.min_salient_separation() * cell.contrast;
    let codes = 255.0
        * (super::colour::srgb_encode(0.5 + sep / 2.0)
            - super::colour::srgb_encode((0.5 - sep / 2.0).max(0.0)));
    if codes < QUANTIZATION_FLOOR_CODES {
        return IdentifiabilityClass::InformationLost;
    }
    if equivalence_members > 1 {
        return IdentifiabilityClass::EquivalentFamily;
    }
    IdentifiabilityClass::Identifiable
}

/// One rendered fixture: the bytes plus everything needed to score them.
#[derive(Debug, Clone)]
pub struct RenderedFixture {
    pub scene_id: String,
    pub group_id: String,
    pub cell_id: String,
    pub width_px: u32,
    pub height_px: u32,
    pub rgba8: Vec<u8>,
    pub identifiability: IdentifiabilityClass,
    pub inverse_crime: bool,
}

/// Render one (scene, cell) pair to 8-bit RGBA.
pub fn render_cell(
    scene: &GtScene,
    cell: &DegradationCell,
    equivalence_members: usize,
) -> Result<RenderedFixture, String> {
    if !cell.is_realizable() {
        return Err(format!(
            "cell {} is not realizable by its profile",
            cell.id()
        ));
    }
    let paints: Vec<vice_ir::Paint> = scene
        .scene()
        .graph()
        .faces
        .iter()
        .map(|f| f.paint)
        .collect();

    let (work_px, upsample_from) = match cell.resize {
        ResizeChain::None => (cell.size_px, None),
        ResizeChain::DownFrom2x => (cell.size_px * 2, None),
        ResizeChain::UpFromHalf => (cell.size_px.div_ceil(2), Some(cell.size_px)),
    };
    let scale = f64::from(work_px) / f64::from(AUTHORING_CANVAS_PX);
    let t = ViewTransform {
        scale,
        dx: cell.subpixel_dx,
        dy: cell.subpixel_dy,
        width_px: work_px,
        height_px: work_px,
    };
    let stack = rasterize(scene.certified(), &t, cell.profile, cell.psf)?;
    let mut rgba = composite_rgba8(&stack, &paints, cell.blend, cell.contrast);
    let mut w = work_px;

    if cell.resize == ResizeChain::DownFrom2x {
        rgba = box_downsample_2x(&rgba, w);
        w /= 2;
    }
    if let Some(target) = upsample_from {
        rgba = bilinear_upsample(&rgba, w, target);
        w = target;
    }

    Ok(RenderedFixture {
        scene_id: scene.id().to_string(),
        group_id: scene.group_id().to_string(),
        cell_id: cell.id(),
        width_px: w,
        height_px: w,
        rgba8: rgba,
        identifiability: identifiability(scene, cell, equivalence_members),
        inverse_crime: cell.is_inverse_crime(),
    })
}

fn box_downsample_2x(src: &[u8], w: u32) -> Vec<u8> {
    let hw = (w / 2) as usize;
    let w = w as usize;
    let mut out = vec![0u8; hw * hw * 4];
    for y in 0..hw {
        for x in 0..hw {
            for ch in 0..4 {
                let mut acc = 0u32;
                for dy in 0..2 {
                    for dx in 0..2 {
                        acc += u32::from(src[((2 * y + dy) * w + 2 * x + dx) * 4 + ch]);
                    }
                }
                out[(y * hw + x) * 4 + ch] = ((acc + 2) / 4) as u8;
            }
        }
    }
    out
}

fn bilinear_upsample(src: &[u8], w: u32, target: u32) -> Vec<u8> {
    let (sw, tw) = (w as usize, target as usize);
    let mut out = vec![0u8; tw * tw * 4];
    let ratio = f64::from(w) / f64::from(target);
    for y in 0..tw {
        let fy = ((y as f64 + 0.5) * ratio - 0.5).clamp(0.0, (sw - 1) as f64);
        let (y0, ty) = (fy.floor() as usize, fy.fract());
        let y1 = (y0 + 1).min(sw - 1);
        for x in 0..tw {
            let fx = ((x as f64 + 0.5) * ratio - 0.5).clamp(0.0, (sw - 1) as f64);
            let (x0, tx) = (fx.floor() as usize, fx.fract());
            let x1 = (x0 + 1).min(sw - 1);
            for ch in 0..4 {
                let p = |yy: usize, xx: usize| f64::from(src[(yy * sw + xx) * 4 + ch]);
                let top = p(y0, x0) * (1.0 - tx) + p(y0, x1) * tx;
                let bot = p(y1, x0) * (1.0 - tx) + p(y1, x1) * tx;
                out[(y * tw + x) * 4 + ch] = (top * (1.0 - ty) + bot * ty).round() as u8;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gt::build::SceneBuilder;
    use crate::gt::grammar::flat2_formation;
    use crate::gt::{AuthoredTruth, GtScene, SalientFeature};
    use std::collections::BTreeSet;
    use vice_geom::Pt;
    use vice_ir::{ExteriorModel, LinearRgb, Paint};

    fn ink() -> Paint {
        Paint::OpaqueSolid(LinearRgb {
            r: 0.05,
            g: 0.08,
            b: 0.2,
        })
    }

    fn sq(cx: f64, cy: f64, half: f64) -> Vec<Pt> {
        vec![
            Pt::new(cx - half, cy - half),
            Pt::new(cx + half, cy - half),
            Pt::new(cx + half, cy + half),
            Pt::new(cx - half, cy + half),
        ]
    }

    const OUTER_HALF_FRAC: f64 = 0.3;

    /// A square outline of `half` scene px with an optional square hole.
    fn probe(outer_half: f64, hole_half: Option<f64>, id: &str) -> GtScene {
        let c = f64::from(AUTHORING_CANVAS_PX);
        let mut b = SceneBuilder::new(
            AUTHORING_CANVAS_PX,
            AUTHORING_CANVAS_PX,
            flat2_formation(ExteriorModel::Transparent),
        );
        let face = b.add_face(ink());
        b.add_polygon_ring(
            &sq(c / 2.0, c / 2.0, outer_half),
            face,
            SceneBuilder::EXTERIOR,
        )
        .unwrap();
        if let Some(h) = hole_half {
            let hole = b.add_face(Paint::TransparentExterior);
            b.add_polygon_ring(&sq(c / 2.0, c / 2.0, h), hole, face)
                .unwrap();
        }
        let scene = b.build().unwrap();
        GtScene::new(
            id,
            "g",
            scene,
            AuthoredTruth::new("hole probe", &[("outer_half_px", outer_half)]),
            vec![SalientFeature::Hole {
                face: 2,
                area_px2: 4.0 * hole_half.unwrap_or(outer_half).powi(2),
            }],
        )
        .unwrap()
    }

    /// The two scenes the identifiability question actually needs: the
    /// same outline with a hole, and with a hole 1.4x larger.
    ///
    /// The rival is a DIFFERENT MEMBER OF THE SAME FAMILY, not the absence
    /// of the feature. §1.5 asks whether the scene is determined by the
    /// observation, so the question is whether the feature's PARAMETERS can
    /// be recovered: if a hole and a hole half again as big produce the
    /// same bytes, nothing downstream can tell them apart, and confidently
    /// reporting either one is a guess.
    fn hole_and_rival(hole_half: f64) -> (GtScene, GtScene) {
        let c = f64::from(AUTHORING_CANVAS_PX);
        let outer = c * OUTER_HALF_FRAC;
        (
            probe(outer, Some(hole_half), "holed"),
            probe(outer, Some(hole_half * 1.4), "rival"),
        )
    }

    fn max_code_difference(a: &RenderedFixture, b: &RenderedFixture) -> u8 {
        a.rgba8
            .iter()
            .zip(&b.rgba8)
            .map(|(x, y)| x.abs_diff(*y))
            .max()
            .unwrap_or(0)
    }

    /// The observability floor is CALIBRATED against a RIVAL MEMBER of the
    /// same family, and both directions are proven (F-0010: a threshold
    /// with only sensitivity or only specificity is not a threshold).
    ///
    /// Two earlier formulations were refuted by their own measurements and
    /// are recorded here because the refutations are the point:
    ///
    /// 1. "the feature changes no byte" - false. A hole 0.2 render px
    ///    across still moves 3 code values. Leaving a TRACE and being
    ///    IDENTIFIABLE are different questions.
    /// 2. "a hole-free scene of equal ink area reproduces the bytes" -
    ///    also false, and for a structural reason: the hole is in the
    ///    middle and a shrunk outline differs at the BORDER, so the two
    ///    disagree in different places and the max-difference statistic
    ///    was identical for both comparisons (measured: 3/3, 10/10,
    ///    23/23...). An area-matched rival is not a rival for a centred
    ///    feature.
    ///
    /// What the label therefore claims, precisely: below the floor the
    /// feature's PARAMETERS are not recoverable - a hole and a hole 1.4x
    /// larger produce the same bytes. It does NOT claim the feature leaves
    /// no trace at all; the measurement above says otherwise, and the
    /// corpus states the weaker, true thing.
    #[test]
    fn observability_floor_is_calibrated_against_a_rival_member() {
        let cell = DegradationCell::base(64, RasterProfile::ExactClip);
        let scale = f64::from(cell.size_px) / f64::from(AUTHORING_CANVAS_PX);
        let mut rows = Vec::new();
        for &hole_half in &[0.4f64, 0.8, 1.2, 2.0, 4.0, 8.0] {
            let (holed, rival) = hole_and_rival(hole_half);
            let a = render_cell(&holed, &cell, 1).unwrap();
            let r = render_cell(&rival, &cell, 1).unwrap();
            let d_rival = max_code_difference(&a, &r);
            let len_px = 2.0 * hole_half * scale;
            println!(
                "hole {len_px:.3} render px: vs 1.4x rival {d_rival:>3}, label {:?}",
                a.identifiability
            );
            rows.push((len_px, d_rival, a.identifiability));
        }

        for (len_px, d_rival, label) in rows {
            if len_px < OBSERVABILITY_FLOOR_PX {
                assert_eq!(
                    label,
                    IdentifiabilityClass::InformationLost,
                    "a {len_px:.3} px hole is below the floor and must be labelled lost"
                );
                assert!(
                    d_rival <= RIVAL_INDISTINGUISHABLE_CODES,
                    "labelled lost, but a 1.4x larger hole is {d_rival} code values away -                      the parameters ARE recoverable and the floor is too high"
                );
            } else {
                assert_eq!(
                    label,
                    IdentifiabilityClass::Identifiable,
                    "a {len_px:.3} px hole is above the floor"
                );
                assert!(
                    d_rival > RIVAL_INDISTINGUISHABLE_CODES,
                    "labelled identifiable, but a 1.4x larger hole is only {d_rival} code                      values away - the floor is too low"
                );
            }
        }
    }

    /// How close a rival member has to come before the feature's
    /// parameters count as unrecoverable: a few code values, i.e. the
    /// quantization noise. Set from the measurement printed by the test
    /// above, not chosen first.
    const RIVAL_INDISTINGUISHABLE_CODES: u8 = 4;

    /// The contrast clause has the same obligation: below the quantization
    /// floor the two paints must genuinely collapse into the same bytes.
    #[test]
    fn the_contrast_clause_is_calibrated_by_measurement() {
        let c = f64::from(AUTHORING_CANVAS_PX);
        let mut b = SceneBuilder::new(
            AUTHORING_CANVAS_PX,
            AUTHORING_CANVAS_PX,
            flat2_formation(ExteriorModel::Opaque),
        );
        let bg = b.add_face(Paint::OpaqueSolid(LinearRgb {
            r: 0.5,
            g: 0.5,
            b: 0.5,
        }));
        b.add_polygon_ring(&sq(c / 2.0, c / 2.0, c / 2.0), bg, SceneBuilder::EXTERIOR)
            .unwrap();
        let fg = b.add_face(Paint::OpaqueSolid(LinearRgb {
            r: 0.5 + 0.02,
            g: 0.5 + 0.02,
            b: 0.5 + 0.02,
        }));
        b.add_polygon_ring(&sq(c / 2.0, c / 2.0, c * 0.25), fg, bg)
            .unwrap();
        let scene = b.build().unwrap();
        let s = GtScene::new(
            "lowcontrast",
            "g",
            scene,
            AuthoredTruth::new("low contrast pair", &[]),
            vec![
                SalientFeature::PaintPair { separation: 0.02 },
                SalientFeature::Component {
                    area_px2: (c * 0.5) * (c * 0.5),
                },
            ],
        )
        .unwrap();

        for (contrast, expect_lost) in [(1.0f64, false), (0.02, true)] {
            let mut cell = DegradationCell::base(64, RasterProfile::ExactClip);
            cell.contrast = contrast;
            let r = render_cell(&s, &cell, 1).unwrap();
            let distinct: BTreeSet<[u8; 3]> = r
                .rgba8
                .chunks(4)
                .filter(|p| p[3] > 250)
                .map(|p| [p[0], p[1], p[2]])
                .collect();
            println!(
                "contrast {contrast}: {} distinct opaque colours",
                distinct.len()
            );
            if expect_lost {
                assert_eq!(
                    r.identifiability,
                    IdentifiabilityClass::InformationLost,
                    "contrast {contrast} must be labelled lost"
                );
                assert_eq!(
                    distinct.len(),
                    1,
                    "labelled lost but the two paints are still distinct in the bytes"
                );
            } else {
                assert_ne!(
                    r.identifiability,
                    IdentifiabilityClass::InformationLost,
                    "contrast {contrast} must be observable"
                );
                assert!(
                    distinct.len() > 1,
                    "labelled observable but the bytes show one colour"
                );
            }
        }
    }

    /// The matrix must cover every axis §27.2 names, and every cell it
    /// declares must be realizable by its own profile.
    #[test]
    fn the_matrix_covers_every_declared_axis_and_only_realizable_cells() {
        let cells = matrix_v1();
        assert!(!cells.is_empty());
        for c in &cells {
            assert!(
                c.is_realizable(),
                "declared an unrealizable cell {}",
                c.id()
            );
        }
        let ids: BTreeSet<String> = cells.iter().map(|c| c.id()).collect();
        assert_eq!(ids.len(), cells.len(), "cell ids must be unique");

        let sizes: BTreeSet<u32> = cells.iter().map(|c| c.size_px).collect();
        assert!(sizes.contains(&16) && sizes.contains(&512), "sizes 16..512");
        assert!(cells
            .iter()
            .any(|c| c.subpixel_dx != 0.0 || c.subpixel_dy != 0.0));
        assert!(cells.iter().any(|c| c.blend == BlendSpace::EncodedSrgb));
        assert!(cells.iter().any(|c| c.blend == BlendSpace::LinearLight));
        assert!(cells.iter().any(|c| matches!(c.psf, Psf::Triangle)));
        assert!(cells.iter().any(|c| matches!(c.psf, Psf::Gaussian { .. })));
        assert!(cells.iter().any(|c| c.resize == ResizeChain::DownFrom2x));
        assert!(cells.iter().any(|c| c.resize == ResizeChain::UpFromHalf));
        assert!(cells.iter().any(|c| c.contrast < 0.5));
        let profiles: BTreeSet<&str> = cells.iter().map(|c| c.profile.as_str()).collect();
        assert_eq!(
            profiles.len(),
            RasterProfile::ALL.len(),
            "every profile must appear, including the flagged inverse-crime arm"
        );
        assert!(
            cells.iter().any(|c| c.is_inverse_crime()),
            "the inverse-crime arm must be present AND flagged, not omitted"
        );
        // JPEG/WebP are M9 and must NOT be here (§27.2).
        assert!(!cells
            .iter()
            .any(|c| c.id().contains("jpeg") || c.id().contains("webp")));
    }

    /// Rendering is deterministic, and a cell really is a different image.
    #[test]
    fn cells_are_deterministic_and_actually_different() {
        let s = probe(
            f64::from(AUTHORING_CANVAS_PX) * OUTER_HALF_FRAC,
            Some(20.0),
            "probe",
        );
        let cells = matrix_v1();
        let pick = |pred: &dyn Fn(&DegradationCell) -> bool| {
            *cells.iter().find(|c| pred(c)).expect("cell present")
        };
        let base = pick(&|c| {
            c.size_px == 128
                && c.profile == RasterProfile::ExactClip
                && c.subpixel_dx == 0.0
                && c.blend == BlendSpace::LinearLight
                && c.resize == ResizeChain::None
                && c.contrast == 1.0
        });
        let a = render_cell(&s, &base, 1).unwrap();
        let b = render_cell(&s, &base, 1).unwrap();
        assert_eq!(a.rgba8, b.rgba8, "a cell must be reproducible");

        // The blend-space cell is deliberately NOT in this list: with a
        // single ink on a transparent exterior the mixing happens in alpha,
        // which is linear either way, so linear and sRGB blending produce
        // identical bytes. That is a property of the axis, established in
        // `raster::tests::blending_in_linear_light_and_in_srgb_give_different_bytes`
        // on a two-opaque-paint fixture; asserting it here on a fixture
        // that cannot show it would be asserting something false.
        for other in [
            pick(&|c| c.size_px == 128 && c.subpixel_dx != 0.0),
            pick(&|c| c.size_px == 128 && c.resize == ResizeChain::UpFromHalf),
            pick(&|c| c.size_px == 128 && c.contrast < 0.9),
        ] {
            let r = render_cell(&s, &other, 1).unwrap();
            assert_eq!(
                r.width_px,
                base.size_px,
                "cell {} changed the size",
                other.id()
            );
            assert_ne!(
                r.rgba8,
                a.rgba8,
                "cell {} must produce a different image from the base",
                other.id()
            );
        }
    }

    /// Information loss outranks equivalence: a member of an equivalence
    /// class whose distinguishing feature is gone is `InformationLost`,
    /// not `EquivalentFamily`. The two mean different things in a report.
    #[test]
    fn information_loss_outranks_equivalence() {
        let tiny = hole_and_rival(0.4).0;
        let cell = DegradationCell::base(32, RasterProfile::ExactClip);
        assert_eq!(
            identifiability(&tiny, &cell, 3),
            IdentifiabilityClass::InformationLost
        );
        let big = hole_and_rival(24.0).0;
        assert_eq!(
            identifiability(&big, &cell, 3),
            IdentifiabilityClass::EquivalentFamily
        );
        assert_eq!(
            identifiability(&big, &cell, 1),
            IdentifiabilityClass::Identifiable
        );
    }
}
