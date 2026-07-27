//! Palette and exterior hypotheses (spec §9.2).
//!
//! §9.2 asks for SEVERAL hypotheses rather than one pair:
//!
//! - transparent exterior + one opaque foreground;
//! - opaque border-supported background + foreground;
//! - full-bleed two-face scene WITHOUT assuming the border is the
//!   background;
//! - the label-swapped canonical equivalent.
//!
//! and it adds the rule that makes thin shapes honest: *"if a thin shape has
//! no reliable interior core, do not invent a colour from one pixel — use a
//! bounded colour hypothesis interval and let the posterior/abstention
//! decide"*.
//!
//! Two decisions worth stating because they are not the obvious ones.
//!
//! **Colours are estimated from OPAQUE pixels, in linear light.** An opaque
//! pixel stores its paint's own encoded colour whatever blend space the
//! rasterizer used, so the estimate does not depend on the formation
//! hypothesis — which is what lets §9.2 say colours are estimated in linear
//! space but COMPARED through the forward formation hypothesis. Mixed
//! pixels are not colour samples; they are the thing the mixture explains.
//!
//! **The border is evidence, never a conclusion.** `FullBleedTwoFace` and
//! its label-swapped twin exist precisely so that "the border is the
//! background" stays one hypothesis among several. The border statistic is
//! recorded on every hypothesis so a reader can see how much of the claim
//! rests on it.

use serde::Serialize;
use vice_image::{norm, sub, ObservationTensor, CHANNELS};
use vice_ir::color::{linear_to_srgb_encoded, srgb_encoded_to_linear};
use vice_ir::{ExteriorModel, LinearRgb};

use crate::interior::{InteriorConfidence, CORE_WEIGHT};

/// Coefficients of the palette proposal, in 8-bit codes where they are
/// magnitudes and in fractions where they are shares.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PaletteConfig {
    /// Alpha within this many codes of full counts as an OPAQUE sample.
    pub opaque_alpha_codes: f64,
    /// Alpha within this many codes of zero counts as EXTERIOR.
    pub transparent_alpha_codes: f64,
    /// Width of a colour histogram bin, in encoded codes.
    pub bin_codes: u32,
    /// Two modes closer than this (encoded codes, max channel) are the same
    /// mode. Set from the quantization floor of the identifiability rule:
    /// paints closer than a few codes are not distinguishable in the bytes.
    pub min_separation_codes: f64,
    /// A colour cluster below this share of the opaque weight is not a face
    /// of the visible partition; it is antialiasing or noise.
    pub min_mode_share: f64,
    /// Below this much total core weight there is no reliable interior core
    /// and colours become intervals (§9.2).
    pub min_core_weight: f64,
    /// Share of pixels that must be transparent before a transparent
    /// exterior is proposed at all.
    pub min_exterior_share: f64,
}

pub const PALETTE_CONFIG_V1: PaletteConfig = PaletteConfig {
    opaque_alpha_codes: 1.0,
    transparent_alpha_codes: 1.0,
    bin_codes: 4,
    min_separation_codes: 6.0,
    min_mode_share: 0.02,
    min_core_weight: 8.0,
    min_exterior_share: 0.002,
};

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

/// The other side of the mixture: a transparent exterior or an opaque face.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BackgroundHypothesis {
    TransparentExterior,
    OpaqueFace(ColorHypothesis),
}

impl BackgroundHypothesis {
    pub fn exterior_model(&self) -> ExteriorModel {
        match self {
            BackgroundHypothesis::TransparentExterior => ExteriorModel::Transparent,
            BackgroundHypothesis::OpaqueFace(_) => ExteriorModel::Opaque,
        }
    }
}

