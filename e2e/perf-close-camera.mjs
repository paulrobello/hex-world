// Close-camera draw-call work A/B on the web (docs/perf/close-camera-draws/README.md): two built
// dists, the 20-tile scatter world (docs/checkpoints/lod-band-instanced/draws.json placements) at its
// overview, forest-cluster, meadow and 152 m band cameras, and the exact 2,066-tile save
// (docs/perf/post-arc001-2026-10-16/local-world.hexworld) at its close camera and an overview.
// Uncapped GPU Chromium, 2560x1440, DPR 1, scenario=play tier=high tilt=off fps=off grass=off
// seed=1. Each run is a fresh browser on one build and one world; every camera window is read
// 700 frames after the camera move, so the 600-frame diag ring holds only it. Builds alternate
// order each round; every run sits inside e2e/perf-load.mjs's guard (1-minute load <= 15).
// The 31-tile mixed world (docs/checkpoints/scatter/scatter-mixed.json) runs at its every-tile overview and
// the 20-tile overview pose.
// usage: node e2e/perf-close-camera.mjs <dist-before> <dist-after> <out.json>   ROUNDS (default 5), WORLDS (default all)
//    or: node e2e/perf-close-camera.mjs <name>=<dist>[?k=v&...] ... <out.json>   any number of arms, each a dist
//        and extra page keys (e.g. base=dist no_o4=dist?kit_ablate=coalesce); round r runs the arms rotated by r.
// WARMUP=1 runs the first arm once on the first world before round 0 and discards it (a session's first run
// can read fast). Each camera window also records `cpu`: the main-thread time of each engine rAF callback
// that submitted GPU work, over the window's last 600 frames, so a delta splits into CPU and the rest.
// An arm key `pause_agents=1` opens photo mode with the agents paused (no sim tick). GPU_CALLS=1 and
// PROFILE_DIR are diagnostics (docs/perf/large-world-cpu/), not timing evidence.
import { createServer } from "node:http";
import { readFile, writeFile } from "node:fs/promises";
import { extname, join } from "node:path";
import { cpus, hostname, loadavg } from "node:os";
import { chromium } from "playwright";
import { buildWorld } from "./build-world.mjs";
import { BAD_TEXT } from "./console-errors.mjs";
import { guarded, LOAD_LIMIT } from "./perf-load.mjs";

const argv = process.argv.slice(2);
const OUT = argv.at(-1);
if (argv.length < 2) throw new Error("usage: node e2e/perf-close-camera.mjs <dist-before> <dist-after> <out.json>");
// Legacy two positional dists are the arms `before` and `after`.
const legacy = argv.slice(0, -1).every((a) => !a.includes("="));
if (legacy && argv.length !== 3) throw new Error("more than two arms need name=dist");
const ARMS = legacy ? [["before", argv[0], ""], ["after", argv[1], ""]] : argv.slice(0, -1).map((a) => {
  const [name, rest] = [a.slice(0, a.indexOf("=")), a.slice(a.indexOf("=") + 1)];
  const [dist, query = ""] = rest.split("?");
  return [name, dist, query];
});
if (new Set(ARMS.map(([n]) => n)).size !== ARMS.length) throw new Error("arm names must differ");
const WARMUP = process.env.WARMUP === "1";
const ROUNDS = Number(process.env.ROUNDS ?? 5);
const SAVE = process.env.SAVE ?? "docs/perf/post-arc001-2026-10-16/local-world.hexworld";
const W = 2560;
const H = 1440;
const WINDOW_FRAMES = 700;
const SETTLE_FRAMES = 300;
const KEYS = { scenario: "play", tier: "high", tilt: "off", fps: "off", grass: "off", seed: "1" };
const ARGS = ["--use-angle=metal", "--enable-gpu", "--ignore-gpu-blocklist", "--enable-unsafe-webgpu",
  "--disable-gpu-vsync", "--disable-frame-rate-limit", ...(process.env.CHROME_ARGS ?? "").split(" ").filter(Boolean)];
// HOLD_MS: after each camera's window, log `hold <arm> <world> <camera>` and keep the camera for that long,
// so scripts/perf/large-world-trace.sh can record a Metal System Trace inside it.
const HOLD_MS = Number(process.env.HOLD_MS ?? 0);

