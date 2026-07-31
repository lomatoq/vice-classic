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
    CanonicalImage::from_straight_srgb8(size, size, px, true, IccAssumption::NoProfileAssumedSrgb)
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

#[test]
fn an_opaque_two_face_tie_keeps_the_border_supported_canonical_reading() {
    let img = image(48, |x, y| {
        if (10..38).contains(&x) && (12..36).contains(&y) {
            [30, 90, 180, 255]
        } else {
            [235, 232, 220, 255]
        }
    });
    let output = analyze_full_for_filters(&img, &ANALYSIS_CONFIG_V1, None, &[PixelFilter::Box]);
    assert!(output.report.is_supported(), "{:?}", output.report.outcome);
    assert!(
        output
            .chosen
            .expect("supported opaque evidence")
            .hypothesis
            .id
            .starts_with("H2/border-supported-background"),
        "a numerical near-tie must not turn the full canvas into foreground"
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
