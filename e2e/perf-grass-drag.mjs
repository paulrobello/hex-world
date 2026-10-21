// Grass density slider drag (card "Grass: doubled density and a density slider", criterion 2): does a
// real drag of Settings → Graphics → Grass density hitch? The water world of e2e/water-world.mjs
// (radius 10 by default: the low meadow camera of e2e/perf-grass.mjs holds the same grass groups as
// on the 2,066-tile world, whose baseline is CPU-bound past the 16.6 ms budget with grass off, so a
// hitch would drown there) built by script in the play scenario on High at 2560x1440, DPR 1, tilt
// off, in uncapped GPU Chromium with the perf-grass flags and the default frame latency (3). With the
// camera still, a page-side requestAnimationFrame loop records every frame interval for 5 s with the
// Settings sheet closed, then with it open for a 5 s baseline and a 5 s drag that steps the slider in 5% steps
// 100 -> 0 -> 150 -> 50 -> 100 from an in-page timer (an `input` event each step, one `change` on
// release, so the HUD's 150 ms set_grass throttle and its flush run as in a real drag; every set_grass
// the HUD sends is logged, and each frame over 25 ms is placed after the latest one). Nothing polls
// state() inside any window. Rebuild evidence compares state() before and after the drag (grass
// uploads, groups and instances, tier changes, tiles, diag entities). An untimed phase then grows the
// grass distance to Far at 100% and again at 150% and reads state().grass every frame for the build
// cost (each building frame's slowest tile). Still 5 s windows at 0% and 150% after the drag show
// what the grass itself costs on this camera. Every run is inside the
// host-load guard. Local only. usage: node e2e/perf-grass-drag.mjs <dist>
//   PERF_OUT (default docs/perf/p2-grass-drag.json), DRAG_RUNS (default 3), DRAG_RADIUS (default 10),
//   DRAG_MS (default 5000), DRAG_BASELINE_MS (default 5000)
import { chromium } from "playwright";
import { createServer } from "node:http";
import { readFile, writeFile } from "node:fs/promises";
import { extname, join } from "node:path";
import { LOAD_LIMIT, guarded } from "./perf-load.mjs";
import { BAD_TEXT } from "./console-errors.mjs";
import { waterWorldScript } from "./water-world.mjs";
import { buildWorld } from "./build-world.mjs";

const dist = process.argv[2] ?? "dist";
const OUT = process.env.PERF_OUT ?? "docs/perf/p2-grass-drag.json";
const RUNS = Number(process.env.DRAG_RUNS ?? 3);
const RADIUS = Number(process.env.DRAG_RADIUS ?? 10);
const DRAG_MS = Number(process.env.DRAG_MS ?? 5000);
const BASELINE_MS = Number(process.env.DRAG_BASELINE_MS ?? 5000);
const W = 2560, H = 1440;
const SETTLE_FRAMES = 300;
const BUDGET_MS = 16.6;
const world = waterWorldScript(RADIUS);
const CAMERA = { cmd: "camera", pos: [10, 12, 40], look_at: [0, 0, 0] };
const KEYS = { scenario: "play", tier: "high", fps: "off", seed: "1", tilt: "off", grass: "1" };
// The drag's slider percents: 100 -> 0 -> 150 -> 50 -> 100 in the slider's 5% steps.
const ramp = (a, b) => { const s = a < b ? 5 : -5, v = []; for (let x = a + s; s > 0 ? x <= b : x >= b; x += s) v.push(x); return v; };
const STEPS = [...ramp(100, 0), ...ramp(0, 150), ...ramp(150, 50), ...ramp(50, 100)];
const ARGS = ["--use-angle=metal", "--enable-gpu", "--ignore-gpu-blocklist", "--enable-unsafe-webgpu",
  "--disable-gpu-vsync", "--disable-frame-rate-limit", "--enable-webgpu-developer-features"];

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
function stats(ms) {
  const s = [...ms].sort((a, b) => a - b);
  return { frames: s.length, p50: round2(pct(s, 0.5)), p95: round2(pct(s, 0.95)), p99: round2(pct(s, 0.99)),
    max: round2(s.at(-1)), mean: round2(s.reduce((a, b) => a + b, 0) / s.length),
    over_16_6ms: s.filter((x) => x > BUDGET_MS).length, over_25ms: s.filter((x) => x > 25).length,
    tail_ms: s.filter((x) => x > 25).map(round2) };
}
const grassOf = (st) => ({ density: st.grass.density, distance: st.grass.distance, status: st.grass.status,
  groups: st.grass.groups, blades: st.grass.blades, triangles: st.grass.triangles, instances: st.grass.instances,
  mesh_bytes: st.grass.mesh_bytes, uploads: st.grass.uploads, last_build_ms: st.grass.last_build_ms,
  last_swap_ms: st.grass.last_swap_ms });
