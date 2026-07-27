//! Corridor calibration on independent GT rasterizers (spec §13.1, §28 M4).
//!
//! §13.1 lists what has to be checked, and this harness produces every line
//! of it from the committed corpus:
//!
//! ```text
//! empirical coverage @50/@90/@95/@99
//! median/p95 width
//! conditional calibration by resolution, contrast, PSF, blend space, phase
//! bias along normals
//! invariance to sample step
//! calibration under a held-out rasterizer
//! ```
//!
//! ## What is being measured against what
//!
//! The observation is a corpus render — bytes produced by an engine that is
//! not ours (§27.1). The evidence stage sees ONLY those bytes: no palette is
//! injected, no formation is injected, no geometry is injected. The truth it
//! is scored against is the certified mesh of the same scene, transformed
//! into render space by the same view transform the cell used. So the number
//! this harness reports is the distance from an extracted boundary to the
//! geometry that actually produced the pixels.
//!
//! ## Splits
//!
//! §27.1 gives three splits and this harness respects all three:
//!
//! - **development** — where the coefficients of the corridor were frozen;
//! - **calibration** — where the coverage is EVALUATED, including under the
//!   held-out rasterizer `tiny-skia` (which the split policy keeps out of
//!   development entirely);
//! - **sealed_audit** — NOT TOUCHED. Scoring it is what opens it (§27.1,
//!   `AuditSeal::check`), and a calibration run is not a release candidate.
//!   The harness skips those groups and the report says how many.
//!
//! ## The GT boundary
//!
//! Taken from the shared BOUNDARIES of the planar graph rather than from
//! face loops, and boundaries incident to the exterior face are dropped when
//! the scene's exterior is OPAQUE: there the background face covers the
//! canvas, so its outer ring is the canvas edge and not a visible interface.
//! Including it would let a sample near the canvas match the wrong curve and
//! report a distance smaller than the truth — an error in the flattering
//! direction, which is the one worth engineering against.

pub mod report;

#[cfg(test)]
mod tests;

use serde::Serialize;
use vice_evidence::analysis::{analyze_full, Flat2Outcome, UnsupportedReason, ANALYSIS_CONFIG_V1};
use vice_evidence::boundary::{observe_boundaries, BoundaryConfig, BOUNDARY_CONFIG_V1};
use vice_evidence::corridor::{CORRIDOR_CONFIG_V1, COVERAGE_LEVELS};
use vice_evidence::palette::BackgroundHypothesis;
use vice_geom::Pt;
use vice_image::{CanonicalImage, IccAssumption};
use vice_ir::{BlendSpace, ExteriorModel, PixelFilter};

use crate::gt::corpus::all_groups;
use crate::gt::degradation::{matrix_v1, render_cell, DegradationCell, ResizeChain};
use crate::gt::grammar::AUTHORING_CANVAS_PX;
use crate::gt::raster::{Psf, RasterProfile};
use crate::gt::split::{Split, SPLIT_POLICY_V1};
use crate::gt::GtScene;
use crate::hashing::sha256_hex;

pub const CORRIDOR_SCHEMA: &str = "vice-classic/m4-corridor/v1";
pub const CORRIDOR_CONFIG_SCHEMA: &str = "vice-classic/m4-corridor-config/v1";

