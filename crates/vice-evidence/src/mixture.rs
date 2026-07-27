//! Premultiplied Flat2 mixture evidence (spec §10, §22, §1.6).
//!
//! The estimator is §10 verbatim, in premultiplied observation vectors:
//!
//! ```text
//! â_p = clamp[0,1] ( (P_i − P_b)·(P_f − P_b) / ‖P_f − P_b‖² )
//! r_p = P_i − [ â_p P_f + (1 − â_p) P_b ]
//! ```
//!
//! and it is correct for a transparent exterior `P_b = (0,0,0,0)` without a
//! special case, because premultiplication already put the coverage in every
//! component (see `vice-image::observation`).
//!
//! §10 then lists what has to be KEPT beside the estimate, and the list is
//! the reason this struct has the fields it has: the residual VECTOR rather
//! than its norm, the contrast/conditioning, the local gradient, the
//! quantization interval, and spatially correlated residual indicators.
//! Storing only `‖r‖` would throw away the direction that says WHICH
//! hypothesis is wrong.
//!
//! ## Semi-transparent interiors (§1.6)
//!
//! Flat2 v1 does not support an interior fill with a true constant
//! `0 < α < 1`. Such an input must be `unsupported` or stay in a competing
//! model — what it must NOT do is pass as an ordinary two-colour coverage
//! problem, because then a constant 50 % fill becomes "coverage 0.5
//! everywhere" and the geometry that explains it is nonsense.
//!
//! The observable signature is a COHERENT, FLAT region at intermediate
//! alpha. Both halves matter, and the second one is what makes the check
//! survive the subclass that would otherwise break it: at 16 px under a
//! σ = 1 Gaussian an entire small shape sits at intermediate alpha, so an
//! area test alone would fire on a legitimately blurred image. A blurred
//! edge has a gradient everywhere (`|∇α| ≈ 0.4/px` at σ = 1); a genuinely
//! flat semi-transparent fill has only quantization noise. The flatness
//! test is therefore in units of the alpha quantization noise, which is
//! `1/(255·conditioning)` — so a low-contrast pair, where alpha is genuinely
//! uncertain, automatically demands more evidence before crying
//! semi-transparency.

use serde::Serialize;
use vice_image::{dot, mix, norm, sub, ObservationTensor, CHANNELS, TRANSPARENT_EXTERIOR_PREMUL};
use vice_ir::{GlobalFormationHypothesis, PixelFilter};

use crate::formation::{check_agreement, formation_id, FormationMismatch};
use crate::palette::{BackgroundHypothesis, Flat2Hypothesis};
use crate::support::{ObservationSupport, SurrogateRole, SurrogateScore};

/// Coefficients of the mixture stage.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct MixtureConfig {
    /// Smallest `‖P_f − P_b‖` (full-scale units) the mixture will divide by.
    /// Below it the alpha estimate is noise: with a per-channel quantization
    /// σ of `1/(255√12)`, a separation of 0.02 already means ±5 % of a
    /// pixel's coverage.
    pub min_conditioning: f64,
    /// The alpha band that counts as INTERMEDIATE for §1.6.
    pub intermediate_alpha_lo: f64,
    pub intermediate_alpha_hi: f64,
    /// `|∇α|`, in units of the alpha quantization noise, below which a pixel
    /// counts as FLAT.
    pub flat_alpha_gradient_ratio: f64,
    /// Smallest flat intermediate-alpha region, in px, that is reported as a
    /// semi-transparent interior. Measured in both directions on the corpus
    /// by `vice-bench` (`the_semi_transparent_floor_separates_both_ways`).
    pub semi_transparent_min_area_px: u64,
}

