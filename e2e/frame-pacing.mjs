// Frame pacing probe (card "frame pacing: closed-Settings play alternates short and long frames"):
// the world, camera, keys, viewport and browser flags of e2e/perf-grass-drag.mjs, then 5 s still
// windows in a sequence of HUD states inside one page, so each arm is compared with itself. Each
// window reports the rAF and engine-submit interval stats with the pair statistic (`alternation` near
// 1 when frames alternate short and long), submit-to-done latency, the engine frames drawn and the
// frame-latency limiter's deferrals. Findings and the MiniMac recipe: docs/perf/frame-pacing/.
// usage: node e2e/frame-pacing.mjs <dist>
//   PACING_OUT (default e2e/test-results/frame-pacing.json), PACING_ARMS (comma list, default all),
//   PACING_RUNS (default 1), PACING_MS (default 5000), PACING_TRACE (a directory: also writes a 3 s
//   Chrome trace of the first closed and the open window per arm, read by e2e/frame-pacing-trace.mjs),
//   PACING_GUARD=1 (host-load guard), PACING_PACED=1 (display pacing, not uncapped), PACING_SIZE, PACING_DPR, PACING_KEYS, PACING_RADIUS,
//   PACING_FOCUS, PACING_TIMELINE, PACING_SHIM_A / PACING_SHIM_B (see where each is read).
import { chromium } from "playwright";
import { createServer } from "node:http";
import { open, readFile, writeFile, mkdir } from "node:fs/promises";
import { extname, join } from "node:path";
import { loadavg } from "node:os";
import { guarded } from "./perf-load.mjs";
import { waterWorldScript } from "./water-world.mjs";
import { buildWorld } from "./build-world.mjs";

const dist = process.argv[2] ?? "dist";
const OUT = process.env.PACING_OUT ?? "e2e/test-results/frame-pacing.json";
const RUNS = Number(process.env.PACING_RUNS ?? 1);
const MS = Number(process.env.PACING_MS ?? 5000);
const TRACE = process.env.PACING_TRACE ?? null;
const GUARD = process.env.PACING_GUARD === "1";
// PACING_SIZE (default 2560x1440): a larger viewport (5120x2880 is four times the pixels) makes a fast
// GPU as GPU-bound as a base M4 at 2560x1440. The canvas follows CSS pixels, so PACING_DPR does not.
const [W, H] = (process.env.PACING_SIZE ?? "2560x1440").split("x").map(Number);
const DPR = Number(process.env.PACING_DPR ?? 1);
const world = waterWorldScript(Number(process.env.PACING_RADIUS ?? 10));
const CAMERA = { cmd: "camera", pos: [10, 12, 40], look_at: [0, 0, 0] };
// PACING_KEYS (`k=v&k=v`) adds or overrides page keys for every arm, e.g. `gpu_timing=on`.
const KEYS = { scenario: "play", tier: "high", fps: "off", seed: "1", tilt: "off", grass: "1",
  ...Object.fromEntries(new URLSearchParams(process.env.PACING_KEYS ?? "")) };
// PACING_PACED=1 keeps Chrome's display pacing (drops the two uncapping flags), as players run.
const PACED = process.env.PACING_PACED === "1";
const ARGS = ["--use-angle=metal", "--enable-gpu", "--ignore-gpu-blocklist", "--enable-unsafe-webgpu",
  ...(PACED ? [] : ["--disable-gpu-vsync", "--disable-frame-rate-limit"]), "--enable-webgpu-developer-features"];
const TRACE_CATS = ["devtools.timeline", "disabled-by-default-devtools.timeline", "disabled-by-default-devtools.timeline.frame",
  "v8.execute", "blink", "cc", "viz", "gpu", "gpu.dawn", "webgpu", "toplevel", "benchmark", "rail"];

// Arms: extra keys for the page, and the HUD state sequence run inside it.
const FULL = ["closed", "open", "closed", "unfocused", "closed", "tiles_closed", "closed", "hidden", "closed"];
const SHORT = ["closed", "open", "closed", "unfocused", "closed"];
const ARMS = {
  default: { keys: {}, phases: FULL },
  latency_off: { keys: { frame_latency: "off" }, phases: SHORT },
  latency_1: { keys: { frame_latency: "1" }, phases: SHORT },
  latency_2: { keys: { frame_latency: "2" }, phases: SHORT },
  latency_8: { keys: { frame_latency: "8" }, phases: SHORT },
  hud_off: { keys: {}, hud: false, phases: ["closed", "unfocused", "closed"] },
  // The page's web/frame-latency.js replaced by the file at PACING_SHIM_A / PACING_SHIM_B, so two
  // limiter versions compare on one dist (for example `git show 8464de8:web/frame-latency.js`).
  shim_a: { keys: {}, shim: "PACING_SHIM_A", phases: ["closed", "open", "closed"] },
  shim_b: { keys: {}, shim: "PACING_SHIM_B", phases: ["closed", "open", "closed"] },
};
const pick = (process.env.PACING_ARMS ?? Object.keys(ARMS).join(",")).split(",");

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