/// The observation cells this harness runs on, BY ID, so every one is a cell
/// of the frozen §27.2 matrix and every observation is literally a corpus
/// render.
///
/// Chosen to make each conditional axis of §13.1 measurable on its own:
/// three resolutions, two independent engines plus the held-out one, the
/// three PSF excursions, the sRGB blend space, two contrasts and a subpixel
/// phase. Resize chains are excluded for the reason
/// `oracle::ceiling::ArmRefusal::ResizeChainNotSupported` gives: the
/// coverage stack lives at work resolution and an edge metric over a
/// resampled mask is a different quantity under the same name.
pub const CORRIDOR_CELL_IDS: &[&str] = &[
    // resolution axis, our exact integrator
    "s32_pexact-clip_box_lin_none_dx0.00dy0.00_c1.00",
    "s64_pexact-clip_box_lin_none_dx0.00dy0.00_c1.00",
    "s128_pexact-clip_box_lin_none_dx0.00dy0.00_c1.00",
    // independent engines, including the HELD-OUT one
    "s32_praqote_box_lin_none_dx0.00dy0.00_c1.00",
    "s64_praqote_box_lin_none_dx0.00dy0.00_c1.00",
    "s32_ptiny-skia_box_lin_none_dx0.00dy0.00_c1.00",
    "s64_ptiny-skia_box_lin_none_dx0.00dy0.00_c1.00",
    "s128_ptiny-skia_box_lin_none_dx0.00dy0.00_c1.00",
    // PSF axis (only the supersampler can realize a non-box kernel)
    "s32_psupersample_triangle_lin_none_dx0.00dy0.00_c1.00",
    "s32_psupersample_gauss0.50_lin_none_dx0.00dy0.00_c1.00",
    "s32_psupersample_gauss1.00_lin_none_dx0.00dy0.00_c1.00",
    // blend space, contrast, phase
    "s128_pexact-clip_box_srgb_none_dx0.00dy0.00_c1.00",
    "s128_pexact-clip_box_lin_none_dx0.00dy0.00_c0.50",
    "s128_pexact-clip_box_lin_none_dx0.33dy0.50_c1.00",
];

/// Cells the cheap scope keeps: one small size per axis.
const TEST_SCOPE_CELLS: &[&str] = &[
    "s32_pexact-clip_box_lin_none_dx0.00dy0.00_c1.00",
    "s32_ptiny-skia_box_lin_none_dx0.00dy0.00_c1.00",
];

/// Alpha levels at which the §1.6 probe multiplies a corpus render.
pub const SEMI_TRANSPARENT_PROBE_ALPHAS: &[f64] = &[0.35, 0.5, 0.75];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CorridorScope {
    Full,
    Test,
}

