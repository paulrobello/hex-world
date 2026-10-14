// Web stall-tail attribution (ADR 0001 carry-over, card "Web >25 ms stall tail"): the perf-passes
// workload (gate2000, instanced, mid600 shadows, High, 2560x1440, DPR 1) in GPU Chromium, one run per
// arm, each inside the host-load guard. Arms toggle one suspect each (pacing, HUD overlay, diag
// polling, GPU load, tilt-shift); the `hooks` arm wraps GPUQueue.submit, onSubmittedWorkDone,
// getCurrentTexture and requestAnimationFrame in the page to see where a stalled frame's time goes
// and how many submits are in flight. With PROBE_TRACE=<file> one extra run records a Chrome trace
// (v8, GC, wasm, gpu, dawn, viz, devtools timeline) of a 1 s steady-state window.
// Local only. usage: node e2e/stall-probe.mjs <dist> <out.json>
//   PROBE_ARMS=base,paced,... (default all), PROBE_ROUNDS (default 2), PROBE_TRACE=<trace.json>, PROBE_TRACE_ARM
import { chromium } from "playwright";
import { createServer } from "node:http";
import { open, readFile, writeFile } from "node:fs/promises";
import { extname, join } from "node:path";
import { LOAD_LIMIT, guarded } from "./perf-load.mjs";

const dist = process.argv[2] ?? "dist";
const OUT = process.argv[3] ?? "target/stall-probe.json";
const ROUNDS = Number(process.env.PROBE_ROUNDS ?? 2);
const KEYS = { scenario: "gate2000", view: "strategic", layout: "instanced", chunk: "7", post: "full", shadows: "on",
  shadow_casters: "all", cascades: "3", shadow_dist: "600", tier: "high", fps: "off", tilt: "off", gpu_timing: "off" };
const UNCAPPED = ["--disable-gpu-vsync", "--disable-frame-rate-limit"];
// One suspect per arm; everything else is the perf-passes budget run.
const ARMS = {
  base: {},
  paced: { paced: true },
  hud_off: { hud: false },
  poll_slow: { poll: 5000 },
  quiet: { hud: false, poll: 5000 },
  post_none: { keys: { post: "none" } },
  shadows_off: { keys: { shadows: "off" } },
  small: { size: [1280, 720] },
  tilt_on: { keys: { tilt: "on" } },
  hooks: { hooks: true },
  hooks_paced: { hooks: true, paced: true },
  // Candidate fix, as a page shim only: at most N frames' submits unfinished before the next rAF runs.
  limit2: { limit: 2 },
  limit2_paced: { limit: 2, paced: true },
  limit3: { limit: 3 },
  // The shipped opt-in key (web/frame-latency.js), same limiter.
  fl3: { keys: { frame_latency: "3" } },
  fl3_paced: { keys: { frame_latency: "3" }, paced: true },
};
const pickArms = (process.env.PROBE_ARMS ?? Object.keys(ARMS).join(",")).split(",");

const types = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm", ".css": "text/css",
  ".ktx2": "image/ktx2", ".glb": "model/gltf-binary", ".png": "image/png", ".wgsl": "text/plain",
  ".jpg": "image/jpeg", ".woff2": "font/woff2", ".webm": "audio/webm", ".m4a": "audio/mp4" };
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

// Page hooks, installed before the wasm loads. Recording starts when __probe.on is set.
const HOOKS = () => {
  const P = { on: false, submits: [], done: [], gct: [], raf: [], inflight: 0, n: 0 };
  window.__probe = P;
  const sub = GPUQueue.prototype.submit;
  GPUQueue.prototype.submit = function (bufs) {
    const t0 = performance.now();
    const r = sub.call(this, bufs);
    const t1 = performance.now();
    if (!P.on) return r;
    const id = ++P.n;
    P.inflight++;
    P.submits.push([id, t0, t1 - t0, P.inflight]);
    this.onSubmittedWorkDone().then(() => { P.inflight--; P.done.push([id, performance.now()]); });
    return r;
  };
  const gct = GPUCanvasContext.prototype.getCurrentTexture;
  GPUCanvasContext.prototype.getCurrentTexture = function () {
    const t0 = performance.now();
    const r = gct.call(this);
    if (P.on) P.gct.push([t0, performance.now() - t0]);
    return r;
  };
  const raf = window.requestAnimationFrame.bind(window);
  window.requestAnimationFrame = (cb) => raf((ts) => {
    const t0 = performance.now();
    try { return cb(ts); } finally { if (P.on) P.raf.push([t0, performance.now() - t0, ts]); }
  });
};

