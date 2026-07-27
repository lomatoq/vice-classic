//! The premultiplied observation tensor of one blend-space hypothesis
//! (spec §5.2, §10, §1.6).
//!
//! §10 gives the Flat2 mixture in premultiplied vectors:
//!
//! ```text
//! â_p = clamp[0,1] ( (P_i − P_b)·(P_f − P_b) / ‖P_f − P_b‖² )
//! r_p = P_i − [ â_p P_f + (1 − â_p) P_b ]
//! ```
//!
//! and that arithmetic is only linear in coverage in the space the
//! rasterizer actually blended in. So the tensor is built PER HYPOTHESIS:
//!
//! | hypothesis | the quantity that is linear in coverage |
//! |---|---|
//! | `LinearLight` | `srgb_to_linear(byte)·α` |
//! | `EncodedSrgb` | `(byte/255)·α` |
//!
//! Both put alpha in the fourth component unchanged, which is what makes
//! the transparent exterior `P_b = (0,0,0,0)` work without a special case:
//! a pixel that is 40 % covered by an opaque ink stores the ink's FULL
//! colour next to `α = 0.4`, and only after premultiplication is the
//! observation `0.4·P_f`.
//!
//! The same step is what erases RGB under `α ≈ 0` (§1.6). This crate never
//! un-premultiplies.

use serde::Serialize;
use vice_ir::color::{linear_to_srgb_encoded, srgb_u8_to_linear};
use vice_ir::{BlendSpace, LinearRgb};

use crate::decode::CanonicalImage;

/// Components of an observation vector: R, G, B, A.
pub const CHANNELS: usize = 4;

/// The transparent exterior of §10: `P_b = (0,0,0,0)` in every blend space.
pub const TRANSPARENT_EXTERIOR_PREMUL: [f64; CHANNELS] = [0.0, 0.0, 0.0, 0.0];

/// One 8-bit code, as a fraction of full scale.
const CODE: f64 = 1.0 / 255.0;

/// How an opaque paint would be OBSERVED at full coverage, under one blend
/// space. This is `P_f` of §10 for an opaque face.
pub fn paint_observation_premul(c: LinearRgb, blend: BlendSpace) -> [f64; CHANNELS] {
    match blend {
        BlendSpace::LinearLight => [c.r, c.g, c.b, 1.0],
        BlendSpace::EncodedSrgb => [
            linear_to_srgb_encoded(c.r.clamp(0.0, 1.0)),
            linear_to_srgb_encoded(c.g.clamp(0.0, 1.0)),
            linear_to_srgb_encoded(c.b.clamp(0.0, 1.0)),
            1.0,
        ],
    }
}

/// The premultiplied observation of a whole image, under one blend space.
#[derive(Debug, Clone, PartialEq)]
pub struct ObservationTensor {
    width_px: u32,
    height_px: u32,
    blend: BlendSpace,
    premul: Vec<[f64; CHANNELS]>,
    /// Half-width of the 8-bit quantization cell of each component, in the
    /// tensor's own units. §10 asks for the quantization interval to be
    /// carried with the evidence rather than folded into a residual.
    quant_halfwidth: Vec<[f64; CHANNELS]>,
}

/// Summary of a tensor, for reports.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TensorSummary {
    pub blend_space: &'static str,
    pub pixels: u64,
    pub fully_transparent_pixels: u64,
    pub fully_opaque_pixels: u64,
    pub partial_alpha_pixels: u64,
}

impl ObservationTensor {
    pub fn of(img: &CanonicalImage, blend: BlendSpace) -> ObservationTensor {
        let n = img.pixel_count();
        let mut premul = Vec::with_capacity(n);
        let mut quant = Vec::with_capacity(n);
        // Channel transfer, and the width of one code step at that value.
        let value = |v: u8| -> f64 {
            match blend {
                BlendSpace::LinearLight => srgb_u8_to_linear(v),
                BlendSpace::EncodedSrgb => f64::from(v) * CODE,
            }
        };
        // The transfer is non-linear under `LinearLight`, so the width of a
        // code cell depends on WHERE it is: near black one code is worth
        // 3e-4 of linear light, near white 6e-3. Taking the wider of the
        // two neighbouring steps keeps the interval a bound rather than an
        // average.
        let step = |v: u8| -> f64 {
            let lo = value(v.saturating_sub(1));
            let hi = value(v.saturating_add(1));
            let here = value(v);
            (here - lo).abs().max((hi - here).abs())
        };
        for i in 0..n {
            let p = img.pixel(i);
            let a = f64::from(p[3]) * CODE;
            let a_half = 0.5 * CODE;
            let mut v = [0.0f64; CHANNELS];
            let mut q = [0.0f64; CHANNELS];
            for ch in 0..3 {
                let c = value(p[ch]);
                v[ch] = c * a;
                // First order in the two independent byte errors: the
                // colour byte scaled by alpha, plus the alpha byte scaled
                // by the colour.
                q[ch] = 0.5 * step(p[ch]) * a + c * a_half;
            }
            v[3] = a;
            q[3] = a_half;
            premul.push(v);
            quant.push(q);
        }
        ObservationTensor {
            width_px: img.width_px(),
            height_px: img.height_px(),
            blend,
            premul,
            quant_halfwidth: quant,
        }
    }

