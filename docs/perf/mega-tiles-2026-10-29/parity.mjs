// Scratch (not committed): flat-world pixel parity on the web for the mega-tiles perf check.
// Loads world2066 at clock=fixed water_clock=fixed, photo mode (DOF off, agents paused), and captures the
// close and overview cameras via hexWorld.screenshot on fixed absolute frames T_CLOSE and T_OVER.
// usage: node e2e/mega-parity-scratch.mjs <name>=<dist> ... <outdir>   env GRASS (default unset = game default)
import { createServer } from "node:http";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { extname, join } from "node:path";
import { loadavg } from "node:os";
import { chromium } from "playwright";
import { BAD_TEXT } from "./console-errors.mjs";
import { waitForLoad } from "./perf-load.mjs";

const argv = process.argv.slice(2);
const OUT = argv.at(-1);
const ARMS = argv.slice(0, -1).map((a) => [a.slice(0, a.indexOf("=")), a.slice(a.indexOf("=") + 1)]);
const SAVE = "docs/perf/post-arc001-2026-10-16/local-world.hexworld";
const W = 1600, H = 1000;
const T_CLOSE = Number(process.env.T_CLOSE ?? 4000);
const T_OVER = T_CLOSE + 700;
const KEYS = { scenario: "play", tier: "high", tilt: "off", fps: "off", seed: "1", clock: "fixed", water_clock: "fixed",
  ...(process.env.GRASS ? { grass: process.env.GRASS } : {}) };
const ARGS = ["--use-angle=metal", "--enable-gpu", "--ignore-gpu-blocklist", "--enable-unsafe-webgpu",
  "--disable-gpu-vsync", "--disable-frame-rate-limit"];
const CAMS = { close: [10, 12, 40, 0, 0, 0], overview: [20, 160, 150, 20, 0, -5] };
const cam = ([x, y, z, lx, ly, lz]) => ({ cmd: "camera", pos: [x, y, z], look_at: [lx, ly, lz] });
const types = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm", ".css": "text/css",
  ".ktx2": "image/ktx2", ".glb": "model/gltf-binary", ".png": "image/png", ".wgsl": "text/plain", ".wesl": "text/plain",
  ".jpg": "image/jpeg", ".woff2": "font/woff2", ".json": "application/json", ".webm": "audio/webm", ".m4a": "audio/mp4",
  ".webp": "image/webp", ".svg": "image/svg+xml" };
const serve = (dist) => new Promise((ok) => {
  const s = createServer(async (req, res) => {
    let path = decodeURIComponent(new URL(req.url, "http://x").pathname);
    if (path.endsWith("/")) path += "index.html";
    try { const body = await readFile(join(dist, path)); res.writeHead(200, { "content-type": types[extname(path)] ?? "application/octet-stream" }); res.end(body); }
    catch { res.writeHead(404); res.end(); }
  }).listen(0, () => ok(s));
});
const saveBytes = [...(await readFile(SAVE))];
await mkdir(OUT, { recursive: true });
const log = [];
for (const [name, dist] of ARMS) {
  const load_before = await waitForLoad(name);
  const server = await serve(dist);
  const browser = await chromium.launch({ channel: "chromium", headless: true, args: ARGS });
  const errors = [];
  const row = { arm: name, dist, load_before };
  try {
    const page = await browser.newPage({ viewport: { width: W, height: H }, deviceScaleFactor: 1 });
    page.on("console", (m) => { if (m.type() === "error" || BAD_TEXT.test(m.text())) errors.push(m.text()); });
    page.on("pageerror", (e) => errors.push(String(e)));
    await page.goto(`http://127.0.0.1:${server.address().port}/?${new URLSearchParams(KEYS)}`);
    await page.waitForFunction(() => window.hexWorld?.ready(), null, { timeout: 180_000 });
    row.renderer = await page.evaluate(() => { const gl = document.createElement("canvas").getContext("webgl"); const e = gl?.getExtension("WEBGL_debug_renderer_info"); return e ? gl.getParameter(e.UNMASKED_RENDERER_WEBGL) : null; });
    if (!/apple/i.test(row.renderer ?? "")) throw new Error(`renderer ${row.renderer}`);
    const r = await page.evaluate((b) => window.hexWorld.load(new Uint8Array(b)), saveBytes);
    if (!r.ok || r.tiles !== 2066) throw new Error(`load ${JSON.stringify(r)}`);
    await page.waitForFunction(() => window.hexWorld.state().play?.tiles?.length === 2066, null, { timeout: 300_000, polling: 500 });
    await page.evaluate(() => window.hexWorld.cmd([{ cmd: "hover", q: 9, r: -9 }, { cmd: "toggle", what: "photo" }, { cmd: "photo", dof: "off", pause_agents: true }]));
    await page.waitForFunction(() => { const s = window.hexWorld.state(); return Object.values(s.packs ?? {}).every((v) => v === "ready") && !s.scatter?.waiting?.length && s.queued === 0; }, null, { timeout: 300_000, polling: 500 });
    row.frames_ready = await page.evaluate(() => window.hexWorld.diag().frames);
    for (const [cname, c] of Object.entries(CAMS)) {
      const T = cname === "close" ? T_CLOSE : T_OVER;
      const res = await page.evaluate(async ([j, T, n]) => {
        const f = window.hexWorld.diag().frames;
        if (f >= T - 2) return { error: `frame ${f} already past target ${T}` };
        window.hexWorld.cmd([j, { cmd: "wait", frames: T - f }]);
        const url = await window.hexWorld.screenshot(n, 600_000);
        return { f, url };
      }, [cam(c), T, `${name}-${cname}`]);
      if (res.error) throw new Error(res.error);
      await writeFile(join(OUT, `${name}-${cname}.png`), Buffer.from(res.url.split(",")[1], "base64"));
      row[`${cname}_enqueued_at_frame`] = res.f;
      row[`${cname}_target_frame`] = T;
    }
  } catch (e) { row.error = String(e.message ?? e); }
  finally { await browser.close(); server.close(); }
  row.load_after = loadavg()[0];
  row.console_errors = errors;
  log.push(row);
  console.log(JSON.stringify(row));
}
await writeFile(join(OUT, "parity-run.json"), `${JSON.stringify({ keys: KEYS, viewport: `${W}x${H}`, t_close: T_CLOSE, t_over: T_OVER, cameras: CAMS, rows: log }, null, 2)}\n`);
