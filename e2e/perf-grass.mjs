// Grass blade cost (crates/hex-render/src/play/grass.rs) on a ~2,000-tile play world: the water world of
// e2e/water-world.mjs at radius 26 (2,066 tiles: ocean, coast, rivers, forests, meadows, roads and
// houses) built by script in the play scenario on High at 2560x1440, DPR 1, tilt-shift off, then read
// from a low camera over the start meadows (the grass close-up the owner asked for, with the rest of
// the world to the horizon) once every placement has risen and settled. Web only: uncapped GPU
// Chromium with the perf-passes flags and the HUD overlay live (native play is pinned to 60 Hz, so its
// p95 is pacing). Arms `grass=off`, `normal` and `lush` on the same build, interleaved rounds, every run
// inside the host-load guard. The PRD budget (p95 <= 16.6 ms) is set on gate2000, which is not a play
// world and draws no grass; this is the play world of that size.
// Local only. usage: node e2e/perf-grass.mjs <dist>
//   PERF_OUT (default docs/perf/p2-grass.json), GRASS_ROUNDS (default 2), GRASS_RADIUS (default 26),
//   GRASS_ARMS (default off,normal,lush), GRASS_TRACE (optional Chrome trace + CPU profile path),
//   GRASS_GPU_TIMING=on (diagnostic only, not budget runs), GRASS_SAVE (optional exact-world save)
import { chromium } from "playwright";
import { createServer } from "node:http";
import { readFile, writeFile } from "node:fs/promises";
import { extname, join } from "node:path";
import { LOAD_LIMIT, guarded } from "./perf-load.mjs";
import { BAD_TEXT } from "./console-errors.mjs";
import { waterWorldScript } from "./water-world.mjs";
import { buildWorld } from "./build-world.mjs";

const dist = process.argv[2] ?? "dist";
const OUT = process.env.PERF_OUT ?? "docs/perf/p2-grass.json";
const ROUNDS = Number(process.env.GRASS_ROUNDS ?? 2);
const RADIUS = Number(process.env.GRASS_RADIUS ?? 26);
const ARMS = (process.env.GRASS_ARMS ?? "off,normal,lush").split(",");
const W = 2560, H = 1440;
const SETTLE_FRAMES = 700;
const world = waterWorldScript(RADIUS);
const CAMERA = { cmd: "camera", pos: [10, 12, 40], look_at: [0, 0, 0] };
const KEYS = { scenario: "play", tier: "high", fps: "off", seed: "1", tilt: "off",
  ...(process.env.GRASS_GPU_TIMING === "on" ? { gpu_timing: "on" } : {}) };

const types = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm", ".css": "text/css",
  ".ktx2": "image/ktx2", ".glb": "model/gltf-binary", ".png": "image/png", ".wgsl": "text/plain",
  ".jpg": "image/jpeg", ".woff2": "font/woff2", ".json": "application/json", ".webm": "audio/webm", ".m4a": "audio/mp4" };
const server = createServer(async (req, res) => {
  let path = decodeURIComponent(new URL(req.url, "http://x").pathname);
  if (path.endsWith("/")) path += "index.html";
  try {
    const body = await readFile(join(dist, path));
    res.writeHead(200, { "content-type": types[extname(path)] ?? "application/octet-stream" });
    res.end(body);
  } catch { res.writeHead(404); res.end(); }
}).listen(0);
const base = `http://127.0.0.1:${server.address().port}/`;

const pick = (d) => ({ p50: d.p50, p95: d.p95, p99: d.p99, mean: d.mean, max: d.max, dropped_over_25ms: d.dropped_over_25ms,
  samples: d.samples, frames: d.frames, window_start_frame: d.window_start_frame, ssao: d.ssao, post: d.post,
  adapter: d.adapter, wasm_memory_bytes: d.wasm_memory_bytes, visible_tris: d.visible_tris,
  stall: d.stall && { histogram: d.stall.histogram, over_25ms: d.stall.over_25ms, tail_ms: d.stall.tail.map((s) => s.ms) } });

