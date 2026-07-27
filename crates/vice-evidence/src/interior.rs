//! Interior confidence (spec §9.1).
//!
//! §9.1 lists what earns a pixel a high weight when a palette is being
//! estimated: low physical gradient, low local covariance, coherent
//! same-colour support, stable alpha, distance from a mixed edge — and two
//! prohibitions: *"pixels with `alpha≈0` give no RGB evidence"* and *"edge
//! pixels do not train the palette at full weight"*.
//!
//! Every quantity here is measured in units of the LOCAL QUANTIZATION NOISE
//! rather than in raw channel values. That is not decoration: under
//! `LinearLight` one 8-bit code is worth 3e-4 of linear light near black and
//! 6e-3 near white, so a fixed threshold in channel units would call a dark
//! interior "smooth" and an equally smooth light interior "structured". The
//! tensor already carries the width of a code cell at each pixel
//! (`ObservationTensor::quantization_halfwidth`), so the natural unit is
//! there for the taking.
//!
//! The coefficients below are a WEIGHTING, not a threshold: nothing is
//! accepted or rejected by them, they only decide how much a pixel votes.
//! They are frozen here and their effect is MEASURED on the development
//! split by `vice-bench` (`interior_confidence_separates_cores_from_edges`),
//! which is what keeps "these numbers look reasonable" from being the whole
//! argument.

use serde::Serialize;
use vice_image::{norm, sub, ObservationTensor, CHANNELS};

/// Dimensionless coefficients of the interior weighting, in units of the
/// local quantization noise (see the module docs).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct InteriorConfig {
    /// Gradient magnitude, in noise units, at which the weight halves.
    pub gradient_scale: f64,
    /// Local window diameter, in noise units, at which the weight halves.
    pub spread_scale: f64,
    /// Neighbourhood alpha variation, in 8-bit codes, at which the weight
    /// halves.
    pub alpha_variation_scale_codes: f64,
    /// Gradient, in noise units, above which a pixel counts as MIXED and
    /// seeds the edge distance transform.
    pub mixed_gradient_ratio: f64,
    /// Distance from a mixed edge, in px, at which the distance term
    /// saturates. §9.1: an edge pixel does not train the palette at full
    /// weight.
    pub edge_reach_px: f64,
    /// Alpha below which a pixel carries NO RGB evidence at all (§1.6),
    /// in 8-bit codes.
    pub alpha_zero_codes: f64,
}

pub const INTERIOR_CONFIG_V1: InteriorConfig = InteriorConfig {
    gradient_scale: 8.0,
    spread_scale: 12.0,
    alpha_variation_scale_codes: 8.0,
    mixed_gradient_ratio: 8.0,
    edge_reach_px: 1.5,
    alpha_zero_codes: 1.0,
};

/// Per-pixel interior evidence.
#[derive(Debug, Clone, PartialEq)]
pub struct InteriorConfidence {
    width_px: u32,
    height_px: u32,
    /// Total weight in `[0, 1]`: how much this pixel may train a palette.
    weight: Vec<f64>,
    /// `‖∇P‖` in units of the local quantization noise.
    gradient: Vec<f64>,
    /// Diameter of the 3x3 window in observation space, in noise units.
    spread: Vec<f64>,
    /// Largest neighbourhood alpha difference, in 8-bit codes.
    alpha_variation_codes: Vec<f64>,
    /// Chamfer distance to the nearest MIXED pixel, in px.
    edge_distance_px: Vec<f64>,
    /// False where `alpha ≈ 0`: the stored RGB is not colour evidence.
    gives_rgb_evidence: Vec<bool>,
    mixed_pixels: u64,
}

/// Summary for reports.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InteriorSummary {
    pub pixels: u64,
    pub mixed_pixels: u64,
    pub pixels_without_rgb_evidence: u64,
    pub high_confidence_pixels: u64,
    pub max_weight: f64,
    pub mean_weight: f64,
}

/// Weight above which a pixel counts as a reliable interior core sample.
pub const CORE_WEIGHT: f64 = 0.5;

fn soft(x: f64, scale: f64) -> f64 {
    let r = x / scale;
    1.0 / (1.0 + r * r)
}

