//! The two §1.6 populations: constant `0<α<1` over a TRANSPARENT exterior,
//! and constant `0<α<1` OVER ANOTHER AUTHORED LAYER.
//!
//! They are in one module because the pair is the argument. The same ink at
//! the same constant alpha is decidable in the first authoring and not
//! decidable in the second, and a reader who sees only one of them cannot
//! tell which of those two facts a number is about.
//!
//! [`scaled_alpha`] is the first: it scales the alpha of a render whose
//! exterior is transparent, so the plateau is visible in the alpha channel
//! and the detector has something to detect. The gate row of §28 M4 is about
//! those probes, and about the resolved-interior subset of them.
//!
//! REVIEW_M4 M4-N5 observed that this is a subclass, and the decidable one:
//! the subclass the spec sentence actually describes — paint at constant α
//! over an opaque layer — was not probed at all, and the reviewer built it
//! and got an ordinary two-colour reading back. [`over_opaque_layer`] is
//! that one.
//!
//! This module probes it, and the interesting part is not the outcome but
//! WHY the outcome is the right one. Composite ink `F` at constant `β` over
//! an opaque layer `B`, on a shape whose coverage field is `a`:
//!
//! ```text
//! probe   = a·β·F + (1 − a·β)·B
//! ```
//!
//! and now author an ordinary two-colour scene with faces `F' = β·F + (1−β)·B`
//! and `B`, with the SAME coverage field:
//!
//! ```text
//! two-colour = a·F' + (1 − a)·B = a·β·F + (1 − a·β)·B
//! ```
//!
//! The same expression. Not similar, not close — the same. So the two
//! authorings produce the same bytes, and no analysis of those bytes can
//! separate them, whatever it does. Delivering the input as a two-colour
//! coverage problem is not a §1.6 failure here; it is §1.5 information loss,
//! and §1.6 cannot bind on a distinction the pixels do not carry.
//!
//! So the module MEASURES the identity rather than arguing it: it builds both
//! images and reports the largest byte difference between them. What is left
//! is quantization — `F'` is authored as a `u8`, so the two roundings can
//! differ by a code — and the report carries the count of probes where the
//! difference is zero and where it is at most one code.
//!
//! The measurement is made where it is DEFINED, which is the arms carrying a
//! single ink: there the equivalent authoring is two-colour and can be built.
//! An arm with several inks maps, by the same algebra, to an authoring with
//! as many composited faces, and the harness does not construct that, so
//! those probes are counted separately and no number is attached to them. A
//! first version reported one number over both populations and it read 128
//! codes, which measured the multi-ink arms against the wrong reference
//! rather than measuring anything about §1.6.
//!
//! What the second population is NOT: a §1.6 clause. Nothing about it is
//! conjoined into the gate row, because a population whose outcome is fixed
//! by construction cannot test anything. It is published so that the SCOPE of
//! the clause is on the record with a number attached, which is what M4-N5
//! asked for.

use serde::Serialize;
use vice_evidence::analysis::{analyze, ANALYSIS_CONFIG_V1};
use vice_evidence::{Flat2Outcome, UnsupportedReason};
use vice_image::{CanonicalImage, IccAssumption};
use vice_ir::color::{linear_to_srgb_u8, srgb_u8_to_linear};
use vice_ir::BlendSpace;

use crate::gt::degradation::RenderedFixture;

/// Result of the §1.6 probe on one arm: the arm's own render with its alpha
/// SCALED, which is an authored layer of constant alpha over the same
/// geometry, with the exterior still transparent.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SemiTransparentProbe {
    pub scene_id: String,
    pub cell_id: String,
    pub alpha: f64,
    pub outcome: String,
    pub rejected_as_semi_transparent: bool,
    pub largest_region_px: Option<u64>,
    /// True when the UNMODIFIED arm has a resolved interior, which is the
    /// condition under which scaling its alpha is observable at all: a
    /// full-coverage plateau scaled by beta becomes a plateau at beta, and
    /// no opaque geometry can produce one. Without a plateau to scale there
    /// is nothing to see, and a thinner shape explains the same bytes
    /// (§1.5 information loss).
    pub observable: bool,
}

