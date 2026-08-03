import init, { vectorize_product } from "./pkg/vice_wasm.js";

try {
  await init();
  self.postMessage({ type: "ready" });
} catch (error) {
  self.postMessage({
    type: "error",
    message: `WASM initialization failed: ${error instanceof Error ? error.message : String(error)}`,
  });
}

self.addEventListener("message", ({ data }) => {
  if (data.type !== "vectorize") return;
  self.postMessage({ type: "phase", message: "Routing and vectorizing" });
  try {
    const output = vectorize_product(
      new Uint8Array(data.bytes),
      data.mode,
      data.preset,
      data.experimental,
    );
    self.postMessage({ type: "result", output });
  } catch (error) {
    self.postMessage({
      type: "error",
      message: error instanceof Error ? error.message : String(error),
    });
  }
});