impl InteriorConfidence {
    pub fn weight(&self, i: usize) -> f64 {
        self.weight[i]
    }
    pub fn gradient(&self, i: usize) -> f64 {
        self.gradient[i]
    }
    pub fn spread(&self, i: usize) -> f64 {
        self.spread[i]
    }
    pub fn alpha_variation_codes(&self, i: usize) -> f64 {
        self.alpha_variation_codes[i]
    }
    pub fn edge_distance_px(&self, i: usize) -> f64 {
        self.edge_distance_px[i]
    }
    pub fn gives_rgb_evidence(&self, i: usize) -> bool {
        self.gives_rgb_evidence[i]
    }
    pub fn is_mixed(&self, i: usize) -> bool {
        self.edge_distance_px[i] == 0.0
    }
    pub fn len(&self) -> usize {
        self.weight.len()
    }
    pub fn is_empty(&self) -> bool {
        self.weight.is_empty()
    }
    pub fn width_px(&self) -> u32 {
        self.width_px
    }
    pub fn height_px(&self) -> u32 {
        self.height_px
    }

    pub fn summary(&self) -> InteriorSummary {
        let n = self.weight.len() as f64;
        InteriorSummary {
            pixels: self.weight.len() as u64,
            mixed_pixels: self.mixed_pixels,
            pixels_without_rgb_evidence: self.gives_rgb_evidence.iter().filter(|v| !**v).count()
                as u64,
            high_confidence_pixels: self.weight.iter().filter(|w| **w >= CORE_WEIGHT).count()
                as u64,
            max_weight: self.weight.iter().copied().fold(0.0, f64::max),
            mean_weight: if n > 0.0 {
                self.weight.iter().sum::<f64>() / n
            } else {
                0.0
            },
        }
    }
}

/// Compute interior confidence over one observation tensor.
pub fn interior_confidence(t: &ObservationTensor, cfg: &InteriorConfig) -> InteriorConfidence {
    let (w, h) = (t.width_px() as i64, t.height_px() as i64);
    let n = t.len();
    let at = |x: i64, y: i64| -> usize {
        let cx = x.clamp(0, w - 1);
        let cy = y.clamp(0, h - 1);
        (cy as usize) * (w as usize) + (cx as usize)
    };
    // The local unit: the magnitude of one quantization cell, floored so a
    // fully transparent pixel (whose colour cell has zero width because it
    // is multiplied by alpha) does not divide by zero.
    let noise = |i: usize| -> f64 {
        let q = t.quantization_halfwidth(i);
        let mut acc = 0.0;
        for v in q.iter().take(CHANNELS) {
            acc += v * v;
        }
        acc.sqrt().max(1.0 / 255.0 * 0.5)
    };

    let mut gradient = vec![0.0f64; n];
    let mut spread = vec![0.0f64; n];
    let mut alpha_variation = vec![0.0f64; n];
    let mut gives_rgb = vec![false; n];
    let alpha_zero = cfg.alpha_zero_codes / 255.0;

    for y in 0..h {
        for x in 0..w {
            let i = at(x, y);
            let unit = noise(i);
            let here = t.premul(i);
            let dx = sub(t.premul(at(x + 1, y)), t.premul(at(x - 1, y)));
            let dy = sub(t.premul(at(x, y + 1)), t.premul(at(x, y - 1)));
            // Central differences over a two-pixel baseline.
            let g = 0.5 * (norm(dx).hypot(norm(dy)));
            gradient[i] = g / unit;

            let mut diam = 0.0f64;
            let mut amax = 0.0f64;
            for oy in -1..=1i64 {
                for ox in -1..=1i64 {
                    let j = at(x + ox, y + oy);
                    diam = diam.max(norm(sub(t.premul(j), here)));
                    amax = amax.max((t.alpha(j) - t.alpha(i)).abs());
                }
            }
            spread[i] = diam / unit;
            alpha_variation[i] = amax * 255.0;
            gives_rgb[i] = t.alpha(i) > alpha_zero;
        }
    }

    // Mixed pixels: partially covered, or structured beyond the noise.
    let mut edge_distance = vec![f64::INFINITY; n];
    let mut mixed_pixels = 0u64;
    for i in 0..n {
        let a = t.alpha(i);
        let partial = a > alpha_zero && a < 1.0 - alpha_zero;
        if partial || gradient[i] > cfg.mixed_gradient_ratio {
            edge_distance[i] = 0.0;
            mixed_pixels += 1;
        }
    }
    chamfer_distance(&mut edge_distance, w as usize, h as usize);

    let weight = (0..n)
        .map(|i| {
            soft(gradient[i], cfg.gradient_scale)
                * soft(spread[i], cfg.spread_scale)
                * soft(alpha_variation[i], cfg.alpha_variation_scale_codes)
                * (edge_distance[i] / cfg.edge_reach_px).clamp(0.0, 1.0)
        })
        .collect();

    InteriorConfidence {
        width_px: t.width_px(),
        height_px: t.height_px(),
        weight,
        gradient,
        spread,
        alpha_variation_codes: alpha_variation,
        edge_distance_px: edge_distance,
        gives_rgb_evidence: gives_rgb,
        mixed_pixels,
    }
}

