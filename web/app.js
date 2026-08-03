import init, { vectorize_flat2 } from "./pkg/vice_wasm.js";

const input = document.querySelector("#input");
const preset = document.querySelector("#preset");
const run = document.querySelector("#run");
const status = document.querySelector("#status");
const result = document.querySelector("#result");
const preview = document.querySelector("#preview");
const download = document.querySelector("#download");
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
  status.textContent = "Vectorizing…";
  try {
    const output = vectorize_flat2(new Uint8Array(await file.arrayBuffer()), preset.value);
    report.textContent = JSON.stringify(output.report, null, 2);
    const reason = output.report?.reason;
    status.textContent = reason
      ? `Outcome: ${output.status} — ${reason.reason}: ${reason.detail}`
      : `Outcome: ${output.status}`;
    if (output.result_svg) {
      if (resultUrl) URL.revokeObjectURL(resultUrl);
      resultUrl = URL.createObjectURL(new Blob([output.result_svg], { type: "image/svg+xml" }));
      preview.src = resultUrl;
      download.href = resultUrl;
      result.hidden = false;
    }
  } catch (error) {
    status.textContent = `Failed: ${error}`;
  } finally {
    run.disabled = false;
  }
});
