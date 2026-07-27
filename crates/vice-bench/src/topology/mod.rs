//! Candidate-recall harness for the topology envelope (spec §11.3, §23,
//! §28 M4.5).
//!
//! ```text
//! Gate: GT-equivalent topology PRESENT IN ENVELOPE on identifiable
//!       supported fixtures; ambiguous fixtures retain alternatives;
//!       no magic-threshold-only architecture.
//! ```
//!
//! Three clauses, three measurements, and each of them is a number over a
//! NAMED population rather than a sentence.
//!
//! ## What the ground truth of a topology is here, and why it is that
//!
//! The observation the envelope sees is a coverage field, so the truth it
//! has to be compared with is the topology of the region that field is the
//! coverage OF: the union of the scene's OPAQUE faces. Not the partition's
//! face count, and not the complement of the exterior — FAILURE_LEDGER
//! F-0025 is exactly the second mistake, where a hole face was counted as
//! ink because "ink" had been defined as "not the exterior".
//!
//! The continuous ink region is digitized on the render grid by the MAJORITY
//! rule: a pixel belongs to it when the exact area of ink inside that pixel
//! is at least half. Three things make that the honest choice rather than a
//! threshold smuggled onto the truth side:
//!
//! - it is computed by the INDEPENDENT exact-clip integrator and never by
//!   the production renderer (REVIEW_M3 M3-N5);
//! - it is a function of the scene and the view transform only. No estimate,
//!   no palette, no formation enters it, so it cannot be tuned by anything
//!   the envelope does;
//! - a fixture where the digitization is itself uncertain is exactly a
//!   fixture the corpus labels `InformationLost` or `EquivalentFamily`, and
//!   the recall clause counts only `Identifiable` renders.
//!
//! §5.3 gives TWO admissible complementary-connectivity conventions and this
//! milestone treats both as hypotheses, so the truth is computed under both
//! and a candidate matching EITHER counts. That is the "admissible visible
//! scene equivalence class" of §1.5 applied to digital topology, and both
//! readings are published so a reviewer can see when they differ.
//!
//! ## Scope of the recall clause, stated rather than implied
//!
//! Only TRANSPARENT-exterior scenes. For a full-bleed scene the ink region
//! is whichever face is not the background, and deciding that is a palette
//! question this milestone does not answer; a truth field that guessed it
//! would be F-0025 a second time. Opaque-exterior arms are still run and
//! still counted — as a NAMED excluded population with its size, which is
//! the form condition C2 asked of the §1.6 clause.

pub mod independent;
pub mod report;

use serde::Serialize;
use vice_evidence::analysis::{analyze_full, Flat2Outcome, ANALYSIS_CONFIG_V1};
use vice_image::{CanonicalImage, IccAssumption};
use vice_ir::{ComplementaryConnectivity, ExteriorModel, PixelFilter};
use vice_topology::{
    propose, CoverageObservation, LevelConfig, TopologyConfig, LEVEL_CONFIG_V1, TOPOLOGY_CONFIG_V1,
};

use crate::gt::corpus::all_groups;
use crate::gt::degradation::{matrix_v1, render_cell, DegradationCell, ResizeChain};
use crate::gt::grammar::AUTHORING_CANVAS_PX;
use crate::gt::raster::{rasterize, Psf, RasterProfile, ViewTransform};
use crate::gt::split::{Split, SPLIT_POLICY_V1};
use crate::gt::GtScene;
use crate::hashing::sha256_hex;

pub const TOPOLOGY_RUN_SCHEMA: &str = "vice-classic/topology-recall/v1";

/// Cells the recall run covers.
///
/// Deliberately smaller than the corridor's fourteen. The corridor is
/// calibrating an interval and needs every conditional axis of §13.1; this
/// run is asking a yes/no question about a SET, and the axes that move the
/// answer are resolution, the engine that drew the image and the kernel that
/// blurred it. Every id below is a cell of the frozen §27.2 matrix — a run
/// that invented a cell would be measuring a corpus that does not exist.
pub const TOPOLOGY_CELL_IDS: &[&str] = &[
    // resolution axis, our exact integrator
    "s32_pexact-clip_box_lin_none_dx0.00dy0.00_c1.00",
    "s64_pexact-clip_box_lin_none_dx0.00dy0.00_c1.00",
    "s128_pexact-clip_box_lin_none_dx0.00dy0.00_c1.00",
    // independent engines, including the HELD-OUT one
    "s64_praqote_box_lin_none_dx0.00dy0.00_c1.00",
    "s64_ptiny-skia_box_lin_none_dx0.00dy0.00_c1.00",
    // a kernel wide enough that the coverage field is genuinely blurred
    "s32_psupersample_gauss0.50_lin_none_dx0.00dy0.00_c1.00",
];

