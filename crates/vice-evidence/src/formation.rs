//! The minimal global formation family, and what identifies its members
//! (spec §10.1, §16.2, §5.2).
//!
//! §10.1 fixes the family M4 supports, verbatim:
//!
//! ```text
//! blend space: linear | encoded-sRGB
//! coverage filter: analytic box | triangle | small Gaussian family
//! 8-bit quantization
//! transparent or opaque exterior
//! ```
//!
//! and §16.2 adds the sentence that shapes this module more than any other:
//! *"the kernel is GLOBAL for the image; a per-edge kernel is forbidden"*
//! (also §32 rule 16). That is why nothing here is indexed by boundary,
//! edge or region: [`enumerate`] returns whole-image hypotheses, the
//! estimator scores whole-image statistics, and there is no type in this
//! crate that could hold a kernel per edge. The prohibition is a shape, not
//! a comment.
//!
//! The exterior is NOT enumerated here. §10.1 lists it as part of the
//! family and §9.2 makes it part of the palette hypothesis; representing it
//! twice would let a run carry a formation that says "transparent" beside a
//! palette that says "opaque background". [`for_palette`] therefore derives
//! the formation's exterior from the palette hypothesis, and
//! [`FormationMismatch`] is what a caller gets for trying to pair them by
//! hand.
//!
//! ## What identifies a member
//!
//! Two things, and they are identified by DIFFERENT evidence:
//!
//! - the **blend space** shows up in the mixture RESIDUAL, and only where
//!   two opaque paints mix. With a single ink on a transparent exterior the
//!   mixing happens in alpha, which is linear either way, so the two blend
//!   spaces produce identical bytes and the honest answer is "not
//!   identifiable" rather than a coin flip;
//! - the **pixel filter** shows up in the WIDTH of the transition band,
//!   which the residual cannot see at all: a two-colour mixture fits any
//!   coverage value, so a wrong kernel leaves no residual, only a
//!   differently-shaped alpha field.
//!
//! The width statistic and its per-kernel values are measured on the
//! development split by `vice-bench`
//! (`the_kernel_profile_table_matches_the_corpus`), not asserted here.

use serde::Serialize;
use vice_ir::{
    BlendSpace, ExteriorModel, GlobalFormationHypothesis, PixelFilter, QuantizationModel,
};

use crate::palette::{BackgroundHypothesis, Flat2Hypothesis};

/// The blend spaces of §10.1.
pub const BLEND_SPACES: &[BlendSpace] = &[BlendSpace::LinearLight, BlendSpace::EncodedSrgb];

/// The coverage filters of §10.1: analytic box, triangle, and a SMALL
/// Gaussian family. "Small" is two members, chosen to bracket the corpus's
/// own PSF excursions (§27.2 uses σ = 0.5 and σ = 1.0); a wider kernel set
/// belongs to M9, which owns kernel estimation.
pub const PIXEL_FILTERS: &[PixelFilter] = &[
    PixelFilter::Box,
    PixelFilter::Triangle,
    PixelFilter::Gaussian { sigma_px: 0.5 },
    PixelFilter::Gaussian { sigma_px: 1.0 },
];

/// Size of the family a run enumerates for one palette hypothesis.
///
/// The enumeration is EXHAUSTIVE: nothing is truncated, so no search bound
/// is claimed and none is needed. That is a fact the oracle's compatibility
/// key records (§27.6 `candidate_budget`).
pub const FAMILY_SIZE: usize = 8;

