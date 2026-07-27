//! O0 / G30 — the renderer + serialization ceiling (spec §27.6, §28 M3.5).
//!
//! §27.6 defines `G30` as "GT parameters / no optimizer (renderer /
//! serialization ceiling)": the residual that remains when the answer is
//! PERFECT. Nothing downstream can do better than this, so every later
//! ceiling in the ladder is measured against it, and a milestone that
//! reports a boundary error smaller than G30 is reporting a bug.
//!
//! The measurement, exactly:
//!
//! ```text
//! observation := the corpus render of scene S under degradation cell C
//! model       := S -> canonical bytes -> parse -> validate -> certify
//!                  -> rendered under C with the reference backend's
//!                     rasterizer, same formation, same view transform
//! ceiling     := |model - observation| in 8-bit code units
//! ```
//!
//! Two components are separable and both are reported:
//!
//! - **serialization.** The canonical round trip is byte-exact when the
//!   scene digest survives it, and the harness asserts the digest rather
//!   than assuming it. At this milestone the component measures ZERO, which
//!   is a measurement and not a claim: it is recorded per arm so that the
//!   day a quantization step lands (§20.2, M7) the number moves.
//! - **renderer.** Everything else: the backend rasterizer and the engine
//!   that produced the observation disagree about antialiasing, and after
//!   8-bit quantization that disagreement is the floor.
//!
//! Which is why the pairing matters, and why [`super::crime`] exists: run
//! the backend against an observation from the SAME implementation and the
//! residual collapses to the serialization component, measuring shared
//! assumptions rather than accuracy. The harness runs that pairing on
//! purpose — it is the cheapest way to isolate serialization — and flags
//! every arm of it, because a number that looks like a ceiling and is not
//! one is worse than no number (F-0009).

use serde::Serialize;

use super::crime::InverseCrime;
use super::design::FormationSource;
use super::key::{CandidateBudget, CompatibilityKey, KeyedMeasurement};
use crate::gt::degradation::{render_cell_raster, DegradationCell, ResizeChain};
use crate::gt::raster::RasterProfile;
use crate::gt::GtScene;
use crate::hashing::sha256_hex;
use vice_ir::{Paint, ValidatedScene};
use vice_render::{CertifiedMesh, RenderOptions};

/// The serialization stage a backend puts the injected scene through.
///
/// One variant, because one exists: the canonical byte round trip of M1.
/// The parameter quantization of §20.2 is M7's, and naming it here as an
/// unimplemented variant would be the placeholder §32 rule 7 forbids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Serialization {
    CanonicalRoundtrip,
}

impl Serialization {
    pub fn id(&self) -> &'static str {
        match self {
            Serialization::CanonicalRoundtrip => "canonical-roundtrip",
        }
    }
}

/// The reference backend of an arm: what turns an injected scene into
/// bytes that can be compared with an observation.
///
/// Deliberately NOT `Serialize`: what a report records is [`OracleBackend::id`],
/// the string that is a compatibility-key component, so there is one spelling
/// of a backend in every artifact rather than two.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OracleBackend {
    pub serialization: Serialization,
    pub rasterizer: RasterProfile,
}

impl OracleBackend {
    pub fn new(rasterizer: RasterProfile) -> OracleBackend {
        OracleBackend {
            serialization: Serialization::CanonicalRoundtrip,
            rasterizer,
        }
    }

    /// `backend_id` of the compatibility key (§27.6).
    pub fn id(&self) -> String {
        format!("{}+{}", self.serialization.id(), self.rasterizer.as_str())
    }
}

