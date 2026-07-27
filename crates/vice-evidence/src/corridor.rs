//! The corridor: how far the true boundary can be from the extracted one
//! (spec §13, §13.1, §10).
//!
//! §10 is explicit that this must not be a hand-tuned formula: *"uncertainty
//! is derived from a calibrated formation/noise model, not simply from a
//! hand-tuned formula. A formula-based corridor is admissible as an
//! INITIALIZATION, but its coefficients freeze on the dev set and are
//! checked by calibration gates."* So the halfwidth here is derived, and
//! every term of the derivation is a quantity the evidence already carries.
//!
//! ## The derivation
//!
//! The extracted boundary is the `α = 0.5` level set. Displace a point along
//! the boundary normal by `d`; the coverage changes by `|∂α/∂n|·d`. So an
//! uncertainty `σ_α` in the coverage estimate is an uncertainty
//!
//! ```text
//! σ_pos = σ_α / |∂α/∂n|
//! ```
//!
//! in the position, and the halfwidth of a `q`-corridor is `z_q · σ_pos`.
//! Nothing is fitted to a coverage target; `z_q` is the standard normal
//! quantile, which is what makes the measured coverage a TEST of the model
//! rather than a restatement of it. If the measured coverage@95 comes out
//! below 95 %, that is a model mismatch to report — §13.1 says a wide
//! corridor does not turn a failure into a success, and the same holds for
//! a corridor widened until it passes.
//!
//! `σ_α` has three terms and each is measured, not chosen:
//!
//! | term | where it comes from |
//! |---|---|
//! | quantization | the 8-bit cell the tensor carries, divided by the mixture conditioning, as a uniform distribution (`h/√3`) |
//! | model mismatch | the LOCAL residual of the mixture, in alpha units |
//! | formation disagreement | the frozen clean-bucket coverage noise of `configs/GATES_V1.toml`, in the COVERAGE domain and therefore not divided by anything, and measured WITHOUT the held-out engine |
//!
//! The conditioning appears in the first term exactly as §10 asks: a
//! low-contrast pair divides by a smaller separation, so its corridor is
//! wider, without anyone writing a special case for low contrast.
//!
//! ## The extraction term, and why it is added rather than squared
//!
//! Noise is not the only thing between the extracted boundary and the true
//! one. The `α = 0.5` crossing is found by LINEAR interpolation of a field
//! that is not linear, and the error of linear interpolation on a smooth
//! function is `|f''|·h²/8` — a BIAS, present with the same sign whatever
//! the noise does. Its size is read off the field itself, as the second
//! difference of the coverage along the normal at unit spacing, so nothing
//! is fitted:
//!
//! ```text
//! halfwidth = z_q · σ_α/|∂α/∂n|  +  |∂²α/∂n²| / (8·|∂α/∂n|)
//! ```
//!
//! A systematic term is ADDED, not combined in quadrature: quadrature is
//! for independent random contributions, and this one is not random. The
//! first draft of this module had only the noise term, and the corpus said
//! so — measured coverage@95 was 0.46 against an expected 0.95, with the
//! misses concentrated exactly where the coverage profile is curved
//! (FAILURE_LEDGER F-0023).
//!
//! For the same reason `∂α/∂n` is measured over a UNIT baseline centred on
//! the sample (`α(p+n/2) − α(p−n/2)`) rather than by a central difference
//! of the pixel grid: the latter spans two pixels and halves the gradient
//! of a one-pixel-wide edge, which would have widened every corridor by 2x
//! for a reason that is an artifact of the stencil.
//!
//! ## The cap
//!
//! A halfwidth is capped at [`CorridorConfig::max_halfwidth_px`]. The cap
//! can only make the corridor NARROWER, so it can only LOWER the measured
//! coverage — it is conservative in the direction that matters, and the
//! share of capped samples is reported so a reader can see when it binds.

use serde::Serialize;

use crate::mixture::Flat2Evidence;