/// Stable id of one formation hypothesis, for reports and hypothesis ids.
pub fn formation_id(f: &GlobalFormationHypothesis) -> String {
    let blend = match f.blend_space {
        BlendSpace::LinearLight => "lin",
        BlendSpace::EncodedSrgb => "srgb",
    };
    let filter = match f.pixel_filter {
        PixelFilter::Box => "box".to_string(),
        PixelFilter::Triangle => "triangle".to_string(),
        PixelFilter::Gaussian { sigma_px } => format!("gauss{sigma_px:.2}"),
    };
    let ext = match f.exterior {
        ExteriorModel::Transparent => "transparent",
        ExteriorModel::Opaque => "opaque",
    };
    format!("{blend}/{filter}/u8/{ext}")
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "formation exterior {formation} contradicts the palette hypothesis {palette}: the exterior \
     model is part of BOTH in the spec (10.1, 9.2) and pairing them by hand is how the two \
     silently disagree"
)]
pub struct FormationMismatch {
    pub formation: &'static str,
    pub palette: &'static str,
}

fn exterior_name(e: ExteriorModel) -> &'static str {
    match e {
        ExteriorModel::Transparent => "transparent",
        ExteriorModel::Opaque => "opaque",
    }
}

/// Every formation hypothesis of the family, for a given exterior model.
pub fn enumerate(exterior: ExteriorModel) -> Vec<GlobalFormationHypothesis> {
    let mut out = Vec::with_capacity(FAMILY_SIZE);
    for blend in BLEND_SPACES {
        for filter in PIXEL_FILTERS {
            out.push(GlobalFormationHypothesis {
                blend_space: *blend,
                pixel_filter: *filter,
                quantization: QuantizationModel::Uint8,
                exterior,
            });
        }
    }
    out
}

/// The family for one palette hypothesis: the exterior comes from the
/// palette, never from the caller.
pub fn for_palette(h: &Flat2Hypothesis) -> Vec<GlobalFormationHypothesis> {
    enumerate(h.background.exterior_model())
}

/// Check that a formation and a palette hypothesis agree about the exterior.
pub fn check_agreement(
    f: &GlobalFormationHypothesis,
    h: &Flat2Hypothesis,
) -> Result<(), FormationMismatch> {
    let want = h.background.exterior_model();
    if f.exterior == want {
        Ok(())
    } else {
        Err(FormationMismatch {
            formation: exterior_name(f.exterior),
            palette: exterior_name(want),
        })
    }
}

/// Whether the blend space can be told apart AT ALL on this hypothesis.
///
/// Not a confidence and not a threshold: a structural fact. The blend space
/// changes where coverage is applied relative to the transfer function, so
/// it is observable only where two OPAQUE paints mix. Over a transparent
/// exterior the mixing is in alpha, which is linear in both spaces, and the
/// two hypotheses predict identical bytes.
pub fn blend_space_is_identifiable(h: &Flat2Hypothesis) -> bool {
    matches!(h.background, BackgroundHypothesis::OpaqueFace(_))
}

/// The transition-band statistic of a coverage field.
///
/// `S = Σ_p 4·α_p(1−α_p) / L`, where `L` is the length of the extracted
/// `α = 0.5` contour in pixels. The numerator is the classic "how much of
/// the image is undecided" mass, weighted so a pixel exactly at 0.5 counts
/// one and a decided pixel counts zero; dividing by the contour length turns
/// it into a WIDTH, which is the thing a kernel determines and a shape does
/// not.
///
/// For an axis-aligned box-filtered edge the profile is linear over one
/// pixel and the closed form is `4∫₀¹t(1−t)dt = 2/3`; for a Gaussian of
/// width σ it is `4σ/√π ≈ 2.26σ`. Those are the values the measured table
/// below has to be compared against, and they are why the statistic is this
/// one rather than "count the pixels strictly between 0 and 1", which counts
/// the shape's perimeter as much as the kernel.
pub fn transition_width_px(alpha: &[f64], contour_length_px: f64) -> f64 {
    if contour_length_px <= 0.0 {
        return 0.0;
    }
    let mass: f64 = alpha.iter().map(|a| 4.0 * a * (1.0 - a)).sum();
    mass / contour_length_px
}

