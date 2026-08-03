//! M7 held-out measurement court.
//!
//! The production objective never judges itself here. Ground-truth topology
//! comes from the independently built certified fixture, while boundary and
//! paint errors compare the selected canonical scene against that truth.
//! Rows are raw measurements: confidence and tail thresholds are frozen from
//! calibration rows before the sealed-audit split is opened.

pub mod analysis;
pub mod artifact;
pub mod baseline;
pub mod determinism;
pub mod governance;
pub mod oracle;
pub mod release;

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{atomic::AtomicBool, atomic::AtomicU64, Arc};
use std::thread::JoinHandle;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use vice_core::{CoreConfig, Preset, VectorizeRequest};
use vice_geom::Pt;
use vice_ir::color::linear_to_srgb_u8;
use vice_ir::ValidatedScene;
use vice_opt::BoundValue;
use vice_render::{CertifiedMesh, RenderOptions};

use crate::gt::corpus::{
    certify_flat2_supported_group, certify_m7_flat2_group,
    groups_with_variants_filtered_for_generation, is_m7_sealed_flat2_family,
    M7_SUCCESSOR_POPULATION_POLICY, M7_SUCCESSOR_PROCEDURAL_VARIANTS,
};
use crate::gt::degradation::{matrix_v1, render_cell, DegradationCell};
use crate::gt::grammar::{AUTHORING_CANVAS_PX, M7_PROCEDURAL_GENERATION};
use crate::gt::raster::RasterProfile;
use crate::gt::split::{Split, SPLIT_POLICY_V1};
use crate::gt::{FixtureOrigin, GtScene, PartitionTruth};

pub const M7_MEASUREMENT_SCHEMA: &str = "vice-classic/m7-held-out-measurement/v23";
pub const M7_ALL_SPLIT_POPULATION_POLICY: &str = "vice-classic/m7-population/all-split-groups/v1";
pub const M7_CALIBRATION_POPULATION_POLICY: &str =
    "vice-classic/m7-population/calibration-flat2-supported/v2";
pub const M7_SEALED_POPULATION_POLICY: &str = M7_SUCCESSOR_POPULATION_POLICY;
pub const M7_RELEASE_PROCEDURAL_VARIANTS: usize = M7_SUCCESSOR_PROCEDURAL_VARIANTS;
pub const M7_MANDATORY_SIZES: [u32; 3] = [128, 256, 512];
pub const M7_BOUNDARY_P95_GATE_PX: f64 = 0.35;
pub const M7_BOUNDARY_P99_GATE_PX: f64 = 0.60;
pub const M7_BOUNDARY_MAX_GATE_PX: f64 = 1.50;
const BOUNDARY_SAMPLE_STEP_PX: f64 = 0.25;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeasurementScope {
    Smoke,
    CalibrationSmoke,
    Calibration,
    SealedAudit,
}