const place = (list) => list.flatMap(([id, cells]) => [{ cmd: "select_tile", id }, ...cells.map(([q, r]) => ({ cmd: "place", q, r }))]);
const world20 = place([["forest", [[1, 0]]], ["oak", [[2, 0]]], ["forest", [[1, -1], [2, -1]]], ["oak", [[3, -1]]],
  ["forest", [[3, 0], [0, -1]]], ["meadow", [[-1, 0]]], ["river_straight", [[-1, -1]]], ["road_straight", [[1, -2], [2, -2]]],
  ["house_cluster", [[3, -2]]], ["meadow", [[-1, 1], [0, 1], [1, 1], [0, 2], [-1, 2]]], ["forest", [[2, 1], [1, 2]]]]);
// The 31-tile mixed river, beach and ocean world of make shots-scatter (owner A24 Q5): its placements,
// read from the checkpoint script, at its every-tile overview and at the 20-tile overview pose.
const mixedScript = JSON.parse(await readFile("docs/checkpoints/scatter/scatter-mixed.json", "utf8"))
  .filter((c) => ["select_tile", "hover", "place"].includes(c.cmd));
const WORLDS = {
  world20: { tiles: 20, script: world20, start: [20, 160, 150, 20, 0, -5], cameras: {
    overview: [20, 160, 150, 20, 0, -5],      // draws-overview (gated at 500 draws)
    forest_cluster: [46, 58, 48, 46, 0, -12], // draws-forest-cluster
    meadow: [6, 13, 44, 12, 0, 22],           // draws-meadow
    band152: [20, 56, 136, 20, 0, -5],        // draws-band: world centre 152 m away
  } },
  world2066: { tiles: 2066, save: SAVE, cameras: {
    close: [10, 12, 40, 0, 0, 0],             // the post-arc001 measurement camera
    overview: [20, 160, 150, 20, 0, -5],
  } },
  mixed: { tiles: 31, script: mixedScript, start: [12, 172, 165, 12, 0, -2], cameras: {
    overview: [12, 172, 165, 12, 0, -2],      // every tile in frame (scatter-mixed-world)
    pose20: [20, 160, 150, 20, 0, -5],        // the 20-tile overview pose (scatter-mixed-20tile-camera)
  } },
};
// WORLDS=world2066 (comma list) runs only the named worlds; every world by default.
const RUN_WORLDS = (process.env.WORLDS ?? Object.keys(WORLDS).join(",")).split(",").filter((w) => {
  if (!WORLDS[w]) throw new Error(`unknown world ${w}; known: ${Object.keys(WORLDS).join(", ")}`);
  return true;
});
const cam = ([x, y, z, lx, ly, lz]) => ({ cmd: "camera", pos: [x, y, z], look_at: [lx, ly, lz] });

const types = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm", ".css": "text/css",
  ".ktx2": "image/ktx2", ".glb": "model/gltf-binary", ".png": "image/png", ".wgsl": "text/plain", ".wesl": "text/plain",
  ".jpg": "image/jpeg", ".woff2": "font/woff2", ".json": "application/json", ".webm": "audio/webm", ".m4a": "audio/mp4",
  ".webp": "image/webp", ".svg": "image/svg+xml" };