const TEST_SCOPE_CELLS: &[&str] = &["s32_pexact-clip_box_lin_none_dx0.00dy0.00_c1.00"];

/// The fixed-probe-only generator used by the ablation of clause 3.
///
/// Not a second architecture: the SAME generator with its event-driven
/// sources switched off, so the contrast measures the sources and not two
/// different programs.
pub const FIXED_ONLY_LEVELS: LevelConfig = LevelConfig {
    max_plateau_levels: 0,
    max_event_levels: 0,
    min_event_persistence: LEVEL_CONFIG_V1.min_event_persistence,
    fixed_smoke_levels: LEVEL_CONFIG_V1.fixed_smoke_levels,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TopologyScope {
    Full,
    Test,
}

impl TopologyScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            TopologyScope::Full => "full",
            TopologyScope::Test => "test",
        }
    }
    fn stride(&self) -> usize {
        match self {
            TopologyScope::Full => 1,
            TopologyScope::Test => 12,
        }
    }
    fn cells(&self) -> &'static [&'static str] {
        match self {
            TopologyScope::Full => TOPOLOGY_CELL_IDS,
            TopologyScope::Test => TEST_SCOPE_CELLS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TopologyConfigRecord {
    pub schema: &'static str,
    pub scope: &'static str,
    pub cells: Vec<String>,
    pub splits_measured: Vec<&'static str>,
    pub topology: TopologyConfig,
    pub fixed_only_levels: LevelConfig,
}

impl TopologyConfigRecord {
    pub fn v1(scope: TopologyScope) -> TopologyConfigRecord {
        TopologyConfigRecord {
            schema: TOPOLOGY_RUN_SCHEMA,
            scope: scope.as_str(),
            cells: scope.cells().iter().map(|s| (*s).to_string()).collect(),
            splits_measured: vec![Split::Development.as_str(), Split::Calibration.as_str()],
            topology: TOPOLOGY_CONFIG_V1,
            fixed_only_levels: FIXED_ONLY_LEVELS,
        }
    }
    pub fn hash(&self) -> String {
        sha256_hex(
            serde_json::to_string(self)
                .expect("topology config serializes")
                .as_bytes(),
        )
    }
}

/// The digital topology of a region, under one convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct GtSignature {
    pub components: u32,
    pub holes: u32,
}

/// One measured arm of the recall run.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TopologyArm {
    pub scene_id: String,
    pub group_id: String,
    pub cell_id: String,
    pub split: &'static str,
    pub profile: &'static str,
    pub size_px: u32,
    pub identifiability: &'static str,
    pub exterior_truth: &'static str,
    pub outcome: String,
    /// The GT digital topology under each admissible convention.
    pub gt_four: GtSignature,
    pub gt_eight: GtSignature,
    pub candidates: usize,
    /// Surviving candidates from each complementary arm, `(fg 4, fg 8)`.
    ///
    /// RT45-A1: the recall clause relaxes its success condition by pointing at
    /// a mechanism — a candidate matching EITHER convention's truth counts,
    /// "because we keep both arms" — and nothing measured that both arms were
    /// there. Deleting one from the generator left the gate table, the config
    /// hash and the structural projection unmoved.
    pub candidates_by_arm: (usize, usize),
    pub signature_classes: usize,
    /// Whether the envelope contains a candidate matching either GT reading.
    pub gt_in_envelope: bool,
    /// The same question over the candidates that would exist WITHOUT any
    /// fixed smoke probe.
    pub gt_in_envelope_events_only: bool,
    /// The same question for a generator whose ONLY source is the fixed
    /// probes.
    pub gt_in_envelope_fixed_only: bool,
    /// Fields of §11.1 that produced a matching candidate.
    pub matching_fields: Vec<&'static str>,
    /// Fields whose removal would lose the match, i.e. the only field that
    /// produced it.
    pub unique_field: Option<&'static str>,
    pub events: usize,
    pub tie_batches: u32,
    pub largest_batch_pixels: u32,
    pub saddle_alternatives: usize,
    pub budget_removed: usize,
    pub dominated_removed: usize,
    pub continuation_plans: usize,
    pub continuation_executed_steps: usize,
    pub continuation_partial_steps: usize,
    pub continuation_refused_steps: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RefusedArm {
    pub scene_id: String,
    pub cell_id: String,
    pub reason: String,
}

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
    /// This is the only thing allowed to excuse a pair whose readings are
    /// not both retained, and it is not this milestone's judgement: the
    /// label comes from the M3 rule that compares the distinguishing
    /// feature's size in render px with the FROZEN
    /// `[identifiability] observability_floor_px`. §1.5 says in terms that a
    /// feature which leaves no trace is not recoverable, and requiring an
    /// envelope to carry one would be requiring invention.
    pub identifiability_at_collapse_a: &'static str,
    pub identifiability_at_collapse_b: &'static str,
    /// Premultiplied max code difference between the two renders at the
    /// collapse cell, so the collapse is a number here too.
    pub collapse_max_code_diff: f64,
    pub note: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TopologyRun {
    pub config: TopologyConfigRecord,
    pub config_hash: String,
    pub fixture_set_hash: String,
    pub scenes: u64,
    pub arms: Vec<TopologyArm>,
    pub refused: Vec<RefusedArm>,
    pub ambiguity: Vec<AmbiguityRow>,
    pub sealed_audit_groups_skipped: u64,
    /// Ambiguity pairs skipped because their GROUP is in the sealed audit.
    ///
    /// Zero today, and the point is that it is a NUMBER rather than a fact
    /// about the current split assignment. The recall loop filters the audit
    /// and the pair loop reads `adversarial::ambiguity_pairs()` directly, so
    /// without this the rule "the sealed audit is never scored" would be
    /// applied to one of the two loops — which is FAILURE_LEDGER F-0026
    /// exactly (four measurements filtered, the fifth did not).
    pub ambiguity_pairs_in_sealed_audit_skipped: u64,
    /// Arms whose scene has an OPAQUE exterior: run, counted, and excluded
    /// from the recall clause with the reason in the gate row.
    pub opaque_exterior_arms: u64,
}

fn resolve_cells(ids: &[&str]) -> Result<Vec<DegradationCell>, String> {
    let matrix = matrix_v1();
    ids.iter()
        .map(|want| {
            matrix
                .iter()
                .find(|c| c.id() == *want)
                .copied()
                .ok_or_else(|| format!("topology cell {want} is not a cell of the frozen matrix"))
        })
        .collect()
}

fn view_for(cell: &DegradationCell) -> ViewTransform {
    ViewTransform {
        scale: f64::from(cell.size_px) / f64::from(AUTHORING_CANVAS_PX),
        dx: cell.subpixel_dx,
        dy: cell.subpixel_dy,
        width_px: cell.size_px,
        height_px: cell.size_px,
    }
}

/// The exact ink coverage of a scene at one view, from the INDEPENDENT
/// exact-clip integrator.
///
/// Ink is the union of the OPAQUE faces. A hole face is not the exterior and
/// is not ink either — its paint is `TransparentExterior` by IR contract, and
/// treating "not exterior" as "ink" is F-0025.
pub fn exact_ink_coverage(scene: &GtScene, t: &ViewTransform) -> Result<Vec<f64>, String> {
    let stack = rasterize(scene.certified(), t, RasterProfile::ExactClip, Psf::Box)?;
    let opaque: Vec<usize> = scene
        .scene()
        .graph()
        .faces
        .iter()
        .enumerate()
        .filter(|(_, f)| matches!(f.paint, vice_ir::Paint::OpaqueSolid(_)))
        .map(|(fi, _)| fi)
        .collect();
    let n = (t.width_px as usize) * (t.height_px as usize);
    Ok((0..n)
        .map(|i| {
            opaque
                .iter()
                .map(|fi| stack.per_face[*fi][i])
                .sum::<f64>()
                .clamp(0.0, 1.0)
        })
        .collect())
}

/// The GT digital topology of a scene at one view, under one convention.
///
/// Majority rule: a pixel is ink when at least half its area is ink.
///
/// ## Nothing in this function calls `vice-topology` (RT45-A2)
///
/// It used to call `vice_topology::threshold` and `vice_topology::signature` —
/// the same functions that sign the candidates — so only the AREA INTEGRATOR
/// was independent. The red team put the convention error §5.3 forbids by name
/// into `signature` and watched it cancel between the two sides: three clauses
/// MET, 489 tests green, **0 of 132 arms changed their GT signature**.
///
/// The majority rule is two lines and is written here; the three numbers come
/// from [`independent`], which counts components by flood fill and DERIVES
/// holes from a bit-quad Euler characteristic — the opposite direction from
/// production, which counts holes and derives Euler. A convention error can no
/// longer cancel, and `the_independent_chain_agrees_with_the_production_signature`
/// is the witness that says so on a diagonal ring.
pub fn gt_signature(
    scene: &GtScene,
    t: &ViewTransform,
    conn: ComplementaryConnectivity,
) -> Result<GtSignature, String> {
    let ink = exact_ink_coverage(scene, t)?;
    let inside: Vec<bool> = ink.iter().map(|v| *v >= 0.5).collect();
    let s = independent::signature_of(&inside, t.width_px as usize, t.height_px as usize, conn);
    Ok(GtSignature {
        components: s.components,
        holes: s.holes,
    })
}

fn observations_for<'a>(
    ev: &'a vice_evidence::Flat2Evidence,
    palette_id: &str,
) -> CoverageObservation<'a> {
    CoverageObservation {
        palette_id: palette_id.to_string(),
        formation_id: vice_evidence::formation::formation_id(&ev.formation),
        filter: ev.formation.pixel_filter,
        filter_identifiable: vice_evidence::formation::filter_is_identifiable(ev.alpha_field()),
        alpha: ev.alpha_field(),
        width_px: ev.width_px() as usize,
        height_px: ev.height_px() as usize,
    }
}

