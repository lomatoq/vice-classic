//! The M4 Flat2 analysis: hypotheses in, a typed outcome out
//! (spec §7, §9.2, §10, §1.4, §1.6, §28 M4).
//!
//! This stage stops at M4 evidence: no topology envelope, fitter, or
//! confidence. Label-swapped hypotheses share an alpha field up to
//! `α ↔ 1−α` and byte-identical residuals, so they form one mixture class;
//! ambiguity is judged between physically different classes.
//!
//! - **Supported** — exactly one mixture class explains the pixels;
//! - **Ambiguous** — two or more physically different classes do;
//! - **Unsupported** — none does, or every class that does shows an
//!   interior fill with a true constant `0 < α < 1`, which §1.6 removes
//!   from Flat2 v1 outright.
//!
//! `Unsupported` prevents the silent reading of a constant partial-alpha fill
//! as partial geometric coverage—the exact failure §1.6 names.

use serde::Serialize;
use vice_image::{CanonicalImage, ObservationTensor};
use vice_ir::color::linear_to_srgb_encoded;
use vice_ir::{BlendSpace, LinearRgb, PixelFilter};

use crate::boundary::{
    contour_length_px, observe_boundaries, BoundaryConfig, BoundaryObservation, BOUNDARY_CONFIG_V1,
};
use crate::corridor::{CorridorConfig, CORRIDOR_CONFIG_V1};
use crate::formation::{
    blend_space_is_identifiable, filter_is_identifiable, filter_penalty, for_palette, formation_id,
    resolved_fraction, transition_width_px, PIXEL_FILTERS,
};
use crate::interior::{interior_confidence, InteriorConfig, InteriorSummary, INTERIOR_CONFIG_V1};
use crate::mixture::{
    infer_mixture, Flat2Evidence, MixtureConfig, SemiTransparentInterior, MIXTURE_CONFIG_V1,
};
use crate::palette::{
    propose_flat2, BackgroundHypothesis, ColorHypothesis, Flat2Hypothesis, Flat2Kind,
    PaletteConfig, PALETTE_CONFIG_V1,
};
use crate::support::{SurrogateSummary, NOT_A_LIKELIHOOD};

pub const ANALYSIS_SCHEMA: &str = "vice-classic/m4-flat2-evidence/v1";

/// Largest residual, in 8-bit codes at the 95th percentile, that a
/// hypothesis may leave and still count as EXPLAINING the image.
///
/// Measured in both directions on the corpus by `vice-bench::corridor`
/// (`the_residual_tolerance_separates_right_from_wrong_hypotheses`): the
/// correct hypothesis on an independently rasterized clean render leaves a
/// few codes, and a wrong palette or blend space leaves an order of
/// magnitude more.
pub const MAX_RESIDUAL_P95_CODES: f64 = 12.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct AnalysisConfig {
    pub interior: InteriorConfig,
    pub palette: PaletteConfig,
    pub mixture: MixtureConfig,
    pub boundary: BoundaryConfig,
    pub corridor: CorridorConfig,
    /// Which corridor level the reported halfwidths belong to.
    pub coverage_level: f64,
    pub max_residual_p95_codes: f64,
}

pub const ANALYSIS_CONFIG_V1: AnalysisConfig = AnalysisConfig {
    interior: INTERIOR_CONFIG_V1,
    palette: PALETTE_CONFIG_V1,
    mixture: MIXTURE_CONFIG_V1,
    boundary: BOUNDARY_CONFIG_V1,
    corridor: CORRIDOR_CONFIG_V1,
    coverage_level: 0.95,
    max_residual_p95_codes: MAX_RESIDUAL_P95_CODES,
};

/// What the decode knows about the input (spec §8.1).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImageFacts {
    pub width_px: u32,
    pub height_px: u32,
    pub source_sha256: String,
    pub source_had_alpha: bool,
    pub icc_assumption: &'static str,
    pub icc_was_assumed: bool,
}