const round2 = (x) => Math.round(x * 100) / 100;
const pct = (s, p) => s[Math.min(s.length - 1, Math.floor(p * s.length))];
// The pair statistic of e2e/perf-grass-drag.mjs (pairStats), kept identical.
function pairStats(ms) {
  const pairs = [];
  for (let i = 0; i + 1 < ms.length; i += 2) pairs.push([Math.min(ms[i], ms[i + 1]), Math.max(ms[i], ms[i + 1])]);
  if (!pairs.length) return null;
  const sorted = [...ms].sort((a, b) => a - b);
  const lo = sorted[Math.floor((sorted.length - 1) / 2)], hi = sorted[Math.ceil((sorted.length - 1) / 2)];
  const mean = (v) => round2(v.reduce((a, b) => a + b, 0) / v.length);
  return { pairs: pairs.length, short_mean: mean(pairs.map(([a]) => a)), long_mean: mean(pairs.map(([, b]) => b)),
    alternation: round2(pairs.filter(([a, b]) => a < b && a <= lo && b >= hi).length / pairs.length) };
}
const msStats = (v) => {
  const s = [...v].sort((a, b) => a - b);
  return { p50: round2(pct(s, 0.5)), p95: round2(pct(s, 0.95)), mean: round2(s.reduce((a, b) => a + b, 0) / s.length) };
};
function stats(ms) {
  const s = [...ms].sort((a, b) => a - b);
  return { frames: s.length, p50: round2(pct(s, 0.5)), p95: round2(pct(s, 0.95)), max: round2(s.at(-1)),
    mean: round2(s.reduce((a, b) => a + b, 0) / s.length), over_25ms: s.filter((x) => x > 25).length,
    pairs: pairStats(ms), first_40: ms.slice(0, 40).map((x) => Math.round(x * 10) / 10) };
}

// Installed before any page script, so it sits under web/frame-latency.js's own submit wrapper: while
// `__pacing.on`, the first GPUQueue.submit of each task is an engine frame (`submit`), and the time
// its work is reported done (`done`). The probe's rAF goes through the frame-latency limiter like the
// engine's, so its intervals show when the limiter releases callbacks; submit intervals are the frames.
const SUBMIT_PROBE = () => {
  const P = (window.__pacing = { on: false, submit: [], done: [], log: null });
  if (!window.GPUQueue) return;
  const submit = GPUQueue.prototype.submit;
  let armed = false;
  // While `P.log` is an array: every real requestAnimationFrame call (under the limiter, so a deferred
  // request shows up when the limiter releases it), callback start and end, and submit, each with the
  // limiter's in-flight count and whether it ran inside a callback.
  let depth = 0;
  const raf = window.requestAnimationFrame.bind(window);
  const fl = () => window.__frameLatency?.inflight() ?? null;
  window.requestAnimationFrame = (cb) => {
    P.log?.push(["req", performance.now(), depth, fl()]);
    return raf((ts) => {
      depth++;
      P.log?.push(["cb", performance.now(), depth, fl()]);
      try { cb(ts); } finally { P.log?.push(["end", performance.now(), depth, fl()]); depth--; }
    });
  };
  GPUQueue.prototype.submit = function (b) {
    P.log?.push(["submit", performance.now(), depth, fl()]);
    const r = submit.call(this, b);
    if (P.on && !armed) {
      armed = true;
      queueMicrotask(() => { armed = false; });
      P.submit.push(performance.now());
      this.onSubmittedWorkDone().then(() => { P.done.push(performance.now()); P.log?.push(["done", performance.now(), depth, fl()]); });
    }
    return r;
  };
};
const recordFrames = (page, ms) => page.evaluate((ms) => new Promise((resolve) => {
  const t = [];
  const end = performance.now() + ms;
  const fl = window.__frameLatency;
  const d0 = fl ? fl.deferrals() : null;
  const f0 = window.hexWorld.state().frames;
  const P = window.__pacing;
  if (P) { P.submit = []; P.done = []; P.on = true; }
  const tick = (now) => {
    t.push(now);
    if (now < end) requestAnimationFrame(tick);
    else {
      if (P) P.on = false;
      resolve({ t, submit: P?.submit ?? [], done: P?.done ?? [], deferrals: fl ? fl.deferrals() - d0 : null,
        engine_frames: window.hexWorld.state().frames - f0 });
    }
  };
  requestAnimationFrame(tick);
}), ms);
const waitFrames = (page, n) => page.evaluate((n) => new Promise((r) => {
  let i = 0;
  const f = () => (++i >= n ? r() : requestAnimationFrame(f));
  requestAnimationFrame(f);
}), n);

