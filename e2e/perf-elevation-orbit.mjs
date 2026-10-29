// Raised-world orbit cost (P4): what the per-frame work of a raised world costs while the camera
// moves. A raised world re-runs the CPU `occlude` re-cut in hex-play's affordances whenever the eye
// moves, and its runtime ground has no LOD. One page session builds the raised world
// (docs/checkpoints/elevation/elevation.json) and a flat control (docs/checkpoints/elevation/
// flat-control.json: the same cells, each a level-0 meadow, so the same tile count and footprint;
// roads are left out because a road cannot be placed cell for cell), and for each orbits the camera
// continuously, one `camera` command per frame from a page-side requestAnimationFrame loop, so the
// eye moves every frame. Frame intervals come from that same loop: rAF-to-rAF intervals, not the
// hexWorld.diag() ring perf-close-camera reads, so these numbers do not compare with that script's.
// The control is not content-matched: the raised world also carries roads, a ramp, portals, a river,
// a beach and their cars, water and scatter, so the ratio is an upper bound on elevation's own cost.
// Uncapped GPU Chromium, 2560x1440, DPR 1, scenario=play tier=high tilt=off fps=off seed=1 (the
// perf-close-camera flags). Each sample sits inside e2e/perf-load.mjs's guard (1-minute load <= 15
// before and after, else discarded and re-run). Worlds alternate order each round. Local only.
// usage: node e2e/perf-elevation-orbit.mjs <dist> <out.json>
//   ROUNDS (default 5), WARMUP (default 1: one discarded orbit of the raised world first),
//   ORBIT_MS (default 8000), ORBIT_RADIUS (default 70 m), ORBIT_HEIGHT (default 30 m),
//   ORBIT_PERIOD_S (default 12)
import { chromium } from "playwright";
import { createServer } from "node:http";
import { readFile, writeFile } from "node:fs/promises";
import { extname, join } from "node:path";
import { cpus, hostname } from "node:os";
import { LOAD_LIMIT, guarded } from "./perf-load.mjs";
import { BAD_TEXT } from "./console-errors.mjs";
import { buildWorld } from "./build-world.mjs";

const [dist, OUT] = process.argv.slice(2);
if (!dist || !OUT) throw new Error("usage: node e2e/perf-elevation-orbit.mjs <dist> <out.json>");
const ROUNDS = Number(process.env.ROUNDS ?? 5);
const WARMUP = Number(process.env.WARMUP ?? 1);
const ORBIT_MS = Number(process.env.ORBIT_MS ?? 8000);
const RADIUS = Number(process.env.ORBIT_RADIUS ?? 70);
const HEIGHT = Number(process.env.ORBIT_HEIGHT ?? 30);
const PERIOD_S = Number(process.env.ORBIT_PERIOD_S ?? 12);
const W = 2560, H = 1440;
const SETTLE_FRAMES = 300;
const KEYS = { scenario: "play", tier: "high", tilt: "off", fps: "off", grass: "off", seed: "1" };
const ARGS = ["--use-angle=metal", "--enable-gpu", "--ignore-gpu-blocklist", "--enable-unsafe-webgpu",
  "--disable-gpu-vsync", "--disable-frame-rate-limit", ...(process.env.CHROME_ARGS ?? "").split(" ").filter(Boolean)];
// The orbit's centre: the middle of the elevation world's footprint (hex-core Axial::to_world, 16 m).
const CENTRE = [95, 0, -12];

const load = async (f) => JSON.parse(await readFile(f, "utf8"));
const raisedScript = await load("docs/checkpoints/elevation/elevation.json");
const flatScript = await load("docs/checkpoints/elevation/flat-control.json");
// The start tile every play world opens with comes on top of the script's placements.
const tilesOf = (s) => s.filter((c) => c.cmd === "place").length + 1;
const WORLDS = {
  raised: { script: raisedScript, tiles: tilesOf(raisedScript) },
  flat: { script: flatScript, tiles: tilesOf(flatScript) },
};
if (WORLDS.raised.tiles !== WORLDS.flat.tiles) throw new Error("the control must hold the raised world's tile count");

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
const stats = (ms) => {
  const s = [...ms].sort((a, b) => a - b);
  return { frames: s.length, mean: round2(s.reduce((a, b) => a + b, 0) / s.length), p50: round2(pct(s, 0.5)),
    p95: round2(pct(s, 0.95)), p99: round2(pct(s, 0.99)), max: round2(s.at(-1)) };
};
const mean = (v) => v.reduce((a, b) => a + b, 0) / v.length;

