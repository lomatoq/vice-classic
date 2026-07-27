//! Measurements that set (and keep) the M4 coefficients.
//!
//! Every constant `vice-evidence` freezes is measured HERE, against the
//! committed corpus, and every one is checked in BOTH directions: the value
//! it is set to must separate the case it is about from the case it is not.
//! That is the discipline F-0016 and F-0010 record, applied to the four
//! numbers M4 introduces.
//!
//! The corpus-wide measurements carry `#[ignore]`, and the reason is the
//! project's own rule that a check nobody waits for is a check nobody runs:
//! they render and analyse hundreds of images, which is seconds in release
//! and MINUTES in debug, and putting them in the default `cargo test` path
//! would push the standard command past the point where anyone runs it. They
//! are not thereby optional — CI executes them in release on every push, as
//! a named step, exactly as the M3 corpus reserved its full manifest for a
//! documented command. What stays in the default path is the shape of the
//! run and its determinism, plus (in `gates`) the check that every frozen
//! number still agrees with the constant the code uses.

use super::*;
use std::sync::OnceLock;
use vice_evidence::formation::{filters_within_margin, transition_width_px, KERNEL_PROFILES_V1};

fn run_once() -> &'static CorridorRun {
    static R: OnceLock<CorridorRun> = OnceLock::new();
    R.get_or_init(|| run(CorridorScope::Test).expect("the test-scope corridor run must succeed"))
}

/// The kernel profile table of `vice-evidence::formation`, measured on the
/// corpus rather than asserted.
#[test]
#[ignore = "corpus-wide measurement: minutes in debug. CI runs these in RELEASE via --ignored (see the module docs and REPRODUCIBILITY_M4)."]
fn the_kernel_profile_table_matches_the_corpus() {
    // The LEGAL population, and nothing else: development groups,
    // development-legal profiles. The previous version walked
    // `procedural_groups(1)` unfiltered and rendered five SEALED-AUDIT
    // families (REVIEW_M4 M4-N1).
    let groups = frozen_calibration_groups().unwrap();
    let legal = frozen_calibration_profiles();
    let mut rows = Vec::new();
    for k in KERNEL_PROFILES_V1 {
        let psf = match k.filter {
            vice_ir::PixelFilter::Box => Psf::Box,
            vice_ir::PixelFilter::Triangle => Psf::Triangle,
            vice_ir::PixelFilter::Gaussian { sigma_px } => Psf::Gaussian { sigma_px },
        };
        // The BOX kernel is measured over every engine that can realize it,
        // and over two sizes. A table measured on one engine would describe
        // that engine's antialiasing rather than the kernel, and the
        // estimator would then be wrong on exactly the images it will see
        // (the first draft measured the supersampler alone and recovered
        // the filter on 3 arms out of 7).
        let engines: Vec<RasterProfile> = if psf == Psf::Box {
            legal.clone()
        } else {
            vec![RasterProfile::Supersample]
        };
        // The box row walks EVERY family, because the statistic varies with
        // the shape (corner density, curvature) at least as much as with the
        // engine, and a spread measured on a third of the families would
        // understate it. The other kernels run only on the supersampler,
        // which costs 576 inside-tests per pixel, so they walk every third.
        let stride = if psf == Psf::Box { 1 } else { 2 };
        let mut widths = Vec::new();
        let mut unresolved = 0u32;
        for (g, profile, size) in groups
            .iter()
            .step_by(stride)
            .flat_map(|g| engines.iter().map(move |p| (g, *p)))
            .flat_map(|(g, p)| [32u32, 64].iter().map(move |s| (g, p, *s)))
        {
            let scene = &g.scenes[0];
            let cell = DegradationCell {
                size_px: size,
                subpixel_dx: 0.0,
                subpixel_dy: 0.0,
                profile,
                psf,
                blend: BlendSpace::LinearLight,
                resize: ResizeChain::None,
                contrast: 1.0,
            };
            let Ok(f) = render_cell(scene, &cell, 1) else {
                continue;
            };
            let img = CanonicalImage::from_straight_srgb8(
                f.width_px,
                f.height_px,
                f.rgba8,
                true,
                IccAssumption::NoProfileAssumedSrgb,
            )
            .unwrap();
            let out = analyze_full(&img, &ANALYSIS_CONFIG_V1, None);
            let Some(ev) = out.chosen else { continue };
            if !vice_evidence::formation::filter_is_identifiable(ev.alpha_field()) {
                // The shape is thinner than the kernel, so the statistic
                // would measure the shape. Counted, not silently dropped.
                unresolved += 1;
                continue;
            }
            let length = vice_evidence::boundary::contour_length_px(
                ev.alpha_field(),
                ev.width_px() as usize,
                ev.height_px() as usize,
                0.5,
            );
            widths.push(transition_width_px(ev.alpha_field(), length));
        }
        assert!(!widths.is_empty(), "{:?}: no measurement", k.filter);
        let mean = widths.iter().sum::<f64>() / widths.len() as f64;
        let sd =
            (widths.iter().map(|w| (w - mean).powi(2)).sum::<f64>() / widths.len() as f64).sqrt();
        println!(
            "{:?}: width {mean:.3} sd {sd:.3} over {} resolved renders, {unresolved} unresolved \
             and excluded (table {:.3} +- {:.3})",
            k.filter,
            widths.len(),
            k.width_px,
            k.sd_px
        );
        if k.filter == vice_ir::PixelFilter::Box {
            assert!(
                unresolved > 0,
                "the corpus must contain shapes the kernel cannot be read off, or the \
                 identifiability criterion is decorative"
            );
            assert!(
                widths.len() > unresolved as usize,
                "most renders must be resolved, or the criterion is discarding the corpus"
            );
        }
        rows.push((k, mean, sd));
    }
    for (k, mean, sd) in &rows {
        assert!(
            (mean - k.width_px).abs() <= 2.0 * k.sd_px,
            "{:?}: the corpus measures {mean:.3} against the table's {:.3} +- {:.3}",
            k.filter,
            k.width_px,
            k.sd_px
        );
        assert!(
            *sd <= 2.0 * k.sd_px,
            "{:?}: measured spread {sd:.3} exceeds the table's {:.3}",
            k.filter,
            k.sd_px
        );
    }
    // And the statistic SEPARATES what the estimator claims to separate.
    let boxed = rows[0].1;
    let wide = rows[3].1;
    assert!(
        filters_within_margin(boxed, 2.0) == vec![vice_ir::PixelFilter::Box],
        "the box kernel must be identified alone at its own measured width"
    );
    assert!(
        filters_within_margin(wide, 2.0) == vec![vice_ir::PixelFilter::Gaussian { sigma_px: 1.0 }],
        "the wide gaussian must be identified alone at its own measured width"
    );
}

