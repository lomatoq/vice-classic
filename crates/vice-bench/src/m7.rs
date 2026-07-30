//! M7 held-out measurement court.
//!
//! The production objective never judges itself here. Ground-truth topology
//! comes from the independently built certified fixture, while boundary and
//! paint errors compare the selected canonical scene against that truth.
//! Rows are raw measurements: confidence and tail thresholds are frozen from
//! calibration rows before the sealed-audit split is opened.

pub mod analysis;
pub mod baseline;
pub mod determinism;
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

use crate::gt::corpus::all_groups_with_variants;
use crate::gt::degradation::{matrix_v1, render_cell, DegradationCell};
use crate::gt::grammar::AUTHORING_CANVAS_PX;
use crate::gt::raster::RasterProfile;
use crate::gt::split::{Split, SPLIT_POLICY_V1};
use crate::gt::{GtScene, PartitionTruth};

pub const M7_MEASUREMENT_SCHEMA: &str = "vice-classic/m7-held-out-measurement/v11";
pub const M7_RELEASE_PROCEDURAL_VARIANTS: usize = 200;
pub const M7_MANDATORY_SIZES: [u32; 3] = [128, 256, 512];
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
pub struct BoundaryTail {
    pub samples: u64,
    pub p95_px: f64,
    pub p99_px: f64,
    pub max_px: f64,
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
pub struct MeasurementRow {
    pub group_id: String,
    pub scene_id: String,
    pub shape_family: String,
    pub cell_id: String,
    pub size_px: u32,
    pub rasterizer: String,
    pub identifiability: String,
    pub core_runtime_ms: u64,
    pub court_runtime_ms: u64,
    pub row_elapsed_ms: u64,
    pub decision_status: String,
    pub production_provenance: bool,
    pub production_accepted: bool,
    pub candidate_available: bool,
    pub selected_hypothesis_id: Option<String>,
    pub selected_scene_digest_sha256: Option<String>,
    pub selected_delivery_digest_sha256: Option<String>,
    pub selected_artifact_bundle_sha256: Option<String>,
    pub selected_complexity: Option<SceneComplexity>,
    pub internal_baseline: Option<InternalBaselineMeasurement>,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeasurementReport {
    pub schema: String,
    pub scope: String,
    pub split: String,
    pub preset: Preset,
    pub procedural_variants_per_family: usize,
    pub mandatory_sizes_px: Vec<u32>,
    pub rasterizers: Vec<String>,
    pub identity: vice_opt::ModelIdentity,
    pub delivery_policy_sha256: String,
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

pub fn default_worker_count() -> usize {
    std::thread::available_parallelism()
        .map_or(1, usize::from)
        // One Quality run may approach the 1 GiB per-process research
        // envelope. Two workers provide useful parallelism without making an
        // unsafe aggregate-memory promise.
        .min(2)
}

#[derive(Clone, Copy)]
struct MeasurementJob {
    group: usize,
    scene: usize,
    cell: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct MeasurementJournalHeader {
    schema: String,
    scope: String,
    split: String,
    preset: Preset,
    procedural_variants_per_family: usize,
    mandatory_sizes_px: Vec<u32>,
    rasterizers: Vec<String>,
    identity: vice_opt::ModelIdentity,
    delivery_policy_sha256: String,
    shard_index: u32,
    shard_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "record", rename_all = "snake_case")]
enum MeasurementJournalRecord {
    Header {
        header: Box<MeasurementJournalHeader>,
    },
    Row {
        row: Box<MeasurementRow>,
    },
}

pub fn measure(request: MeasurementRequest) -> Result<MeasurementReport, String> {
    let config = CoreConfig::development_for(request.preset);
    measure_with_config(request, &config)
}

pub fn measure_with_config(
    request: MeasurementRequest,
    config: &CoreConfig,
) -> Result<MeasurementReport, String> {
    measure_resuming(request, config, Vec::new(), 0, 0, 0, 0, |_| Ok(()))
}

pub fn measure_to_path(
    request: MeasurementRequest,
    out: &Path,
    resume: bool,
) -> Result<MeasurementReport, String> {
    let config = CoreConfig::development_for(request.preset);
    measure_to_path_with_config(request, &config, out, resume)
}

pub fn measure_to_path_with_config(
    request: MeasurementRequest,
    config: &CoreConfig,
    out: &Path,
    resume: bool,
) -> Result<MeasurementReport, String> {
    let request = request.validate()?;
    let journal = journal_path(out);
    if !resume && (out.exists() || journal.exists()) {
        return Err(format!(
            "{} or its checkpoint journal already exists; pass --resume or choose a new output",
            out.display()
        ));
    }

    let expected_header = journal_header(request, config)?;
    let mut rows = Vec::new();
    let mut previous_elapsed_ms = 0;
    let mut previous_runs = 0;
    let mut previous_max_workers = 0;
    let mut previous_peak_working_set_bytes = 0;
    if resume && out.exists() {
        let previous = read_report(out)?;
        validate_report_header(&previous, request, &expected_header)?;
        previous_elapsed_ms = previous.elapsed_ms;
        previous_runs = previous.runs;
        previous_max_workers = previous.max_workers_per_shard;
        previous_peak_working_set_bytes = previous.peak_working_set_bytes;
        rows.extend(previous.rows);
    }
    if resume && journal.exists() {
        let (header, journal_rows) = read_journal(&journal)?;
        if header != expected_header {
            return Err(format!(
                "checkpoint {} belongs to a different M7 measurement identity or shard",
                journal.display()
            ));
        }
        rows.extend(journal_rows);
    }

    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    let journal_exists = journal.exists();
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&journal)
        .map_err(|error| format!("open checkpoint {}: {error}", journal.display()))?;
    let mut writer = BufWriter::new(file);
    if !journal_exists {
        write_journal_record(
            &mut writer,
            &MeasurementJournalRecord::Header {
                header: Box::new(expected_header),
            },
        )?;
    }
    let report = measure_resuming(
        request,
        config,
        rows,
        previous_elapsed_ms,
        previous_runs,
        previous_max_workers,
        previous_peak_working_set_bytes,
        |row| {
            write_journal_record(
                &mut writer,
                &MeasurementJournalRecord::Row {
                    row: Box::new(row.clone()),
                },
            )
        },
    )?;
    writer
        .flush()
        .map_err(|error| format!("flush checkpoint {}: {error}", journal.display()))?;
    write_report(out, &report)?;
    Ok(report)
}

fn measure_resuming(
    request: MeasurementRequest,
    config: &CoreConfig,
    resume_rows: Vec<MeasurementRow>,
    previous_elapsed_ms: u64,
    previous_runs: u32,
    previous_max_workers: u32,
    previous_peak_working_set_bytes: u64,
    mut checkpoint: impl FnMut(&MeasurementRow) -> Result<(), String>,
) -> Result<MeasurementReport, String> {
    let request = request.validate()?;
    let started = Instant::now();
    let peak_memory = PeakWorkingSetMonitor::start()?;
    let scope = request.scope;
    if (scope == MeasurementScope::SealedAudit) != config.is_sealed_production() {
        return Err(
            "sealed-audit measurement requires a digest-pinned production config, while \
             development/calibration measurement requires an unsealed config"
                .into(),
        );
    }
    let groups = all_groups_with_variants(scope.variants())?;
    let cells = scope
        .cells()
        .into_iter()
        .filter(|cell| request.size_filter.is_none_or(|size| cell.size_px == size))
        .collect::<Vec<_>>();
    if cells.is_empty() {
        return Err("M7 measurement selected no degradation cells".into());
    }
    let split = scope.split();
    let preset = request.preset;
    let identity = config.identity();
    let mut jobs = Vec::new();
    let mut source_groups = BTreeSet::new();
    for (group_index, group) in groups.iter().enumerate() {
        if SPLIT_POLICY_V1.split_of_group(group) != split
            || (scope == MeasurementScope::CalibrationSmoke && group.id != "proc/annulus/000")
            || measurement_shard(&group.id, request.shard_count) != request.shard_index
        {
            continue;
        }
        source_groups.insert(group.id.as_str());
        for scene_index in 0..group.scenes.len() {
            for cell_index in 0..cells.len() {
                jobs.push(MeasurementJob {
                    group: group_index,
                    scene: scene_index,
                    cell: cell_index,
                });
            }
        }
    }
    let expected_keys = jobs
        .iter()
        .map(|job| {
            row_key_parts(
                &groups[job.group].id,
                groups[job.group].scenes[job.scene].id(),
                &cells[job.cell].id(),
            )
        })
        .collect::<BTreeSet<_>>();
    let mut rows_by_key = BTreeMap::new();
    for row in resume_rows {
        let key = row_key(&row);
        if !expected_keys.contains(&key) {
            return Err(format!(
                "resume row {key:?} is outside requested M7 shard/cell population"
            ));
        }
        match rows_by_key.insert(key.clone(), row.clone()) {
            Some(previous) if previous != row => {
                return Err(format!("conflicting resume rows for {key:?}"))
            }
            _ => {}
        }
    }
    let resumed_rows = rows_by_key.len() as u64;
    let pending = jobs
        .iter()
        .copied()
        .filter(|job| {
            !rows_by_key.contains_key(&row_key_parts(
                &groups[job.group].id,
                groups[job.group].scenes[job.scene].id(),
                &cells[job.cell].id(),
            ))
        })
        .collect::<Vec<_>>();
    let worker_count = request.workers.min(pending.len().max(1));
    if !pending.is_empty() {
        let next = AtomicUsize::new(0);
        let (sender, receiver) = mpsc::channel();
        let mut checkpoint_error = None;
        std::thread::scope(|threads| {
            for _ in 0..worker_count {
                let sender = sender.clone();
                let next = &next;
                let pending = &pending;
                let groups = &groups;
                let cells = &cells;
                let config = config;
                threads.spawn(move || loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(job) = pending.get(index) else {
                        break;
                    };
                    let group = &groups[job.group];
                    let scene = &group.scenes[job.scene];
                    let equivalence_members = group
                        .equivalence_class
                        .as_ref()
                        .map_or(1, |class| class.members.len());
                    let row = measure_one(
                        group.id.as_str(),
                        group.shape_family.as_str(),
                        scene,
                        &cells[job.cell],
                        equivalence_members,
                        preset,
                        config,
                    );
                    if sender.send(row).is_err() {
                        break;
                    }
                });
            }
            drop(sender);
            for row in receiver {
                let key = row_key(&row);
                if checkpoint_error.is_none() {
                    if let Err(error) = checkpoint(&row) {
                        checkpoint_error = Some(error);
                    }
                }
                rows_by_key.insert(key, row);
            }
        });
        if let Some(error) = checkpoint_error {
            return Err(error);
        }
    }
    let mut rows = rows_by_key.into_values().collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        (
            left.group_id.as_str(),
            left.scene_id.as_str(),
            left.cell_id.as_str(),
        )
            .cmp(&(
                right.group_id.as_str(),
                right.scene_id.as_str(),
                right.cell_id.as_str(),
            ))
    });
    let complete = rows.len() == jobs.len();
    let candidates_available = rows.iter().filter(|row| row.candidate_available).count() as u64;
    let truncated_renders = rows
        .iter()
        .filter(|row| row.search_truncated == Some(true))
        .count() as u64;
    let peak_working_set_bytes = previous_peak_working_set_bytes.max(peak_memory.finish());
    Ok(MeasurementReport {
        schema: M7_MEASUREMENT_SCHEMA.to_string(),
        scope: scope.as_str().to_string(),
        split: split.as_str().to_string(),
        preset,
        procedural_variants_per_family: scope.variants(),
        mandatory_sizes_px: {
            let mut sizes = cells.iter().map(|cell| cell.size_px).collect::<Vec<_>>();
            sizes.sort_unstable();
            sizes.dedup();
            sizes
        },
        rasterizers: {
            let mut rasterizers = cells
                .iter()
                .map(|cell| cell.profile.as_str().to_string())
                .collect::<Vec<_>>();
            rasterizers.sort();
            rasterizers.dedup();
            rasterizers
        },
        identity,
        delivery_policy_sha256: config.delivery_policy_sha256(),
        included_shards: vec![request.shard_index],
        shard_count: request.shard_count,
        max_workers_per_shard: previous_max_workers
            .max(worker_count.try_into().unwrap_or(u32::MAX)),
        complete,
        expected_renders_included_shards: jobs.len().try_into().unwrap_or(u64::MAX),
        resumed_rows,
        runs: previous_runs.saturating_add(1),
        renders: rows.len() as u64,
        rows,
        source_groups: source_groups.len().try_into().unwrap_or(u64::MAX),
        candidates_available,
        truncated_renders,
        elapsed_ms: previous_elapsed_ms
            .saturating_add(started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)),
        peak_working_set_bytes,
    })
}