impl CorridorScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            CorridorScope::Full => "full",
            CorridorScope::Test => "test",
        }
    }
    fn stride(&self) -> usize {
        match self {
            CorridorScope::Full => 1,
            CorridorScope::Test => 12,
        }
    }
    fn cells(&self) -> &'static [&'static str] {
        match self {
            CorridorScope::Full => CORRIDOR_CELL_IDS,
            CorridorScope::Test => TEST_SCOPE_CELLS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CorridorConfigRecord {
    pub schema: &'static str,
    pub scope: &'static str,
    pub cells: Vec<String>,
    pub coverage_levels: Vec<f64>,
    pub sample_steps_px: Vec<f64>,
    pub splits_measured: Vec<&'static str>,
    pub semi_transparent_probe_alphas: Vec<f64>,
    pub analysis: vice_evidence::analysis::AnalysisConfig,
}

impl CorridorConfigRecord {
    pub fn v1(scope: CorridorScope) -> CorridorConfigRecord {
        CorridorConfigRecord {
            schema: CORRIDOR_CONFIG_SCHEMA,
            scope: scope.as_str(),
            cells: scope.cells().iter().map(|s| (*s).to_string()).collect(),
            coverage_levels: COVERAGE_LEVELS.to_vec(),
            sample_steps_px: SAMPLE_STEPS_PX.to_vec(),
            splits_measured: vec![Split::Development.as_str(), Split::Calibration.as_str()],
            semi_transparent_probe_alphas: SEMI_TRANSPARENT_PROBE_ALPHAS.to_vec(),
            analysis: ANALYSIS_CONFIG_V1,
        }
    }
    pub fn hash(&self) -> String {
        sha256_hex(
            serde_json::to_string(self)
                .expect("corridor config serializes")
                .as_bytes(),
        )
    }
}

/// Sample steps for the invariance check of §13.1.
pub const SAMPLE_STEPS_PX: &[f64] = &[0.25, 0.5, 1.0];

/// One boundary sample, scored against the truth.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScoredSample {
    /// Distance from the sample to the nearest true boundary, in px. The
    /// CONSERVATIVE quantity: it is never smaller than the component along
    /// the normal, so the coverage test cannot be flattered by measuring
    /// the displacement in an easier direction.
    pub distance_px: f64,
    /// Signed displacement along the sample's own normal: positive means the
    /// extracted boundary sits on the FOREGROUND side of the truth.
    pub bias_px: f64,
    pub weight_ds: f64,
    pub halfwidth_px: [f64; 4],
    pub capped: [bool; 4],
    pub confidence: f64,
    pub corr_length_px: f64,
}

/// What one (scene, cell) arm produced.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ArmRow {
    pub scene_id: String,
    pub group_id: String,
    pub cell_id: String,
    pub split: &'static str,
    pub profile: &'static str,
    pub size_px: u32,
    pub outcome: String,
    /// GT exterior model of the scene, and what the evidence recovered.
    pub exterior_truth: &'static str,
    pub exterior_recovered: Option<&'static str>,
    pub blend_truth: &'static str,
    pub blend_recovered: Option<&'static str>,
    pub blend_identifiable: Option<bool>,
    pub filter_truth: String,
    pub filter_recovered: Option<String>,
    /// False when the shape is thinner than the kernel: the transition-width
    /// statistic then measures the shape, every filter ties, and counting
    /// the tie-break as a recovery would be counting a coin flip.
    pub filter_identifiable: Option<bool>,
    pub samples: u64,
    pub coverage_at_95: Option<f64>,
    pub median_halfwidth_px: Option<f64>,
    pub p95_distance_px: Option<f64>,
    pub bias_px: Option<f64>,
    /// Largest `|α̂ − true coverage|`, on cells whose observing engine is the
    /// exact integrator (where the true coverage is an exact area).
    pub max_alpha_error: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RefusedArm {
    pub scene_id: String,
    pub cell_id: String,
    pub reason: String,
}

/// Result of the §1.6 probe on one arm.
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

/// Everything one run measured.
#[derive(Debug, Clone)]
pub struct CorridorRun {
    pub config: CorridorConfigRecord,
    pub config_hash: String,
    pub fixture_set_hash: String,
    pub scenes: u64,
    pub arms: Vec<ArmRow>,
    pub refused: Vec<RefusedArm>,
    pub samples: Vec<(ArmKey, ScoredSample)>,
    pub step_invariance: Vec<(f64, f64)>,
    pub probes: Vec<SemiTransparentProbe>,
    pub sealed_audit_groups_skipped: u64,
}

/// The bucket coordinates of one sample, for the conditional calibration of
/// §13.1.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ArmKey {
    pub split: &'static str,
    pub profile: &'static str,
    pub size_px: u32,
    pub psf: String,
    pub blend: &'static str,
    pub contrast_milli: u32,
    pub phase: String,
    pub held_out: bool,
}

fn blend_name(b: BlendSpace) -> &'static str {
    match b {
        BlendSpace::LinearLight => "linear_light",
        BlendSpace::EncodedSrgb => "encoded_srgb",
    }
}

fn exterior_name(e: ExteriorModel) -> &'static str {
    match e {
        ExteriorModel::Transparent => "transparent",
        ExteriorModel::Opaque => "opaque",
    }
}

fn psf_id(p: Psf) -> String {
    match p {
        Psf::Gaussian { sigma_px } => format!("gauss{sigma_px:.2}"),
        other => other.as_str().to_string(),
    }
}

/// The pixel filter a cell's PSF corresponds to in the M4 formation family.
fn truth_filter(p: Psf) -> PixelFilter {
    match p {
        Psf::Box => PixelFilter::Box,
        Psf::Triangle => PixelFilter::Triangle,
        Psf::Gaussian { sigma_px } => PixelFilter::Gaussian { sigma_px },
    }
}