/// A run produces arms, and the three gate rows are computed from data that
/// exists.
#[test]
fn a_run_produces_arms_and_a_report() {
    let r = run_once();
    let rep = report::build(r);
    println!(
        "scenes {}, arms {}, refused {}, samples {}, audit groups skipped {}",
        r.scenes,
        r.arms.len(),
        r.refused.len(),
        r.samples.len(),
        r.sealed_audit_groups_skipped
    );
    for (name, ok, why) in rep.gate_table() {
        println!("[{}] {name}: {why}", if ok { "MET" } else { "NOT MET" });
    }
    println!("overall {:?}", rep.overall);
    println!("held-out {:?}", rep.held_out);
    println!("formation {:?}", rep.formation_recovery);
    println!("semi {:?}", rep.semi_transparent);
    println!("steps {:?}", rep.step_invariance);
    assert!(r.scenes > 0);
    assert!(!r.arms.is_empty());
    assert!(
        r.sealed_audit_groups_skipped > 0,
        "the audit must be skipped"
    );
}

/// The clean-bucket noise scale of `configs/GATES_V1.toml` `[noise_scales]`,
/// MEASURED (spec §17.1: "noise scales freeze per dev/formation bucket").
///
/// Measured on the LEGAL population only (`frozen_calibration_groups` /
/// `frozen_calibration_profiles`): development groups, development-legal
/// profiles. The held-out `tiny-skia` is therefore absent, and the assertion
/// below fails if it ever returns — because this constant sets the width of
/// the corridor whose clause is about generalizing to exactly that engine
/// (REVIEW_M4 M4-N2).
///
/// What the number has to be the scale OF, and this is the part the first
/// draft got wrong: the dominant observation noise of the clean bucket is
/// not quantization. It is the disagreement between the analytic coverage
/// the M4 formation family predicts and the antialiasing an independent
/// engine actually performed. Quantization contributes 1/sqrt(12) of a code;
/// the AA disagreement contributes an order of magnitude more, and F-0017
/// measured that it is WHITE at the pixel level (corr-len 1, iid overcount
/// 1.0x), which is exactly the condition under which a per-pixel sigma is
/// the right way to carry it.
///
/// Measured over EDGE pixels only: interior and empty pixels agree by
/// construction and would dilute the scale into a statement about canvas
/// size (the same reason `measure_profile_accuracy` does it).
#[test]
#[ignore = "corpus-wide measurement: minutes in debug. CI runs these in RELEASE via --ignored (see the module docs and REPRODUCIBILITY_M4)."]
fn the_clean_bucket_noise_scale_is_measured_on_the_development_split() {
    use crate::gt::raster::{rasterize, ViewTransform};
    let groups = frozen_calibration_groups().unwrap();
    // INDEPENDENT engines that are legal here. `tiny-skia` is held out and
    // is therefore absent: the corridor clause it would contaminate is the
    // one about generalizing to an engine the calibration never saw
    // (REVIEW_M4 M4-N2).
    let engines: Vec<RasterProfile> = frozen_calibration_profiles()
        .into_iter()
        .filter(|p| p.is_independent_engine())
        .collect();
    assert!(
        !engines.contains(&RasterProfile::TinySkia),
        "the held-out engine must not appear in a frozen coefficient"
    );
    assert!(!engines.is_empty(), "no independent engine is legal here");
    let mut rows: Vec<(&str, f64, f64, u64)> = Vec::new();
    for engine in engines {
        let (mut sum2, mut max, mut n) = (0.0f64, 0.0f64, 0u64);
        for g in groups.iter() {
            let scene = &g.scenes[0];
            for size in [32u32, 64] {
                let t = ViewTransform {
                    scale: f64::from(size) / f64::from(AUTHORING_CANVAS_PX),
                    dx: 0.0,
                    dy: 0.0,
                    width_px: size,
                    height_px: size,
                };
                let Ok(truth) =
                    rasterize(scene.certified(), &t, RasterProfile::ExactClip, Psf::Box)
                else {
                    continue;
                };
                let Ok(got) = rasterize(scene.certified(), &t, engine, Psf::Box) else {
                    continue;
                };
                for (a, b) in truth.per_face.iter().zip(&got.per_face) {
                    for (x, y) in a.iter().zip(b) {
                        if *x > 0.0 && *x < 1.0 {
                            let d = (x - y).abs();
                            sum2 += d * d;
                            max = max.max(d);
                            n += 1;
                        }
                    }
                }
            }
        }
        let rms = if n > 0 { (sum2 / n as f64).sqrt() } else { 0.0 };
        println!(
            "{}: edge-pixel coverage RMS {:.5} ({:.2} codes), max {:.4}, {n} edge pixels",
            engine.as_str(),
            rms,
            rms * 255.0,
            max
        );
        rows.push((engine.as_str(), rms, max, n));
    }
    let pooled = (rows.iter().map(|r| r.1 * r.1 * r.3 as f64).sum::<f64>()
        / rows.iter().map(|r| r.3 as f64).sum::<f64>())
    .sqrt();
    println!("pooled RMS {:.5} = {:.3} codes", pooled, pooled * 255.0);
    assert!(rows.iter().all(|r| r.3 > 1000), "too few edge pixels");
    let frozen = vice_evidence::corridor::CLEAN_BUCKET_SIGMA_CODES;
    assert!(
        (pooled * 255.0 - frozen).abs() < 1.0,
        "the frozen clean-bucket sigma is {frozen} codes, the development split measures {:.3}",
        pooled * 255.0
    );
    // Both directions: the scale must be well above quantization (or it
    // would be measuring nothing) and well below full range (or it would
    // excuse anything).
    assert!(frozen > 1.0 / 12.0f64.sqrt() && frozen < 40.0);
}

