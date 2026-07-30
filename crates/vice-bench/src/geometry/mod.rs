//! §27.6 / §28 M6 geometry-oracle decomposition.
//!
//! The harness renders one scene per independent development source group plus
//! explicit M6 family/joint witnesses, extracts Stage-F chains from those
//! rasters, then binds eligible closed chains to `(scene id, BoundaryId)`.
//! Ground truth supplies only labels, breakpoints, and scoring targets; no
//! ground-truth curve samples enter the fit observation. The sealed audit split
//! is never rendered or opened.
//! Arms are distinct interventions over one common compatibility key:
//!
//! - G00: automatic candidates, automatic physical-bit selector;
//! - G10: inject a GT-equivalent family/breakpoint candidate, automatic selector;
//! - G01: automatic candidates, oracle geometry-error selector;
//! - G11: GT-equivalent candidates, oracle selector;
//! - G20: force GT-equivalent families/breakpoints, automatic parameter fit
//!   and automatic physical-bit selector.
//!
//! “GT-equivalent” does not inject parameters.
//! [`vice_fit::fit_forced_boundary_models`] re-fits every forced span to the
//! raster-derived observation and runs the production joint solver. Exact GT
//! parameters are used only to construct the independent scoring target.

use std::collections::BTreeMap;

use serde::Serialize;
use vice_evidence::BoundaryChain;
use vice_fit::{
    fit_forced_boundary_models, k_best_boundary_models, BoundaryModel, SpanFamily, FIT_BUDGET_V1,
    K_DISCRETE_PATHS, MAX_CANONICAL_CUTS,
};
use vice_geom::{ChordTolerancePx, Pt};
use vice_ir::Segment;

use crate::gates::GatesFile;
use crate::gt::corpus::Platform;
use crate::hashing::sha256_hex;
use crate::oracle::key::{CandidateBudget, CompatibilityKey};
use crate::universe::{model_universe_hash, SupportedModelUniverseV1};

pub const GEOMETRY_M6_SCHEMA: &str = "vice-classic/m6-geometry-oracle/v2";
const INTERVENTION_SCHEMA: &str = "vice-classic/m6-geometry-interventions/v2";
mod gate;
mod observations;
use gate::derive_coverage;
pub use gate::{evaluate_gate, GeometryGateConfig};

const BACKEND_ID: &str = "vice-fit/stage-g-h/v2";
const SAMPLE_STEP_PX: f64 = 1.0;
const CORRIDOR_HALFWIDTH_PX: f64 = 0.35;
const CORRELATION_LENGTH_PX: f64 = 1.0;
const TRUTH_CHORD_TOLERANCE_PX: f64 = 0.02;
const ARM_IDS: [&str; 5] = ["G00", "G10", "G01", "G11", "G20"];

/// Frozen population floors read from the six-row raster-derived common
/// population. Floors stay below measured counts where there is headroom;
/// exact family counts and the five-arm set encode the claim being guarded.
pub const GATE_MIN_GEOMETRY_BOUNDARIES: usize = 6;
pub const GATE_MIN_GEOMETRY_ARMS_PER_BOUNDARY: usize = 5;
pub const GATE_MIN_ORACLE_CANDIDATE_INJECTIONS: usize = 10;
pub const GATE_MIN_ORACLE_SELECTOR_CHANGES: usize = 1;
pub const GATE_MIN_INJECTION_SELECTOR_CHANGES: usize = 1;
pub const GATE_MIN_FORCED_SELECTOR_CHANGES: usize = 1;
pub const GATE_MIN_RASTER_DERIVED_ROWS: usize = 6;
pub const GATE_MIN_MULTI_SPAN_ROWS: usize = 6;
pub const GATE_MIN_MULTI_FAMILY_ROWS: usize = 2;
pub const GATE_MIN_ARC_ROWS: usize = 1;
pub const GATE_MIN_QUAD_ROWS: usize = 1;
pub const GATE_MIN_CUBIC_ROWS: usize = 2;
pub const GATE_MIN_FORCED_MULTI_CANDIDATE_ROWS: usize = 2;
pub const GATE_MIN_FORCED_SMOOTH_ROWS: usize = 2;
pub const GATE_MIN_RELATION_SELECTED_ROWS: usize = 2;
pub const GATE_MIN_PRIMITIVE_SELECTED_ROWS: usize = 1;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GeometryOracleConfig {
    pub population_split: &'static str,
    pub scenes_per_source_group: usize,
    pub observation_source: &'static str,
    pub render_size_px: u32,
    pub max_stage_f_truth_match_px: f64,
    pub canvas_dim_px: f64,
    pub sample_step_px: f64,
    pub corridor_halfwidth_px: f64,
    pub correlation_length_px: f64,
    pub truth_chord_tolerance_px: f64,
    pub candidate_budget: usize,
    pub k_discrete_paths: usize,
    pub max_canonical_cuts: usize,
    /// Bind the key to the exact grammar and every load-bearing selector price;
    /// otherwise a model-version change can retain the old fingerprint.
    pub model_universe_hash: String,
    pub geometry_pricing_sha256: String,
    pub backend_source_sha256: String,
}