const serve = (dist) => new Promise((ok) => {
  const s = createServer(async (req, res) => {
    let path = decodeURIComponent(new URL(req.url, "http://x").pathname);
    if (path.endsWith("/")) path += "index.html";
    try {
      const body = await readFile(join(dist, path));
      res.writeHead(200, { "content-type": types[extname(path)] ?? "application/octet-stream" });
      res.end(body);
    } catch {
      res.writeHead(404);
      res.end();
    }
  }).listen(0, () => ok(s));
});
const servers = Object.fromEntries(await Promise.all(ARMS.map(async ([n, dist]) => [n, await serve(dist)])));
const extraKeys = Object.fromEntries(ARMS.map(([n, , q]) => [n, Object.fromEntries(new URLSearchParams(q))]));
const baseOf = (b) => `http://127.0.0.1:${servers[b].address().port}/`;
// Installed before any page script: the start-to-end time of each rAF callback that submitted GPU work
// (an engine frame's main-thread time), pushed to `__cb.ms` while `__cb.on` (e2e/frame-pacing.mjs's probe).
const CB_PROBE = () => {
  const P = (window.__cb = { on: false, ms: [] });
  if (!window.GPUQueue) return;
  let depth = 0;
  let submitted = false;
  const raf = window.requestAnimationFrame.bind(window);
  window.requestAnimationFrame = (cb) => raf((ts) => {
    depth++;
    const t0 = performance.now();
    const outer = submitted;
    submitted = false;
    try { cb(ts); } finally {
      if (submitted && P.on) P.ms.push(performance.now() - t0);
      submitted = outer || submitted;
      depth--;
    }
  });
  const submit = GPUQueue.prototype.submit;
  GPUQueue.prototype.submit = function (b) {
    if (depth > 0) submitted = true;
    return submit.call(this, b);
  };
};
// GPU_CALLS=1: also the time spent inside WebGPU queue, device, canvas, encoder and buffer methods
// (not the per-draw pass encoder calls) while the callback probe is on, summed per method, so a
// callback's time splits into time in the browser's WebGPU implementation and the rest.
const GPU_CALLS = process.env.GPU_CALLS === "1";
// PROFILE_DIR=<dir>: a V8 CPU profile (250 us sampling) of each camera window, written there as
// <arm>-<world>-<camera>-<ms>.cpuprofile with the window's engine frame count; for attribution only,
// since the profiler slows the frames it samples. Function names need a wasm with its name section.
const PROFILE_DIR = process.env.PROFILE_DIR ?? "";
const GPU_PROBE = () => {
  const G = (window.__gpuCalls = {});
  const wrap = (proto, names) => {
    if (!proto) return;
    for (const n of names) {
      const f = proto[n];
      if (typeof f !== "function") continue;
      proto[n] = function (...a) {
        if (!window.__cb?.on) return f.apply(this, a);
        const t0 = performance.now();
        try { return f.apply(this, a); } finally {
          const e = (G[n] ??= { ms: 0, calls: 0 });
          e.ms += performance.now() - t0;
          e.calls++;
        }
      };
    }
  };
  wrap(window.GPUQueue?.prototype, ["submit", "writeBuffer", "writeTexture", "copyExternalImageToTexture", "onSubmittedWorkDone"]);
  wrap(window.GPUCanvasContext?.prototype, ["getCurrentTexture", "configure"]);
  wrap(window.GPUDevice?.prototype, ["createBindGroup", "createBuffer", "createTexture", "createSampler", "createCommandEncoder",
    "createRenderPipeline", "createComputePipeline", "createShaderModule", "createBindGroupLayout", "createPipelineLayout"]);
  wrap(window.GPUTexture?.prototype, ["createView", "destroy"]);
  wrap(window.GPUCommandEncoder?.prototype, ["beginRenderPass", "beginComputePass", "finish", "copyBufferToBuffer",
    "copyBufferToTexture", "copyTextureToTexture", "clearBuffer", "resolveQuerySet", "writeTimestamp"]);
  wrap(window.GPUBuffer?.prototype, ["mapAsync", "getMappedRange", "unmap", "destroy"]);
  wrap(window.GPURenderPassEncoder?.prototype, ["end", "executeBundles"]);
  wrap(window.GPUComputePassEncoder?.prototype, ["end"]);
};
const msStats = (ms) => {
  if (ms.length < 3) return null;
  const s = [...ms].sort((a, b) => a - b);
  const at = (q) => s[Math.min(s.length - 1, Math.floor(q * s.length))];
  const r2 = (x) => Math.round(x * 100) / 100;
  return { frames: s.length, mean: r2(s.reduce((a, b) => a + b, 0) / s.length), p50: r2(at(0.5)), p95: r2(at(0.95)), max: r2(s.at(-1)) };
};
const count = (p) => p.multidraw_sets + p.batchable_bins + p.unbatchable + p.non_mesh;
const saveBytes = [...(await readFile(SAVE))];

