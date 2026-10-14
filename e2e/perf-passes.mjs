// P2 performance groundwork (PRD §10, §15 P2; ADR 0001 carry-over): the scripted 2,000-tile world
// (`scenario=gate2000`, instanced layout, the shipped mid600 shadows: all casters, 3 cascades,
// 600 m) on High at 2560x1440, web (uncapped GPU Chromium) and native (`--present immediate`).
// Per round and platform, four interleaved arms:
//   timing off, tilt off  -> the frame-time budget run (p95 <= 16.6 ms) and the stall tail
//   timing off, tilt on   -> the tilt-shift cost on the 2,000-tile workload (minus the arm above)
//   timing on,  tilt off  -> per-pass GPU times (diag.gpu) and stalls joined to them
//   timing on,  tilt on   -> the tilt-shift pass's own GPU segment
// Every run sits inside the host-load guard (perf-load.mjs); each row records its loads.
// Local only. Writes docs/perf/p2-passes.json (PERF_OUT overrides).
// usage: node e2e/perf-passes.mjs <dist> <native-bin>
//   PASSES_ONLY=web|native, PASSES_ROUNDS (default 3), PASSES_VIEW (strategic|closeup)
import { chromium } from "playwright";
import { createServer } from "node:http";
import { spawn } from "node:child_process";
import { readFile, writeFile, mkdir, rm } from "node:fs/promises";
import { extname, join } from "node:path";
import { LOAD_LIMIT, guarded } from "./perf-load.mjs";
import { BAD_TEXT } from "./console-errors.mjs";

const dist = process.argv[2] ?? "dist";
const nativeBin = process.argv[3] ?? "target/release/hex-world";
const OUT = process.env.PERF_OUT ?? "docs/perf/p2-passes.json";
const ONLY = process.env.PASSES_ONLY;
const ROUNDS = Number(process.env.PASSES_ROUNDS ?? 3);
const VIEW = process.env.PASSES_VIEW ?? "strategic";
// Web arms with gpu_timing=on are opt-in (PASSES_WEB_TIMING=on) until the web marker hang is fixed.
const WEB_TIMING = process.env.PASSES_WEB_TIMING === "on";
const W = 2560, H = 1440;
const KEYS = { scenario: "gate2000", view: VIEW, layout: "instanced", chunk: "7", post: "full", shadows: "on",
  shadow_casters: "all", cascades: "3", shadow_dist: "600", tier: "high", fps: "off" };
const ARMS = [
  { timing: "off", tilt: "off" }, { timing: "off", tilt: "on" },
  { timing: "on", tilt: "off" }, { timing: "on", tilt: "on" },
];

const types = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm", ".css": "text/css",
  ".ktx2": "image/ktx2", ".glb": "model/gltf-binary", ".png": "image/png", ".wgsl": "text/plain",
  ".jpg": "image/jpeg", ".woff2": "font/woff2" };
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

const pick = (d) => ({ p50: d.p50, p95: d.p95, p99: d.p99, mean: d.mean, max: d.max,
  dropped_over_25ms: d.dropped_over_25ms, samples: d.samples, frames: d.frames, tiles: d.tiles, ssao: d.ssao,
  post: d.post, adapter: d.adapter, stall: d.stall, gpu: d.gpu?.enabled ? { total: d.gpu.total, passes: d.gpu.passes,
    frames: d.gpu.frames, rejected_non_monotonic: d.gpu.rejected_non_monotonic, tail_over_25ms: d.gpu.tail_over_25ms } : null });
const result = { date: new Date().toISOString(), viewport: `${W}x${H}`, device_scale_factor: 1, keys: KEYS,
  budget_p95_ms: 16.6, load_limit: LOAD_LIMIT, rounds: ROUNDS,
  load_rule: "1-min load average <= load_limit immediately before and after every run; over-limit samples discarded and re-run",
  web: { browser: "chromium-headless-gpu-uncapped", channel: "chromium",
    args: "--use-angle=metal --enable-gpu --ignore-gpu-blocklist --enable-unsafe-webgpu --disable-gpu-vsync --disable-frame-rate-limit --enable-webgpu-developer-features" },
  native: { present: "immediate", exit_after_seconds: 25 }, rows: [] };
const save = async () => {
  await mkdir("docs/perf", { recursive: true });
  await writeFile(OUT, JSON.stringify(result, null, 2) + "\n");
};