/// Why one arm produced no number. Every variant is a stated limit of THIS
/// harness, not a missing milestone capability.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ArmRefusal {
    #[error(
        "backend {backend} cannot realize the formation of cell {cell}: its rasterizer \
         integrates a unit box only, and pretending otherwise would mislabel the injected \
         formation as ground truth"
    )]
    BackendCannotRealizeFormation { backend: String, cell: String },
    #[error(
        "cell {cell} carries a resize chain, so the coverage stack is at work resolution and \
         the edge mask does not align with the output bytes; an edge metric over a resampled \
         mask would be a different quantity wearing the same name"
    )]
    ResizeChainNotSupported { cell: String },
    #[error("the canonical round trip of scene {scene} failed: {detail}")]
    SerializationFailed { scene: String, detail: String },
    #[error("rendering scene {scene} under cell {cell} failed: {detail}")]
    RenderFailed {
        scene: String,
        cell: String,
        detail: String,
    },
    #[error("scene {scene}: observation is {obs} px and model is {model} px")]
    SizeMismatch { scene: String, obs: u32, model: u32 },
    #[error(
        "the formation estimator did not deliver a formation for scene {scene} under cell \
         {cell}: it returned {outcome}. The arm is refused rather than filled with the ground \
         truth, which would make PF10 a copy of PF11"
    )]
    FormationEstimationFailed {
        scene: String,
        cell: String,
        outcome: String,
    },
    #[error(
        "the ESTIMATED formation for scene {scene} is {estimated}, and backend {backend} \
         integrates a unit box only. Refused rather than substituted: an estimate the backend \
         cannot realize is an estimation error, and hiding it behind the ground truth would \
         make the arm measure something else"
    )]
    EstimatedFormationNotRealizable {
        scene: String,
        backend: String,
        estimated: String,
    },
}

/// A scene put through the backend's serialization stage, once.
///
/// Held separately from the arm because the round trip is per SCENE while
/// arms are per (scene, cell, backend): re-parsing the same bytes for every
/// arm would measure the same zero repeatedly and cost the corpus build
/// again.
#[derive(Debug, Clone)]
pub struct RoundtrippedScene {
    pub scene_id: String,
    pub group_id: String,
    pub scene_digest: String,
    /// True when the canonical bytes round-tripped to the SAME digest, i.e.
    /// the serialization component of the ceiling is exactly zero.
    pub digest_identical: bool,
    certified: CertifiedMesh,
    paints: Vec<Paint>,
}

impl RoundtrippedScene {
    pub fn of(scene: &GtScene) -> Result<RoundtrippedScene, ArmRefusal> {
        let fail = |detail: String| ArmRefusal::SerializationFailed {
            scene: scene.id().to_string(),
            detail,
        };
        let original = scene.scene().scene();
        let digest = vice_ir::scene_digest_sha256(original).map_err(|e| fail(e.to_string()))?;
        let bytes = vice_ir::canonical_scene_bytes(original).map_err(|e| fail(e.to_string()))?;
        let parsed = vice_ir::parse_scene(&bytes).map_err(|e| fail(e.to_string()))?;
        let revalidated = ValidatedScene::new(parsed).map_err(|e| fail(e.to_string()))?;
        let after =
            vice_ir::scene_digest_sha256(revalidated.scene()).map_err(|e| fail(e.to_string()))?;
        let certified = CertifiedMesh::from_scene(&revalidated, RenderOptions::default())
            .map_err(|e| fail(e.to_string()))?;
        let paints = revalidated.graph().faces.iter().map(|f| f.paint).collect();
        Ok(RoundtrippedScene {
            scene_id: scene.id().to_string(),
            group_id: scene.group_id().to_string(),
            digest_identical: after == digest,
            scene_digest: digest,
            certified,
            paints,
        })
    }
}

/// What one arm measured, in 8-bit code units (0..255).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CeilingMetrics {
    /// Gross-error detector: the largest channel disagreement anywhere.
    pub max_abs_code: f64,
    /// The GATE metric, scale-invariant: mean |Δ| over pixels the model
    /// covers only partially. Antialiasing disagreement lives exactly
    /// there, and the denominator does not grow with empty canvas
    /// (REVIEW_M2_A M2-A-N1 measured what happens when it does).
    pub edge_mean_abs_code: f64,
    /// DIAGNOSTIC ONLY, never a gate: the same mean over the whole frame,
    /// which moves with canvas size rather than with accuracy.
    pub frame_mean_abs_code: f64,
    pub edge_pixels: u64,
    pub total_pixels: u64,
    /// Fraction of pixels identical in all four channels.
    pub identical_pixels_frac: f64,
    /// The serialization component: `true` means it contributes exactly
    /// zero and the rest of the ceiling is the renderer's.
    pub serialization_digest_identical: bool,
}