struct PeakWorkingSetMonitor {
    stop: Arc<AtomicBool>,
    peak: Arc<AtomicU64>,
    worker: Option<JoinHandle<()>>,
}

impl PeakWorkingSetMonitor {
    fn start() -> Result<Self, String> {
        let pid = sysinfo::get_current_pid().map_err(|error| error.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let peak = Arc::new(AtomicU64::new(0));
        let worker_stop = Arc::clone(&stop);
        let worker_peak = Arc::clone(&peak);
        let worker = std::thread::spawn(move || {
            let mut system = sysinfo::System::new();
            while !worker_stop.load(Ordering::Relaxed) {
                system.refresh_processes_specifics(
                    sysinfo::ProcessesToUpdate::Some(&[pid]),
                    true,
                    sysinfo::ProcessRefreshKind::nothing().with_memory(),
                );
                if let Some(process) = system.process(pid) {
                    worker_peak.fetch_max(process.memory(), Ordering::Relaxed);
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        });
        Ok(Self {
            stop,
            peak,
            worker: Some(worker),
        })
    }

    fn finish(mut self) -> u64 {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.peak.load(Ordering::Relaxed)
    }
}

impl Drop for PeakWorkingSetMonitor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn row_key(row: &MeasurementRow) -> String {
    row_key_parts(&row.group_id, &row.scene_id, &row.cell_id)
}

fn row_key_parts(group_id: &str, scene_id: &str, cell_id: &str) -> String {
    format!("{group_id}\0{scene_id}\0{cell_id}")
}

fn measurement_shard(group_id: &str, shard_count: u32) -> u32 {
    let digest = Sha256::digest(
        [
            b"vice-classic/m7-source-shard/v1/".as_slice(),
            group_id.as_bytes(),
        ]
        .concat(),
    );
    u64::from_be_bytes(digest[..8].try_into().expect("sha256 has eight bytes"))
        .rem_euclid(u64::from(shard_count)) as u32
}

fn preset_for_scope(scope: MeasurementScope) -> Preset {
    if scope == MeasurementScope::Smoke {
        Preset::Fast
    } else {
        Preset::Quality
    }
}

fn journal_header(
    request: MeasurementRequest,
    config: &CoreConfig,
) -> Result<MeasurementJournalHeader, String> {
    let cells = request
        .scope
        .cells()
        .into_iter()
        .filter(|cell| request.size_filter.is_none_or(|size| cell.size_px == size))
        .collect::<Vec<_>>();
    if cells.is_empty() {
        return Err("M7 measurement selected no degradation cells".into());
    }
    let mut sizes = cells.iter().map(|cell| cell.size_px).collect::<Vec<_>>();
    sizes.sort_unstable();
    sizes.dedup();
    let mut rasterizers = cells
        .iter()
        .map(|cell| cell.profile.as_str().to_string())
        .collect::<Vec<_>>();
    rasterizers.sort();
    rasterizers.dedup();
    Ok(MeasurementJournalHeader {
        schema: M7_MEASUREMENT_SCHEMA.to_string(),
        scope: request.scope.as_str().to_string(),
        split: request.scope.split().as_str().to_string(),
        preset: request.preset,
        procedural_variants_per_family: request.scope.variants(),
        mandatory_sizes_px: sizes,
        rasterizers,
        identity: config.identity(),
        delivery_policy_sha256: config.delivery_policy_sha256(),
        shard_index: request.shard_index,
        shard_count: request.shard_count,
    })
}

fn validate_report_header(
    report: &MeasurementReport,
    request: MeasurementRequest,
    expected: &MeasurementJournalHeader,
) -> Result<(), String> {
    let matches = report.schema == expected.schema
        && report.scope == expected.scope
        && report.split == expected.split
        && report.preset == expected.preset
        && report.procedural_variants_per_family == expected.procedural_variants_per_family
        && report.mandatory_sizes_px == expected.mandatory_sizes_px
        && report.rasterizers == expected.rasterizers
        && report.identity == expected.identity
        && report.delivery_policy_sha256 == expected.delivery_policy_sha256
        && report.included_shards == [request.shard_index]
        && report.shard_count == request.shard_count;
    if matches {
        Ok(())
    } else {
        Err("existing M7 report belongs to a different identity, scope, cell set, or shard".into())
    }
}

fn journal_path(out: &Path) -> PathBuf {
    let mut name = out.as_os_str().to_os_string();
    name.push(".rows.jsonl");
    PathBuf::from(name)
}

fn write_journal_record(
    writer: &mut BufWriter<File>,
    record: &MeasurementJournalRecord,
) -> Result<(), String> {
    serde_json::to_writer(&mut *writer, record)
        .map_err(|error| format!("serialize M7 checkpoint: {error}"))?;
    writer
        .write_all(b"\n")
        .and_then(|_| writer.flush())
        .map_err(|error| format!("write M7 checkpoint: {error}"))
}

fn read_journal(path: &Path) -> Result<(MeasurementJournalHeader, Vec<MeasurementRow>), String> {
    let file = File::open(path).map_err(|error| format!("open {}: {error}", path.display()))?;
    let mut header = None;
    let mut rows = Vec::new();
    for (line_index, line) in BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|error| format!("read {}: {error}", path.display()))?;
        let record: MeasurementJournalRecord = serde_json::from_str(&line).map_err(|error| {
            format!("parse {} line {}: {error}", path.display(), line_index + 1)
        })?;
        match record {
            MeasurementJournalRecord::Header { header: found } if line_index == 0 => {
                header = Some(*found)
            }
            MeasurementJournalRecord::Header { .. } => {
                return Err(format!("{} contains more than one header", path.display()))
            }
            MeasurementJournalRecord::Row { row } => rows.push(*row),
        }
    }
    header
        .map(|header| (header, rows))
        .ok_or_else(|| format!("{} has no M7 checkpoint header", path.display()))
}

pub fn read_report(path: &Path) -> Result<MeasurementReport, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("parse {}: {error}", path.display()))
}