/// The §1.6 floor, measured in BOTH directions on the corpus.
///
/// The detector has two numbers: an area floor (noise guard) and a
/// thickness (what separates a semi-transparent fill from a thin opaque
/// shape). This prints the distribution of both over the clean corpus and
/// over the constant-alpha probes, so the floor sits in a gap the data
/// shows rather than in one somebody chose.
#[test]
#[ignore = "corpus-wide measurement: minutes in debug. CI runs these in RELEASE via --ignored (see the module docs and REPRODUCIBILITY_M4)."]
fn the_semi_transparent_floor_separates_both_ways() {
    let groups = frozen_calibration_groups().unwrap();
    let cells: Vec<DegradationCell> = [
        "s32_pexact-clip_box_lin_none_dx0.00dy0.00_c1.00",
        "s64_pexact-clip_box_lin_none_dx0.00dy0.00_c1.00",
        "s32_psupersample_gauss0.50_lin_none_dx0.00dy0.00_c1.00",
    ]
    .iter()
    .map(|id| *matrix_v1().iter().find(|c| c.id() == *id).unwrap())
    .collect();
    const RATIOS: &[f64] = &[3.0, 5.0, 8.0, 12.0];
    let mut clean: Vec<(f64, Option<(u64, f64)>)> = Vec::new();
    let mut probe: Vec<(f64, Option<(u64, f64)>)> = Vec::new();
    for g in groups.iter() {
        for scene in &g.scenes {
            for cell in &cells {
                let Ok(f) = render_cell(scene, cell, 1) else {
                    continue;
                };
                for (scale, into) in [(1.0f64, &mut clean), (0.5, &mut probe)] {
                    let mut bytes = f.rgba8.clone();
                    if scale != 1.0 {
                        for px in bytes.chunks_mut(4) {
                            px[3] = (f64::from(px[3]) * scale).round() as u8;
                        }
                    }
                    let Ok(img) = CanonicalImage::from_straight_srgb8(
                        f.width_px,
                        f.height_px,
                        bytes,
                        true,
                        IccAssumption::NoProfileAssumedSrgb,
                    ) else {
                        continue;
                    };
                    for ratio in RATIOS {
                        let cfg = vice_evidence::analysis::AnalysisConfig {
                            mixture: vice_evidence::mixture::MixtureConfig {
                                flat_alpha_gradient_ratio: *ratio,
                                ..ANALYSIS_CONFIG_V1.mixture
                            },
                            ..ANALYSIS_CONFIG_V1
                        };
                        let out = analyze_full(&img, &cfg, None);
                        let best = out
                            .report
                            .evidences
                            .iter()
                            .filter_map(|e| e.semi_transparent_interior.as_ref())
                            .map(|d| (d.largest_region_px, d.thickness_px))
                            .fold(None, |acc: Option<(u64, f64)>, x| match acc {
                                Some(a) if a.1 >= x.1 => Some(a),
                                _ => Some(x),
                            });
                        into.push((*ratio, best));
                    }
                }
            }
        }
    }
    let thick = vice_evidence::mixture::MIXTURE_CONFIG_V1.min_flat_thickness_px;
    let live = vice_evidence::mixture::MIXTURE_CONFIG_V1.flat_alpha_gradient_ratio;
    let mut chosen = (0.0f64, 0usize, 0usize);
    for ratio in RATIOS {
        let stat = |v: &[(f64, Option<(u64, f64)>)]| {
            let rows: Vec<&(f64, Option<(u64, f64)>)> =
                v.iter().filter(|(r, _)| (r - ratio).abs() < 1e-9).collect();
            let over = rows
                .iter()
                .filter(|(_, d)| d.is_some_and(|(_, t)| t >= thick))
                .count();
            let max_thick = rows
                .iter()
                .filter_map(|(_, d)| d.map(|(_, t)| t))
                .fold(0.0f64, f64::max);
            (rows.len(), over, max_thick)
        };
        let (cn, co, ct) = stat(&clean);
        let (pn, po, pt) = stat(&probe);
        println!(
            "ratio {ratio:>5.1}: CLEAN {co}/{cn} over thickness {thick} (max thickness {ct:.2});              PROBES {po}/{pn} over (max {pt:.2})"
        );
        if (ratio - live).abs() < 1e-9 {
            chosen = (*ratio, co, po);
        }
    }
    assert!(
        !probe.is_empty() && !clean.is_empty(),
        "both populations must exist"
    );
    assert_eq!(
        chosen.1, 0,
        "at the frozen ratio {live} a CLEAN corpus render produced a flat intermediate region          thick enough to be called a semi-transparent interior",
    );
    assert!(
        chosen.2 * 3 > probe.len() / RATIOS.len(),
        "at the frozen ratio {live} only {} constant-alpha probes were caught",
        chosen.2
    );
}