async function webRun(arm) {
  const browser = await chromium.launch({ channel: "chromium", headless: true,
    args: ["--use-angle=metal", "--enable-gpu", "--ignore-gpu-blocklist", "--enable-unsafe-webgpu",
      "--disable-gpu-vsync", "--disable-frame-rate-limit", "--enable-webgpu-developer-features"] });
  const errors = [];
  try {
    const page = await browser.newPage({ viewport: { width: W, height: H }, deviceScaleFactor: 1 });
    page.on("console", (m) => { if (m.type() === "error" || BAD_TEXT.test(m.text())) errors.push(m.text()); });
    page.on("pageerror", (e) => errors.push(String(e)));
    await page.goto(base + "?" + new URLSearchParams({ ...KEYS, grass: arm }));
    await page.waitForFunction(() => window.hexWorld?.ready(), null, { timeout: 120_000 });
    const adapter = await page.evaluate(async () => {
      const i = (await navigator.gpu.requestAdapter())?.info;
      return i ? [i.vendor, i.architecture, i.description].join(" | ") : null;
    });
    if (!adapter || /swiftshader/i.test(adapter)) throw new Error("no hardware WebGPU adapter: " + adapter);
    const renderer = await page.evaluate(() => {
      const gl = document.createElement("canvas").getContext("webgl");
      const ext = gl?.getExtension("WEBGL_debug_renderer_info");
      return ext ? gl.getParameter(ext.UNMASKED_RENDERER_WEBGL) : null;
    });
    if (!renderer || /swiftshader/i.test(renderer)) throw new Error("no hardware WebGL renderer: " + renderer);
    await buildWorld(page, [...world.script, CAMERA], world.tiles.length);
    const f0 = await page.evaluate(() => window.hexWorld.diag().frames);
    await page.waitForFunction((f) => window.hexWorld.diag().frames >= f, f0 + SETTLE_FRAMES, { timeout: 240_000, polling: 250 });
    const d = await page.evaluate(() => window.hexWorld.diag());
    const st = await page.evaluate(() => window.hexWorld.state());
    if (st.tier?.tier !== "high") throw new Error(`tier ${st.tier?.tier}, expected high`);
    const share = { off: 0, sparse: 0.5, normal: 1, lush: 1.5 }[arm] ?? Number(arm);
    if (st.grass?.density !== share) throw new Error(`grass density ${st.grass?.density}, expected ${share} (${arm})`);
    const n = st.play?.tiles?.length;
    if (n !== world.tiles.length) throw new Error(`world has ${n} tiles, expected ${world.tiles.length}`);
    if (st.tilt?.enabled || st.play?.rising?.length) throw new Error("world must be settled with tilt off");
    if (d.samples !== 600) throw new Error(`incomplete diagnostic window: ${d.samples}`);
    const grass = { status: st.grass.status, groups: st.grass.groups, blades: st.grass.blades, mesh_bytes: st.grass.mesh_bytes };
    let trace = null;
    if (process.env.GRASS_TRACE) {
      const cdp = await page.context().newCDPSession(page);
      await cdp.send("Profiler.enable");
      await cdp.send("Profiler.setSamplingInterval", { interval: 1000 });
      await cdp.send("Profiler.start");
      const events = [];
      cdp.on("Tracing.dataCollected", (e) => events.push(...e.value));
      const complete = new Promise((resolve) => cdp.once("Tracing.tracingComplete", resolve));
      await cdp.send("Tracing.start", { transferMode: "ReportEvents", traceConfig: {
        includedCategories: ["devtools.timeline", "v8", "v8.execute", "v8.wasm", "gpu", "gpu.dawn", "gpu.service", "webgpu", "toplevel", "disabled-by-default-v8.cpu_profiler"],
        recordMode: "recordContinuously", enableSampling: true,
      } });
      const first = await page.evaluate(() => window.hexWorld.diag().frames);
      await page.waitForTimeout(2000);
      const last = await page.evaluate(() => window.hexWorld.diag().frames);
      await cdp.send("Tracing.end");
      await complete;
      const profile = await cdp.send("Profiler.stop");
      await writeFile(process.env.GRASS_TRACE + ".cpuprofile", JSON.stringify(profile.profile));
      await writeFile(process.env.GRASS_TRACE, JSON.stringify({ traceEvents: events }));
      trace = { path: process.env.GRASS_TRACE, events: events.length, frames: [first, last], instrumented: true };
    }
    if (process.env.GRASS_SAVE) {
      const bytes = await page.evaluate(() => Array.from(window.hexWorld.saveNow(null)));
      await writeFile(process.env.GRASS_SAVE, new Uint8Array(bytes));
    }
    const build_id = await page.evaluate(() => window.__HEX_BUILD ?? null);
    return { platform: "web", arm, build_id, browser_version: browser.version(), renderer, trace, webgpu_adapter: adapter, tiles: n,
      camera: st.camera, rising: st.play?.rising?.length ?? 0, settled_at_frame: f0, grass, ...pick(d), gpu: d.gpu, console_errors: errors };
  } finally {
    await browser.close();
  }
}

