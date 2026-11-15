// Scratch harness (copy into e2e/ to run): a massif's grass build units, against a single Meadow's, on the web.
// Each repeat is a fresh browser on one dist and one world. The world is a row of Meadows east from the start
// tile, ending in the target: a massif centred on (8, 0) (cells (7, 0) to (9, 0) and four more), or a Meadow
// at (8, 0). The camera first sits over the origin, so the start Meadows build and pay the Meadow ground's
// one-time preparation. Then one `camera` command jumps it east of the target, so only the target comes within
// the 60 m build radius, and every frame's `state().grass` is read until no upload has happened for 90 frames
// (`cold`). The camera then goes back over the origin (the target's groups drop past 72 m) and jumps east
// again (`warm`: the definition's ground is already prepared). Each repeat runs under e2e/perf-load.mjs's guard.
// usage: node e2e/grass-units.mjs <dist> <out.json>   REPEATS (default 5)
import { createServer } from "node:http";
import { readFile, writeFile } from "node:fs/promises";
import { extname, resolve, sep } from "node:path";
import { cpus, hostname } from "node:os";
import { chromium } from "playwright";
import { BAD_TEXT } from "./console-errors.mjs";
import { guarded, LOAD_LIMIT } from "./perf-load.mjs";

const [DIST, OUT] = process.argv.slice(2);
const REPEATS = Number(process.env.REPEATS ?? 5);
const KEYS = { scenario: "play", tier: "high", tilt: "off", fps: "off", grass: "normal", seed: "1" };
const ARGS = ["--use-angle=metal", "--enable-gpu", "--ignore-gpu-blocklist", "--enable-unsafe-webgpu",
  "--disable-gpu-vsync", "--disable-frame-rate-limit"];
const X = (q, r) => 27.7128 * (q + r / 2);
const row = (to) => [{ cmd: "select_tile", id: "meadow" }, ...Array.from({ length: to }, (_, i) => ({ cmd: "place", q: i + 1, r: 0 }))];
const WORLDS = {
  massif: { script: [...row(6), { cmd: "select_tile", id: "massif_granite" }, { cmd: "place", q: 8, r: 0 }],
    target: { q: 8, r: 0, tile: "massif_granite", units: 7 }, eye: [X(8, 0) + 38, 20, 0] },
  meadow: { script: [...row(7), { cmd: "select_tile", id: "meadow" }, { cmd: "place", q: 8, r: 0 }],
    target: { q: 8, r: 0, tile: "meadow", units: 1 }, eye: [X(8, 0) + 56, 20, 0] },
};
const HOME = { cmd: "camera", pos: [0, 20, 40], look_at: [0, 0, 0] };
const types = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm", ".css": "text/css",
  ".ktx2": "image/ktx2", ".glb": "model/gltf-binary", ".png": "image/png", ".wgsl": "text/plain", ".wesl": "text/plain",
  ".jpg": "image/jpeg", ".woff2": "font/woff2", ".json": "application/json", ".webm": "audio/webm", ".m4a": "audio/mp4",
  ".webp": "image/webp", ".svg": "image/svg+xml" };
const server = await new Promise((ok) => {
  const base = resolve(DIST);
  const s = createServer(async (req, res) => {
    let path = decodeURIComponent(new URL(req.url, "http://x").pathname);
    if (path.endsWith("/")) path += "index.html";
    const file = resolve(base, "." + path);
    if (file !== base && !file.startsWith(base + sep)) { res.writeHead(403); res.end(); return; }
    try { const body = await readFile(file); res.writeHead(200, { "content-type": types[extname(path)] ?? "application/octet-stream" }); res.end(body); }
    catch { res.writeHead(404); res.end(); }
  }).listen(0, "127.0.0.1", () => ok(s));
});

// In the page: send `jump`, then read state().grass every rAF until 90 frames pass with no new upload
// after at least one (or 1,200 frames). Returns the building frames only, plus the totals.
const watch = async ([jump, target]) => {
  const g0 = window.hexWorld.state().grass;
  window.hexWorld.cmd(jump);
  const frames = [];
  let last = g0.uploads, quiet = 0, n = 0;
  while (n < 1200) {
    await new Promise((r) => requestAnimationFrame(r));
    n++;
    const s = window.hexWorld.state();
    const g = s.grass;
    if (g.uploads !== last) {
      const t = g.tiles.find((x) => x.q === target.q && x.r === target.r);
      frames.push({ frame: n, engine_frame: s.frames, units: g.uploads - last, last_build_ms: g.last_build_ms, last_frame_build_ms: g.last_frame_build_ms,
        groups: g.groups, target_blades: t?.blades ?? 0 });
      last = g.uploads;
      quiet = 0;
    } else if (frames.length && ++quiet >= 90) break;
  }
  const g = window.hexWorld.state().grass;
  const t = g.tiles.find((x) => x.q === target.q && x.r === target.r);
  const units = frames.reduce((a, f) => a + f.units, 0);
  return { frames, units, expected_units: target.units, units_ok: units === target.units, read_frames: n, target_row: t ?? null, groups_end: g.groups };
};