pub const MIXTURE_CONFIG_V1: MixtureConfig = MixtureConfig {
    min_conditioning: 0.02,
    intermediate_alpha_lo: 0.08,
    intermediate_alpha_hi: 0.92,
    flat_alpha_gradient_ratio: 3.0,
    semi_transparent_min_area_px: 24,
};

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum MixtureRefusal {
    #[error(
        "hypothesis {hypothesis} under formation {formation} is ill-conditioned: \
         ‖P_f − P_b‖ = {conditioning:.5} below {min:.5}, so the coverage estimate would be \
         dividing by less than the quantization noise"
    )]
    IllConditioned {
        hypothesis: String,
        formation: String,
        conditioning: f64,
        min: f64,
    },
    #[error("{0}")]
    Formation(#[from] FormationMismatch),
    #[error(
        "the observation tensor was built in {tensor} but the formation hypothesis says {formation}"
    )]
    BlendSpaceMismatch {
        tensor: &'static str,
        formation: &'static str,
    },
}

/// A coherent flat region at intermediate alpha: the observable signature of
/// an interior fill with a true constant `0 < α < 1` (§1.6).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SemiTransparentInterior {
    pub largest_region_px: u64,
    pub regions: u64,
    /// Mean alpha over the largest region.
    pub alpha: f64,
    /// Alpha span within the largest region: a genuine constant fill is
    /// flat, and the span says how flat.
    pub alpha_span: f64,
    pub total_flat_intermediate_px: u64,
}

/// Spatially correlated residual indicators (§10).
///
/// Indicators, not a correlation MODEL: the calibrated residual-correlation
/// machinery of §17.1 lives in `vice-bench::correlation` (M3) and is what a
/// production likelihood must use. What is recorded here is the cheap
/// structural evidence a hypothesis generator needs — whether the residual
/// is edge-localized (kernel wrong) or spread over interiors (palette or
/// blend space wrong), and how long its same-sign runs are.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResidualIndicators {
    pub mean_abs_codes: f64,
    pub p95_abs_codes: f64,
    pub max_abs_codes: f64,
    /// Share of total `‖r‖` that lives on partially covered pixels.
    pub mass_on_mixed_pixels: f64,
    /// Mean run length, in px, of same-sign residual projections along the
    /// mixture axis; 1.0 means white noise at this resolution.
    pub mean_signed_run_px: f64,
}

/// Everything §10 asks to be kept for one (palette, formation) pair.
#[derive(Debug, Clone, PartialEq)]
pub struct Flat2Evidence {
    pub hypothesis: Flat2Hypothesis,
    pub formation: GlobalFormationHypothesis,
    width_px: u32,
    height_px: u32,
    /// `â_p`, clamped to `[0,1]`.
    alpha: Vec<f64>,
    /// `r_p`, the VECTOR (§10: not only the norm).
    residual: Vec<[f64; CHANNELS]>,
    /// `‖∇α‖` per pixel, from central differences.
    alpha_gradient: Vec<f64>,
    /// Half-width of the 8-bit quantization cell expressed in ALPHA units:
    /// `‖q‖ / ‖P_f − P_b‖`. The conditioning enters here exactly as §10
    /// says it should.
    alpha_quant_halfwidth: Vec<f64>,
    pub conditioning: f64,
    pub indicators: ResidualIndicators,
    pub semi_transparent_interior: Option<SemiTransparentInterior>,
    pub support: ObservationSupport,
    /// Fit quality as a SURROGATE (§10.2): it orders and prunes hypotheses
    /// and it is not in the units of the final posterior.
    pub fit: SurrogateScore,
}

impl Flat2Evidence {
    pub fn width_px(&self) -> u32 {
        self.width_px
    }
    pub fn height_px(&self) -> u32 {
        self.height_px
    }
    pub fn len(&self) -> usize {
        self.alpha.len()
    }
    pub fn is_empty(&self) -> bool {
        self.alpha.is_empty()
    }
    pub fn alpha(&self, i: usize) -> f64 {
        self.alpha[i]
    }
    pub fn alpha_field(&self) -> &[f64] {
        &self.alpha
    }
    pub fn residual(&self, i: usize) -> [f64; CHANNELS] {
        self.residual[i]
    }
    pub fn alpha_gradient(&self, i: usize) -> f64 {
        self.alpha_gradient[i]
    }
    pub fn alpha_quant_halfwidth(&self, i: usize) -> f64 {
        self.alpha_quant_halfwidth[i]
    }
    pub fn index(&self, x: u32, y: u32) -> usize {
        (y as usize) * (self.width_px as usize) + (x as usize)
    }
    pub fn id(&self) -> String {
        format!("{}@{}", self.hypothesis.id, formation_id(&self.formation))
    }