const result = { date: new Date().toISOString(), viewport: `${W}x${H}`, device_scale_factor: 1, keys: KEYS, camera: CAMERA,
  world: { generator: `e2e/water-world.mjs waterWorldScript(${RADIUS})`, tiles: world.tiles.length, counts: world.counts },
  budget_p95_ms: 16.6, load_limit: LOAD_LIMIT, rounds: ROUNDS, arms: ARMS,
  load_rule: "1-min load average <= load_limit immediately before and after every run; over-limit samples discarded and re-run",
  web: { browser: "chromium-headless-gpu-uncapped", channel: "chromium",
    args: "--use-angle=metal --enable-gpu --ignore-gpu-blocklist --enable-unsafe-webgpu --disable-gpu-vsync --disable-frame-rate-limit --enable-webgpu-developer-features" },
  rows: [] };
try {
  for (let r = 0; r < ROUNDS; r++)
    for (const arm of r % 2 ? [...ARMS].reverse() : ARMS) {
      const label = `grass web round ${r} grass=${arm}`;
      const g = await guarded(() => webRun(arm).catch((e) => ({ platform: "web", arm, error: String(e.message ?? e) })), label);
      result.rows.push({ round: r, ...g.result, load_before: g.load_before, load_after: g.load_after, load_discarded: g.load_discarded });
      const x = result.rows.at(-1);
      console.log(`${label}: ${x.error ?? `p50 ${x.p50} p95 ${x.p95} mean ${x.mean} max ${x.max} over25 ${x.dropped_over_25ms} ` +
        `grass ${JSON.stringify(x.grass)}`} (load ${g.load_before.toFixed(2)} -> ${g.load_after.toFixed(2)})`);
      await writeFile(OUT, JSON.stringify(result, null, 2) + "\n");
    }
} finally {
  server.close();
}
const median = (v) => { const s = v.filter((x) => typeof x === "number").sort((a, b) => a - b); return s.length ? s[Math.floor(s.length / 2)] : null; };
result.summary = {};
for (const arm of ARMS) {
  const rows = result.rows.filter((x) => x.arm === arm && !x.error);
  if (!rows.length) continue;
  const p95 = median(rows.map((x) => x.p95));
  result.summary[`web_grass_${arm}`] = { p95_median: p95, mean_median: median(rows.map((x) => x.mean)),
    over_25ms_median: median(rows.map((x) => x.dropped_over_25ms)), pass: p95 !== null && p95 <= 16.6, runs: rows.length };
}
await writeFile(OUT, JSON.stringify(result, null, 2) + "\n");
console.log(JSON.stringify(result.summary, null, 2));
console.log(`wrote ${OUT}`);
