import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import http from "node:http";
import path from "node:path";
import { fileURLToPath } from "node:url";

const webRoot = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(webRoot, "..");
const host = "127.0.0.1";
const port = Number.parseInt(process.env.VICE_WEB_PORT ?? "8765", 10);
const origin = `http://${host}:${port}`;
const noOpen = process.argv.includes("--no-open");

if (!Number.isInteger(port) || port < 1 || port > 65535) {
  throw new Error("VICE_WEB_PORT must be an integer from 1 to 65535");
}

const wasmPackage = path.join(webRoot, "pkg", "vice_wasm.js");
if (!fs.existsSync(wasmPackage)) {
  console.log("Browser engine is missing; building it once…");
  const built = spawnSync(
    "wasm-pack",
    ["build", "crates/vice-wasm", "--release", "--target", "web", "--out-dir", "../../web/pkg"],
    { cwd: repoRoot, stdio: "inherit", shell: process.platform === "win32" },
  );
  if (built.status !== 0) {
    throw new Error("wasm-pack build failed; install wasm-pack 0.13.1 and retry");
  }
}

const contentTypes = new Map([
  [".css", "text/css; charset=utf-8"],
  [".html", "text/html; charset=utf-8"],
  [".js", "text/javascript; charset=utf-8"],
  [".json", "application/json; charset=utf-8"],
  [".mjs", "text/javascript; charset=utf-8"],
  [".png", "image/png"],
  [".svg", "image/svg+xml"],
  [".wasm", "application/wasm"],
]);

function openBrowser(url) {
  if (noOpen) return;
  const command =
    process.platform === "win32"
      ? ["cmd", ["/c", "start", "", url]]
      : process.platform === "darwin"
        ? ["open", [url]]
        : ["xdg-open", [url]];
  const child = spawn(command[0], command[1], {
    detached: true,
    stdio: "ignore",
    windowsHide: true,
  });
  child.unref();
}

const server = http.createServer((request, response) => {
  let pathname;
  try {
    pathname = decodeURIComponent(new URL(request.url ?? "/", origin).pathname);
  } catch {
    response.writeHead(400).end("Bad request");
    return;
  }
  const relative = pathname === "/" ? "index.html" : pathname.slice(1);
  const file = path.resolve(webRoot, relative);
  if (file !== webRoot && !file.startsWith(`${webRoot}${path.sep}`)) {
    response.writeHead(403).end("Forbidden");
    return;
  }
  fs.readFile(file, (error, bytes) => {
    if (error) {
      response.writeHead(error.code === "ENOENT" ? 404 : 500).end("Not found");
      return;
    }
    response.writeHead(200, {
      "Cache-Control": "no-store",
      "Content-Type": contentTypes.get(path.extname(file)) ?? "application/octet-stream",
    });
    response.end(bytes);
  });
});

server.on("error", (error) => {
  if (error.code === "EADDRINUSE") {
    console.log(`vice-classic already appears to be running at ${origin}`);
    openBrowser(origin);
    process.exit(0);
  }
  throw error;
});

server.listen(port, host, () => {
  console.log(`vice-classic is running at ${origin}`);
  console.log("Press Ctrl+C to stop it.");
  openBrowser(origin);
});