impl MeasurementScope {
    fn as_str(self) -> &'static str {
        match self {
            Self::Smoke => "smoke",
            Self::CalibrationSmoke => "calibration_smoke",
            Self::Calibration => "calibration",
            Self::SealedAudit => "sealed_audit",
        }
    }

    fn split(self) -> Split {
        match self {
            Self::Smoke => Split::Development,
            Self::CalibrationSmoke | Self::Calibration => Split::Calibration,
            Self::SealedAudit => Split::SealedAudit,
        }
    }

    fn variants(self) -> usize {
        match self {
            Self::Smoke | Self::CalibrationSmoke => 1,
            Self::Calibration | Self::SealedAudit => M7_RELEASE_PROCEDURAL_VARIANTS,
        }
    }

    fn population_policy(self) -> &'static str {
        match self {
            Self::Smoke => M7_ALL_SPLIT_POPULATION_POLICY,
            Self::CalibrationSmoke | Self::Calibration => M7_CALIBRATION_POPULATION_POLICY,
            Self::SealedAudit => M7_SEALED_POPULATION_POLICY,
        }
    }

    fn admits_origin(self, origin: FixtureOrigin) -> bool {
        self != Self::SealedAudit || origin == FixtureOrigin::Procedural
    }

    fn admits_shape_family(self, family: &str) -> bool {
        self != Self::SealedAudit || is_m7_sealed_flat2_family(family)
    }

    fn admits_group(self, group: &crate::gt::GtSourceGroup) -> Result<bool, String> {
        if !self.admits_origin(group.origin) || !self.admits_shape_family(&group.shape_family) {
            return Ok(false);
        }
        match self {
            Self::Smoke => Ok(true),
            Self::CalibrationSmoke | Self::Calibration => {
                Ok(certify_flat2_supported_group(group).is_ok())
            }
            Self::SealedAudit => certify_m7_flat2_group(group).map(|()| true),
        }
    }

    fn cells(self) -> Vec<DegradationCell> {
        matrix_v1()
            .into_iter()
            .filter(|cell| match self {
                Self::Smoke => {
                    cell.size_px == 32 && cell.profile == RasterProfile::ExactClip && is_spine(cell)
                }
                Self::CalibrationSmoke => {
                    M7_MANDATORY_SIZES.contains(&cell.size_px)
                        && cell.profile == RasterProfile::TinySkia
                        && is_spine(cell)
                }
                Self::Calibration | Self::SealedAudit => {
                    M7_MANDATORY_SIZES.contains(&cell.size_px)
                        && cell.profile == RasterProfile::TinySkia
                        && is_spine(cell)
                }
            })
            .collect()
    }
}