/// One measured arm of the ceiling.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CeilingArm {
    pub arm: &'static str,
    /// Where the formation this arm rendered with came from. The factor of
    /// the 2x2 that M4 made measurable.
    pub formation_source: FormationSource,
    /// The formation the arm actually rendered with, as an id.
    pub formation: String,
    /// True when the estimated formation equals the cell's own. Recorded per
    /// arm so "the estimator recovered the truth" is a count and not a
    /// claim.
    pub formation_matches_gt: bool,
    pub scene_id: String,
    pub group_id: String,
    pub cell_id: String,
    pub observation_profile: &'static str,
    pub backend_id: String,
    /// The backend's rasterizer, separately from the composite id, so a
    /// reader of the artifact can RE-DERIVE `inverse_crime` from the two
    /// profiles instead of trusting the recorded flag (REVIEW_M3 M3-N5).
    pub backend_rasterizer: &'static str,
    /// The compatibility key this arm was measured under.
    ///
    /// Carried in full, not only as a fingerprint, because a factorial arm
    /// is DERIVED from measurements and the derivation needs the components
    /// (condition B1 / REVIEW_M3_5 M35-N3). The fingerprint stays for a
    /// reader who wants one value to compare.
    pub key: CompatibilityKey,
    pub key_fingerprint: String,
    pub inverse_crime: InverseCrime,
    pub metrics: CeilingMetrics,
}

impl super::key::sealed::Sealed for CeilingArm {}

impl KeyedMeasurement for CeilingArm {
    fn measurement_key(&self) -> &CompatibilityKey {
        &self.key
    }
    fn measurement_value(&self, metric: &str) -> Option<f64> {
        self.metrics.get(metric)
    }
    fn measurement_crime(&self) -> &InverseCrime {
        &self.inverse_crime
    }
}

/// The named metrics an arm publishes, so the factorial can be assembled
/// per metric without a string typo silently creating a new column.
pub const CEILING_METRICS: &[&str] = &[
    "max_abs_code",
    "edge_mean_abs_code",
    "identical_pixels_frac",
];

impl CeilingMetrics {
    pub fn get(&self, metric: &str) -> Option<f64> {
        match metric {
            "max_abs_code" => Some(self.max_abs_code),
            "edge_mean_abs_code" => Some(self.edge_mean_abs_code),
            "identical_pixels_frac" => Some(self.identical_pixels_frac),
            _ => None,
        }
    }
}

/// The compatibility key of one arm (§27.6).
pub fn arm_key(
    backend: &OracleBackend,
    config_hash: &str,
    scene_digest: &str,
    cell: &DegradationCell,
) -> CompatibilityKey {
    CompatibilityKey {
        backend_id: backend.id(),
        config_hash: config_hash.to_string(),
        // M4 runs a formation estimator, and it enumerates the §10.1 family
        // EXHAUSTIVELY - nothing truncated, so no search bound is claimed.
        // The budget is a property of the RUN, identical on every arm: PF11
        // simply does not need the search it is allowed. M3.5's arms said
        // `not_applicable` because no search existed at all, so an M3.5 arm
        // and an M4 arm are not commensurable, which is the key doing its
        // job rather than a versioning accident.
        candidate_budget: CandidateBudget::Exhaustive {
            formation_family: vice_evidence::formation::FAMILY_SIZE as u64,
        },
        fixture_hash: sha256_hex(format!("{scene_digest}\u{1f}{}", cell.id()).as_bytes()),
        intervention_schema_version: super::design::INTERVENTION_SCHEMA_VERSION.to_string(),
    }
}