impl Default for GeometryOracleConfig {
    fn default() -> Self {
        GeometryOracleConfig {
            population_split: "development+targeted_m6_raster_witnesses",
            scenes_per_source_group: 1,
            observation_source: "production_stage_f_from_independent_exact_clip_raster",
            render_size_px: 128,
            max_stage_f_truth_match_px: 2.0,
            canvas_dim_px: 128.0,
            sample_step_px: SAMPLE_STEP_PX,
            corridor_halfwidth_px: CORRIDOR_HALFWIDTH_PX,
            correlation_length_px: CORRELATION_LENGTH_PX,
            truth_chord_tolerance_px: TRUTH_CHORD_TOLERANCE_PX,
            candidate_budget: FIT_BUDGET_V1.cap(),
            k_discrete_paths: K_DISCRETE_PATHS,
            max_canonical_cuts: MAX_CANONICAL_CUTS,
            model_universe_hash: model_universe_hash(&SupportedModelUniverseV1::v1()),
            geometry_pricing_sha256: sha256_hex(vice_fit::pricing_surface_v1().as_bytes()),
            backend_source_sha256: backend_source_hash(),
        }
    }
}

const BACKEND_SOURCE_PATHS: [(&str, &str); 28] = [
    (
        "crates/vice-fit/src/code.rs",
        include_str!("../../../vice-fit/src/code.rs"),
    ),
    (
        "crates/vice-fit/src/corner.rs",
        include_str!("../../../vice-fit/src/corner.rs"),
    ),
    (
        "crates/vice-fit/src/cost.rs",
        include_str!("../../../vice-fit/src/cost.rs"),
    ),
    (
        "crates/vice-fit/src/gate.rs",
        include_str!("../../../vice-fit/src/gate.rs"),
    ),
    (
        "crates/vice-fit/src/grammar/closure.rs",
        include_str!("../../../vice-fit/src/grammar/closure.rs"),
    ),
    (
        "crates/vice-fit/src/grammar/control.rs",
        include_str!("../../../vice-fit/src/grammar/control.rs"),
    ),
    (
        "crates/vice-fit/src/grammar/surface.rs",
        include_str!("../../../vice-fit/src/grammar/surface.rs"),
    ),
    (
        "crates/vice-fit/src/grammar/tests.rs",
        include_str!("../../../vice-fit/src/grammar/tests.rs"),
    ),
    (
        "crates/vice-fit/src/grammar.rs",
        include_str!("../../../vice-fit/src/grammar.rs"),
    ),
    (
        "crates/vice-fit/src/lib.rs",
        include_str!("../../../vice-fit/src/lib.rs"),
    ),
    (
        "crates/vice-fit/src/models.rs",
        include_str!("../../../vice-fit/src/models.rs"),
    ),
    (
        "crates/vice-fit/src/models/bounded_tests.rs",
        include_str!("../../../vice-fit/src/models/bounded_tests.rs"),
    ),
    (
        "crates/vice-fit/src/models/closed.rs",
        include_str!("../../../vice-fit/src/models/closed.rs"),
    ),
    (
        "crates/vice-fit/src/models/ranking_tests.rs",
        include_str!("../../../vice-fit/src/models/ranking_tests.rs"),
    ),
    (
        "crates/vice-fit/src/primitive.rs",
        include_str!("../../../vice-fit/src/primitive.rs"),
    ),
    (
        "crates/vice-fit/src/refit.rs",
        include_str!("../../../vice-fit/src/refit.rs"),
    ),
    (
        "crates/vice-fit/src/refit/g1.rs",
        include_str!("../../../vice-fit/src/refit/g1.rs"),
    ),
    (
        "crates/vice-fit/src/relation.rs",
        include_str!("../../../vice-fit/src/relation.rs"),
    ),
    (
        "crates/vice-fit/src/relation/topology.rs",
        include_str!("../../../vice-fit/src/relation/topology.rs"),
    ),
    (
        "crates/vice-fit/src/schedule.rs",
        include_str!("../../../vice-fit/src/schedule.rs"),
    ),
    (
        "crates/vice-fit/src/solve.rs",
        include_str!("../../../vice-fit/src/solve.rs"),
    ),
    (
        "crates/vice-fit/src/solve/corridor.rs",
        include_str!("../../../vice-fit/src/solve/corridor.rs"),
    ),
    (
        "crates/vice-fit/src/span.rs",
        include_str!("../../../vice-fit/src/span.rs"),
    ),
    (
        "crates/vice-bench/src/geometry/gate.rs",
        include_str!("gate.rs"),
    ),
    (
        "crates/vice-bench/src/geometry/mod.rs",
        include_str!("mod.rs"),
    ),
    (
        "crates/vice-bench/src/geometry/observations.rs",
        include_str!("observations.rs"),
    ),
    (
        "crates/vice-bench/src/geometry/tests.rs",
        include_str!("tests.rs"),
    ),
    (
        "crates/vice-bench/src/geometry/source_manifest_v1",
        "all Rust sources under vice-fit/src and vice-bench/src/geometry",
    ),
];