async function webRun(arm) {
  // Developer features lift Chrome's 100 µs timestamp quantization; they change no rendering.
  const browser = await chromium.launch({ channel: "chromium", headless: true,
    args: ["--use-angle=metal", "--enable-gpu", "--ignore-gpu-blocklist", "--enable-unsafe-webgpu",
      "--disable-gpu-vsync", "--disable-frame-rate-limit", "--enable-webgpu-developer-features"] });
  const errors = [];
  try {
    const page = await browser.newPage({ viewport: { width: W, height: H }, deviceScaleFactor: 1 });
    page.on("console", (m) => { if (m.type() === "error" || BAD_TEXT.test(m.text())) errors.push(m.text()); });
    page.on("pageerror", (e) => errors.push(String(e)));
    const q = new URLSearchParams({ ...KEYS, tilt: arm.tilt, gpu_timing: arm.timing });
    await page.goto(base + "?" + q);
    await page.waitForFunction(() => window.hexWorld?.ready(), null, { timeout: 120_000 });
    const adapter = await page.evaluate(async () => {
      const i = (await navigator.gpu.requestAdapter())?.info;
      return i ? [i.vendor, i.architecture, i.description].join(" | ") : null;
    });
    if (!adapter || /swiftshader/i.test(adapter)) throw new Error("no hardware WebGPU adapter: " + adapter);
    await page.waitForFunction(() => {
      const d = window.hexWorld.diag();
      return d.window_start_frame > 0 && d.samples >= 600 && d.frames - d.window_start_frame + 1 >= 600;
    }, null, { timeout: 240_000, polling: 250 });
    const d = await page.evaluate(() => window.hexWorld.diag());
    const st = await page.evaluate(() => window.hexWorld.state());
    if (st.tier?.tier !== "high" || st.tilt?.enabled !== (arm.tilt === "on"))
      throw new Error(`state disagrees with the arm: tier ${JSON.stringify(st.tier?.tier)} tilt ${JSON.stringify(st.tilt)}`);
    return { platform: "web", ...arm, webgpu_adapter: adapter, ...pick(d), console_errors: errors };
  } finally {
    await browser.close();
  }
}

async function nativeRun(arm) {
  const dump = `target/perf-passes-${arm.timing}-${arm.tilt}.json`;
  const args = ["--diag", "--present", "immediate", "--size", `${W}x${H}`, "--exit-after-seconds", "25",
    "--dump-state", dump, "--tilt", arm.tilt, "--gpu_timing", arm.timing];
  for (const [k, v] of Object.entries(KEYS)) args.push(`--${k}`, v);
  const code = await new Promise((resolve) => {
    const p = spawn(nativeBin, args, { stdio: "ignore" });
    const killer = setTimeout(() => p.kill("SIGKILL"), 120_000);
    p.on("close", (c) => { clearTimeout(killer); resolve(c); });
  });
  const state = JSON.parse(await readFile(dump, "utf8").catch(() => {
    throw new Error(`native run wrote no state (exit ${code})`);
  }));
  await rm(dump, { force: true });
  const d = state.diag;
  if (!d || d.samples < 600) throw new Error(`native diag window not full (exit ${code}): ${JSON.stringify(d?.samples)}`);
  // A native window takes keyboard focus and `T` toggles tilt-shift: a run whose final state
  // disagrees with its arm is an error, never a sample.
  if (state.tier?.tier !== "high" || state.tilt?.enabled !== (arm.tilt === "on"))
    throw new Error(`state disagrees with the arm: tier ${JSON.stringify(state.tier?.tier)} tilt ${JSON.stringify(state.tilt)}`);
  return { platform: "native", ...arm, exit_code: code, ...pick(d) };
}

try {
  for (let r = 0; r < ROUNDS; r++)
    for (const arm of r % 2 ? [...ARMS].reverse() : ARMS)
      for (const [platform, run] of [["web", webRun], ["native", nativeRun]]) {
        if (ONLY && ONLY !== platform) continue;
        if (platform === "web" && arm.timing === "on" && !WEB_TIMING) continue;
        const label = `passes ${platform} round ${r} timing=${arm.timing} tilt=${arm.tilt}`;
        const g = await guarded(() => run(arm).catch((e) => ({ platform, ...arm, error: String(e.message ?? e) })), label);
        result.rows.push({ round: r, ...g.result, load_before: g.load_before, load_after: g.load_after, load_discarded: g.load_discarded });
        const x = result.rows.at(-1);
        console.log(`${label}: ${x.error ?? `p95 ${x.p95} mean ${x.mean} over25 ${x.dropped_over_25ms} gpu ${JSON.stringify(x.gpu?.total ?? null)}`} ` +
          `(load ${g.load_before.toFixed(2)} -> ${g.load_after.toFixed(2)})`);
        await save();
      }
} finally {
  server.close();
}
// Medians per platform and arm, and the tilt-shift cost (tilt on minus off, timing off).
const median = (v) => { const s = v.filter((x) => typeof x === "number").sort((a, b) => a - b); return s.length ? s[Math.floor(s.length / 2)] : null; };
result.summary = {};
for (const platform of ["web", "native"]) {
  const rows = result.rows.filter((x) => x.platform === platform && !x.error);
  if (!rows.length) continue;
  const arm = (timing, tilt) => rows.filter((x) => x.timing === timing && x.tilt === tilt);
  const m = (rs, k) => median(rs.map((x) => x[k]));
  const off = arm("off", "off"), on = arm("off", "on");
  result.summary[platform] = {
    budget_run: { p95_median: m(off, "p95"), mean_median: m(off, "mean"), over_25ms_median: m(off, "dropped_over_25ms"),
      pass: m(off, "p95") !== null && m(off, "p95") <= 16.6 },
    tilt_shift_cost_ms: { mean: m(on, "mean") - m(off, "mean"), p95: m(on, "p95") - m(off, "p95") },
    gpu_total_p50_median: median(arm("on", "off").map((x) => x.gpu?.total?.p50)),
    gpu_tilt_segment_p50_median: median(arm("on", "on").map((x) => x.gpu?.passes?.tilt_shift?.p50)),
  };
}
await save();
console.log(JSON.stringify(result.summary, null, 2));
console.log(`wrote ${OUT}`);