// One orbit window in a single page task: every animation frame moves the eye and sends it as a
// `camera` command, and the frame intervals are recorded from the same loop.
const orbit = (page) => page.evaluate(({ ms, radius, height, period, centre }) => new Promise((resolve) => {
  const hw = window.hexWorld;
  const t = [];
  const start = performance.now();
  const tick = (now) => {
    t.push(now);
    const a = ((now - start) / 1000) * ((2 * Math.PI) / period);
    hw.cmd({ cmd: "camera", pos: [centre[0] + radius * Math.sin(a), height, centre[2] + radius * Math.cos(a)],
      look_at: centre });
    if (now < start + ms) requestAnimationFrame(tick);
    else resolve(t);
  };
  requestAnimationFrame(tick);
}), { ms: ORBIT_MS, radius: RADIUS, height: HEIGHT, period: PERIOD_S, centre: CENTRE });
const intervals = (t) => t.slice(1).map((x, i) => x - t[i]);

async function main() {
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

    const sample = async (name, label) => {
      const w = WORLDS[name];
      await buildWorld(page, [{ cmd: "new_world" }, ...w.script, { cmd: "camera", pos: [CENTRE[0], HEIGHT, CENTRE[2] + RADIUS], look_at: CENTRE }], w.tiles);
      const f = await page.evaluate(() => window.hexWorld.state().frames);
      await page.waitForFunction((n) => window.hexWorld.state().frames >= n, f + SETTLE_FRAMES, { timeout: 120_000, polling: 250 });
      const g = await guarded(async () => intervals(await orbit(page)), label);
      const st = await page.evaluate(() => window.hexWorld.state());
      if (st.play?.tiles?.length !== w.tiles) throw new Error(`${name}: ${st.play?.tiles?.length} tiles, expected ${w.tiles}`);
      if (st.tilt?.enabled || st.play?.rising?.length) throw new Error(`${name}: the world must be settled with tilt off`);
      const levels = [...new Set(st.play.tiles.map((t) => t.level))].sort();
      return { world: name, tiles: w.tiles, levels, ...stats(g.result), load_before: round2(g.load_before),
        load_after: round2(g.load_after), load_discarded: g.load_discarded };
    };

    for (let i = 0; i < WARMUP; i++) {
      const w = await sample("raised", `warm-up ${i}`);
      console.log(`warm-up raised discarded (mean ${w.mean} ms, load ${w.load_before} -> ${w.load_after})`);
    }
    const runs = [];
    for (let r = 0; r < ROUNDS; r++) {
      for (const name of r % 2 === 0 ? ["raised", "flat"] : ["flat", "raised"]) {
        const s = { round: r, ...(await sample(name, `round ${r} ${name}`)) };
        runs.push(s);
        console.log(`round ${r} ${name}: mean ${s.mean} p95 ${s.p95} p99 ${s.p99} ms (load ${s.load_before} -> ${s.load_after})`);
      }
    }
    const per = (name) => {
      const v = runs.filter((s) => s.world === name);
      return { tiles: v[0].tiles, levels: v[0].levels, rounds: v.length, mean: round2(mean(v.map((s) => s.mean))),
        p95: round2(mean(v.map((s) => s.p95))), p99: round2(mean(v.map((s) => s.p99))),
        mean_range: [Math.min(...v.map((s) => s.mean)), Math.max(...v.map((s) => s.mean))] };
    };
    const raised = per("raised"), flat = per("flat");
    const result = {
      script: "e2e/perf-elevation-orbit.mjs", date: new Date().toISOString(), host: hostname(), cpus: cpus().length,
      adapter, dist, viewport: [W, H], keys: KEYS, load_limit: LOAD_LIMIT,
      orbit: { ms: ORBIT_MS, radius_m: RADIUS, height_m: HEIGHT, period_s: PERIOD_S, centre: CENTRE },
      rounds: ROUNDS, warmup: WARMUP, raised, flat,
      ratio: { mean: round2(raised.mean / flat.mean), p95: round2(raised.p95 / flat.p95), p99: round2(raised.p99 / flat.p99) },
      control_note: "flat control holds meadows only; the raised world also carries roads, a ramp, portals, water and cars, so the ratio over-states elevation's own cost",
      runs, errors,
    };
    await writeFile(OUT, JSON.stringify(result, null, 2) + "\n");
    console.log(`raised/flat ratio: mean ${result.ratio.mean}, p95 ${result.ratio.p95}, p99 ${result.ratio.p99}; wrote ${OUT}`);
    if (errors.length > 0) throw new Error(`page errors: ${errors.slice(0, 3).join(" | ")}`);
  } finally {
    await browser.close();
  }
}
main().then(() => { server.close(); process.exit(0); }, (e) => { console.error(e); server.close(); process.exit(1); });
