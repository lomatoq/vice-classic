//! M8 multiregion calibration and untouched release court.

use std::collections::BTreeMap;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use vice_core::{
    seal_multiregion_delivery, solve_multiregion_exact, M8DeliveryConfig, M8ExactConfig,
};
use vice_ir::ValidatedScene;

use crate::correlation::ResidualModel;
use crate::gt::degradation::{render_cell, DegradationCell, ResizeChain};
use crate::gt::grammar::procedural_groups_filtered_for_generation;
use crate::gt::raster::{Psf, RasterProfile};
use crate::m7::{judge::judge_scene, BoundaryTail, TopologyComparison};
use crate::reliability::{risk_coverage, RenderOutcome, RiskCoverage};

pub const M8_COURT_SCHEMA: &str = "vice-classic/m8-court/v1";
pub const M8_CALIBRATION_SCHEMA: &str = "vice-classic/m8-calibration/v1";
pub const M8_RELEASE_SCHEMA: &str = "vice-classic/m8-release/v1";
pub const M8_PROCEDURAL_GENERATION: u32 = 6;
pub const M8_VARIANTS_PER_FAMILY: usize = 650;
pub const M8_CLUSTER_SIZE: usize = 5;
pub const M8_CONFIDENCE: f64 = 0.99;
pub const M8_CATASTROPHIC_RISK_TARGET: f64 = 0.01;
pub const M8_MIN_COVERAGE: f64 = 0.80;
pub const M8_CATASTROPHIC_BOUNDARY_MAX_PX: f64 = 4.0;
pub const M8_CATASTROPHIC_PAINT_DELTA_CODES: u8 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum M8CourtScope {
    Smoke,
    Calibration,
    SealedAudit,
}

impl M8CourtScope {
    fn profile(self) -> RasterProfile {
        match self {
            Self::Smoke => RasterProfile::ExactClip,
            Self::Calibration => RasterProfile::Raqote,
            Self::SealedAudit => RasterProfile::TinySkia,
        }
    }