const settingsOpen = (page) => page.evaluate(() => !!document.querySelector(".sheet--settings:not(.is-closed)"));
const tilesOpen = (page) => page.evaluate(() => document.querySelector(".tool[data-action=inventory]")?.classList.contains("is-on") ?? null);
const hudHidden = (page) => page.evaluate(() => document.querySelector("#hud")?.classList.contains("is-hidden") ?? null);

async function setPhase(page, phase, hud) {
  if (!hud) return;
  const wantSettings = phase === "open";
  const wantTiles = phase !== "tiles_closed";
  const wantHidden = phase === "hidden";
  if ((await hudHidden(page)) !== wantHidden) { await page.keyboard.press("h"); await waitFrames(page, 5); }
  if (wantHidden) return;
  if ((await settingsOpen(page)) !== wantSettings) await page.locator(".tool[data-action=settings]").click();
  if ((await tilesOpen(page)) !== wantTiles) await page.locator(".tool[data-action=inventory]").click();
}
// Bevy's default WinitSettings (game) update at 60 Hz reactive while the canvas is not focused, and
// opening Settings moves focus into the sheet: every phase but `unfocused` puts focus back on the
// canvas (PACING_FOCUS=keep leaves focus where the HUD put it).
async function setFocus(page, phase) {
  if (phase === "unfocused") return page.evaluate(() => document.querySelector("canvas#hex-world")?.blur());
  if (process.env.PACING_FOCUS === "keep") return;
  await page.evaluate(() => document.querySelector("canvas#hex-world")?.focus({ preventScroll: true }));
}
const focusOf = (page) => page.evaluate(() => {
  const a = document.activeElement;
  return a === document.querySelector("canvas#hex-world") ? "canvas" : (a?.className || a?.tagName || null);
});

async function trace(page, path) {
  const cdp = await page.context().newCDPSession(page);
  const chunks = [];
  cdp.on("Tracing.dataCollected", (e) => chunks.push(...e.value));
  const done = new Promise((r) => cdp.once("Tracing.tracingComplete", r));
  await cdp.send("Tracing.start", { traceConfig: { includedCategories: TRACE_CATS, recordMode: "recordContinuously" },
    transferMode: "ReportEvents" });
  const rec = await recordFrames(page, 3000);
  await cdp.send("Tracing.end");
  await done;
  const fh = await open(path, "w");
  await fh.write("[\n");
  for (let i = 0; i < chunks.length; i++) await fh.write(JSON.stringify(chunks[i]) + (i + 1 < chunks.length ? ",\n" : "\n"));
  await fh.write("]\n");
  await fh.close();
  await cdp.detach();
  return { path, events: chunks.length, raf_t: rec.t };
}