    /// Local residual scale in ALPHA units, over a 3x3 window: the
    /// model-mismatch part of the corridor's noise budget (§13.1).
    pub fn local_residual_alpha_sigma(&self, i: usize) -> f64 {
        let (w, h) = (self.width_px as i64, self.height_px as i64);
        let x = (i % (w as usize)) as i64;
        let y = (i / (w as usize)) as i64;
        let mut acc = 0.0;
        let mut n = 0.0f64;
        for oy in -1..=1i64 {
            for ox in -1..=1i64 {
                let cx = (x + ox).clamp(0, w - 1);
                let cy = (y + oy).clamp(0, h - 1);
                let j = (cy as usize) * (w as usize) + cx as usize;
                let r = norm(self.residual[j]);
                acc += r * r;
                n += 1.0;
            }
        }
        (acc / n.max(1.0)).sqrt() / self.conditioning
    }
}

fn blend_name(b: vice_ir::BlendSpace) -> &'static str {
    match b {
        vice_ir::BlendSpace::LinearLight => "linear_light",
        vice_ir::BlendSpace::EncodedSrgb => "encoded_srgb",
    }
}

/// §22 `propose_flat2_models`, for one (palette, formation) pair.
pub fn infer_mixture(
    t: &ObservationTensor,
    hypothesis: &Flat2Hypothesis,
    formation: &GlobalFormationHypothesis,
    image_sha256: &str,
    cfg: &MixtureConfig,
) -> Result<Flat2Evidence, MixtureRefusal> {
    check_agreement(formation, hypothesis)?;
    if t.blend_space() != formation.blend_space {
        return Err(MixtureRefusal::BlendSpaceMismatch {
            tensor: blend_name(t.blend_space()),
            formation: blend_name(formation.blend_space),
        });
    }
    let blend = formation.blend_space;
    let pf = vice_image::paint_observation_premul(hypothesis.foreground.center(), blend);
    let pb = match hypothesis.background {
        BackgroundHypothesis::TransparentExterior => TRANSPARENT_EXTERIOR_PREMUL,
        BackgroundHypothesis::OpaqueFace(c) => {
            vice_image::paint_observation_premul(c.center(), blend)
        }
    };
    let d = sub(pf, pb);
    let denom = dot(d, d);
    let conditioning = denom.sqrt();
    if conditioning < cfg.min_conditioning {
        return Err(MixtureRefusal::IllConditioned {
            hypothesis: hypothesis.id.clone(),
            formation: formation_id(formation),
            conditioning,
            min: cfg.min_conditioning,
        });
    }

    let n = t.len();
    let mut alpha = Vec::with_capacity(n);
    let mut residual = Vec::with_capacity(n);
    let mut quant = Vec::with_capacity(n);
    for i in 0..n {
        let pi = t.premul(i);
        let a = (dot(sub(pi, pb), d) / denom).clamp(0.0, 1.0);
        alpha.push(a);
        residual.push(sub(pi, mix(pb, pf, a)));
        quant.push(norm(t.quantization_halfwidth(i)) / conditioning);
    }

    let (w, h) = (t.width_px() as i64, t.height_px() as i64);
    let at = |x: i64, y: i64| -> usize {
        let cx = x.clamp(0, w - 1);
        let cy = y.clamp(0, h - 1);
        (cy as usize) * (w as usize) + (cx as usize)
    };
    let mut gradient = vec![0.0f64; n];
    for y in 0..h {
        for x in 0..w {
            let gx = 0.5 * (alpha[at(x + 1, y)] - alpha[at(x - 1, y)]);
            let gy = 0.5 * (alpha[at(x, y + 1)] - alpha[at(x, y - 1)]);
            gradient[at(x, y)] = gx.hypot(gy);
        }
    }

    let indicators = indicators_of(t, &residual, d, conditioning);
    let semi = detect_semi_transparent(&alpha, &gradient, &quant, w as usize, h as usize, cfg);
    let support = ObservationSupport::whole_image(image_sha256, n);
    // The surrogate that orders hypotheses: a standardized residual mass.
    // Deliberately NOT a log-likelihood over pixels — those pixels are
    // spatially correlated (§17.1, measured in M3: a formation mismatch
    // overcounts independent evidence ninefold) and a number that looked
    // like a likelihood would invite exactly the sum §10.2 forbids.
    let fit = SurrogateScore::new(
        SurrogateRole::HypothesisGeneration,
        indicators.p95_abs_codes,
        support.clone(),
    );

    Ok(Flat2Evidence {
        hypothesis: hypothesis.clone(),
        formation: *formation,
        width_px: t.width_px(),
        height_px: t.height_px(),
        alpha,
        residual,
        alpha_gradient: gradient,
        alpha_quant_halfwidth: quant,
        conditioning,
        indicators,
        semi_transparent_interior: semi,
        support,
        fit,
    })
}

