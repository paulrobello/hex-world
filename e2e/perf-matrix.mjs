// Engine gate diagnosis matrix: web (uncapped GPU Chromium, same rules as perf-gate.mjs) plus
// native `--present immediate` runs. Every config runs MATRIX_RUNS times (default 3), round-robin
// so drift never lands on one config, each web run in a fresh browser. Writes
// docs/perf/p0-gate-matrix.json after every run: raw runs plus per-config median and min–max.
// Local only.
import { chromium } from "playwright";
import { createServer } from "node:http";
import { spawn } from "node:child_process";
import { readFile, writeFile, mkdir, rm } from "node:fs/promises";
import { extname, join } from "node:path";
import { LOAD_LIMIT, guarded } from "./perf-load.mjs";
import { BAD_TEXT } from "./console-errors.mjs";

const dist = process.argv[2] ?? "dist";
const nativeBin = process.argv[3] ?? "target/release/hex-world";
// MATRIX_PACED=1 keeps Chromium's vsync and frame-rate limit (a shipping 60 Hz app never queues
// frames behind the GPU); MATRIX_CONFIGS limits the configs; MATRIX_OUT redirects the JSON.
const PACED = process.env.MATRIX_PACED === "1";
const OUT = process.env.MATRIX_OUT ?? "docs/perf/p0-gate-matrix.json";
const views = ["strategic", "closeup"];

const RUNS = Number(process.env.MATRIX_RUNS ?? 3);
// Task 14 configurations: the entities baseline, the instanced shipping layout with full
// shadows, with tuned shadows, and with shadows and post off.
const configs = [
  { id: "entities-full", layout: "entities", post: "full", shadows: "on", chunk: null,
    shadow_casters: "all", cascades: 4, shadow_dist: 2000 },
  { id: "instanced-full", layout: "instanced", post: "full", shadows: "on", chunk: 7,
    shadow_casters: "all", cascades: 4, shadow_dist: 2000 },
  { id: "instanced-tuned", layout: "instanced", post: "full", shadows: "on", chunk: 7,
    shadow_casters: "large", cascades: 3, shadow_dist: 2000 },
  { id: "instanced-bare", layout: "instanced", post: "none", shadows: "off", chunk: 7,
    shadow_casters: "all", cascades: 4, shadow_dist: 2000 },
  // Middle shadow settings (Task 14 fix round 1): every piece casts, 3 cascades, shorter reach.
  { id: "instanced-mid600", layout: "instanced", post: "full", shadows: "on", chunk: 7,
    shadow_casters: "all", cascades: 3, shadow_dist: 600 },
  { id: "instanced-mid1200", layout: "instanced", post: "full", shadows: "on", chunk: 7,
    shadow_casters: "all", cascades: 3, shadow_dist: 1200 },
];
// Run by default: the four Task 14 configs. MATRIX_CONFIGS selects any subset, including the
// middle settings.
const DEFAULT_CONFIGS = ["entities-full", "instanced-full", "instanced-tuned", "instanced-bare"];
const onlyConfigs = process.env.MATRIX_CONFIGS?.split(",") ?? DEFAULT_CONFIGS;
const web = [];
const native = [];
for (const c of configs.filter((c) => !onlyConfigs || onlyConfigs.includes(c.id)))
  for (const view of views) {
    web.push({ ...c, view });
    native.push({ ...c, view });
  }
const query = (req) => `?scenario=gate2000&view=${req.view}&layout=${req.layout}&post=${req.post}` +
  `&shadows=${req.shadows}&shadow_casters=${req.shadow_casters}&cascades=${req.cascades}` +
  `&shadow_dist=${req.shadow_dist}` + (req.chunk ? `&chunk=${req.chunk}` : "");

const only = process.env.MATRIX_ONLY; // "web" | "native" to run one half
const result = { date: new Date().toISOString(), budget_p95_ms: 16.6, viewport: "2560x1440",
  window: "600 frames after READY (frame 121 on)", runs_per_config: RUNS,
  web: { browser: PACED ? "chromium-headless-gpu, vsync and frame-rate limit on (paced), fresh browser per run"
    : "chromium-headless-gpu-uncapped, fresh browser per run" },
  native: { present: "immediate", exit_after_seconds: 20 }, load_limit: LOAD_LIMIT,
  load_rule: "1-min load average <= load_limit immediately before and after every run; over-limit samples discarded and re-run",
  summary: [], rows: [] };
const median = (xs) => { const v = [...xs].sort((a, b) => a - b); const n = v.length;
  return n ? (n % 2 ? v[(n - 1) / 2] : (v[n / 2 - 1] + v[n / 2]) / 2) : null; };
