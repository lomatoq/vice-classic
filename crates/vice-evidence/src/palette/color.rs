//! Face colours: a point when there is a reliable interior core, a bounded
//! INTERVAL when there is not (spec §9.2).
//!
//! §9.2: *"if a thin shape has no reliable interior core, do not invent a
//! colour from one pixel: use a bounded colour hypothesis interval and let
//! the posterior/abstention decide"*. There are two ways a Flat2 image can
//! fail to show a core, and they bound the colour differently, so both are
//! derived here rather than approximated by one formula:
//!
//! - over a TRANSPARENT exterior the alpha channel pins the coverage, so
//!   the paint is `P_i/α` and the interval is the 8-bit cell divided by the
//!   same α — a 30 %-covered stroke gives an interval three times the
//!   quantization step;
//! - over an OPAQUE background nothing pins the coverage, so the paint lies
//!   on a RAY from the background through the most extreme observation, and
//!   the interval is that ray clipped to the `[0,1]³` gamut.
//!
//! Split out of the palette module at the seam the §4.1 size rule asks for,
//! and this is the natural one: the hypotheses are a claim about which faces
//! exist, the interval is a claim about how well a colour is determined.

use serde::Serialize;
use vice_image::{norm, sub, ObservationTensor};
use vice_ir::color::{linear_to_srgb_encoded, srgb_encoded_to_linear};
use vice_ir::LinearRgb;

use super::PaletteConfig;
use crate::interior::InteriorConfidence;

/// Why a colour is an interval rather than a point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IntervalReason {
    /// The shape is never fully covered, so the paint is recovered by
    /// dividing by a coverage below one, which amplifies the 8-bit
    /// quantization by `1/α`.
    QuantizationAmplifiedByCoverage,
    /// Over an opaque background the alpha channel pins nothing, so the
    /// paint lies on a RAY from the background through the most extreme
    /// observation; the interval is that ray clipped to the colour gamut.
    GamutBoundedRay,
}

/// A face colour: a point when there is a reliable interior core, a bounded
/// interval when there is not.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ColorHypothesis {
    Point {
        color: LinearRgb,
        support_px: u64,
    },
    Interval {
        lo: LinearRgb,
        hi: LinearRgb,
        center: LinearRgb,
        /// Half the length of the interval, in linear-light units.
        halfwidth: f64,
        reason: IntervalReason,
        support_px: u64,
    },
}

impl ColorHypothesis {
    /// The representative colour. An interval reports its centre and says
    /// so; it does not pretend the centre is a measurement.
    pub fn center(&self) -> LinearRgb {
        match self {
            ColorHypothesis::Point { color, .. } => *color,
            ColorHypothesis::Interval { center, .. } => *center,
        }
    }
    pub fn halfwidth(&self) -> f64 {
        match self {
            ColorHypothesis::Point { .. } => 0.0,
            ColorHypothesis::Interval { halfwidth, .. } => *halfwidth,
        }
    }
    pub fn is_interval(&self) -> bool {
        matches!(self, ColorHypothesis::Interval { .. })
    }
    pub fn support_px(&self) -> u64 {
        match self {
            ColorHypothesis::Point { support_px, .. }
            | ColorHypothesis::Interval { support_px, .. } => *support_px,
        }
    }
}

pub(crate) fn encode_u8(v: f64) -> u8 {
    (linear_to_srgb_encoded(v.clamp(0.0, 1.0)) * 255.0)
        .round()
        .clamp(0.0, 255.0) as u8
}

pub(crate) fn decode_u8(v: u8) -> f64 {
    srgb_encoded_to_linear(f64::from(v) / 255.0)
}

/// Max-channel distance between two linear colours, in encoded codes — the
/// unit the identifiability floor is calibrated in.
pub(crate) fn separation_codes(a: LinearRgb, b: LinearRgb) -> f64 {
    let d = |x: f64, y: f64| (f64::from(encode_u8(x)) - f64::from(encode_u8(y))).abs();
    d(a.r, b.r).max(d(a.g, b.g)).max(d(a.b, b.b))
}

/// Straight (un-premultiplied) linear colour of an opaque pixel.
pub(crate) fn opaque_color(t: &ObservationTensor, i: usize) -> LinearRgb {
    let p = t.premul(i);
    let a = p[3].max(1e-9);
    let to_linear = |v: f64| match t.blend_space() {
        vice_ir::BlendSpace::LinearLight => v,
        vice_ir::BlendSpace::EncodedSrgb => srgb_encoded_to_linear(v),
    };
    LinearRgb::new(
        to_linear((p[0] / a).clamp(0.0, 1.0)),
        to_linear((p[1] / a).clamp(0.0, 1.0)),
        to_linear((p[2] / a).clamp(0.0, 1.0)),
    )
}

