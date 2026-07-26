//! Color convention (spec v1.3 §5.2, docs/COORDINATE_CONVENTION.md §2).
//!
//! Canonical scene colors are LINEAR-LIGHT, STRAIGHT (non-premultiplied)
//! RGBA. Compositing / raster likelihood works in PREMULTIPLIED linear RGBA.
//! Whether the observed rasterizer blended coverage in linear light or in
//! encoded sRGB is a formation HYPOTHESIS ([`BlendSpace`]), never a silent
//! assumption.
//!
//! The decode pipeline (bytes → ICC assumption → straight RGBA → linear →
//! premultiplied observation tensor) arrives with `vice-image` in its own
//! milestone; this module fixes the vocabulary and the convention-defining
//! transforms, covered by unit tests.

use serde::{Deserialize, Serialize};

/// Opaque linear-light RGB, components in `[0, 1]`. The canonical paint
/// color of the flat core.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinearRgb {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

impl LinearRgb {
    pub const fn new(r: f64, g: f64, b: f64) -> Self {
        LinearRgb { r, g, b }
    }

    pub fn components(self) -> [f64; 3] {
        [self.r, self.g, self.b]
    }
}

/// Straight (non-premultiplied) linear-light RGBA.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinearRgba {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

/// Premultiplied linear-light RGBA — the observation/compositing space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PremulRgba {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

/// Straight → premultiplied. At `a = 0` every channel becomes exactly 0:
/// RGB under zero alpha is NOT color evidence (spec §1.6, §5.2).
pub fn premultiply(c: LinearRgba) -> PremulRgba {
    PremulRgba {
        r: c.r * c.a,
        g: c.g * c.a,
        b: c.b * c.a,
        a: c.a,
    }
}

/// Premultiplied → straight. `None` at `a = 0`: the straight color is not
/// recoverable and must not be invented.
pub fn unpremultiply(c: PremulRgba) -> Option<LinearRgba> {
    if c.a == 0.0 {
        return None;
    }
    Some(LinearRgba {
        r: c.r / c.a,
        g: c.g / c.a,
        b: c.b / c.a,
        a: c.a,
    })
}

/// In which space did the observed rasterizer blend coverage? A formation
/// hypothesis (spec §5.2): forcing "all AA is linear" and then explaining
/// the mismatch with geometry is forbidden.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum BlendSpace {
    LinearLight,
    EncodedSrgb,
}

/// sRGB electro-optical transfer (IEC 61966-2-1): encoded `[0,1]` → linear.
pub fn srgb_encoded_to_linear(e: f64) -> f64 {
    if e <= 0.04045 {
        e / 12.92
    } else {
        ((e + 0.055) / 1.055).powf(2.4)
    }
}

/// sRGB inverse transfer: linear `[0,1]` → encoded.
pub fn linear_to_srgb_encoded(l: f64) -> f64 {
    if l <= 0.003_130_8 {
        l * 12.92
    } else {
        1.055 * l.powf(1.0 / 2.4) - 0.055
    }
}

/// Decode one 8-bit sRGB channel to linear light.
pub fn srgb_u8_to_linear(v: u8) -> f64 {
    srgb_encoded_to_linear(f64::from(v) / 255.0)
}

/// Encode linear light to the nearest 8-bit sRGB channel value.
pub fn linear_to_srgb_u8(l: f64) -> u8 {
    let e = linear_to_srgb_encoded(l.clamp(0.0, 1.0));
    (e * 255.0).round().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn premultiply_kills_rgb_under_zero_alpha() {
        let c = LinearRgba {
            r: 0.9,
            g: 0.1,
            b: 0.5,
            a: 0.0,
        };
        let p = premultiply(c);
        assert_eq!((p.r, p.g, p.b, p.a), (0.0, 0.0, 0.0, 0.0));
        // And the straight color is not recoverable.
        assert_eq!(unpremultiply(p), None);
    }

    #[test]
    fn premultiply_roundtrips_for_positive_alpha() {
        let c = LinearRgba {
            r: 0.25,
            g: 0.5,
            b: 1.0,
            a: 0.5,
        };
        let back = unpremultiply(premultiply(c)).unwrap();
        assert!((back.r - c.r).abs() < 1e-15);
        assert!((back.g - c.g).abs() < 1e-15);
        assert!((back.b - c.b).abs() < 1e-15);
        assert_eq!(back.a, c.a);
    }

    #[test]
    fn srgb_transfer_endpoints_and_breakpoint() {
        assert_eq!(srgb_encoded_to_linear(0.0), 0.0);
        assert!((srgb_encoded_to_linear(1.0) - 1.0).abs() < 1e-12);
        assert_eq!(linear_to_srgb_encoded(0.0), 0.0);
        assert!((linear_to_srgb_encoded(1.0) - 1.0).abs() < 1e-12);
        // Continuity at the linear/power breakpoint.
        let lo = srgb_encoded_to_linear(0.04045 - 1e-9);
        let hi = srgb_encoded_to_linear(0.04045 + 1e-9);
        assert!((hi - lo).abs() < 1e-6);
    }

    #[test]
    fn srgb_transfer_is_monotonic() {
        let mut prev = -1.0;
        for i in 0..=1000 {
            let e = f64::from(i) / 1000.0;
            let l = srgb_encoded_to_linear(e);
            assert!(l > prev);
            prev = l;
        }
    }

    #[test]
    fn srgb_u8_roundtrip_is_exact_for_all_256_values() {
        for v in 0..=255u8 {
            assert_eq!(linear_to_srgb_u8(srgb_u8_to_linear(v)), v, "v={v}");
        }
    }

    #[test]
    fn transfer_functions_are_mutual_inverses_inside_gamut() {
        for i in 0..=100 {
            let l = f64::from(i) / 100.0;
            let roundtrip = srgb_encoded_to_linear(linear_to_srgb_encoded(l));
            assert!((roundtrip - l).abs() < 1e-12, "l={l}");
        }
    }
}