/// Share of the covered pixels that reach FULL coverage.
///
/// The statistic above is a width only while the shape is RESOLVED. On a
/// shape thinner than the kernel the coverage never reaches one, the
/// "undecided mass" is the whole shape, and `mass/length` measures the
/// shape's half-thickness instead of the kernel. The corpus says how much
/// that matters: over every family the box statistic has a spread of 1.36 px
/// around a mean of 1.09 - no information at all - while over the resolved
/// subset the spread is 0.07.
pub fn resolved_fraction(alpha: &[f64]) -> f64 {
    let covered = alpha.iter().filter(|a| **a > 0.02).count();
    if covered == 0 {
        return 0.0;
    }
    alpha.iter().filter(|a| **a >= 0.98).count() as f64 / covered as f64
}

/// Below this share of fully covered pixels the transition-width statistic
/// is measuring the shape rather than the kernel, and the pixel filter is
/// NOT identifiable from it. Both directions are measured on the corpus by
/// `vice-bench::corridor::tests::the_kernel_profile_table_matches_the_corpus`.
pub const MIN_RESOLVED_FRACTION: f64 = 0.25;

/// Is the pixel filter identifiable from this coverage field at all?
pub fn filter_is_identifiable(alpha: &[f64]) -> bool {
    resolved_fraction(alpha) >= MIN_RESOLVED_FRACTION
}

/// The transition width one kernel produces, as MEASURED on the development
/// split, with the spread over shapes and phases.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct KernelProfile {
    pub filter: PixelFilter,
    /// Mean of [`transition_width_px`] over the development scenes.
    pub width_px: f64,
    /// Standard deviation of the same, over shapes and subpixel phases. It
    /// is the denominator of the filter term, so a kernel whose statistic
    /// varies a lot across shapes cannot dominate one whose statistic does
    /// not.
    pub sd_px: f64,
}

/// Measured on the corpus by
/// `vice-bench::corridor::tests::the_kernel_profile_table_matches_the_corpus`,
/// which prints the table and fails if the code and the corpus disagree.
///
/// The BOX row is measured over EVERY engine that can realize it — the
/// exact integrator, the supersampler and both external engines — over two
/// sizes and over every shape family, which is why its spread is the widest
/// of the four. A table measured on one engine would describe that engine's
/// antialiasing rather than the kernel: the first draft did exactly that and
/// recovered the filter on three arms out of seven.
///
/// The honest consequence of the measured spreads: box (0.748 ± 0.131) and
/// triangle (1.045 ± 0.067) are separated by about two pooled spreads, so a
/// shape whose statistic lands in the tail IS misclassified, and the
/// corridor report publishes the recovery rate rather than a claim.
///
/// The numbers are NEAR the closed forms quoted on [`transition_width_px`]
/// (box 2/3, Gaussian 2.26σ) and are not equal to them: a real shape has
/// corners and a finite perimeter, and the statistic sees them. Using the
/// measurement rather than the closed form is the point.
pub const KERNEL_PROFILES_V1: &[KernelProfile] = &[
    KernelProfile {
        filter: PixelFilter::Box,
        width_px: 0.748,
        sd_px: 0.131,
    },
    KernelProfile {
        filter: PixelFilter::Triangle,
        width_px: 1.045,
        sd_px: 0.067,
    },
    KernelProfile {
        filter: PixelFilter::Gaussian { sigma_px: 0.5 },
        width_px: 1.218,
        sd_px: 0.049,
    },
    KernelProfile {
        filter: PixelFilter::Gaussian { sigma_px: 1.0 },
        width_px: 2.388,
        sd_px: 0.030,
    },
];

pub fn profile_of(filter: PixelFilter) -> Option<&'static KernelProfile> {
    KERNEL_PROFILES_V1.iter().find(|k| k.filter == filter)
}