pub fn write_report(path: &Path, report: &MeasurementReport) -> Result<(), String> {
    let text = serde_json::to_string_pretty(report)
        .map_err(|error| format!("serialize report: {error}"))?;
    std::fs::write(path, format!("{text}\n"))
        .map_err(|error| format!("write {}: {error}", path.display()))
}

pub fn merge_reports(reports: Vec<MeasurementReport>) -> Result<MeasurementReport, String> {
    let Some(first) = reports.first().cloned() else {
        return Err("M7 merge requires at least one report".into());
    };
    let compatible = |report: &MeasurementReport| {
        report.schema == first.schema
            && report.scope == first.scope
            && report.split == first.split
            && report.preset == first.preset
            && report.procedural_variants_per_family == first.procedural_variants_per_family
            && report.mandatory_sizes_px == first.mandatory_sizes_px
            && report.rasterizers == first.rasterizers
            && report.identity == first.identity
            && report.delivery_policy_sha256 == first.delivery_policy_sha256
            && report.shard_count == first.shard_count
    };
    if reports.iter().any(|report| !compatible(report)) {
        return Err(
            "M7 merge inputs disagree on schema, identity, population, cell set, or shard count"
                .into(),
        );
    }

    let mut included_shards = BTreeSet::new();
    let mut rows = BTreeMap::new();
    let mut source_groups = 0u64;
    let mut expected_renders = 0u64;
    let mut resumed_rows = 0u64;
    let mut runs = 0u32;
    let mut max_workers = 0u32;
    let mut elapsed_ms = 0u64;
    let mut peak_working_set_bytes = 0u64;
    let mut inputs_complete = true;
    for report in reports {
        for shard in report.included_shards {
            if !included_shards.insert(shard) {
                return Err(format!("M7 merge includes shard {shard} more than once"));
            }
        }
        for row in report.rows {
            let key = row_key(&row);
            match rows.insert(key.clone(), row.clone()) {
                Some(previous) if previous != row => {
                    return Err(format!("M7 merge has conflicting rows for {key:?}"))
                }
                Some(_) => return Err(format!("M7 merge duplicates row {key:?}")),
                None => {}
            }
        }
        source_groups = source_groups.saturating_add(report.source_groups);
        expected_renders = expected_renders.saturating_add(report.expected_renders_included_shards);
        resumed_rows = resumed_rows.saturating_add(report.resumed_rows);
        runs = runs.saturating_add(report.runs);
        max_workers = max_workers.max(report.max_workers_per_shard);
        elapsed_ms = elapsed_ms.saturating_add(report.elapsed_ms);
        peak_working_set_bytes = peak_working_set_bytes.max(report.peak_working_set_bytes);
        inputs_complete &= report.complete;
    }
    let rows = rows.into_values().collect::<Vec<_>>();
    let candidates_available = rows.iter().filter(|row| row.candidate_available).count() as u64;
    let truncated_renders = rows
        .iter()
        .filter(|row| row.search_truncated == Some(true))
        .count() as u64;
    let all_shards_present = included_shards.len() == first.shard_count as usize
        && included_shards.iter().copied().eq(0..first.shard_count);
    Ok(MeasurementReport {
        schema: first.schema.clone(),
        scope: first.scope.clone(),
        split: first.split.clone(),
        preset: first.preset,
        procedural_variants_per_family: first.procedural_variants_per_family,
        mandatory_sizes_px: first.mandatory_sizes_px.clone(),
        rasterizers: first.rasterizers.clone(),
        identity: first.identity.clone(),
        delivery_policy_sha256: first.delivery_policy_sha256.clone(),
        included_shards: included_shards.into_iter().collect(),
        shard_count: first.shard_count,
        max_workers_per_shard: max_workers,
        complete: inputs_complete && all_shards_present && rows.len() as u64 == expected_renders,
        expected_renders_included_shards: expected_renders,
        resumed_rows,
        runs,
        renders: rows.len() as u64,
        rows,
        source_groups,
        candidates_available,
        truncated_renders,
        elapsed_ms,
        peak_working_set_bytes,
    })
}

