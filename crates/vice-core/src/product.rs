//! Unified routed product surface over the implemented milestone lanes.
//!
//! Production admission remains owned by each calibrated lane. Routing and
//! experimental delivery never turn an inspectable candidate into a sealed
//! production success.

use std::collections::BTreeMap;

use serde::Serialize;
use vice_evidence::analysis::{analyze, Flat2Outcome, ANALYSIS_CONFIG_V1};
use vice_image::{CanonicalImage, DecodeLimits};
use web_time::Instant;

use crate::{
    DecisionStatus, Intent, Preset, ProductPerformanceTrace, VectorizeOutcome, VectorizeRequest,
};

pub const PRODUCT_REPORT_SCHEMA: &str = "vice-classic/product-vectorize-report/v1";
pub const EXPERIMENTAL_MANIFEST_SCHEMA: &str = "vice-classic/experimental-artifact-manifest/v1";
const AUTO_FLAT2_MAX_BOUNDARY_CHAINS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductMode {
    Auto,
    Flat2,
    Multiregion,
    LineArt,
    Gradient,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductLane {
    Flat2,
    Multiregion,
    LineArt,
    Gradient,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProductRequest {
    pub mode: ProductMode,
    pub intent: Intent,
    pub preset: Preset,
    pub production: bool,
    pub experimental_artifacts: bool,
    pub trace: bool,
    pub strict: bool,
}

impl Default for ProductRequest {
    fn default() -> Self {
        Self {
            mode: ProductMode::Auto,
            intent: Intent::Clean,
            preset: Preset::Quality,
            production: true,
            experimental_artifacts: false,
            trace: false,
            strict: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RouteObservation {
    pub lane: ProductLane,
    pub accepted: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RouteDecision {
    pub selected_lane: ProductLane,
    pub reason: String,
    pub observations: Vec<RouteObservation>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProductReport {
    pub schema: &'static str,
    pub status: DecisionStatus,
    pub requested_mode: ProductMode,
    pub selected_lane: ProductLane,
    pub route_reason: String,
    pub route_observations: Vec<RouteObservation>,
    pub production: bool,
    pub experimental_artifacts: bool,
    pub artifact_trust: &'static str,
    pub message: String,
    pub reason: Option<serde_json::Value>,
    pub lane_report: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProductArtifacts {
    pub result_svg: Option<Vec<u8>>,
    pub pure_partition_svg: Option<Vec<u8>>,
    pub scene_json: Option<Vec<u8>>,
    pub export_plan_json: Option<Vec<u8>>,
    pub render_png: Option<Vec<u8>>,
    pub seal_json: Option<Vec<u8>>,
    pub trace_json: Option<Vec<u8>>,
    pub report_json: Vec<u8>,
    pub artifact_manifest_json: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProductResult {
    pub report: ProductReport,
    pub artifacts: ProductArtifacts,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExperimentalArtifactManifest {
    pub schema: &'static str,
    pub production: bool,
    pub experimental: bool,
    pub selected_lane: ProductLane,
    pub artifact_trust: &'static str,
    pub result_svg: Option<&'static str>,
    pub pure_partition_svg: Option<&'static str>,
    pub render_png: Option<&'static str>,
    pub scene_json: Option<&'static str>,
    pub export_plan_json: Option<&'static str>,
    pub seal_json: Option<&'static str>,
}

/// Inspect the input class without treating Flat2 refusal as the product's
/// terminal result. The order and thresholds are fixed product policy.
pub fn classify_product_lane(bytes: &[u8]) -> Result<RouteDecision, String> {
    let image = CanonicalImage::decode(bytes, &DecodeLimits::default())
        .map_err(|error| format!("decode: {error}"))?;
    classify_decoded(
        bytes,
        &image,
        false,
        &mut ProductPerformanceTrace::default(),
    )
}

fn classify_decoded(
    bytes: &[u8],
    image: &CanonicalImage,
    cheap_multiregion_preview: bool,
    performance: &mut ProductPerformanceTrace,
) -> Result<RouteDecision, String> {
    let mut observations = Vec::new();
    let flat2_started = Instant::now();
    let flat2 = analyze(image, &ANALYSIS_CONFIG_V1, None);
    performance.route_flat2_probe_ms = perf_ms(flat2_started);
    if matches!(flat2.outcome, Flat2Outcome::Supported { .. }) {
        let chains = flat2
            .boundary
            .as_ref()
            .map_or(0, |boundary| boundary.chains.len());
        // Auto is a responsive product route, not permission to enter the
        // combinatorial M7 fitter on an arbitrary glyph inventory. Explicit
        // Flat2 mode remains available for operators who choose that cost.
        if chains <= AUTO_FLAT2_MAX_BOUNDARY_CHAINS {
            let line_art_started = Instant::now();
            let line_art = crate::inspect_m10_line_art(bytes);
            performance.route_line_art_probe_ms = performance
                .route_line_art_probe_ms
                .saturating_add(perf_ms(line_art_started));
            if let Ok(inspection) = line_art {
                let (stroke_first, detail) = line_art_route_signal(&inspection);
                if stroke_first {
                    observations.push(observation(
                        ProductLane::Flat2,
                        false,
                        format!(
                            "Flat2 evidence is supported with {chains} boundary chain(s), but positive thin-stroke evidence selects M10"
                        ),
                    ));
                    observations.push(observation(ProductLane::LineArt, true, detail));
                    return Ok(RouteDecision {
                        selected_lane: ProductLane::LineArt,
                        reason: "thin stroke-first evidence overrides fill fitting in auto mode"
                            .into(),
                        observations,
                    });
                }
            }
            observations.push(observation(
                ProductLane::Flat2,
                true,
                format!(
                    "Flat2 evidence has one supported mixture class and {chains} boundary chain(s)"
                ),
            ));
            return Ok(RouteDecision {
                selected_lane: ProductLane::Flat2,
                reason: "supported Flat2 evidence inside the auto route complexity contract".into(),
                observations,
            });
        }
        observations.push(observation(
            ProductLane::Flat2,
            false,
            format!(
                "Flat2 evidence is supported but {chains} boundary chains exceed the auto route cap of {AUTO_FLAT2_MAX_BOUNDARY_CHAINS}; use explicit Flat2 mode to opt into the full fitter"
            ),
        ));
    } else {
        observations.push(observation(
            ProductLane::Flat2,
            false,
            format!("Flat2 evidence outcome: {:?}", flat2.outcome),
        ));
    }

    let multicolor_started = Instant::now();
    let dominant_share = dominant_opaque_color_share(image, 16);
    performance.route_multicolor_probe_ms = perf_ms(multicolor_started);

    // Three or more stable palette modes plus concentrated authored colours
    // are the positive M8 signal. Check this before line-art so multicolour
    // logos are not mistaken for strokes, while smooth ramps remain eligible
    // for M11.
    if flat2.palette_modes >= 3 && dominant_share >= 0.85 {
        if cheap_multiregion_preview {
            observations.push(observation(
                ProductLane::Multiregion,
                true,
                format!(
                    "{} stable palette modes with {:.1}% concentrated authored colours; seed preparation is deferred to the bounded preview",
                    flat2.palette_modes,
                    dominant_share * 100.0
                ),
            ));
            return Ok(RouteDecision {
                selected_lane: ProductLane::Multiregion,
                reason: "cheap multicolour flat-art preview evidence".into(),
                observations,
            });
        }
        let seed_started = Instant::now();
        let proposed = crate::propose_multiregion_seeds(bytes);
        performance.route_multicolor_probe_ms = performance
            .route_multicolor_probe_ms
            .saturating_add(perf_ms(seed_started));
        match proposed {
            Ok(seeds) if !seeds.seeds.is_empty() => {
                observations.push(observation(
                    ProductLane::Multiregion,
                    true,
                    format!(
                        "{} stable palette modes and {} M8 seed candidate(s)",
                        flat2.palette_modes,
                        seeds.seeds.len()
                    ),
                ));
                return Ok(RouteDecision {
                    selected_lane: ProductLane::Multiregion,
                    reason: "multicolour flat-art evidence".into(),
                    observations,
                });
            }
            Ok(_) => observations.push(observation(
                ProductLane::Multiregion,
                false,
                "M8 produced an empty seed inventory",
            )),
            Err(error) => observations.push(observation(
                ProductLane::Multiregion,
                false,
                format!("M8 inspection refused: {error}"),
            )),
        }
    }

    // Smooth opaque colour fields have low exact-colour concentration. Flat
    // artwork (including antialiased edges) is kept out of the gradient lane.
    let source_is_opaque = image
        .straight_srgb8()
        .chunks_exact(4)
        .all(|pixel| pixel[3] == 255);
    if source_is_opaque && dominant_share < 0.85 {
        let gradient_started = Instant::now();
        let classified_gradient = crate::classify_m11_gradient(bytes);
        performance.route_gradient_probe_ms = perf_ms(gradient_started);
        match classified_gradient {
            Ok(classified)
                if !matches!(classified.report.decision, crate::M11GradientKind::Solid)
                    && classified.report.margin_bits > 0.0 =>
            {
                observations.push(observation(
                    ProductLane::Gradient,
                    true,
                    format!(
                        "{:?} beats the runner-up by {:.3} bits",
                        classified.report.decision, classified.report.margin_bits
                    ),
                ));
                return Ok(RouteDecision {
                    selected_lane: ProductLane::Gradient,
                    reason: "globally smooth gradient evidence".into(),
                    observations,
                });
            }
            Ok(classified) => observations.push(observation(
                ProductLane::Gradient,
                false,
                format!(
                    "gradient classifier selected {:?} with margin {:.3}",
                    classified.report.decision, classified.report.margin_bits
                ),
            )),
            Err(error) => observations.push(observation(
                ProductLane::Gradient,
                false,
                format!("gradient inspection refused: {error}"),
            )),
        }
    } else {
        observations.push(observation(
            ProductLane::Gradient,
            false,
            format!(
                "source is alpha-bearing or top colours cover {:.1}% of opaque pixels",
                dominant_share * 100.0
            ),
        ));
    }

    let line_art_started = Instant::now();
    let inspected_line_art = crate::inspect_m10_line_art(bytes);
    performance.route_line_art_probe_ms = performance
        .route_line_art_probe_ms
        .saturating_add(perf_ms(line_art_started));
    match inspected_line_art {
        Ok(inspection) => {
            let (stroke_first, detail) = line_art_route_signal(&inspection);
            if stroke_first {
                observations.push(observation(ProductLane::LineArt, true, detail));
                return Ok(RouteDecision {
                    selected_lane: ProductLane::LineArt,
                    reason: "thin stroke-first evidence".into(),
                    observations,
                });
            }
            observations.push(observation(ProductLane::LineArt, false, detail));
        }
        Err(error) => observations.push(observation(
            ProductLane::LineArt,
            false,
            format!("line-art inspection refused: {error}"),
        )),
    }

    let seed_started = Instant::now();
    let proposed = crate::propose_multiregion_seeds(bytes);
    performance.route_multicolor_probe_ms = performance
        .route_multicolor_probe_ms
        .saturating_add(perf_ms(seed_started));
    match proposed {
        Ok(seeds) if !seeds.seeds.is_empty() => {
            observations.push(observation(
                ProductLane::Multiregion,
                true,
                format!("{} M8 seed candidate(s)", seeds.seeds.len()),
            ));
            Ok(RouteDecision {
                selected_lane: ProductLane::Multiregion,
                reason: "multiregion evidence after other lane refusals".into(),
                observations,
            })
        }
        Ok(_) => unsupported_route(observations, "no lane produced a candidate"),
        Err(error) => {
            observations.push(observation(
                ProductLane::Multiregion,
                false,
                format!("M8 inspection refused: {error}"),
            ));
            unsupported_route(observations, "no implemented lane supports this input")
        }
    }
}

fn line_art_route_signal(inspection: &crate::M10Inspection) -> (bool, String) {
    let pixels = u64::from(inspection.evidence.width_px) * u64::from(inspection.evidence.height_px);
    let foreground_share = if pixels == 0 {
        1.0
    } else {
        inspection.evidence.foreground_pixels as f64 / pixels as f64
    };
    let width_limit = (f64::from(
        inspection
            .evidence
            .width_px
            .min(inspection.evidence.height_px),
    ) * 0.08)
        .max(4.0);
    let accepted = foreground_share <= 0.40 && inspection.evidence.median_width_px <= width_limit;
    let qualifier = if accepted {
        "stroke evidence"
    } else {
        "stroke evidence is too fill-like"
    };
    (
        accepted,
        format!(
            "{qualifier}: {:.1}% foreground, median width {:.2}px (limit {:.2}px)",
            foreground_share * 100.0,
            inspection.evidence.median_width_px,
            width_limit,
        ),
    )
}

fn unsupported_route(
    observations: Vec<RouteObservation>,
    reason: &str,
) -> Result<RouteDecision, String> {
    Ok(RouteDecision {
        selected_lane: ProductLane::None,
        reason: reason.into(),
        observations,
    })
}

fn observation(lane: ProductLane, accepted: bool, detail: impl Into<String>) -> RouteObservation {
    RouteObservation {
        lane,
        accepted,
        detail: detail.into(),
    }
}

fn dominant_opaque_color_share(image: &CanonicalImage, limit: usize) -> f64 {
    let mut counts = BTreeMap::<[u8; 3], u64>::new();
    let mut opaque = 0u64;
    for pixel in image.straight_srgb8().chunks_exact(4) {
        if pixel[3] == 255 {
            *counts.entry([pixel[0], pixel[1], pixel[2]]).or_default() += 1;
            opaque += 1;
        }
    }
    if opaque == 0 {
        return 1.0;
    }
    let mut values = counts.into_values().collect::<Vec<_>>();
    values.sort_unstable_by(|a, b| b.cmp(a));
    values.into_iter().take(limit).sum::<u64>() as f64 / opaque as f64
}

pub fn vectorize_product(bytes: &[u8], request: &ProductRequest) -> ProductResult {
    let decode_started = Instant::now();
    let image = match CanonicalImage::decode(bytes, &DecodeLimits::default()) {
        Ok(image) => image,
        Err(error) => {
            return finish(
                ProductReport {
                    schema: PRODUCT_REPORT_SCHEMA,
                    status: DecisionStatus::Failed,
                    requested_mode: request.mode,
                    selected_lane: ProductLane::None,
                    route_reason: "input decode failed".into(),
                    route_observations: Vec::new(),
                    production: false,
                    experimental_artifacts: false,
                    artifact_trust: "none",
                    message: error.to_string(),
                    reason: Some(serde_json::json!({
                        "reason": "decode",
                        "detail": error.to_string(),
                    })),
                    lane_report: serde_json::Value::Null,
                },
                ProductArtifacts::default(),
            );
        }
    };
    let mut performance = ProductPerformanceTrace {
        decode_ms: perf_ms(decode_started),
        ..ProductPerformanceTrace::default()
    };
    let route = match request.mode {
        ProductMode::Auto => match classify_decoded(
            bytes,
            &image,
            request.experimental_artifacts && !request.strict,
            &mut performance,
        ) {
            Ok(route) => route,
            Err(error) => {
                return failed_result(request, ProductLane::None, "route inspection failed", error)
            }
        },
        ProductMode::Flat2 => explicit_route(ProductLane::Flat2),
        ProductMode::Multiregion => explicit_route(ProductLane::Multiregion),
        ProductMode::LineArt => explicit_route(ProductLane::LineArt),
        ProductMode::Gradient => explicit_route(ProductLane::Gradient),
    };
    match route.selected_lane {
        ProductLane::Flat2 => execute_flat2(bytes, request, route),
        ProductLane::Multiregion => execute_multiregion(bytes, &image, request, route, performance),
        ProductLane::LineArt => execute_line_art(bytes, request, route),
        ProductLane::Gradient => execute_gradient(bytes, request, route),
        ProductLane::None => finish(
            ProductReport {
                schema: PRODUCT_REPORT_SCHEMA,
                status: DecisionStatus::Unsupported,
                requested_mode: request.mode,
                selected_lane: ProductLane::None,
                route_reason: route.reason.clone(),
                route_observations: route.observations,
                production: false,
                experimental_artifacts: false,
                artifact_trust: "none",
                message: "No implemented product lane supports this input".into(),
                reason: Some(serde_json::json!({
                    "reason": "route",
                    "detail": route.reason,
                })),
                lane_report: serde_json::Value::Null,
            },
            ProductArtifacts::default(),
        ),
    }
}

fn explicit_route(lane: ProductLane) -> RouteDecision {
    RouteDecision {
        selected_lane: lane,
        reason: format!("explicit {:?} mode", lane),
        observations: vec![observation(lane, true, "selected explicitly by operator")],
    }
}

fn flat2_request(request: &ProductRequest) -> VectorizeRequest {
    VectorizeRequest {
        intent: request.intent,
        preset: request.preset,
        trace: request.trace,
        strict: request.strict,
        production: request.production,
        ..VectorizeRequest::default()
    }
}

fn execute_flat2(bytes: &[u8], request: &ProductRequest, route: RouteDecision) -> ProductResult {
    let outcome = crate::vectorize_embedded_production(bytes, &flat2_request(request));
    product_from_flat2_with_route(outcome, request, route)
}

/// Adapt an explicitly configured legacy Flat2 call into the unified product
/// contract without changing its production verdict or artifacts.
pub fn product_from_flat2_outcome(
    outcome: VectorizeOutcome,
    request: &ProductRequest,
    route_reason: impl Into<String>,
) -> ProductResult {
    product_from_flat2_with_route(
        outcome,
        request,
        RouteDecision {
            selected_lane: ProductLane::Flat2,
            reason: route_reason.into(),
            observations: vec![observation(
                ProductLane::Flat2,
                true,
                "selected explicitly by operator",
            )],
        },
    )
}

fn product_from_flat2_with_route(
    outcome: VectorizeOutcome,
    request: &ProductRequest,
    route: RouteDecision,
) -> ProductResult {
    let status = outcome.report().status;
    let production =
        matches!(&outcome, VectorizeOutcome::Success(success) if success.report.production);
    let lane_report = serde_json::to_value(outcome.report())
        .unwrap_or_else(|error| serde_json::json!({"serialization_error": error.to_string()}));
    let reason = lane_report
        .get("reason")
        .filter(|value| !value.is_null())
        .cloned();
    let mut artifacts = ProductArtifacts::default();
    if let VectorizeOutcome::Success(success) = outcome {
        artifacts.result_svg = Some(success.artifacts.result_svg);
        artifacts.pure_partition_svg = Some(success.artifacts.pure_partition_svg);
        artifacts.scene_json = Some(success.artifacts.scene_json);
        artifacts.export_plan_json = Some(success.artifacts.export_plan_json);
        artifacts.render_png = Some(success.artifacts.render_png);
        artifacts.seal_json = Some(success.artifacts.seal_json);
        artifacts.trace_json = success.artifacts.trace_json;
    }
    finish(
        ProductReport {
            schema: PRODUCT_REPORT_SCHEMA,
            status,
            requested_mode: request.mode,
            selected_lane: ProductLane::Flat2,
            route_reason: route.reason,
            route_observations: route.observations,
            production,
            experimental_artifacts: false,
            artifact_trust: if production {
                "sealed_production"
            } else {
                "none"
            },
            message: if production {
                "Calibrated Flat2 production success".into()
            } else {
                "Flat2 returned a typed non-success outcome".into()
            },
            reason,
            lane_report,
        },
        artifacts,
    )
}

fn execute_multiregion(
    bytes: &[u8],
    image: &CanonicalImage,
    request: &ProductRequest,
    route: RouteDecision,
    performance: ProductPerformanceTrace,
) -> ProductResult {
    if request.experimental_artifacts && !request.strict {
        let preview_cfg = match request.preset {
            Preset::Fast => crate::M8PreviewConfig::fast(),
            Preset::Quality => crate::M8PreviewConfig::quality(),
        };
        let preview = match crate::preview_multiregion(image, preview_cfg, performance) {
            Ok(preview) => preview,
            Err(error) => {
                return unsupported_result(
                    request,
                    route,
                    format!("M8 bounded preview refused: {error}"),
                )
            }
        };
        let mut artifacts = ProductArtifacts {
            result_svg: Some(preview.result_svg),
            pure_partition_svg: Some(preview.pure_svg),
            scene_json: Some(preview.scene_json),
            export_plan_json: Some(preview.plan_json),
            render_png: partition_png(&preview.proxy_render).ok(),
            ..ProductArtifacts::default()
        };
        let lane_report = serde_json::json!({
            "preview": preview.report,
            "exact": null,
            "delivery": "lightweight_experimental_no_release_seal",
        });
        let has_artifact = artifacts.result_svg.is_some() || artifacts.render_png.is_some();
        return experimental_lane_result(
            request,
            route,
            lane_report,
            std::mem::take(&mut artifacts),
            has_artifact,
            "M8 bounded preview: one proxy, one seed, zero exact refinement trials; non-production",
        );
    }
    let exact_cfg = crate::M8ExactConfig::default();
    let solved = match crate::solve_multiregion_exact(bytes, &exact_cfg) {
        Ok(solved) => solved,
        Err(error) => {
            return unsupported_result(request, route, format!("M8 exact route refused: {error}"))
        }
    };
    let mut artifacts = ProductArtifacts::default();
    let mut delivery_report = serde_json::Value::Null;
    let mut delivery_error = None;
    if request.experimental_artifacts {
        match crate::seal_multiregion_delivery(
            bytes,
            &solved,
            &exact_cfg,
            &crate::M8DeliveryConfig::default(),
        ) {
            Ok(delivery) => {
                delivery_report = serde_json::to_value(&delivery.report).unwrap_or_default();
                artifacts.result_svg = Some(delivery.seam_svg);
                artifacts.pure_partition_svg = Some(delivery.pure_svg);
                artifacts.scene_json = Some(delivery.scene_json);
                artifacts.export_plan_json = Some(delivery.plan_json);
                artifacts.render_png = Some(delivery.seam_png);
                artifacts.seal_json = Some(delivery.seal_json);
            }
            Err(error) => {
                delivery_error = Some(error.to_string());
                artifacts.scene_json = vice_ir::canonical_scene_bytes(&solved.scene).ok();
                artifacts.render_png = partition_png(&solved.render).ok();
            }
        }
    }
    let has_artifact = artifacts.scene_json.is_some()
        || artifacts.result_svg.is_some()
        || artifacts.render_png.is_some();
    let lane_report = serde_json::json!({
        "exact": solved.report,
        "delivery": delivery_report,
        "delivery_error": delivery_error,
    });
    experimental_lane_result(
        request,
        route,
        lane_report,
        artifacts,
        has_artifact,
        "M8 candidate is available only as an explicitly non-production artifact on this routed surface",
    )
}

fn perf_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
}

fn execute_line_art(bytes: &[u8], request: &ProductRequest, route: RouteDecision) -> ProductResult {
    let selected = match crate::select_m10_line_art_stroke_only(bytes) {
        Ok(selected) => selected,
        Err(error) => {
            return unsupported_result(
                request,
                route,
                format!("M10 line-art route refused: {error}"),
            )
        }
    };
    let lane_report = serde_json::to_value(&selected.report).unwrap_or_default();
    let mut artifacts = ProductArtifacts::default();
    if request.experimental_artifacts {
        artifacts.scene_json = vice_ir::stroke_scene_bytes(&selected.selected_stroke).ok();
        let canvas = selected.selected_stroke.scene().canvas;
        artifacts.render_png = encode_rgba_png(
            canvas.width_px,
            canvas.height_px,
            &selected.selected_straight_srgb8,
        )
        .ok();
    }
    let has_artifact = artifacts.scene_json.is_some() || artifacts.render_png.is_some();
    experimental_lane_result(
        request,
        route,
        lane_report,
        artifacts,
        has_artifact,
        "M10 stroke-only selection omits the unavailable fill witness and is non-production",
    )
}

fn execute_gradient(bytes: &[u8], request: &ProductRequest, route: RouteDecision) -> ProductResult {
    let selected = match crate::classify_m11_gradient(bytes) {
        Ok(selected) => selected,
        Err(error) => {
            return unsupported_result(
                request,
                route,
                format!("M11 gradient route refused: {error}"),
            )
        }
    };
    let lane_report = serde_json::to_value(&selected.report).unwrap_or_default();
    let mut artifacts = ProductArtifacts::default();
    if request.experimental_artifacts {
        artifacts.scene_json = Some(selected.selected_scene_json);
        let canvas = selected.selected_scene.scene().canvas;
        artifacts.render_png = encode_rgba_png(
            canvas.width_px,
            canvas.height_px,
            &selected.selected_straight_srgb8,
        )
        .ok();
    }
    let has_artifact = artifacts.scene_json.is_some() || artifacts.render_png.is_some();
    experimental_lane_result(
        request,
        route,
        lane_report,
        artifacts,
        has_artifact,
        "M11 is an uncalibrated product lane; scene and render are for manual inspection only",
    )
}

fn experimental_lane_result(
    request: &ProductRequest,
    route: RouteDecision,
    lane_report: serde_json::Value,
    mut artifacts: ProductArtifacts,
    has_artifact: bool,
    message: &str,
) -> ProductResult {
    let experimental = request.experimental_artifacts && has_artifact;
    if experimental {
        let manifest = ExperimentalArtifactManifest {
            schema: EXPERIMENTAL_MANIFEST_SCHEMA,
            production: false,
            experimental: true,
            selected_lane: route.selected_lane,
            artifact_trust: "non_production_manual_inspection_only",
            result_svg: artifacts
                .result_svg
                .as_ref()
                .map(|_| "result.experimental.svg"),
            pure_partition_svg: artifacts
                .pure_partition_svg
                .as_ref()
                .map(|_| "result.experimental.pure-partition.svg"),
            render_png: artifacts
                .render_png
                .as_ref()
                .map(|_| "result.experimental.render.png"),
            scene_json: artifacts
                .scene_json
                .as_ref()
                .map(|_| "result.experimental.scene.json"),
            export_plan_json: artifacts
                .export_plan_json
                .as_ref()
                .map(|_| "result.experimental.export-plan.json"),
            seal_json: artifacts
                .seal_json
                .as_ref()
                .map(|_| "result.experimental.seal.json"),
        };
        artifacts.artifact_manifest_json = serde_json::to_vec_pretty(&manifest).ok();
    }
    finish(
        ProductReport {
            schema: PRODUCT_REPORT_SCHEMA,
            status: if experimental {
                DecisionStatus::Ambiguous
            } else {
                DecisionStatus::Unsupported
            },
            requested_mode: request.mode,
            selected_lane: route.selected_lane,
            route_reason: route.reason,
            route_observations: route.observations,
            production: false,
            experimental_artifacts: experimental,
            artifact_trust: if experimental {
                "non_production_manual_inspection_only"
            } else {
                "none"
            },
            message: if experimental {
                message.into()
            } else {
                "Lane candidate is not production-admitted; rerun with --experimental to inspect it"
                    .into()
            },
            reason: Some(serde_json::json!({
                "reason": "non_production",
                "detail": message,
            })),
            lane_report,
        },
        artifacts,
    )
}

fn unsupported_result(
    request: &ProductRequest,
    route: RouteDecision,
    message: String,
) -> ProductResult {
    let reason = serde_json::json!({
        "reason": "lane",
        "detail": message.clone(),
    });
    finish(
        ProductReport {
            schema: PRODUCT_REPORT_SCHEMA,
            status: DecisionStatus::Unsupported,
            requested_mode: request.mode,
            selected_lane: route.selected_lane,
            route_reason: route.reason,
            route_observations: route.observations,
            production: false,
            experimental_artifacts: false,
            artifact_trust: "none",
            message,
            reason: Some(reason),
            lane_report: serde_json::Value::Null,
        },
        ProductArtifacts::default(),
    )
}

fn failed_result(
    request: &ProductRequest,
    lane: ProductLane,
    reason: &str,
    message: String,
) -> ProductResult {
    let detail = message.clone();
    finish(
        ProductReport {
            schema: PRODUCT_REPORT_SCHEMA,
            status: DecisionStatus::Failed,
            requested_mode: request.mode,
            selected_lane: lane,
            route_reason: reason.into(),
            route_observations: Vec::new(),
            production: false,
            experimental_artifacts: false,
            artifact_trust: "none",
            message,
            reason: Some(serde_json::json!({
                "reason": "internal",
                "detail": detail,
            })),
            lane_report: serde_json::Value::Null,
        },
        ProductArtifacts::default(),
    )
}

fn finish(report: ProductReport, mut artifacts: ProductArtifacts) -> ProductResult {
    artifacts.report_json = serde_json::to_vec(&report)
        .unwrap_or_else(|error| format!("{{\"serialization_error\":\"{error}\"}}").into_bytes());
    ProductResult { report, artifacts }
}

fn encode_rgba_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    if rgba.len() != width as usize * height as usize * 4 {
        return Err("RGBA render dimensions do not match byte length".into());
    }
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
        writer
            .write_image_data(rgba)
            .map_err(|error| error.to_string())?;
    }
    Ok(bytes)
}

fn partition_png(render: &vice_render::PartitionRender) -> Result<Vec<u8>, String> {
    let mut rgba = Vec::with_capacity(render.composite.len() * 4);
    for pixel in &render.composite {
        if pixel.a <= 1e-12 {
            rgba.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            rgba.extend_from_slice(&[
                vice_ir::color::linear_to_srgb_u8(pixel.r / pixel.a),
                vice_ir::color::linear_to_srgb_u8(pixel.g / pixel.a),
                vice_ir::color::linear_to_srgb_u8(pixel.b / pixel.a),
                (pixel.a.clamp(0.0, 1.0) * 255.0).round() as u8,
            ]);
        }
    }
    encode_rgba_png(render.width_px, render.height_px, &rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32, pixel: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                rgba.extend_from_slice(&pixel(x, y));
            }
        }
        encode_rgba_png(width, height, &rgba).unwrap()
    }

    #[test]
    fn auto_routes_supported_flat2_to_flat2() {
        let bytes = png(128, 128, |x, y| {
            if (24..104).contains(&x) && (24..104).contains(&y) {
                [220, 40, 30, 255]
            } else {
                [0, 0, 0, 0]
            }
        });
        let route = classify_product_lane(&bytes).unwrap();
        assert_eq!(route.selected_lane, ProductLane::Flat2);
    }

    #[test]
    fn auto_does_not_enter_full_flat2_fitting_for_a_high_chain_logo() {
        let bytes = png(128, 64, |x, y| {
            let cell_x = x / 16;
            let cell_y = y / 16;
            let inside = (x % 16 >= 4 && x % 16 < 10)
                && (y % 16 >= 4 && y % 16 < 10)
                && (cell_x + cell_y) % 2 == 0;
            if inside {
                [20, 20, 20, 255]
            } else {
                [250, 250, 250, 255]
            }
        });
        let route = classify_product_lane(&bytes).unwrap();
        assert_ne!(route.selected_lane, ProductLane::Flat2);
        assert!(route
            .observations
            .iter()
            .any(|row| row.lane == ProductLane::Flat2 && row.detail.contains("auto route cap")));
    }

    #[test]
    fn auto_routes_three_colour_flat_art_to_multiregion() {
        let bytes = png(24, 12, |x, _| match x / 8 {
            0 => [230, 20, 20, 255],
            1 => [20, 220, 30, 255],
            _ => [20, 40, 230, 255],
        });
        let route = classify_product_lane(&bytes).unwrap();
        assert_eq!(route.selected_lane, ProductLane::Multiregion);
    }

    #[test]
    fn experimental_multiregion_uses_one_bounded_preview_candidate() {
        let bytes = png(256, 170, |x, _| match x / 86 {
            0 => [230, 20, 20, 255],
            1 => [20, 220, 30, 255],
            _ => [20, 40, 230, 255],
        });
        let result = vectorize_product(
            &bytes,
            &ProductRequest {
                mode: ProductMode::Auto,
                preset: Preset::Fast,
                experimental_artifacts: true,
                ..ProductRequest::default()
            },
        );
        assert_eq!(result.report.selected_lane, ProductLane::Multiregion);
        assert_eq!(result.report.status, DecisionStatus::Ambiguous);
        assert!(!result.report.production);
        assert!(result.report.experimental_artifacts);
        let preview = &result.report.lane_report["preview"];
        assert_eq!(preview["performance"]["base_candidate_count"], 1);
        assert_eq!(preview["performance"]["exact_candidate_count"], 0);
        assert_eq!(preview["performance"]["vertex_trial_count"], 0);
        assert!(result
            .artifacts
            .result_svg
            .as_ref()
            .is_some_and(|bytes| !bytes.is_empty()));
        assert!(result.artifacts.seal_json.is_none());
    }

    #[test]
    fn auto_routes_a_thin_bar_to_line_art_even_when_flat2_is_supported() {
        let bytes = png(48, 24, |x, y| {
            if (5..43).contains(&x) && (10..14).contains(&y) {
                [0, 0, 0, 255]
            } else {
                [255, 255, 255, 255]
            }
        });
        let route = classify_product_lane(&bytes).unwrap();
        assert_eq!(route.selected_lane, ProductLane::LineArt);
        assert!(route.reason.contains("stroke-first"));
    }

    #[test]
    fn auto_routes_a_smooth_opaque_ramp_to_gradient() {
        let bytes = png(64, 24, |x, _| {
            let red = ((x as f64 / 63.0) * 255.0).round() as u8;
            [red, 40, 255 - red, 255]
        });
        let route = classify_product_lane(&bytes).unwrap();
        assert_eq!(route.selected_lane, ProductLane::Gradient);
    }

    #[test]
    fn experimental_gradient_is_inspectable_but_never_production() {
        let bytes = png(32, 16, |x, _| {
            let red = ((x as f64 / 31.0) * 255.0).round() as u8;
            [red, 20, 255 - red, 255]
        });
        let result = vectorize_product(
            &bytes,
            &ProductRequest {
                mode: ProductMode::Gradient,
                experimental_artifacts: true,
                ..ProductRequest::default()
            },
        );
        assert_eq!(result.report.selected_lane, ProductLane::Gradient);
        assert!(!result.report.production);
        assert!(result.report.experimental_artifacts);
        assert!(result.artifacts.scene_json.is_some());
        assert!(result.artifacts.render_png.is_some());
        assert!(result.artifacts.artifact_manifest_json.is_some());
    }
}
