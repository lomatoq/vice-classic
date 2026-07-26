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
pub fn composite_rgba8(
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