fn measure_one(
    group_id: &str,
    shape_family: &str,
    truth_scene: &GtScene,
    cell: &DegradationCell,
    equivalence_members: usize,
    preset: Preset,
    config: &CoreConfig,
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
        preset,
        production: config.is_sealed_production(),
        ..VectorizeRequest::default()
    };
    let run = vice_core::vectorize_for_calibration(&png, &request, config);
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
        court_runtime_ms: 0,
        row_elapsed_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
        decision_status: format!("{:?}", report.status).to_lowercase(),
        production_provenance: report.production,
        production_accepted: matches!(&run.outcome, vice_core::VectorizeOutcome::Success(_)),
        candidate_available: false,
        selected_hypothesis_id: report.selected_hypothesis_id.clone(),
        selected_scene_digest_sha256: None,
        selected_delivery_digest_sha256: None,
        selected_artifact_bundle_sha256: None,
        selected_complexity: None,
        internal_baseline: None,
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
        row.measurement_refusal = Some(report.reason.as_ref().map_or_else(
            || "no selected calibration witness".into(),
            |reason| {
                serde_json::to_string(reason).unwrap_or_else(|_| "unserializable reason".into())
            },
        ));
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
    row.court_runtime_ms = court_started
        .elapsed()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX);
    row.row_elapsed_ms = started.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
    row
}