// 60 Hz and 120 Hz frame intervals; a p50 within 0.3 ms of one suggests presentation pacing.
const vsyncLike = (p50) => [16.67, 8.33].some((t) => Math.abs(p50 - t) <= 0.3);
function summarize() {
  result.summary = [];
  for (const platform of ["web", "native"])
    for (const c of configs)
      for (const view of views) {
        const rs = result.rows.filter((r) => r.platform === platform && r.id === c.id && r.view === view);
        const ok = rs.filter((r) => !r.error && !r.echo_mismatch.length);
        if (!rs.length) continue;
        const p50s = ok.map((r) => r.p50), p95s = ok.map((r) => r.p95), means = ok.map((r) => r.mean);
        const p50 = median(p50s), p95 = median(p95s);
        result.summary.push({ platform, id: c.id, view, runs: rs.length, ok_runs: ok.length,
          p50_median: p50, p50_min: ok.length ? Math.min(...p50s) : null, p50_max: ok.length ? Math.max(...p50s) : null,
          p95_median: p95, p95_min: ok.length ? Math.min(...p95s) : null, p95_max: ok.length ? Math.max(...p95s) : null,
          max_max: ok.length ? Math.max(...ok.map((r) => r.max)) : null,
          dropped_over_25ms_per_run: ok.map((r) => r.dropped_over_25ms),
          mean_median: median(means), mean_min: ok.length ? Math.min(...means) : null,
          mean_max: ok.length ? Math.max(...means) : null,
          pass_budget_median: p95 !== null && p95 <= result.budget_p95_ms,
          vsync_like_runs: ok.filter((r) => vsyncLike(r.p50)).length,
          max_load_accepted: Math.max(...rs.map((r) => Math.max(r.load_before ?? 0, r.load_after ?? 0))) });
      }
}
const save = async () => {
  summarize();
  await mkdir("docs/perf", { recursive: true });
  await writeFile(OUT, JSON.stringify(result, null, 2) + "\n");
};

// What the app echoes back must match what was requested, or the row measured something else.
function echoMismatch(req, d) {
  const bad = [];
  if (d.layout !== req.layout) bad.push(`layout=${d.layout}`);
  if (d.shadows !== (req.shadows === "on")) bad.push(`shadows=${d.shadows}`);
  const post = [...(d.post ?? [])].sort().join(",");
  if (req.post === "full" && post !== "bloom,dof,ssao,taa") bad.push(`post=${post}`);
  if (req.post === "none" && post !== "") bad.push(`post=${post}`);
  if (req.layout !== "entities" && d.chunk_size !== req.chunk) bad.push(`chunk_size=${d.chunk_size}`);
  if (d.shadow_casters !== req.shadow_casters) bad.push(`shadow_casters=${d.shadow_casters}`);
  if (d.cascades !== req.cascades) bad.push(`cascades=${d.cascades}`);
  if (Math.round(d.shadow_dist) !== req.shadow_dist) bad.push(`shadow_dist=${d.shadow_dist}`);
  if (d.tiles !== 2000 || d.pieces !== 40000) bad.push(`tiles=${d.tiles} pieces=${d.pieces}`);
  return bad;
}

function row(platform, req, d, extra = {}) {
  const r = { platform, ...req, p50: d?.p50 ?? null, p95: d?.p95 ?? null, p99: d?.p99 ?? null,
    mean: d?.mean ?? null, max: d?.max ?? null, dropped_over_25ms: d?.dropped_over_25ms ?? null,
    pass_budget: d ? d.p95 <= result.budget_p95_ms : false,
    entities: d?.entities ?? null, chunks: d?.chunks ?? null,
    visible_entities: d?.visible_entities ?? null, visible_mesh_material_pairs: d?.visible_mesh_material_pairs ?? null,
    visible_tris: d?.visible_tris ?? null, visible_by_lod: d?.visible_by_lod ?? null,
    samples: d?.samples ?? null, adapter: d?.adapter ?? null,
    gpu_preprocessing_supported: d?.gpu_preprocessing_supported ?? null,
    wasm_memory_bytes: d?.wasm_memory_bytes || null, echo_mismatch: d ? echoMismatch(req, d) : [],
    passes: d?.passes ?? null, instanced: d?.instanced ?? null, ...extra };
  console.log(`${platform} ${req.id} ${req.view} run${extra.run}: ` +
    (r.error ? `ERROR ${r.error}` : `p50=${r.p50} p95=${r.p95} p99=${r.p99} mean=${r.mean} max=${r.max} dropped=${r.dropped_over_25ms} ents=${r.entities} vis=${r.visible_entities} pairs=${r.visible_mesh_material_pairs}`));
  return r;
}

