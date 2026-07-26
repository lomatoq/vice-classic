//! Residual-correlation benchmark and the likelihood calibration protocol
//! (spec §17.1, §28 M3 gate "correlation-aware likelihood protocol exists
//! before any confidence claim").
//!
//! The failure §17.1 names: blur, resize, antialiasing and codec residuals
//! are spatially correlated, so an independent per-pixel likelihood
//! multiplies ONE physical edge residual by the number of pixels it
//! touches and makes the posterior dangerously sharp. The spec's remedy is
//! that the final likelihood must use one of three audited variants —
//! whitening, block, or orthogonal band — and that `iid pixel` is a
//! diagnostic baseline that may never drive production confidence.
//!
//! Two things are needed for that to be more than a paragraph, and both
//! are here:
//!
//! 1. A BENCHMARK that measures the empirical correlation length on this
//!    corpus, so the block size or whitening support of a future model has
//!    a number to be calibrated against rather than a guess. It also
//!    measures how far off an iid model would be, which is the empirical
//!    justification for the ban.
//! 2. A GUARD, [`guard_confidence_claim`], that refuses to let a confidence
//!    claim be made under a diagnostic-only residual model. M3 makes no
//!    confidence claims — there is no vectorizer — so what M3 owes is
//!    exactly this: the protocol, and the refusal, in place BEFORE the
//!    first claim can be attempted.

use serde::Serialize;

/// The residual models §17.1 permits, plus the one it forbids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResidualModel {
    /// Independent per pixel. DIAGNOSTIC ONLY (§17.1).
    IidPixel,
    /// Whitening / precision model for the residual field.
    Whitening,
    /// Block likelihood with a block no smaller than the calibrated
    /// correlation support.
    Block,
    /// Orthogonal frequency-band likelihood with separate frozen noise
    /// scales per band.
    OrthogonalBand,
}

impl ResidualModel {
    pub fn id(&self) -> &'static str {
        match self {
            ResidualModel::IidPixel => "iid_pixel",
            ResidualModel::Whitening => "whitening",
            ResidualModel::Block => "block",
            ResidualModel::OrthogonalBand => "orthogonal_band",
        }
    }

    /// May this model drive a PRODUCTION confidence number?
    pub fn admissible_for_confidence(&self) -> bool {
        !matches!(self, ResidualModel::IidPixel)
    }

    /// What a future milestone must calibrate before using it, stated per
    /// model so the protocol is actionable rather than a list of names.
    pub fn calibration_requirement(&self) -> &'static str {
        match self {
            ResidualModel::IidPixel => {
                "none: it is a diagnostic baseline and may not drive confidence at all"
            }
            ResidualModel::Whitening => {
                "an estimated precision operator for the residual field, plus whitened-residual \
                 diagnostics showing the whitened field is white on held-out data"
            }
            ResidualModel::Block => {
                "a block size at least the measured correlation support, and a per-block noise \
                 scale frozen on the calibration split"
            }
            ResidualModel::OrthogonalBand => {
                "an orthogonal band decomposition with separate frozen noise scales per band, \
                 and a check that bands do not re-count the same pixels (§17.2)"
            }
        }
    }

    pub const ALL: &'static [ResidualModel] = &[
        ResidualModel::IidPixel,
        ResidualModel::Whitening,
        ResidualModel::Block,
        ResidualModel::OrthogonalBand,
    ];
}

/// Why a confidence claim was refused.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ConfidenceGuard {
    #[error(
        "residual model {model} is diagnostic-only: it may not drive production confidence \
         (spec §17.1). On this corpus an iid pixel likelihood over-counts independent evidence \
         by a measured factor of about {measured_overcount:.0}x"
    )]
    DiagnosticModel {
        model: &'static str,
        measured_overcount: f64,
    },
    #[error("no residual model was declared: a confidence claim requires one (spec §31)")]
    NoModelDeclared,
    #[error(
        "residual model {model} is admissible but uncalibrated: {requirement}. An uncalibrated \
         model is not a licence to be confident"
    )]
    Uncalibrated {
        model: &'static str,
        requirement: &'static str,
    },
}

/// The gate of §28 M3: a confidence claim requires an admissible, calibrated
/// residual model.
///
/// M3 always ends in a refusal, because nothing is calibrated yet. That is
/// the point: the refusal exists before the first claim can be attempted,
/// rather than being added once someone wants a number.
pub fn guard_confidence_claim(
    model: Option<ResidualModel>,
    calibrated: bool,
    measured_overcount: f64,
) -> Result<(), ConfidenceGuard> {
    let Some(model) = model else {
        return Err(ConfidenceGuard::NoModelDeclared);
    };
    if !model.admissible_for_confidence() {
        return Err(ConfidenceGuard::DiagnosticModel {
            model: model.id(),
            measured_overcount,
        });
    }
    if !calibrated {
        return Err(ConfidenceGuard::Uncalibrated {
            model: model.id(),
            requirement: model.calibration_requirement(),
        });
    }
    Ok(())
}