async function run(build, worldName) {
  const world = WORLDS[worldName];
  const browser = await chromium.launch({ channel: "chromium", headless: true, args: ARGS });
  const errors = [];
  try {
    const page = await browser.newPage({ viewport: { width: W, height: H }, deviceScaleFactor: 1 });
    await page.addInitScript(CB_PROBE);
    if (GPU_CALLS) await page.addInitScript(GPU_PROBE);
    page.on("console", (m) => { if (m.type() === "error" || BAD_TEXT.test(m.text())) errors.push(m.text()); });
    page.on("pageerror", (e) => errors.push(String(e)));
    // `pause_agents=1` is the harness's own arm key, not a page key: photo mode pauses the agents (sim tick).
    const { pause_agents: pauseKey, ...pageKeys } = extraKeys[build];
    await page.goto(`${baseOf(build)}?${new URLSearchParams({ ...KEYS, ...pageKeys })}`);
    await page.waitForFunction(() => window.hexWorld?.ready(), null, { timeout: 180_000 });
    const gpu = await page.evaluate(async () => {
      const i = (await navigator.gpu.requestAdapter())?.info;
      const gl = document.createElement("canvas").getContext("webgl");
      const ext = gl?.getExtension("WEBGL_debug_renderer_info");
      return { webgpu_adapter: i ? [i.vendor, i.architecture, i.description].join(" | ") : null,
        unmasked_renderer: ext ? gl.getParameter(ext.UNMASKED_RENDERER_WEBGL) : null,
        dpr: window.devicePixelRatio, ua: navigator.userAgent };
    });
    if (!/apple/i.test(gpu.unmasked_renderer ?? "") || /swiftshader/i.test(gpu.unmasked_renderer ?? ""))
      throw new Error(`renderer is not the Apple GPU: ${gpu.unmasked_renderer}`);
    if (world.save) {
      const r = await page.evaluate((b) => window.hexWorld.load(new Uint8Array(b)), saveBytes);
      if (!r.ok || r.tiles !== world.tiles) throw new Error(`load refused or wrong size: ${JSON.stringify(r)}`);
      await page.waitForFunction((n) => window.hexWorld.state().play?.tiles?.length === n, world.tiles,
        { timeout: 300_000, polling: 500 });
    } else {
      await page.evaluate((c) => window.hexWorld.cmd(c), cam(world.start));
      await buildWorld(page, world.script, world.tiles, 300_000);
    }
    // As the draws census (draws.json): the pointer off the world, photo mode on with DOF off.
    await page.evaluate((s) => window.hexWorld.cmd(s), [{ cmd: "hover", q: 9, r: -9 }, { cmd: "toggle", what: "photo" },
      { cmd: "photo", dof: "off", pause_agents: pauseKey === "1" }]);
    // Every pack streamed and every scatter instance placed before any window opens.
    await page.waitForFunction(() => {
      const s = window.hexWorld.state();
      return Object.values(s.packs ?? {}).every((v) => v === "ready") && !s.scatter?.waiting?.length && s.queued === 0;
    }, null, { timeout: 300_000, polling: 500 });
    const cdp = PROFILE_DIR ? await page.context().newCDPSession(page) : null;
    if (cdp) {
      await cdp.send("Profiler.enable");
      await cdp.send("Profiler.setSamplingInterval", { interval: 250 });
    }
    const f0 = await page.evaluate(() => window.hexWorld.diag().frames);
    await page.waitForFunction((f) => window.hexWorld.diag().frames >= f, f0 + SETTLE_FRAMES, { timeout: 300_000, polling: 250 });
    const cameras = {};
    for (const [name, c] of Object.entries(world.cameras)) {
      await page.evaluate((j) => window.hexWorld.cmd(j), cam(c));
      const f1 = await page.evaluate(() => window.hexWorld.diag().frames);
      // The callback probe covers about the window's last 600 frames, as the diag ring does.
      await page.waitForFunction((f) => window.hexWorld.diag().frames >= f, f1 + WINDOW_FRAMES - 600, { timeout: 600_000, polling: 50 });
      await page.evaluate(() => { window.__cb.ms = []; window.__cb.on = true; if (window.__gpuCalls) for (const k in window.__gpuCalls) delete window.__gpuCalls[k]; });
      if (cdp) await cdp.send("Profiler.start");
      await page.waitForFunction((f) => window.hexWorld.diag().frames >= f, f1 + WINDOW_FRAMES, { timeout: 600_000, polling: 250 });
      const cbMs = await page.evaluate(() => { window.__cb.on = false; return window.__cb.ms; });
      if (cdp) {
        const { profile } = await cdp.send("Profiler.stop");
        await writeFile(join(PROFILE_DIR, `${build}-${worldName}-${name}-${Date.now()}.cpuprofile`), JSON.stringify({ ...profile, frames: cbMs.length }));
      }
      const gpuCalls = GPU_CALLS ? await page.evaluate((n) => Object.fromEntries(Object.entries(window.__gpuCalls)
        .map(([k, v]) => [k, { ms_per_frame: Math.round((v.ms / n) * 1000) / 1000, calls_per_frame: Math.round((v.calls / n) * 10) / 10 }])), cbMs.length) : undefined;
      const d = await page.evaluate(() => window.hexWorld.diag());
      const st = await page.evaluate(() => window.hexWorld.state());
      if (st.tier?.tier !== "high") throw new Error(`tier ${st.tier?.tier}`);
      if (st.play?.tiles?.length !== world.tiles) throw new Error(`world has ${st.play?.tiles?.length} tiles`);
      cameras[name] = { p50: d.p50, p95: d.p95, p99: d.p99, mean: d.mean, max: d.max,
        dropped_over_25ms: d.dropped_over_25ms, samples: d.samples, cpu: msStats(cbMs), gpu_calls: gpuCalls, histogram: d.stall?.histogram ?? null, window_start: f1, read_at_frame: d.frames,
        draws: (d.passes ?? []).reduce((n, p) => n + count(p), 0),
        passes: Object.fromEntries((d.passes ?? []).map((p) => [`${p.view}/${p.phase}`, count(p)])),
        kit_draws: d.kit_draws ? { slots: d.kit_draws.slots, total: d.kit_draws.total } : null,
        gpu_timing: d.gpu ?? null, camera_pos: st.camera?.pos, load_at_read: loadavg()[0] };
      if (HOLD_MS > 0) {
        console.log(`hold ${build} ${worldName} ${name} mean ${d.mean}`);
        await page.waitForTimeout(HOLD_MS);
      }
    }
    return { build, keys: extraKeys[build], world: worldName, ...gpu, cameras, console_errors: errors };
  } finally {
    await browser.close();
  }
}

