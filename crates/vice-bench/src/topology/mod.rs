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

pub mod ambiguity;
pub mod gate;
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

/// Radius of the knockout disk, as a fraction of `min(width, height)`.
///
/// RT45-A12: this was the literal `0.3` in the middle of `measure_arm`, and it
/// decides whether the §28 M4.5 clause-1 control measures anything. Setting it
/// to `0.0001` empties the unrelated field, takes the knockout to 0 of 100 and
/// left the clause MET — a number that can switch a gate control off is a gate
/// number, and §27.7 wants gate numbers in the frozen file where a feature
/// commit cannot move them alongside the code they judge.
pub const KNOCKOUT_DISK_RADIUS_FRACTION: f64 = 0.3;

/// The coverage level at which exact ink becomes inside/outside for the ground
/// truth (§5.3 majority rule).
///
/// Registered for the same reason and a stronger one: this constant does not
/// tune a control, it defines the TRUTH the whole clause is scored against.
/// Moving it re-labels every fixture at once, and it must not be movable in the
/// same commit as the code that is being scored.
pub const GT_MAJORITY_LEVEL: f64 = 0.5;

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
    /// §27.1 keeps splits by SHAPE FAMILY, and §27.4 makes the source-scene
    /// family the unit of a reliability trial. `group_id` is finer than that —
    /// `proc/annulus/{000,001,003}` are three variants of ONE family — so a
    /// breadth number counted in groups cites dependent trials as independent
    /// (M45-N3, RT45-A8).
    pub shape_family: String,
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
    /// The same question for an envelope built from a field that has NOTHING
    /// to do with the scene.
    ///
    /// The knockout control of condition 4, and the number it produces is
    /// uncomfortable on purpose: on an arm whose GT is (1, 0) — 69 of 100 —
    /// almost any single blob matches, so the metric cannot tell a working
    /// generator from a synthetic disk there. Meta-rule M-2 says a metric that
    /// answers the same with and without the mechanism is a coincidence and
    /// not a proof; the only way to know which one this is, is to measure it.
    pub gt_in_envelope_unrelated_field: bool,
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
    /// Candidates that CARRIED the GT reading and were removed by the budget,
    /// counted from the removal record — i.e. from the state BEFORE tier 3 —
    /// against the envelope that survived it.
    ///
    /// M45-N8: the previous number was
    /// `!gt_in_envelope && budget_removed > 0`, and `gt_in_envelope` is only
    /// ever computed on the KEPT set, so it could not be non-zero in any world
    /// where recall was 100 % — while sitting in the same conjunction as
    /// `hits == arms`. It was a paraphrase published as an independent
    /// measurement. This one can be non-zero at 100 % recall, which is exactly
    /// the near-miss a reader of §36 wants to see.
    pub budget_removed_gt_class_candidates: usize,
    /// The §36 stop condition proper: the budget removed a candidate carrying
    /// the GT reading and NONE survived. Two states, not one.
    pub budget_removed_the_last_gt_class_candidate: bool,
    pub continuation_plans: usize,
    pub continuation_partial_steps: usize,
    pub continuation_refused_steps: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RefusedArm {
    pub scene_id: String,
    pub cell_id: String,
    /// The corpus's identifiability label for the render that was refused.
    ///
    /// Without it the artifact publishes the SIZE of the exclusion and not its
    /// COMPOSITION, and the composition is what matters here: the excluding
    /// predicate is the M4 evidence stage, i.e. the same pipeline whose output
    /// the clause checks, so difficulty correlates with refusal (M45-N3).
    /// 44 of the 52 refusals turned out to be `identifiable` renders, and the
    /// families they belong to are exactly the multi-component ones.
    pub identifiability: &'static str,
    pub shape_family: String,
    pub reason: String,
}

/// Why one arm did not reach the topology stage, with what it excluded.
struct ArmRefusal {
    identifiability: &'static str,
    reason: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TopologyRun {
    pub config: TopologyConfigRecord,
    pub config_hash: String,
    pub fixture_set_hash: String,
    pub scenes: u64,
    pub arms: Vec<TopologyArm>,
    pub refused: Vec<RefusedArm>,
    pub ambiguity: Vec<ambiguity::AmbiguityRow>,
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

pub(super) fn view_for(cell: &DegradationCell) -> ViewTransform {
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
pub(crate) fn exact_ink_coverage(scene: &GtScene, t: &ViewTransform) -> Result<Vec<f64>, String> {
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
pub(crate) fn gt_signature(
    scene: &GtScene,
    t: &ViewTransform,
    conn: ComplementaryConnectivity,
) -> Result<GtSignature, String> {
    let ink = exact_ink_coverage(scene, t)?;
    let inside: Vec<bool> = ink.iter().map(|v| *v >= GT_MAJORITY_LEVEL).collect();
    let s = independent::signature_of(&inside, t.width_px as usize, t.height_px as usize, conn);
    Ok(GtSignature {
        components: s.components,
        holes: s.holes,
    })
}

pub(super) fn observations_for<'a>(
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
                match measure_arm(scene, cell, split, members, &group.shape_family) {
                    Ok(Some(row)) => arms.push(row),
                    Ok(None) => opaque_arms += 1,
                    Err(detail) => refused.push(RefusedArm {
                        scene_id: scene.id().to_string(),
                        cell_id: cell.id(),
                        identifiability: detail.identifiability,
                        shape_family: group.shape_family.clone(),
                        reason: detail.reason,
                    }),
                }
            }
        }
    }

