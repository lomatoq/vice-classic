import fs from "node:fs";
import init, { vectorize_flat2, vectorize_product } from "./pkg/vice_wasm.js";

await init({
  module_or_path: fs.readFileSync(new URL("./pkg/vice_wasm_bg.wasm", import.meta.url)),
});

if (typeof vectorize_product !== "function" || typeof vectorize_flat2 !== "function") {
  throw new Error("WASM product exports are incomplete");
}

const input = new Uint8Array(
  fs.readFileSync(new URL("../tests/fixtures/smoke/circle_64.png", import.meta.url)),
);
const result = vectorize_product(input, "gradient", "fast", true);

if (
  result.schema !== "vice-classic/wasm-product-result/v1" ||
  result.status !== "ambiguous" ||
  result.selected_lane !== "gradient" ||
  result.production !== false ||
  result.experimental_artifacts !== true ||
  result.report?.schema !== "vice-classic/product-vectorize-report/v1" ||
  result.report?.artifact_trust !== "non_production_manual_inspection_only" ||
  typeof result.scene_json !== "string" ||
  !Array.isArray(result.render_png) ||
  result.render_png.length === 0 ||
  result.artifact_manifest?.experimental !== true ||
  result.artifact_manifest?.production !== false
) {
  throw new Error(`routed WASM serialization smoke failed: ${JSON.stringify(result)}`);
}