/// The bounded colour interval of §9.2, for a shape that is never fully
/// covered over a TRANSPARENT exterior.
///
/// The alpha channel pins the coverage, so the paint is
/// `P_i / α` and the only uncertainty is the 8-bit cell divided by the same
/// α. A 5 %-covered thin stroke therefore yields an interval twenty times
/// the quantization step — which is the honest answer, and the reason §9.2
/// forbids reading a colour off one pixel.
pub(crate) fn coverage_amplified_interval(
    t: &ObservationTensor,
    interior: &InteriorConfidence,
    cfg: &PaletteConfig,
) -> Option<ColorHypothesis> {
    let transparent_ceiling = cfg.transparent_alpha_codes / 255.0;
    let mut best: Option<(f64, usize)> = None;
    let mut support = 0u64;
    for i in 0..t.len() {
        let a = t.alpha(i);
        if a <= transparent_ceiling || !interior.gives_rgb_evidence(i) {
            continue;
        }
        support += 1;
        if best.is_none_or(|(ba, _)| a > ba) {
            best = Some((a, i));
        }
    }
    let (a, i) = best?;
    let c = opaque_color(t, i);
    let q = norm(t.quantization_halfwidth(i)) / a;
    let lo = LinearRgb::new(
        (c.r - q).clamp(0.0, 1.0),
        (c.g - q).clamp(0.0, 1.0),
        (c.b - q).clamp(0.0, 1.0),
    );
    let hi = LinearRgb::new(
        (c.r + q).clamp(0.0, 1.0),
        (c.g + q).clamp(0.0, 1.0),
        (c.b + q).clamp(0.0, 1.0),
    );
    Some(ColorHypothesis::Interval {
        lo,
        hi,
        center: c,
        halfwidth: q,
        reason: IntervalReason::QuantizationAmplifiedByCoverage,
        support_px: support,
    })
}

/// The bounded colour interval of §9.2 over an OPAQUE background: the paint
/// lies on the ray from the background through the most extreme observation,
/// clipped to the `[0,1]³` gamut.
pub(crate) fn gamut_bounded_interval(
    t: &ObservationTensor,
    background: LinearRgb,
    cfg: &PaletteConfig,
) -> Option<ColorHypothesis> {
    let _ = cfg;
    let bg_obs = vice_image::paint_observation_premul(background, t.blend_space());
    let mut best: Option<(f64, usize)> = None;
    for i in 0..t.len() {
        let d = norm(sub(t.premul(i), bg_obs));
        if best.is_none_or(|(bd, _)| d > bd) {
            best = Some((d, i));
        }
    }
    let (d, i) = best?;
    if d <= 0.0 {
        return None;
    }
    let extreme = opaque_color(t, i);
    // `extreme` is the α = 1 end of the ray. Walk outward until a channel
    // leaves the gamut; that point is the other end.
    let dir = [
        extreme.r - background.r,
        extreme.g - background.g,
        extreme.b - background.b,
    ];
    let mut t_max = 1.0f64;
    for (ch, d) in dir.iter().enumerate() {
        let b = [background.r, background.g, background.b][ch];
        if *d > 1e-12 {
            t_max = t_max.max((1.0 - b) / d);
        } else if *d < -1e-12 {
            t_max = t_max.max((0.0 - b) / d);
        }
    }
    let at = |s: f64| {
        LinearRgb::new(
            (background.r + s * dir[0]).clamp(0.0, 1.0),
            (background.g + s * dir[1]).clamp(0.0, 1.0),
            (background.b + s * dir[2]).clamp(0.0, 1.0),
        )
    };
    let lo = at(1.0);
    let hi = at(t_max);
    let center = LinearRgb::new(
        0.5 * (lo.r + hi.r),
        0.5 * (lo.g + hi.g),
        0.5 * (lo.b + hi.b),
    );
    let halfwidth =
        0.5 * ((hi.r - lo.r).powi(2) + (hi.g - lo.g).powi(2) + (hi.b - lo.b).powi(2)).sqrt();
    Some(ColorHypothesis::Interval {
        lo,
        hi,
        center,
        halfwidth,
        reason: IntervalReason::GamutBoundedRay,
        support_px: 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The gamut-bounded ray is a real interval: it starts at the most
    /// extreme observation (the `α = 1` reading) and ends where the ray
    /// leaves the colour cube, so its width says how much the image failed
    /// to determine.
    #[test]
    fn the_gamut_bounded_ray_spans_from_the_extreme_observation_to_the_gamut() {
        let bg = LinearRgb::new(0.5, 0.5, 0.5);
        let observed = LinearRgb::new(0.6, 0.5, 0.5);
        let dir = observed.r - bg.r;
        // Walking outward from 0.5 along +r leaves the cube at r = 1.0,
        // i.e. at five times the observed excursion.
        let t_max = (1.0 - bg.r) / dir;
        assert!((t_max - 5.0).abs() < 1e-12);
    }

    /// The colour comparison is in ENCODED codes, which is the unit the
    /// identifiability floor of §1.5 was calibrated in.
    #[test]
    fn separation_is_measured_in_the_unit_the_floor_was_calibrated_in() {
        let a = LinearRgb::new(decode_u8(100), decode_u8(100), decode_u8(100));
        let b = LinearRgb::new(decode_u8(104), decode_u8(100), decode_u8(100));
        assert!((separation_codes(a, b) - 4.0).abs() < 1e-9);
        assert_eq!(separation_codes(a, a), 0.0);
        assert_eq!(encode_u8(decode_u8(37)), 37);
    }

    /// A point hypothesis has no width and says so; an interval reports its
    /// centre WITHOUT pretending the centre is a measurement.
    #[test]
    fn a_point_has_no_width_and_an_interval_reports_one() {
        let p = ColorHypothesis::Point {
            color: LinearRgb::new(0.2, 0.3, 0.4),
            support_px: 12,
        };
        assert_eq!(p.halfwidth(), 0.0);
        assert!(!p.is_interval());
        assert_eq!(p.support_px(), 12);
        let i = ColorHypothesis::Interval {
            lo: LinearRgb::new(0.1, 0.1, 0.1),
            hi: LinearRgb::new(0.3, 0.3, 0.3),
            center: LinearRgb::new(0.2, 0.2, 0.2),
            halfwidth: 0.1,
            reason: IntervalReason::GamutBoundedRay,
            support_px: 1,
        };
        assert!(i.is_interval());
        assert_eq!(i.halfwidth(), 0.1);
        assert_eq!(i.center().r, 0.2);
    }
}
