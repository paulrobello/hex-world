// Water- and scatter-heavy play workload (P2 re-run): the 208-tile world of e2e/water-world.mjs
// (ocean, coast, rivers, forests and meadows; the hero-water bar and the prop scatter) built by
// script in the play scenario on High at 2560x1440, DPR 1, then measured once every placement has
// risen and settled. Web (uncapped GPU Chromium, the perf-passes flags, the HUD overlay live as in
// the game) and native (`--present immediate`), tilt-shift on (the play default) and off, interleaved
// rounds, every run inside the host-load guard. The diag ring is rolling and these reads land after
// its first window, so the visible-set census (every 30 frames) is inside every play window.
// Local only. usage: node e2e/perf-water-world.mjs <dist> <native-bin>
//   PERF_OUT (default docs/perf/p2-water-world.json), WW_ROUNDS (default 2), WW_ONLY=web|native
import { chromium } from "playwright";
import { createServer } from "node:http";
import { spawn } from "node:child_process";
import { readFile, writeFile, rm } from "node:fs/promises";
import { extname, join } from "node:path";
import { LOAD_LIMIT, guarded } from "./perf-load.mjs";
import { BAD_TEXT } from "./console-errors.mjs";
import { waterWorldScript } from "./water-world.mjs";

const dist = process.argv[2] ?? "dist";
const nativeBin = process.argv[3] ?? "target/release/hex-world";
const OUT = process.env.PERF_OUT ?? "docs/perf/p2-water-world.json";
const ROUNDS = Number(process.env.WW_ROUNDS ?? 2);
const ONLY = process.env.WW_ONLY;
const W = 2560, H = 1440;
// Frames measured after the world has settled: more than the 600-frame ring.
const SETTLE_FRAMES = 700;
const world = waterWorldScript(8);
const KEYS = { scenario: "play", tier: "high", fps: "off", seed: "1" };

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

// A native window whose frames sit on a display interval was paced, so its p95 is pacing, not cost
// (the rule of perf-matrix.mjs and perf-tilt.mjs; docs/perf/p2-tilt-shift.md). Native only: the
// uncapped web page is not paced (its play frame callback is about 7.3 of 7.8 ms, CPU-bound).
const vsyncLike = (p50) => [16.67, 8.33].some((t) => Math.abs(p50 - t) <= 0.3);
const pick = (d) => ({ p50: d.p50, p95: d.p95, p99: d.p99, mean: d.mean, max: d.max, dropped_over_25ms: d.dropped_over_25ms,
  samples: d.samples, frames: d.frames, window_start_frame: d.window_start_frame, ssao: d.ssao, post: d.post,
  adapter: d.adapter, boids: d.boids, stall: d.stall && { histogram: d.stall.histogram, over_25ms: d.stall.over_25ms,
    tail_ms: d.stall.tail.map((s) => s.ms) } });
const check = (st, tilt) => {
  if (st.tier?.tier !== "high" || st.tilt?.enabled !== (tilt === "on"))
    throw new Error(`state disagrees with the arm: tier ${JSON.stringify(st.tier?.tier)} tilt ${JSON.stringify(st.tilt)}`);
  const n = st.play?.tiles?.length;
  if (n !== world.tiles.length) throw new Error(`world has ${n} tiles, expected ${world.tiles.length}`);
  if (st.play?.rising?.length) throw new Error(`${st.play.rising.length} tiles still rising at the read`);
  return { tiles: n, water_tiles: st.boids?.water_tiles ?? null };
};

async function webRun(tilt) {
  const browser = await chromium.launch({ channel: "chromium", headless: true,
    args: ["--use-angle=metal", "--enable-gpu", "--ignore-gpu-blocklist", "--enable-unsafe-webgpu",
      "--disable-gpu-vsync", "--disable-frame-rate-limit", "--enable-webgpu-developer-features"] });
  const errors = [];
  try {
    const page = await browser.newPage({ viewport: { width: W, height: H }, deviceScaleFactor: 1 });
    page.on("console", (m) => { if (m.type() === "error" || BAD_TEXT.test(m.text())) errors.push(m.text()); });
    page.on("pageerror", (e) => errors.push(String(e)));
    await page.goto(base + "?" + new URLSearchParams({ ...KEYS, tilt }));
    await page.waitForFunction(() => window.hexWorld?.ready(), null, { timeout: 120_000 });
    const adapter = await page.evaluate(async () => {
      const i = (await navigator.gpu.requestAdapter())?.info;
      return i ? [i.vendor, i.architecture, i.description].join(" | ") : null;
    });
    if (!adapter || /swiftshader/i.test(adapter)) throw new Error("no hardware WebGPU adapter: " + adapter);
    await page.evaluate((s) => window.hexWorld.cmd(s), world.script);
    await page.waitForFunction(() => { const s = window.hexWorld.state(); return s.queued === 0 && !s.play?.rising?.length; },
      null, { timeout: 300_000, polling: 250 });
    const f0 = await page.evaluate(() => window.hexWorld.diag().frames);
    await page.waitForFunction((f) => window.hexWorld.diag().frames >= f, f0 + SETTLE_FRAMES, { timeout: 240_000, polling: 250 });
    const d = await page.evaluate(() => window.hexWorld.diag());
    const st = await page.evaluate(() => window.hexWorld.state());
    return { platform: "web", tilt, webgpu_adapter: adapter, settled_at_frame: f0, ...check(st, tilt), ...pick(d), console_errors: errors };
  } finally {
    await browser.close();
  }
}