    fn admits_variant(self, variant: usize) -> bool {
        if self == Self::Smoke {
            return variant < 4;
        }
        let calibration_cluster = (variant / M8_CLUSTER_SIZE).is_multiple_of(2);
        match self {
            Self::Calibration => calibration_cluster,
            Self::SealedAudit => !calibration_cluster,
            Self::Smoke => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct M8CourtRow {
    pub group_id: String,
    pub shape_family: String,
    pub cluster_id: String,
    pub cell_id: String,
    pub source_sha256: String,
    pub accepted: bool,
    pub refusal: Option<String>,
    pub runtime_ms: u64,
    pub exact_candidate_id: Option<String>,
    pub exact_total_bits: Option<f64>,
    pub exact_pixel_bits: Option<f64>,
    pub selected_palette_cardinality: Option<u64>,
    pub opaque_modes_seen: Option<u64>,
    pub selection_class: Option<String>,
    pub pixel_bits_per_block: Option<f64>,
    pub exact_candidates_evaluated: u64,
    pub topology: Option<TopologyComparison>,
    pub boundary: Option<BoundaryTail>,
    pub paint_delta_codes: Option<u8>,
    pub profile_max_channel_delta: Option<u8>,
    pub profile_mean_channel_delta: Option<f64>,
    pub internal_max_channel_delta: Option<u8>,
    pub internal_mean_channel_delta: Option<f64>,
    pub intrinsic_catastrophic: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct M8CourtReport {
    pub schema: String,
    pub scope: M8CourtScope,
    pub procedural_generation: u32,
    pub variants_per_family: usize,
    pub cluster_size: usize,
    pub profile: String,
    pub shard_index: u32,
    pub shard_count: u32,
    pub included_shards: Vec<u32>,
    pub model_universe_sha256: String,
    pub exact_config_sha256: String,
    pub corpus_commitment_sha256: String,
    pub source_groups: u64,
    pub accepted_groups: u64,
    pub rows: Vec<M8CourtRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct M8ReleaseGates {
    pub boundary_p95_px: f64,
    pub boundary_p99_px: f64,
    pub boundary_max_px: f64,
    pub paint_delta_codes: u8,
    pub profile_max_channel_delta: u8,
    pub profile_mean_channel_delta: f64,
    pub internal_max_channel_delta: u8,
    pub internal_mean_channel_delta: f64,
    pub runtime_p95_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct M8CalibrationArtifact {
    pub schema: String,
    pub court_sha256: String,
    pub model_universe_sha256: String,
    pub exact_config_sha256: String,
    pub classes: Vec<M8CalibrationClass>,
    pub excluded_unsafe_classes: Vec<M8ExcludedCalibrationClass>,
    pub reliability: RiskCoverage,
    pub gates: Option<M8ReleaseGates>,
    pub gate_met: bool,
    pub refusals: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct M8CalibrationClass {
    pub name: String,
    pub clean_source_groups: u64,
    pub maximum_pixel_bits_per_block: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct M8ExcludedCalibrationClass {
    pub name: String,
    pub clean_source_groups: u64,
    pub catastrophic_source_groups: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct M8ReleaseArtifact {
    pub schema: &'static str,
    pub court_sha256: String,
    pub calibration_sha256: String,
    pub reliability: RiskCoverage,
    pub gate_met: bool,
    pub catastrophic_rows: u64,
    pub refusals: Vec<String>,
}

pub fn measure_court(
    scope: M8CourtScope,
    variants_per_family: usize,
) -> Result<M8CourtReport, String> {
    measure_court_shard(scope, variants_per_family, 0, 1)
}

pub fn measure_court_shard(
    scope: M8CourtScope,
    variants_per_family: usize,
    shard_index: u32,
    shard_count: u32,
) -> Result<M8CourtReport, String> {
    if variants_per_family == 0 || M8_CLUSTER_SIZE == 0 {
        return Err("M8 court population is empty".into());
    }
    if shard_count == 0 || shard_index >= shard_count {
        return Err("M8 shard index/count is malformed".into());
    }
    let groups = procedural_groups_filtered_for_generation(
        variants_per_family,
        M8_PROCEDURAL_GENERATION,
        |id| id.contains("/dot_cluster/") || id.contains("/triple_junction/"),
    );
    let exact_cfg = court_exact_config();
    let delivery_cfg = M8DeliveryConfig::default();
    let mut rows = Vec::new();
    let mut commitment = Sha256::new();
    commitment.update(M8_COURT_SCHEMA.as_bytes());
    commitment.update((variants_per_family as u64).to_le_bytes());
    for group in groups {
        let variant = parse_variant(&group.id)?;
        if !scope.admits_variant(variant) {
            continue;
        }
        if shard_of(&group.id, shard_count) != shard_index {
            continue;
        }
        let scene = group
            .scenes
            .first()
            .ok_or_else(|| format!("{} has no scene", group.id))?;
        let cell = court_cell(scope.profile(), &group.shape_family);
        // Supported-observation boundary, frozen before running the system:
        // each authored visible face must leave at least a four-pixel salient
        // scale in this court. Sub-two-pixel coloured dots have no stable
        // opaque interior paint evidence and belong to typed abstention, not
        // to the M8 success denominator.
        let visible_scale_px = scene.min_salient_scale_px() * f64::from(cell.size_px)
            / f64::from(crate::gt::grammar::AUTHORING_CANVAS_PX);
        if scene.partition_truth().visible_faces < 3 || visible_scale_px < 5.0 {
            continue;
        }
        commitment.update(group.id.as_bytes());
        commitment.update(
            vice_ir::canonical_scene_bytes(scene.scene().scene()).map_err(|e| e.to_string())?,
        );
        let rendered = render_cell(scene, &cell, 1)?;
        let png = encode_png(rendered.width_px, rendered.height_px, &rendered.rgba8)?;
        let source_sha256 = hex::encode(Sha256::digest(&png));
        let started = Instant::now();
        let result = (|| {
            let solved = solve_multiregion_exact(&png, &exact_cfg).map_err(|e| e.to_string())?;
            let delivery = seal_multiregion_delivery(&png, &solved, &exact_cfg, &delivery_cfg)
                .map_err(|e| e.to_string())?;
            let selected = ValidatedScene::new(solved.scene.clone()).map_err(|e| e.to_string())?;
            let (topology, boundary, paint_delta_codes) = judge_scene(scene, &cell, &selected)?;
            Ok::<_, String>((solved, delivery, topology, boundary, paint_delta_codes))
        })();
        let runtime_ms = started.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
        let cluster_id = format!(
            "{}/cluster-{:03}",
            group.shape_family,
            variant / M8_CLUSTER_SIZE
        );
        match result {
            Ok((solved, delivery, topology, boundary, paint_delta_codes)) => {
                let seal = &delivery.report.seal;
                let intrinsic_catastrophic = !topology.exact
                    || boundary.max_px > M8_CATASTROPHIC_BOUNDARY_MAX_PX
                    || paint_delta_codes > M8_CATASTROPHIC_PAINT_DELTA_CODES;
                rows.push(M8CourtRow {
                    group_id: group.id,
                    shape_family: group.shape_family,
                    cluster_id,
                    cell_id: rendered.cell_id,
                    source_sha256,
                    accepted: true,
                    refusal: None,
                    runtime_ms,
                    exact_candidate_id: Some(solved.report.selected.id.clone()),
                    exact_total_bits: Some(solved.report.selected.exact_total_bits),
                    exact_pixel_bits: Some(solved.report.selected.exact_pixel_bits),
                    selected_palette_cardinality: Some(solved.report.selected.palette_cardinality),
                    opaque_modes_seen: Some(solved.report.selected.opaque_modes_seen),
                    selection_class: Some(solved.report.selected.selection_class.clone()),
                    pixel_bits_per_block: Some(solved.report.selected.pixel_bits_per_block),
                    exact_candidates_evaluated: solved.report.exact_candidates_evaluated,
                    topology: Some(topology),
                    boundary: Some(boundary),
                    paint_delta_codes: Some(paint_delta_codes),
                    profile_max_channel_delta: Some(seal.profile_comparison.max_channel_delta),
                    profile_mean_channel_delta: Some(seal.profile_comparison.mean_channel_delta),
                    internal_max_channel_delta: Some(
                        seal.internal_to_pure_comparison
                            .max_channel_delta
                            .max(seal.internal_to_seam_comparison.max_channel_delta),
                    ),
                    internal_mean_channel_delta: Some(
                        seal.internal_to_pure_comparison
                            .mean_channel_delta
                            .max(seal.internal_to_seam_comparison.mean_channel_delta),
                    ),
                    intrinsic_catastrophic,
                });
            }
            Err(refusal) => rows.push(M8CourtRow {
                group_id: group.id,
                shape_family: group.shape_family,
                cluster_id,
                cell_id: rendered.cell_id,
                source_sha256,
                accepted: false,
                refusal: Some(refusal),
                runtime_ms,
                exact_candidate_id: None,
                exact_total_bits: None,
                exact_pixel_bits: None,
                selected_palette_cardinality: None,
                opaque_modes_seen: None,
                selection_class: None,
                pixel_bits_per_block: None,
                exact_candidates_evaluated: 0,
                topology: None,
                boundary: None,
                paint_delta_codes: None,
                profile_max_channel_delta: None,
                profile_mean_channel_delta: None,
                internal_max_channel_delta: None,
                internal_mean_channel_delta: None,
                intrinsic_catastrophic: false,
            }),
        }
    }
    rows.sort_by(|a, b| a.group_id.cmp(&b.group_id));
    let source_groups = rows.len() as u64;
    let accepted_groups = rows.iter().filter(|row| row.accepted).count() as u64;
    let exact_config_sha256 = rows
        .iter()
        .find_map(|row| row.accepted.then_some(()))
        .map_or_else(|| "0".repeat(64), |_| exact_config_digest(&exact_cfg));
    Ok(M8CourtReport {
        schema: M8_COURT_SCHEMA.into(),
        scope,
        procedural_generation: M8_PROCEDURAL_GENERATION,
        variants_per_family,
        cluster_size: M8_CLUSTER_SIZE,
        profile: scope.profile().as_str().into(),
        shard_index,
        shard_count,
        included_shards: vec![shard_index],
        model_universe_sha256: vice_opt::model_universe_hash(
            &vice_opt::SupportedModelUniverseV1::m8(),
        ),
        exact_config_sha256,
        corpus_commitment_sha256: hex::encode(commitment.finalize()),
        source_groups,
        accepted_groups,
        rows,
    })
}

pub fn merge_courts(mut reports: Vec<M8CourtReport>) -> Result<M8CourtReport, String> {
    if reports.is_empty() {
        return Err("M8 merge has no inputs".into());
    }
    reports.sort_by_key(|report| report.shard_index);
    let first = reports.first().expect("nonempty").clone();
    let shard_count = first.shard_count;
    if reports.len() != shard_count as usize
        || reports.iter().enumerate().any(|(index, report)| {
            report.shard_index != index as u32
                || report.shard_count != shard_count
                || report.scope != first.scope
                || report.procedural_generation != first.procedural_generation
                || report.variants_per_family != first.variants_per_family
                || report.cluster_size != first.cluster_size
                || report.profile != first.profile
                || report.model_universe_sha256 != first.model_universe_sha256
                || report.exact_config_sha256 != first.exact_config_sha256
        })
    {
        return Err("M8 shards are incomplete or identity-incompatible".into());
    }
    let mut rows = reports
        .iter_mut()
        .flat_map(|report| std::mem::take(&mut report.rows))
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| a.group_id.cmp(&b.group_id));
    if rows
        .windows(2)
        .any(|pair| pair[0].group_id == pair[1].group_id)
    {
        return Err("M8 shards contain duplicate source groups".into());
    }
    let mut commitment = Sha256::new();
    commitment.update(b"vice-classic/m8-merged-court/v1");
    for report in &reports {
        commitment.update(report.corpus_commitment_sha256.as_bytes());
    }
    let source_groups = rows.len() as u64;
    let accepted_groups = rows.iter().filter(|row| row.accepted).count() as u64;
    Ok(M8CourtReport {
        schema: first.schema.clone(),
        scope: first.scope,
        procedural_generation: first.procedural_generation,
        variants_per_family: first.variants_per_family,
        cluster_size: first.cluster_size,
        profile: first.profile.clone(),
        shard_index: 0,
        shard_count,
        included_shards: (0..shard_count).collect(),
        model_universe_sha256: first.model_universe_sha256.clone(),
        exact_config_sha256: first.exact_config_sha256.clone(),
        corpus_commitment_sha256: hex::encode(commitment.finalize()),
        source_groups,
        accepted_groups,
        rows,
    })
}

pub fn calibrate(report: &M8CourtReport) -> M8CalibrationArtifact {
    let mut refusals = Vec::new();
    if report.scope != M8CourtScope::Calibration {
        refusals.push("calibration requires a calibration-split court".into());
    }
    let mut by_class = BTreeMap::<String, (Vec<f64>, u64)>::new();
    for row in report.rows.iter().filter(|row| row.accepted) {
        if let (Some(class), Some(bits)) = (&row.selection_class, row.pixel_bits_per_block) {
            let entry = by_class.entry(class.clone()).or_default();
            if row.intrinsic_catastrophic {
                entry.1 += 1;
            } else {
                entry.0.push(bits);
            }
        }
    }
    // Admission is observable at release time only at selection-class
    // granularity. If calibration observes even one catastrophe in a class,
    // no pixel-score threshold may relabel overlapping members as safe: the
    // complete class fails closed. This prevents outcome-assisted slicing of
    // an empirically unsafe class.
    let excluded_unsafe_classes = by_class
        .iter()
        .filter(|(_, (_, catastrophic))| *catastrophic > 0)
        .map(|(name, (clean, catastrophic))| M8ExcludedCalibrationClass {
            name: name.clone(),
            clean_source_groups: clean.len() as u64,
            catastrophic_source_groups: *catastrophic,
        })
        .collect::<Vec<_>>();
    let classes = by_class
        .into_iter()
        .filter(|(_, (values, catastrophic))| values.len() >= 2 && *catastrophic == 0)
        .map(|(name, (values, _))| M8CalibrationClass {
            name,
            clean_source_groups: values.len() as u64,
            maximum_pixel_bits_per_block: values.into_iter().fold(0.0, f64::max),
        })
        .collect::<Vec<_>>();
    let admitted = |row: &M8CourtRow| admitted_by_class(row, &classes);
    let reliability = reliability_for(report, admitted, |row| row.intrinsic_catastrophic);
    if !reliability.contract_met {
        refusals.push("clustered 99% catastrophic-risk contract is not met".into());
    }
    if reliability.coverage_per_source < M8_MIN_COVERAGE {
        refusals.push("source coverage is below 80%".into());
    }
    let accepted = report
        .rows
        .iter()
        .filter(|row| row.accepted && admitted(row) && !row.intrinsic_catastrophic)
        .collect::<Vec<_>>();
    let gates = (!accepted.is_empty()).then(|| M8ReleaseGates {
        // These fields gate each accepted row, so they must be calibrated
        // from row maxima. Using a 95th percentile as a zero-violation row
        // ceiling is internally contradictory. Outward rounding gives a
        // deterministic backend-stability envelope without consulting the
        // held-out court.
        boundary_p95_px: outward_max_f64(
            &accepted
                .iter()
                .filter_map(|row| row.boundary.as_ref().map(|value| value.p95_px))
                .collect::<Vec<_>>(),
            0.25,
        ),
        boundary_p99_px: outward_max_f64(
            &accepted
                .iter()
                .filter_map(|row| row.boundary.as_ref().map(|value| value.p99_px))
                .collect::<Vec<_>>(),
            0.25,
        ),
        boundary_max_px: outward_max_f64(
            &accepted
                .iter()
                .filter_map(|row| row.boundary.as_ref().map(|value| value.max_px))
                .collect::<Vec<_>>(),
            0.25,
        ),
        paint_delta_codes: accepted
            .iter()
            .filter_map(|row| row.paint_delta_codes)
            .max()
            .unwrap_or(0),
        profile_max_channel_delta: accepted
            .iter()
            .filter_map(|row| row.profile_max_channel_delta)
            .max()
            .unwrap_or(0),
        profile_mean_channel_delta: outward_max_f64(
            &accepted
                .iter()
                .filter_map(|row| row.profile_mean_channel_delta)
                .collect::<Vec<_>>(),
            1.0 / 65_536.0,
        ),
        internal_max_channel_delta: outward_max_u8(
            &accepted
                .iter()
                .filter_map(|row| row.internal_max_channel_delta)
                .collect::<Vec<_>>(),
            8,
        ),
        internal_mean_channel_delta: outward_max_f64(
            &accepted
                .iter()
                .filter_map(|row| row.internal_mean_channel_delta)
                .collect::<Vec<_>>(),
            1.0 / 256.0,
        ),
        runtime_p95_ms: quantile_u64(
            &accepted
                .iter()
                .map(|row| row.runtime_ms)
                .collect::<Vec<_>>(),
            0.95,
        ),
    });
    if gates.is_none() {
        refusals.push("calibration produced no accepted rows".into());
    }
    M8CalibrationArtifact {
        schema: M8_CALIBRATION_SCHEMA.into(),
        court_sha256: digest_json(report),
        model_universe_sha256: report.model_universe_sha256.clone(),
        exact_config_sha256: report.exact_config_sha256.clone(),
        classes,
        excluded_unsafe_classes,
        reliability,
        gates,
        gate_met: refusals.is_empty(),
        refusals,
    }
}

pub fn release(report: &M8CourtReport, calibration: &M8CalibrationArtifact) -> M8ReleaseArtifact {
    let mut refusals = Vec::new();
    if report.scope != M8CourtScope::SealedAudit {
        refusals.push("release requires the untouched sealed-audit court".into());
    }
    if !calibration.gate_met {
        refusals.push("calibration is not green".into());
    }
    if report.model_universe_sha256 != calibration.model_universe_sha256
        || report.exact_config_sha256 != calibration.exact_config_sha256
    {
        refusals.push("sealed court identity differs from calibration".into());
    }
    let gated_bad = |row: &M8CourtRow| {
        let Some(gates) = &calibration.gates else {
            return true;
        };
        row.intrinsic_catastrophic
            || row.boundary.as_ref().is_none_or(|boundary| {
                boundary.p95_px > gates.boundary_p95_px
                    || boundary.p99_px > gates.boundary_p99_px
                    || boundary.max_px > gates.boundary_max_px
            })
            || row
                .paint_delta_codes
                .is_none_or(|value| value > gates.paint_delta_codes)
            || row
                .profile_max_channel_delta
                .is_none_or(|value| value > gates.profile_max_channel_delta)
            || row
                .profile_mean_channel_delta
                .is_none_or(|value| value > gates.profile_mean_channel_delta)
            || row
                .internal_max_channel_delta
                .is_none_or(|value| value > gates.internal_max_channel_delta)
            || row
                .internal_mean_channel_delta
                .is_none_or(|value| value > gates.internal_mean_channel_delta)
    };
    let admitted = |row: &M8CourtRow| admitted_by_class(row, &calibration.classes);
    let reliability = reliability_for(report, admitted, gated_bad);
    if !reliability.contract_met {
        refusals.push("sealed clustered catastrophic-risk contract is not met".into());
    }
    if reliability.coverage_per_source < M8_MIN_COVERAGE {
        refusals.push("sealed source coverage is below 80%".into());
    }
    let catastrophic_rows = report
        .rows
        .iter()
        .filter(|row| row.accepted && admitted(row) && gated_bad(row))
        .count() as u64;
    if catastrophic_rows != 0 {
        refusals.push("one or more accepted sealed rows violate frozen gates".into());
    }
    M8ReleaseArtifact {
        schema: M8_RELEASE_SCHEMA,
        court_sha256: digest_json(report),
        calibration_sha256: digest_json(calibration),
        reliability,
        gate_met: refusals.is_empty(),
        catastrophic_rows,
        refusals,
    }
}

fn reliability_for(
    report: &M8CourtReport,
    admitted: impl Fn(&M8CourtRow) -> bool,
    catastrophic: impl Fn(&M8CourtRow) -> bool,
) -> RiskCoverage {
    let profile = RasterProfile::from_id(&report.profile).expect("court owns a valid profile");
    let outcomes = report
        .rows
        .iter()
        .map(|row| {
            let accepted = row.accepted && admitted(row);
            RenderOutcome {
                group_id: row.group_id.clone(),
                cell_id: row.cell_id.clone(),
                profile,
                accepted,
                catastrophic: accepted && catastrophic(row),
                mandatory: true,
            }
        })
        .collect::<Vec<_>>();
    risk_coverage(
        "m8_multiregion",
        &outcomes,
        M8_CONFIDENCE,
        M8_CATASTROPHIC_RISK_TARGET,
        Some((ResidualModel::Block, true)),
    )
}

fn admitted_by_class(row: &M8CourtRow, classes: &[M8CalibrationClass]) -> bool {
    let Some(class) = &row.selection_class else {
        return false;
    };
    classes.iter().any(|calibration| calibration.name == *class)
}

fn court_exact_config() -> M8ExactConfig {
    let mut cfg = M8ExactConfig::default();
    cfg.alternation.max_rounds = 2;
    cfg.max_vertex_trials_per_round = 8;
    cfg
}

fn court_cell(profile: RasterProfile, family: &str) -> DegradationCell {
    DegradationCell {
        size_px: if family == "dot_cluster" { 256 } else { 128 },
        subpixel_dx: 0.25,
        subpixel_dy: 0.375,
        profile,
        psf: Psf::Box,
        blend: vice_ir::BlendSpace::LinearLight,
        resize: ResizeChain::None,
        contrast: 1.0,
    }
}

fn parse_variant(id: &str) -> Result<usize, String> {
    id.rsplit('/')
        .next()
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| format!("M8 procedural id has no variant: {id}"))
}

fn shard_of(id: &str, shard_count: u32) -> u32 {
    let digest = Sha256::digest(id.as_bytes());
    let value = u64::from_le_bytes(digest[..8].try_into().expect("eight digest bytes"));
    (value % u64::from(shard_count)) as u32
}

fn encode_png(width: u32, height: u32, rgba8: &[u8]) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(rgba8).map_err(|e| e.to_string())?;
    }
    Ok(bytes)
}

#[derive(Serialize)]
struct ExactConfigIdentity {
    likelihood: vice_opt::BlockLikelihoodConfig,
    alternation: vice_opt::AlternationConfig,
    vertex_step_px: f64,
    max_vertex_trials_per_round: usize,
}

fn exact_config_digest(cfg: &M8ExactConfig) -> String {
    digest_json(&ExactConfigIdentity {
        likelihood: cfg.likelihood,
        alternation: cfg.alternation,
        vertex_step_px: cfg.vertex_step_px,
        max_vertex_trials_per_round: cfg.max_vertex_trials_per_round,
    })
}

fn digest_json(value: &impl Serialize) -> String {
    hex::encode(Sha256::digest(
        serde_json::to_vec(value).expect("M8 court artifact serializes"),
    ))
}

fn outward_max_f64(values: &[f64], quantum: f64) -> f64 {
    let maximum = values.iter().copied().fold(0.0, f64::max);
    (maximum / quantum).ceil() * quantum
}

fn outward_max_u8(values: &[u8], quantum: u8) -> u8 {
    let maximum = values.iter().copied().max().unwrap_or(0);
    maximum
        .saturating_add(quantum.saturating_sub(1))
        .checked_div(quantum)
        .unwrap_or(0)
        .saturating_mul(quantum)
}

fn quantile_u64(values: &[u64], probability: f64) -> u64 {
    if values.is_empty() {
        return u64::MAX;
    }
    let mut values = values.to_vec();
    values.sort_unstable();
    values[((values.len() - 1) as f64 * probability).ceil() as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_court_runs_both_multicolor_families_through_exact_delivery() {
        let report = measure_court(M8CourtScope::Smoke, 4).unwrap();
        assert!(report.source_groups >= 3);
        assert!(report
            .rows
            .iter()
            .any(|row| row.shape_family == "dot_cluster"));
        assert!(report
            .rows
            .iter()
            .any(|row| row.shape_family == "triple_junction"));
        assert_eq!(report.model_universe_sha256.len(), 64);
        for family in ["dot_cluster", "triple_junction"] {
            assert!(report
                .rows
                .iter()
                .any(|row| row.shape_family == family && row.accepted));
        }
        assert!(report.rows.iter().filter(|row| !row.accepted).all(|row| {
            row.refusal
                .as_deref()
                .is_some_and(|reason| reason.contains("refused"))
        }));
    }

    #[test]
    fn clustered_split_never_places_one_cluster_in_both_courts() {
        for variant in 0..100 {
            assert_ne!(
                M8CourtScope::Calibration.admits_variant(variant),
                M8CourtScope::SealedAudit.admits_variant(variant)
            );
        }
    }
}