fn indicators_of(
    t: &ObservationTensor,
    residual: &[[f64; CHANNELS]],
    axis: [f64; CHANNELS],
    conditioning: f64,
) -> ResidualIndicators {
    let n = residual.len();
    let mut norms: Vec<f64> = residual.iter().map(|r| norm(*r) * 255.0).collect();
    let total: f64 = norms.iter().sum();
    let mixed_mass: f64 = (0..n)
        .filter(|i| {
            let a = t.alpha(*i);
            a > 1.0 / 255.0 && a < 1.0 - 1.0 / 255.0
        })
        .map(|i| norms[i])
        .sum();
    let mean = if n > 0 { total / n as f64 } else { 0.0 };
    let max = norms.iter().copied().fold(0.0, f64::max);
    let mut sorted = std::mem::take(&mut norms);
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p95 = if sorted.is_empty() {
        0.0
    } else {
        sorted[((sorted.len() as f64 * 0.95) as usize).min(sorted.len() - 1)]
    };

    // Same-sign runs of the residual PROJECTED on the mixture axis: the
    // component that a wrong palette or blend space biases, as opposed to
    // the isotropic part that is quantization.
    let unit = 1.0 / conditioning.max(1e-12);
    let w = t.width_px() as usize;
    let h = t.height_px() as usize;
    let sign = |i: usize| {
        let p = dot(residual[i], axis) * unit;
        if p > 0.0 {
            1i8
        } else if p < 0.0 {
            -1
        } else {
            0
        }
    };
    let mut runs = 0u64;
    let mut cells = 0u64;
    for y in 0..h {
        let mut prev = 0i8;
        for x in 0..w {
            let s = sign(y * w + x);
            if s != prev || x == 0 {
                runs += 1;
            }
            prev = s;
            cells += 1;
        }
    }
    ResidualIndicators {
        mean_abs_codes: mean,
        p95_abs_codes: p95,
        max_abs_codes: max,
        mass_on_mixed_pixels: if total > 0.0 { mixed_mass / total } else { 0.0 },
        mean_signed_run_px: if runs > 0 {
            cells as f64 / runs as f64
        } else {
            0.0
        },
    }
}

