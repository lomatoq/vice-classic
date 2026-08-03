//! M12 WASM adapter. All inference and release policy remain in `vice-core`.

#![forbid(unsafe_code)]

use serde::Serialize;
use wasm_bindgen::prelude::*;

pub const WASM_RESULT_SCHEMA: &str = "vice-classic/wasm-result/v1";
pub const WASM_PRODUCT_RESULT_SCHEMA: &str = "vice-classic/wasm-product-result/v1";
const MAX_WASM_INPUT_BYTES: usize = 64 * 1024 * 1024;

#[derive(Serialize)]
struct WasmVectorizeResult {
    schema: &'static str,
    status: vice_core::DecisionStatus,
    report: serde_json::Value,
    result_svg: Option<String>,
    pure_partition_svg: Option<String>,
    render_png: Option<Vec<u8>>,
}

#[derive(Serialize)]
struct WasmGradientResult {
    schema: &'static str,
    report: vice_core::M11ClassificationReport,
    scene_json: String,
    render_rgba: Vec<u8>,
}

#[derive(Serialize)]
struct WasmProductResult {
    schema: &'static str,
    status: vice_core::DecisionStatus,
    selected_lane: vice_core::ProductLane,
    route_reason: String,
    production: bool,
    experimental_artifacts: bool,
    report: serde_json::Value,
    result_svg: Option<String>,
    pure_partition_svg: Option<String>,
    scene_json: Option<String>,
    render_png: Option<Vec<u8>>,
    artifact_manifest: Option<serde_json::Value>,
}

#[wasm_bindgen]
pub fn vectorize_flat2(bytes: &[u8], preset: &str) -> Result<JsValue, JsValue> {
    ensure_input(bytes)?;
    let request = vice_core::VectorizeRequest {
        preset: parse_preset(preset)?,
        ..vice_core::VectorizeRequest::default()
    };
    let outcome = vice_core::vectorize_embedded_production(bytes, &request);
    let report = serde_json::to_value(outcome.report()).map_err(js_error)?;
    let result = match outcome {
        vice_core::VectorizeOutcome::Success(success) => WasmVectorizeResult {
            schema: WASM_RESULT_SCHEMA,
            status: success.report.status,
            report,
            result_svg: Some(String::from_utf8(success.artifacts.result_svg).map_err(js_error)?),
            pure_partition_svg: Some(
                String::from_utf8(success.artifacts.pure_partition_svg).map_err(js_error)?,
            ),
            render_png: Some(success.artifacts.render_png),
        },
        vice_core::VectorizeOutcome::Ambiguous(value)
        | vice_core::VectorizeOutcome::Unsupported(value)
        | vice_core::VectorizeOutcome::Failed(value) => WasmVectorizeResult {
            schema: WASM_RESULT_SCHEMA,
            status: value.status,
            report,
            result_svg: None,
            pure_partition_svg: None,
            render_png: None,
        },
    };
    result
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(js_error)
}

/// Primary routed browser product entry point. Experimental artifacts remain
/// visibly non-production in both the unified report and manifest.
#[wasm_bindgen]
pub fn vectorize_product(
    bytes: &[u8],
    mode: &str,
    preset: &str,
    experimental: bool,
) -> Result<JsValue, JsValue> {
    ensure_input(bytes)?;
    let request = vice_core::ProductRequest {
        mode: parse_mode(mode)?,
        preset: parse_preset(preset)?,
        experimental_artifacts: experimental,
        ..vice_core::ProductRequest::default()
    };
    let product = vice_core::vectorize_product(bytes, &request);
    let report = serde_json::to_value(&product.report).map_err(js_error)?;
    let manifest = product
        .artifacts
        .artifact_manifest_json
        .as_deref()
        .map(serde_json::from_slice)
        .transpose()
        .map_err(js_error)?;
    let result = WasmProductResult {
        schema: WASM_PRODUCT_RESULT_SCHEMA,
        status: product.report.status,
        selected_lane: product.report.selected_lane,
        route_reason: product.report.route_reason,
        production: product.report.production,
        experimental_artifacts: product.report.experimental_artifacts,
        report,
        result_svg: utf8_artifact(product.artifacts.result_svg)?,
        pure_partition_svg: utf8_artifact(product.artifacts.pure_partition_svg)?,
        scene_json: utf8_artifact(product.artifacts.scene_json)?,
        render_png: product.artifacts.render_png,
        artifact_manifest: manifest,
    };
    result
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(js_error)
}

#[wasm_bindgen]
pub fn classify_gradient(bytes: &[u8]) -> Result<JsValue, JsValue> {
    ensure_input(bytes)?;
    let result = vice_core::classify_m11_gradient(bytes).map_err(js_error)?;
    serde_wasm_bindgen::to_value(&WasmGradientResult {
        schema: WASM_RESULT_SCHEMA,
        report: result.report,
        scene_json: String::from_utf8(result.selected_scene_json).map_err(js_error)?,
        render_rgba: result.selected_straight_srgb8,
    })
    .map_err(js_error)
}

fn ensure_input(bytes: &[u8]) -> Result<(), JsValue> {
    if bytes.len() > MAX_WASM_INPUT_BYTES {
        Err(JsValue::from_str(
            "encoded input exceeds the 64 MiB WASM boundary",
        ))
    } else {
        Ok(())
    }
}

fn parse_preset(value: &str) -> Result<vice_core::Preset, JsValue> {
    match value {
        "fast" => Ok(vice_core::Preset::Fast),
        "quality" => Ok(vice_core::Preset::Quality),
        _ => Err(JsValue::from_str("preset must be 'fast' or 'quality'")),
    }
}

fn parse_mode(value: &str) -> Result<vice_core::ProductMode, JsValue> {
    match value {
        "auto" => Ok(vice_core::ProductMode::Auto),
        "flat2" => Ok(vice_core::ProductMode::Flat2),
        "multiregion" => Ok(vice_core::ProductMode::Multiregion),
        "line-art" | "line_art" => Ok(vice_core::ProductMode::LineArt),
        "gradient" => Ok(vice_core::ProductMode::Gradient),
        _ => Err(JsValue::from_str(
            "mode must be 'auto', 'flat2', 'multiregion', 'line-art', or 'gradient'",
        )),
    }
}

fn utf8_artifact(bytes: Option<Vec<u8>>) -> Result<Option<String>, JsValue> {
    bytes.map(String::from_utf8).transpose().map_err(js_error)
}

fn js_error(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_adapter_has_a_bounded_input_contract() {
        assert_eq!(MAX_WASM_INPUT_BYTES, 64 * 1024 * 1024);
        assert_eq!(parse_preset("fast").unwrap(), vice_core::Preset::Fast);
        assert_eq!(parse_preset("quality").unwrap(), vice_core::Preset::Quality);
        assert_eq!(parse_mode("auto").unwrap(), vice_core::ProductMode::Auto);
        assert_eq!(
            parse_mode("multiregion").unwrap(),
            vice_core::ProductMode::Multiregion
        );
    }

    #[test]
    fn browser_source_calls_the_routed_contract_and_exposes_route_fields() {
        let app = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/app.js"),
        )
        .unwrap();
        assert!(app.contains("vectorize_product"));
        assert!(app.contains("selected_lane"));
        assert!(app.contains("experimental_artifacts"));
        assert!(!app.contains("vectorize_flat2("));
    }
}
