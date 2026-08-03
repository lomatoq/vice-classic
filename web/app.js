import init, { vectorize_product } from "./pkg/vice_wasm.js";

const input = document.querySelector("#input");
const mode = document.querySelector("#mode");
const preset = document.querySelector("#preset");
const experimental = document.querySelector("#experimental");
const run = document.querySelector("#run");
const status = document.querySelector("#status");
const route = document.querySelector("#route");
const result = document.querySelector("#result");
const preview = document.querySelector("#preview");
const download = document.querySelector("#download");
const diagnostics = document.querySelector("#diagnostics");
const report = document.querySelector("#report");
let resultUrl;

try {
  await init();
} catch (error) {
  run.disabled = true;
  status.textContent = `WASM initialization failed: ${error}`;
  throw error;
}

run.addEventListener("click", async () => {
  const file = input.files?.[0];
  if (!file) {
    status.textContent = "Choose an image first.";
    return;
  }
  run.disabled = true;
  result.hidden = true;
  route.textContent = "";
  report.textContent = "";
  status.textContent = "Inspecting and vectorizing…";
  try {
    // Let the busy state paint before entering the synchronous WASM call.
    await new Promise((resolve) => requestAnimationFrame(resolve));
    const output = vectorize_product(
      new Uint8Array(await file.arrayBuffer()),
      mode.value,
      preset.value,
      experimental.checked,
    );
    report.textContent = JSON.stringify(output.report, null, 2);
    diagnostics.open = output.status !== "success";
    status.textContent = `Outcome: ${output.status}`;
    route.textContent = `Lane: ${output.selected_lane}. ${output.route_reason}. ${output.report.message}`;

    if (resultUrl) {
      URL.revokeObjectURL(resultUrl);
      resultUrl = undefined;
    }
    if (output.result_svg) {
      resultUrl = URL.createObjectURL(
        new Blob([output.result_svg], { type: "image/svg+xml" }),
      );
      preview.src = resultUrl;
      preview.alt = `${output.selected_lane} SVG result preview`;
      download.href = resultUrl;
      download.download = output.experimental_artifacts
        ? "result.experimental.svg"
        : "result.svg";
      download.textContent = output.experimental_artifacts
        ? "Download experimental SVG"
        : "Download SVG";
      result.hidden = false;
    } else if (output.render_png) {
      resultUrl = URL.createObjectURL(
        new Blob([new Uint8Array(output.render_png)], { type: "image/png" }),
      );
      preview.src = resultUrl;
      preview.alt = `${output.selected_lane} rendered result preview`;
      download.href = resultUrl;
      download.download = "result.experimental.render.png";
      download.textContent = "Download experimental render";
      result.hidden = false;
    }
  } catch (error) {
    status.textContent = `Failed: ${error}`;
  } finally {
    run.disabled = false;
  }
});