/// Which of the §9.2 readings a hypothesis is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Flat2Kind {
    TransparentExteriorForeground,
    BorderSupportedBackground,
    FullBleedTwoFace,
    LabelSwappedFullBleed,
    /// Supplied by `--fg/--bg/--exterior`. §9.2 and §30: an oracle override
    /// marks the run NON-PRODUCTION, and the marker travels with the
    /// hypothesis rather than with the command line.
    OracleOverride,
}

impl Flat2Kind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Flat2Kind::TransparentExteriorForeground => "transparent_exterior_foreground",
            Flat2Kind::BorderSupportedBackground => "border_supported_background",
            Flat2Kind::FullBleedTwoFace => "full_bleed_two_face",
            Flat2Kind::LabelSwappedFullBleed => "label_swapped_full_bleed",
            Flat2Kind::OracleOverride => "oracle_override",
        }
    }

    /// True when this hypothesis rests on the assumption that the canvas
    /// border shows the background.
    pub fn assumes_border_is_background(&self) -> bool {
        matches!(self, Flat2Kind::BorderSupportedBackground)
    }

    pub fn is_oracle_override(&self) -> bool {
        matches!(self, Flat2Kind::OracleOverride)
    }
}

/// One Flat2 palette/exterior hypothesis.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Flat2Hypothesis {
    pub id: String,
    pub kind: Flat2Kind,
    pub foreground: ColorHypothesis,
    pub background: BackgroundHypothesis,
    /// Share of BORDER pixels consistent with the background of this
    /// hypothesis. Recorded on every hypothesis, including the ones that do
    /// not use it, so the reader can see what the border evidence would say.
    pub border_support_of_background: f64,
    /// Share of border pixels consistent with the FOREGROUND: a full-bleed
    /// scene whose "foreground" reaches the border is not impossible, and
    /// hiding the number would hide the ambiguity.
    pub border_support_of_foreground: f64,
}

/// A colour mode found among opaque interior samples.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ColorMode {
    pub color: LinearRgb,
    pub weight: f64,
    pub pixels: u64,
    pub border_pixels: u64,
}

/// Why no Flat2 hypothesis could be proposed.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PaletteRefusal {
    #[error(
        "the image shows {modes} distinct opaque colour modes plus {exterior_share:.3} \
         transparent share: more than two visible faces is multicolor (spec 28 M8), not Flat2"
    )]
    MoreThanTwoFaces { modes: usize, exterior_share: f64 },
    #[error("the image shows one uniform face and no exterior: there is no boundary to observe")]
    SingleUniformFace,
    #[error("the image carries no pixel with any alpha: there is nothing to explain")]
    Empty,
}

/// Everything the proposal stage produced.
#[derive(Debug, Clone, PartialEq)]
pub struct Flat2Proposals {
    pub hypotheses: Vec<Flat2Hypothesis>,
    pub modes: Vec<ColorMode>,
    pub exterior_share: f64,
    pub border_transparent_share: f64,
    pub core_weight: f64,
    pub refusal: Option<PaletteRefusal>,
}

fn encode_u8(v: f64) -> u8 {
    (linear_to_srgb_encoded(v.clamp(0.0, 1.0)) * 255.0)
        .round()
        .clamp(0.0, 255.0) as u8
}

fn decode_u8(v: u8) -> f64 {
    srgb_encoded_to_linear(f64::from(v) / 255.0)
}

/// Max-channel distance between two linear colours, in encoded codes — the
/// unit the identifiability floor is calibrated in.
fn separation_codes(a: LinearRgb, b: LinearRgb) -> f64 {
    let d = |x: f64, y: f64| (f64::from(encode_u8(x)) - f64::from(encode_u8(y))).abs();
    d(a.r, b.r).max(d(a.g, b.g)).max(d(a.b, b.b))
}

