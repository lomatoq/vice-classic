//! The §1.6 exclusion, as a detector.
//!
//! Flat2 v1 does not support an interior fill with a true constant
//! `0 < α < 1`. What makes that clause enforceable rather than aspirational
//! is that such a fill has an OBSERVABLE signature: a coherent, FLAT region
//! at intermediate alpha.
//!
//! Both halves matter, and the second one is what makes the check survive
//! the subclass that would otherwise break it. At 16 px under a σ = 1
//! Gaussian an entire small shape sits at intermediate alpha, so an
//! area-only test would convict a legitimately blurred image. A blurred edge
//! has a gradient everywhere (`|∇α| ≈ 0.4/px` at σ = 1); a genuinely flat
//! semi-transparent fill has only quantization noise. Flatness is therefore
//! measured in units of the ALPHA quantization noise — which is
//! `‖q‖/‖P_f − P_b‖`, so a low-contrast pair, where alpha is genuinely
//! uncertain, automatically demands more evidence before this fires.
//!
//! Split from the mixture module at the seam §4.1's size rule asks for: the
//! mixture answers "what coverage explains this pixel", this module answers
//! "is the answer outside the supported model".

use serde::Serialize;

use super::MixtureConfig;

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

/// §1.6: a coherent FLAT region at intermediate alpha.
pub(crate) fn detect_semi_transparent(
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

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> MixtureConfig {
        super::super::MIXTURE_CONFIG_V1
    }

    /// A region below the floor is not reported at all: the detector says
    /// "semi-transparent interior", and three pixels are not an interior.
    #[test]
    fn a_region_under_the_floor_is_not_an_interior() {
        let (w, h) = (8usize, 8usize);
        let mut alpha = vec![0.0f64; w * h];
        let mut grad = vec![0.0f64; w * h];
        let quant = vec![0.004f64; w * h];
        for i in [9usize, 10, 17] {
            alpha[i] = 0.5;
        }
        assert!(detect_semi_transparent(&alpha, &grad, &quant, w, h, &cfg()).is_none());
        // The same field with a region above the floor IS reported.
        for j in 0..h {
            for i in 0..w {
                alpha[j * w + i] = 0.5;
                grad[j * w + i] = 0.0;
            }
        }
        let d = detect_semi_transparent(&alpha, &grad, &quant, w, h, &cfg())
            .expect("64 flat intermediate pixels are an interior");
        assert_eq!(d.largest_region_px, 64);
        assert_eq!(d.regions, 1);
        assert_eq!(d.alpha_span, 0.0);
    }

    /// The flatness test is in units of the alpha quantization noise, so the
    /// SAME gradient is flat for an ill-conditioned pair and structured for
    /// a well-conditioned one. That is the low-contrast case getting more
    /// benefit of the doubt, by construction rather than by a special case.
    #[test]
    fn flatness_is_judged_against_the_alpha_noise_not_against_a_constant() {
        let (w, h) = (8usize, 8usize);
        let alpha = vec![0.5f64; w * h];
        let grad = vec![0.02f64; w * h];
        let sharp = vec![0.002f64; w * h];
        let noisy = vec![0.02f64; w * h];
        assert!(
            detect_semi_transparent(&alpha, &grad, &sharp, w, h, &cfg()).is_none(),
            "against a small alpha noise this gradient is structure"
        );
        assert!(
            detect_semi_transparent(&alpha, &grad, &noisy, w, h, &cfg()).is_some(),
            "against a large alpha noise the same gradient is flatness"
        );
    }
}