const sceneOf = (st, d) => ({ tiles: st.play?.tiles?.length, tier: st.tier?.tier, tier_changes: st.tier?.changes,
  entities: d.entities, pieces: d.pieces, grass: grassOf(st) });

// Frame intervals from the page's own requestAnimationFrame loop, between `from` and `to` marks.
const recordFrames = (page, ms) => page.evaluate((ms) => new Promise((resolve) => {
  const t = [];
  const end = performance.now() + ms;
  const tick = (now) => { t.push(now); if (now < end) requestAnimationFrame(tick); else resolve(t); };
  requestAnimationFrame(tick);
}), ms);

// The drag and the frame record in one page task: no Playwright round trip between steps. The HUD
// calls `hexWorld.cmd` through the bridge object, so a wrapper there logs every set_grass it sends.
const dragFrames = (page, steps, ms) => page.evaluate(({ steps, ms }) => new Promise((resolve) => {
  const input = document.querySelector("input[aria-label='Grass density']");
  const hw = window.hexWorld;
  const cmd = hw.cmd;
  const sends = [];
  hw.cmd = (c) => {
    for (const x of Array.isArray(c) ? c : [c]) if (x?.cmd === "set_grass") sends.push({ t: performance.now(), density: x.density });
    return cmd.call(hw, c);
  };
  const t = [];
  const every = ms / steps.length;
  const start = performance.now();
  let i = 0;
  const step = () => {
    input.value = String(steps[i]);
    input.dispatchEvent(new Event("input", { bubbles: true }));
    i++;
    if (i < steps.length) setTimeout(step, start + i * every - performance.now());
    else input.dispatchEvent(new Event("change", { bubbles: true }));
  };
  const tick = (now) => {
    t.push(now);
    if (i < steps.length || now < start + ms) requestAnimationFrame(tick);
    else { hw.cmd = cmd; resolve({ t, sends }); }
  };
  requestAnimationFrame(tick);
  step();
}), { steps, ms });
// Each frame over 25 ms of the drag with the time since the latest set_grass sent before it ended.
const dragTail = (t, sends) => t.slice(1).flatMap((end, i) => {
  const ms = end - t[i];
  if (ms <= 25) return [];
  const prior = sends.filter((x) => x.t <= end).at(-1);
  return [{ ms: round2(ms), at_ms: round2(end - t[0]), since_send_ms: prior ? round2(end - prior.t) : null }];
});
const intervals = (t) => t.slice(1).map((x, i) => x - t[i]);