fn filter_id(f: PixelFilter) -> String {
    match f {
        PixelFilter::Box => "box".to_string(),
        PixelFilter::Triangle => "triangle".to_string(),
        PixelFilter::Gaussian { sigma_px } => format!("gauss{sigma_px:.2}"),
    }
}

// ---------------------------------------------------------------------------
// Ground-truth geometry
// ---------------------------------------------------------------------------

/// The true visible interface of a scene, in RENDER space.
pub fn gt_segments(scene: &GtScene, cell: &DegradationCell) -> Vec<(Pt, Pt)> {
    let scale = f64::from(cell.size_px) / f64::from(AUTHORING_CANVAS_PX);
    let tx = |p: Pt| {
        Pt::new(
            p.x * scale + cell.subpixel_dx,
            p.y * scale + cell.subpixel_dy,
        )
    };
    let graph = scene.scene().graph();
    let mesh = scene.certified().mesh();
    let exterior = graph.exterior;
    let opaque = scene.scene().scene().formation.exterior == ExteriorModel::Opaque;
    let mut out = Vec::new();
    for (bid, b) in graph.boundaries.iter().enumerate() {
        if opaque && (b.left_face == exterior || b.right_face == exterior) {
            continue;
        }
        let poly = &mesh.boundary_polylines[bid];
        for w in poly.points.windows(2) {
            out.push((tx(w[0]), tx(w[1])));
        }
    }
    out
}

/// Squared distance from a point to a segment, and the closest point.
fn point_segment(p: Pt, a: Pt, b: Pt) -> (f64, Pt) {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 <= f64::MIN_POSITIVE {
        0.0
    } else {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0)
    };
    let q = Pt::new(a.x + t * dx, a.y + t * dy);
    ((p.x - q.x).powi(2) + (p.y - q.y).powi(2), q)
}

/// A uniform grid over the truth segments, so the nearest-segment query does
/// not become quadratic on a 128 px render with a few hundred segments.
pub struct SegmentIndex {
    segments: Vec<(Pt, Pt)>,
    cell_px: f64,
    origin: (f64, f64),
    dims: (i64, i64),
    buckets: Vec<Vec<u32>>,
}

