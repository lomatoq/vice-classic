//! Clause 2 of §28 M4.5: the intentionally ambiguous pairs of the corpus.
//!
//! In its own module because `topology/mod.rs` crossed the 800-line rule of
//! §4.1 when the delta added the exclusion composition and the knockout — and
//! that rule is enforced by a test, so it moves code rather than being quoted.
//!
//! What the clause asks is not whether the envelope reproduces the render's
//! own digitization: at the collapse cell both scenes produce the same bytes,
//! so that would be trivially true. It is whether the envelope keeps BOTH
//! readings — the topology scene A really has and the one scene B really has,
//! each taken where they are distinguishable.

use serde::Serialize;
use vice_evidence::analysis::{analyze_full, ANALYSIS_CONFIG_V1};
use vice_image::{CanonicalImage, IccAssumption};
use vice_ir::ComplementaryConnectivity;
use vice_topology::{propose, TopologyConfig, TOPOLOGY_CONFIG_V1};

use super::{gt_signature, observations_for, view_for, GtSignature, FIXED_ONLY_LEVELS};
use crate::gt::degradation::{render_cell, DegradationCell};
use crate::gt::split::{Split, SPLIT_POLICY_V1};
use crate::gt::GtScene;

/// One ambiguity pair, measured at the cell where the two scenes collapse.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AmbiguityRow {
    pub group_id: String,
    pub family: String,
    pub collapse_cell: String,
    pub separate_cell: String,
    pub scene_a: String,
    pub scene_b: String,
    /// The two scenes' own topologies, taken where they are distinguishable.
    pub sig_a: GtSignature,
    pub sig_b: GtSignature,
    /// False when the pair is not a TOPOLOGY ambiguity at all (the two
    /// scenes have the same ink topology), in which case the row is excluded
    /// from the clause and says so.
    pub is_topology_pair: bool,
    /// Rendering A at the collapse cell: does the envelope keep both
    /// readings? And the same rendering B.
    pub both_retained_from_a: Option<bool>,
    pub both_retained_from_b: Option<bool>,
    pub classes_from_a: Vec<(u32, u32)>,
    pub classes_from_b: Vec<(u32, u32)>,
    /// The same question asked of a generator whose ONLY source is the fixed
    /// smoke probe. This is where "no magic-threshold-only architecture"
    /// stops being a claim: one level produces one labelling, so it can
    /// return one reading and not two, and the number says so.
    pub both_retained_fixed_only_from_a: Option<bool>,
    pub both_retained_fixed_only_from_b: Option<bool>,
    pub classes_fixed_only_from_a: Vec<(u32, u32)>,
    pub classes_fixed_only_from_b: Vec<(u32, u32)>,
    /// The corpus's OWN frozen identifiability label for the two renders at
    /// the collapse cell.
    ///
    /// DIAGNOSTIC ONLY. It is printed beside the excuse and it is NOT the
    /// excuse: the predicate reads `collapse_max_code_diff` against the frozen
    /// `identifiability.quantization_floor_codes`, and this label is not read
    /// at all. The previous version of this comment said the opposite in two
    /// places (M45-N4), and the two labels of a pair can differ from each
    /// other (`information_lost` and `equivalent_family` on `hole-or-not`), so
    /// a reader who believed the comment would think the excuse rested on a
    /// value that does not even agree with itself across the pair.
    ///
    /// A doc comment that names the deciding instrument is a claim about the
    /// code and is checked like a number, not like prose.
    pub identifiability_at_collapse_a: &'static str,
    pub identifiability_at_collapse_b: &'static str,
    /// Premultiplied max code difference between the two renders at the
    /// collapse cell, so the collapse is a number here too.
    pub collapse_max_code_diff: f64,
    pub note: &'static str,
}

