//! Colour handling of a degradation cell: blend space, contrast and 8-bit
//! quantization (spec §5.2, §27.2).
//!
//! Split from `raster` so each module stays under the §4.1 size rule and so
//! the geometric and photometric halves of a degradation are separable in
//! review.

use vice_ir::{BlendSpace, LinearRgb, Paint};

use super::raster::CoverageStack;

/// IEC 61966-2-1 sRGB transfer (linear -> encoded).
pub fn srgb_encode(c: f64) -> f64 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// Pull a colour toward mid grey by `contrast` (1.0 = unchanged).
pub fn apply_contrast(c: LinearRgb, contrast: f64) -> LinearRgb {
    let mid = 0.5;
    LinearRgb {
        r: mid + (c.r - mid) * contrast,
        g: mid + (c.g - mid) * contrast,
        b: mid + (c.b - mid) * contrast,
    }
}

/// Composite a coverage stack into 8-bit straight RGBA.
///
/// `blend` decides WHERE the coverage weights are applied: in linear light,
/// or after the sRGB transfer. Real rasterizers do both, and §5.2 forbids
/// assuming one — so the corpus contains both, and the two produce
/// different bytes for the same geometry, which is the point of the axis.
pub(crate) fn composite_rgba8(
    stack: &CoverageStack,
    paints: &[Paint],
    blend: BlendSpace,
    contrast: f64,
) -> Vec<u8> {
    let n = (stack.width_px as usize) * (stack.height_px as usize);
    let mut out = vec![0u8; n * 4];
    let prepared: Vec<Option<[f64; 3]>> = paints
        .iter()
        .map(|p| match p {
            Paint::OpaqueSolid(c) => {
                let c = apply_contrast(*c, contrast);
                Some(match blend {
                    BlendSpace::LinearLight => [c.r, c.g, c.b],
                    BlendSpace::EncodedSrgb => {
                        [srgb_encode(c.r), srgb_encode(c.g), srgb_encode(c.b)]
                    }
                })
            }
            Paint::TransparentExterior => None,
        })
        .collect();

    for i in 0..n {
        let (mut r, mut g, mut b, mut a) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
        for (fi, cov) in stack.per_face.iter().enumerate() {
            let c = cov[i];
            if let Some(rgb) = prepared[fi] {
                r += c * rgb[0];
                g += c * rgb[1];
                b += c * rgb[2];
                a += c;
            }
        }
        // Un-premultiply, then encode if we blended in linear light.
        let (r, g, b) = if a > 1e-12 {
            (r / a, g / a, b / a)
        } else {
            (0.0, 0.0, 0.0)
        };
        let (r, g, b) = match blend {
            BlendSpace::LinearLight => (srgb_encode(r), srgb_encode(g), srgb_encode(b)),
            BlendSpace::EncodedSrgb => (r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0)),
        };
        let q = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        out[4 * i] = q(r);
        out[4 * i + 1] = q(g);
        out[4 * i + 2] = q(b);
        out[4 * i + 3] = q(a);
    }
    out
}

/// Per-channel disagreement of two straight-RGBA8 pixels, in PREMULTIPLIED
/// code units.
///
/// Why premultiplied, and not the bytes as stored. §1.6 is explicit: *"RGB
/// under `alpha≈0` is not colour evidence and is ignored after
/// premultiplication"*, and §5.2 puts the raster likelihood in premultiplied
/// linear RGBA for exactly that reason. `composite_rgba8` writes STRAIGHT
/// RGBA, so a pixel whose coverage is 0.04 carries the paint's full RGB next
/// to an alpha of 11 — and a straight-byte difference against an
/// uncovered pixel reads 243 code values where the physical disagreement is
/// 4% of one pixel's coverage.
///
/// That is not a hypothetical: it was the largest number in the first M3.5
/// ceiling run (FAILURE_LEDGER F-0021). Any MAGNITUDE metric over these
/// bytes therefore goes through here; equality and digests may use the bytes
/// directly, because those ask a different question.
pub fn premultiplied_deltas(a: &[u8], b: &[u8]) -> [f64; 4] {
    let alpha_a = f64::from(a[3]);
    let alpha_b = f64::from(b[3]);
    let mut out = [0.0f64; 4];
    for ch in 0..3 {
        let pa = f64::from(a[ch]) * alpha_a / 255.0;
        let pb = f64::from(b[ch]) * alpha_b / 255.0;
        out[ch] = (pa - pb).abs();
    }
    out[3] = (alpha_a - alpha_b).abs();
    out
}

/// Largest premultiplied per-channel disagreement over two RGBA8 images.
pub fn max_premultiplied_code_difference(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len(), "images must have the same size");
    a.chunks(4)
        .zip(b.chunks(4))
        .flat_map(|(x, y)| premultiplied_deltas(x, y))
        .fold(0.0f64, f64::max)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The case that made the rule: two pixels that agree about coverage to
    /// within 4% must not read as a 243-code disagreement just because one
    /// of them stores a paint colour under an alpha of zero.
    #[test]
    fn rgb_under_transparent_alpha_is_not_a_disagreement() {
        let uncovered = [0u8, 0, 0, 0];
        let barely = [243u8, 137, 124, 11];
        let straight = uncovered
            .iter()
            .zip(&barely)
            .map(|(x, y)| f64::from(x.abs_diff(*y)))
            .fold(0.0, f64::max);
        assert_eq!(straight, 243.0, "the straight-byte metric says this");
        let premul = premultiplied_deltas(&uncovered, &barely);
        assert!(
            premul[..3].iter().all(|d| *d < 11.0),
            "premultiplied colour deltas are bounded by the alpha: {premul:?}"
        );
        assert_eq!(premul[3], 11.0, "and the real disagreement is the alpha");

        // Fully opaque pixels are unaffected: the fix must not blunt the
        // metric where it was already right.
        let red = [255u8, 0, 0, 255];
        let green = [0u8, 255, 0, 255];
        assert_eq!(max_premultiplied_code_difference(&red, &green), 255.0);
        assert_eq!(max_premultiplied_code_difference(&red, &red), 0.0);
    }
}