/// Map an estimated formation onto the cell the model renders under.
fn estimated_cell(
    scene: &GtScene,
    cell: &DegradationCell,
    backend: &OracleBackend,
    rgba8: &[u8],
    width_px: u32,
    height_px: u32,
) -> Result<(DegradationCell, String), ArmRefusal> {
    let img = vice_image::CanonicalImage::from_straight_srgb8(
        width_px,
        height_px,
        rgba8.to_vec(),
        true,
        vice_image::IccAssumption::NoProfileAssumedSrgb,
    )
    .map_err(|e| ArmRefusal::RenderFailed {
        scene: scene.id().to_string(),
        cell: cell.id(),
        detail: e.to_string(),
    })?;
    let out = vice_evidence::analysis::analyze_full(
        &img,
        &vice_evidence::analysis::ANALYSIS_CONFIG_V1,
        None,
    );
    let Some(ev) = out.chosen else {
        return Err(ArmRefusal::FormationEstimationFailed {
            scene: scene.id().to_string(),
            cell: cell.id(),
            outcome: serde_json::to_string(&out.report.outcome).unwrap_or_default(),
        });
    };
    let mut model_cell = *cell;
    model_cell.profile = backend.rasterizer;
    model_cell.blend = ev.formation.blend_space;
    model_cell.psf = match ev.formation.pixel_filter {
        vice_ir::PixelFilter::Box => crate::gt::raster::Psf::Box,
        vice_ir::PixelFilter::Triangle => crate::gt::raster::Psf::Triangle,
        vice_ir::PixelFilter::Gaussian { sigma_px } => {
            crate::gt::raster::Psf::Gaussian { sigma_px }
        }
    };
    let id = vice_evidence::formation::formation_id(&ev.formation);
    if !model_cell.is_realizable() {
        return Err(ArmRefusal::EstimatedFormationNotRealizable {
            scene: scene.id().to_string(),
            backend: backend.id(),
            estimated: id,
        });
    }
    Ok((model_cell, id))
}

/// Measure the ceiling of one (scene, cell, backend, formation source) arm.
pub fn measure_ceiling(
    scene: &GtScene,
    round: &RoundtrippedScene,
    cell: &DegradationCell,
    backend: OracleBackend,
    formation_source: FormationSource,
    equivalence_members: usize,
    config_hash: &str,
) -> Result<CeilingArm, ArmRefusal> {
    if cell.resize != ResizeChain::None {
        return Err(ArmRefusal::ResizeChainNotSupported { cell: cell.id() });
    }
    // The GT model runs the SAME cell with the backend's rasterizer swapped
    // in: same size, same phase, same blend space, same contrast, same PSF.
    // That is what "GT formation injected" means here - the true observation
    // model, not a nearby one.
    let mut model_cell = *cell;
    model_cell.profile = backend.rasterizer;
    if !model_cell.is_realizable() {
        return Err(ArmRefusal::BackendCannotRealizeFormation {
            backend: backend.id(),
            cell: cell.id(),
        });
    }

    let observation = crate::gt::degradation::render_cell(scene, cell, equivalence_members)
        .map_err(|detail| ArmRefusal::RenderFailed {
            scene: scene.id().to_string(),
            cell: cell.id(),
            detail,
        })?;

    let gt_formation_id = format!(
        "{}/{}",
        match cell.blend {
            vice_ir::BlendSpace::LinearLight => "lin",
            vice_ir::BlendSpace::EncodedSrgb => "srgb",
        },
        match cell.psf {
            crate::gt::raster::Psf::Gaussian { sigma_px } => format!("gauss{sigma_px:.2}"),
            other => other.as_str().to_string(),
        }
    );
    let (model_cell, formation_id) = match formation_source {
        FormationSource::GroundTruth => (model_cell, gt_formation_id.clone()),
        FormationSource::Estimated => {
            let (c, id) = estimated_cell(
                scene,
                cell,
                &backend,
                &observation.rgba8,
                observation.width_px,
                observation.height_px,
            )?;
            // The estimator's id carries the exterior too; compare only the
            // part the model render can act on.
            let short = id.split('/').take(2).collect::<Vec<_>>().join("/");
            (c, short)
        }
    };
    let model =
        render_cell_raster(&round.certified, &round.paints, &model_cell).map_err(|detail| {
            ArmRefusal::RenderFailed {
                scene: scene.id().to_string(),
                cell: model_cell.id(),
                detail,
            }
        })?;
    if model.width_px != observation.width_px {
        return Err(ArmRefusal::SizeMismatch {
            scene: scene.id().to_string(),
            obs: observation.width_px,
            model: model.width_px,
        });
    }

    let metrics = compare(
        &observation.rgba8,
        &model.rgba8,
        &model.stack,
        round.digest_identical,
    );
    let key = arm_key(&backend, config_hash, &round.scene_digest, cell);
    Ok(CeilingArm {
        arm: match formation_source {
            FormationSource::GroundTruth => "PF11",
            FormationSource::Estimated => "PF10",
        },
        formation_source,
        formation_matches_gt: formation_id == gt_formation_id,
        formation: formation_id,
        scene_id: round.scene_id.clone(),
        group_id: round.group_id.clone(),
        cell_id: cell.id(),
        observation_profile: cell.profile.as_str(),
        backend_id: backend.id(),
        backend_rasterizer: backend.rasterizer.as_str(),
        key_fingerprint: key.fingerprint(),
        key,
        inverse_crime: InverseCrime::of(backend.rasterizer, cell.profile),
        metrics,
    })
}