/// Probe one arm by SCALING its alpha, at every alpha in `alphas`.
///
/// `observable` is decided by the caller from the unmodified arm, because it
/// is a property of the arm rather than of the probe: the criterion is the
/// same resolved-interior fraction the detector itself uses.
pub fn scaled_alpha(
    scene_id: &str,
    cell_id: &str,
    fixture: &RenderedFixture,
    alphas: &[f64],
    observable: bool,
) -> Result<Vec<SemiTransparentProbe>, String> {
    let mut out = Vec::new();
    for a in alphas {
        let mut bytes = fixture.rgba8.clone();
        for px in bytes.chunks_mut(4) {
            px[3] = (f64::from(px[3]) * a).round() as u8;
        }
        let img = CanonicalImage::from_straight_srgb8(
            fixture.width_px,
            fixture.height_px,
            bytes,
            true,
            IccAssumption::NoProfileAssumedSrgb,
        )
        .map_err(|e| e.to_string())?;
        let report = analyze(&img, &ANALYSIS_CONFIG_V1, None);
        let (rejected, region) = match &report.outcome {
            Flat2Outcome::Unsupported(UnsupportedReason::SemiTransparentInterior {
                detail,
                ..
            }) => (true, Some(detail.largest_region_px)),
            _ => (false, None),
        };
        out.push(SemiTransparentProbe {
            scene_id: scene_id.to_string(),
            cell_id: cell_id.to_string(),
            alpha: *a,
            outcome: super::outcome_name(&report.outcome),
            rejected_as_semi_transparent: rejected,
            largest_region_px: region,
            observable,
        });
    }
    Ok(out)
}

/// One probe: one arm's ink, at one constant alpha, over an opaque layer.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OpaqueLayerProbe {
    pub scene_id: String,
    pub cell_id: String,
    /// The constant alpha of the authored layer.
    pub alpha: f64,
    pub outcome: String,
    pub rejected_as_semi_transparent: bool,
    /// Whether the arm has ONE ink, which is the case where the equivalent
    /// authoring is a two-colour scene and the identity below is defined.
    pub single_ink: bool,
    /// Largest per-channel difference, in codes, between this composite and
    /// the two-colour scene that is algebraically equal to it. Zero means the
    /// bytes are identical and no analysis could tell the authorings apart.
    /// `None` on a multi-ink arm, where the two-colour scene is not the right
    /// reference and a number would be meaningless rather than merely large.
    pub max_byte_difference: Option<u64>,
}

/// The opaque layer, per channel: a colour 128 codes away from the ink, so
/// the pair is well conditioned by construction and a failure to read the
/// scene cannot be blamed on contrast (§10 conditioning).
fn contrasting_layer(c: u8) -> u8 {
    if c >= 128 {
        c - 128
    } else {
        c + 128
    }
}

/// `a·f + (1−a)·b`, evaluated in the blend space the cell renders in, so the
/// probe is a scene of the same formation family rather than a scene in a
/// space nothing in the corpus uses.
fn over(f: u8, b: u8, a: f64, blend: BlendSpace) -> u8 {
    match blend {
        BlendSpace::LinearLight => {
            let (f, b) = (srgb_u8_to_linear(f), srgb_u8_to_linear(b));
            linear_to_srgb_u8(a * f + (1.0 - a) * b)
        }
        BlendSpace::EncodedSrgb => {
            let (f, b) = (f64::from(f) / 255.0, f64::from(b) / 255.0);
            (((a * f + (1.0 - a) * b) * 255.0).round()).clamp(0.0, 255.0) as u8
        }
    }
}

/// The straight colour of the most opaque pixel: the ink of the shape, and
/// whether it is the ONLY one.
///
/// `None` when no pixel is opaque, i.e. the arm has no ink to put over a
/// layer. The caller records nothing rather than probing a shape it does not
/// have.
///
/// The second component decides where the byte-identity is CHECKABLE. With
/// one ink the equivalent authoring is a two-colour scene, which this module
/// constructs and compares against. With several inks the same algebra sends
/// a k-face authoring to a k-face authoring — the argument does not change —
/// but the two-colour construction is simply the wrong reference, so those
/// probes are counted and no identity is claimed for them. Measuring the
/// wrong reference and reporting the difference would be worse than saying
/// nothing.
fn ink(rgba8: &[u8]) -> Option<([u8; 3], bool)> {
    let px = rgba8.chunks_exact(4).max_by_key(|p| p[3])?;
    if px[3] != 255 {
        return None;
    }
    let ink = [px[0], px[1], px[2]];
    // EVERY pixel the shape touches has to carry that colour, not only the
    // opaque ones. A scene can hold a second colour that is never fully
    // covered — a hairline crossing a disk — and judging by the opaque
    // pixels alone would call it single-ink and compare it against a
    // two-colour scene it is not. A partially covered pixel whose straight
    // colour has been unpremultiplied at small alpha also lands here, and
    // that is the conservative direction: it costs a probe from the
    // population where the identity is checked, and it never lets a probe in
    // whose reference would be wrong.
    let single = rgba8
        .chunks_exact(4)
        .filter(|p| p[3] > 0)
        .all(|p| [p[0], p[1], p[2]] == ink);
    Some((ink, single))
}