/// What the benchmark measured on one residual field.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CorrelationReport {
    pub label: String,
    pub width_px: u32,
    pub height_px: u32,
    /// Pixels that carry residual worth measuring (non-zero field).
    pub active_pixels: u64,
    pub rms: f64,
    /// Normalized autocorrelation at lags 1..=`max_lag`, along x and y.
    ///
    /// Both directions, because the residual of an antialiasing model is
    /// ANISOTROPIC: it lives in a one-pixel band along an edge, so across
    /// the edge it decorrelates immediately while along the edge it stays
    /// correlated for the edge's whole length. Measuring only x reported
    /// correlation length 1 for a field that is obviously structured — the
    /// estimator was measuring the wrong direction, which is the same class
    /// of error as measuring a threshold on the wrong denominator (F-0010).
    pub autocorrelation_x: Vec<f64>,
    pub autocorrelation_y: Vec<f64>,
    pub correlation_length_x_px: Option<u32>,
    pub correlation_length_y_px: Option<u32>,
    /// The larger of the two: a structure correlated in ANY direction is
    /// not a set of independent samples.
    pub correlation_length_px: Option<u32>,
    /// How many independent samples the field really contains, treating a
    /// correlation patch as one sample.
    pub effective_sample_size: f64,
    /// The factor by which an iid pixel likelihood over-counts independent
    /// evidence: `active_pixels / effective_sample_size`.
    pub iid_overcount_factor: f64,
}