// Untimed: grow the grass distance to Far and read state().grass every frame until its groups settle.
const buildPhase = (page) => page.evaluate(() => new Promise((resolve) => {
  const hw = window.hexWorld;
  const g0 = hw.state().grass;
  const samples = [];
  let prev = g0, still = 0;
  hw.cmd({ cmd: "set_grass_distance", scale: 2 });
  const tick = () => {
    const g = hw.state().grass;
    const built = g.uploads - prev.uploads;
    if (built > 0) samples.push({ built, ms: g.last_build_ms });
    still = g.uploads === prev.uploads && g.groups === prev.groups ? still + 1 : 0;
    prev = g;
    if (still >= 120) resolve({ before: g0, after: g, samples });
    else requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
}));
const dropPhase = (page) => page.evaluate(() => new Promise((resolve) => {
  const hw = window.hexWorld;
  hw.cmd({ cmd: "set_grass_distance", scale: 1 });
  let prev = hw.state().grass, still = 0;
  const tick = () => {
    const g = hw.state().grass;
    still = g.groups === prev.groups ? still + 1 : 0;
    prev = g;
    if (still >= 60) resolve(g);
    else requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
}));
// `last_build_ms` is the slowest single tile build (seeded set and instance records) of the latest
// frame that built one; the upload counter can trail the build by a frame, so each sample is that
// frame's slowest tile, an upper bound on the tiles beside it.
function buildStats(p) {
  const slowest = p.samples.map((s) => s.ms).sort((a, b) => a - b);
  return { groups_built: p.after.uploads - p.before.uploads, frames_building: p.samples.length,
    slowest_tile_per_frame_ms: slowest.length ? { p50: round2(pct(slowest, 0.5)), p95: round2(pct(slowest, 0.95)),
      max: round2(slowest.at(-1)), mean: round2(slowest.reduce((a, b) => a + b, 0) / slowest.length) } : null };
}

async function run() {
  const browser = await chromium.launch({ channel: "chromium", headless: true, args: ARGS });
  const errors = [];
  try {
    const page = await browser.newPage({ viewport: { width: W, height: H }, deviceScaleFactor: 1 });
    page.on("console", (m) => { if (m.type() === "error" || BAD_TEXT.test(m.text())) errors.push(m.text()); });
    page.on("pageerror", (e) => errors.push(String(e)));
    await page.goto(base + "?" + new URLSearchParams(KEYS));
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
    // A still window with the Settings sheet closed: the scene's own frame time on this camera.
    const f1 = await page.evaluate(() => window.hexWorld.state().frames);
    await page.waitForFunction((f) => window.hexWorld.state().frames >= f, f1 + SETTLE_FRAMES, { timeout: 120_000, polling: 250 });
    const still_closed = stats(intervals(await recordFrames(page, BASELINE_MS)));
    // The Settings sheet is open for the baseline too, so the two windows differ only by the drag.
    await page.locator(".tool[data-action=settings]").click();
    await page.waitForFunction(() => document.querySelector(".sheet--settings:not(.is-closed)"), null, { timeout: 30_000 });
    const slider = page.locator("input[aria-label='Grass density']");
    await slider.scrollIntoViewIfNeeded();
    await page.waitForFunction(() => !document.querySelector("input[aria-label='Grass density']")?.disabled, null, { timeout: 30_000 });
    // Settle: the grass groups within reach of the still camera are built and stay put.
    let settled = null;
    for (let i = 0; i < 60; i++) {
      const f0 = await page.evaluate(() => window.hexWorld.state().frames);
      await page.waitForFunction((f) => window.hexWorld.state().frames >= f, f0 + SETTLE_FRAMES / 3, { timeout: 120_000, polling: 250 });
      const g = await page.evaluate(() => window.hexWorld.state().grass);
      if (settled && g.groups === settled.groups && g.uploads === settled.uploads && g.groups > 0) break;
      settled = g;
    }
    const at100 = await page.evaluate(() => window.hexWorld.state());
    if (at100.tier?.tier !== "high") throw new Error(`tier ${at100.tier?.tier}, expected high`);
    if (at100.grass?.density !== 1) throw new Error(`grass density ${at100.grass?.density}, expected 1`);
    if (at100.play?.tiles?.length !== world.tiles.length) throw new Error(`world has ${at100.play?.tiles?.length} tiles`);
    if (at100.tilt?.enabled || at100.play?.rising?.length) throw new Error("world must be settled with tilt off");
    const before = sceneOf(at100, await page.evaluate(() => window.hexWorld.diag()));

    const baseline = stats(intervals(await recordFrames(page, BASELINE_MS)));
    const dragged = await dragFrames(page, STEPS, DRAG_MS);
    const drag = { ...stats(intervals(dragged.t)), tail: dragTail(dragged.t, dragged.sends) };
    const sent = dragged.sends.map((x) => x.density);
    const sends = { count: sent.length, min: Math.min(...sent), max: Math.max(...sent), last: sent.at(-1),
      mean_gap_ms: sent.length > 1 ? round2((dragged.sends.at(-1).t - dragged.sends[0].t) / (sent.length - 1)) : null,
      densities: sent };
    // The throttle sends the latest value at most every 150 ms, so a step through an end may be skipped.
    if (sends.count < 25 || sends.min > 0.1 || sends.max < 1.4 || sends.last !== 1)
      throw new Error(`the drag did not drive set_grass through the slider: ${JSON.stringify(sends)}`);
    // Let the release's set_grass land, then read the scene once.
    await page.waitForFunction(() => window.hexWorld.state().grass.density === 1, null, { timeout: 10_000, polling: 100 });
    const afterSt = await page.evaluate(() => window.hexWorld.state());
    const after = sceneOf(afterSt, await page.evaluate(() => window.hexWorld.diag()));
    const rebuild = {
      uploads_delta: after.grass.uploads - before.grass.uploads,
      groups_delta: after.grass.groups - before.grass.groups,
      instances_delta: after.grass.instances - before.grass.instances,
      tier_changes_delta: after.tier_changes - before.tier_changes,
      tiles_delta: after.tiles - before.tiles,
      entities_delta: after.entities - before.entities,
      last_swap_ms_after: after.grass.last_swap_ms,
    };
    rebuild.none = Object.entries(rebuild).filter(([k]) => k.endsWith("_delta")).every(([, v]) => v === 0);

    // Still windows at 0% and 150% (the slider's ends) on the same camera: what the grass itself costs.
    const setGrass = async (density) => {
      await page.evaluate((d) => window.hexWorld.cmd({ cmd: "set_grass", density: d }), density);
      await page.waitForFunction((d) => window.hexWorld.state().grass.density === d, density, { timeout: 10_000, polling: 100 });
    };
    await setGrass(0);
    const still_0 = stats(intervals(await recordFrames(page, BASELINE_MS)));
    // Blade counts and bytes at 100% and 150% over the same still camera.
    await setGrass(1.5);
    const still_150 = stats(intervals(await recordFrames(page, BASELINE_MS)));
    const at150 = grassOf(await page.evaluate(() => window.hexWorld.state()));
    await page.evaluate(() => window.hexWorld.cmd({ cmd: "set_grass", density: 1 }));
    await page.waitForFunction(() => window.hexWorld.state().grass.density === 1, null, { timeout: 10_000, polling: 100 });

    // Build cost per tile at 100% then 150%: Far reaches tiles the camera's Near radius leaves out.
    const build100 = await buildPhase(page);
    await dropPhase(page);
    await page.evaluate(() => window.hexWorld.cmd({ cmd: "set_grass", density: 1.5 }));
    await page.waitForFunction(() => window.hexWorld.state().grass.density === 1.5, null, { timeout: 10_000, polling: 100 });
    const build150 = await buildPhase(page);
    const build_id = await page.evaluate(() => window.__HEX_BUILD ?? null);
    const frame_latency = await page.evaluate(() => window.__frameLatency ? { limit: window.__frameLatency.limit, source: window.__frameLatency.source } : null);
    return { build_id, browser_version: browser.version(), renderer, webgpu_adapter: adapter, frame_latency,
      tiles: before.tiles, camera: at100.camera, steps: STEPS.length, step_ms: round2(DRAG_MS / STEPS.length),
      still_closed, baseline, drag, sends, still_0, still_150, before, after, rebuild,
      counts: { at_100: before.grass, at_150: at150,
        bytes_per_group: before.grass.groups ? Math.round(before.grass.mesh_bytes / before.grass.groups) : null },
      build: { at_100: { ...buildStats(build100), far: grassOf({ grass: build100.after }) },
        at_150: { ...buildStats(build150), far: grassOf({ grass: build150.after }) } },
      console_errors: errors };
  } finally {
    await browser.close();
  }
}

const result = { date: new Date().toISOString(), viewport: `${W}x${H}`, device_scale_factor: 1, keys: KEYS, camera: CAMERA,
  world: { generator: `e2e/water-world.mjs waterWorldScript(${RADIUS})`, tiles: world.tiles.length, counts: world.counts },
  drag: { percents: STEPS, ms: DRAG_MS, baseline_ms: BASELINE_MS, path: "Settings sheet slider: input per step, change on release (150 ms throttle, flush)" },
  budget_p95_ms: BUDGET_MS, load_limit: LOAD_LIMIT, runs: RUNS,
  load_rule: "1-min load average <= load_limit immediately before and after every run; over-limit samples discarded and re-run",
  web: { browser: "chromium-headless-gpu-uncapped", channel: "chromium", args: ARGS.join(" ") },
  rows: [] };
try {
  for (let r = 0; r < RUNS; r++) {
    const label = `grass drag run ${r}`;
    const g = await guarded(() => run().catch((e) => ({ error: String(e.message ?? e) })), label);
    result.rows.push({ run: r, ...g.result, load_before: g.load_before, load_after: g.load_after, load_discarded: g.load_discarded });
    const x = result.rows.at(-1);
    console.log(`${label}: ${x.error ?? `baseline p50 ${x.baseline.p50} p95 ${x.baseline.p95} max ${x.baseline.max} over25 ${x.baseline.over_25ms} | ` +
      `drag p50 ${x.drag.p50} p95 ${x.drag.p95} max ${x.drag.max} over25 ${x.drag.over_25ms} sends ${x.sends.count} | ` +
      `closed p95 ${x.still_closed.p95} | rebuild none ${x.rebuild.none}`}` +
      ` (load ${g.load_before.toFixed(2)} -> ${g.load_after.toFixed(2)})`);
    await writeFile(OUT, JSON.stringify(result, null, 2) + "\n");
  }
} finally {
  server.close();
}
const ok = result.rows.filter((x) => !x.error);
const median = (v) => { const s = v.filter((x) => typeof x === "number").sort((a, b) => a - b); return s.length ? s[Math.floor(s.length / 2)] : null; };
result.summary = ok.length ? {
  runs: ok.length,
  still_closed_p95_median: median(ok.map((x) => x.still_closed.p95)),
  baseline_p95_median: median(ok.map((x) => x.baseline.p95)), drag_p95_median: median(ok.map((x) => x.drag.p95)),
  baseline_over_25ms: ok.map((x) => x.baseline.over_25ms), drag_over_25ms: ok.map((x) => x.drag.over_25ms),
  still_0_p95_median: median(ok.map((x) => x.still_0.p95)), still_150_p95_median: median(ok.map((x) => x.still_150.p95)),
  drag_minus_baseline_p95_ms: ok.map((x) => round2(x.drag.p95 - x.baseline.p95)),
  drag_minus_baseline_max_ms: ok.map((x) => round2(x.drag.max - x.baseline.max)),
  drag_extra_over_25ms: ok.map((x) => x.drag.over_25ms - x.baseline.over_25ms),
  set_grass_sends: ok.map((x) => x.sends.count),
  no_rebuild: ok.every((x) => x.rebuild.none),
} : null;
await writeFile(OUT, JSON.stringify(result, null, 2) + "\n");
console.log(JSON.stringify(result.summary, null, 2));
console.log(`wrote ${OUT}`);