/// The clean-bucket observation noise scale, in 8-bit codes OF COVERAGE.
///
/// Frozen in `configs/GATES_V1.toml` `[noise_scales]`, measured by
/// `vice-bench::corridor::tests::the_clean_bucket_noise_scale_is_measured_on_the_development_split`
/// on the population `corridor::frozen_calibration_groups` defines:
/// development groups, and only the profiles the split policy allows there.
///
/// **25.57 codes, from `raqote` alone, over 11481 edge pixels.** The first
/// version of this constant was 18.8, pooled with `tiny-skia` (9.54) — and
/// `tiny-skia` is the HELD-OUT engine, which the frozen split policy keeps
/// out of development and which the §28 M4 clause exists to generalize TO
/// (REVIEW_M4 M4-N2, FAILURE_LEDGER F-0027). Calibrating on it made the
/// clause a test of memory rather than of generalization.
///
/// The correction makes the corridor WIDER, not narrower: 18.8 was a
/// stricter number than the legal procedure yields, so the published
/// coverage was never bought with width.
///
/// What it is the scale OF, because getting that wrong is what made the
/// first corridor two to three times too narrow (FAILURE_LEDGER F-0023):
/// the dominant observation noise of the clean bucket is NOT quantization
/// and NOT the mixture residual. It is the disagreement between the
/// analytic coverage the M4 formation family predicts and the antialiasing
/// an independent engine actually performed — a quantity the two-colour
/// mixture cannot see at all, because the mixture fits ANY coverage value
/// and leaves no residual when the coverage itself is wrong.
///
/// It is in the COVERAGE domain, so unlike the quantization term it is NOT
/// divided by the mixture conditioning: an engine that puts 0.07 of extra
/// coverage in a pixel has moved alpha by 0.07 whatever the two paints are.
///
/// It comes from the coarsest engine the calibration is allowed to see. At
/// inference time nothing says which engine drew the image, so the corridor
/// has to hold for the coarse one; that makes it conservative for a finer
/// one, and the per-engine numbers are published so a reader can see by how
/// much.
pub const CLEAN_BUCKET_SIGMA_CODES: f64 = 25.57;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct CorridorConfig {
    /// Largest halfwidth reported, in px. Narrowing only.
    pub max_halfwidth_px: f64,
    /// Coverage-domain noise scale in 8-bit codes; see
    /// [`CLEAN_BUCKET_SIGMA_CODES`].
    pub coverage_noise_codes: f64,
}

pub const CORRIDOR_CONFIG_V1: CorridorConfig = CorridorConfig {
    max_halfwidth_px: 4.0,
    coverage_noise_codes: CLEAN_BUCKET_SIGMA_CODES,
};

/// The two-sided standard normal quantile of a coverage level.
///
/// Tabulated rather than computed so the numbers a report quotes are the
/// ones a reader can look up. `coverage@95` means `z = 1.95996…`.
pub fn z_for_coverage(q: f64) -> Option<f64> {
    let table = [
        (0.50, 0.674_489_750_196_082),
        (0.90, 1.644_853_626_951_472),
        (0.95, 1.959_963_984_540_054),
        (0.99, 2.575_829_303_548_901),
    ];
    table
        .iter()
        .find(|(p, _)| (p - q).abs() < 1e-9)
        .map(|(_, z)| *z)
}

/// The coverage levels §13.1 asks to be reported.
pub const COVERAGE_LEVELS: &[f64] = &[0.50, 0.90, 0.95, 0.99];

/// The alpha-domain uncertainty at one pixel, with its three terms kept
/// separate so a report can say WHICH one dominates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct AlphaSigma {
    pub quantization: f64,
    pub model_mismatch: f64,
    /// The coverage the observing engine and the M4 formation family
    /// disagree about. Dominant in the clean bucket, and invisible to the
    /// mixture residual.
    pub formation_disagreement: f64,
    pub total: f64,
}