fn artifact_bundle_digest(witness: &vice_core::CalibrationWitness) -> String {
    let mut hash = Sha256::new();
    for (name, bytes) in [
        ("scene", witness.scene_json.as_slice()),
        ("export_plan", witness.export_plan_json.as_slice()),
        ("pure_svg", witness.pure_partition_svg.as_slice()),
        ("seam_svg", witness.seam_safe_svg.as_slice()),
        ("render", witness.rendered_png.as_slice()),
        ("seal", witness.seal_json.as_slice()),
    ] {
        hash.update((name.len() as u64).to_be_bytes());
        hash.update(name.as_bytes());
        hash.update((bytes.len() as u64).to_be_bytes());
        hash.update(bytes);
    }
    hex::encode(hash.finalize())
}

fn scene_complexity(witness: &vice_core::CalibrationWitness) -> Result<SceneComplexity, String> {
    let scene = vice_ir::parse_scene(&witness.scene_json)
        .map_err(|error| format!("parse scene for complexity: {error}"))?;
    Ok(SceneComplexity {
        vertices: scene.graph.vertices.len().try_into().unwrap_or(u64::MAX),
        boundaries: scene.graph.boundaries.len().try_into().unwrap_or(u64::MAX),
        curve_segments: scene
            .graph
            .boundaries
            .iter()
            .map(|boundary| u64::try_from(boundary.curve.segments.len()).unwrap_or(u64::MAX))
            .fold(0u64, u64::saturating_add),
        canonical_delivery_bytes: [
            witness.scene_json.len(),
            witness.export_plan_json.len(),
            witness.pure_partition_svg.len(),
            witness.seam_safe_svg.len(),
            witness.rendered_png.len(),
            witness.seal_json.len(),
        ]
        .into_iter()
        .map(|bytes| u64::try_from(bytes).unwrap_or(u64::MAX))
        .fold(0u64, u64::saturating_add),
    })
}

fn measure_internal_baseline(
    truth_scene: &GtScene,
    cell: &DegradationCell,
    witness: &vice_core::CalibrationWitness,
    config: &CoreConfig,
) -> Result<InternalBaselineMeasurement, String> {
    let (topology, boundary, max_palette_code_delta) = judge_witness(truth_scene, cell, witness)?;
    Ok(InternalBaselineMeasurement {
        hypothesis_id: witness.candidate.hypothesis_id.clone(),
        scene_digest_sha256: witness.candidate.scene_digest_sha256.clone(),
        delivery_digest_sha256: witness.candidate.delivery_digest.clone(),
        artifact_bundle_sha256: artifact_bundle_digest(witness),
        topology,
        boundary,
        max_palette_code_delta,
        profile_max_channel_delta: witness
            .candidate
            .delivery_seal
            .profile_comparison
            .max_channel_delta,
        profile_mean_channel_delta: witness
            .candidate
            .delivery_seal
            .profile_comparison
            .mean_channel_delta,
        internal_to_pure_max_channel_delta: witness
            .candidate
            .delivery_seal
            .internal_to_pure_comparison
            .max_channel_delta,
        internal_to_pure_mean_channel_delta: witness
            .candidate
            .delivery_seal
            .internal_to_pure_comparison
            .mean_channel_delta,
        internal_to_seam_max_channel_delta: witness
            .candidate
            .delivery_seal
            .internal_to_seam_comparison
            .max_channel_delta,
        internal_to_seam_mean_channel_delta: witness
            .candidate
            .delivery_seal
            .internal_to_seam_comparison
            .mean_channel_delta,
        complexity: scene_complexity(witness)?,
        verifier_clean: witness.candidate.pre_quantization.worst_g1_spread_rad
            <= config.verification.max_g1_spread_rad,
    })
}