/// §1.6: a coherent FLAT region at intermediate alpha.
fn detect_semi_transparent(
    alpha: &[f64],
    gradient: &[f64],
    quant: &[f64],
    w: usize,
    h: usize,
    cfg: &MixtureConfig,
) -> Option<SemiTransparentInterior> {
    let n = alpha.len();
    let flat: Vec<bool> = (0..n)
        .map(|i| {
            alpha[i] > cfg.intermediate_alpha_lo
                && alpha[i] < cfg.intermediate_alpha_hi
                && gradient[i] <= cfg.flat_alpha_gradient_ratio * quant[i].max(1e-9)
        })
        .collect();
    let total: u64 = flat.iter().filter(|v| **v).count() as u64;
    if total == 0 {
        return None;
    }
    let mut seen = vec![false; n];
    let mut best: Option<(u64, f64, f64, f64)> = None;
    let mut regions = 0u64;
    for start in 0..n {
        if !flat[start] || seen[start] {
            continue;
        }
        regions += 1;
        let mut stack = vec![start];
        seen[start] = true;
        let mut area = 0u64;
        let mut sum = 0.0;
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        while let Some(i) = stack.pop() {
            area += 1;
            sum += alpha[i];
            lo = lo.min(alpha[i]);
            hi = hi.max(alpha[i]);
            let (x, y) = (i % w, i / w);
            let push = |j: usize, seen: &mut Vec<bool>, stack: &mut Vec<usize>| {
                if flat[j] && !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            };
            if x > 0 {
                push(i - 1, &mut seen, &mut stack);
            }
            if x + 1 < w {
                push(i + 1, &mut seen, &mut stack);
            }
            if y > 0 {
                push(i - w, &mut seen, &mut stack);
            }
            if y + 1 < h {
                push(i + w, &mut seen, &mut stack);
            }
        }
        let mean = sum / area as f64;
        if best.is_none_or(|(a, _, _, _)| area > a) {
            best = Some((area, mean, lo, hi));
        }
    }
    let (area, mean, lo, hi) = best?;
    if area < cfg.semi_transparent_min_area_px {
        return None;
    }
    Some(SemiTransparentInterior {
        largest_region_px: area,
        regions,
        alpha: mean,
        alpha_span: hi - lo,
        total_flat_intermediate_px: total,
    })
}

/// The kernel-independent part of the formation score: how much of the
/// residual the hypothesis leaves. See [`crate::formation::filter_penalty`]
/// for the part that depends on the kernel.
pub fn residual_penalty(e: &Flat2Evidence) -> f64 {
    e.indicators.p95_abs_codes
}