/// Half of the squared standardized distance between an observed width and
/// the width a kernel predicts — the Gaussian negative log density, without
/// the constant that is the same for every kernel.
///
/// A SURROGATE score in the sense of [`crate::support`]: it orders formation
/// hypotheses and prunes impossible ones, and it is not in the units of the
/// final posterior.
pub fn filter_penalty(filter: PixelFilter, observed_width_px: f64) -> f64 {
    match profile_of(filter) {
        None => f64::INFINITY,
        Some(k) => {
            let z = (observed_width_px - k.width_px) / k.sd_px.max(1e-6);
            0.5 * z * z
        }
    }
}

/// Which kernels the observed width CANNOT separate.
///
/// Two kernels whose predicted widths are closer than the measurement's own
/// spread are not distinguishable by this statistic, and saying so is the
/// difference between an estimate and a guess. Returns every filter whose
/// penalty is within `margin` of the best.
pub fn filters_within_margin(observed_width_px: f64, margin: f64) -> Vec<PixelFilter> {
    let best = PIXEL_FILTERS
        .iter()
        .map(|f| filter_penalty(*f, observed_width_px))
        .fold(f64::INFINITY, f64::min);
    PIXEL_FILTERS
        .iter()
        .copied()
        .filter(|f| filter_penalty(*f, observed_width_px) <= best + margin)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::oracle_override;
    use vice_ir::LinearRgb;

    #[test]
    fn the_family_is_the_one_the_spec_names_and_nothing_more() {
        let f = enumerate(ExteriorModel::Transparent);
        assert_eq!(f.len(), FAMILY_SIZE);
        assert_eq!(BLEND_SPACES.len() * PIXEL_FILTERS.len(), FAMILY_SIZE);
        assert!(f.iter().all(|h| h.quantization == QuantizationModel::Uint8));
        assert!(f.iter().all(|h| h.exterior == ExteriorModel::Transparent));
        let ids: std::collections::BTreeSet<String> = f.iter().map(formation_id).collect();
        assert_eq!(ids.len(), FAMILY_SIZE, "ids must be distinct");
        // The Gaussian family is SMALL and bracketed; a resize chain, a
        // codec model or a free kernel are M9's and must not be here.
        assert!(f.iter().all(
            |h| !matches!(h.pixel_filter, PixelFilter::Gaussian { sigma_px } if sigma_px > 1.0)
        ));
    }

    /// §16.2 / §32 rule 16: the kernel is global. The type carrying it is
    /// the whole-image scene formation, and the family is enumerated per
    /// IMAGE, not per boundary — there is no per-edge variant to construct.
    #[test]
    fn the_exterior_comes_from_the_palette_and_a_mismatch_is_typed() {
        let transparent = oracle_override(LinearRgb::new(0.2, 0.3, 0.4), None);
        let opaque = oracle_override(
            LinearRgb::new(0.2, 0.3, 0.4),
            Some(LinearRgb::new(0.9, 0.9, 0.9)),
        );
        assert!(for_palette(&transparent)
            .iter()
            .all(|f| f.exterior == ExteriorModel::Transparent));
        assert!(for_palette(&opaque)
            .iter()
            .all(|f| f.exterior == ExteriorModel::Opaque));
        for f in for_palette(&transparent) {
            assert!(check_agreement(&f, &transparent).is_ok());
            let e = check_agreement(&f, &opaque).unwrap_err();
            assert_eq!(e.formation, "transparent");
            assert_eq!(e.palette, "opaque");
        }
    }

    /// The blend space is observable only where two opaque paints mix. This
    /// is a fact about the axis, measured in the corpus
    /// (`gt::raster::tests::blending_in_linear_light_and_in_srgb_give_different_bytes`),
    /// and reported here rather than papered over with a tie-break.
    #[test]
    fn the_blend_space_is_not_identifiable_over_a_transparent_exterior() {
        let transparent = oracle_override(LinearRgb::new(0.2, 0.3, 0.4), None);
        let opaque = oracle_override(
            LinearRgb::new(0.2, 0.3, 0.4),
            Some(LinearRgb::new(0.9, 0.9, 0.9)),
        );
        assert!(!blend_space_is_identifiable(&transparent));
        assert!(blend_space_is_identifiable(&opaque));
    }

    /// The width statistic reproduces its own closed forms on synthetic
    /// profiles, in both directions: a box edge gives 2/3 and a Gaussian
    /// gives 2.26σ. Without this the measured table would be a set of
    /// numbers with nothing to check them against (meta-rule M-4).
    #[test]
    fn the_width_statistic_matches_its_closed_form_on_synthetic_edges() {
        // A straight vertical edge of length L in a tall image: alpha
        // depends on the column only.
        //
        // Averaged over SUBPIXEL PHASE, because the statistic is a sum over
        // pixel centres and a single phase is a one-point quadrature of the
        // integral it approximates: with a box filter and the edge exactly
        // on a pixel boundary NO pixel is partial and the sum is zero. That
        // is a fact about sampling, not about the statistic, and averaging
        // over phase is what the corpus does anyway (§27.2 has a phase
        // axis).
        let l = 64.0;
        let phases = 16;
        let mean_width = |profile: &dyn Fn(f64) -> f64| {
            let mut acc = 0.0;
            for k in 0..phases {
                let shift = f64::from(k) / f64::from(phases);
                let mut alpha = Vec::new();
                for _ in 0..(l as usize) {
                    for x in 0..64 {
                        alpha.push(profile(f64::from(x) + 0.5 - 32.0 - shift));
                    }
                }
                acc += transition_width_px(&alpha, l);
            }
            acc / f64::from(phases)
        };

        let w = mean_width(&|d: f64| d.clamp(-0.5, 0.5) + 0.5);
        assert!(
            (w - 2.0 / 3.0).abs() < 0.02,
            "box width {w} against the closed form 0.667"
        );

        for sigma in [0.5f64, 1.0] {
            let w = mean_width(&|d: f64| 0.5 * (1.0 + erf(d / (sigma * std::f64::consts::SQRT_2))));
            let closed = 4.0 * sigma / std::f64::consts::PI.sqrt();
            assert!(
                (w - closed).abs() < 0.05,
                "gaussian σ={sigma}: {w} against the closed form {closed}"
            );
        }
    }

    /// The measured table must SEPARATE the kernels it claims to estimate —
    /// and where it does not, the estimator has to say so. Both directions.
    #[test]
    fn the_kernel_table_separates_what_it_can_and_admits_what_it_cannot() {
        // Each kernel's own measured width picks it, and picks it alone.
        let picked = filters_within_margin(0.748, 2.0);
        assert_eq!(picked, vec![PixelFilter::Box], "{picked:?}");
        assert_eq!(
            filters_within_margin(2.388, 2.0),
            vec![PixelFilter::Gaussian { sigma_px: 1.0 }]
        );
        // Halfway between the triangle and the narrow Gaussian the
        // statistic CANNOT choose, and the estimator returns both instead of
        // pretending. That case exists — it is not a hypothetical.
        let ambiguous = filters_within_margin(1.125, 2.0);
        assert!(
            ambiguous.contains(&PixelFilter::Triangle)
                && ambiguous.contains(&PixelFilter::Gaussian { sigma_px: 0.5 }),
            "{ambiguous:?}"
        );
        assert!(!ambiguous.contains(&PixelFilter::Box));
    }

    /// Abramowitz–Stegun 7.1.26; test-only, to state the closed forms above.
    fn erf(x: f64) -> f64 {
        let s = x.signum();
        let x = x.abs();
        let t = 1.0 / (1.0 + 0.327_591_1 * x);
        let y = 1.0
            - (((((1.061_405_429 * t - 1.453_152_027) * t) + 1.421_413_741) * t - 0.284_496_736)
                * t
                + 0.254_829_592)
                * t
                * (-x * x).exp();
        s * y
    }
}