async function runWeb() {
  const types = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm",
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
  // One browser per row, so no row inherits another row's GPU-process state (chunk rows load GBs).
  const launch = () => chromium.launch({ channel: "chromium", headless: true,
    args: ["--use-angle=metal", "--enable-gpu", "--ignore-gpu-blocklist", "--enable-unsafe-webgpu",
           ...(PACED ? [] : ["--disable-gpu-vsync", "--disable-frame-rate-limit"])] });
  try {
    const webRun = async (req, run) => {
      let out;
      const browser = await launch();
      const page = await browser.newPage({ viewport: { width: 2560, height: 1440 }, deviceScaleFactor: 1 });
      const errors = [];
      page.on("console", (m) => { if (m.type() === "error" || BAD_TEXT.test(m.text())) errors.push(m.text().slice(0, 300)); });
      page.on("pageerror", (e) => errors.push(String(e).slice(0, 300)));
      const crashed = new Promise((_, rej) => page.on("crash", () => rej(new Error("page crashed"))));
      crashed.catch(() => {});
      const t0 = Date.now();
      try {
        await Promise.race([crashed, (async () => {
          await page.goto(base + query(req));
          if (!result.web.webgpu_adapter) {
            const gpu = await page.evaluate(async () => {
              const a = await navigator.gpu.requestAdapter();
              return a ? [a.info.vendor, a.info.architecture, a.info.description].join(" | ") : null;
            });
            if (!gpu || /swiftshader/i.test(gpu)) throw new Error(`no hardware WebGPU adapter: ${gpu}`);
            result.web.webgpu_adapter = gpu;
            result.web.ua = await page.evaluate(() => navigator.userAgent);
          }
          await page.waitForFunction(() => window.hexWorld?.ready(), null, { timeout: 240_000 });
          const readyS = (Date.now() - t0) / 1000;
          await page.waitForFunction(() => {
            const d = window.hexWorld.diag();
            return d.window_start_frame > 0 && d.samples >= 600 && d.frames - d.window_start_frame + 1 >= 600;
          }, null, { timeout: 240_000, polling: 250 });
          const d = await page.evaluate(() => window.hexWorld.diag());
          out = row("web", req, d, { run, ready_s: readyS, console_errors: errors });
        })()]);
      } catch (e) {
        out = row("web", req, null, { run, error: String(e.message ?? e).slice(0, 300), console_errors: errors });
      }
      await page.close().catch(() => {});
      await browser.close().catch(() => {});
      return out;
    };
    for (let run = 1; run <= RUNS; run++) for (const req of web) {
      const g = await guarded(() => webRun(req, run), `web ${req.id} ${req.view} run${run}`);
      result.rows.push({ ...g.result, load_before: g.load_before, load_after: g.load_after,
        load_discarded: g.load_discarded });
      await save();
    }
  } finally {
    server.close();
  }
}

function runNativeRow(req, run) {
  const dump = `target/perf-matrix-${req.id}-${req.view}.json`;
  const args = ["-l", nativeBin, "--scenario", "gate2000", "--diag", "--present", "immediate",
    "--size", "2560x1440", "--exit-after-seconds", "20", "--dump-state", dump,
    "--view", req.view, "--layout", req.layout, "--post", req.post, "--shadows", req.shadows,
    "--shadow_casters", req.shadow_casters, "--cascades", String(req.cascades),
    "--shadow_dist", String(req.shadow_dist),
    ...(req.chunk ? ["--chunk", String(req.chunk)] : [])];
  return new Promise((resolve) => {
    let stderr = "";
    const p = spawn("/usr/bin/time", args, { stdio: ["ignore", "ignore", "pipe"], detached: true });
    let timedOut = false;
    const killer = setTimeout(() => { timedOut = true; try { process.kill(-p.pid, "SIGKILL"); } catch { p.kill("SIGKILL"); } }, 60_000);
    p.stderr.on("data", (b) => { stderr += b; });
    p.on("close", async (code) => {
      clearTimeout(killer);
      const num = (re) => { const m = stderr.match(re); return m ? Number(m[1]) : null; };
      const extra = { run, exit_code: code, max_rss_bytes: num(/(\d+)\s+maximum resident set size/),
        peak_memory_footprint_bytes: num(/(\d+)\s+peak memory footprint/) };
      try {
        if (timedOut) throw new Error("native run killed after 60 s timeout");
        const state = JSON.parse(await readFile(dump, "utf8"));
        const d = state.diag;
        if (!d || d.samples < 600) throw new Error(`diag window not full: ${JSON.stringify(d?.samples)}`);
        resolve(row("native", req, d, extra));
      } catch (e) {
        resolve(row("native", req, null, { ...extra, error: String(e.message ?? e).slice(0, 300) }));
      }
      await rm(dump, { force: true });
    });
  });
}

if (only !== "native") await runWeb();
if (only !== "web") {
  for (let run = 1; run <= RUNS; run++) for (const req of native) {
    const g = await guarded(() => runNativeRow(req, run), `native ${req.id} ${req.view} run${run}`);
    result.rows.push({ ...g.result, load_before: g.load_before, load_after: g.load_after,
      load_discarded: g.load_discarded });
    await save();
  }
}
await save();
const failed = result.rows.filter((r) => r.error || r.echo_mismatch.length);
if (failed.length) console.error(`WARN: ${failed.length} row(s) errored or echoed a different config`);
console.log(`wrote ${OUT}`);
