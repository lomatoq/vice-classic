//! §27.6 / §28 M6 geometry-oracle decomposition.
//!
//! The harness binds every observation to `(scene id, BoundaryId)` before
//! Stage G. That binding is the missing chain identity that made the original
//! M6 report say “0 of 5 arms producible”. It uses the development split only,
//! one scene per independent source group, and never renders or opens the
//! sealed audit.
//! Arms are distinct interventions over one common compatibility key:
//!
//! - G00: automatic candidates, automatic physical-bit selector;
//! - G10: inject a GT-equivalent family/breakpoint candidate, automatic selector;
//! - G01: automatic candidates, oracle geometry-error selector;
//! - G11: GT-equivalent candidates, oracle selector;
//! - G20: force GT-equivalent families/breakpoints, automatic parameter fit
//!   and automatic physical-bit selector.
//!
//! “GT-equivalent” does not inject parameters. [`vice_fit::fit_forced_boundary_models`]
//! re-fits every forced span to the observation and runs the production joint
//! solver. Exact GT parameters are the G30 reference and are used only as the
//! error target (zero by construction), not as an M6 candidate.

use std::collections::BTreeMap;

use serde::Serialize;
use vice_evidence::{BoundaryChain, BoundarySample};
use vice_fit::{
    fit_forced_boundary_models, k_best_boundary_models, BoundaryModel, SpanFamily, FIT_BUDGET_V1,
    K_DISCRETE_PATHS,
};
use vice_geom::{ChordTolerancePx, Pt};
use vice_ir::Segment;

use crate::gates::GatesFile;
use crate::gt::corpus::Platform;
use crate::hashing::sha256_hex;
use crate::oracle::key::{CandidateBudget, CompatibilityKey};
use crate::universe::{model_universe_hash, SupportedModelUniverseV1};

pub const GEOMETRY_M6_SCHEMA: &str = "vice-classic/m6-geometry-oracle/v1";
const INTERVENTION_SCHEMA: &str = "vice-classic/m6-geometry-interventions/v1";
const BACKEND_ID: &str = "vice-fit/stage-g-h/v1";
const SAMPLE_STEP_PX: f64 = 1.0;
const CORRIDOR_HALFWIDTH_PX: f64 = 0.35;
const CORRELATION_LENGTH_PX: f64 = 1.0;
const TRUTH_CHORD_TOLERANCE_PX: f64 = 0.02;
const ARM_IDS: [&str; 5] = ["G00", "G10", "G01", "G11", "G20"];

/// Frozen population floors, read from the 205-boundary development run.
/// Kept below the measured counts except for the arm set, where “all five”
/// is the property and losing one arm is exactly the regression.
pub const GATE_MIN_GEOMETRY_BOUNDARIES: usize = 100;
pub const GATE_MIN_GEOMETRY_ARMS_PER_BOUNDARY: usize = 5;
pub const GATE_MIN_ORACLE_CANDIDATE_INJECTIONS: usize = 100;
pub const GATE_MIN_ORACLE_SELECTOR_CHANGES: usize = 1;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GeometryOracleConfig {
    pub population_split: &'static str,
    pub scenes_per_source_group: usize,
    pub formation: &'static str,
    pub canvas_dim_px: f64,
    pub sample_step_px: f64,
    pub corridor_halfwidth_px: f64,
    pub correlation_length_px: f64,
    pub truth_chord_tolerance_px: f64,
    pub candidate_budget: usize,
    pub k_discrete_paths: usize,
    /// Bind the key to the exact grammar and every load-bearing selector price;
    /// otherwise a model-version change can retain the old fingerprint.
    pub model_universe_hash: String,
    pub geometry_pricing_sha256: String,
}