fn backend_source_hash() -> String {
    let framed = BACKEND_SOURCE_PATHS
        .iter()
        .map(|(path, source)| format!("{path}\u{1f}{source}"))
        .collect::<Vec<_>>()
        .join("\u{1e}");
    sha256_hex(framed.as_bytes())
}

fn compatibility_key(
    config: &GeometryOracleConfig,
    config_hash: &str,
    fixture_set_hash: &str,
) -> CompatibilityKey {
    CompatibilityKey {
        backend_id: format!("{BACKEND_ID}/{}", config.backend_source_sha256),
        config_hash: config_hash.to_string(),
        candidate_budget: CandidateBudget::Candidates {
            max: config.candidate_budget as u64,
        },
        fixture_hash: fixture_set_hash.to_string(),
        intervention_schema_version: INTERVENTION_SCHEMA.to_string(),
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
    pub compatibility_key: CompatibilityKey,
    pub candidate_models: usize,
    pub selected_source: &'static str,
    pub families: Vec<&'static str>,
    pub breakpoints: Vec<usize>,
    pub smooth: Vec<bool>,
    pub closure_smooth: bool,
    pub relations_considered: usize,
    pub relations_selected: usize,
    pub primitives_considered: usize,
    pub primitive_selected: bool,
    pub selected_geometry: &'static str,
    pub geometry_sha256: String,
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
    pub stage_f_truth_match_px: f64,
    pub render_cell: String,
    pub injected_models: usize,
    pub oracle_selector_changed: bool,
    pub injection_selector_changed: bool,
    pub forced_selector_changed: bool,
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
    pub injection_selector_changes: usize,
    pub forced_selector_changes: usize,
    pub raster_derived_rows: usize,
    pub multi_span_rows: usize,
    pub multi_family_rows: usize,
    pub arc_rows: usize,
    pub quad_rows: usize,
    pub cubic_rows: usize,
    pub forced_multi_candidate_rows: usize,
    pub forced_smooth_rows: usize,
    pub relation_selected_rows: usize,
    pub primitive_selected_rows: usize,
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

#[derive(Clone, Serialize)]
pub(super) struct RasterBoundObservation {
    fixture_id: String,
    scene_id: String,
    boundary_id: usize,
    /// The Stage-F order exactly as emitted by the observation pipeline.
    /// Automatic arms must not inherit a cut or orientation from GT.
    chain: BoundaryChain,
    /// A cyclic reindexing of `chain` used only to express forced GT labels.
    /// It contains the same physical samples; no authored point enters it.
    forced_chain: BoundaryChain,
    truth: Vec<Pt>,
    gt_families: Vec<SpanFamily>,
    gt_breakpoints: Vec<usize>,
    stage_f_truth_match_px: f64,
    render_cell: String,
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
    let observations::ObservationPopulation {
        source_groups,
        scenes,
        attempted,
        observations,
        mut exclusions,
    } = observations::collect(&config)?;
    let fixture_contents: Vec<String> = observations
        .iter()
        .map(|observation| {
            serde_json::to_vec(observation)
                .map(|bytes| sha256_hex(&bytes))
                .map_err(|error| error.to_string())
        })
        .collect::<Result<_, _>>()?;
    let fixture_set_hash = sha256_hex(fixture_contents.join("\u{1f}").as_bytes());
    let key = compatibility_key(&config, &config_hash, &fixture_set_hash);
    let mut rows = Vec::new();
    for observation in observations {
        match measure_boundary(&observation, &config, &config_hash, &fixture_set_hash) {
            Ok(row) => rows.push(row),
            Err(reason) => exclusions.push(GeometryExclusion {
                fixture_id: observation.fixture_id,
                stage: "five_arm_common_population",
                reason,
            }),
        }
    }
    rows.sort_by(|a, b| a.fixture_id.cmp(&b.fixture_id));
    exclusions.sort_by(|a, b| a.fixture_id.cmp(&b.fixture_id).then(a.stage.cmp(b.stage)));
    let aggregates = aggregate(&rows);
    let derived = derive_coverage(&rows, &config);

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
        oracle_candidate_injections: derived.candidate_injections,
        oracle_selector_changes: derived.oracle_selector_changes,
        injection_selector_changes: derived.injection_selector_changes,
        forced_selector_changes: derived.forced_selector_changes,
        raster_derived_rows: derived.raster_derived_rows,
        multi_span_rows: derived.multi_span_rows,
        multi_family_rows: derived.multi_family_rows,
        arc_rows: derived.arc_rows,
        quad_rows: derived.quad_rows,
        cubic_rows: derived.cubic_rows,
        forced_multi_candidate_rows: derived.forced_multi_candidate_rows,
        forced_smooth_rows: derived.forced_smooth_rows,
        relation_selected_rows: derived.relation_selected_rows,
        primitive_selected_rows: derived.primitive_selected_rows,
        exclusions,
        aggregates,
        rows,
    })
}