fn measured_bound(bound: &BoundValue<f64>) -> (Option<f64>, String) {
    match bound {
        BoundValue::Certified(value) => (Some(*value), "certified".into()),
        BoundValue::EmpiricallyCalibrated(value) => (Some(*value), "empirically_calibrated".into()),
        BoundValue::Unknown => (None, "unknown".into()),
    }
}

fn unexplored_proxy_hypotheses(report: &vice_core::VectorizeReport) -> Option<u64> {
    let search = report.search_mass.as_ref()?;
    if !search.truncated {
        return Some(0);
    }
    let topology = report.topology.as_ref().map_or(0u64, |topology| {
        topology
            .prefit_budget_pruned_arms
            .len()
            .try_into()
            .unwrap_or(u64::MAX)
    });
    let fit = report.fits.iter().fold(0u64, |total, fit| {
        let skipped_levels = u64::try_from(fit.proposal_levels_skipped_after_certification)
            .unwrap_or(u64::MAX)
            .saturating_mul(vice_fit::K_DISCRETE_PATHS.try_into().unwrap_or(u64::MAX));
        total
            .saturating_add(fit.resource_pruned_proposals.try_into().unwrap_or(u64::MAX))
            .saturating_add(skipped_levels)
    });
    let materialization = report.beam.as_ref().map_or(0u64, |beam| {
        beam.unmaterialized_by_candidate_budget
            .saturating_add(beam.unmaterialized_by_time_budget)
    });
    Some(topology.saturating_add(fit).saturating_add(materialization))
}

fn refusal_row(
    group_id: &str,
    shape_family: &str,
    scene: &GtScene,
    cell: &DegradationCell,
    stage: &str,
    detail: String,
    started: Instant,
) -> MeasurementRow {
    MeasurementRow {
        group_id: group_id.to_string(),
        scene_id: scene.id().to_string(),
        shape_family: shape_family.to_string(),
        cell_id: cell.id(),
        size_px: cell.size_px,
        rasterizer: cell.profile.as_str().to_string(),
        identifiability: "measurement_refused".into(),
        core_runtime_ms: 0,
        court_runtime_ms: 0,
        row_elapsed_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
        decision_status: "measurement_refused".into(),
        production_provenance: false,
        production_accepted: false,
        candidate_available: false,
        selected_hypothesis_id: None,
        selected_scene_digest_sha256: None,
        selected_delivery_digest_sha256: None,
        selected_artifact_bundle_sha256: None,
        selected_complexity: None,
        internal_baseline: None,
        search_truncated: None,
        explored_mass: None,
        topology_classes_upper_bound: None,
        formation_classes_upper_bound: None,
        top_topology_explored_mass: None,
        top_formation_explored_mass: None,
        selected_delivery_mass: None,
        retained_normalized_mass: None,
        delivery_classes: None,
        top2_class_margin_bits: None,
        posterior_lower_bound: None,
        posterior_bound_status: "absent".into(),
        unexplored_proxy_hypotheses: None,
        candidate_bytes: 0,
        serialized_pixel_bits: None,
        serialized_pixel_bits_per_block: None,
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
        measurement_refusal: Some(format!("{stage}: {detail}")),
    }
}

fn judge_witness(
    truth_scene: &GtScene,
    cell: &DegradationCell,
    witness: &vice_core::CalibrationWitness,
) -> Result<(TopologyComparison, BoundaryTail, u8), String> {
    let selected = vice_ir::parse_scene(&witness.scene_json)
        .map_err(|error| format!("parse selected scene: {error}"))?;
    let selected = ValidatedScene::new(selected)
        .map_err(|error| format!("validate selected scene: {error}"))?;
    let selected_mesh = CertifiedMesh::from_scene(&selected, RenderOptions::default())
        .map_err(|error| format!("certify selected scene: {error}"))?;
    let selected_truth = PartitionTruth::measure(&selected, &selected_mesh)
        .map_err(|error| format!("measure selected partition: {error}"))?;
    let truth = truth_scene.partition_truth();
    let topology = TopologyComparison {
        truth_visible_faces: truth.visible_faces,
        selected_visible_faces: selected_truth.visible_faces,
        truth_components: truth.components,
        selected_components: selected_truth.components,
        truth_holes: truth.holes,
        selected_holes: selected_truth.holes,
        truth_exterior: truth.exterior_model.to_string(),
        selected_exterior: selected_truth.exterior_model.to_string(),
        exact: truth.visible_faces == selected_truth.visible_faces
            && truth.components == selected_truth.components
            && truth.holes == selected_truth.holes
            && truth.exterior_model == selected_truth.exterior_model,
    };
    let scale = f64::from(cell.size_px) / f64::from(AUTHORING_CANVAS_PX);
    let truth_segments = mesh_segments(
        truth_scene.certified(),
        scale,
        cell.subpixel_dx,
        cell.subpixel_dy,
    );
    let selected_segments = mesh_segments(&selected_mesh, 1.0, 0.0, 0.0);
    let mut distances = directed_distances(&truth_segments, &selected_segments);
    distances.extend(directed_distances(&selected_segments, &truth_segments));
    if distances.is_empty() {
        return Err("boundary court received no finite distance samples".into());
    }
    distances.sort_by(f64::total_cmp);
    let boundary = BoundaryTail {
        samples: distances.len() as u64,
        p95_px: quantile(&distances, 0.95),
        p99_px: quantile(&distances, 0.99),
        max_px: *distances.last().expect("nonempty"),
    };
    let paint_delta = palette_code_delta(&truth.palette, &selected_truth.palette);
    Ok((topology, boundary, paint_delta))
}