/// Straight (un-premultiplied) linear colour of an opaque pixel.
fn opaque_color(t: &ObservationTensor, i: usize) -> LinearRgb {
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

/// Weighted colour modes among opaque, high-confidence pixels.
fn find_modes(
    t: &ObservationTensor,
    interior: &InteriorConfidence,
    border: &[usize],
    cfg: &PaletteConfig,
) -> (Vec<ColorMode>, f64) {
    let opaque_floor = 1.0 - cfg.opaque_alpha_codes / 255.0;
    let bin = f64::from(cfg.bin_codes.max(1));
    let mut hist: std::collections::BTreeMap<[u16; 3], (f64, u64, u64)> = Default::default();
    let mut core_weight = 0.0;
    let is_border: std::collections::BTreeSet<usize> = border.iter().copied().collect();
    for i in 0..t.len() {
        if t.alpha(i) < opaque_floor {
            continue;
        }
        let w = interior.weight(i);
        if w <= 0.0 {
            continue;
        }
        if w >= CORE_WEIGHT {
            core_weight += w;
        }
        let c = opaque_color(t, i);
        let key = [
            (f64::from(encode_u8(c.r)) / bin).floor() as u16,
            (f64::from(encode_u8(c.g)) / bin).floor() as u16,
            (f64::from(encode_u8(c.b)) / bin).floor() as u16,
        ];
        let e = hist.entry(key).or_insert((0.0, 0, 0));
        e.0 += w;
        e.1 += 1;
        if is_border.contains(&i) {
            e.2 += 1;
        }
    }
    let total: f64 = hist.values().map(|v| v.0).sum();
    if total <= 0.0 {
        return (Vec::new(), core_weight);
    }
    // Deterministic order: weight descending, then the bin key. No hash
    // iteration and no tie broken by insertion order (§5.5).
    let mut bins: Vec<([u16; 3], (f64, u64, u64))> = hist.into_iter().collect();
    bins.sort_by(|a, b| {
        b.1 .0
            .partial_cmp(&a.1 .0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
    });

    let center_of = |key: [u16; 3]| {
        let mid = |v: u16| {
            let lo = f64::from(v) * bin;
            decode_u8((lo + bin / 2.0).clamp(0.0, 255.0) as u8)
        };
        LinearRgb::new(mid(key[0]), mid(key[1]), mid(key[2]))
    };

    let mut modes: Vec<ColorMode> = Vec::new();
    for (key, (w, px, bpx)) in &bins {
        if w / total < cfg.min_mode_share {
            continue;
        }
        let c = center_of(*key);
        if let Some(existing) = modes
            .iter_mut()
            .find(|m| separation_codes(m.color, c) < cfg.min_separation_codes)
        {
            // Same mode seen through a neighbouring bin: merge by weight,
            // which also refines the colour beyond bin resolution.
            let tw = existing.weight + w;
            existing.color = LinearRgb::new(
                (existing.color.r * existing.weight + c.r * w) / tw,
                (existing.color.g * existing.weight + c.g * w) / tw,
                (existing.color.b * existing.weight + c.b * w) / tw,
            );
            existing.weight = tw;
            existing.pixels += px;
            existing.border_pixels += bpx;
            continue;
        }
        modes.push(ColorMode {
            color: c,
            weight: *w,
            pixels: *px,
            border_pixels: *bpx,
        });
    }
    modes.sort_by(|a, b| {
        b.weight
            .partial_cmp(&a.weight)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                encode_u8(a.color.r)
                    .cmp(&encode_u8(b.color.r))
                    .then(encode_u8(a.color.g).cmp(&encode_u8(b.color.g)))
                    .then(encode_u8(a.color.b).cmp(&encode_u8(b.color.b))),
            )
    });
    (modes, core_weight)
}

/// The bounded colour interval of §9.2, for a shape that is never fully
/// covered over a TRANSPARENT exterior.
///
/// The alpha channel pins the coverage, so the paint is
/// `P_i / α` and the only uncertainty is the 8-bit cell divided by the same
/// α. A 5 %-covered thin stroke therefore yields an interval twenty times
/// the quantization step — which is the honest answer, and the reason §9.2
/// forbids reading a colour off one pixel.
fn coverage_amplified_interval(
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
fn gamut_bounded_interval(
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

fn point(mode: &ColorMode) -> ColorHypothesis {
    ColorHypothesis::Point {
        color: mode.color,
        support_px: mode.pixels,
    }
}

/// Share of border pixels whose colour is within `min_separation_codes` of a
/// candidate colour (and opaque), or transparent for the exterior case.
fn border_share(
    t: &ObservationTensor,
    border: &[usize],
    cfg: &PaletteConfig,
    want: Option<LinearRgb>,
) -> f64 {
    if border.is_empty() {
        return 0.0;
    }
    let opaque_floor = 1.0 - cfg.opaque_alpha_codes / 255.0;
    let transparent_ceiling = cfg.transparent_alpha_codes / 255.0;
    let hits = border
        .iter()
        .filter(|i| match want {
            None => t.alpha(**i) <= transparent_ceiling,
            Some(c) => {
                t.alpha(**i) >= opaque_floor
                    && separation_codes(opaque_color(t, **i), c) < cfg.min_separation_codes
            }
        })
        .count();
    hits as f64 / border.len() as f64
}

/// Propose the §9.2 hypotheses for one image.
pub fn propose_flat2(
    t: &ObservationTensor,
    interior: &InteriorConfidence,
    border: &[usize],
    cfg: &PaletteConfig,
) -> Flat2Proposals {
    let (modes, core_weight) = find_modes(t, interior, border, cfg);
    let transparent_ceiling = cfg.transparent_alpha_codes / 255.0;
    let n = t.len().max(1) as f64;
    let exterior_share = (0..t.len())
        .filter(|i| t.alpha(*i) <= transparent_ceiling)
        .count() as f64
        / n;
    let border_transparent_share = border_share(t, border, cfg, None);
    let has_exterior = exterior_share >= cfg.min_exterior_share;
    let reliable_core = core_weight >= cfg.min_core_weight;

    let mut hypotheses = Vec::new();
    let mut refusal = None;

    let with_border = |fg: LinearRgb, bg: Option<LinearRgb>| {
        (
            border_share(t, border, cfg, bg),
            border_share(t, border, cfg, Some(fg)),
        )
    };

    match (has_exterior, modes.len()) {
        (true, 0) => {
            // A shape exists (something is not transparent) but no opaque
            // core does: the thin-shape case §9.2 is about.
            match coverage_amplified_interval(t, interior, cfg) {
                Some(fg) => {
                    let (bsb, bsf) = with_border(fg.center(), None);
                    hypotheses.push(Flat2Hypothesis {
                        id: "H1/transparent-exterior".to_string(),
                        kind: Flat2Kind::TransparentExteriorForeground,
                        foreground: fg,
                        background: BackgroundHypothesis::TransparentExterior,
                        border_support_of_background: bsb,
                        border_support_of_foreground: bsf,
                    });
                }
                None => refusal = Some(PaletteRefusal::Empty),
            }
        }
        (true, 1) => {
            let fg = if reliable_core {
                point(&modes[0])
            } else {
                coverage_amplified_interval(t, interior, cfg).unwrap_or_else(|| point(&modes[0]))
            };
            let (bsb, bsf) = with_border(fg.center(), None);
            hypotheses.push(Flat2Hypothesis {
                id: "H1/transparent-exterior".to_string(),
                kind: Flat2Kind::TransparentExteriorForeground,
                foreground: fg,
                background: BackgroundHypothesis::TransparentExterior,
                border_support_of_background: bsb,
                border_support_of_foreground: bsf,
            });
        }
        (false, 0) => refusal = Some(PaletteRefusal::Empty),
        (false, 1) => refusal = Some(PaletteRefusal::SingleUniformFace),
        (_, 2) if !has_exterior => {
            // Two opaque faces. THREE readings, and the border decides only
            // the first of them.
            let (a, b) = (modes[0], modes[1]);
            let a_border = border_share(t, border, cfg, Some(a.color));
            let b_border = border_share(t, border, cfg, Some(b.color));
            let (bg, fg) = if b_border >= a_border { (b, a) } else { (a, b) };
            let fg_color = if reliable_core {
                point(&fg)
            } else {
                gamut_bounded_interval(t, bg.color, cfg).unwrap_or_else(|| point(&fg))
            };
            hypotheses.push(Flat2Hypothesis {
                id: "H2/border-supported-background".to_string(),
                kind: Flat2Kind::BorderSupportedBackground,
                foreground: fg_color,
                background: BackgroundHypothesis::OpaqueFace(point(&bg)),
                border_support_of_background: border_share(t, border, cfg, Some(bg.color)),
                border_support_of_foreground: border_share(t, border, cfg, Some(fg.color)),
            });
            // Canonical order by encoded luminance, so the pair of
            // hypotheses below is a function of the colours and not of the
            // border.
            let lum = |c: LinearRgb| 0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b;
            let (first, second) = if lum(a.color) <= lum(b.color) {
                (a, b)
            } else {
                (b, a)
            };
            for (kind, f, g, id) in [
                (
                    Flat2Kind::FullBleedTwoFace,
                    first,
                    second,
                    "H3/full-bleed-two-face",
                ),
                (
                    Flat2Kind::LabelSwappedFullBleed,
                    second,
                    first,
                    "H4/label-swapped",
                ),
            ] {
                hypotheses.push(Flat2Hypothesis {
                    id: id.to_string(),
                    kind,
                    foreground: point(&f),
                    background: BackgroundHypothesis::OpaqueFace(point(&g)),
                    border_support_of_background: border_share(t, border, cfg, Some(g.color)),
                    border_support_of_foreground: border_share(t, border, cfg, Some(f.color)),
                });
            }
        }
        (_, modes_len) => {
            refusal = Some(PaletteRefusal::MoreThanTwoFaces {
                modes: modes_len,
                exterior_share,
            })
        }
    }

    Flat2Proposals {
        hypotheses,
        modes,
        exterior_share,
        border_transparent_share,
        core_weight,
        refusal,
    }
}

/// Build the single hypothesis an oracle override names (spec §9.2, §30:
/// `--fg/--bg/--exterior` are diagnostic and mark the run NON-PRODUCTION).
pub fn oracle_override(foreground: LinearRgb, background: Option<LinearRgb>) -> Flat2Hypothesis {
    Flat2Hypothesis {
        id: "H0/oracle-override".to_string(),
        kind: Flat2Kind::OracleOverride,
        foreground: ColorHypothesis::Point {
            color: foreground,
            support_px: 0,
        },
        background: match background {
            None => BackgroundHypothesis::TransparentExterior,
            Some(c) => BackgroundHypothesis::OpaqueFace(ColorHypothesis::Point {
                color: c,
                support_px: 0,
            }),
        },
        border_support_of_background: 0.0,
        border_support_of_foreground: 0.0,
    }
}

/// `‖P_f − P_b‖` for one hypothesis under one blend space: the conditioning
/// of the mixture (§10).
pub fn conditioning(h: &Flat2Hypothesis, blend: vice_ir::BlendSpace) -> f64 {
    let pf = vice_image::paint_observation_premul(h.foreground.center(), blend);
    let pb = match h.background {
        BackgroundHypothesis::TransparentExterior => vice_image::TRANSPARENT_EXTERIOR_PREMUL,
        BackgroundHypothesis::OpaqueFace(c) => {
            vice_image::paint_observation_premul(c.center(), blend)
        }
    };
    let mut acc = 0.0;
    let d = sub(pf, pb);
    for v in d.iter().take(CHANNELS) {
        acc += v * v;
    }
    acc.sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interior::{interior_confidence, INTERIOR_CONFIG_V1};
    use vice_image::{CanonicalImage, IccAssumption};
    use vice_ir::BlendSpace;

    fn img_from(size: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> CanonicalImage {
        let mut px = vec![0u8; (size * size * 4) as usize];
        for y in 0..size {
            for x in 0..size {
                let i = ((y * size + x) * 4) as usize;
                px[i..i + 4].copy_from_slice(&f(x, y));
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

    fn propose(img: &CanonicalImage) -> Flat2Proposals {
        let t = ObservationTensor::of(img, BlendSpace::LinearLight);
        let interior = interior_confidence(&t, &INTERIOR_CONFIG_V1);
        propose_flat2(&t, &interior, &img.border_indices(), &PALETTE_CONFIG_V1)
    }

    /// A disc of ink on a transparent exterior: ONE hypothesis, transparent
    /// exterior, colour a point because the core is reliable.
    #[test]
    fn ink_on_a_transparent_exterior_proposes_the_transparent_reading() {
        let img = img_from(32, |x, y| {
            let inside = (f64::from(x) - 15.5).hypot(f64::from(y) - 15.5) < 9.0;
            if inside {
                [40, 130, 210, 255]
            } else {
                [0, 0, 0, 0]
            }
        });
        let p = propose(&img);
        assert!(p.refusal.is_none(), "{:?}", p.refusal);
        assert_eq!(p.hypotheses.len(), 1);
        let h = &p.hypotheses[0];
        assert_eq!(h.kind, Flat2Kind::TransparentExteriorForeground);
        assert_eq!(h.background, BackgroundHypothesis::TransparentExterior);
        assert!(!h.foreground.is_interval(), "a solid core is a point");
        assert!(
            separation_codes(
                h.foreground.center(),
                LinearRgb::new(decode_u8(40), decode_u8(130), decode_u8(210))
            ) <= 2.0,
            "recovered {:?}",
            h.foreground.center()
        );
        assert!(
            h.border_support_of_background > 0.99,
            "the border is exterior"
        );
        assert!(p.exterior_share > 0.5);
    }

    /// Two opaque faces: FOUR readings are proposed, and exactly one of them
    /// is the border-supported one. §9.2 asks for the full-bleed reading and
    /// its label-swapped twin PRECISELY so that "the border is the
    /// background" stays a hypothesis.
    #[test]
    fn two_opaque_faces_propose_the_border_reading_and_both_label_orders() {
        let img = img_from(32, |x, y| {
            let inside = (8..24).contains(&x) && (8..24).contains(&y);
            if inside {
                [230, 40, 40, 255]
            } else {
                [20, 20, 60, 255]
            }
        });
        let p = propose(&img);
        assert!(p.refusal.is_none(), "{:?}", p.refusal);
        assert_eq!(p.modes.len(), 2, "modes {:?}", p.modes);
        let kinds: Vec<Flat2Kind> = p.hypotheses.iter().map(|h| h.kind).collect();
        assert_eq!(
            kinds,
            vec![
                Flat2Kind::BorderSupportedBackground,
                Flat2Kind::FullBleedTwoFace,
                Flat2Kind::LabelSwappedFullBleed
            ]
        );
        let border = &p.hypotheses[0];
        assert!(border.kind.assumes_border_is_background());
        assert!(border.border_support_of_background > 0.99);
        assert!(border.border_support_of_foreground < 0.01);
        // The two full-bleed readings are exact label swaps of each other.
        let (a, b) = (&p.hypotheses[1], &p.hypotheses[2]);
        assert_eq!(
            a.foreground.center(),
            match b.background {
                BackgroundHypothesis::OpaqueFace(c) => c.center(),
                _ => panic!("a full-bleed reading has an opaque background"),
            }
        );
        assert!(!a.kind.assumes_border_is_background());
        // And the canonical order is a function of the COLOURS, not of the
        // border: the darker face is the foreground of H3 whichever one the
        // border shows.
        assert!(a.foreground.center().g < b.foreground.center().g);
    }

    /// §9.2's thin-shape rule: no reliable core, so the colour is a bounded
    /// INTERVAL whose width is the quantization divided by the largest
    /// coverage — not a value read off one pixel.
    #[test]
    fn a_thin_shape_with_no_core_yields_a_bounded_interval_not_a_colour() {
        // A one-pixel-wide diagonal stroke that never reaches full coverage.
        let img = img_from(24, |x, y| {
            if x == y {
                [200, 60, 30, 77] // α ≈ 0.3
            } else {
                [0, 0, 0, 0]
            }
        });
        let p = propose(&img);
        assert!(p.refusal.is_none(), "{:?}", p.refusal);
        assert_eq!(p.hypotheses.len(), 1);
        let fg = p.hypotheses[0].foreground;
        assert!(fg.is_interval(), "no core must not become a point: {fg:?}");
        match fg {
            ColorHypothesis::Interval {
                reason, halfwidth, ..
            } => {
                assert_eq!(reason, IntervalReason::QuantizationAmplifiedByCoverage);
                // 1/0.3 of a quantization cell is far wider than one code.
                assert!(halfwidth > 1.0 / 255.0, "halfwidth {halfwidth}");
                assert!(halfwidth < 0.25, "halfwidth {halfwidth}");
            }
            other => panic!("{other:?}"),
        }
        assert!(p.core_weight < PALETTE_CONFIG_V1.min_core_weight);
    }

    /// Three visible faces are not Flat2. The refusal names the milestone
    /// that owns them rather than silently dropping the third colour.
    #[test]
    fn three_opaque_faces_are_refused_as_multicolor() {
        let img = img_from(32, |x, _| {
            if x < 10 {
                [240, 10, 10, 255]
            } else if x < 20 {
                [10, 240, 10, 255]
            } else {
                [10, 10, 240, 255]
            }
        });
        let p = propose(&img);
        match p.refusal {
            Some(PaletteRefusal::MoreThanTwoFaces { modes, .. }) => assert!(modes >= 3),
            other => panic!("{other:?}"),
        }
        assert!(p.hypotheses.is_empty(), "a refusal proposes nothing");
    }

    #[test]
    fn a_uniform_image_has_no_boundary_and_says_so() {
        let img = img_from(8, |_, _| [100, 100, 100, 255]);
        assert!(matches!(
            propose(&img).refusal,
            Some(PaletteRefusal::SingleUniformFace)
        ));
        let empty = img_from(8, |_, _| [0, 0, 0, 0]);
        assert!(matches!(
            propose(&empty).refusal,
            Some(PaletteRefusal::Empty)
        ));
    }

    /// The conditioning of §10 is the separation the mixture divides by, and
    /// it collapses exactly when the two faces become indistinguishable.
    #[test]
    fn conditioning_measures_the_separation_the_mixture_divides_by() {
        let far = oracle_override(
            LinearRgb::new(1.0, 1.0, 1.0),
            Some(LinearRgb::new(0.0, 0.0, 0.0)),
        );
        let near = oracle_override(
            LinearRgb::new(0.5, 0.5, 0.5),
            Some(LinearRgb::new(0.5, 0.5, 0.501)),
        );
        for blend in [BlendSpace::LinearLight, BlendSpace::EncodedSrgb] {
            assert!(conditioning(&far, blend) > 1.0);
            assert!(conditioning(&near, blend) < 0.01);
        }
        // A transparent exterior is well conditioned even for a dark ink,
        // because the alpha component alone separates the two ends.
        let dark = oracle_override(LinearRgb::new(0.0, 0.0, 0.0), None);
        assert!((conditioning(&dark, BlendSpace::LinearLight) - 1.0).abs() < 1e-12);
        assert!(dark.kind.is_oracle_override());
    }
}