/// Measure the spatial correlation of a residual field.
///
/// Autocorrelation is computed only over pixels where the residual is
/// actually non-zero, because a field that is zero over 95% of the canvas
/// would otherwise report a correlation length dominated by the empty
/// background — measuring the shape's size rather than the residual's
/// structure. That distinction is the same one F-0010 made about
/// frame-mean versus edge-mean.
pub fn measure_correlation(
    label: &str,
    field: &[f64],
    w: u32,
    h: u32,
    max_lag: u32,
) -> CorrelationReport {
    let (wi, hi) = (w as usize, h as usize);
    let active: Vec<usize> = (0..field.len())
        .filter(|i| field[*i].abs() > 1e-12)
        .collect();
    let n = active.len() as f64;
    let mean = if n > 0.0 {
        active.iter().map(|i| field[*i]).sum::<f64>() / n
    } else {
        0.0
    };
    let var = if n > 0.0 {
        active
            .iter()
            .map(|i| (field[*i] - mean).powi(2))
            .sum::<f64>()
            / n
    } else {
        0.0
    };
    let rms = var.sqrt();

    let autocorr = |along_x: bool| -> Vec<f64> {
        let mut out = Vec::with_capacity(max_lag as usize);
        for lag in 1..=max_lag {
            let l = lag as usize;
            let (mut acc, mut count) = (0.0f64, 0.0f64);
            for y in 0..hi {
                for x in 0..wi {
                    let (x2, y2) = if along_x { (x + l, y) } else { (x, y + l) };
                    if x2 >= wi || y2 >= hi {
                        continue;
                    }
                    let a = field[y * wi + x];
                    let b = field[y2 * wi + x2];
                    // Both ends must be active, or the statistic measures
                    // the boundary of the residual support instead of its
                    // correlation.
                    if a.abs() > 1e-12 && b.abs() > 1e-12 {
                        acc += (a - mean) * (b - mean);
                        count += 1.0;
                    }
                }
            }
            out.push(if count > 0.0 && var > 0.0 {
                (acc / count) / var
            } else {
                0.0
            });
        }
        out
    };
    let autocorrelation_x = autocorr(true);
    let autocorrelation_y = autocorr(false);

    let threshold = 1.0 / std::f64::consts::E;
    let length_of = |ac: &[f64]| -> Option<u32> {
        ac.iter().position(|c| *c < threshold).map(|i| i as u32 + 1)
    };
    let correlation_length_x_px = length_of(&autocorrelation_x);
    let correlation_length_y_px = length_of(&autocorrelation_y);
    // `None` means "still correlated at max_lag", which is MORE correlated
    // than any finite length, so it dominates.
    let correlation_length_px = match (correlation_length_x_px, correlation_length_y_px) {
        (None, _) | (_, None) => None,
        (Some(a), Some(b)) => Some(a.max(b)),
    };
    // A correlation patch is (2Lx-1)(2Ly-1) pixels; below that, samples are
    // not independent. An unbounded direction is treated as the window.
    let lx = f64::from(correlation_length_x_px.unwrap_or(max_lag));
    let ly = f64::from(correlation_length_y_px.unwrap_or(max_lag));
    let patch = ((2.0 * lx - 1.0) * (2.0 * ly - 1.0)).max(1.0);
    let effective = (n / patch).max(1.0);
    CorrelationReport {
        label: label.to_string(),
        width_px: w,
        height_px: h,
        active_pixels: active.len() as u64,
        rms,
        autocorrelation_x,
        autocorrelation_y,
        correlation_length_x_px,
        correlation_length_y_px,
        correlation_length_px,
        effective_sample_size: effective,
        iid_overcount_factor: if effective > 0.0 { n / effective } else { 1.0 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gt::grammar::procedural_groups;
    use crate::gt::raster::{rasterize, Psf, RasterProfile, ViewTransform};

    /// The two residual fields worth measuring, and why they are two.
    ///
    /// §17.1 is about residuals that are spatially CORRELATED. Not every
    /// residual is: the disagreement between two antialiasing models is
    /// dominated by per-pixel 8-bit rounding and measures as nearly white.
    /// The residual that matters is a mis-specified FORMATION — a box
    /// filter judged against a gaussian one — which spreads one physical
    /// edge over a halo. The benchmark reports both so the ban on iid is
    /// justified by the case that needs it rather than by a slogan.
    fn residual_fields(size: u32) -> Vec<(String, Vec<f64>, u32)> {
        let g = procedural_groups(1);
        let s = &g[1].scenes[0];
        let t = ViewTransform {
            scale: f64::from(size) / f64::from(crate::gt::grammar::AUTHORING_CANVAS_PX),
            dx: 0.0,
            dy: 0.0,
            width_px: size,
            height_px: size,
        };
        let cov = |p: RasterProfile, psf: Psf| {
            rasterize(s.certified(), &t, p, psf).unwrap().per_face[1].clone()
        };
        let exact = cov(RasterProfile::ExactClip, Psf::Box);
        let judge = cov(RasterProfile::TinySkia, Psf::Box);
        let blurred = cov(RasterProfile::Supersample, Psf::Gaussian { sigma_px: 1.0 });
        // MAGNITUDE of the residual, deliberately.
        //
        // A mis-specified blur produces a signed dipole across an edge:
        // coverage is removed inside and added outside. Autocorrelation of
        // the SIGNED field cancels the two lobes against each other and
        // reports a whiteness that is not there - measured, and it is why
        // the first version of this benchmark found correlation length 1
        // for an obviously coherent halo. The magnitude field is the
        // coherent structure, and it is what a block size or a whitening
        // support has to cover.
        let sub = |a: &[f64], b: &[f64]| -> Vec<f64> {
            a.iter().zip(b).map(|(x, y)| (x - y).abs()).collect()
        };
        vec![
            (
                format!("aa-model-disagreement@{size}"),
                sub(&exact, &judge),
                size,
            ),
            (
                format!("formation-mismatch-blur@{size}"),
                sub(&exact, &blurred),
                size,
            ),
        ]
    }

    /// White noise must measure as white. Without this control the
    /// correlation length could be an artefact of the estimator, and the
    /// whole benchmark would be measuring itself (M-4).
    #[test]
    fn the_estimator_reports_white_noise_as_white() {
        let (w, h) = (64u32, 64u32);
        let mut seed = 0x243f_6a88_85a3_08d3u64;
        let field: Vec<f64> = (0..(w * h))
            .map(|_| {
                seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                ((seed >> 11) as f64 / (1u64 << 53) as f64) - 0.5
            })
            .collect();
        let r = measure_correlation("white", &field, w, h, 8);
        println!(
            "white noise autocorrelation x: {:?}",
            &r.autocorrelation_x[..4]
        );
        assert_eq!(
            r.correlation_length_px,
            Some(1),
            "uncorrelated noise must decorrelate at lag 1"
        );
        assert!(r.iid_overcount_factor < 1.5, "{}", r.iid_overcount_factor);
    }

    /// And a deliberately smooth field must measure as correlated, so the
    /// estimator is sensitive as well as specific.
    #[test]
    fn the_estimator_reports_a_smooth_field_as_correlated() {
        let (w, h) = (64u32, 64u32);
        let field: Vec<f64> = (0..(w * h))
            .map(|i| {
                let x = f64::from(i % w);
                let y = f64::from(i / w);
                ((x / 9.0).sin() + (y / 11.0).cos()) * 0.01
            })
            .collect();
        let r = measure_correlation("smooth", &field, w, h, 12);
        assert!(
            r.correlation_length_px.unwrap_or(u32::MAX) >= 3,
            "a field smooth over ~9 px must not decorrelate at lag 1 or 2: {:?}",
            r.correlation_length_px
        );
        assert!(r.iid_overcount_factor > 5.0);
    }

    /// The benchmark on the real corpus, and the finding it produced.
    ///
    /// Measured: the antialiasing-model residual is nearly WHITE (8-bit
    /// rounding, correlation length 1), while a mis-specified blur is
    /// strongly correlated. So §17.1's danger is real but not universal,
    /// and the number the future block/whitening model must be calibrated
    /// against comes from the second case, not the first.
    #[test]
    fn the_corpus_residual_is_measurably_correlated_where_it_matters() {
        let mut blur_overcount = 0.0f64;
        let mut aa_length = 0u32;
        for size in [64u32] {
            for (label, field, w) in residual_fields(size) {
                let r = measure_correlation(&label, &field, w, w, 10);
                println!(
                    "{}: active {} px, rms {:.5}, corr-len x={:?} y={:?}, effective n {:.1}, iid overcount {:.1}x",
                    r.label,
                    r.active_pixels,
                    r.rms,
                    r.correlation_length_x_px,
                    r.correlation_length_y_px,
                    r.effective_sample_size,
                    r.iid_overcount_factor
                );
                assert!(r.active_pixels > 20, "the residual must have support");
                assert!(r.rms > 1e-4, "the two models must actually disagree");
                if label.starts_with("aa-model") {
                    aa_length = r.correlation_length_px.unwrap_or(99);
                } else {
                    blur_overcount = r.iid_overcount_factor;
                    assert!(
                        r.correlation_length_px.unwrap_or(99) >= 2,
                        "a mis-specified blur must NOT look white: {:?}",
                        r.correlation_length_px
                    );
                }
            }
        }
        assert_eq!(
            aa_length, 1,
            "8-bit AA disagreement is dominated by per-pixel rounding and measures as white;              claiming otherwise would be inventing correlation the data does not show"
        );
        assert!(
            blur_overcount > 3.0,
            "an iid likelihood on a blur residual must over-count by a real factor, got {blur_overcount}"
        );
    }

    /// The gate itself: no confidence claim without an admissible,
    /// calibrated model - and M3 can satisfy none of it, by design.
    #[test]
    fn a_confidence_claim_is_refused_without_a_calibrated_admissible_model() {
        assert!(matches!(
            guard_confidence_claim(None, true, 30.0),
            Err(ConfidenceGuard::NoModelDeclared)
        ));
        match guard_confidence_claim(Some(ResidualModel::IidPixel), true, 30.0) {
            Err(ConfidenceGuard::DiagnosticModel { model, .. }) => assert_eq!(model, "iid_pixel"),
            other => panic!("iid must be refused even when 'calibrated': {other:?}"),
        }
        for m in [
            ResidualModel::Whitening,
            ResidualModel::Block,
            ResidualModel::OrthogonalBand,
        ] {
            assert!(matches!(
                guard_confidence_claim(Some(m), false, 30.0),
                Err(ConfidenceGuard::Uncalibrated { .. })
            ));
            assert!(guard_confidence_claim(Some(m), true, 30.0).is_ok());
            assert!(!m.calibration_requirement().is_empty());
        }
        // The M3 state of the world, asserted so it cannot drift silently.
        assert!(
            guard_confidence_claim(Some(ResidualModel::Block), false, 30.0).is_err(),
            "M3 has no calibrated residual model and therefore may make no confidence claim"
        );
    }

    #[test]
    fn the_admissible_set_matches_the_frozen_gates_file() {
        let g = crate::gates::GatesFile::load(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../configs/GATES_V1.toml"),
        )
        .unwrap();
        let allowed = g
            .gate_value("likelihood", "allowed_production_residual_models")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        let from_code: Vec<String> = ResidualModel::ALL
            .iter()
            .filter(|m| m.admissible_for_confidence())
            .map(|m| m.id().to_string())
            .collect();
        assert_eq!(allowed, from_code, "the gate and the code must agree");
        let diagnostic = g
            .gate_value("likelihood", "diagnostic_only_residual_models")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(diagnostic.len(), 1);
        assert_eq!(diagnostic[0].as_str().unwrap(), "iid_pixel");
    }
}