async function run(name, arm) {
  const hud = arm.hud !== false;
  const browser = await chromium.launch({ channel: "chromium", headless: true, args: ARGS });
  try {
    const page = await browser.newPage({ viewport: { width: W, height: H }, deviceScaleFactor: DPR });
    if (!hud) await page.route((u) => u.pathname.endsWith("/web/ui/hud.js"), (r) => r.fulfill({ status: 200, contentType: "text/javascript", body: "" }));
    if (arm.shim) {
      const file = process.env[arm.shim];
      if (!file) throw new Error(`${name} needs ${arm.shim}`);
      const body = await readFile(file, "utf8");
      await page.route((u) => u.pathname.endsWith("/web/frame-latency.js"), (r) => r.fulfill({ status: 200, contentType: "text/javascript", body }));
    }
    await page.addInitScript(SUBMIT_PROBE);
    await page.goto(base + "?" + new URLSearchParams({ ...KEYS, ...arm.keys }));
    await page.waitForFunction(() => window.hexWorld?.ready(), null, { timeout: 180_000 });
    const adapter = await page.evaluate(async () => {
      const i = (await navigator.gpu.requestAdapter())?.info;
      return i ? [i.vendor, i.architecture, i.description].join(" | ") : null;
    });
    if (!adapter || /swiftshader/i.test(adapter)) throw new Error("no hardware WebGPU adapter: " + adapter);
    if (hud) await page.waitForFunction(() => document.querySelector(".tool[data-action=settings]"), null, { timeout: 60_000 });
    await buildWorld(page, [...world.script, CAMERA], world.tiles.length);
    await waitFrames(page, 300);
    const phases = [];
    let traced = 0;
    for (const phase of arm.phases) {
      await setPhase(page, phase === "unfocused" ? "closed" : phase, hud);
      await setFocus(page, phase);
      await waitFrames(page, 60);
      const focus = await focusOf(page);
      const state = hud ? { settings: await settingsOpen(page), tiles: await tilesOpen(page), hidden: await hudHidden(page), focus } : { focus };
      const rec = await recordFrames(page, MS);
      const iv = rec.t.slice(1).map((x, i) => x - rec.t[i]);
      const gaps = (v) => v.slice(1).map((x, i) => x - v[i]);
      const row = { phase, state, ...stats(iv), submit: rec.submit.length > 2 ? stats(gaps(rec.submit)) : null,
        done: rec.done.length > 2 ? stats(gaps(rec.done)) : null, deferrals: rec.deferrals, engine_frames: rec.engine_frames,
        // Submit to work-done per frame (the promises resolve in submit order).
        latency: rec.done.length > 2 ? msStats(rec.done.map((d, i) => d - rec.submit[i])) : null };
      if (TRACE && (phase === "open" || (phase === "closed" && traced === 0)) && traced < 2) {
        await mkdir(TRACE, { recursive: true });
        const tr = await trace(page, join(TRACE, `${name}-${phase}.json`));
        const tiv = tr.raf_t.slice(1).map((x, i) => x - tr.raf_t[i]);
        row.trace = { path: tr.path, events: tr.events, raf: stats(tiv), raf_t: tr.raf_t.map((x) => round2(x)) };
        traced++;
      }
      // PACING_TIMELINE=1: a 1 s event log (see SUBMIT_PROBE) after each phase's window.
      if (process.env.PACING_TIMELINE === "1")
        row.timeline = await page.evaluate(() => new Promise((r) => {
          const P = window.__pacing;
          P.log = [];
          setTimeout(() => { const l = P.log; P.log = null; r(l.map(([k, t, d, n]) => [k, Math.round(t * 100) / 100, d, n])); }, 1000);
        }));
      phases.push(row);
      console.log(`${name} ${phase}: frames ${row.frames} mean ${row.mean} p50 ${row.p50} p95 ${row.p95} alternation ` +
        `${row.pairs?.alternation} (${row.pairs?.short_mean}/${row.pairs?.long_mean}) | submit n ${row.submit?.frames} mean ${row.submit?.mean} ` +
        `p95 ${row.submit?.p95} alt ${row.submit?.pairs?.alternation} (${row.submit?.pairs?.short_mean}/${row.submit?.pairs?.long_mean}) | ` +
        `done alt ${row.done?.pairs?.alternation} (${row.done?.pairs?.short_mean}/${row.done?.pairs?.long_mean}) | latency p50 ${row.latency?.p50} | deferrals ${row.deferrals} engine ${row.engine_frames} ` +
        `state ${JSON.stringify(state)}`);
    }
    const frame_latency = await page.evaluate(() => window.__frameLatency ? { limit: window.__frameLatency.limit, source: window.__frameLatency.source } : null);
    const build_id = await page.evaluate(() => window.__HEX_BUILD ?? null);
    const canvas = await page.evaluate(() => { const c = document.querySelector("canvas#hex-world"); return c ? [c.width, c.height] : null; });
    const gpu = await page.evaluate(() => { const g = window.hexWorld.diag().gpu; return g?.total ?? g ?? null; });
    return { build_id, adapter, frame_latency, canvas, gpu, phases };
  } finally {
    await browser.close();
  }
}

const result = { date: new Date().toISOString(), viewport: `${W}x${H}`, device_scale_factor: DPR, keys: KEYS, camera: CAMERA, window_ms: MS,
  guard: GUARD, paced: PACED, web: { args: ARGS.join(" ") }, rows: [] };
try {
  for (let r = 0; r < RUNS; r++)
    for (const name of pick) {
      const arm = ARMS[name];
      if (!arm) throw new Error(`unknown arm ${name}`);
      const measure = () => run(name, arm).catch((e) => ({ error: String(e.message ?? e) }));
      const g = GUARD ? await guarded(measure, `${name} run ${r}`)
        : { result: await (async () => { const lb = loadavg()[0]; const x = await measure(); return { ...x, lb }; })() };
      const row = { arm: name, run: r, ...g.result, load_before: g.load_before ?? g.result.lb, load_after: g.load_after ?? loadavg()[0] };
      delete row.lb;
      if (row.error) console.log(`${name} run ${r}: ${row.error}`);
      result.rows.push(row);
      await writeFile(OUT, JSON.stringify(result, null, 2) + "\n");
    }
} finally {
  server.close();
}
console.log(`wrote ${OUT}`);