/// The residual tolerance of `vice-evidence::analysis`, measured in BOTH
/// directions: the CORRECT hypothesis on an independently rasterized render
/// leaves a few codes, and a wrong one leaves an order of magnitude more.
#[test]
#[ignore = "corpus-wide measurement: minutes in debug. CI runs these in RELEASE via --ignored (see the module docs and REPRODUCIBILITY_M4)."]
fn the_residual_tolerance_separates_right_from_wrong_hypotheses() {
    use vice_evidence::analysis::MAX_RESIDUAL_P95_CODES;
    let groups = frozen_calibration_groups().unwrap();
    let cell = *matrix_v1()
        .iter()
        .find(|c| c.id() == "s64_praqote_box_lin_none_dx0.00dy0.00_c1.00")
        .unwrap();
    let (mut right, mut wrong) = (Vec::new(), Vec::new());
    for g in groups.iter() {
        for scene in &g.scenes {
            let Ok(f) = render_cell(scene, &cell, 1) else {
                continue;
            };
            let Ok(img) = CanonicalImage::from_straight_srgb8(
                f.width_px,
                f.height_px,
                f.rgba8,
                true,
                IccAssumption::NoProfileAssumedSrgb,
            ) else {
                continue;
            };
            let out = analyze_full(&img, &ANALYSIS_CONFIG_V1, None);
            if let Some(c) = out.report.chosen() {
                right.push(c.residual.p95_abs_codes);
            }
            // A WRONG palette: the same image explained by a foreground that
            // is not in it. Nothing else changes.
            let bogus = vice_evidence::palette::oracle_override(
                vice_ir::LinearRgb::new(0.9, 0.1, 0.55),
                Some(vice_ir::LinearRgb::new(0.05, 0.6, 0.1)),
            );
            let bad = vice_evidence::analysis::analyze(&img, &ANALYSIS_CONFIG_V1, Some(bogus));
            if let Some(e) = bad.evidences.first() {
                wrong.push(e.residual.p95_abs_codes);
            }
        }
    }
    let stat = |v: &mut Vec<f64>| {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        (v[0], v[v.len() / 2], v[v.len() - 1])
    };
    let (rl, rm, rh) = stat(&mut right);
    let (wl, wm, wh) = stat(&mut wrong);
    println!(
        "correct hypothesis p95 residual: min {rl:.2} median {rm:.2} max {rh:.2} codes over {} \
         renders",
        right.len()
    );
    println!("wrong palette:                   min {wl:.2} median {wm:.2} max {wh:.2} codes");
    assert!(right.len() > 5 && wrong.len() > 5);
    assert!(
        rh < MAX_RESIDUAL_P95_CODES,
        "the correct hypothesis must stay under the tolerance: worst {rh}"
    );
    assert!(
        wl > MAX_RESIDUAL_P95_CODES,
        "a wrong palette must exceed it: best {wl}"
    );
}