// PROBE_TRACE_CATS overrides (comma list); the default keeps a 1 s window under a few hundred MB.
// Frame-latency limiter (what wgpu's native surface does with desired_maximum_frame_latency): a rAF
// callback that submitted counts as in flight until queue.onSubmittedWorkDone resolves; while N are
// in flight, new rAF requests wait and are re-requested as frames finish.
const LIMITER = (N) => {
  let queue = null, submitted = false, inflight = 0, deferrals = 0;
  const deferred = [];
  const sub = GPUQueue.prototype.submit;
  GPUQueue.prototype.submit = function (b) { queue = this; submitted = true; return sub.call(this, b); };
  const raf = window.requestAnimationFrame.bind(window);
  const release = () => { while (inflight < N && deferred.length) raf(deferred.shift()); };
  window.requestAnimationFrame = (cb) => {
    const wrapped = (ts) => {
      submitted = false;
      try { return cb(ts); } finally {
        if (submitted && queue) { inflight++; queue.onSubmittedWorkDone().then(() => { inflight--; release(); }); }
      }
    };
    if (inflight < N) return raf(wrapped);
    deferrals++;
    deferred.push(wrapped);
    return 0;
  };
  window.__limiter = { N, deferrals: () => deferrals };
};

const TRACE_CATS = (process.env.PROBE_TRACE_CATS ?? ["devtools.timeline", "v8", "v8.execute", "disabled-by-default-v8.gc",
  "v8.wasm", "viz", "gpu", "gpu.dawn", "gpu.service", "webgpu", "disabled-by-default-webgpu", "toplevel"].join(",")).split(",");
const TRACE_MS = Number(process.env.PROBE_TRACE_MS ?? 1000);

async function run(name, arm, trace = null) {
  const [W, H] = arm.size ?? [2560, 1440];
  const browser = await chromium.launch({ channel: "chromium", headless: true,
    args: ["--use-angle=metal", "--enable-gpu", "--ignore-gpu-blocklist", "--enable-unsafe-webgpu",
      "--enable-webgpu-developer-features", ...(arm.paced ? [] : UNCAPPED)] });
  try {
    const page = await browser.newPage({ viewport: { width: W, height: H }, deviceScaleFactor: 1 });
    const errors = [];
    page.on("pageerror", (e) => errors.push(String(e)));
    if (arm.hud === false) await page.route("**/web/ui/hud.js", (r) => r.fulfill({ status: 200, contentType: "text/javascript", body: "" }));
    if (arm.hooks) await page.addInitScript(HOOKS);
    if (arm.limit) await page.addInitScript(LIMITER, arm.limit);
    const q = new URLSearchParams({ ...KEYS, ...(arm.keys ?? {}) });
    await page.goto(base + "?" + q);
    await page.waitForFunction(() => window.hexWorld?.ready(), null, { timeout: 120_000 });
    const mem0 = (await page.evaluate(() => window.hexWorld.diag())).wasm_memory_bytes;
    if (arm.hooks) await page.evaluate(() => { window.__probe.on = true; });
    const poll = arm.poll ?? 250;
    await page.waitForFunction(() => {
      const d = window.hexWorld.diag();
      return d.window_start_frame > 0 && d.samples >= 600 && d.frames - d.window_start_frame + 1 >= 600;
    }, null, { timeout: 240_000, polling: poll });
    const d = await page.evaluate(() => window.hexWorld.diag());
    const limiter = await page.evaluate(() => (window.__limiter ? { N: window.__limiter.N, deferrals: window.__limiter.deferrals() }
      : window.__frameLatency ? { N: window.__frameLatency.limit, deferrals: window.__frameLatency.deferrals(), shipped: true } : null));
    let probe = null;
    if (arm.hooks) probe = await page.evaluate(() => { const P = window.__probe; P.on = false; return P; });
    let traced = null;
    if (trace) {
      const cdp = await page.context().newCDPSession(page);
      const chunks = [];
      cdp.on("Tracing.dataCollected", (e) => chunks.push(...e.value));
      const done = new Promise((r) => cdp.once("Tracing.tracingComplete", r));
      await cdp.send("Tracing.start", { traceConfig: { includedCategories: TRACE_CATS, recordMode: "recordContinuously" },
        transferMode: "ReportEvents" });
      const f0 = (await page.evaluate(() => window.hexWorld.diag())).frames;
      await page.waitForTimeout(TRACE_MS);
      const f1 = (await page.evaluate(() => window.hexWorld.diag())).frames;
      await cdp.send("Tracing.end");
      await done;
      // Chrome's JSON array trace format, one event per line (a single stringify overflows V8's string limit).
      const fh = await open(trace, "w");
      await fh.write("[\n");
      for (let i = 0; i < chunks.length; i++) await fh.write(JSON.stringify(chunks[i]) + (i + 1 < chunks.length ? ",\n" : "\n"));
      await fh.write("]\n");
      await fh.close();
      traced = { path: trace, events: chunks.length, frames: [f0, f1] };
    }
    const tail = d.stall?.tail ?? [];
    const gaps = tail.slice(1).map((s, i) => s.frame - tail[i].frame);
    return { arm: name, viewport: `${W}x${H}`, paced: !!arm.paced, hud: arm.hud !== false, poll_ms: poll,
      keys: arm.keys ?? {}, p50: d.p50, p95: d.p95, p99: d.p99, mean: d.mean, max: d.max, over_25ms: d.dropped_over_25ms,
      histogram: d.stall?.histogram, stall_ms: tail.map((s) => s.ms), stall_frame_gaps: gaps,
      pipelines_waiting_max: Math.max(0, ...tail.map((s) => s.pipelines_waiting ?? 0)),
      read_frames_past_window: d.frames - d.window_start_frame + 1, limiter,
      wasm_memory_bytes: [mem0, d.wasm_memory_bytes], adapter: d.adapter, page_errors: errors,
      probe: probe && summarizeProbe(probe), trace: traced };
  } finally {
    await browser.close();
  }
}