    pub fn width_px(&self) -> u32 {
        self.width_px
    }
    pub fn height_px(&self) -> u32 {
        self.height_px
    }
    pub fn len(&self) -> usize {
        self.premul.len()
    }
    pub fn is_empty(&self) -> bool {
        self.premul.is_empty()
    }
    pub fn blend_space(&self) -> BlendSpace {
        self.blend
    }
    pub fn premul(&self, i: usize) -> [f64; CHANNELS] {
        self.premul[i]
    }
    pub fn alpha(&self, i: usize) -> f64 {
        self.premul[i][3]
    }
    pub fn quantization_halfwidth(&self, i: usize) -> [f64; CHANNELS] {
        self.quant_halfwidth[i]
    }
    pub fn index(&self, x: u32, y: u32) -> usize {
        (y as usize) * (self.width_px as usize) + (x as usize)
    }

    pub fn summary(&self) -> TensorSummary {
        let mut transparent = 0u64;
        let mut opaque = 0u64;
        let mut partial = 0u64;
        for v in &self.premul {
            if v[3] <= 0.5 * CODE {
                transparent += 1;
            } else if v[3] >= 1.0 - 0.5 * CODE {
                opaque += 1;
            } else {
                partial += 1;
            }
        }
        TensorSummary {
            blend_space: match self.blend {
                BlendSpace::LinearLight => "linear_light",
                BlendSpace::EncodedSrgb => "encoded_srgb",
            },
            pixels: self.premul.len() as u64,
            fully_transparent_pixels: transparent,
            fully_opaque_pixels: opaque,
            partial_alpha_pixels: partial,
        }
    }
}

/// `a − b`, componentwise.
pub fn sub(a: [f64; CHANNELS], b: [f64; CHANNELS]) -> [f64; CHANNELS] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2], a[3] - b[3]]
}

/// `a · b`.
pub fn dot(a: [f64; CHANNELS], b: [f64; CHANNELS]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3]
}

/// `‖a‖`.
pub fn norm(a: [f64; CHANNELS]) -> f64 {
    dot(a, a).sqrt()
}