pub(super) fn flatten_truth_segment(
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

fn measure_boundary(
    observation: &RasterBoundObservation,
    config: &GeometryOracleConfig,
    config_hash: &str,
    fixture_set_hash: &str,
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
        &observation.forced_chain,
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
    let key = compatibility_key(config, config_hash, fixture_set_hash);
    let context = ArmContext {
        key: &key,
        truth: &observation.truth,
    };

    let arms = vec![
        arm_result("G00", "automatic", auto_first, auto.models.len(), &context)?,
        arm_result(
            "G10",
            union_source,
            union_first,
            auto.models.len() + forced.models.len(),
            &context,
        )?,
        arm_result("G01", "automatic", auto_oracle, auto.models.len(), &context)?,
        arm_result(
            "G11",
            "forced_gt",
            forced_oracle,
            forced.models.len(),
            &context,
        )?,
        arm_result(
            "G20",
            "forced_gt",
            forced_first,
            forced.models.len(),
            &context,
        )?,
    ];
    if arms[2].error.symmetric_max_px > arms[0].error.symmetric_max_px + f64::EPSILON {
        return Err("G01 oracle selector is worse than G00 on the same candidate set".to_string());
    }
    if arms[3].error.symmetric_max_px > arms[4].error.symmetric_max_px + f64::EPSILON {
        return Err("G11 oracle selector is worse than G20 on the same candidate set".to_string());
    }

    let oracle_selector_changed = arms[2].geometry_sha256 != arms[0].geometry_sha256;
    let injection_selector_changed = arms[1].geometry_sha256 != arms[0].geometry_sha256;
    let forced_selector_changed = arms[3].geometry_sha256 != arms[4].geometry_sha256;
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
        stage_f_truth_match_px: observation.stage_f_truth_match_px,
        render_cell: observation.render_cell.clone(),
        injected_models: forced.models.len(),
        oracle_selector_changed,
        injection_selector_changed,
        forced_selector_changed,
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

struct ArmContext<'a> {
    key: &'a CompatibilityKey,
    truth: &'a [Pt],
}

fn arm_result(
    arm: &'static str,
    selected_source: &'static str,
    model: &BoundaryModel,
    candidate_models: usize,
    context: &ArmContext<'_>,
) -> Result<GeometryArmResult, String> {
    Ok(GeometryArmResult {
        arm,
        compatibility_key: context.key.clone(),
        candidate_models,
        selected_source,
        families: model
            .families
            .iter()
            .map(|family| family.universe_name())
            .collect(),
        breakpoints: model.breakpoints.clone(),
        smooth: model.smooth.clone(),
        closure_smooth: model.closure_smooth,
        relations_considered: model.relations.len(),
        relations_selected: model.relations_kept,
        primitives_considered: model.primitives.len(),
        primitive_selected: model.primitive_kept.is_some(),
        selected_geometry: match &model.geometry {
            vice_fit::SelectedBoundaryGeometry::TypedChain { .. } => "typed_chain",
            vice_fit::SelectedBoundaryGeometry::LoopPrimitive { .. } => "loop_primitive",
        },
        geometry_sha256: sha256_hex(
            &serde_json::to_vec(&model.geometry)
                .map_err(|error| format!("selected geometry does not serialize: {error}"))?,
        ),
        code_bits: model.code.total_bits(),
        proposal_cost_px: model.proposal_cost_px,
        error: geometry_error(model, context.truth)?,
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