fn filter_id(f: PixelFilter) -> String {
    match f {
        PixelFilter::Box => "box".to_string(),
        PixelFilter::Triangle => "triangle".to_string(),
        PixelFilter::Gaussian { sigma_px } => format!("gauss{sigma_px:.2}"),
    }
}

fn matches_gt(sig: (u32, u32), a: GtSignature, b: GtSignature) -> bool {
    (sig.0 == a.components && sig.1 == a.holes) || (sig.0 == b.components && sig.1 == b.holes)
}

/// Run the recall harness.
pub fn run(scope: TopologyScope) -> Result<TopologyRun, String> {
    let config = TopologyConfigRecord::v1(scope);
    let config_hash = config.hash();
    let cells = resolve_cells(scope.cells())?;
    let groups = all_groups()?;
    let policy = &SPLIT_POLICY_V1;

    let mut arms = Vec::new();
    let mut refused = Vec::new();
    let mut digests = Vec::new();
    let mut scenes = 0u64;
    let mut skipped_audit = 0u64;
    let mut opaque_arms = 0u64;

    for group in groups.iter().step_by(scope.stride()) {
        let split = policy.split_of_group(group);
        if split == Split::SealedAudit {
            // §27.1: scoring the sealed audit is what OPENS it.
            skipped_audit += 1;
            continue;
        }
        let members = group
            .equivalence_class
            .as_ref()
            .map_or(1, |e| e.members.len());
        for scene in &group.scenes {
            scenes += 1;
            digests.push(
                vice_ir::scene_digest_sha256(scene.scene().scene()).map_err(|e| e.to_string())?,
            );
            for cell in &cells {
                if cell.resize != ResizeChain::None {
                    continue;
                }
                if !policy.profile_allowed(split, cell.profile.as_str()) {
                    continue;
                }
                match measure_arm(scene, cell, split, members) {
                    Ok(Some(row)) => arms.push(row),
                    Ok(None) => opaque_arms += 1,
                    Err(reason) => refused.push(RefusedArm {
                        scene_id: scene.id().to_string(),
                        cell_id: cell.id(),
                        reason,
                    }),
                }
            }
        }
    }

    let (ambiguity, ambiguity_skipped) = measure_ambiguity_pairs()?;

    digests.sort();
    Ok(TopologyRun {
        config,
        config_hash,
        fixture_set_hash: sha256_hex(digests.join("\u{1f}").as_bytes()),
        scenes,
        arms,
        refused,
        ambiguity,
        ambiguity_pairs_in_sealed_audit_skipped: ambiguity_skipped,
        sealed_audit_groups_skipped: skipped_audit,
        opaque_exterior_arms: opaque_arms,
    })
}