impl AlphaSigma {
    pub fn at(e: &Flat2Evidence, i: usize, cfg: &CorridorConfig) -> AlphaSigma {
        // A quantization cell is a UNIFORM distribution of half-width h, so
        // its standard deviation is h/√3 — not h, which would inflate every
        // corridor by 73 %.
        let quantization = e.alpha_quant_halfwidth(i) / 3.0f64.sqrt();
        let model_mismatch = e.local_residual_alpha_sigma(i);
        // NOT divided by the conditioning: this term is already a coverage
        // error, not a channel error (see CLEAN_BUCKET_SIGMA_CODES).
        let formation_disagreement = cfg.coverage_noise_codes / 255.0;
        let total = (quantization * quantization
            + model_mismatch * model_mismatch
            + formation_disagreement * formation_disagreement)
            .sqrt();
        AlphaSigma {
            quantization,
            model_mismatch,
            formation_disagreement,
            total,
        }
    }
}

/// One corridor evaluation at one point on the boundary.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Corridor {
    pub sigma_alpha: AlphaSigma,
    /// `∂α/∂n` at the point, per px, over a unit baseline.
    pub gradient_along_normal: f64,
    /// `∂²α/∂n²` at the point, per px², over a unit baseline.
    pub curvature_along_normal: f64,
    /// `σ_α / |∂α/∂n|`: the RANDOM part, before the quantile.
    pub sigma_pos_px: f64,
    /// `|∂²α/∂n²| / (8|∂α/∂n|)`: the SYSTEMATIC part, the error of finding
    /// a level set by linear interpolation of a curved profile.
    pub extraction_bias_px: f64,
    /// `z_q · σ_pos + extraction_bias`, capped.
    pub halfwidth_px: f64,
    pub capped: bool,
    /// The coverage level this halfwidth belongs to.
    pub level: f64,
}

/// The corridor at one pixel, for one coverage level.
pub fn corridor_at(
    e: &Flat2Evidence,
    i: usize,
    gradient_along_normal: f64,
    curvature_along_normal: f64,
    level: f64,
    cfg: &CorridorConfig,
) -> Option<Corridor> {
    let z = z_for_coverage(level)?;
    let sigma_alpha = AlphaSigma::at(e, i, cfg);
    let g = gradient_along_normal.abs().max(1e-9);
    let sigma_pos_px = sigma_alpha.total / g;
    let extraction_bias_px = curvature_along_normal.abs() / (8.0 * g);
    let raw = z * sigma_pos_px + extraction_bias_px;
    let halfwidth_px = raw.min(cfg.max_halfwidth_px);
    Some(Corridor {
        sigma_alpha,
        gradient_along_normal,
        curvature_along_normal,
        sigma_pos_px,
        extraction_bias_px,
        halfwidth_px,
        capped: raw > cfg.max_halfwidth_px,
        level,
    })
}

