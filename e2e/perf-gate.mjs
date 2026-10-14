// Engine gate (PRD P0): 2,000 tiles + full post stack at 1440p, GPU Chromium. Local only.
import { chromium } from "playwright";
import { createServer } from "node:http";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { extname, join } from "node:path";
import { LOAD_LIMIT, guarded } from "./perf-load.mjs";
import { BAD_TEXT } from "./console-errors.mjs";

const dist = process.argv[2] ?? "dist";
const types = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm", ".css": "text/css",
  ".ktx2": "image/ktx2", ".glb": "model/gltf-binary", ".png": "image/png", ".wgsl": "text/plain" };
const server = createServer(async (req, res) => {
  let path = decodeURIComponent(new URL(req.url, "http://x").pathname);
  if (path.endsWith("/")) path += "index.html";
  try {
    const body = await readFile(join(dist, path));
    res.writeHead(200, { "content-type": types[extname(path)] ?? "application/octet-stream" });
    res.end(body);
  } catch { res.writeHead(404); res.end(); }
}).listen(0);
// PERF_FPS=off hides the FPS overlay (default on); PERF_OUT redirects the JSON for A/B runs.
const fpsOverlay = process.env.PERF_FPS === "off" ? "off" : "on";
const outPath = process.env.PERF_OUT ?? "docs/perf/p0-engine-gate.json";
const base = `http://127.0.0.1:${server.address().port}/`;

// Uncapped: a vsync-locked 60 Hz run reports p95 ≈ 16.67 ms and would read as a false failure.
const browser = await chromium.launch({ channel: "chromium", headless: true,
  args: ["--use-angle=metal", "--enable-gpu", "--ignore-gpu-blocklist", "--enable-unsafe-webgpu",
         "--disable-gpu-vsync", "--disable-frame-rate-limit"] });
const errors = [];
const views = {};
let ua = "";
let gpu = null;
const measureView = async (view) => {
  const page = await browser.newPage({ viewport: { width: 2560, height: 1440 }, deviceScaleFactor: 1 });
  page.on("console", (m) => { if (m.type() === "error" || BAD_TEXT.test(m.text())) errors.push(`[${view}] ${m.text()}`); });
  page.on("pageerror", (e) => errors.push(`[${view}] ${e}`));
  await page.goto(base + `?scenario=gate2000&view=${view}&fps=${fpsOverlay}`);
  await page.waitForFunction(() => window.hexWorld?.ready(), null, { timeout: 120_000 });
  if (!gpu) {
    gpu = await page.evaluate(async () => {
      const a = await navigator.gpu.requestAdapter();
      if (!a) return null;
      const i = a.info;
      return { adapter: [i.vendor, i.architecture, i.description].join(" | "),
        max_storage_textures_per_shader_stage: a.limits.maxStorageTexturesPerShaderStage };
    });
    // A SwiftShader adapter would measure the CPU, not the GPU: refuse to report it.
    if (!gpu || /swiftshader/i.test(gpu.adapter)) {
      console.error("FAIL: no hardware WebGPU adapter:", JSON.stringify(gpu));
      await browser.close();
      server.close();
      process.exit(1);
    }
  }
  // Wait for a full 600-frame window that starts after READY (diag refreshes every 30 frames).
  await page.waitForFunction(() => {
    const d = window.hexWorld.diag();
    return d.window_start_frame > 0 && d.samples >= 600 && d.frames - d.window_start_frame + 1 >= 600;
  }, null, { timeout: 180_000, polling: 250 });
  const d = await page.evaluate(() => window.hexWorld.diag());
  if (view === "strategic") {
    // Review Focus 5: resize mid-run must not error.
    await page.setViewportSize({ width: 1600, height: 900 });
    await page.waitForTimeout(2_000);
    await page.setViewportSize({ width: 2560, height: 1440 });
    await page.waitForTimeout(2_000);
  }
  ua = await page.evaluate(() => navigator.userAgent);
  await page.close();
  return d;
};
// Host-load guard (owner rule): load <= PERF_LOAD_LIMIT before and after each view's run.
for (const view of ["strategic", "closeup"]) {
  const g = await guarded(() => measureView(view), `perf-gate ${view}`);
  views[view] = { ...g.result, load_before: g.load_before, load_after: g.load_after,
    load_discarded: g.load_discarded };
}
await browser.close();
server.close();

// Describes what ran, read back from the app; budgets sit beside the measured values.
const s = views.strategic;
// `boids` is what the GPU compute pass dispatched in the sampled frame, not the configured count.
const workload = { tiles: s.tiles, pieces: s.pieces, mesh_entities: s.entities, vehicles: s.vehicles,
  boids: s.boids ?? 0, tris_lod0_per_tile: s.tris_lod0_per_tile, materials: s.materials, post: s.post,
  not_measured: (s.boids ?? 0) > 0 ? [] : ["boids"], configured: { shadow_cascades: 4, lod_switch_m: [150, 400] },
  budget: { tiles: 2000, pieces: 40000, tris_lod0_per_tile: 15700, moving: 300, boids: 1000 } };
const fullWindow = (d) => d.samples >= 600 && d.window_start_frame > 0 && d.frames - d.window_start_frame + 1 >= 600;
const ok = (d) => d.tiles === 2000 && d.pieces >= 40000 && d.ssao === true && fullWindow(d) && d.p95 <= 16.6;
const vsyncLike = (d) => d.p50 >= 16.5 && d.p50 <= 16.8;
const vsync_suspected = vsyncLike(views.strategic) && vsyncLike(views.closeup);
const thin = Object.entries(views).filter(([, d]) => !fullWindow(d)).map(([v]) => v);
const result = { date: new Date().toISOString(), browser: "chromium-headless-gpu-uncapped", ua, viewport: "2560x1440", fps_overlay: fpsOverlay,
  webgpu_adapter: gpu.adapter, max_storage_textures_per_shader_stage: gpu.max_storage_textures_per_shader_stage,
  budget_p95_ms: 16.6, load_limit: LOAD_LIMIT, workload, views, console_errors: errors, vsync_suspected, thin_sample_views: thin,
  pass: ok(views.strategic) && ok(views.closeup) && errors.length === 0 };
await mkdir("docs/perf", { recursive: true });
await writeFile(outPath, JSON.stringify(result, null, 2) + "\n");
console.log(JSON.stringify(result, null, 2));
if (vsync_suspected) console.error("WARN: p50 sits at the 60 Hz frame time in both views; uncapping did not take effect");
if (errors.length) console.error("FAIL: console errors");
if (!result.pass) process.exitCode = 1;