/// One arm, or `None` when the scene has an opaque exterior (the named
/// excluded population).
fn measure_arm(
    scene: &GtScene,
    cell: &DegradationCell,
    split: Split,
    members: usize,
) -> Result<Option<TopologyArm>, String> {
    let truth_exterior = scene.scene().scene().formation.exterior;
    if truth_exterior != ExteriorModel::Transparent {
        return Ok(None);
    }
    let fixture = render_cell(scene, cell, members)?;
    let identifiability = fixture.identifiability;
    let img = CanonicalImage::from_straight_srgb8(
        fixture.width_px,
        fixture.height_px,
        fixture.rgba8.clone(),
        true,
        IccAssumption::NoProfileAssumedSrgb,
    )
    .map_err(|e| e.to_string())?;

    let out = analyze_full(&img, &ANALYSIS_CONFIG_V1, None);
    let outcome = match &out.report.outcome {
        Flat2Outcome::Supported { .. } => "supported",
        Flat2Outcome::Ambiguous { .. } => "ambiguous",
        Flat2Outcome::Unsupported(_) => "unsupported",
    };
    let Some(ev) = out.chosen else {
        return Err(format!(
            "the evidence stage returned {outcome} for {} at {}, so there is no coverage field to \
             take a topology of",
            scene.id(),
            cell.id()
        ));
    };

    let t = view_for(cell);
    let [four, eight] = ComplementaryConnectivity::arms();
    let gt_four = gt_signature(scene, &t, four)?;
    let gt_eight = gt_signature(scene, &t, eight)?;

    let palette_id = out
        .report
        .chosen()
        .map(|e| e.id.clone())
        .unwrap_or_else(|| "chosen".to_string());
    let obs = observations_for(&ev, &palette_id);
    let proposal = propose(std::slice::from_ref(&obs), &TOPOLOGY_CONFIG_V1);

    let hit = |c: &vice_topology::TopologyHypothesis| {
        matches_gt(
            (c.signature.components, c.signature.holes),
            gt_four,
            gt_eight,
        )
    };
    let matching: Vec<&vice_topology::TopologyHypothesis> = proposal
        .envelope
        .hypotheses
        .iter()
        .filter(|c| hit(c))
        .collect();
    let mut matching_fields: Vec<&'static str> = matching
        .iter()
        .map(|c| c.provenance.field.as_str())
        .collect();
    matching_fields.sort_unstable();
    matching_fields.dedup();
    let unique_field = (matching_fields.len() == 1).then(|| matching_fields[0]);

    let events_only = matching
        .iter()
        .any(|c| !c.ambiguity.level_from_fixed_probe_only);

    // The ablation: the SAME generator with its event-driven sources
    // switched off. A second program would be measuring two programs.
    let fixed_cfg = TopologyConfig {
        level: FIXED_ONLY_LEVELS,
        ..TOPOLOGY_CONFIG_V1
    };
    let fixed = propose(std::slice::from_ref(&obs), &fixed_cfg);
    let fixed_hit = fixed.envelope.hypotheses.iter().any(hit);

    let plans =
        vice_topology::plan_continuations(&proposal.envelope, &TOPOLOGY_CONFIG_V1.continuation);

    Ok(Some(TopologyArm {
        scene_id: scene.id().to_string(),
        group_id: scene.group_id().to_string(),
        cell_id: cell.id(),
        split: split.as_str(),
        profile: cell.profile.as_str(),
        size_px: cell.size_px,
        identifiability: identifiability.as_str(),
        exterior_truth: "transparent",
        outcome: format!("{outcome}/{}", filter_id(ev.formation.pixel_filter)),
        gt_four,
        gt_eight,
        candidates: proposal.envelope.hypotheses.len(),
        candidates_by_arm: proposal.envelope.candidates_by_arm(),
        signature_classes: proposal.envelope.signature_classes().len(),
        gt_in_envelope: !matching.is_empty(),
        gt_in_envelope_events_only: events_only,
        gt_in_envelope_fixed_only: fixed_hit,
        matching_fields,
        unique_field,
        events: proposal.events_seen,
        tie_batches: proposal.tie_batches,
        largest_batch_pixels: proposal.largest_batch_pixels,
        saddle_alternatives: proposal.saddle_alternatives_generated,
        budget_removed: proposal.envelope.pruning.budget_removed,
        dominated_removed: proposal.envelope.pruning.dominated_removed,
        continuation_plans: plans.len(),
        continuation_executed_steps: plans.iter().map(|p| p.executed_steps()).sum(),
        continuation_partial_steps: plans.iter().map(|p| p.partial_steps()).sum(),
        continuation_refused_steps: plans.iter().map(|p| p.refused_steps()).sum(),
    }))
}

/// The intentionally ambiguous pairs, at the cell where they collapse.
///
/// The question is not whether the envelope reproduces the render's own
/// digitization — at the collapse cell both scenes produce the same bytes, so
/// that would be trivially true. It is whether the envelope keeps BOTH
/// readings: the topology scene A really has and the topology scene B really
/// has, each taken where they are distinguishable.
fn measure_ambiguity_pairs() -> Result<(Vec<AmbiguityRow>, u64), String> {
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
        let [four, _] = ComplementaryConnectivity::arms();
        let sig_a = gt_signature(a, &sep, four)?;
        let sig_b = gt_signature(b, &sep, four)?;
        let is_topology_pair = sig_a != sig_b;

        // The two renders at the collapse cell, and the corpus's own frozen
        // identifiability label for each. Rendered here rather than taken on
        // trust: the label is the only thing allowed to excuse a pair whose
        // alternatives are not both retained.
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

fn envelope_classes(
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

#[cfg(test)]
mod tests;