/// The intentionally ambiguous pairs, at the cell where they collapse.
///
/// The question is not whether the envelope reproduces the render's own
/// digitization — at the collapse cell both scenes produce the same bytes, so
/// that would be trivially true. It is whether the envelope keeps BOTH
/// readings: the topology scene A really has and the topology scene B really
/// has, each taken where they are distinguishable.
pub(super) fn measure_ambiguity_pairs() -> Result<(Vec<AmbiguityRow>, u64), String> {
    let mut out = Vec::new();
    let mut skipped = 0u64;
    for pair in crate::gt::adversarial::ambiguity_pairs() {
        let g = &pair.group;
        // §27.1: scoring the sealed audit is what OPENS it. The recall loop
        // filters it; this loop reads the pairs directly, so it has to
        // filter it too. Applying the rule to one loop and not the other is
        // F-0026 verbatim.
        if SPLIT_POLICY_V1.split_of_group(g) == Split::SealedAudit {
            skipped += 1;
            continue;
        }
        if g.scenes.len() != 2 {
            continue;
        }
        let (a, b) = (&g.scenes[0], &g.scenes[1]);
        let sep = view_for(&pair.separate_cell);
        let [four, eight] = ComplementaryConnectivity::arms();
        // Both conventions, not one. `matches_gt` counts a candidate as a hit
        // if it matches EITHER admissible reading, so a pair may only be
        // EXCLUDED from the clause when it is the same topology under BOTH
        // (M45-N13). An excluding predicate coarser than the including one
        // decides the excluded set with the blunter instrument.
        let sig_a = gt_signature(a, &sep, four)?;
        let sig_b = gt_signature(b, &sep, four)?;
        let sig_a8 = gt_signature(a, &sep, eight)?;
        let sig_b8 = gt_signature(b, &sep, eight)?;
        let is_topology_pair = sig_a != sig_b || sig_a8 != sig_b8;

        // The two renders at the collapse cell. `collapse_max_code_diff` is
        // what the excuse is measured with; the identifiability labels are
        // recorded beside it as diagnostics and are NOT read by the predicate
        // (M45-N4).
        let ra = render_cell(a, &pair.collapse_cell, 2)?;
        let rb = render_cell(b, &pair.collapse_cell, 2)?;
        let ident_a = ra.identifiability.as_str();
        let ident_b = rb.identifiability.as_str();
        let code_diff = crate::gt::colour::max_premultiplied_code_difference(&ra.rgba8, &rb.rgba8);

        let mut row = AmbiguityRow {
            group_id: g.id.clone(),
            family: g.shape_family.clone(),
            collapse_cell: pair.collapse_cell.id(),
            separate_cell: pair.separate_cell.id(),
            scene_a: a.id().to_string(),
            scene_b: b.id().to_string(),
            sig_a,
            sig_b,
            is_topology_pair,
            both_retained_from_a: None,
            both_retained_from_b: None,
            classes_from_a: Vec::new(),
            classes_from_b: Vec::new(),
            both_retained_fixed_only_from_a: None,
            both_retained_fixed_only_from_b: None,
            classes_fixed_only_from_a: Vec::new(),
            classes_fixed_only_from_b: Vec::new(),
            identifiability_at_collapse_a: ident_a,
            identifiability_at_collapse_b: ident_b,
            collapse_max_code_diff: code_diff,
            note: if is_topology_pair {
                "both scenes' own topologies must be present in the envelope built from either \
                 render at the collapse cell"
            } else {
                "the two scenes have the SAME ink topology, so this pair is an ambiguity about \
                 paint or partition and not about topology; excluded from the clause and counted \
                 here so the exclusion is visible"
            },
        };

        if is_topology_pair {
            let wants = |classes: &[(u32, u32)]| {
                classes.contains(&(sig_a.components, sig_a.holes))
                    && classes.contains(&(sig_b.components, sig_b.holes))
            };
            for (scene, first) in [(a, true), (b, false)] {
                let full = envelope_classes(scene, &pair.collapse_cell, &TOPOLOGY_CONFIG_V1)?;
                let fixed_cfg = TopologyConfig {
                    level: FIXED_ONLY_LEVELS,
                    ..TOPOLOGY_CONFIG_V1
                };
                let fixed = envelope_classes(scene, &pair.collapse_cell, &fixed_cfg)?;
                let (bf, bx) = (wants(&full), wants(&fixed));
                if first {
                    row.both_retained_from_a = Some(bf);
                    row.classes_from_a = full;
                    row.both_retained_fixed_only_from_a = Some(bx);
                    row.classes_fixed_only_from_a = fixed;
                } else {
                    row.both_retained_from_b = Some(bf);
                    row.classes_from_b = full;
                    row.both_retained_fixed_only_from_b = Some(bx);
                    row.classes_fixed_only_from_b = fixed;
                }
            }
        }
        out.push(row);
    }
    Ok((out, skipped))
}

/// `pub(crate)` so a test can recompute the SAME reading the row publishes.
///
/// RT45-A24: the threshold site filtered the published list by a plausibility
/// bound, and a bound describes the sentinel someone already showed you. The
/// property is not "the number looks reasonable" but "this class came out of
/// the envelope", and the only way to say that is to ask the envelope again.
pub(crate) fn envelope_classes(
    scene: &GtScene,
    cell: &DegradationCell,
    cfg: &TopologyConfig,
) -> Result<Vec<(u32, u32)>, String> {
    let fixture = render_cell(scene, cell, 2)?;
    let img = CanonicalImage::from_straight_srgb8(
        fixture.width_px,
        fixture.height_px,
        fixture.rgba8,
        true,
        IccAssumption::NoProfileAssumedSrgb,
    )
    .map_err(|e| e.to_string())?;
    let out = analyze_full(&img, &ANALYSIS_CONFIG_V1, None);
    let Some(ev) = out.chosen else {
        return Err(format!(
            "no coverage field for {} at {}",
            scene.id(),
            cell.id()
        ));
    };
    let obs = observations_for(&ev, "ambiguity");
    let p = propose(std::slice::from_ref(&obs), cfg);
    Ok(p.envelope.signature_classes().into_iter().collect())
}