/// Per-pixel comparison. The edge mask comes from the MODEL's coverage
/// stack, which is the exact area fraction: a pixel is an edge pixel iff a
/// boundary crosses it, independently of how the observing engine chose to
/// antialias it.
fn compare(
    observation: &[u8],
    model: &[u8],
    stack: &crate::gt::raster::CoverageStack,
    serialization_digest_identical: bool,
) -> CeilingMetrics {
    const EPS: f64 = 1e-12;
    let n = observation.len() / 4;
    let mut max_abs = 0.0f64;
    let mut sum_all = 0.0f64;
    let mut sum_edge = 0.0f64;
    let mut edge_pixels = 0u64;
    let mut identical = 0u64;
    for i in 0..n {
        let is_edge = stack
            .per_face
            .iter()
            .any(|cov| cov.get(i).is_some_and(|v| *v > EPS && *v < 1.0 - EPS));
        // PREMULTIPLIED, per §1.6 and §5.2: straight RGB under alpha≈0 is
        // not colour evidence, and taking it as one made a 4%-of-a-pixel
        // coverage disagreement read as 243 code values (F-0021).
        let deltas = crate::gt::colour::premultiplied_deltas(
            &observation[i * 4..i * 4 + 4],
            &model[i * 4..i * 4 + 4],
        );
        let pixel_max = deltas.iter().copied().fold(0.0f64, f64::max);
        let pixel_sum: f64 = deltas.iter().sum();
        if pixel_max == 0.0 {
            identical += 1;
        }
        max_abs = max_abs.max(pixel_max);
        sum_all += pixel_sum / 4.0;
        if is_edge {
            sum_edge += pixel_sum / 4.0;
            edge_pixels += 1;
        }
    }
    CeilingMetrics {
        max_abs_code: max_abs,
        edge_mean_abs_code: if edge_pixels > 0 {
            sum_edge / edge_pixels as f64
        } else {
            0.0
        },
        frame_mean_abs_code: if n > 0 { sum_all / n as f64 } else { 0.0 },
        edge_pixels,
        total_pixels: n as u64,
        identical_pixels_frac: if n > 0 {
            identical as f64 / n as f64
        } else {
            1.0
        },
        serialization_digest_identical,
    }
}

/// The ceiling over many scenes, for one (backend, cell) pairing.
///
/// The only constructor is [`CeilingAggregate::of`], so the contamination
/// fold cannot be skipped and the status cannot be chosen: an aggregate
/// over a contaminated arm is contaminated, by construction rather than by
/// remembering.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CeilingAggregate {
    pub arm: &'static str,
    pub backend_id: String,
    pub cell_id: String,
    pub observation_profile: &'static str,
    pub arms: u64,
    pub inverse_crime: InverseCrime,
    /// Worst single-channel disagreement over the arms, and where.
    pub max_abs_code: f64,
    pub worst_scene_id: String,
    pub edge_mean_abs_code_mean: f64,
    pub edge_mean_abs_code_max: f64,
    pub identical_pixels_frac_min: f64,
    pub all_serialization_identical: bool,
    /// How many arms of this group rendered with the formation the cell
    /// actually used. On a GT arm this is trivially all of them; on an
    /// ESTIMATED arm it is the estimator's recovery rate, per pairing.
    pub formation_matches_gt: u64,
}