/// `b + t·(a − b)`, the mixture of §10.
pub fn mix(b: [f64; CHANNELS], a: [f64; CHANNELS], t: f64) -> [f64; CHANNELS] {
    [
        b[0] + t * (a[0] - b[0]),
        b[1] + t * (a[1] - b[1]),
        b[2] + t * (a[2] - b[2]),
        b[3] + t * (a[3] - b[3]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::IccAssumption;

    fn image(pixels: &[[u8; 4]]) -> CanonicalImage {
        let mut bytes = Vec::new();
        for p in pixels {
            bytes.extend_from_slice(p);
        }
        CanonicalImage::from_straight_srgb8(
            pixels.len() as u32,
            1,
            bytes,
            true,
            IccAssumption::NoProfileAssumedSrgb,
        )
        .unwrap()
    }

    /// §1.6, as arithmetic: a pixel with `α = 0` is the zero vector no
    /// matter what colour its bytes carry, in EVERY blend space. This is
    /// the pixel that made F-0021 a 243-code disagreement.
    #[test]
    fn rgb_under_zero_alpha_is_the_zero_vector_in_every_blend_space() {
        let img = image(&[[243, 137, 124, 0], [0, 0, 0, 0]]);
        for blend in [BlendSpace::LinearLight, BlendSpace::EncodedSrgb] {
            let t = ObservationTensor::of(&img, blend);
            assert_eq!(t.premul(0), [0.0; 4], "{blend:?}");
            assert_eq!(t.premul(0), t.premul(1), "{blend:?}");
        }
    }

    /// The mixture geometry of §10 with a transparent exterior: a pixel
    /// that stores an ink's full colour next to a partial alpha lies
    /// EXACTLY on the segment from the origin to `P_f`, and its parameter
    /// is the coverage.
    ///
    /// This is what makes `P_b = (0,0,0,0)` work without a special case,
    /// and it holds in both blend spaces because straight storage keeps the
    /// colour bytes independent of coverage.
    #[test]
    fn partial_coverage_of_one_ink_lies_on_the_segment_to_the_paint() {
        let ink = LinearRgb::new(0.05, 0.35, 0.8);
        // The bytes a compositor writes for this ink: its own colour,
        // encoded, with alpha carrying the coverage.
        let enc = |v: f64| (linear_to_srgb_encoded(v) * 255.0).round() as u8;
        let bytes = [enc(ink.r), enc(ink.g), enc(ink.b), 102]; // α = 0.4
        let img = image(&[bytes]);
        for blend in [BlendSpace::LinearLight, BlendSpace::EncodedSrgb] {
            let t = ObservationTensor::of(&img, blend);
            let pf = paint_observation_premul(ink, blend);
            let pi = t.premul(0);
            let alpha_hat = dot(pi, pf) / dot(pf, pf);
            assert!(
                (alpha_hat - 102.0 / 255.0).abs() < 2e-3,
                "{blend:?}: â = {alpha_hat}"
            );
            // And the residual against that estimate is at the quantization
            // floor, i.e. the model explains the pixel.
            let r = sub(pi, mix(TRANSPARENT_EXTERIOR_PREMUL, pf, alpha_hat));
            assert!(norm(r) < 4.0 * CODE, "{blend:?}: ‖r‖ = {}", norm(r));
        }
    }

    /// The recorded quantization interval must BOUND the effect of one code
    /// of error, or carrying it would be decoration. Checked over the whole
    /// byte range in both spaces, including near black where the linear
    /// transfer is flattest.
    #[test]
    fn the_quantization_interval_bounds_a_one_code_perturbation() {
        for blend in [BlendSpace::LinearLight, BlendSpace::EncodedSrgb] {
            for v in 0..=255u8 {
                for a in [0u8, 1, 64, 128, 254, 255] {
                    let base = image(&[[v, v, v, a]]);
                    let t = ObservationTensor::of(&base, blend);
                    let half = t.quantization_halfwidth(0);
                    for (dv, da) in [(1i16, 0i16), (-1, 0), (0, 1), (0, -1)] {
                        let v2 = (i16::from(v) + dv).clamp(0, 255) as u8;
                        let a2 = (i16::from(a) + da).clamp(0, 255) as u8;
                        let other = ObservationTensor::of(&image(&[[v2, v2, v2, a2]]), blend);
                        for (ch, h) in half.iter().enumerate() {
                            let d = (t.premul(0)[ch] - other.premul(0)[ch]).abs();
                            assert!(
                                d <= 2.0 * h + 1e-12,
                                "{blend:?} v={v} a={a} ch={ch}: |Δ| = {d} > 2·{h}"
                            );
                        }
                    }
                }
            }
        }
    }

    /// Control in the other direction: the interval is not so wide that it
    /// says nothing. One code near mid-grey must stay a small fraction of
    /// full scale.
    #[test]
    fn the_quantization_interval_is_not_vacuously_wide() {
        let img = image(&[[128, 128, 128, 255]]);
        for blend in [BlendSpace::LinearLight, BlendSpace::EncodedSrgb] {
            let t = ObservationTensor::of(&img, blend);
            let h = t.quantization_halfwidth(0);
            for (ch, v) in h.iter().enumerate() {
                assert!(*v > 0.0 && *v < 0.02, "{blend:?} ch={ch}: {v}");
            }
        }
    }

    /// The two blend spaces are DIFFERENT tensors wherever the transfer
    /// bends, which is what makes the blend space a hypothesis worth
    /// testing rather than a formality.
    #[test]
    fn the_two_blend_spaces_disagree_on_a_mid_tone() {
        let img = image(&[[128, 64, 200, 255]]);
        let lin = ObservationTensor::of(&img, BlendSpace::LinearLight);
        let srgb = ObservationTensor::of(&img, BlendSpace::EncodedSrgb);
        assert_ne!(lin.premul(0), srgb.premul(0));
        // …and agree on the endpoints of the transfer, where it does not.
        let ends = image(&[[0, 0, 0, 255], [255, 255, 255, 255]]);
        let l = ObservationTensor::of(&ends, BlendSpace::LinearLight);
        let s = ObservationTensor::of(&ends, BlendSpace::EncodedSrgb);
        for i in 0..2 {
            for ch in 0..CHANNELS {
                assert!((l.premul(i)[ch] - s.premul(i)[ch]).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn the_summary_counts_the_three_alpha_classes() {
        let img = image(&[[0, 0, 0, 0], [10, 10, 10, 128], [20, 20, 20, 255]]);
        let s = ObservationTensor::of(&img, BlendSpace::LinearLight).summary();
        assert_eq!(s.pixels, 3);
        assert_eq!(s.fully_transparent_pixels, 1);
        assert_eq!(s.partial_alpha_pixels, 1);
        assert_eq!(s.fully_opaque_pixels, 1);
    }
}
