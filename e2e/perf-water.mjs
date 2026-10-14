// Water look-dev frame time (PRD §9.1, §10): scenario=water at 2560x1440, web (uncapped GPU Chromium,
// as perf-gate.mjs) and native (`--present immediate`), per tier and view. Every run sits inside the
// host-load guard (perf-load.mjs). Local only. Writes docs/perf/p2-water.json (PERF_OUT overrides).
// usage: node e2e/perf-water.mjs <dist> <native-bin>
import { chromium } from "playwright";
import { createServer } from "node:http";
import { spawn } from "node:child_process";
import { readFile, writeFile, mkdir, rm } from "node:fs/promises";
import { extname, join } from "node:path";
import { LOAD_LIMIT, guarded } from "./perf-load.mjs";
import { BAD_TEXT } from "./console-errors.mjs";

const dist = process.argv[2] ?? "dist";
const nativeBin = process.argv[3] ?? "target/release/hex-world";
const OUT = process.env.PERF_OUT ?? "docs/perf/p2-water.json";
const TIERS = (process.env.WATER_TIERS ?? "high,low").split(",");
const ONLY = process.env.WATER_ONLY; // "web" | "native"
// `overview` is the scenario's start camera; `ocean` is the checkpoint ocean still, water filling
// most of the frame (scripts/shots/water.json).
const stills = JSON.parse(await readFile("scripts/shots/water.json", "utf8"));
const oceanCam = stills[stills.findIndex((c) => c.name === "water-ocean") - 2];
const VIEWS = { overview: null, ocean: oceanCam };

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
const base = `http://127.0.0.1:${server.address().port}/`;

const pick = (d) => ({ p50: d.p50, p95: d.p95, p99: d.p99, mean: d.mean, max: d.max, samples: d.samples,
  frames: d.frames, window_start_frame: d.window_start_frame, adapter: d.adapter, ssao: d.ssao, post: d.post });
const result = { date: new Date().toISOString(), viewport: "2560x1440", budget_p95_ms: 16.6, load_limit: LOAD_LIMIT,
  load_rule: "1-min load average <= load_limit immediately before and after every run; over-limit samples discarded and re-run",
  web: { browser: "chromium-headless-gpu-uncapped", args: "--use-angle=metal --enable-gpu --ignore-gpu-blocklist" },
  native: { present: "immediate", exit_after_seconds: 25 }, rows: [] };
const save = async () => {
  await mkdir("docs/perf", { recursive: true });
  await writeFile(OUT, JSON.stringify(result, null, 2) + "\n");
};

async function webRun(tier, view) {
  const browser = await chromium.launch({ channel: "chromium", headless: true,
    args: ["--use-angle=metal", "--enable-gpu", "--ignore-gpu-blocklist", "--enable-unsafe-webgpu",
      "--disable-gpu-vsync", "--disable-frame-rate-limit"] });
  const errors = [];
  try {
    const page = await browser.newPage({ viewport: { width: 2560, height: 1440 }, deviceScaleFactor: 1 });
    page.on("console", (m) => { if (m.type() === "error" || BAD_TEXT.test(m.text())) errors.push(m.text()); });
    page.on("pageerror", (e) => errors.push(String(e)));
    await page.goto(base + `?scenario=water&tier=${tier}&fps=off`);
    await page.waitForFunction(() => window.hexWorld?.ready(), null, { timeout: 120_000 });
    const adapter = await page.evaluate(async () => {
      const i = (await navigator.gpu.requestAdapter())?.info;
      return i ? [i.vendor, i.architecture, i.description].join(" | ") : null;
    });
    if (!adapter || /swiftshader/i.test(adapter)) throw new Error("no hardware WebGPU adapter: " + adapter);
    if (VIEWS[view]) await page.evaluate((j) => window.hexWorld.cmd(j), JSON.stringify(VIEWS[view]));
    const from = (await page.evaluate(() => window.hexWorld.diag())).frames ?? 0;
    // A full 600-frame window entirely after the camera move (diag refreshes every 30 frames).
    await page.waitForFunction((f) => {
      const d = window.hexWorld.diag();
      return d.samples >= 600 && d.frames >= f + 660;
    }, from, { timeout: 180_000, polling: 250 });
    const d = await page.evaluate(() => window.hexWorld.diag());
    return { platform: "web", tier, view, webgpu_adapter: adapter, ...pick(d), console_errors: errors };
  } finally {
    await browser.close();
  }
}

async function nativeRun(tier, view) {
  const dump = `target/perf-water-${tier}-${view}.json`;
  const script = `target/perf-water-${view}.json`;
  await writeFile(script, JSON.stringify(VIEWS[view] ? [VIEWS[view]] : []));
  const args = [nativeBin, "--scenario", "water", "--tier", tier, "--diag", "--present", "immediate", "--fps", "off",
    "--size", "2560x1440", "--exit-after-seconds", "25", "--dump-state", dump, "--script", script];
  const code = await new Promise((resolve) => {
    const p = spawn(args[0], args.slice(1), { stdio: "ignore" });
    const killer = setTimeout(() => p.kill("SIGKILL"), 90_000);
    p.on("close", (c) => { clearTimeout(killer); resolve(c); });
  });
  const state = JSON.parse(await readFile(dump, "utf8").catch(() => {
    throw new Error(`native run wrote no state (exit ${code})`);
  }));
  await rm(dump, { force: true });
  const d = state.diag;
  if (!d || d.samples < 600) throw new Error(`native diag window not full (exit ${code}): ${JSON.stringify(d?.samples)}`);
  return { platform: "native", tier, view, exit_code: code, ...pick(d) };
}

try {
  for (const tier of TIERS)
    for (const view of Object.keys(VIEWS))
      for (const [platform, run] of [["web", webRun], ["native", nativeRun]]) {
        if (ONLY && ONLY !== platform) continue;
        const g = await guarded(() => run(tier, view).catch((e) => ({ platform, tier, view, error: String(e.message ?? e) })),
          `water ${platform} ${tier} ${view}`);
        result.rows.push({ ...g.result, load_before: g.load_before, load_after: g.load_after, load_discarded: g.load_discarded });
        console.log(JSON.stringify(result.rows.at(-1)));
        await save();
      }
} finally {
  server.close();
}
await save();
console.log(`wrote ${OUT}`);