impl CeilingAggregate {
    pub fn of(arms: &[&CeilingArm]) -> Option<CeilingAggregate> {
        let first = arms.first()?;
        let mut max_abs = f64::NEG_INFINITY;
        let mut worst_scene = String::new();
        let mut edge_sum = 0.0;
        let mut edge_max = f64::NEG_INFINITY;
        let mut ident_min = f64::INFINITY;
        let mut all_ser = true;
        let crime = InverseCrime::fold_all(arms.iter().map(|a| &a.inverse_crime));
        for a in arms {
            if a.metrics.max_abs_code > max_abs {
                max_abs = a.metrics.max_abs_code;
                worst_scene = a.scene_id.clone();
            }
            edge_sum += a.metrics.edge_mean_abs_code;
            edge_max = edge_max.max(a.metrics.edge_mean_abs_code);
            ident_min = ident_min.min(a.metrics.identical_pixels_frac);
            all_ser &= a.metrics.serialization_digest_identical;
        }
        Some(CeilingAggregate {
            arm: first.arm,
            backend_id: first.backend_id.clone(),
            cell_id: first.cell_id.clone(),
            observation_profile: first.observation_profile,
            arms: arms.len() as u64,
            inverse_crime: crime,
            max_abs_code: max_abs,
            worst_scene_id: worst_scene,
            edge_mean_abs_code_mean: edge_sum / arms.len() as f64,
            edge_mean_abs_code_max: edge_max,
            identical_pixels_frac_min: ident_min,
            all_serialization_identical: all_ser,
            formation_matches_gt: arms.iter().filter(|a| a.formation_matches_gt).count() as u64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gt::raster::CoverageStack;

    fn stack(edge: bool) -> CoverageStack {
        CoverageStack {
            width_px: 1,
            height_px: 1,
            per_face: vec![
                vec![if edge { 0.5 } else { 1.0 }],
                vec![if edge { 0.5 } else { 0.0 }],
            ],
        }
    }

    /// F-0021, pinned. A pixel the model covers 4% and the observation does
    /// not cover at all is a 4%-of-a-pixel disagreement — not a 243-code
    /// one, which is what the straight bytes say because `composite_rgba8`
    /// stores the paint's full RGB next to an alpha of 11 (§1.6: RGB under
    /// alpha≈0 is not colour evidence).
    ///
    /// This was the LARGEST number in the first ceiling run, so the metric
    /// would have published a renderer ceiling of 243 code units that was
    /// almost entirely an artifact of the storage convention.
    #[test]
    fn the_ceiling_metric_is_taken_in_premultiplied_space() {
        let uncovered = [0u8, 0, 0, 0];
        let barely = [243u8, 137, 124, 11];
        let m = compare(&uncovered, &barely, &stack(true), true);
        assert!(
            m.max_abs_code <= 11.0,
            "the ceiling must be bounded by the coverage disagreement, got {}",
            m.max_abs_code
        );
        assert_eq!(m.edge_pixels, 1);

        // Control: where alpha does NOT hide the colour, the metric is
        // still full-range, so the fix did not simply blunt it.
        let red = [255u8, 0, 0, 255];
        let green = [0u8, 255, 0, 255];
        let c = compare(&red, &green, &stack(true), true);
        assert_eq!(c.max_abs_code, 255.0);
        assert_eq!(c.identical_pixels_frac, 0.0);

        // And an identical pair is identical.
        let same = compare(&red, &red, &stack(false), true);
        assert_eq!(same.max_abs_code, 0.0);
        assert_eq!(same.identical_pixels_frac, 1.0);
        assert_eq!(same.edge_pixels, 0, "a fully covered pixel is not an edge");
    }

    /// The refusals are typed and name the limit, not "unsupported".
    #[test]
    fn a_cell_the_harness_cannot_measure_is_refused_by_name() {
        let r = ArmRefusal::ResizeChainNotSupported {
            cell: "s32_x".into(),
        };
        assert!(r.to_string().contains("resize chain"));
        let r = ArmRefusal::BackendCannotRealizeFormation {
            backend: "b".into(),
            cell: "c".into(),
        };
        assert!(r.to_string().contains("unit box"));
    }
}