const result = { date: new Date().toISOString(), host: hostname(), cpu: cpus()[0]?.model, viewport: `${W}x${H}`,
  device_scale_factor: 1, keys: KEYS, save: SAVE, browser: { mode: "chromium headless (new), channel chromium, uncapped",
    args: ARGS.join(" ") }, window_frames: WINDOW_FRAMES, settle_frames: SETTLE_FRAMES, load_limit: LOAD_LIMIT,
  rounds: ROUNDS, arms: ARMS.map(([name, dist, query]) => ({ name, dist, keys: query })), rows: [] };
const names = ARMS.map(([n]) => n);
try {
  if (WARMUP) {
    const g = await guarded(() => run(names[0], RUN_WORLDS[0]).catch((e) => ({ error: String(e.message ?? e) })), "warm-up");
    result.warmup = { build: names[0], world: RUN_WORLDS[0], error: g.result.error ?? null, load_before: g.load_before, load_after: g.load_after };
    console.log(`warm-up ${names[0]} ${RUN_WORLDS[0]} discarded (load ${g.load_before.toFixed(2)} -> ${g.load_after.toFixed(2)})`);
  }
  for (let r = 0; r < ROUNDS; r++)
    for (const worldName of RUN_WORLDS)
      for (const build of names.map((_, i) => names[(i + r) % names.length])) {
        const label = `${build} ${worldName} round ${r}`;
        const g = await guarded(() => run(build, worldName).catch((e) => ({ build, world: worldName, error: String(e.message ?? e) })), label);
        result.rows.push({ round: r, ...g.result, load_before: g.load_before, load_after: g.load_after, load_discarded: g.load_discarded });
        const x = result.rows.at(-1);
        console.log(`${label}: ${x.error ?? Object.entries(x.cameras).map(([n, c]) => `${n} p50 ${c.p50} p95 ${c.p95} mean ${c.mean} cpu ${c.cpu?.mean}/${c.cpu?.p95} draws ${c.draws} kit ${c.kit_draws?.total}`).join(" | ")}` +
          ` errors ${x.console_errors?.length ?? "-"} (load ${g.load_before.toFixed(2)} -> ${g.load_after.toFixed(2)})`);
        await writeFile(OUT, `${JSON.stringify(result, null, 2)}\n`);
      }
} finally {
  for (const s of Object.values(servers)) s.close();
}
console.log(`wrote ${OUT}`);