async function nativeRun(tilt) {
  const dump = `target/perf-water-world-${tilt}.json`;
  const script = `target/perf-water-world-script.json`;
  // At about 7 ms a frame the 2 s rise settles in under 300 frames; 900 more frames fill the ring after it.
  await writeFile(script, JSON.stringify([...world.script, { cmd: "wait", frames: 900 }, { cmd: "exit" }]));
  const args = ["--diag", "--present", "immediate", "--size", `${W}x${H}`, "--dump-state", dump, "--tilt", tilt,
    "--script", script];
  for (const [k, v] of Object.entries(KEYS)) args.push(`--${k}`, v);
  const code = await new Promise((resolve) => {
    const p = spawn(nativeBin, args, { stdio: "ignore" });
    const killer = setTimeout(() => p.kill("SIGKILL"), 240_000);
    p.on("close", (c) => { clearTimeout(killer); resolve(c); });
  });
  const state = JSON.parse(await readFile(dump, "utf8").catch(() => { throw new Error(`native run wrote no state (exit ${code})`); }));
  await rm(dump, { force: true });
  const d = state.diag;
  if (!d || d.samples < 600) throw new Error(`native diag window not full (exit ${code}): ${JSON.stringify(d?.samples)}`);
  return { platform: "native", tilt, vsync_like: vsyncLike(d.p50), exit_code: code, ...check(state, tilt), ...pick(d) };
}

const result = { date: new Date().toISOString(), viewport: `${W}x${H}`, device_scale_factor: 1, keys: KEYS,
  world: { generator: "e2e/water-world.mjs waterWorldScript(8)", tiles: world.tiles.length, counts: world.counts,
    camera: world.script[0] }, budget_p95_ms: 16.6, load_limit: LOAD_LIMIT, rounds: ROUNDS,
  load_rule: "1-min load average <= load_limit immediately before and after every run; over-limit samples discarded and re-run",
  window_note: `read ${SETTLE_FRAMES}+ frames after the last placement settled (web) or 900 frames after the script (native); ` +
    "the rolling 600-frame ring includes the every-30-frame visible-set census; the web page runs the play HUD overlay, which reads hexWorld.state() every animation frame",
  web: { browser: "chromium-headless-gpu-uncapped", channel: "chromium",
    args: "--use-angle=metal --enable-gpu --ignore-gpu-blocklist --enable-unsafe-webgpu --disable-gpu-vsync --disable-frame-rate-limit --enable-webgpu-developer-features" },
  native: { present: "immediate" }, rows: [] };
try {
  for (let r = 0; r < ROUNDS; r++)
    for (const tilt of r % 2 ? ["off", "on"] : ["on", "off"])
      for (const [platform, run] of [["web", webRun], ["native", nativeRun]]) {
        if (ONLY && ONLY !== platform) continue;
        const label = `water-world ${platform} round ${r} tilt=${tilt}`;
        const g = await guarded(() => run(tilt).catch((e) => ({ platform, tilt, error: String(e.message ?? e) })), label);
        result.rows.push({ round: r, ...g.result, load_before: g.load_before, load_after: g.load_after, load_discarded: g.load_discarded });
        const x = result.rows.at(-1);
        console.log(`${label}: ${x.error ?? `p50 ${x.p50} p95 ${x.p95} mean ${x.mean} max ${x.max} over25 ${x.dropped_over_25ms}`} ` +
          `(load ${g.load_before.toFixed(2)} -> ${g.load_after.toFixed(2)})`);
        await writeFile(OUT, JSON.stringify(result, null, 2) + "\n");
      }
} finally {
  server.close();
}
const median = (v) => { const s = v.filter((x) => typeof x === "number").sort((a, b) => a - b); return s.length ? s[Math.floor(s.length / 2)] : null; };
result.summary = {};
for (const platform of ["web", "native"])
  for (const tilt of ["off", "on"]) {
    const rows = result.rows.filter((x) => x.platform === platform && x.tilt === tilt && !x.error);
    if (!rows.length) continue;
    const p95 = median(rows.map((x) => x.p95));
    result.summary[`${platform}_tilt_${tilt}`] = { p95_median: p95, mean_median: median(rows.map((x) => x.mean)),
      over_25ms_median: median(rows.map((x) => x.dropped_over_25ms)), pass: p95 !== null && p95 <= 16.6,
      vsync_like_runs: rows.filter((x) => x.vsync_like).length, runs: rows.length };
  }
await writeFile(OUT, JSON.stringify(result, null, 2) + "\n");
console.log(JSON.stringify(result.summary, null, 2));
console.log(`wrote ${OUT}`);
