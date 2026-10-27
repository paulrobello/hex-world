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
import { createServer } from "node:http";
import { readFile, writeFile } from "node:fs/promises";
import { extname, join } from "node:path";
import { cpus, hostname, loadavg } from "node:os";
import { chromium } from "playwright";
import { buildWorld } from "./build-world.mjs";
import { BAD_TEXT } from "./console-errors.mjs";
import { guarded, LOAD_LIMIT } from "./perf-load.mjs";

const [distBefore, distAfter, OUT] = process.argv.slice(2);
if (!OUT) throw new Error("usage: node e2e/perf-close-camera.mjs <dist-before> <dist-after> <out.json>");
const ROUNDS = Number(process.env.ROUNDS ?? 5);
const SAVE = process.env.SAVE ?? "docs/perf/post-arc001-2026-10-16/local-world.hexworld";
const W = 2560;
const H = 1440;
const WINDOW_FRAMES = 700;
const SETTLE_FRAMES = 300;
const KEYS = { scenario: "play", tier: "high", tilt: "off", fps: "off", grass: "off", seed: "1" };
const ARGS = ["--use-angle=metal", "--enable-gpu", "--ignore-gpu-blocklist", "--enable-unsafe-webgpu",
  "--disable-gpu-vsync", "--disable-frame-rate-limit"];

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
const servers = { before: await serve(distBefore), after: await serve(distAfter) };
const baseOf = (b) => `http://127.0.0.1:${servers[b].address().port}/`;
const count = (p) => p.multidraw_sets + p.batchable_bins + p.unbatchable + p.non_mesh;
const saveBytes = [...(await readFile(SAVE))];

async function run(build, worldName) {
  const world = WORLDS[worldName];
  const browser = await chromium.launch({ channel: "chromium", headless: true, args: ARGS });
  const errors = [];
  try {
    const page = await browser.newPage({ viewport: { width: W, height: H }, deviceScaleFactor: 1 });
    page.on("console", (m) => { if (m.type() === "error" || BAD_TEXT.test(m.text())) errors.push(m.text()); });
    page.on("pageerror", (e) => errors.push(String(e)));
    await page.goto(`${baseOf(build)}?${new URLSearchParams(KEYS)}`);
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
      { cmd: "photo", dof: "off", pause_agents: false }]);
    // Every pack streamed and every scatter instance placed before any window opens.
    await page.waitForFunction(() => {
      const s = window.hexWorld.state();
      return Object.values(s.packs ?? {}).every((v) => v === "ready") && !s.scatter?.waiting?.length && s.queued === 0;
    }, null, { timeout: 300_000, polling: 500 });
    const f0 = await page.evaluate(() => window.hexWorld.diag().frames);
    await page.waitForFunction((f) => window.hexWorld.diag().frames >= f, f0 + SETTLE_FRAMES, { timeout: 300_000, polling: 250 });
    const cameras = {};
    for (const [name, c] of Object.entries(world.cameras)) {
      await page.evaluate((j) => window.hexWorld.cmd(j), cam(c));
      const f1 = await page.evaluate(() => window.hexWorld.diag().frames);
      await page.waitForFunction((f) => window.hexWorld.diag().frames >= f, f1 + WINDOW_FRAMES, { timeout: 600_000, polling: 250 });
      const d = await page.evaluate(() => window.hexWorld.diag());
      const st = await page.evaluate(() => window.hexWorld.state());
      if (st.tier?.tier !== "high") throw new Error(`tier ${st.tier?.tier}`);
      if (st.play?.tiles?.length !== world.tiles) throw new Error(`world has ${st.play?.tiles?.length} tiles`);
      cameras[name] = { p50: d.p50, p95: d.p95, p99: d.p99, mean: d.mean, max: d.max,
        dropped_over_25ms: d.dropped_over_25ms, samples: d.samples, window_start: f1, read_at_frame: d.frames,
        draws: (d.passes ?? []).reduce((n, p) => n + count(p), 0),
        passes: Object.fromEntries((d.passes ?? []).map((p) => [`${p.view}/${p.phase}`, count(p)])),
        kit_draws: d.kit_draws ? { slots: d.kit_draws.slots, total: d.kit_draws.total } : null,
        gpu_timing: d.gpu ?? null, camera_pos: st.camera?.pos, load_at_read: loadavg()[0] };
    }
    return { build, world: worldName, ...gpu, cameras, console_errors: errors };
  } finally {
    await browser.close();
  }
}

const result = { date: new Date().toISOString(), host: hostname(), cpu: cpus()[0]?.model, viewport: `${W}x${H}`,
  device_scale_factor: 1, keys: KEYS, save: SAVE, browser: { mode: "chromium headless (new), channel chromium, uncapped",
    args: ARGS.join(" ") }, window_frames: WINDOW_FRAMES, settle_frames: SETTLE_FRAMES, load_limit: LOAD_LIMIT,
  rounds: ROUNDS, rows: [] };
try {
  for (let r = 0; r < ROUNDS; r++)
    for (const worldName of RUN_WORLDS)
      for (const build of r % 2 ? ["after", "before"] : ["before", "after"]) {
        const label = `${build} ${worldName} round ${r}`;
        const g = await guarded(() => run(build, worldName).catch((e) => ({ build, world: worldName, error: String(e.message ?? e) })), label);
        result.rows.push({ round: r, ...g.result, load_before: g.load_before, load_after: g.load_after, load_discarded: g.load_discarded });
        const x = result.rows.at(-1);
        console.log(`${label}: ${x.error ?? Object.entries(x.cameras).map(([n, c]) => `${n} p50 ${c.p50} p95 ${c.p95} mean ${c.mean} draws ${c.draws}`).join(" | ")}` +
          ` errors ${x.console_errors?.length ?? "-"} (load ${g.load_before.toFixed(2)} -> ${g.load_after.toFixed(2)})`);
        await writeFile(OUT, `${JSON.stringify(result, null, 2)}\n`);
      }
} finally {
  for (const s of Object.values(servers)) s.close();
}
console.log(`wrote ${OUT}`);