/// Probe one arm OVER AN OPAQUE LAYER, at every alpha in `alphas`.
pub fn over_opaque_layer(
    scene_id: &str,
    cell_id: &str,
    fixture: &RenderedFixture,
    blend: BlendSpace,
    alphas: &[f64],
) -> Result<Vec<OpaqueLayerProbe>, String> {
    let (width_px, height_px) = (fixture.width_px, fixture.height_px);
    let rgba8 = &fixture.rgba8;
    let Some((ink, single_ink)) = ink(rgba8) else {
        return Ok(Vec::new());
    };
    let layer = [
        contrasting_layer(ink[0]),
        contrasting_layer(ink[1]),
        contrasting_layer(ink[2]),
    ];
    let mut out = Vec::new();
    for beta in alphas {
        // The composite: the arm's ink at constant beta over the layer.
        let mut bytes = vec![0u8; rgba8.len()];
        // The face of the two-colour scene this equals: `beta*F + (1-beta)*B`,
        // authored as a u8 the way any other face is.
        let face = [
            over(ink[0], layer[0], *beta, blend),
            over(ink[1], layer[1], *beta, blend),
            over(ink[2], layer[2], *beta, blend),
        ];
        // The two-colour scene is compared byte by byte as it is produced,
        // not materialized: the claim is about the DIFFERENCE, and building
        // a second image to throw away would allocate one per probe for
        // nothing.
        let mut max_diff = 0u64;
        for (i, px) in rgba8.chunks_exact(4).enumerate() {
            let coverage = f64::from(px[3]) / 255.0;
            for c in 0..3 {
                let composite = over(px[c], layer[c], coverage * beta, blend);
                let authored = over(face[c], layer[c], coverage, blend);
                bytes[i * 4 + c] = composite;
                max_diff = max_diff.max(u64::from(composite.abs_diff(authored)));
            }
            bytes[i * 4 + 3] = 255;
        }
        let img = CanonicalImage::from_straight_srgb8(
            width_px,
            height_px,
            bytes,
            true,
            IccAssumption::NoProfileAssumedSrgb,
        )
        .map_err(|e| e.to_string())?;
        let report = analyze(&img, &ANALYSIS_CONFIG_V1, None);
        out.push(OpaqueLayerProbe {
            scene_id: scene_id.to_string(),
            cell_id: cell_id.to_string(),
            alpha: *beta,
            outcome: super::outcome_name(&report.outcome),
            rejected_as_semi_transparent: matches!(
                &report.outcome,
                Flat2Outcome::Unsupported(UnsupportedReason::SemiTransparentInterior { .. })
            ),
            single_ink,
            max_byte_difference: single_ink.then_some(max_diff),
        });
    }
    Ok(out)
}

/// What the run says about the subclass, for the artifact and the gate text.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OverOpaqueLayerSummary {
    pub probes: u64,
    pub delivered_as_two_colour: u64,
    pub rejected_as_semi_transparent: u64,
    pub other_outcomes: u64,
    /// Probes on an arm with ONE ink. Only there is the equivalent authoring
    /// a TWO-colour scene, so only there is the identity below defined.
    pub single_ink_probes: u64,
    /// Probes on an arm with several inks: the same algebra sends a k-face
    /// authoring to a k-face authoring, and the harness does not check that
    /// generalization, so these are counted and nothing is claimed.
    pub multi_ink_probes: u64,
    /// Single-ink probes rejected under §1.6. Expected to be ZERO and worth a
    /// field: these inputs are within a code of an authored two-colour scene,
    /// so a rejection here would be the detector firing on a scene that is
    /// two-colour by construction — the same false positive the clause's
    /// `clean_arms_rejected` control watches for, on a population built to
    /// look exactly like the thing §1.6 forbids.
    pub single_ink_rejected_as_semi_transparent: u64,
    /// Single-ink probes whose bytes are IDENTICAL to the two-colour
    /// authoring.
    pub single_ink_byte_identical: u64,
    /// ... and within one code of it, the difference being the rounding of
    /// `F'` into a `u8`.
    pub single_ink_within_one_code: u64,
    pub single_ink_max_byte_difference: u64,
    pub not_a_clause: &'static str,
}