/// One (palette, formation) pair, as published.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EvidenceSummary {
    pub id: String,
    pub palette_kind: &'static str,
    pub formation: String,
    pub mixture_class: String,
    pub conditioning: f64,
    pub transition_width_px: f64,
    pub contour_length_px: f64,
    pub filter_penalty: f64,
    pub residual: crate::mixture::ResidualIndicators,
    pub score: f64,
    pub blend_space_identifiable: bool,
    /// False when the shape is thinner than the kernel, so the transition
    /// width measures the shape and not the filter.
    pub filter_identifiable: bool,
    pub resolved_fraction: f64,
    pub foreground_is_interval: bool,
    pub semi_transparent_interior: Option<SemiTransparentInterior>,
    pub fit: SurrogateSummary,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RefusedPair {
    pub palette: String,
    pub formation: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UnsupportedReason {
    /// §1.6: an interior fill with a true constant `0 < α < 1`.
    SemiTransparentInterior {
        detail: SemiTransparentInterior,
        classes: Vec<String>,
        note: &'static str,
    },
    /// No palette hypothesis could be proposed at all.
    Palette { detail: String },
    /// Hypotheses existed, none explained the pixels.
    NoHypothesisExplains {
        best_residual_p95_codes: f64,
        tolerance_codes: f64,
    },
    /// Every pair was refused before a residual existed.
    NoWellConditionedPair { refusals: usize },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Flat2Outcome {
    Supported {
        evidence_id: String,
        mixture_class: String,
        /// Formations that cannot be told apart from the chosen one.
        ///
        /// §5.4: a deterministic tie-break may not erase an ambiguity. Over
        /// a transparent exterior the two blend spaces predict identical
        /// bytes, so BOTH are listed — and the reason is structural, not a
        /// numerical near-tie that rounding could break.
        tied_formations: Vec<String>,
    },
    Ambiguous {
        mixture_classes: Vec<String>,
        note: &'static str,
    },
    Unsupported(UnsupportedReason),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Flat2Analysis {
    pub schema: &'static str,
    pub image: ImageFacts,
    pub interior: InteriorSummary,
    pub palette_modes: usize,
    pub exterior_share: f64,
    pub border_transparent_share: f64,
    pub core_weight: f64,
    pub hypotheses: Vec<String>,
    pub evidences: Vec<EvidenceSummary>,
    pub refused: Vec<RefusedPair>,
    pub outcome: Flat2Outcome,
    pub boundary: Option<BoundaryObservation>,
    pub boundary_refusal: Option<String>,
    /// False when an oracle override (`--fg/--bg/--exterior`) was used:
    /// §30 says such a run is NOT production, and the flag travels with the
    /// artifact rather than with the command line.
    pub production: bool,
    /// The sentence every surrogate in this report carries (§10.2).
    pub evidence_is_not_a_likelihood: &'static str,
}

fn encoded_key(c: LinearRgb) -> [u8; 3] {
    let e = |v: f64| (linear_to_srgb_encoded(v.clamp(0.0, 1.0)) * 255.0).round() as u8;
    [e(c.r), e(c.g), e(c.b)]
}

/// The mixture class of a hypothesis: the two faces, whichever way round,
/// plus the exterior model. Two hypotheses in one class have the same alpha
/// field up to `α ↔ 1−α` and identical residuals.
fn mixture_class(h: &Flat2Hypothesis) -> String {
    let fg = encoded_key(h.foreground.center());
    match h.background {
        BackgroundHypothesis::TransparentExterior => {
            format!("transparent:{:02x}{:02x}{:02x}", fg[0], fg[1], fg[2])
        }
        BackgroundHypothesis::OpaqueFace(c) => {
            let bg = encoded_key(c.center());
            let (a, b) = if fg <= bg { (fg, bg) } else { (bg, fg) };
            format!(
                "opaque:{:02x}{:02x}{:02x}-{:02x}{:02x}{:02x}",
                a[0], a[1], a[2], b[0], b[1], b[2]
            )
        }
    }
}

/// The report, plus the evidence the outcome selected.
///
/// Separate from [`analyze`] because a `Flat2Evidence` carries a residual
/// VECTOR per pixel — megabytes on a large image — and only the calibration
/// harness needs it (to re-observe the boundary at the other coverage
/// levels of §13.1). Everything a report needs is in [`Flat2Analysis`].
pub struct AnalysisOutput {
    pub report: Flat2Analysis,
    pub chosen: Option<Flat2Evidence>,
}

/// Run the M4 evidence stage on one image.
pub fn analyze(
    img: &CanonicalImage,
    cfg: &AnalysisConfig,
    override_hypothesis: Option<Flat2Hypothesis>,
) -> Flat2Analysis {
    analyze_full(img, cfg, override_hypothesis).report
}

/// [`analyze`], keeping the chosen evidence.
pub fn analyze_full(
    img: &CanonicalImage,
    cfg: &AnalysisConfig,
    override_hypothesis: Option<Flat2Hypothesis>,
) -> AnalysisOutput {
    analyze_full_for_filters(img, cfg, override_hypothesis, PIXEL_FILTERS)
}

/// [`analyze_full`] restricted to filters admitted by the caller's universe.
/// Filtering happens before surrogate selection (§8.2), so an unsupported M4
/// winner cannot suppress a supported M7 explanation.
pub fn analyze_full_for_filters(
    img: &CanonicalImage,
    cfg: &AnalysisConfig,
    override_hypothesis: Option<Flat2Hypothesis>,
    supported_filters: &[PixelFilter],
) -> AnalysisOutput {
    let linear = ObservationTensor::of(img, BlendSpace::LinearLight);
    let encoded = ObservationTensor::of(img, BlendSpace::EncodedSrgb);
    // Interior confidence and the palette are read off the LINEAR tensor:
    // both use OPAQUE pixels, whose stored colour is the paint's own
    // whatever space the rasterizer blended in, so this choice cannot bias
    // the blend-space hypothesis (palette module docs).
    let interior = interior_confidence(&linear, &cfg.interior);
    let border = img.border_indices();
    let proposals = propose_flat2(&linear, &interior, &border, &cfg.palette);

    let hypotheses: Vec<Flat2Hypothesis> = match &override_hypothesis {
        Some(h) => vec![h.clone()],
        None => proposals.hypotheses.clone(),
    };
    let production = override_hypothesis.is_none();

    let facts = ImageFacts {
        width_px: img.width_px(),
        height_px: img.height_px(),
        source_sha256: img.source_sha256().to_string(),
        source_had_alpha: img.source_had_alpha(),
        icc_assumption: img.icc_assumption().as_str(),
        icc_was_assumed: img.icc_assumption().is_assumed(),
    };

    let mut evidences: Vec<(Flat2Evidence, EvidenceSummary)> = Vec::new();
    let mut refused = Vec::new();
    for h in &hypotheses {
        for f in for_palette(h)
            .into_iter()
            .filter(|formation| supported_filters.contains(&formation.pixel_filter))
        {
            let t = match f.blend_space {
                BlendSpace::LinearLight => &linear,
                BlendSpace::EncodedSrgb => &encoded,
            };
            match infer_mixture(t, h, &f, img.source_sha256(), &cfg.mixture) {
                Err(e) => refused.push(RefusedPair {
                    palette: h.id.clone(),
                    formation: formation_id(&f),
                    reason: e.to_string(),
                }),
                Ok(ev) => {
                    let length = contour_length_px(
                        ev.alpha_field(),
                        ev.width_px() as usize,
                        ev.height_px() as usize,
                        cfg.boundary.level,
                    );
                    let width = transition_width_px(ev.alpha_field(), length);
                    // A kernel the coverage field cannot distinguish costs
                    // nothing, so every filter ties and the tie is REPORTED
                    // (§5.4) instead of being resolved by a statistic that
                    // is measuring the shape.
                    let filter_identifiable = filter_is_identifiable(ev.alpha_field());
                    let fp = if filter_identifiable {
                        filter_penalty(f.pixel_filter, width)
                    } else {
                        0.0
                    };
                    let residual_z = ev.indicators.p95_abs_codes
                        / crate::corridor::CLEAN_BUCKET_SIGMA_CODES.max(1e-9);
                    let summary = EvidenceSummary {
                        id: ev.id(),
                        palette_kind: h.kind.as_str(),
                        formation: formation_id(&f),
                        mixture_class: mixture_class(h),
                        conditioning: ev.conditioning,
                        transition_width_px: width,
                        contour_length_px: length,
                        filter_penalty: fp,
                        residual: ev.indicators.clone(),
                        score: fp + 0.5 * residual_z * residual_z,
                        blend_space_identifiable: blend_space_is_identifiable(h),
                        filter_identifiable,
                        resolved_fraction: resolved_fraction(ev.alpha_field()),
                        foreground_is_interval: matches!(
                            h.foreground,
                            ColorHypothesis::Interval { .. }
                        ),
                        semi_transparent_interior: ev.semi_transparent_interior.clone(),
                        fit: ev.fit.summary(),
                    };
                    evidences.push((ev, summary));
                }
            }
        }
    }

    // Best evidence per mixture class, deterministic on ties.
    let mut classes: Vec<String> = evidences
        .iter()
        .map(|(_, s)| s.mixture_class.clone())
        .collect();
    classes.sort();
    classes.dedup();
    let best_of = |class: &str| -> Option<usize> {
        evidences
            .iter()
            .enumerate()
            .filter(|(_, (_, s))| s.mixture_class == class)
            .min_by(|a, b| {
                a.1 .1
                    .score
                    .partial_cmp(&b.1 .1.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.1 .1.id.cmp(&b.1 .1.id))
            })
            .map(|(i, _)| i)
    };

    let mut explaining: Vec<(String, usize)> = Vec::new();
    let mut semi: Vec<(String, SemiTransparentInterior, bool)> = Vec::new();
    let mut best_residual = f64::INFINITY;
    for class in &classes {
        let Some(i) = best_of(class) else { continue };
        let s = &evidences[i].1;
        best_residual = best_residual.min(s.residual.p95_abs_codes);
        if let Some(d) = &s.semi_transparent_interior {
            let thick = d.thickness_px >= cfg.mixture.min_flat_thickness_px;
            semi.push((class.clone(), d.clone(), thick));
            continue;
        }
        if s.residual.p95_abs_codes <= cfg.max_residual_p95_codes {
            explaining.push((class.clone(), i));
        }
    }

    // An oracle override supplies its own hypothesis, so a palette refusal
    // does not apply to it: the run is diagnostic and the caller asked for
    // exactly this pair. Found by
    // `vice-bench::corridor::tests::the_residual_tolerance_separates_right_from_wrong_hypotheses`,
    // which overrides the palette on an image whose own proposal refuses -
    // the first version reached an `unreachable!` there.
    let palette_refusal = match (&proposals.refusal, &override_hypothesis) {
        (Some(r), None) => Some(r.to_string()),
        _ => None,
    };
    let outcome = if let Some(detail) = palette_refusal {
        Flat2Outcome::Unsupported(UnsupportedReason::Palette { detail })
    } else if evidences.is_empty() {
        Flat2Outcome::Unsupported(UnsupportedReason::NoWellConditionedPair {
            refusals: refused.len(),
        })
    } else if explaining.is_empty() {
        if let Some((_, d, thick_enough)) = semi.first() {
            if *thick_enough {
                Flat2Outcome::Unsupported(UnsupportedReason::SemiTransparentInterior {
                    detail: d.clone(),
                    classes: semi.iter().map(|(c, _, _)| c.clone()).collect(),
                    note: "spec 1.6: Flat2 v1 does not support an interior fill with a true \
                           constant 0 < alpha < 1; reading it as coverage 0.5 everywhere would be \
                           the failure that clause names",
                })
            } else {
                // The flat intermediate region is THINNER than the kernel
                // can resolve, and there the two readings are the same
                // bytes: a sliver of partial coverage and a semi-transparent
                // fill of the same patch are not distinguishable (§1.5
                // information loss). §1.6 allows exactly this — "unsupported
                // OR still in a competing model" — so both readings are
                // retained instead of one being asserted.
                Flat2Outcome::Ambiguous {
                    mixture_classes: semi.iter().map(|(c, _, _)| c.clone()).collect(),
                    note: "a flat intermediate-alpha region thinner than the kernel can \
                           resolve: a thin opaque shape at partial coverage and a \
                           semi-transparent fill produce the same bytes, so spec 1.6 keeps both \
                           readings rather than choosing",
                }
            }
        } else {
            Flat2Outcome::Unsupported(UnsupportedReason::NoHypothesisExplains {
                best_residual_p95_codes: best_residual,
                tolerance_codes: cfg.max_residual_p95_codes,
            })
        }
    } else if explaining.len() > 1 {
        Flat2Outcome::Ambiguous {
            mixture_classes: explaining.iter().map(|(c, _)| c.clone()).collect(),
            note: "two physically different readings explain the pixels; spec 1.4 keeps this \
                   apart from unsupported, and M4 does not choose between them",
        }
    } else {
        let (class, i) = explaining[0].clone();
        let chosen = &evidences[i].1;
        let chosen_formation = evidences[i].0.formation;
        // Retained ties, and the reason they are ties is STRUCTURAL rather
        // than numerical where it can be: over a transparent exterior the
        // two blend spaces predict the same bytes (formation module), so
        // the member with the same filter is kept whatever the residual
        // norms happen to be — comparing scores would let 1e-3 of
        // rounding decide something the physics says is undecidable.
        let tied: Vec<String> = evidences
            .iter()
            .filter(|(ev, s)| {
                s.mixture_class == class
                    && s.id != chosen.id
                    && ((!chosen.blend_space_identifiable
                        && ev.formation.pixel_filter == chosen_formation.pixel_filter)
                        || (!chosen.filter_identifiable
                            && ev.formation.blend_space == chosen_formation.blend_space)
                        || (s.score - chosen.score).abs() <= 1e-9)
            })
            .map(|(_, s)| s.formation.clone())
            .collect();
        let tied = {
            // Deduplicated: the label-swapped readings of §9.2 are in the
            // same class and carry the same formation id, and listing a
            // formation twice would suggest two different ties.
            let mut t = tied;
            t.sort();
            t.dedup();
            t
        };
        Flat2Outcome::Supported {
            evidence_id: chosen.id.clone(),
            mixture_class: class,
            tied_formations: tied,
        }
    };

    // Boundary observations for the chosen evidence only: §13 observes the
    // boundary of a hypothesis, and there is no point observing the ones
    // the outcome did not retain.
    let (boundary, boundary_refusal) = match &outcome {
        Flat2Outcome::Supported { evidence_id, .. } => {
            match evidences.iter().find(|(_, s)| &s.id == evidence_id) {
                None => (None, None),
                Some((ev, _)) => {
                    match observe_boundaries(ev, cfg.coverage_level, &cfg.boundary, &cfg.corridor) {
                        Ok(o) => (Some(o), None),
                        Err(e) => (None, Some(e.to_string())),
                    }
                }
            }
        }
        _ => (None, None),
    };

    let chosen_evidence = match &outcome {
        Flat2Outcome::Supported { evidence_id, .. } => evidences
            .iter()
            .find(|(_, s)| &s.id == evidence_id)
            .map(|(ev, _)| ev.clone()),
        _ => None,
    };

    let report = Flat2Analysis {
        schema: ANALYSIS_SCHEMA,
        image: facts,
        interior: interior.summary(),
        palette_modes: proposals.modes.len(),
        exterior_share: proposals.exterior_share,
        border_transparent_share: proposals.border_transparent_share,
        core_weight: proposals.core_weight,
        hypotheses: hypotheses.iter().map(|h| h.id.clone()).collect(),
        evidences: evidences.into_iter().map(|(_, s)| s).collect(),
        refused,
        outcome,
        boundary,
        boundary_refusal,
        production,
        evidence_is_not_a_likelihood: NOT_A_LIKELIHOOD,
    };
    AnalysisOutput {
        report,
        chosen: chosen_evidence,
    }
}

impl Flat2Analysis {
    pub fn canonical_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("analysis serializes")
    }

    pub fn is_supported(&self) -> bool {
        matches!(self.outcome, Flat2Outcome::Supported { .. })
    }

    /// The evidence the outcome selected, if any.
    pub fn chosen(&self) -> Option<&EvidenceSummary> {
        match &self.outcome {
            Flat2Outcome::Supported { evidence_id, .. } => {
                self.evidences.iter().find(|e| &e.id == evidence_id)
            }
            _ => None,
        }
    }

    /// Whether the OracleOverride kind appears among the hypotheses.
    pub fn used_oracle_override(&self) -> bool {
        !self.production
    }
}

/// Whether a hypothesis came from an oracle override.
pub fn is_override(h: &Flat2Hypothesis) -> bool {
    h.kind == Flat2Kind::OracleOverride
}

#[cfg(test)]
mod tests {
    use super::*;
    use vice_image::IccAssumption;
    use vice_ir::color::linear_to_srgb_encoded;

    fn enc(v: f64) -> u8 {
        (linear_to_srgb_encoded(v.clamp(0.0, 1.0)) * 255.0).round() as u8
    }

    fn image(size: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> CanonicalImage {
        let mut px = Vec::new();
        for y in 0..size {
            for x in 0..size {
                px.extend_from_slice(&f(x, y));
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

    const INK: LinearRgb = LinearRgb {
        r: 0.08,
        g: 0.36,
        b: 0.82,
    };

    fn disc(size: u32, r: f64) -> CanonicalImage {
        let c = f64::from(size) / 2.0;
        image(size, |x, y| {
            let d = (f64::from(x) + 0.5 - c).hypot(f64::from(y) + 0.5 - c);
            let a = (r + 0.5 - d).clamp(0.0, 1.0);
            [
                enc(INK.r),
                enc(INK.g),
                enc(INK.b),
                (a * 255.0).round() as u8,
            ]
        })
    }

    /// The ordinary case: one shape on a transparent exterior is SUPPORTED,
    /// with a boundary observation and a corridor. Without this control
    /// every refusal below would be indistinguishable from a stage that
    /// refuses everything (meta-rule M-2).
    #[test]
    fn a_clean_flat2_image_is_supported_and_yields_a_boundary() {
        let a = analyze(&disc(48, 14.0), &ANALYSIS_CONFIG_V1, None);
        assert!(a.is_supported(), "{:?}", a.outcome);
        assert!(a.production, "no override was used");
        let chosen = a.chosen().expect("a chosen evidence");
        assert!(chosen.residual.p95_abs_codes < 2.0, "{:?}", chosen.residual);
        assert!(chosen.conditioning > 1.0);
        let b = a
            .boundary
            .clone()
            .expect("a supported outcome observes its boundary");
        assert_eq!(b.chains.len(), 1);
        assert!(
            b.median_halfwidth_px < 0.35,
            "median {}",
            b.median_halfwidth_px
        );
        assert!(b.p95_halfwidth_px < 0.75, "p95 {}", b.p95_halfwidth_px);
        // The blend space is NOT identifiable here, and the report says so
        // rather than claiming one.
        assert!(!chosen.blend_space_identifiable);
        match &a.outcome {
            Flat2Outcome::Supported {
                tied_formations, ..
            } => {
                assert!(
                    !tied_formations.is_empty(),
                    "the two blend spaces predict the same bytes and must both be retained"
                );
            }
            other => panic!("{other:?}"),
        }
    }

    /// §1.6, as an outcome: a constant half-covered fill is UNSUPPORTED, and
    /// the reason names the clause instead of delivering "coverage one half,
    /// everywhere".
    #[test]
    fn a_semi_transparent_interior_is_unsupported_and_says_which_clause() {
        let img = image(32, |x, y| {
            let inside = (6..26).contains(&x) && (6..26).contains(&y);
            [
                enc(INK.r),
                enc(INK.g),
                enc(INK.b),
                if inside { 128 } else { 0 },
            ]
        });
        let a = analyze(&img, &ANALYSIS_CONFIG_V1, None);
        match &a.outcome {
            Flat2Outcome::Unsupported(UnsupportedReason::SemiTransparentInterior {
                detail,
                note,
                ..
            }) => {
                assert!(detail.largest_region_px > 300, "{detail:?}");
                assert!(note.contains("1.6"));
            }
            other => panic!("{other:?}"),
        }
        assert!(
            a.boundary.is_none(),
            "an unsupported input observes nothing"
        );
    }

    /// Three visible faces are not Flat2, and the outcome carries the
    /// palette refusal rather than silently fitting two of the three.
    #[test]
    fn a_three_colour_image_is_unsupported_through_the_palette() {
        let img = image(32, |x, _| {
            if x < 10 {
                [240, 20, 20, 255]
            } else if x < 21 {
                [20, 240, 20, 255]
            } else {
                [20, 20, 240, 255]
            }
        });
        match &analyze(&img, &ANALYSIS_CONFIG_V1, None).outcome {
            Flat2Outcome::Unsupported(UnsupportedReason::Palette { detail }) => {
                assert!(detail.contains("M8"), "{detail}");
            }
            other => panic!("{other:?}"),
        }
    }

    /// An oracle override marks the run NON-PRODUCTION, in the artifact
    /// (§30). The flag is on the report, not on the command line, so it
    /// survives being copied somewhere else.
    #[test]
    fn an_oracle_override_marks_the_run_non_production() {
        let a = analyze(
            &disc(32, 9.0),
            &ANALYSIS_CONFIG_V1,
            Some(crate::palette::oracle_override(INK, None)),
        );
        assert!(!a.production);
        assert!(a.used_oracle_override());
        assert!(a.canonical_json().contains("\"production\": false"));
        // And a normal run is production.
        assert!(analyze(&disc(32, 9.0), &ANALYSIS_CONFIG_V1, None).production);
    }

    #[test]
    fn a_supported_filter_scope_is_applied_before_evidence_selection() {
        let output = analyze_full_for_filters(
            &disc(48, 14.0),
            &ANALYSIS_CONFIG_V1,
            None,
            &[PixelFilter::Box],
        );
        assert!(output.report.is_supported(), "{:?}", output.report.outcome);
        assert!(output
            .report
            .evidences
            .iter()
            .all(|summary| summary.formation.contains("/box/")));
        assert_eq!(
            output
                .chosen
                .expect("supported evidence")
                .formation
                .pixel_filter,
            PixelFilter::Box
        );
    }

    /// §10.2 as a property of the ARTIFACT: no field of this report is in
    /// the units of the final posterior, and every surrogate says what it is
    /// not. Walking the serialized keys rather than trusting that nobody
    /// adds `*_bits` later.
    #[test]
    fn no_evidence_field_is_published_in_posterior_units() {
        let a = analyze(&disc(40, 12.0), &ANALYSIS_CONFIG_V1, None);
        let v: serde_json::Value = serde_json::from_str(&a.canonical_json()).unwrap();
        // The two DISCLAIMER fields are the only place these words may
        // appear: they say what the number is not.
        const DISCLAIMERS: &[&str] = &["evidence_is_not_a_likelihood", "not_a_likelihood"];
        fn walk(v: &serde_json::Value, path: &str, bad: &mut Vec<String>, seen: &mut Vec<String>) {
            match v {
                serde_json::Value::Object(m) => {
                    for (k, x) in m {
                        if DISCLAIMERS.contains(&k.as_str()) {
                            seen.push(k.clone());
                        } else if k.contains("bits")
                            || k.contains("posterior")
                            || k.contains("likelihood")
                        {
                            bad.push(format!("{path}/{k}"));
                        }
                        walk(x, &format!("{path}/{k}"), bad, seen);
                    }
                }
                serde_json::Value::Array(a) => {
                    for (i, x) in a.iter().enumerate() {
                        walk(x, &format!("{path}[{i}]"), bad, seen);
                    }
                }
                _ => {}
            }
        }
        let mut bad = Vec::new();
        let mut seen = Vec::new();
        walk(&v, "", &mut bad, &mut seen);
        assert!(
            bad.is_empty(),
            "a field in the units of the final posterior appeared in the M4 artifact: {bad:?}"
        );
        // The walk is NOT vacuous: it must have visited the disclaimers,
        // both the report-level one and one per surrogate. Without this the
        // assertion above would pass on an empty document.
        assert!(seen.contains(&"evidence_is_not_a_likelihood".to_string()));
        assert!(
            seen.iter().filter(|s| *s == "not_a_likelihood").count() >= a.evidences.len(),
            "every surrogate must carry the sentence: {seen:?}"
        );
        assert!(a.evidence_is_not_a_likelihood.contains("10.2"));
    }

    /// The mixture class is a function of the two faces and NOT of which is
    /// called foreground, which is what stops the label swap from being
    /// reported as an ambiguity.
    #[test]
    fn the_label_swap_lands_in_one_mixture_class() {
        let a = LinearRgb::new(0.9, 0.2, 0.2);
        let b = LinearRgb::new(0.05, 0.05, 0.3);
        let fwd = crate::palette::oracle_override(a, Some(b));
        let rev = crate::palette::oracle_override(b, Some(a));
        assert_eq!(mixture_class(&fwd), mixture_class(&rev));
        let transparent = crate::palette::oracle_override(a, None);
        assert_ne!(mixture_class(&transparent), mixture_class(&fwd));
        assert!(mixture_class(&transparent).starts_with("transparent:"));
    }
}