/// Interior confidence separates a flat core from an antialiased rim ON THE
/// CORPUS, not only on the synthetic fixture in `vice-evidence`.
#[test]
#[ignore = "corpus-wide measurement: minutes in debug. CI runs these in RELEASE via --ignored (see the module docs and REPRODUCIBILITY_M4)."]
fn interior_confidence_separates_cores_from_edges() {
    use vice_evidence::interior::{interior_confidence, INTERIOR_CONFIG_V1};
    let groups = frozen_calibration_groups().unwrap();
    let cell = *matrix_v1()
        .iter()
        .find(|c| c.id() == "s128_pexact-clip_box_lin_none_dx0.00dy0.00_c1.00")
        .unwrap();
    let mut core = Vec::new();
    let mut rim = Vec::new();
    for g in groups.iter() {
        let scene = &g.scenes[0];
        let Ok(f) = render_cell(scene, &cell, 1) else {
            continue;
        };
        let Ok(img) = CanonicalImage::from_straight_srgb8(
            f.width_px,
            f.height_px,
            f.rgba8,
            true,
            IccAssumption::NoProfileAssumedSrgb,
        ) else {
            continue;
        };
        let t = vice_image::ObservationTensor::of(&img, BlendSpace::LinearLight);
        let c = interior_confidence(&t, &INTERIOR_CONFIG_V1);
        for i in 0..t.len() {
            let a = t.alpha(i);
            if a > 0.004 && a < 0.996 {
                rim.push(c.weight(i));
            } else if a >= 0.996 {
                core.push(c.weight(i));
            }
        }
    }
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len().max(1) as f64;
    println!(
        "mean interior weight: opaque core {:.3} over {} px, mixed rim {:.3} over {} px",
        mean(&core),
        core.len(),
        mean(&rim),
        rim.len()
    );
    assert!(
        core.len() > 1000 && rim.len() > 500,
        "both populations must exist"
    );
    assert!(
        mean(&core) > 4.0 * mean(&rim),
        "the palette must be trained by cores, not by edges (§9.1)"
    );
}

/// The run is deterministic, which is what makes the committed artifact a
/// comparison rather than a judgement call.
#[test]
fn the_corridor_report_is_deterministic() {
    let a = report::build(run_once());
    let b = report::build(&run(CorridorScope::Test).unwrap());
    assert_eq!(a.canonical_json(), b.canonical_json());
}