/// The pixel filter of a formation, as a plain id (for report keys).
pub fn filter_id(f: PixelFilter) -> String {
    match f {
        PixelFilter::Box => "box".to_string(),
        PixelFilter::Triangle => "triangle".to_string(),
        PixelFilter::Gaussian { sigma_px } => format!("gauss{sigma_px:.2}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::oracle_override;
    use vice_image::{CanonicalImage, IccAssumption};
    use vice_ir::color::linear_to_srgb_encoded;
    use vice_ir::{BlendSpace, ExteriorModel, LinearRgb, QuantizationModel};

    fn enc(v: f64) -> u8 {
        (linear_to_srgb_encoded(v.clamp(0.0, 1.0)) * 255.0).round() as u8
    }

    fn formation(blend: BlendSpace, ext: ExteriorModel) -> GlobalFormationHypothesis {
        GlobalFormationHypothesis {
            blend_space: blend,
            pixel_filter: PixelFilter::Box,
            quantization: QuantizationModel::Uint8,
            exterior: ext,
        }
    }

    fn image(w: u32, h: u32, px: Vec<u8>) -> CanonicalImage {
        CanonicalImage::from_straight_srgb8(w, h, px, true, IccAssumption::NoProfileAssumedSrgb)
            .unwrap()
    }

    /// The estimator recovers the coverage that produced the pixels, over a
    /// TRANSPARENT exterior, with `P_b = (0,0,0,0)` and no special case
    /// (§10). This is the transparent-exterior half of the §28 M4 gate.
    #[test]
    fn coverage_is_recovered_exactly_over_a_transparent_exterior() {
        let ink = LinearRgb::new(0.05, 0.35, 0.8);
        let coverages: Vec<f64> = (0..=10).map(|i| f64::from(i) / 10.0).collect();
        let mut px = Vec::new();
        for a in &coverages {
            px.extend_from_slice(&[
                enc(ink.r),
                enc(ink.g),
                enc(ink.b),
                (a * 255.0).round() as u8,
            ]);
        }
        let img = image(coverages.len() as u32, 1, px);
        let h = oracle_override(ink, None);
        for blend in [BlendSpace::LinearLight, BlendSpace::EncodedSrgb] {
            let t = ObservationTensor::of(&img, blend);
            let e = infer_mixture(
                &t,
                &h,
                &formation(blend, ExteriorModel::Transparent),
                "img",
                &MIXTURE_CONFIG_V1,
            )
            .expect("well conditioned");
            for (i, want) in coverages.iter().enumerate() {
                assert!(
                    (e.alpha(i) - want).abs() < 2.0 / 255.0,
                    "{blend:?} α[{i}] = {} want {want}",
                    e.alpha(i)
                );
                assert!(norm(e.residual(i)) * 255.0 < 2.0, "residual at {i}");
            }
            // And the RGB under α = 0 contributed nothing: the pixel that
            // stores a full-strength ink colour next to zero alpha is
            // explained exactly (§1.6).
            assert_eq!(e.alpha(0), 0.0);
            assert!(norm(e.residual(0)) == 0.0);
        }
    }

    /// The label swap is an exact symmetry of the mixture: `α ↔ 1−α` with
    /// the SAME residual. §9.2 asks for the swapped hypothesis precisely
    /// because it is a relabeling and not a rival, and a selector that
    /// preferred one would be inventing evidence.
    #[test]
    fn swapping_the_two_faces_maps_alpha_to_its_complement_and_keeps_the_residual() {
        let a = LinearRgb::new(0.9, 0.15, 0.15);
        let b = LinearRgb::new(0.08, 0.08, 0.24);
        let mut px = Vec::new();
        for i in 0..16 {
            let cov = f64::from(i) / 15.0;
            let mixed = LinearRgb::new(
                b.r + cov * (a.r - b.r),
                b.g + cov * (a.g - b.g),
                b.b + cov * (a.b - b.b),
            );
            px.extend_from_slice(&[enc(mixed.r), enc(mixed.g), enc(mixed.b), 255]);
        }
        let img = image(16, 1, px);
        let t = ObservationTensor::of(&img, BlendSpace::LinearLight);
        let f = formation(BlendSpace::LinearLight, ExteriorModel::Opaque);
        let fwd = infer_mixture(
            &t,
            &oracle_override(a, Some(b)),
            &f,
            "img",
            &MIXTURE_CONFIG_V1,
        )
        .unwrap();
        let rev = infer_mixture(
            &t,
            &oracle_override(b, Some(a)),
            &f,
            "img",
            &MIXTURE_CONFIG_V1,
        )
        .unwrap();
        for i in 0..16 {
            assert!(
                (fwd.alpha(i) + rev.alpha(i) - 1.0).abs() < 1e-9,
                "{i}: {} + {}",
                fwd.alpha(i),
                rev.alpha(i)
            );
            assert!((norm(fwd.residual(i)) - norm(rev.residual(i))).abs() < 1e-12);
        }
        assert!((fwd.conditioning - rev.conditioning).abs() < 1e-12);
    }

    /// A pair of paints too close to tell apart is a typed refusal, not a
    /// noisy answer. The conditioning IS the contrast §10 asks to keep.
    #[test]
    fn an_ill_conditioned_pair_is_refused_by_name() {
        let img = image(4, 1, vec![128; 16]);
        let t = ObservationTensor::of(&img, BlendSpace::LinearLight);
        let h = oracle_override(
            LinearRgb::new(0.5, 0.5, 0.5),
            Some(LinearRgb::new(0.5, 0.5, 0.5005)),
        );
        match infer_mixture(
            &t,
            &h,
            &formation(BlendSpace::LinearLight, ExteriorModel::Opaque),
            "img",
            &MIXTURE_CONFIG_V1,
        ) {
            Err(MixtureRefusal::IllConditioned { conditioning, .. }) => {
                assert!(conditioning < MIXTURE_CONFIG_V1.min_conditioning)
            }
            other => panic!("{other:?}"),
        }
    }

    /// A formation whose exterior contradicts the palette cannot be paired
    /// with it, and a tensor built in the wrong blend space is refused too:
    /// both are ways for the two halves of the model to disagree silently.
    #[test]
    fn a_formation_that_contradicts_its_palette_is_refused() {
        let img = image(4, 1, vec![200; 16]);
        let t = ObservationTensor::of(&img, BlendSpace::LinearLight);
        let opaque = oracle_override(
            LinearRgb::new(0.1, 0.1, 0.1),
            Some(LinearRgb::new(0.9, 0.9, 0.9)),
        );
        assert!(matches!(
            infer_mixture(
                &t,
                &opaque,
                &formation(BlendSpace::LinearLight, ExteriorModel::Transparent),
                "img",
                &MIXTURE_CONFIG_V1
            ),
            Err(MixtureRefusal::Formation(_))
        ));
        assert!(matches!(
            infer_mixture(
                &t,
                &opaque,
                &formation(BlendSpace::EncodedSrgb, ExteriorModel::Opaque),
                "img",
                &MIXTURE_CONFIG_V1
            ),
            Err(MixtureRefusal::BlendSpaceMismatch { .. })
        ));
    }

    /// §1.6, both directions. A constant 50 % fill over a transparent
    /// exterior is DETECTED as a semi-transparent interior; a fully covered
    /// shape with an antialiased rim is not — and neither is a small shape
    /// that is entirely intermediate because it was blurred, which is the
    /// subclass an area-only test would fail on.
    #[test]
    fn a_flat_intermediate_region_is_detected_and_a_blurred_edge_is_not() {
        let ink = LinearRgb::new(0.2, 0.5, 0.9);
        let side = 24u32;
        let build = |f: &dyn Fn(u32, u32) -> u8| {
            let mut px = Vec::new();
            for y in 0..side {
                for x in 0..side {
                    px.extend_from_slice(&[enc(ink.r), enc(ink.g), enc(ink.b), f(x, y)]);
                }
            }
            image(side, side, px)
        };
        let h = oracle_override(ink, None);
        let f = formation(BlendSpace::LinearLight, ExteriorModel::Transparent);
        let run = |img: &CanonicalImage| {
            let t = ObservationTensor::of(img, BlendSpace::LinearLight);
            infer_mixture(&t, &h, &f, "img", &MIXTURE_CONFIG_V1).unwrap()
        };

        // (a) a constant half-covered square: FLAT and intermediate.
        let semi = build(&|x, y| {
            if (4..20).contains(&x) && (4..20).contains(&y) {
                128
            } else {
                0
            }
        });
        let d = run(&semi)
            .semi_transparent_interior
            .expect("a constant 0.5 fill must be detected");
        assert!(d.largest_region_px >= 196, "{d:?}");
        assert!((d.alpha - 0.502).abs() < 0.01, "{d:?}");
        assert!(d.alpha_span < 0.01, "a constant fill is flat: {d:?}");

        // (b) an opaque square with a one-pixel antialiased rim: the
        // intermediate pixels exist but are not flat.
        let aa = build(&|x, y| {
            let inside = (5..19).contains(&x) && (5..19).contains(&y);
            let rim = (4..20).contains(&x) && (4..20).contains(&y);
            if inside {
                255
            } else if rim {
                128
            } else {
                0
            }
        });
        assert!(
            run(&aa).semi_transparent_interior.is_none(),
            "an antialiased rim is not a semi-transparent interior"
        );

        // (c) the subclass that breaks an area-only test: a SMALL shape so
        // blurred that every one of its pixels is intermediate. It has a
        // gradient everywhere, so it must not be detected either.
        let blurred = build(&|x, y| {
            let d = ((f64::from(x) - 11.5).powi(2) + (f64::from(y) - 11.5).powi(2)).sqrt();
            let v = (1.0 - (d - 2.0) / 6.0).clamp(0.0, 1.0);
            (v * 255.0).round() as u8
        });
        let got = run(&blurred).semi_transparent_interior;
        assert!(
            got.is_none(),
            "a blurred blob is intermediate everywhere but not flat: {got:?}"
        );
    }

    /// The residual indicators point at WHICH hypothesis is wrong: a wrong
    /// blend space leaves its residual where two paints mix, and a residual
    /// vector rather than a norm is what makes that visible.
    #[test]
    fn the_indicators_localize_a_blend_space_error_on_the_mixed_pixels() {
        // NOT two greys: any monotone transfer maps the grey diagonal onto
        // itself, so a grey pair lies on one segment in BOTH blend spaces
        // and the wrong space leaves no residual at all. Found by this test
        // failing on its first fixture, which is the subclass meta-rule M-2
        // is about.
        let a = LinearRgb::new(0.95, 0.10, 0.10);
        let b = LinearRgb::new(0.03, 0.03, 0.85);
        // Composited in ENCODED space, as `composite_rgba8` does under
        // `EncodedSrgb`: the stored value is the encoded average.
        let mut px = Vec::new();
        for i in 0..32 {
            let cov = f64::from(i) / 31.0;
            let e = |x: f64, y: f64| {
                ((linear_to_srgb_encoded(y)
                    + cov * (linear_to_srgb_encoded(x) - linear_to_srgb_encoded(y)))
                    * 255.0)
                    .round() as u8
            };
            px.extend_from_slice(&[e(a.r, b.r), e(a.g, b.g), e(a.b, b.b), 255]);
        }
        let img = image(32, 1, px);
        let h = oracle_override(a, Some(b));
        let right = infer_mixture(
            &ObservationTensor::of(&img, BlendSpace::EncodedSrgb),
            &h,
            &formation(BlendSpace::EncodedSrgb, ExteriorModel::Opaque),
            "img",
            &MIXTURE_CONFIG_V1,
        )
        .unwrap();
        let wrong = infer_mixture(
            &ObservationTensor::of(&img, BlendSpace::LinearLight),
            &h,
            &formation(BlendSpace::LinearLight, ExteriorModel::Opaque),
            "img",
            &MIXTURE_CONFIG_V1,
        )
        .unwrap();
        println!(
            "right p95 {:.3} codes, wrong p95 {:.3} codes",
            right.indicators.p95_abs_codes, wrong.indicators.p95_abs_codes
        );
        assert!(
            right.indicators.p95_abs_codes < 1.0,
            "the correct blend space must explain the pixels: {:?}",
            right.indicators
        );
        assert!(
            wrong.indicators.p95_abs_codes > 5.0 * right.indicators.p95_abs_codes.max(0.2),
            "the wrong blend space must leave a residual: {:?}",
            wrong.indicators
        );
    }

    /// The evidence carries its support, so the §10.2 mechanism applies to
    /// it: two hypotheses over one image cannot have their fits added.
    #[test]
    fn two_evidences_over_one_image_cannot_have_their_fits_added() {
        let ink = LinearRgb::new(0.3, 0.3, 0.3);
        let img = image(8, 1, {
            let mut v = Vec::new();
            for i in 0..8 {
                v.extend_from_slice(&[enc(ink.r), enc(ink.g), enc(ink.b), (i * 32) as u8]);
            }
            v
        });
        let t = ObservationTensor::of(&img, BlendSpace::LinearLight);
        let f = formation(BlendSpace::LinearLight, ExteriorModel::Transparent);
        let e1 = infer_mixture(
            &t,
            &oracle_override(ink, None),
            &f,
            "img",
            &MIXTURE_CONFIG_V1,
        )
        .unwrap();
        let e2 = infer_mixture(
            &t,
            &oracle_override(LinearRgb::new(0.31, 0.3, 0.3), None),
            &f,
            "img",
            &MIXTURE_CONFIG_V1,
        )
        .unwrap();
        assert!(e1.fit.add_disjoint(&e2.fit).is_err());
        assert_eq!(e1.fit.role(), SurrogateRole::HypothesisGeneration);
        assert!(residual_penalty(&e1) >= 0.0);
        assert_eq!(
            filter_id(PixelFilter::Gaussian { sigma_px: 0.5 }),
            "gauss0.50"
        );
    }
}