    let (ambiguity, ambiguity_skipped) = ambiguity::measure_ambiguity_pairs()?;

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
    shape_family: &str,
) -> Result<Option<TopologyArm>, ArmRefusal> {
    let truth_exterior = scene.scene().scene().formation.exterior;
    if truth_exterior != ExteriorModel::Transparent {
        return Ok(None);
    }
    let fixture = render_cell(scene, cell, members).map_err(|reason| ArmRefusal {
        identifiability: "not_rendered",
        reason,
    })?;
    let identifiability = fixture.identifiability;
    let img = CanonicalImage::from_straight_srgb8(
        fixture.width_px,
        fixture.height_px,
        fixture.rgba8.clone(),
        true,
        IccAssumption::NoProfileAssumedSrgb,
    )
    .map_err(|e| ArmRefusal {
        identifiability: identifiability.as_str(),
        reason: e.to_string(),
    })?;

    let out = analyze_full(&img, &ANALYSIS_CONFIG_V1, None);
    let outcome = match &out.report.outcome {
        Flat2Outcome::Supported { .. } => "supported",
        Flat2Outcome::Ambiguous { .. } => "ambiguous",
        Flat2Outcome::Unsupported(_) => "unsupported",
    };
    let Some(ev) = out.chosen else {
        return Err(ArmRefusal {
            identifiability: identifiability.as_str(),
            reason: format!(
                "the evidence stage returned {outcome} for {} at {}, so there is no coverage \
                 field to take a topology of",
                scene.id(),
                cell.id()
            ),
        });
    };

    let t = view_for(cell);
    let [four, eight] = ComplementaryConnectivity::arms();
    let refuse = |reason: String| ArmRefusal {
        identifiability: identifiability.as_str(),
        reason,
    };
    let gt_four = gt_signature(scene, &t, four).map_err(refuse)?;
    let gt_eight = gt_signature(scene, &t, eight).map_err(refuse)?;

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
    // The state BEFORE tier 3: every candidate the budget removed, with the
    // reading it carried. `Removed` records the signature counts precisely so
    // that this comparison is possible without keeping a second envelope.
    let budget_removed_gt = proposal
        .envelope
        .pruning
        .removed
        .iter()
        .filter(|rm| {
            rm.tier == "budget" && matches_gt((rm.components, rm.holes), gt_four, gt_eight)
        })
        .count();
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

    // The KNOCKOUT: the same generator on a field that is not this scene.
    // A centred disk of radius KNOCKOUT_DISK_RADIUS_FRACTION·min(w, h) — no
    // palette, no formation and no pixel of the render enters it.
    let (kw, kh) = (ev.width_px() as usize, ev.height_px() as usize);
    let unrelated: Vec<f64> = {
        let (cx, cy) = (kw as f64 / 2.0, kh as f64 / 2.0);
        let r = KNOCKOUT_DISK_RADIUS_FRACTION * (kw.min(kh) as f64);
        (0..kw * kh)
            .map(|i| {
                let (x, y) = ((i % kw) as f64 + 0.5, (i / kw) as f64 + 0.5);
                let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
                (r + 0.5 - d).clamp(0.0, 1.0)
            })
            .collect()
    };
    let unrelated_obs = CoverageObservation {
        palette_id: "knockout".to_string(),
        formation_id: obs.formation_id.clone(),
        filter: obs.filter,
        filter_identifiable: obs.filter_identifiable,
        alpha: &unrelated,
        width_px: kw,
        height_px: kh,
    };
    let knockout = propose(std::slice::from_ref(&unrelated_obs), &TOPOLOGY_CONFIG_V1);
    let knockout_hit = knockout.envelope.hypotheses.iter().any(hit);

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
        shape_family: shape_family.to_string(),
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
        gt_in_envelope_unrelated_field: knockout_hit,
        matching_fields,
        unique_field,
        events: proposal.events_seen,
        tie_batches: proposal.tie_batches,
        largest_batch_pixels: proposal.largest_batch_pixels,
        saddle_alternatives: proposal.saddle_alternatives_generated,
        budget_removed: proposal.envelope.pruning.budget_removed,
        dominated_removed: proposal.envelope.pruning.dominated_removed,
        budget_removed_gt_class_candidates: budget_removed_gt,
        budget_removed_the_last_gt_class_candidate: budget_removed_gt > 0 && matching.is_empty(),
        continuation_plans: plans.len(),
        continuation_partial_steps: plans.iter().map(|p| p.partial_steps()).sum(),
        continuation_refused_steps: plans.iter().map(|p| p.refused_steps()).sum(),
    }))
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_envelope;
