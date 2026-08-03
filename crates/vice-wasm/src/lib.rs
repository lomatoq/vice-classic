//! M12 WASM adapter. All inference and release policy remain in `vice-core`.

#![forbid(unsafe_code)]

use serde::Serialize;
use wasm_bindgen::prelude::*;

pub const WASM_RESULT_SCHEMA: &str = "vice-classic/wasm-result/v1";
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
    serde_wasm_bindgen::to_value(&result).map_err(js_error)
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
    }
}