pub const NOT_A_CLAUSE: &str =
    "constant alpha over an OPAQUE layer produces the same bytes as a scene whose faces are \
     already composited (beta*F+(1-beta)*B), so the two authorings are indistinguishable by any \
     analysis of those bytes: spec 1.5 information loss, not a spec 1.6 failure. Measured rather \
     than asserted where the arm has ONE ink and the equivalent authoring is therefore \
     two-colour: the largest per-channel difference between the two constructions is reported, \
     and what remains of it is the rounding of that face into a u8, below the quantization \
     interval spec 5.2 already carries as a bound. On a multi-ink arm the same algebra sends a \
     k-face authoring to a k-face authoring; that generalization is NOT checked here and no \
     number is reported for it. This population is not part of the 1.6 gate row, whose scope is \
     the probes on a RESOLVED interior over a transparent exterior";

pub fn summarize(probes: &[OpaqueLayerProbe]) -> OverOpaqueLayerSummary {
    OverOpaqueLayerSummary {
        probes: probes.len() as u64,
        delivered_as_two_colour: probes.iter().filter(|p| p.outcome == "supported").count() as u64,
        rejected_as_semi_transparent: probes
            .iter()
            .filter(|p| p.rejected_as_semi_transparent)
            .count() as u64,
        other_outcomes: probes
            .iter()
            .filter(|p| p.outcome != "supported" && !p.rejected_as_semi_transparent)
            .count() as u64,
        single_ink_probes: probes.iter().filter(|p| p.single_ink).count() as u64,
        multi_ink_probes: probes.iter().filter(|p| !p.single_ink).count() as u64,
        single_ink_rejected_as_semi_transparent: probes
            .iter()
            .filter(|p| p.single_ink && p.rejected_as_semi_transparent)
            .count() as u64,
        single_ink_byte_identical: probes
            .iter()
            .filter(|p| p.max_byte_difference == Some(0))
            .count() as u64,
        single_ink_within_one_code: probes
            .iter()
            .filter(|p| p.max_byte_difference.is_some_and(|d| d <= 1))
            .count() as u64,
        single_ink_max_byte_difference: probes
            .iter()
            .filter_map(|p| p.max_byte_difference)
            .max()
            .unwrap_or(0),
        not_a_clause: NOT_A_CLAUSE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rendered fixture the probes take, around bytes built here.
    fn fixture(size: u32, rgba8: Vec<u8>) -> RenderedFixture {
        RenderedFixture {
            scene_id: "test/disk".to_string(),
            group_id: "test".to_string(),
            cell_id: "unit".to_string(),
            width_px: size,
            height_px: size,
            rgba8,
            identifiability: crate::gt::IdentifiabilityClass::Identifiable,
            inverse_crime: false,
        }
    }

    /// A disk with an antialiased edge, as straight sRGB8 over a transparent
    /// exterior: alpha is the coverage, the colour is the ink.
    fn disk(size: u32, radius: f64, ink: [u8; 3]) -> Vec<u8> {
        let mut out = vec![0u8; (size * size * 4) as usize];
        let c = f64::from(size) / 2.0;
        for y in 0..size {
            for x in 0..size {
                let mut inside = 0.0f64;
                for sy in 0..4 {
                    for sx in 0..4 {
                        let px = f64::from(x) + (f64::from(sx) + 0.5) / 4.0;
                        let py = f64::from(y) + (f64::from(sy) + 0.5) / 4.0;
                        if (px - c).hypot(py - c) <= radius {
                            inside += 1.0;
                        }
                    }
                }
                let i = ((y * size + x) * 4) as usize;
                out[i] = ink[0];
                out[i + 1] = ink[1];
                out[i + 2] = ink[2];
                out[i + 3] = (inside / 16.0 * 255.0).round() as u8;
            }
        }
        out
    }

    /// The reviewer's ADV-2 case (a), and the control that makes it mean
    /// something.
    ///
    /// SAME ink, SAME constant alpha, two authorings:
    ///
    /// - over an OPAQUE layer, the bytes are those of a two-colour scene, and
    ///   the analysis reads it as one. The probe records how far the two
    ///   constructions are apart, and it is at most the rounding of the
    ///   composited face into a u8.
    /// - over a TRANSPARENT exterior, the same constant alpha is a plateau in
    ///   the alpha channel that no opaque geometry can produce, and §1.6
    ///   rejects it.
    ///
    /// Without the second half this test would look like a demonstration that
    /// the detector does not work.
    #[test]
    fn the_same_ink_at_the_same_alpha_is_decidable_over_one_authoring_and_not_the_other() {
        let (size, ink) = (48u32, [220u8, 40, 40]);
        let base = disk(size, 16.0, ink);
        let beta = 0.5;

        let fixture = fixture(size, base.clone());
        let over_layer = over_opaque_layer(
            "test/disk",
            "unit",
            &fixture,
            BlendSpace::LinearLight,
            &[beta],
        )
        .expect("the probe builds");
        assert_eq!(over_layer.len(), 1);
        let p = &over_layer[0];
        assert_eq!(
            p.outcome, "supported",
            "over an opaque layer the input IS a two-colour scene"
        );
        assert!(p.single_ink, "the disk has one ink");
        assert!(
            p.max_byte_difference.is_some_and(|d| d <= 1),
            "the composite must be the two-colour scene up to the rounding of the face into a \
             u8, got {:?} codes",
            p.max_byte_difference
        );

        // The control: the same constant alpha over a TRANSPARENT exterior.
        let mut scaled = base.clone();
        for px in scaled.chunks_exact_mut(4) {
            px[3] = (f64::from(px[3]) * beta).round() as u8;
        }
        let img = CanonicalImage::from_straight_srgb8(
            size,
            size,
            scaled,
            true,
            IccAssumption::NoProfileAssumedSrgb,
        )
        .expect("canonical");
        let control = analyze(&img, &ANALYSIS_CONFIG_V1, None);
        assert!(
            matches!(
                control.outcome,
                Flat2Outcome::Unsupported(UnsupportedReason::SemiTransparentInterior { .. })
            ),
            "the observable authoring must still be rejected, else the comparison above says \
             nothing: got {:?}",
            control.outcome
        );
    }

    /// The summary counts what it says it counts: the three outcomes, and the
    /// byte-identity ONLY over the arms where it is defined. A multi-ink probe
    /// must not enter any of the identity numbers, in either direction.
    #[test]
    fn the_summary_separates_the_outcomes_and_scopes_the_identity() {
        let row = |outcome: &str, rejected: bool, diff: Option<u64>| OpaqueLayerProbe {
            scene_id: "s".to_string(),
            cell_id: "c".to_string(),
            alpha: 0.5,
            outcome: outcome.to_string(),
            rejected_as_semi_transparent: rejected,
            single_ink: diff.is_some(),
            max_byte_difference: diff,
        };
        let s = summarize(&[
            row("supported", false, Some(0)),
            row("supported", false, Some(1)),
            row("unsupported/semi_transparent_interior", true, Some(2)),
            row("unsupported/palette", false, None),
            row("supported", false, None),
        ]);
        assert_eq!(s.probes, 5);
        assert_eq!(s.delivered_as_two_colour, 3);
        assert_eq!(s.rejected_as_semi_transparent, 1);
        assert_eq!(s.other_outcomes, 1);
        assert_eq!(s.single_ink_probes, 3);
        assert_eq!(s.multi_ink_probes, 2);
        assert_eq!(s.single_ink_rejected_as_semi_transparent, 1);
        assert_eq!(s.single_ink_byte_identical, 1);
        assert_eq!(s.single_ink_within_one_code, 2);
        assert_eq!(
            s.single_ink_max_byte_difference, 2,
            "the multi-ink probes must not contribute a difference at all"
        );
        assert!(s.not_a_clause.contains("1.5"));
    }
}