/// Confidence of one boundary sample, in `[0,1]`.
///
/// NOT a probability and NOT the calibrated confidence of §1.5 — that one is
/// bound to a frozen risk–coverage calibration this project cannot yet
/// perform. This is the monotone weight §13 asks a `BoundarySample` to
/// carry: how sharply the coverage field determines the position here,
/// relative to the corridor cap.
pub fn sample_confidence(c: &Corridor, cfg: &CorridorConfig) -> f64 {
    (1.0 - (c.sigma_pos_px + c.extraction_bias_px) / cfg.max_halfwidth_px).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mixture::{infer_mixture, MIXTURE_CONFIG_V1};
    use crate::palette::oracle_override;
    use vice_image::{CanonicalImage, IccAssumption, ObservationTensor};
    use vice_ir::color::linear_to_srgb_encoded;
    use vice_ir::{
        BlendSpace, ExteriorModel, GlobalFormationHypothesis, LinearRgb, PixelFilter,
        QuantizationModel,
    };

    fn enc(v: f64) -> u8 {
        (linear_to_srgb_encoded(v.clamp(0.0, 1.0)) * 255.0).round() as u8
    }

    /// A horizontal ramp of coverage: alpha rises linearly over `width` px,
    /// so `|∂α/∂n| = 1/width` exactly and the corridor has a closed form.
    fn ramp(ink: LinearRgb, width: f64, bg: Option<LinearRgb>) -> Flat2Evidence {
        let n = 32u32;
        let mut px = Vec::new();
        for _ in 0..4 {
            for x in 0..n {
                let a = (((f64::from(x) + 0.5) - 16.0) / width + 0.5).clamp(0.0, 1.0);
                match bg {
                    None => px.extend_from_slice(&[
                        enc(ink.r),
                        enc(ink.g),
                        enc(ink.b),
                        (a * 255.0).round() as u8,
                    ]),
                    Some(b) => {
                        let m = |x: f64, y: f64| enc(y + a * (x - y));
                        px.extend_from_slice(&[m(ink.r, b.r), m(ink.g, b.g), m(ink.b, b.b), 255]);
                    }
                }
            }
        }
        let img = CanonicalImage::from_straight_srgb8(
            n,
            4,
            px,
            true,
            IccAssumption::NoProfileAssumedSrgb,
        )
        .unwrap();
        let t = ObservationTensor::of(&img, BlendSpace::LinearLight);
        let h = oracle_override(ink, bg);
        let f = GlobalFormationHypothesis {
            blend_space: BlendSpace::LinearLight,
            pixel_filter: PixelFilter::Box,
            quantization: QuantizationModel::Uint8,
            exterior: if bg.is_some() {
                ExteriorModel::Opaque
            } else {
                ExteriorModel::Transparent
            },
        };
        infer_mixture(&t, &h, &f, "img", &MIXTURE_CONFIG_V1).unwrap()
    }

    /// The corridor is `z·σ_α/|∂α/∂n|`, and it is WIDER exactly where the
    /// coverage field is less able to locate the boundary: a blurrier edge
    /// has a smaller gradient, so the same alpha noise buys less position.
    #[test]
    fn a_blurrier_edge_has_a_proportionally_wider_corridor() {
        let ink = LinearRgb::new(0.1, 0.4, 0.8);
        let sharp = ramp(ink, 1.0, None);
        let blurry = ramp(ink, 4.0, None);
        let i = sharp.index(16, 2);
        let a = corridor_at(&sharp, i, 1.0, 0.0, 0.95, &CORRIDOR_CONFIG_V1).unwrap();
        let b = corridor_at(&blurry, i, 0.25, 0.0, 0.95, &CORRIDOR_CONFIG_V1).unwrap();
        println!(
            "sharp {:.4} px, blurry {:.4} px (σ_α {:.5} vs {:.5})",
            a.halfwidth_px, b.halfwidth_px, a.sigma_alpha.total, b.sigma_alpha.total
        );
        assert!(
            b.halfwidth_px > 3.0 * a.halfwidth_px,
            "{} vs {}",
            a.halfwidth_px,
            b.halfwidth_px
        );
        // And the sharp corridor is well under a pixel: a clean AA edge
        // locates the boundary to a fraction of the pixel it lives in.
        assert!(a.halfwidth_px < 0.35, "{}", a.halfwidth_px);
    }

    /// The conditioning enters through the quantization term, so a
    /// low-contrast pair gets a wider corridor without a special case for
    /// low contrast anywhere in the code (§10).
    #[test]
    fn low_contrast_enters_the_corridor_through_the_conditioning() {
        let high = ramp(
            LinearRgb::new(0.95, 0.95, 0.95),
            1.0,
            Some(LinearRgb::new(0.02, 0.02, 0.02)),
        );
        let low = ramp(
            LinearRgb::new(0.52, 0.52, 0.52),
            1.0,
            Some(LinearRgb::new(0.48, 0.48, 0.48)),
        );
        let i = high.index(16, 2);
        let a = corridor_at(&high, i, 1.0, 0.0, 0.95, &CORRIDOR_CONFIG_V1).unwrap();
        let b = corridor_at(&low, i, 1.0, 0.0, 0.95, &CORRIDOR_CONFIG_V1).unwrap();
        println!(
            "conditioning {:.4} -> {:.4} px (q {:.5}); {:.4} -> {:.4} px (q {:.5})",
            high.conditioning,
            a.halfwidth_px,
            a.sigma_alpha.quantization,
            low.conditioning,
            b.halfwidth_px,
            b.sigma_alpha.quantization
        );
        assert!(low.conditioning < high.conditioning / 5.0);
        assert!(
            b.sigma_alpha.quantization > 4.0 * a.sigma_alpha.quantization,
            "the conditioning must enter the quantization term"
        );
        assert!(b.halfwidth_px > a.halfwidth_px, "and widen the corridor");
        // The measured fact worth recording rather than assuming the
        // opposite: in the CLEAN bucket the formation-disagreement term
        // DOMINATES the quantization one, so contrast moves the total only
        // as the pair approaches the conditioning floor. A corridor built on
        // quantization alone would have been two to three times too narrow
        // (F-0023), and this assertion is what keeps that in view.
        assert!(a.sigma_alpha.formation_disagreement > 4.0 * a.sigma_alpha.quantization);
    }

    /// The cap only ever NARROWS: it cannot rescue a failure by widening,
    /// and the flag says when it bound.
    #[test]
    fn the_cap_narrows_and_says_so() {
        let e = ramp(LinearRgb::new(0.1, 0.4, 0.8), 1.0, None);
        let i = e.index(16, 2);
        let flat = corridor_at(&e, i, 1e-6, 0.0, 0.99, &CORRIDOR_CONFIG_V1).unwrap();
        assert!(flat.capped);
        assert_eq!(flat.halfwidth_px, CORRIDOR_CONFIG_V1.max_halfwidth_px);
        assert!(flat.sigma_pos_px > CORRIDOR_CONFIG_V1.max_halfwidth_px);
        assert_eq!(sample_confidence(&flat, &CORRIDOR_CONFIG_V1), 0.0);
        let sharp = corridor_at(&e, i, 1.0, 0.0, 0.50, &CORRIDOR_CONFIG_V1).unwrap();
        assert!(!sharp.capped);
        assert!(sample_confidence(&sharp, &CORRIDOR_CONFIG_V1) > 0.9);
    }

    /// The quantile is the standard normal one, and the corridor is
    /// monotone in it. If these were fitted to hit a coverage target the
    /// measured coverage would stop being a test of the model.
    #[test]
    fn the_quantiles_are_the_standard_normal_ones_and_the_corridor_is_monotone() {
        assert!((z_for_coverage(0.95).unwrap() - 1.959_963_984_540_054).abs() < 1e-12);
        assert!(z_for_coverage(0.80).is_none(), "no invented levels");
        let e = ramp(LinearRgb::new(0.1, 0.4, 0.8), 1.0, None);
        let i = e.index(16, 2);
        let mut prev = 0.0;
        for level in COVERAGE_LEVELS {
            let c = corridor_at(&e, i, 1.0, 0.0, *level, &CORRIDOR_CONFIG_V1).unwrap();
            assert!(c.halfwidth_px > prev, "{level}: {}", c.halfwidth_px);
            prev = c.halfwidth_px;
        }
    }

    /// The three terms of σ_α are kept apart, and the formation-disagreement
    /// term really is a floor: it cannot be undercut by a locally clean
    /// patch, which is the situation it exists for — the mixture residual
    /// vanishes exactly when the coverage error is invisible to it.
    #[test]
    fn the_formation_term_is_a_floor_and_the_terms_stay_separable() {
        let e = ramp(LinearRgb::new(0.1, 0.4, 0.8), 1.0, None);
        let s = AlphaSigma::at(&e, e.index(2, 2), &CORRIDOR_CONFIG_V1);
        assert!(s.total >= s.formation_disagreement);
        assert!(s.formation_disagreement > 0.0);
        assert!(s.quantization > 0.0);
        let no_floor = AlphaSigma::at(
            &e,
            e.index(2, 2),
            &CorridorConfig {
                coverage_noise_codes: 0.0,
                ..CORRIDOR_CONFIG_V1
            },
        );
        assert!(no_floor.total < s.total, "the floor must bind");
    }
}