fn is_spine(cell: &DegradationCell) -> bool {
    cell.subpixel_dx == 0.0
        && cell.subpixel_dy == 0.0
        && cell.psf == crate::gt::raster::Psf::Box
        && cell.blend == vice_ir::BlendSpace::LinearLight
        && cell.resize == crate::gt::degradation::ResizeChain::None
        && cell.contrast == 1.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoundaryGateCounts {
    pub p95_gate_px: f64,
    pub samples_at_or_below_p95_gate: u64,
    pub p99_gate_px: f64,
    pub samples_at_or_below_p99_gate: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoundaryTail {
    pub samples: u64,
    pub p95_px: f64,
    pub p99_px: f64,
    pub max_px: f64,
    pub gate_counts: BoundaryGateCounts,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopologyComparison {
    pub truth_visible_faces: u32,
    pub selected_visible_faces: u32,
    pub truth_components: u32,
    pub selected_components: u32,
    pub truth_holes: u32,
    pub selected_holes: u32,
    pub truth_exterior: String,
    pub selected_exterior: String,
    pub exact: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneComplexity {
    pub vertices: u64,
    pub boundaries: u64,
    pub curve_segments: u64,
    pub canonical_delivery_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InternalBaselineMeasurement {
    pub hypothesis_id: String,
    pub scene_digest_sha256: String,
    pub delivery_digest_sha256: String,
    pub artifact_bundle_sha256: String,
    pub topology: TopologyComparison,
    pub boundary: BoundaryTail,
    pub max_palette_code_delta: u8,
    pub profile_max_channel_delta: u8,
    pub profile_mean_channel_delta: f64,
    pub internal_to_pure_max_channel_delta: u8,
    pub internal_to_pure_mean_channel_delta: f64,
    pub internal_to_seam_max_channel_delta: u8,
    pub internal_to_seam_mean_channel_delta: f64,
    pub complexity: SceneComplexity,
    pub verifier_clean: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PfArmMeasurement {
    pub arm: String,
    pub partition_source: String,
    pub formation_source: String,
    pub scene_digest_sha256: String,
    pub serialized_svg_sha256: String,
    pub max_premultiplied_code_delta: f64,
    pub mean_premultiplied_code_delta: f64,
    pub identical_pixels_fraction: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PfOracleMeasurement {
    pub intervention_schema: String,
    pub common_backend: String,
    pub common_config_sha256: String,
    pub arms: Vec<PfArmMeasurement>,
    pub refusals: Vec<String>,
    pub complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CostRefusalCount {
    pub family: String,
    pub reason: String,
    pub count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct NumericalConditioningDiagnostics {
    pub evidence_pairs: u64,
    pub evidence_pairs_refused: u64,
    pub evidence_conditioning_min: Option<f64>,
    pub evidence_conditioning_max: Option<f64>,
    pub fit_runs: u64,
    pub fit_material_cost_samples: u64,
    pub fit_worst_normal_to_euclidean_ratio: Option<f64>,
    pub fit_worst_ratio_at_deviation_px: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeasurementRow {
    pub group_id: String,
    pub scene_id: String,
    pub shape_family: String,
    pub cell_id: String,
    pub size_px: u32,
    pub rasterizer: String,
    pub identifiability: String,
    pub core_runtime_ms: u64,
    pub runtime_stages: vice_core::RuntimeStageSummary,
    pub court_runtime_ms: u64,
    pub row_elapsed_ms: u64,
    pub decision_status: String,
    pub decision_reason: Option<String>,
    pub production_provenance: bool,
    pub production_accepted: bool,
    pub candidate_available: bool,
    pub selected_hypothesis_id: Option<String>,
    pub selected_scene_digest_sha256: Option<String>,
    pub selected_delivery_digest_sha256: Option<String>,
    pub selected_artifact_bundle_sha256: Option<String>,
    pub selected_complexity: Option<SceneComplexity>,
    pub internal_baseline: Option<InternalBaselineMeasurement>,
    #[serde(default)]
    pub internal_baseline_refusals: Vec<String>,
    pub pf_oracle: Option<PfOracleMeasurement>,
    pub cost_refusal_histogram: Vec<CostRefusalCount>,
    pub numerical_conditioning: NumericalConditioningDiagnostics,
    pub search_truncated: Option<bool>,
    pub explored_mass: Option<f64>,
    pub topology_classes_upper_bound: Option<u64>,
    pub formation_classes_upper_bound: Option<u64>,
    pub top_topology_explored_mass: Option<f64>,
    pub top_formation_explored_mass: Option<f64>,
    pub selected_delivery_mass: Option<f64>,
    pub retained_normalized_mass: Option<f64>,
    pub delivery_classes: Option<u64>,
    pub top2_class_margin_bits: Option<f64>,
    pub posterior_lower_bound: Option<f64>,
    pub posterior_bound_status: String,
    /// Finite empirical proxy for resource-pruned model mass: every omitted
    /// schedule slot is charged one best-retained-hypothesis mass. This is an
    /// R1 calibration variable, never a certified R2 bound.
    pub unexplored_proxy_hypotheses: Option<u64>,
    pub candidate_bytes: u64,
    pub serialized_pixel_bits: Option<f64>,
    pub serialized_pixel_bits_per_block: Option<f64>,
    pub support_isotopy_displacement_px: Option<f64>,
    /// Production-observable distance between the selected serialized scene
    /// palette and independent Flat2 palette evidence.
    pub evidence_palette_shift_codes: Option<u8>,
    pub palette_support_px: Option<u64>,
    pub palette_interval_radius_codes: Option<u8>,
    pub paint_calibration_class: Option<String>,
    pub empirical_correlation_length_px: Option<f64>,
    pub max_abs_lag1: Option<f64>,
    pub topology_entropy_upper_bound: Option<f64>,
    pub topology_entropy_bound_status: String,
    pub formation_entropy_upper_bound: Option<f64>,
    pub formation_entropy_bound_status: String,
    pub perturbation_stability: Option<f64>,
    pub phase_envelope_stable: Option<bool>,
    pub sample_step_certificate_stable: Option<bool>,
    pub render_tolerance_certificate_stable: Option<bool>,
    pub render_tolerance_refusal: Option<String>,
    pub solver_certificate_stable: Option<bool>,
    pub topology: Option<TopologyComparison>,
    pub boundary: Option<BoundaryTail>,
    pub max_palette_code_delta: Option<u8>,
    pub profile_max_channel_delta: Option<u8>,
    pub profile_mean_channel_delta: Option<f64>,
    pub internal_to_pure_max_channel_delta: Option<u8>,
    pub internal_to_pure_mean_channel_delta: Option<f64>,
    pub internal_to_seam_max_channel_delta: Option<u8>,
    pub internal_to_seam_mean_channel_delta: Option<f64>,
    pub verifier_clean: bool,
    pub measurement_refusal: Option<String>,
}

pub(crate) struct BoundaryPopulationTail {
    pub render_p95_q95_px: f64,
    pub render_p99_q99_px: f64,
    pub max_px: f64,
    pub pooled_samples: u64,
    pub p95_gate_met: bool,
    pub p99_gate_met: bool,
    pub max_gate_met: bool,
}

/// Exact pooled-sample gate decisions plus conservative render-tail
/// diagnostics. Missing or differently preregistered counts invalidate the
/// population summary.
pub(crate) fn boundary_population_tail(
    rows: &[&MeasurementRow],
    p95_gate_px: f64,
    p99_gate_px: f64,
    max_gate_px: f64,
) -> Option<BoundaryPopulationTail> {
    fn field_quantile(
        rows: &[&MeasurementRow],
        value: impl Fn(&BoundaryTail) -> f64,
        q: f64,
    ) -> Option<f64> {
        let mut values = rows
            .iter()
            .filter_map(|row| row.boundary.as_ref().map(&value))
            .collect::<Vec<_>>();
        if values.len() != rows.len() || values.is_empty() {
            return None;
        }
        values.sort_by(f64::total_cmp);
        let index = ((values.len() - 1) as f64 * q).ceil() as usize;
        Some(values[index.min(values.len() - 1)])
    }

    let tails = rows
        .iter()
        .map(|row| row.boundary.as_ref())
        .collect::<Option<Vec<_>>>()?;
    if tails.is_empty()
        || tails.iter().any(|tail| {
            tail.samples == 0
                || tail.gate_counts.p95_gate_px.to_bits() != p95_gate_px.to_bits()
                || tail.gate_counts.p99_gate_px.to_bits() != p99_gate_px.to_bits()
                || tail.gate_counts.samples_at_or_below_p95_gate > tail.samples
                || tail.gate_counts.samples_at_or_below_p99_gate > tail.samples
        })
    {
        return None;
    }
    let pooled_samples = tails
        .iter()
        .try_fold(0u64, |sum, tail| sum.checked_add(tail.samples))?;
    let p95_at_or_below = tails.iter().try_fold(0u64, |sum, tail| {
        sum.checked_add(tail.gate_counts.samples_at_or_below_p95_gate)
    })?;
    let p99_at_or_below = tails.iter().try_fold(0u64, |sum, tail| {
        sum.checked_add(tail.gate_counts.samples_at_or_below_p99_gate)
    })?;
    let required = |q: f64| ((pooled_samples - 1) as f64 * q).ceil() as u64 + 1;
    Some(BoundaryPopulationTail {
        render_p95_q95_px: field_quantile(rows, |tail| tail.p95_px, 0.95)?,
        render_p99_q99_px: field_quantile(rows, |tail| tail.p99_px, 0.99)?,
        max_px: field_quantile(rows, |tail| tail.max_px, 1.0)?,
        pooled_samples,
        p95_gate_met: p95_at_or_below >= required(0.95),
        p99_gate_met: p99_at_or_below >= required(0.99),
        max_gate_met: tails.iter().all(|tail| tail.max_px <= max_gate_px),
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeasurementReport {
    pub schema: String,
    pub scope: String,
    pub split: String,
    pub preset: Preset,
    pub procedural_generation: u32,
    pub population_policy: String,
    pub procedural_variants_per_family: usize,
    pub mandatory_sizes_px: Vec<u32>,
    pub rasterizers: Vec<String>,
    pub identity: vice_opt::ModelIdentity,
    pub delivery_policy_sha256: String,
    pub confidence_calibration: Option<vice_core::ConfidenceCalibration>,
    /// Source-group shards included in this report. Sharding never splits a
    /// source group, so correlated renders cannot become independent trials.
    pub included_shards: Vec<u32>,
    pub shard_count: u32,
    pub max_workers_per_shard: u32,
    pub complete: bool,
    pub expected_renders_included_shards: u64,
    pub resumed_rows: u64,
    pub runs: u32,
    pub rows: Vec<MeasurementRow>,
    pub source_groups: u64,
    pub renders: u64,
    pub candidates_available: u64,
    pub truncated_renders: u64,
    pub elapsed_ms: u64,
    /// Aggregate process peak; multi-worker runs are not misreported as
    /// independent per-row memory observations.
    pub peak_working_set_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeasurementRequest {
    pub scope: MeasurementScope,
    pub preset: Preset,
    pub size_filter: Option<u32>,
    pub workers: usize,
    pub shard_index: u32,
    pub shard_count: u32,
}

impl MeasurementRequest {
    pub fn new(scope: MeasurementScope) -> Self {
        Self {
            scope,
            preset: preset_for_scope(scope),
            size_filter: None,
            workers: default_worker_count(),
            shard_index: 0,
            shard_count: 1,
        }
    }

    fn validate(self) -> Result<Self, String> {
        if self.workers == 0 {
            return Err("M7 measurement workers must be positive".into());
        }
        if self.shard_count == 0 || self.shard_index >= self.shard_count {
            return Err(format!(
                "invalid M7 shard {}/{}",
                self.shard_index, self.shard_count
            ));
        }
        Ok(self)
    }
}

mod measure;
#[cfg(test)]
use measure::measurement_shard;
use measure::preset_for_scope;
pub use measure::{
    default_worker_count, measure, measure_to_path, measure_to_path_with_config,
    measure_with_config, merge_reports, read_report, report_content_sha256, write_report,
};

#[derive(Debug, Clone, Copy)]
struct MeasurementExecution {
    preset: Preset,
    capture_baseline: bool,
}

fn measure_one(
    group_id: &str,
    shape_family: &str,
    truth_scene: &GtScene,
    cell: &DegradationCell,
    equivalence_members: usize,
    config: &CoreConfig,
    execution: MeasurementExecution,
) -> MeasurementRow {
    let started = Instant::now();
    let fixture = match render_cell(truth_scene, cell, equivalence_members) {
        Ok(fixture) => fixture,
        Err(error) => {
            return refusal_row(
                group_id,
                shape_family,
                truth_scene,
                cell,
                "render truth fixture",
                error,
                started,
            )
        }
    };
    let png = match encode_png(fixture.width_px, fixture.height_px, &fixture.rgba8) {
        Ok(bytes) => bytes,
        Err(error) => {
            return refusal_row(
                group_id,
                shape_family,
                truth_scene,
                cell,
                "encode input PNG",
                error,
                started,
            )
        }
    };
    let request = VectorizeRequest {
        preset: execution.preset,
        production: config.is_sealed_production(),
        ..VectorizeRequest::default()
    };
    let run = if execution.capture_baseline {
        vice_core::vectorize_for_calibration(&png, &request, config)
    } else {
        vice_core::vectorize_for_calibration_without_baseline(&png, &request, config)
    };
    let report = run.outcome.report();
    let mut row = MeasurementRow {
        group_id: group_id.to_string(),
        scene_id: truth_scene.id().to_string(),
        shape_family: shape_family.to_string(),
        cell_id: fixture.cell_id,
        size_px: fixture.width_px,
        rasterizer: cell.profile.as_str().to_string(),
        identifiability: fixture.identifiability.as_str().to_string(),
        core_runtime_ms: report.runtime.elapsed_ms,
        runtime_stages: report.runtime.stages.clone(),
        court_runtime_ms: 0,
        row_elapsed_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
        decision_status: format!("{:?}", report.status).to_lowercase(),
        decision_reason: report.reason.as_ref().map(stable_failure_reason),
        production_provenance: report.production,
        production_accepted: matches!(&run.outcome, vice_core::VectorizeOutcome::Success(_)),
        candidate_available: false,
        selected_hypothesis_id: report.selected_hypothesis_id.clone(),
        selected_scene_digest_sha256: None,
        selected_delivery_digest_sha256: None,
        selected_artifact_bundle_sha256: None,
        selected_complexity: None,
        internal_baseline: None,
        internal_baseline_refusals: run
            .baseline_refusals
            .iter()
            .map(|refusal| format!("{:?}: {}", refusal.stage, refusal.detail))
            .collect(),
        pf_oracle: None,
        cost_refusal_histogram: cost_refusal_histogram(report),
        numerical_conditioning: numerical_conditioning(report),
        search_truncated: report.search_mass.as_ref().map(|search| search.truncated),
        explored_mass: report
            .search_mass
            .as_ref()
            .map(|search| search.explored_mass),
        topology_classes_upper_bound: report
            .search_mass
            .as_ref()
            .map(|search| search.topology_classes_upper_bound),
        formation_classes_upper_bound: report
            .search_mass
            .as_ref()
            .map(|search| search.formation_classes_upper_bound),
        top_topology_explored_mass: report
            .search_mass
            .as_ref()
            .and_then(|search| search.topology.first())
            .map(|class| class.explored_mass),
        top_formation_explored_mass: report
            .search_mass
            .as_ref()
            .and_then(|search| search.formation.first())
            .map(|class| class.explored_mass),
        selected_delivery_mass: None,
        retained_normalized_mass: None,
        delivery_classes: report
            .search_mass
            .as_ref()
            .map(|search| search.delivery.len().try_into().unwrap_or(u64::MAX)),
        top2_class_margin_bits: report.search_mass.as_ref().and_then(|search| {
            let top1 = search.delivery.first()?.explored_mass;
            let top2 = search.delivery.get(1)?.explored_mass;
            (top1 > 0.0 && top2 > 0.0)
                .then(|| (top1 / top2).log2())
                .filter(|margin| margin.is_finite())
        }),
        posterior_lower_bound: None,
        posterior_bound_status: "absent".into(),
        unexplored_proxy_hypotheses: unexplored_proxy_hypotheses(report),
        candidate_bytes: report.runtime.candidate_bytes,
        serialized_pixel_bits: None,
        serialized_pixel_bits_per_block: None,
        support_isotopy_displacement_px: None,
        evidence_palette_shift_codes: None,
        palette_support_px: None,
        palette_interval_radius_codes: None,
        paint_calibration_class: None,
        empirical_correlation_length_px: None,
        max_abs_lag1: None,
        topology_entropy_upper_bound: None,
        topology_entropy_bound_status: "absent".into(),
        formation_entropy_upper_bound: None,
        formation_entropy_bound_status: "absent".into(),
        perturbation_stability: None,
        phase_envelope_stable: None,
        sample_step_certificate_stable: None,
        render_tolerance_certificate_stable: None,
        render_tolerance_refusal: None,
        solver_certificate_stable: None,
        topology: None,
        boundary: None,
        max_palette_code_delta: None,
        profile_max_channel_delta: None,
        profile_mean_channel_delta: None,
        internal_to_pure_max_channel_delta: None,
        internal_to_pure_mean_channel_delta: None,
        internal_to_seam_max_channel_delta: None,
        internal_to_seam_mean_channel_delta: None,
        verifier_clean: false,
        measurement_refusal: None,
    };
    if let Some(metrics) = &report.confidence_metrics {
        (
            row.topology_entropy_upper_bound,
            row.topology_entropy_bound_status,
        ) = measured_bound(&metrics.topology_entropy_upper_bound);
        (
            row.formation_entropy_upper_bound,
            row.formation_entropy_bound_status,
        ) = measured_bound(&metrics.formation_entropy_upper_bound);
        row.perturbation_stability = Some(metrics.perturbation_stability.score);
        row.support_isotopy_displacement_px = Some(metrics.support_isotopy_displacement_px);
        row.evidence_palette_shift_codes = Some(metrics.evidence_palette_shift_codes);
        row.palette_support_px = Some(metrics.palette_support_px);
        row.palette_interval_radius_codes = Some(metrics.palette_interval_radius_codes);
        row.paint_calibration_class = Some(metrics.paint_calibration_class.clone());
        row.phase_envelope_stable = Some(metrics.perturbation_stability.phase_envelope_stable);
        row.sample_step_certificate_stable = Some(
            metrics
                .perturbation_stability
                .sample_step_certificate_stable,
        );
        row.render_tolerance_certificate_stable = Some(
            metrics
                .perturbation_stability
                .render_tolerance_certificate_stable,
        );
        row.render_tolerance_refusal = metrics
            .perturbation_stability
            .render_tolerance_refusal
            .clone();
        row.solver_certificate_stable =
            Some(metrics.perturbation_stability.solver_certificate_stable);
    }
    if let Some(best) = report
        .search_mass
        .as_ref()
        .and_then(vice_opt::SearchMassCertificate::best_delivery)
    {
        row.selected_delivery_mass = Some(best.explored_mass);
        row.retained_normalized_mass = Some(best.retained_normalized_mass);
        match best.posterior_lower_bound {
            BoundValue::Certified(value) => {
                row.posterior_lower_bound = Some(value);
                row.posterior_bound_status = "certified".into();
            }
            BoundValue::EmpiricallyCalibrated(value) => {
                row.posterior_lower_bound = Some(value);
                row.posterior_bound_status = "empirically_calibrated".into();
            }
            BoundValue::Unknown => row.posterior_bound_status = "unknown".into(),
        }
    }
    let Some(witness) = run.selected else {
        let reason = report.reason.as_ref().map_or_else(
            || "no selected calibration witness".into(),
            |reason| {
                serde_json::to_string(reason).unwrap_or_else(|_| "unserializable reason".into())
            },
        );
        let candidate_refusals = serde_json::to_string(&report.candidate_refusals)
            .unwrap_or_else(|_| "unserializable candidate refusals".into());
        row.measurement_refusal =
            Some(format!("{reason}; candidate_refusals={candidate_refusals}"));
        return row;
    };
    row.selected_scene_digest_sha256 = Some(witness.candidate.scene_digest_sha256.clone());
    row.selected_delivery_digest_sha256 = Some(witness.candidate.delivery_digest.clone());
    row.selected_artifact_bundle_sha256 = Some(artifact_bundle_digest(&witness));
    row.selected_complexity = scene_complexity(&witness).ok();
    let court_started = Instant::now();
    match judge_witness(truth_scene, cell, &witness) {
        Ok((topology, boundary, paint_delta)) => {
            row.candidate_available = true;
            row.topology = Some(topology);
            row.boundary = Some(boundary);
            row.max_palette_code_delta = Some(paint_delta);
            let candidate = &witness.candidate;
            let seal = &candidate.delivery_seal;
            row.profile_max_channel_delta = Some(seal.profile_comparison.max_channel_delta);
            row.profile_mean_channel_delta = Some(seal.profile_comparison.mean_channel_delta);
            row.internal_to_pure_max_channel_delta =
                Some(seal.internal_to_pure_comparison.max_channel_delta);
            row.internal_to_pure_mean_channel_delta =
                Some(seal.internal_to_pure_comparison.mean_channel_delta);
            row.internal_to_seam_max_channel_delta =
                Some(seal.internal_to_seam_comparison.max_channel_delta);
            row.internal_to_seam_mean_channel_delta =
                Some(seal.internal_to_seam_comparison.mean_channel_delta);
            let diagnostics = &candidate.score.diagnostics;
            row.serialized_pixel_bits = Some(candidate.score.pixel_bits);
            row.serialized_pixel_bits_per_block = (diagnostics.blocks > 0)
                .then_some(candidate.score.pixel_bits / diagnostics.blocks as f64);
            row.empirical_correlation_length_px = Some(diagnostics.empirical_correlation_length_px);
            row.max_abs_lag1 = Some(diagnostics.lag1_x.abs().max(diagnostics.lag1_y.abs()));
            row.verifier_clean = candidate.pre_quantization.worst_g1_spread_rad
                <= config.verification.max_g1_spread_rad;
        }
        Err(error) => row.measurement_refusal = Some(error),
    }
    if let Some(baseline) = run.baseline {
        row.internal_baseline =
            measure_internal_baseline(truth_scene, cell, &baseline, config).ok();
    }
    row.pf_oracle = Some(measure_pf_oracle(
        truth_scene,
        cell,
        &fixture.rgba8,
        &witness,
        config,
    ));
    row.court_runtime_ms = court_started
        .elapsed()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX);
    row.row_elapsed_ms = started.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
    row
}

mod row;
use row::*;

mod judge;
#[cfg(test)]
use judge::{directed_distances, point_segment_distance, quantile, SegmentIndex};
use judge::{encode_png, judge_witness};

#[cfg(test)]
#[path = "m7/tests.rs"]
mod tests;