/// Two-pass chamfer distance transform with (1, √2) weights.
///
/// An exact Euclidean transform would be more accurate; the chamfer error is
/// under 5 % and every consumer here uses the distance as a soft weight that
/// saturates within two pixels, so the difference cannot change a decision.
/// Said out loud because "approximate distance transform" is the kind of
/// detail that silently becomes a threshold later.
fn chamfer_distance(d: &mut [f64], w: usize, h: usize) {
    const D1: f64 = 1.0;
    const D2: f64 = std::f64::consts::SQRT_2;
    let idx = |x: usize, y: usize| y * w + x;
    for y in 0..h {
        for x in 0..w {
            let mut v = d[idx(x, y)];
            if y > 0 {
                v = v.min(d[idx(x, y - 1)] + D1);
                if x > 0 {
                    v = v.min(d[idx(x - 1, y - 1)] + D2);
                }
                if x + 1 < w {
                    v = v.min(d[idx(x + 1, y - 1)] + D2);
                }
            }
            if x > 0 {
                v = v.min(d[idx(x - 1, y)] + D1);
            }
            d[idx(x, y)] = v;
        }
    }
    for y in (0..h).rev() {
        for x in (0..w).rev() {
            let mut v = d[idx(x, y)];
            if y + 1 < h {
                v = v.min(d[idx(x, y + 1)] + D1);
                if x + 1 < w {
                    v = v.min(d[idx(x + 1, y + 1)] + D2);
                }
                if x > 0 {
                    v = v.min(d[idx(x - 1, y + 1)] + D2);
                }
            }
            if x + 1 < w {
                v = v.min(d[idx(x + 1, y)] + D1);
            }
            d[idx(x, y)] = v;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vice_image::{CanonicalImage, IccAssumption};
    use vice_ir::BlendSpace;

    /// A square of ink on a transparent exterior, with one row of AA.
    fn square_with_aa(size: u32) -> CanonicalImage {
        let mut px = vec![0u8; (size * size * 4) as usize];
        let lo = size / 4;
        let hi = size - size / 4;
        for y in 0..size {
            for x in 0..size {
                let inside = x > lo && x < hi - 1 && y > lo && y < hi - 1;
                let on_edge = (x == lo || x == hi - 1 || y == lo || y == hi - 1)
                    && (lo..hi).contains(&x)
                    && (lo..hi).contains(&y);
                let a: u8 = if inside {
                    255
                } else if on_edge {
                    128
                } else {
                    0
                };
                let i = ((y * size + x) * 4) as usize;
                px[i] = 30;
                px[i + 1] = 90;
                px[i + 2] = 200;
                px[i + 3] = a;
            }
        }
        CanonicalImage::from_straight_srgb8(
            size,
            size,
            px,
            true,
            IccAssumption::NoProfileAssumedSrgb,
        )
        .unwrap()
    }

    fn confidence(img: &CanonicalImage) -> (InteriorConfidence, CanonicalImage) {
        let t = ObservationTensor::of(img, BlendSpace::LinearLight);
        (interior_confidence(&t, &INTERIOR_CONFIG_V1), img.clone())
    }

    /// The property §9.1 exists for: the core of a flat region votes and its
    /// antialiased rim does not. Both directions, because a weighting that
    /// returned zero everywhere would satisfy half of it.
    #[test]
    fn a_flat_core_votes_and_its_antialiased_rim_does_not() {
        let img = square_with_aa(24);
        let (c, img) = confidence(&img);
        let core = img.index(12, 12);
        let rim = img.index(6, 12);
        let outside = img.index(1, 1);
        assert!(c.weight(core) > 0.9, "core weight {}", c.weight(core));
        assert!(c.weight(rim) < 0.2, "rim weight {}", c.weight(rim));
        assert!(c.is_mixed(rim), "the rim must seed the edge distance");
        assert!(!c.is_mixed(core));
        assert!(c.edge_distance_px(core) > 2.0);
        // The exterior is flat too, so it is a legitimate interior of the
        // exterior face - but it carries NO RGB evidence (§1.6).
        assert!(!c.gives_rgb_evidence(outside));
        assert!(c.gives_rgb_evidence(core));
    }

    /// The unit is the local quantization noise, so the SAME geometric
    /// structure is judged the same way on a dark ink and on a light one.
    /// With a fixed channel-value threshold this test fails.
    #[test]
    fn the_weighting_does_not_depend_on_how_bright_the_ink_is() {
        let mut weights = Vec::new();
        for v in [12u8, 250u8] {
            let size = 16u32;
            let mut px = vec![0u8; (size * size * 4) as usize];
            for y in 0..size {
                for x in 0..size {
                    let i = ((y * size + x) * 4) as usize;
                    let inside = (4..12).contains(&x) && (4..12).contains(&y);
                    px[i] = v;
                    px[i + 1] = v;
                    px[i + 2] = v;
                    px[i + 3] = if inside { 255 } else { 0 };
                }
            }
            let img = CanonicalImage::from_straight_srgb8(
                size,
                size,
                px,
                true,
                IccAssumption::NoProfileAssumedSrgb,
            )
            .unwrap();
            let (c, img) = confidence(&img);
            weights.push(c.weight(img.index(8, 8)));
        }
        assert!(weights[0] > 0.9 && weights[1] > 0.9, "{weights:?}");
        assert!(
            (weights[0] - weights[1]).abs() < 0.05,
            "a dark and a light core must be judged alike: {weights:?}"
        );
    }

    /// The chamfer transform is a distance: zero on the seeds, monotone
    /// away from them, and within the (1, √2) error of the true Euclidean
    /// distance to the ACTUAL seed set.
    ///
    /// The seed set is not the antialiased rim alone — a pixel one step
    /// inside the rim has a large central difference and is mixed too — so
    /// the reference here is computed by brute force from the transform's
    /// own seeds. Asserting "8 px to the rim" instead would have been
    /// asserting a fact about the fixture, not about the transform.
    #[test]
    fn the_edge_distance_is_zero_on_the_edge_and_grows_inward() {
        let img = square_with_aa(32);
        let (c, img) = confidence(&img);
        assert_eq!(c.edge_distance_px(img.index(8, 16)), 0.0);
        assert!(c.edge_distance_px(img.index(16, 16)) > c.edge_distance_px(img.index(10, 16)));

        let seeds: Vec<(f64, f64)> = (0..32)
            .flat_map(|y| (0..32).map(move |x| (x, y)))
            .filter(|(x, y)| c.is_mixed(img.index(*x, *y)))
            .map(|(x, y)| (f64::from(x), f64::from(y)))
            .collect();
        assert!(!seeds.is_empty());
        let mut worst_ratio = 1.0f64;
        for y in 0..32u32 {
            for x in 0..32u32 {
                let got = c.edge_distance_px(img.index(x, y));
                let truth = seeds
                    .iter()
                    .map(|(sx, sy)| (sx - f64::from(x)).hypot(sy - f64::from(y)))
                    .fold(f64::INFINITY, f64::min);
                assert!(
                    got >= truth - 1e-9,
                    "({x},{y}): chamfer {got} under the true {truth}"
                );
                if truth > 0.0 {
                    worst_ratio = worst_ratio.max(got / truth);
                }
            }
        }
        assert!(
            worst_ratio < 1.09,
            "the (1, √2) chamfer overshoots by {:.1} %",
            100.0 * (worst_ratio - 1.0)
        );
    }

    #[test]
    fn the_summary_counts_what_it_says_it_counts() {
        let img = square_with_aa(20);
        let (c, _) = confidence(&img);
        let s = c.summary();
        assert_eq!(s.pixels, 400);
        assert!(s.mixed_pixels > 0 && s.mixed_pixels < s.pixels);
        assert!(s.high_confidence_pixels > 0);
        assert!(s.pixels_without_rgb_evidence > 0);
        assert!(s.max_weight > 0.9 && s.mean_weight > 0.0);
    }
}
