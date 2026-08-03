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
let startedAt;
let elapsedTimer;
let watchdogTimer;
let currentPhase = "Preparing input";
let workerReady = false;
let workerBusy = false;
let nextReadyMessage = "Ready. Choose an image.";
let worker;

const MAX_RUN_MS = 15_000;

function elapsedSeconds() {
  return startedAt ? ((performance.now() - startedAt) / 1000).toFixed(1) : "0.0";
}

function showWorking(phase) {
  currentPhase = phase;
  status.classList.add("working");
  status.textContent = `${currentPhase}… ${elapsedSeconds()} s`;
}

function startTimer() {
  clearInterval(elapsedTimer);
  elapsedTimer = setInterval(() => showWorking(currentPhase), 100);
}

function finishTimer() {
  clearInterval(elapsedTimer);
  clearTimeout(watchdogTimer);
  elapsedTimer = undefined;
  watchdogTimer = undefined;
  status.classList.remove("working");
}

function clearOutput() {
  if (resultUrl) {
    URL.revokeObjectURL(resultUrl);
    resultUrl = undefined;
  }
  preview.removeAttribute("src");
  preview.alt = "";
  download.removeAttribute("href");
  result.hidden = true;
  route.textContent = "";
  report.textContent = "";
}

function showOutput(output) {
  finishTimer();
  workerBusy = false;
  run.disabled = !workerReady;
  clearOutput();
  report.textContent = JSON.stringify(output.report, null, 2);
  diagnostics.open = output.status !== "success";
  status.textContent = `Completed in ${elapsedSeconds()} s — outcome: ${output.status}`;
  route.textContent = `Lane: ${output.selected_lane}. ${output.route_reason}. ${output.report.message}`;
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
}

function showFailure(message) {
  finishTimer();
  workerBusy = false;
  run.disabled = !workerReady;
  status.textContent = `Failed after ${elapsedSeconds()} s — ${message}`;
}

function startWorker() {
  worker?.terminate();
  workerReady = false;
  run.disabled = true;
  worker = new Worker(new URL("./worker.js", import.meta.url), { type: "module" });
  worker.addEventListener("message", ({ data }) => {
    if (data.type === "ready") {
      workerReady = true;
      run.disabled = workerBusy;
      status.textContent = nextReadyMessage;
      nextReadyMessage = "Ready. Choose an image.";
    } else if (data.type === "phase") {
      showWorking(data.message);
    } else if (data.type === "result") {
      showOutput(data.output);
    } else if (data.type === "error") {
      showFailure(data.message);
    }
  });
  worker.addEventListener("error", (error) => {
    workerReady = false;
    showFailure(`background vectorizer crashed: ${error.message}`);
  });
}

function armWatchdog() {
  clearTimeout(watchdogTimer);
  watchdogTimer = setTimeout(() => {
    if (!workerBusy) return;
    workerReady = false;
    worker.terminate();
    showFailure("stopped by the 15 s safety limit; no more multi-minute hangs");
    nextReadyMessage = "Previous run was stopped at 15 s. Ready for another image.";
    startWorker();
  }, MAX_RUN_MS);
}

run.disabled = true;
status.textContent = "Loading vectorizer…";
startWorker();

run.addEventListener("click", async () => {
  const file = input.files?.[0];
  if (!file) {
    status.textContent = "Choose an image first.";
    return;
  }
  if (!workerReady || workerBusy) return;

  workerBusy = true;
  run.disabled = true;
  clearOutput();
  startedAt = performance.now();
  showWorking("Reading image");
  startTimer();
  armWatchdog();

  try {
    const bytes = await file.arrayBuffer();
    showWorking("Routing and vectorizing");
    worker.postMessage(
      {
        type: "vectorize",
        bytes,
        mode: mode.value,
        preset: preset.value,
        experimental: experimental.checked,
      },
      [bytes],
    );
  } catch (error) {
    showFailure(error instanceof Error ? error.message : String(error));
  }
});