const rows = [];
for (let rep = 0; rep < REPEATS; rep++) {
  for (const name of rep % 2 ? ["meadow", "massif"] : ["massif", "meadow"]) {
    const w = WORLDS[name];
    const { result, load_before, load_after, load_discarded } = await guarded(async () => {
      const browser = await chromium.launch({ channel: "chromium", headless: true, args: ARGS });
      const errors = [];
      const out = { repeat: rep, world: name };
      try {
        const page = await browser.newPage({ viewport: { width: 2560, height: 1440 }, deviceScaleFactor: 1 });
        page.on("console", (m) => { if (m.type() === "error" || BAD_TEXT.test(m.text())) errors.push(m.text()); });
        page.on("pageerror", (e) => errors.push(String(e)));
        await page.goto(`http://127.0.0.1:${server.address().port}/?${new URLSearchParams(KEYS)}`);
        await page.waitForFunction(() => window.hexWorld?.ready(), null, { timeout: 180_000 });
        out.renderer = await page.evaluate(() => { const gl = document.createElement("canvas").getContext("webgl"); const e = gl?.getExtension("WEBGL_debug_renderer_info"); return e ? gl.getParameter(e.UNMASKED_RENDERER_WEBGL) : null; });
        if (!/apple/i.test(out.renderer ?? "")) throw new Error(`renderer ${out.renderer}`);
        await page.evaluate(([s, home]) => window.hexWorld.cmd([home, ...s, { cmd: "hover", q: -9, r: 9 }]), [w.script, HOME]);
        await page.waitForFunction(() => { const s = window.hexWorld.state(); return Object.values(s.packs ?? {}).every((v) => v === "ready") && !s.scatter?.waiting?.length && s.queued === 0; }, null, { timeout: 300_000, polling: 500 });
        out.tiles = await page.evaluate(() => window.hexWorld.state().play.tiles);
        const t = w.target;
        if (!out.tiles.some((x) => x.q === t.q && x.r === t.r && x.tile === t.tile)) throw new Error(`target ${t.tile} not placed`);
        await page.waitForTimeout(3000);
        out.home = await page.evaluate(() => { const g = window.hexWorld.state().grass; return { status: g.status, groups: g.groups, uploads: g.uploads, tiles: g.tiles.map((x) => [x.q, x.r]) }; });
        if (out.home.status !== "on") throw new Error(`grass status ${out.home.status}`);
        if (out.home.tiles.some(([q, r]) => q === t.q && r === t.r)) throw new Error("target built from the home camera");
        const jump = { cmd: "camera", pos: w.eye, look_at: [X(8, 0), 0, 0] };
        out.cold = await page.evaluate(watch, [jump, t]);
        await page.evaluate((home) => window.hexWorld.cmd(home), HOME);
        await page.waitForFunction((t) => !window.hexWorld.state().grass.tiles.some((x) => x.q === t.q && x.r === t.r), t, { timeout: 60_000, polling: 100 });
        await page.waitForTimeout(1000);
        out.warm = await page.evaluate(watch, [jump, t]);
      } catch (e) { out.error = String(e.message ?? e); }
      finally { await browser.close(); }
      out.console_errors = errors;
      return out;
    }, `${name}-${rep}`);
    const r = { ...result, load_before, load_after, load_discarded };
    rows.push(r);
    const s = (x) => x ? `${x.units} units in ${x.frames.length} frames, unit max ${Math.max(...x.frames.map((f) => f.last_build_ms))} ms, frame max ${Math.max(...x.frames.map((f) => f.last_frame_build_ms))} ms` : "-";
    console.log(`${name} rep ${rep}: cold ${s(r.cold)} | warm ${s(r.warm)} | load ${load_before.toFixed(2)}->${load_after.toFixed(2)} ${r.error ?? ""}`);
  }
}
server.close();
await writeFile(OUT, `${JSON.stringify({ date: new Date().toISOString(), host: hostname(), cpu: cpus()[0]?.model, dist: DIST, keys: KEYS,
  viewport: "2560x1440", load_limit: LOAD_LIMIT, worlds: WORLDS, home: HOME, rows }, null, 2)}\n`);
