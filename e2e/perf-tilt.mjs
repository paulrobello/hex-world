// Tilt-shift frame-time cost (PRD §9.1): the kit scenario with `tilt=off` vs `tilt=on`, same camera,
// uncapped, 2560x1440, interleaved off/on rounds so slow drift hits both arms. Local only.
//   node e2e/perf-tilt.mjs web dist [rounds]
//   node e2e/perf-tilt.mjs native target/release/hex-world [rounds]
// Every sample runs under the host-load guard (load <= 15 before and after, else discarded and re-run);
// each sample records both loads. Writes docs/perf/p2-tilt-shift-<mode>.json (PERF_OUT overrides).
import { chromium } from "playwright";
import { createServer } from "node:http";
import { spawn } from "node:child_process";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { extname, join } from "node:path";
import { LOAD_LIMIT, guarded } from "./perf-load.mjs";
import { BAD_TEXT } from "./console-errors.mjs";

const [mode = "web", target = mode === "web" ? "dist" : "target/release/hex-world", roundsArg = "5"] = process.argv.slice(2);
const rounds = Number(roundsArg);
const outPath = process.env.PERF_OUT ?? `docs/perf/p2-tilt-shift-${mode}.json`;
const W = 2560, H = 1440;
const pick = (d) => ({ mean: d.mean, p50: d.p50, p95: d.p95, p99: d.p99, samples: d.samples, adapter: d.adapter });

async function webSampler() {
  const types = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm", ".css": "text/css",
    ".ktx2": "image/ktx2", ".glb": "model/gltf-binary", ".png": "image/png", ".wgsl": "text/plain" };
  const server = createServer(async (req, res) => {
    let path = decodeURIComponent(new URL(req.url, "http://x").pathname);
    if (path.endsWith("/")) path += "index.html";
    try {
      const body = await readFile(join(target, path));
      res.writeHead(200, { "content-type": types[extname(path)] ?? "application/octet-stream" });
      res.end(body);
    } catch { res.writeHead(404); res.end(); }
  }).listen(0);
  const base = `http://127.0.0.1:${server.address().port}/`;
  const browser = await chromium.launch({ channel: "chromium", headless: true,
    args: ["--use-angle=metal", "--enable-gpu", "--ignore-gpu-blocklist", "--enable-unsafe-webgpu",
           "--disable-gpu-vsync", "--disable-frame-rate-limit"] });
  const errors = [];
  const sample = async (tilt) => {
    const page = await browser.newPage({ viewport: { width: W, height: H }, deviceScaleFactor: 1 });
    let spawned = false;
    page.on("console", (m) => {
      if (/kit: spawned/.test(m.text())) spawned = true;
      if (m.type() === "error" || BAD_TEXT.test(m.text())) errors.push(m.text());
    });
    page.on("pageerror", (e) => errors.push(String(e)));
    await page.goto(base + `?scenario=kit&fps=off&tilt=${tilt}&tier=high`);
    await page.waitForFunction(() => window.hexWorld?.ready(), null, { timeout: 120_000 });
    const t0 = Date.now();
    while (!spawned && Date.now() - t0 < 60_000) await page.waitForTimeout(100);
    if (!spawned) throw new Error("kit never spawned");
    const f0 = await page.evaluate(() => window.hexWorld.diag().frames);
    // The diag ring holds the last 600 frames: wait until all of them post-date the spawn.
    await page.waitForFunction((f) => { const d = window.hexWorld.diag(); return d.samples >= 600 && d.frames - f >= 750; },
      f0, { timeout: 180_000, polling: 250 });
    const d = await page.evaluate(() => window.hexWorld.diag());
    const st = await page.evaluate(() => window.hexWorld.state());
    if (st.tilt?.enabled !== (tilt === "on")) throw new Error(`tilt state ${JSON.stringify(st.tilt)} for tilt=${tilt}`);
    await page.close();
    return pick(d);
  };
  return { sample, close: async () => { await browser.close(); server.close(); if (errors.length) throw new Error("console errors: " + errors.join(" | ")); } };
}

async function nativeSampler() {
  const dir = await mkdtemp(join(tmpdir(), "tilt-"));
  let n = 0;
  const sample = async (tilt) => {
    // A native window takes keyboard focus, and `T` toggles the effect: a stray keypress flips the arm
    // mid-run, so a run whose final state disagrees with its arm is retried, never recorded.
    for (let attempt = 0; attempt < 3; attempt++) {
      const r = await nativeOnce(tilt);
      if (r) return r;
      console.log(`tilt state flipped during tilt=${tilt} run, retrying`);
    }
    throw new Error(`tilt state kept flipping for tilt=${tilt}`);
  };
  const nativeOnce = async (tilt) => {
    const out = join(dir, `state-${n++}.json`);
    const args =["--scenario", "kit", "--tier", "high", "--size", `${W}x${H}`, "--present", "immediate", "--fps", "off", "--diag", "1",
      "--tilt", tilt, "--exit-after-seconds", "25", "--dump-state", out];
    const code = await new Promise((res) => spawn(target, args, { stdio: "ignore" }).on("exit", res));
    if (code !== 0) throw new Error(`native exit ${code}`);
    const st = JSON.parse(await readFile(out, "utf8"));
    if (st.tilt?.enabled !== (tilt === "on")) return null;
    if (st.diag.samples < 600) throw new Error(`only ${st.diag.samples} frame samples`);
    return pick(st.diag);
  };
  return { sample, close: async () => {} };
}

const sampler = await (mode === "web" ? webSampler() : nativeSampler());
const runs = [];
try {
  for (let r = 0; r < rounds; r++) {
    for (const tilt of r % 2 ? ["on", "off"] : ["off", "on"]) {
      const g = await guarded(() => sampler.sample(tilt), `tilt=${tilt} round ${r}`);
      runs.push({ round: r, tilt, ...g.result, load_before: g.load_before, load_after: g.load_after, load_discarded: g.load_discarded });
      console.log(`round ${r} tilt=${tilt}: mean ${g.result.mean} ms p50 ${g.result.p50} p95 ${g.result.p95} (load ${g.load_before.toFixed(2)} -> ${g.load_after.toFixed(2)}, discarded ${g.load_discarded})`);
    }
  }
} finally {
  await sampler.close();
}
const avg = (tilt, k) => {
  const v = runs.filter((x) => x.tilt === tilt).map((x) => x[k]);
  return v.reduce((a, b) => a + b, 0) / v.length;
};
const median = (tilt, k) => {
  const v = runs.filter((x) => x.tilt === tilt).map((x) => x[k]).sort((a, b) => a - b);
  return v[Math.floor(v.length / 2)];
};
const summary = Object.fromEntries(["mean", "p50", "p95"].map((k) => [k, {
  off_avg: avg("off", k), on_avg: avg("on", k), cost_avg_ms: avg("on", k) - avg("off", k),
  off_median: median("off", k), on_median: median("on", k), cost_median_ms: median("on", k) - median("off", k) }]));
// A p50 within 0.3 ms of a 60/120 Hz interval means presentation pacing, not render cost: the delta of
// such a pair is meaningless (native on macOS pins here even with `--present immediate`; see p0-gate-matrix.json).
const vsync_like = runs.filter((x) => [16.67, 8.33].some((v) => Math.abs(x.p50 - v) <= 0.3)).length;
const result = { mode, vsync_like_runs: vsync_like, scenario: "kit", resolution: `${W}x${H}`, rounds, load_limit: LOAD_LIMIT, summary, runs };
await writeFile(outPath, JSON.stringify(result, null, 2) + "\n");
console.log(JSON.stringify(summary, null, 2));
console.log("wrote", outPath);