impl SegmentIndex {
    pub fn new(segments: Vec<(Pt, Pt)>) -> SegmentIndex {
        let cell_px = 4.0;
        let (mut lo_x, mut lo_y, mut hi_x, mut hi_y) = (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        );
        for (a, b) in &segments {
            lo_x = lo_x.min(a.x).min(b.x);
            lo_y = lo_y.min(a.y).min(b.y);
            hi_x = hi_x.max(a.x).max(b.x);
            hi_y = hi_y.max(a.y).max(b.y);
        }
        if !lo_x.is_finite() {
            return SegmentIndex {
                segments,
                cell_px,
                origin: (0.0, 0.0),
                dims: (0, 0),
                buckets: Vec::new(),
            };
        }
        let nx = (((hi_x - lo_x) / cell_px).ceil() as i64 + 1).max(1);
        let ny = (((hi_y - lo_y) / cell_px).ceil() as i64 + 1).max(1);
        let mut buckets = vec![Vec::new(); (nx * ny) as usize];
        for (k, (a, b)) in segments.iter().enumerate() {
            let x0 = (((a.x.min(b.x) - lo_x) / cell_px).floor() as i64).clamp(0, nx - 1);
            let x1 = (((a.x.max(b.x) - lo_x) / cell_px).floor() as i64).clamp(0, nx - 1);
            let y0 = (((a.y.min(b.y) - lo_y) / cell_px).floor() as i64).clamp(0, ny - 1);
            let y1 = (((a.y.max(b.y) - lo_y) / cell_px).floor() as i64).clamp(0, ny - 1);
            for y in y0..=y1 {
                for x in x0..=x1 {
                    buckets[(y * nx + x) as usize].push(k as u32);
                }
            }
        }
        SegmentIndex {
            segments,
            cell_px,
            origin: (lo_x, lo_y),
            dims: (nx, ny),
            buckets,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Nearest point on the truth, and its distance.
    pub fn nearest(&self, p: Pt) -> Option<(f64, Pt)> {
        if self.segments.is_empty() {
            return None;
        }
        let (nx, ny) = self.dims;
        let cx = (((p.x - self.origin.0) / self.cell_px).floor() as i64).clamp(-1, nx);
        let cy = (((p.y - self.origin.1) / self.cell_px).floor() as i64).clamp(-1, ny);
        let mut best = (f64::INFINITY, p);
        let mut ring = 0i64;
        loop {
            let mut touched = false;
            for y in (cy - ring)..=(cy + ring) {
                for x in (cx - ring)..=(cx + ring) {
                    // Only the new ring.
                    if ring > 0 && (x - cx).abs() < ring && (y - cy).abs() < ring {
                        continue;
                    }
                    if x < 0 || y < 0 || x >= nx || y >= ny {
                        continue;
                    }
                    touched = true;
                    for k in &self.buckets[(y * nx + x) as usize] {
                        let (a, b) = self.segments[*k as usize];
                        let (d2, q) = point_segment(p, a, b);
                        if d2 < best.0 {
                            best = (d2, q);
                        }
                    }
                }
            }
            // Everything outside the searched rings is at least
            // `ring * cell` away, so stop once the best is closer.
            let guaranteed = (ring as f64) * self.cell_px;
            if best.0.is_finite() && best.0.sqrt() <= guaranteed {
                break;
            }
            ring += 1;
            if ring > nx.max(ny) + 1 {
                if !touched && best.0.is_infinite() {
                    return None;
                }
                break;
            }
        }
        if best.0.is_infinite() {
            None
        } else {
            Some((best.0.sqrt(), best.1))
        }
    }
}

fn resolve_cells(ids: &[&str]) -> Result<Vec<DegradationCell>, String> {
    let matrix = matrix_v1();
    ids.iter()
        .map(|want| {
            matrix
                .iter()
                .find(|c| c.id() == *want)
                .copied()
                .ok_or_else(|| format!("corridor cell {want} is not a cell of the frozen matrix"))
        })
        .collect()
}

/// Run the corridor calibration.
pub fn run(scope: CorridorScope) -> Result<CorridorRun, String> {
    let config = CorridorConfigRecord::v1(scope);
    let config_hash = config.hash();
    let cells = resolve_cells(scope.cells())?;
    let groups = all_groups()?;
    let policy = &SPLIT_POLICY_V1;

    let mut arms = Vec::new();
    let mut refused = Vec::new();
    let mut samples = Vec::new();
    let mut probes = Vec::new();
    let mut digests = Vec::new();
    let mut scenes = 0u64;
    let mut skipped_audit = 0u64;
    let mut step_acc: Vec<(f64, f64, f64)> =
        SAMPLE_STEPS_PX.iter().map(|s| (*s, 0.0, 0.0)).collect();

    for group in groups.iter().step_by(scope.stride()) {
        let split = policy.split_of_group(group);
        if split == Split::SealedAudit {
            // §27.1: scoring the sealed audit is what OPENS it. A
            // calibration run is not a release candidate.
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
                match measure_arm(scene, cell, split, members, &mut step_acc) {
                    Ok((row, mut s, probe)) => {
                        arms.push(row);
                        samples.append(&mut s);
                        probes.extend(probe);
                    }
                    Err(reason) => refused.push(RefusedArm {
                        scene_id: scene.id().to_string(),
                        cell_id: cell.id(),
                        reason,
                    }),
                }
            }
        }
    }

    digests.sort();
    Ok(CorridorRun {
        config,
        config_hash,
        fixture_set_hash: sha256_hex(digests.join("\u{1f}").as_bytes()),
        scenes,
        arms,
        refused,
        samples,
        step_invariance: step_acc
            .into_iter()
            .map(|(s, inside, total)| (s, if total > 0.0 { inside / total } else { 0.0 }))
            .collect(),
        probes,
        sealed_audit_groups_skipped: skipped_audit,
    })
}

fn outcome_name(o: &Flat2Outcome) -> String {
    match o {
        Flat2Outcome::Supported { .. } => "supported".to_string(),
        Flat2Outcome::Ambiguous { .. } => "ambiguous".to_string(),
        Flat2Outcome::Unsupported(r) => match r {
            UnsupportedReason::SemiTransparentInterior { .. } => {
                "unsupported/semi_transparent_interior".to_string()
            }
            UnsupportedReason::Palette { .. } => "unsupported/palette".to_string(),
            UnsupportedReason::NoHypothesisExplains { .. } => {
                "unsupported/no_hypothesis_explains".to_string()
            }
            UnsupportedReason::NoWellConditionedPair { .. } => {
                "unsupported/no_well_conditioned_pair".to_string()
            }
        },
    }
}

type ArmMeasurement = (
    ArmRow,
    Vec<(ArmKey, ScoredSample)>,
    Vec<SemiTransparentProbe>,
);

fn measure_arm(
    scene: &GtScene,
    cell: &DegradationCell,
    split: Split,
    members: usize,
    step_acc: &mut [(f64, f64, f64)],
) -> Result<ArmMeasurement, String> {
    let fixture = render_cell(scene, cell, members)?;
    let img = CanonicalImage::from_straight_srgb8(
        fixture.width_px,
        fixture.height_px,
        fixture.rgba8.clone(),
        true,
        IccAssumption::NoProfileAssumedSrgb,
    )
    .map_err(|e| e.to_string())?;

    let out = analyze_full(&img, &ANALYSIS_CONFIG_V1, None);
    let truth_exterior = scene.scene().scene().formation.exterior;
    let key = ArmKey {
        split: split.as_str(),
        profile: cell.profile.as_str(),
        size_px: cell.size_px,
        psf: psf_id(cell.psf),
        blend: blend_name(cell.blend),
        contrast_milli: (cell.contrast * 1000.0).round() as u32,
        phase: format!("dx{:.2}dy{:.2}", cell.subpixel_dx, cell.subpixel_dy),
        held_out: SPLIT_POLICY_V1
            .held_out_profiles
            .contains(&cell.profile.as_str()),
    };

    let mut row = ArmRow {
        scene_id: scene.id().to_string(),
        group_id: scene.group_id().to_string(),
        cell_id: cell.id(),
        split: split.as_str(),
        profile: cell.profile.as_str(),
        size_px: cell.size_px,
        outcome: outcome_name(&out.report.outcome),
        exterior_truth: exterior_name(truth_exterior),
        exterior_recovered: None,
        blend_truth: blend_name(cell.blend),
        blend_recovered: None,
        blend_identifiable: None,
        filter_truth: filter_id(truth_filter(cell.psf)),
        filter_recovered: None,
        filter_identifiable: None,
        samples: 0,
        coverage_at_95: None,
        median_halfwidth_px: None,
        p95_distance_px: None,
        bias_px: None,
        max_alpha_error: None,
    };

    // The §1.6 probe: the SAME render with its alpha scaled, i.e. an
    // authored layer of constant alpha over the same geometry.
    // A probe is OBSERVABLE when the shape it scales has an interior thick
    // enough for the plateau to be visible. The criterion is the same
    // thickness the detector uses, applied to the arm's own ink: a shape
    // that is everywhere thinner than the kernel has no plateau to scale,
    // and a thinner opaque shape explains the scaled bytes.
    let observable = out
        .chosen
        .as_ref()
        .map(|ev| {
            vice_evidence::formation::resolved_fraction(ev.alpha_field())
                >= vice_evidence::formation::MIN_RESOLVED_FRACTION
        })
        .unwrap_or(false);
    let mut probes = Vec::new();
    if truth_exterior == ExteriorModel::Transparent {
        for a in SEMI_TRANSPARENT_PROBE_ALPHAS {
            let mut bytes = fixture.rgba8.clone();
            for px in bytes.chunks_mut(4) {
                px[3] = (f64::from(px[3]) * a).round() as u8;
            }
            let probe_img = CanonicalImage::from_straight_srgb8(
                fixture.width_px,
                fixture.height_px,
                bytes,
                true,
                IccAssumption::NoProfileAssumedSrgb,
            )
            .map_err(|e| e.to_string())?;
            let p = vice_evidence::analysis::analyze(&probe_img, &ANALYSIS_CONFIG_V1, None);
            let (rejected, region) = match &p.outcome {
                Flat2Outcome::Unsupported(UnsupportedReason::SemiTransparentInterior {
                    detail,
                    ..
                }) => (true, Some(detail.largest_region_px)),
                _ => (false, None),
            };
            probes.push(SemiTransparentProbe {
                scene_id: scene.id().to_string(),
                cell_id: cell.id(),
                alpha: *a,
                outcome: outcome_name(&p.outcome),
                rejected_as_semi_transparent: rejected,
                largest_region_px: region,
                observable,
            });
        }
    }

    let Some(ev) = out.chosen else {
        return Ok((row, Vec::new(), probes));
    };
    row.exterior_recovered = Some(exterior_name(ev.formation.exterior));
    row.blend_recovered = Some(blend_name(ev.formation.blend_space));
    row.blend_identifiable = Some(matches!(
        ev.hypothesis.background,
        BackgroundHypothesis::OpaqueFace(_)
    ));
    row.filter_recovered = Some(filter_id(ev.formation.pixel_filter));
    row.filter_identifiable = out.report.chosen().map(|c| c.filter_identifiable);

    // Where the observing engine is the exact integrator, the true coverage
    // is an exact area and the mixture's alpha can be compared with it
    // directly — the strongest form of "the transparent exterior is
    // handled correctly" (§28 M4).
    if cell.profile == RasterProfile::ExactClip {
        let scale = f64::from(cell.size_px) / f64::from(AUTHORING_CANVAS_PX);
        let t = crate::gt::raster::ViewTransform {
            scale,
            dx: cell.subpixel_dx,
            dy: cell.subpixel_dy,
            width_px: cell.size_px,
            height_px: cell.size_px,
        };
        if let Ok(stack) = crate::gt::raster::rasterize(
            scene.certified(),
            &t,
            RasterProfile::ExactClip,
            crate::gt::raster::Psf::Box,
        ) {
            // INK is the coverage of the OPAQUE faces. A hole face is not
            // the exterior face and is not ink either: its paint is
            // `TransparentExterior` by IR contract, and counting it made the
            // first run report a full unit of alpha error on every scene
            // with a hole.
            let opaque: Vec<usize> = scene
                .scene()
                .graph()
                .faces
                .iter()
                .enumerate()
                .filter(|(_, f)| matches!(f.paint, vice_ir::Paint::OpaqueSolid(_)))
                .map(|(fi, _)| fi)
                .collect();
            let ink: Vec<f64> = (0..ev.len())
                .map(|i| {
                    opaque
                        .iter()
                        .map(|fi| stack.per_face[*fi][i])
                        .sum::<f64>()
                        .clamp(0.0, 1.0)
                })
                .collect();
            // Only meaningful for the transparent-exterior scenes, where
            // "ink coverage" and "the mixture's alpha" are the same thing.
            if truth_exterior == ExteriorModel::Transparent {
                row.max_alpha_error = Some(
                    (0..ev.len())
                        .map(|i| (ev.alpha(i) - ink[i]).abs())
                        .fold(0.0, f64::max),
                );
            }
        }
    }

    let index = SegmentIndex::new(gt_segments(scene, cell));
    if index.is_empty() {
        return Ok((row, Vec::new(), probes));
    }

    // The sample-step invariance of §13.1: the same arm at three steps.
    for (step, inside, total) in step_acc.iter_mut() {
        if let Ok(o) = observe_boundaries(
            &ev,
            0.95,
            &BoundaryConfig {
                sample_step_px: *step,
                ..BOUNDARY_CONFIG_V1
            },
            &CORRIDOR_CONFIG_V1,
        ) {
            for chain in &o.chains {
                for s in &chain.samples {
                    if let Some((d, _)) = index.nearest(s.p) {
                        *total += s.weight_ds;
                        if d <= s.halfwidth {
                            *inside += s.weight_ds;
                        }
                    }
                }
            }
        }
    }

    // The reported samples, at every coverage level.
    let mut per_level = Vec::new();
    for level in COVERAGE_LEVELS {
        per_level.push(observe_boundaries(
            &ev,
            *level,
            &BOUNDARY_CONFIG_V1,
            &CORRIDOR_CONFIG_V1,
        ));
    }
    let base = match &per_level[2] {
        Ok(o) => o.clone(),
        Err(e) => return Err(e.to_string()),
    };
    let flat = |o: &vice_evidence::boundary::BoundaryObservation| -> Vec<(f64, bool)> {
        o.chains
            .iter()
            .flat_map(|c| c.samples.iter())
            .map(|s| {
                (
                    s.halfwidth,
                    s.halfwidth >= CORRIDOR_CONFIG_V1.max_halfwidth_px,
                )
            })
            .collect()
    };
    let levels: Vec<Vec<(f64, bool)>> = per_level
        .iter()
        .map(|o| match o {
            Ok(x) => flat(x),
            Err(_) => Vec::new(),
        })
        .collect();

    let mut out_samples = Vec::new();
    let mut idx = 0usize;
    let mut ds_total = 0.0;
    let mut inside95 = 0.0;
    let mut bias_sum = 0.0;
    let mut dists: Vec<f64> = Vec::new();
    let mut hws: Vec<f64> = Vec::new();
    for chain in &base.chains {
        for s in &chain.samples {
            let Some((d, q)) = index.nearest(s.p) else {
                idx += 1;
                continue;
            };
            let bias = -((q.x - s.p.x) * s.normal.x + (q.y - s.p.y) * s.normal.y);
            let mut halfwidth_px = [0.0f64; 4];
            let mut capped = [false; 4];
            for (li, level) in levels.iter().enumerate() {
                if let Some((hw, c)) = level.get(idx) {
                    halfwidth_px[li] = *hw;
                    capped[li] = *c;
                }
            }
            ds_total += s.weight_ds;
            if d <= halfwidth_px[2] {
                inside95 += s.weight_ds;
            }
            bias_sum += bias * s.weight_ds;
            dists.push(d);
            hws.push(halfwidth_px[2]);
            out_samples.push((
                key.clone(),
                ScoredSample {
                    distance_px: d,
                    bias_px: bias,
                    weight_ds: s.weight_ds,
                    halfwidth_px,
                    capped,
                    confidence: s.confidence,
                    corr_length_px: s.corr_length_px,
                },
            ));
            idx += 1;
        }
    }
    dists.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    hws.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let pick = |v: &[f64], q: f64| -> Option<f64> {
        if v.is_empty() {
            None
        } else {
            Some(v[((v.len() as f64 * q) as usize).min(v.len() - 1)])
        }
    };
    row.samples = out_samples.len() as u64;
    if ds_total > 0.0 {
        row.coverage_at_95 = Some(inside95 / ds_total);
        row.bias_px = Some(bias_sum / ds_total);
    }
    row.median_halfwidth_px = pick(&hws, 0.5);
    row.p95_distance_px = pick(&dists, 0.95);
    Ok((row, out_samples, probes))
}