// Joins each stalled rAF interval to what happened inside it: the callback's own time, the
// getCurrentTexture and submit calls in it, and the in-flight submit depth around it.
function summarizeProbe(P) {
  const raf = P.raf;
  const frames = raf.slice(1).map((r, i) => ({ start: raf[i][0], interval: r[0] - raf[i][0], cb: raf[i][1] }));
  const inWin = (arr, a, b) => arr.filter((x) => x[1] >= a && x[1] < b);
  const doneAt = new Map(P.done.map(([id, t]) => [id, t]));
  const lat = P.submits.filter((s) => doneAt.has(s[0])).map((s) => doneAt.get(s[0]) - s[1]).sort((a, b) => a - b);
  const q = (v, p) => (v.length ? v[Math.min(v.length - 1, Math.floor(p * v.length))] : null);
  const stalls = frames.filter((f) => f.interval > 25).map((f) => {
    const subs = P.submits.filter((s) => s[1] >= f.start && s[1] < f.start + f.interval);
    const g = P.gct.filter((x) => x[0] >= f.start && x[0] < f.start + f.interval);
    return { interval_ms: +f.interval.toFixed(1), callback_ms: +f.cb.toFixed(1),
      get_current_texture_ms: +g.reduce((a, x) => a + x[1], 0).toFixed(1),
      submit_ms: +subs.reduce((a, s) => a + s[2], 0).toFixed(1), inflight_at_submit: subs.map((s) => s[3]) };
  });
  const cbs = frames.map((f) => f.cb).sort((a, b) => a - b);
  const ints = frames.map((f) => f.interval).sort((a, b) => a - b);
  const depth = P.submits.map((s) => s[3]).sort((a, b) => a - b);
  return { raf_frames: frames.length, submits: P.submits.length, done: P.done.length,
    interval_p50: q(ints, 0.5), callback_p50: q(cbs, 0.5), callback_max: q(cbs, 1),
    submit_to_done_ms: { p50: q(lat, 0.5), p95: q(lat, 0.95), max: q(lat, 1) },
    inflight_at_submit: { p50: q(depth, 0.5), p95: q(depth, 0.95), max: q(depth, 1) },
    gct_max_ms: Math.max(0, ...P.gct.map((x) => x[1])), submit_max_ms: Math.max(0, ...P.submits.map((s) => s[2])),
    stalls };
}

const result = { date: new Date().toISOString(), device_scale_factor: 1, keys: KEYS, load_limit: LOAD_LIMIT,
  browser: "chromium-headless-gpu (--use-angle=metal --enable-gpu --ignore-gpu-blocklist --enable-unsafe-webgpu --enable-webgpu-developer-features; uncapped adds --disable-gpu-vsync --disable-frame-rate-limit)",
  rows: [] };
try {
  for (let r = 0; r < ROUNDS; r++)
    for (const name of r % 2 ? [...pickArms].reverse() : pickArms) {
      const g = await guarded(() => run(name, ARMS[name]).catch((e) => ({ arm: name, error: String(e.message ?? e) })), `probe ${name}`);
      result.rows.push({ round: r, ...g.result, load_before: g.load_before, load_after: g.load_after, load_discarded: g.load_discarded });
      const x = result.rows.at(-1);
      console.log(`${name} r${r}: ${x.error ?? `p50 ${x.p50} p95 ${x.p95} mean ${x.mean} max ${x.max} over25 ${x.over_25ms} gaps ${JSON.stringify(x.stall_frame_gaps)}`} ` +
        `(load ${g.load_before.toFixed(2)} -> ${g.load_after.toFixed(2)})`);
      await writeFile(OUT, JSON.stringify(result, null, 2) + "\n");
    }
  if (process.env.PROBE_TRACE) {
    const name = process.env.PROBE_TRACE_ARM ?? "base";
    const g = await guarded(() => run(name, ARMS[name], process.env.PROBE_TRACE), `trace ${name}`);
    result.trace_run = { ...g.result, load_before: g.load_before, load_after: g.load_after, load_discarded: g.load_discarded };
    console.log(`trace ${name}: ${JSON.stringify(g.result.trace)} (load ${g.load_before.toFixed(2)} -> ${g.load_after.toFixed(2)})`);
    await writeFile(OUT, JSON.stringify(result, null, 2) + "\n");
  }
} finally {
  server.close();
}
console.log(`wrote ${OUT}`);