impl Default for GeometryOracleConfig {
    fn default() -> Self {
        GeometryOracleConfig {
            population_split: "development",
            scenes_per_source_group: 1,
            formation: "ground_truth_partition_and_boundary_formation",
            canvas_dim_px: 256.0,
            sample_step_px: SAMPLE_STEP_PX,
            corridor_halfwidth_px: CORRIDOR_HALFWIDTH_PX,
            correlation_length_px: CORRELATION_LENGTH_PX,
            truth_chord_tolerance_px: TRUTH_CHORD_TOLERANCE_PX,
            candidate_budget: FIT_BUDGET_V1.cap(),
            k_discrete_paths: K_DISCRETE_PATHS,
            model_universe_hash: model_universe_hash(&SupportedModelUniverseV1::v1()),
            geometry_pricing_sha256: sha256_hex(vice_fit::pricing_surface_v1().as_bytes()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GeometryError {
    pub symmetric_max_px: f64,
    pub symmetric_mean_px: f64,
    pub truth_to_model_max_px: f64,
    pub model_to_truth_max_px: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GeometryArmResult {
    pub arm: &'static str,
    pub compatibility_key_fingerprint: String,
    pub candidate_models: usize,
    pub selected_source: &'static str,
    pub families: Vec<&'static str>,
    pub breakpoints: Vec<usize>,
    pub smooth: Vec<bool>,
    pub code_bits: f64,
    pub proposal_cost_px: f64,
    pub error: GeometryError,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GeometryBoundaryRow {
    pub fixture_id: String,
    pub scene_id: String,
    pub boundary_id: usize,
    pub samples: usize,
    pub gt_families: Vec<&'static str>,
    pub gt_breakpoints: Vec<usize>,
    pub injected_models: usize,
    pub oracle_selector_changed: bool,
    pub arms: Vec<GeometryArmResult>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GeometryExclusion {
    pub fixture_id: String,
    pub stage: &'static str,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GeometryArmAggregate {
    pub arm: &'static str,
    pub boundaries: usize,
    pub mean_symmetric_max_px: f64,
    pub worst_symmetric_max_px: f64,
    pub selected_auto: usize,
    pub selected_forced: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GeometryMeasurements {
    pub schema: &'static str,
    pub milestone: &'static str,
    pub platform: Platform,
    pub config: GeometryOracleConfig,
    pub config_hash: String,
    pub fixture_set_hash: String,
    pub compatibility_key: CompatibilityKey,
    pub source_groups: usize,
    pub scenes: usize,
    pub boundaries_attempted: usize,
    pub boundaries_measured: usize,
    pub exact_gt_reference_max_px: f64,
    pub oracle_candidate_injections: usize,
    pub oracle_selector_changes: usize,
    pub exclusions: Vec<GeometryExclusion>,
    pub aggregates: Vec<GeometryArmAggregate>,
    pub rows: Vec<GeometryBoundaryRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GeometryGateRow {
    pub clause: &'static str,
    pub met: bool,
    pub measured: String,
    pub required: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GeometryGateTable {
    pub met: bool,
    pub rows: Vec<GeometryGateRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GeometryOracleReport {
    pub measurements: GeometryMeasurements,
    pub gate: GeometryGateTable,
}

#[derive(Clone)]
struct BoundObservation {
    fixture_id: String,
    scene_id: String,
    boundary_id: usize,
    chain: BoundaryChain,
    truth: Vec<Pt>,
    gt_families: Vec<SpanFamily>,
    gt_breakpoints: Vec<usize>,
}

/// The three population floors consumed by the geometry-oracle row. The other
/// M6 values are cross-checked against `vice-fit::gate` in
/// `gates::frozen_claims`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeometryGateConfig {
    pub min_boundaries: usize,
    pub min_arms_per_boundary: usize,
    pub min_candidate_injections: usize,
    pub min_selector_changes: usize,
}

impl GeometryGateConfig {
    pub fn from_gates(gates: &GatesFile) -> Result<GeometryGateConfig, String> {
        let read = |key: &str| -> Result<usize, String> {
            let value = gates
                .gate_value("m6_geometry", key)
                .map_err(|e| e.to_string())?;
            value
                .as_integer()
                .and_then(|v| usize::try_from(v).ok())
                .ok_or_else(|| format!("[m6_geometry].{key} is not a non-negative integer"))
        };
        Ok(GeometryGateConfig {
            min_boundaries: read("gate_min_geometry_boundaries")?,
            min_arms_per_boundary: read("gate_min_geometry_arms_per_boundary")?,
            min_candidate_injections: read("gate_min_oracle_candidate_injections")?,
            min_selector_changes: read("gate_min_oracle_selector_changes")?,
        })
    }
}

pub fn measure(gates: &GatesFile) -> Result<GeometryOracleReport, String> {
    let measurements = measure_raw()?;
    let gate = evaluate_gate(&measurements, GeometryGateConfig::from_gates(gates)?);
    Ok(GeometryOracleReport { measurements, gate })
}

/// Run the decomposition without asking a placeholder for thresholds. This is
/// the §27.7 calibration leg: it publishes the population from which the final
/// gate-file-only freeze is read.
pub fn measure_raw() -> Result<GeometryMeasurements, String> {
    let config = GeometryOracleConfig::default();
    let config_json = serde_json::to_vec(&config).map_err(|e| e.to_string())?;
    let config_hash = sha256_hex(&config_json);
    let groups = crate::corridor::frozen_calibration_groups()?;
    let source_groups = groups.len();
    let mut scenes = 0usize;
    let mut attempted = 0usize;
    let mut exclusions = Vec::new();
    let mut rows = Vec::new();

    for group in &groups {
        let Some(scene) = group.scenes.first() else {
            continue;
        };
        scenes += 1;
        let graph = scene.scene().graph();
        for boundary_id in 0..graph.boundaries.len() {
            attempted += 1;
            let fixture_id = format!("{}/boundary:{boundary_id}", scene.id());
            let observation = match bind_observation(scene, boundary_id, &config) {
                Ok(observation) => observation,
                Err(reason) => {
                    exclusions.push(GeometryExclusion {
                        fixture_id,
                        stage: "bind_ground_truth",
                        reason,
                    });
                    continue;
                }
            };
            match measure_boundary(&observation, &config) {
                Ok(row) => rows.push(row),
                Err(reason) => exclusions.push(GeometryExclusion {
                    fixture_id: observation.fixture_id,
                    stage: "five_arm_common_population",
                    reason,
                }),
            }
        }
    }
    rows.sort_by(|a, b| a.fixture_id.cmp(&b.fixture_id));
    exclusions.sort_by(|a, b| a.fixture_id.cmp(&b.fixture_id).then(a.stage.cmp(b.stage)));

    let fixture_ids: Vec<&str> = rows.iter().map(|row| row.fixture_id.as_str()).collect();
    let fixture_set_hash = sha256_hex(fixture_ids.join("\u{1f}").as_bytes());
    let key = CompatibilityKey {
        backend_id: BACKEND_ID.to_string(),
        config_hash: config_hash.clone(),
        candidate_budget: CandidateBudget::Candidates {
            max: config.candidate_budget as u64,
        },
        fixture_hash: fixture_set_hash.clone(),
        intervention_schema_version: INTERVENTION_SCHEMA.to_string(),
    };
    let fingerprint = key.fingerprint();
    for row in &mut rows {
        for arm in &mut row.arms {
            arm.compatibility_key_fingerprint = fingerprint.clone();
        }
    }
    let aggregates = aggregate(&rows);
    let oracle_candidate_injections = rows.iter().map(|row| row.injected_models).sum();
    let oracle_selector_changes = rows
        .iter()
        .filter(|row| row.oracle_selector_changed)
        .count();

    Ok(GeometryMeasurements {
        schema: GEOMETRY_M6_SCHEMA,
        milestone: "M6",
        platform: Platform::current(),
        config,
        config_hash,
        fixture_set_hash,
        compatibility_key: key,
        source_groups,
        scenes,
        boundaries_attempted: attempted,
        boundaries_measured: rows.len(),
        exact_gt_reference_max_px: 0.0,
        oracle_candidate_injections,
        oracle_selector_changes,
        exclusions,
        aggregates,
        rows,
    })
}

pub fn evaluate_gate(run: &GeometryMeasurements, gates: GeometryGateConfig) -> GeometryGateTable {
    let population_met = run.boundaries_measured >= gates.min_boundaries;
    let arms_found = run.rows.iter().map(|row| row.arms.len()).min().unwrap_or(0);
    let arms_met = arms_found >= gates.min_arms_per_boundary
        && run.rows.iter().all(|row| {
            row.arms
                .iter()
                .map(|arm| arm.arm)
                .eq(ARM_IDS.iter().copied())
        });
    let fingerprint = run.compatibility_key.fingerprint();
    let compatible = run.rows.iter().all(|row| {
        row.arms
            .iter()
            .all(|arm| arm.compatibility_key_fingerprint == fingerprint)
    });
    let injections_met = run.oracle_candidate_injections >= gates.min_candidate_injections;
    let selector_met = run.oracle_selector_changes >= gates.min_selector_changes;
    let rows = vec![
        GeometryGateRow {
            clause: "common_geometry_population",
            met: population_met,
            measured: run.boundaries_measured.to_string(),
            required: format!(">= {}", gates.min_boundaries),
        },
        GeometryGateRow {
            clause: "G00_G10_G01_G11_G20_all_measured",
            met: arms_met,
            measured: format!("{arms_found} arms on every common boundary"),
            required: format!(
                ">= {} and exact declared arm set",
                gates.min_arms_per_boundary
            ),
        },
        GeometryGateRow {
            clause: "no_subtraction_across_incompatible_arms",
            met: compatible && run.boundaries_measured > 0,
            measured: format!(
                "{} arm measurements share key {}",
                run.boundaries_measured * ARM_IDS.len(),
                fingerprint
            ),
            required: "one identical five-component §27.6 key".to_string(),
        },
        GeometryGateRow {
            clause: "oracle_candidate_injection_is_exercised",
            met: injections_met,
            measured: run.oracle_candidate_injections.to_string(),
            required: format!(">= {}", gates.min_candidate_injections),
        },
        GeometryGateRow {
            clause: "oracle_selector_is_load_bearing",
            met: selector_met,
            measured: run.oracle_selector_changes.to_string(),
            required: format!(">= {}", gates.min_selector_changes),
        },
    ];
    GeometryGateTable {
        met: rows.iter().all(|row| row.met),
        rows,
    }
}

fn bind_observation(
    scene: &crate::gt::GtScene,
    boundary_id: usize,
    config: &GeometryOracleConfig,
) -> Result<BoundObservation, String> {
    let graph = scene.scene().graph();
    let boundary = graph
        .boundaries
        .get(boundary_id)
        .ok_or_else(|| format!("boundary {boundary_id} is absent"))?;
    let start = graph.vertices[boundary.start_vertex.index()].pos;
    let end = graph.vertices[boundary.end_vertex.index()].pos;
    let nodes = boundary.curve.node_positions(start, end);
    if nodes.len() != boundary.curve.segments.len() + 1 {
        return Err("curve node/segment shape mismatch".to_string());
    }

    let tolerance = ChordTolerancePx::new(config.truth_chord_tolerance_px).ok_or_else(|| {
        format!(
            "invalid truth tolerance {}",
            config.truth_chord_tolerance_px
        )
    })?;
    let mut sample_points = Vec::new();
    let mut truth = Vec::new();
    let mut gt_families = Vec::new();
    let mut gt_breakpoints = Vec::new();

    for (segment_index, segment) in boundary.curve.segments.iter().enumerate() {
        let family = match segment {
            Segment::Line => SpanFamily::Line,
            Segment::CircularArc { .. } => SpanFamily::CircularArc,
            Segment::Quad { .. } => SpanFamily::Quad,
            Segment::Cubic { .. } => SpanFamily::Cubic,
            Segment::EllipticArc { .. } => {
                return Err(format!(
                    "segment {segment_index} is elliptic_arc; §14.2 requires targeted evidence \
                     and vice-fit deliberately has no speculative ellipse family"
                ))
            }
        };
        let poly = flatten_truth_segment(
            segment,
            nodes[segment_index],
            nodes[segment_index + 1],
            tolerance,
        )?;
        let sampled = resample_polyline(&poly, config.sample_step_px, 3)?;
        if segment_index == 0 {
            truth.extend(poly.iter().copied());
            sample_points.extend(sampled.iter().copied());
        } else {
            truth.extend(poly.iter().copied().skip(1));
            sample_points.extend(sampled.iter().copied().skip(1));
        }
        gt_families.push(family);
        if segment_index + 1 < boundary.curve.segments.len() {
            gt_breakpoints.push(sample_points.len() - 1);
        }
    }
    if sample_points.len() < vice_fit::MIN_SUPPORT_SAMPLES {
        return Err(format!(
            "only {} physical samples, fewer than {}",
            sample_points.len(),
            vice_fit::MIN_SUPPORT_SAMPLES
        ));
    }
    let chain = boundary_chain(&sample_points, config);
    Ok(BoundObservation {
        fixture_id: format!("{}/boundary:{boundary_id}", scene.id()),
        scene_id: scene.id().to_string(),
        boundary_id,
        chain,
        truth,
        gt_families,
        gt_breakpoints,
    })
}

fn flatten_truth_segment(
    segment: &Segment,
    p0: Pt,
    p1: Pt,
    tolerance: ChordTolerancePx,
) -> Result<Vec<Pt>, String> {
    let points = match *segment {
        Segment::Line => vec![p0, p1],
        Segment::Quad { ctrl } => vice_geom::flatten::flatten_quad(p0, ctrl, p1, tolerance).points,
        Segment::Cubic { ctrl1, ctrl2 } => {
            vice_geom::flatten::flatten_cubic(p0, ctrl1, ctrl2, p1, tolerance).points
        }
        Segment::CircularArc {
            radius_px,
            large_arc,
            ccw,
        } => {
            vice_geom::flatten::flatten_circular_arc(p0, p1, radius_px, large_arc, ccw, tolerance)
                .map_err(|e| format!("circular arc does not flatten: {e}"))?
                .points
        }
        Segment::EllipticArc { .. } => return Err("elliptic arc is not an M6 fit family".into()),
    };
    (points.len() >= 2)
        .then_some(points)
        .ok_or_else(|| "truth segment flattened to fewer than two points".to_string())
}

fn resample_polyline(poly: &[Pt], step: f64, min_intervals: usize) -> Result<Vec<Pt>, String> {
    let mut cumulative = Vec::with_capacity(poly.len());
    cumulative.push(0.0);
    for window in poly.windows(2) {
        cumulative
            .push(cumulative.last().copied().unwrap_or(0.0) + (window[1] - window[0]).length());
    }
    let length = *cumulative.last().unwrap_or(&0.0);
    if !(length.is_finite() && length > 0.0 && step.is_finite() && step > 0.0) {
        return Err(format!(
            "non-positive segment length {length} or sample step {step}"
        ));
    }
    let intervals = ((length / step).ceil() as usize).max(min_intervals);
    let mut out = Vec::with_capacity(intervals + 1);
    let mut edge = 0usize;
    for i in 0..=intervals {
        let target = length * i as f64 / intervals as f64;
        while edge + 1 < cumulative.len() - 1 && cumulative[edge + 1] < target {
            edge += 1;
        }
        let a = poly[edge];
        let b = poly[edge + 1];
        let span = cumulative[edge + 1] - cumulative[edge];
        let t = if span > 0.0 {
            (target - cumulative[edge]) / span
        } else {
            0.0
        };
        out.push(a + (b - a) * t.clamp(0.0, 1.0));
    }
    Ok(out)
}

fn boundary_chain(points: &[Pt], config: &GeometryOracleConfig) -> BoundaryChain {
    let mut samples = Vec::with_capacity(points.len());
    let mut length_px = 0.0f64;
    for i in 0..points.len() {
        let back = if i > 0 {
            (points[i] - points[i - 1]).length()
        } else {
            0.0
        };
        let forward = if i + 1 < points.len() {
            (points[i + 1] - points[i]).length()
        } else {
            0.0
        };
        let tangent = match (i > 0, i + 1 < points.len()) {
            (true, true) => points[i + 1] - points[i - 1],
            (false, true) => points[i + 1] - points[i],
            (true, false) => points[i] - points[i - 1],
            (false, false) => Pt::new(1.0, 0.0),
        };
        let tangent_length = tangent.length().max(f64::MIN_POSITIVE);
        let weight_ds = 0.5 * (back + forward);
        length_px += weight_ds;
        samples.push(BoundarySample {
            p: points[i],
            normal: Pt::new(-tangent.y / tangent_length, tangent.x / tangent_length),
            halfwidth: config.corridor_halfwidth_px,
            confidence: 1.0,
            weight_ds,
            corr_length_px: config.correlation_length_px,
        });
    }
    BoundaryChain {
        samples,
        // GT formation provides an oriented BoundaryId and its canonical
        // start, so the cut is part of this oracle intervention. Marking a
        // start=end graph boundary cyclic here would make G00 re-cut away the
        // very family/breakpoint binding G20 is meant to hold fixed.
        closed: false,
        length_px,
        corr_length_px: config.correlation_length_px,
        vertices: points.len() as u64,
    }
}

fn measure_boundary(
    observation: &BoundObservation,
    config: &GeometryOracleConfig,
) -> Result<GeometryBoundaryRow, String> {
    let auto = k_best_boundary_models(
        &observation.chain,
        &FIT_BUDGET_V1,
        config.canvas_dim_px,
        config.k_discrete_paths,
    )
    .map_err(|e| format!("G00 automatic candidate generation refused: {e:?}"))?;
    if auto.models.is_empty() {
        return Err(format!(
            "G00 produced no accepted model: candidates {}, paths {}, refused {:?}",
            auto.candidates, auto.discrete_paths, auto.refused
        ));
    }
    let forced = fit_forced_boundary_models(
        &observation.chain,
        &observation.gt_families,
        &observation.gt_breakpoints,
        config.canvas_dim_px,
        config.k_discrete_paths,
    )
    .map_err(|e| format!("G20 forced discrete fit refused: {e:?}"))?;
    if forced.models.is_empty() {
        return Err("G20 produced no accepted model".to_string());
    }
    if forced.models.iter().any(|model| {
        model.families != observation.gt_families || model.breakpoints != observation.gt_breakpoints
    }) {
        return Err("G20 changed a forced family or breakpoint".to_string());
    }

    let auto_first = &auto.models[0];
    let forced_first = &forced.models[0];
    let auto_oracle = oracle_select(&auto.models, &observation.truth)?;
    let forced_oracle = oracle_select(&forced.models, &observation.truth)?;
    let (union_first, union_source) = auto
        .models
        .iter()
        .map(|model| (model, "automatic"))
        .chain(forced.models.iter().map(|model| (model, "forced_gt")))
        .min_by(|(a, _), (b, _)| {
            a.code
                .total_bits()
                .total_cmp(&b.code.total_bits())
                .then(a.proposal_cost_px.total_cmp(&b.proposal_cost_px))
        })
        .ok_or_else(|| "G10 union is empty".to_string())?;

    let arms = vec![
        arm_result(
            "G00",
            "automatic",
            auto_first,
            auto.models.len(),
            &observation.truth,
        )?,
        arm_result(
            "G10",
            union_source,
            union_first,
            auto.models.len() + forced.models.len(),
            &observation.truth,
        )?,
        arm_result(
            "G01",
            "automatic",
            auto_oracle,
            auto.models.len(),
            &observation.truth,
        )?,
        arm_result(
            "G11",
            "forced_gt",
            forced_oracle,
            forced.models.len(),
            &observation.truth,
        )?,
        arm_result(
            "G20",
            "forced_gt",
            forced_first,
            forced.models.len(),
            &observation.truth,
        )?,
    ];
    if arms[2].error.symmetric_max_px > arms[0].error.symmetric_max_px + f64::EPSILON {
        return Err("G01 oracle selector is worse than G00 on the same candidate set".to_string());
    }
    if arms[3].error.symmetric_max_px > arms[4].error.symmetric_max_px + f64::EPSILON {
        return Err("G11 oracle selector is worse than G20 on the same candidate set".to_string());
    }

    Ok(GeometryBoundaryRow {
        fixture_id: observation.fixture_id.clone(),
        scene_id: observation.scene_id.clone(),
        boundary_id: observation.boundary_id,
        samples: observation.chain.samples.len(),
        gt_families: observation
            .gt_families
            .iter()
            .map(|family| family.universe_name())
            .collect(),
        gt_breakpoints: observation.gt_breakpoints.clone(),
        injected_models: forced.models.len(),
        oracle_selector_changed: !std::ptr::eq(auto_oracle, auto_first),
        arms,
    })
}

fn oracle_select<'a>(
    models: &'a [BoundaryModel],
    truth: &[Pt],
) -> Result<&'a BoundaryModel, String> {
    let mut measured = Vec::with_capacity(models.len());
    for model in models {
        measured.push((geometry_error(model, truth)?, model));
    }
    measured
        .into_iter()
        .min_by(|(a, _), (b, _)| {
            a.symmetric_max_px
                .total_cmp(&b.symmetric_max_px)
                .then(a.symmetric_mean_px.total_cmp(&b.symmetric_mean_px))
        })
        .map(|(_, model)| model)
        .ok_or_else(|| "oracle selector was handed no model".to_string())
}

fn arm_result(
    arm: &'static str,
    selected_source: &'static str,
    model: &BoundaryModel,
    candidate_models: usize,
    truth: &[Pt],
) -> Result<GeometryArmResult, String> {
    Ok(GeometryArmResult {
        arm,
        compatibility_key_fingerprint: String::new(),
        candidate_models,
        selected_source,
        families: model
            .families
            .iter()
            .map(|family| family.universe_name())
            .collect(),
        breakpoints: model.breakpoints.clone(),
        smooth: model.smooth.clone(),
        code_bits: model.code.total_bits(),
        proposal_cost_px: model.proposal_cost_px,
        error: geometry_error(model, truth)?,
    })
}

fn geometry_error(model: &BoundaryModel, truth: &[Pt]) -> Result<GeometryError, String> {
    let poly = model
        .geometry
        .flatten()
        .map_err(|e| format!("selected model does not flatten: {e:?}"))?;
    if poly.len() < 2 || truth.len() < 2 {
        return Err("geometry metric received a degenerate polyline".to_string());
    }
    let truth_distances: Vec<f64> = truth
        .iter()
        .map(|point| vice_fit::cost::euclidean_deviation(*point, &poly))
        .collect();
    let model_distances: Vec<f64> = poly
        .iter()
        .map(|point| vice_fit::cost::euclidean_deviation(*point, truth))
        .collect();
    let truth_max = truth_distances.iter().copied().fold(0.0f64, f64::max);
    let model_max = model_distances.iter().copied().fold(0.0f64, f64::max);
    let sum: f64 = truth_distances.iter().chain(&model_distances).sum();
    let count = truth_distances.len() + model_distances.len();
    Ok(GeometryError {
        symmetric_max_px: truth_max.max(model_max),
        symmetric_mean_px: sum / count as f64,
        truth_to_model_max_px: truth_max,
        model_to_truth_max_px: model_max,
    })
}

fn aggregate(rows: &[GeometryBoundaryRow]) -> Vec<GeometryArmAggregate> {
    let mut by_arm: BTreeMap<&'static str, Vec<&GeometryArmResult>> = BTreeMap::new();
    for row in rows {
        for arm in &row.arms {
            by_arm.entry(arm.arm).or_default().push(arm);
        }
    }
    ARM_IDS
        .iter()
        .map(|id| {
            let values = by_arm.get(id).cloned().unwrap_or_default();
            let boundaries = values.len();
            let mean = if values.is_empty() {
                0.0
            } else {
                values
                    .iter()
                    .map(|arm| arm.error.symmetric_max_px)
                    .sum::<f64>()
                    / boundaries as f64
            };
            GeometryArmAggregate {
                arm: id,
                boundaries,
                mean_symmetric_max_px: mean,
                worst_symmetric_max_px: values
                    .iter()
                    .map(|arm| arm.error.symmetric_max_px)
                    .fold(0.0f64, f64::max),
                selected_auto: values
                    .iter()
                    .filter(|arm| arm.selected_source == "automatic")
                    .count(),
                selected_forced: values
                    .iter()
                    .filter(|arm| arm.selected_source == "forced_gt")
                    .count(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;