fn mesh_segments(mesh: &CertifiedMesh, scale: f64, dx: f64, dy: f64) -> Vec<(Pt, Pt)> {
    mesh.mesh()
        .boundary_polylines
        .iter()
        .flat_map(|boundary| boundary.points.windows(2))
        .map(|pair| {
            (
                Pt::new(pair[0].x * scale + dx, pair[0].y * scale + dy),
                Pt::new(pair[1].x * scale + dx, pair[1].y * scale + dy),
            )
        })
        .filter(|(a, b)| a.is_finite() && b.is_finite() && *a != *b)
        .collect()
}

fn directed_distances(source: &[(Pt, Pt)], target: &[(Pt, Pt)]) -> Vec<f64> {
    if target.is_empty() {
        return Vec::new();
    }
    let spatial_index = SegmentIndex::build(target.to_vec());
    let mut out = Vec::new();
    for &(a, b) in source {
        let length = a.dist(b);
        let pieces = (length / BOUNDARY_SAMPLE_STEP_PX).ceil().max(1.0) as usize;
        for sample_index in 0..=pieces {
            let t = sample_index as f64 / pieces as f64;
            let point = a * (1.0 - t) + b * t;
            let nearest = spatial_index.nearest(point);
            if nearest.is_finite() {
                out.push(nearest);
            }
        }
    }
    out
}

#[derive(Debug, Clone, Copy)]
struct Bounds {
    min: Pt,
    max: Pt,
}

impl Bounds {
    fn of(segments: &[(Pt, Pt)]) -> Self {
        let mut bounds = Self {
            min: Pt::new(f64::INFINITY, f64::INFINITY),
            max: Pt::new(f64::NEG_INFINITY, f64::NEG_INFINITY),
        };
        for &(a, b) in segments {
            for point in [a, b] {
                bounds.min.x = bounds.min.x.min(point.x);
                bounds.min.y = bounds.min.y.min(point.y);
                bounds.max.x = bounds.max.x.max(point.x);
                bounds.max.y = bounds.max.y.max(point.y);
            }
        }
        bounds
    }

    fn distance(self, point: Pt) -> f64 {
        let dx = if point.x < self.min.x {
            self.min.x - point.x
        } else if point.x > self.max.x {
            point.x - self.max.x
        } else {
            0.0
        };
        let dy = if point.y < self.min.y {
            self.min.y - point.y
        } else if point.y > self.max.y {
            point.y - self.max.y
        } else {
            0.0
        };
        dx.hypot(dy)
    }
}

enum SegmentIndex {
    Leaf {
        bounds: Bounds,
        segments: Vec<(Pt, Pt)>,
    },
    Branch {
        bounds: Bounds,
        left: Box<SegmentIndex>,
        right: Box<SegmentIndex>,
    },
}

impl SegmentIndex {
    fn build(mut segments: Vec<(Pt, Pt)>) -> Self {
        let bounds = Bounds::of(&segments);
        if segments.len() <= 8 {
            return Self::Leaf { bounds, segments };
        }
        let split_x = bounds.max.x - bounds.min.x >= bounds.max.y - bounds.min.y;
        segments.sort_by(|left, right| {
            let midpoint = |segment: &(Pt, Pt)| {
                if split_x {
                    segment.0.x + segment.1.x
                } else {
                    segment.0.y + segment.1.y
                }
            };
            midpoint(left).total_cmp(&midpoint(right))
        });
        let right_segments = segments.split_off(segments.len() / 2);
        Self::Branch {
            bounds,
            left: Box::new(Self::build(segments)),
            right: Box::new(Self::build(right_segments)),
        }
    }

    fn bounds(&self) -> Bounds {
        match self {
            Self::Leaf { bounds, .. } | Self::Branch { bounds, .. } => *bounds,
        }
    }

    fn nearest(&self, point: Pt) -> f64 {
        self.nearest_bounded(point, f64::INFINITY)
    }

    fn nearest_bounded(&self, point: Pt, best: f64) -> f64 {
        if self.bounds().distance(point) >= best {
            return best;
        }
        match self {
            Self::Leaf { segments, .. } => segments
                .iter()
                .map(|&(a, b)| point_segment_distance(point, a, b))
                .fold(best, f64::min),
            Self::Branch { left, right, .. } => {
                let (first, second) =
                    if left.bounds().distance(point) <= right.bounds().distance(point) {
                        (left, right)
                    } else {
                        (right, left)
                    };
                let best = first.nearest_bounded(point, best);
                second.nearest_bounded(point, best)
            }
        }
    }
}

fn point_segment_distance(point: Pt, a: Pt, b: Pt) -> f64 {
    let delta = b - a;
    let denominator = delta.dot(delta);
    if denominator <= 0.0 {
        return point.dist(a);
    }
    let t = ((point - a).dot(delta) / denominator).clamp(0.0, 1.0);
    point.dist(a + delta * t)
}

fn quantile(sorted: &[f64], q: f64) -> f64 {
    let index = ((sorted.len() - 1) as f64 * q).ceil() as usize;
    sorted[index.min(sorted.len() - 1)]
}

fn palette_code_delta(truth: &[[f64; 3]], selected: &[[f64; 3]]) -> u8 {
    if truth.len() != selected.len() || truth.is_empty() {
        return u8::MAX;
    }
    truth
        .iter()
        .map(|expected| {
            selected
                .iter()
                .map(|actual| {
                    (0..3)
                        .map(|channel| {
                            linear_to_srgb_u8(expected[channel])
                                .abs_diff(linear_to_srgb_u8(actual[channel]))
                        })
                        .max()
                        .unwrap_or(0)
                })
                .min()
                .unwrap_or(u8::MAX)
        })
        .max()
        .unwrap_or(u8::MAX)
}

fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
        writer
            .write_image_data(rgba)
            .map_err(|error| error.to_string())?;
        writer.finish().map_err(|error| error.to_string())?;
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_and_quantiles_are_not_vacuous() {
        let source = [(Pt::new(0.0, 0.0), Pt::new(2.0, 0.0))];
        let target = [(Pt::new(0.0, 1.0), Pt::new(2.0, 1.0))];
        let mut distances = directed_distances(&source, &target);
        distances.sort_by(f64::total_cmp);
        assert!(distances.len() > 4);
        assert!((quantile(&distances, 0.95) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn boundary_index_is_exactly_the_brute_force_metric() {
        let segments = vec![
            (Pt::new(-2.0, 1.0), Pt::new(4.0, 1.5)),
            (Pt::new(3.0, -4.0), Pt::new(3.0, 8.0)),
            (Pt::new(9.0, 2.0), Pt::new(11.0, 5.0)),
        ];
        let index = SegmentIndex::build(segments.clone());
        for point in [
            Pt::new(0.0, 0.0),
            Pt::new(3.0, 7.0),
            Pt::new(10.0, 4.0),
            Pt::new(-20.0, 30.0),
        ] {
            let brute = segments
                .iter()
                .map(|&(a, b)| point_segment_distance(point, a, b))
                .fold(f64::INFINITY, f64::min);
            assert!((index.nearest(point) - brute).abs() < 1e-12);
        }
    }

    #[test]
    fn smoke_scope_has_one_declared_non_inverse_crime_cell() {
        let cells = MeasurementScope::Smoke.cells();
        assert_eq!(cells.len(), 1);
        assert!(!cells[0].is_inverse_crime());
    }

    fn synthetic_row(group: &str) -> MeasurementRow {
        MeasurementRow {
            group_id: group.into(),
            scene_id: format!("{group}#a"),
            shape_family: "synthetic".into(),
            cell_id: "cell".into(),
            size_px: 128,
            rasterizer: "tiny-skia".into(),
            identifiability: "identifiable".into(),
            core_runtime_ms: 1,
            court_runtime_ms: 1,
            row_elapsed_ms: 2,
            decision_status: "measurement_refused".into(),
            production_provenance: false,
            production_accepted: false,
            candidate_available: false,
            selected_hypothesis_id: None,
            selected_scene_digest_sha256: None,
            selected_delivery_digest_sha256: None,
            selected_artifact_bundle_sha256: None,
            selected_complexity: None,
            internal_baseline: None,
            search_truncated: None,
            explored_mass: None,
            topology_classes_upper_bound: None,
            formation_classes_upper_bound: None,
            top_topology_explored_mass: None,
            top_formation_explored_mass: None,
            selected_delivery_mass: None,
            retained_normalized_mass: None,
            delivery_classes: None,
            top2_class_margin_bits: None,
            posterior_lower_bound: None,
            posterior_bound_status: "absent".into(),
            unexplored_proxy_hypotheses: None,
            candidate_bytes: 0,
            serialized_pixel_bits: None,
            serialized_pixel_bits_per_block: None,
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
            measurement_refusal: Some("synthetic".into()),
        }
    }

    fn synthetic_report(shard: u32, shard_count: u32) -> MeasurementReport {
        let row = synthetic_row(&format!("group-{shard}"));
        MeasurementReport {
            schema: M7_MEASUREMENT_SCHEMA.into(),
            scope: "calibration".into(),
            split: "calibration".into(),
            preset: Preset::Quality,
            procedural_variants_per_family: M7_RELEASE_PROCEDURAL_VARIANTS,
            mandatory_sizes_px: M7_MANDATORY_SIZES.to_vec(),
            rasterizers: vec!["tiny-skia".into()],
            identity: vice_opt::ModelIdentity {
                universe_sha256: "u".into(),
                pricing_sha256: "p".into(),
                backend_sha256: "b".into(),
                config_sha256: "c".into(),
            },
            delivery_policy_sha256: "d".into(),
            included_shards: vec![shard],
            shard_count,
            max_workers_per_shard: 2,
            complete: true,
            expected_renders_included_shards: 1,
            resumed_rows: 0,
            runs: 1,
            rows: vec![row],
            source_groups: 1,
            renders: 1,
            candidates_available: 0,
            truncated_renders: 0,
            elapsed_ms: 2,
            peak_working_set_bytes: 1024,
        }
    }

    #[test]
    fn source_group_shards_are_stable_and_never_multi_assign() {
        for index in 0..100 {
            let id = format!("group/{index:03}");
            let shard = measurement_shard(&id, 7);
            assert!(shard < 7);
            assert_eq!(measurement_shard(&id, 7), shard);
            assert_eq!(
                (0..7)
                    .filter(|candidate| measurement_shard(&id, 7) == *candidate)
                    .count(),
                1
            );
        }
    }

    #[test]
    fn merge_is_complete_only_for_one_copy_of_every_shard() {
        let partial = merge_reports(vec![synthetic_report(1, 2)]).expect("partial merge");
        assert!(!partial.complete);
        let merged = merge_reports(vec![synthetic_report(1, 2), synthetic_report(0, 2)])
            .expect("complete merge");
        assert!(merged.complete);
        assert_eq!(merged.included_shards, vec![0, 1]);
        assert_eq!(merged.renders, 2);
        assert!(merge_reports(vec![synthetic_report(0, 2), synthetic_report(0, 2)]).is_err());
    }
}
