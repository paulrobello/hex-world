// Ghost evidence (owner feedback 2026-10-14, U1): the selected tile's see-through meshes at the
// hovered slot with its own socket colours on each edge, fit and clash edges, and the outline-only
// fallback while the tile's pack is held. GPU Chromium on the built site; writes
// docs/checkpoints/ghost/*.jpg and ghost.json (what each capture shows, read from hexWorld.state()).
// Run: node e2e/ghost-shots.mjs dist
import { chromium } from "playwright";
import { createServer } from "node:http";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { extname, join } from "node:path";
import { BAD_TEXT } from "./console-errors.mjs";

const dist = process.argv[2] ?? "dist";
const out = process.env.GHOST_OUT ?? "docs/checkpoints/ghost";
mkdirSync(out, { recursive: true });
const types = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm", ".css": "text/css",
  ".woff2": "font/woff2", ".ktx2": "image/ktx2", ".glb": "model/gltf-binary", ".png": "image/png", ".webp": "image/webp" };
const server = createServer((req, res) => {
  let path = decodeURIComponent(new URL(req.url, "http://x").pathname);
  if (path.endsWith("/")) path += "index.html";
  try {
    const body = readFileSync(join(dist, path));
    res.writeHead(200, { "content-type": types[extname(path)] ?? "application/octet-stream" });
    res.end(body);
  } catch { res.writeHead(404); res.end(); }
}).listen(0);
const base = `http://127.0.0.1:${server.address().port}/`;
const gpuArgs = process.env.PW_GPU === "swiftshader"
  ? ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--enable-unsafe-webgpu"]
  : ["--use-angle=metal", "--enable-gpu", "--ignore-gpu-blocklist", "--enable-unsafe-webgpu"];

let failures = 0;
const fail = (m) => { console.error("FAIL:", m); failures++; };
const record = [];
const browser = await chromium.launch({ channel: "chromium", headless: true, args: gpuArgs });

/** Pointy-top axial to world metres (hex radius 16 m), as hex_core::Axial::to_world. */
const world = (q, r) => [16 * Math.sqrt(3) * (q + r / 2), 16 * 1.5 * r];

async function open(page) {
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => { if (m.type() === "error" || BAD_TEXT.test(m.text())) errors.push(m.text()); });
  await page.goto(base + "?scenario=play&tier=high");
  await page.waitForFunction(() => window.hexWorld?.ready() === true, null, { timeout: 120_000 });
  await page.evaluate(() => window.hexWorld.cmd({ cmd: "set_reduced_motion", on: true }));
  return errors;
}

async function until(page, fn, what, ms = 30_000) {
  const deadline = Date.now() + ms;
  for (;;) {
    const v = await page.evaluate(fn);
    if (v) return v;
    if (Date.now() > deadline) { fail(`timed out: ${what}`); return v; }
    await page.waitForTimeout(100);
  }
}

async function frame(page, q, r) {
  const [x, z] = world(q, r);
  await page.evaluate(([x, z]) => window.hexWorld.cmd({ cmd: "camera", pos: [x + 34, 62, z + 70], look_at: [x, 0, z] }), [x, z]);
}

async function capture(page, name, note) {
  await page.waitForTimeout(700);
  const s = await page.evaluate(() => {
    const st = window.hexWorld.state();
    return { ghost: st.play?.ghost ?? null, pinned: st.play?.pinned ?? null, ghost_meshes: st.ghost_meshes, packs: st.packs };
  });
  await page.screenshot({ path: `${out}/${name}.jpg`, type: "jpeg", quality: 88 });
  record.push({ name, note, ghost: s.ghost, ghost_meshes: s.ghost_meshes });
  console.log(`${name}: ${note} ${JSON.stringify({ ghost: s.ghost && { tile: s.ghost.tile, rotation: s.ghost.rotation, valid: s.ghost.valid, legal: s.ghost.legal_rotations, edges: s.ghost.edges }, meshes: s.ghost_meshes })}`);
  return s;
}

/** The ghost's rotation when it fits, else null (a slot where nothing fits turns a raw step). */
const rotationAt = (page) => page.evaluate(() => {
  const g = window.hexWorld.state().play?.ghost;
  return g?.valid ? g.rotation : null;
});

/**
 * Hovers `tile` at the first slot (from (q, r), then the frontier) where R reaches a second distinct
 * rotation, and captures it before and after R. A tile with one orientation everywhere keeps its one.
 */
async function ghostPair(page, tile, q0, r0, key) {
  const frontier = await page.evaluate(() => window.hexWorld.state().play.frontier);
  let q = q0;
  let r = r0;
  for (const c of [{ q: q0, r: r0 }, ...frontier]) {
    await page.evaluate(([tile, q, r]) => { const h = window.hexWorld; h.selectTile(tile); h.hover(q, r); }, [tile, c.q, c.r]);
    await page.waitForTimeout(150);
    const a = await rotationAt(page);
    await page.evaluate(() => window.hexWorld.rotateGhost("cw"));
    await page.waitForTimeout(150);
    const b = await rotationAt(page);
    await page.evaluate(() => window.hexWorld.rotateGhost("ccw"));
    await page.waitForTimeout(150);
    if (a !== null && b !== null && a !== b) { q = c.q; r = c.r; break; }
  }
  await page.evaluate(([q, r]) => window.hexWorld.hover(q, r), [q, r]);
  await until(page, () => window.hexWorld.state().ghost_meshes === true, `${tile} ghost draws its meshes`);
  await frame(page, q, r);
  const a = await capture(page, `${key}-a`, `${tile} at (${q}, ${r})`);
  await page.evaluate(() => window.hexWorld.rotateGhost("cw"));
  await page.waitForTimeout(300);
  const b = await capture(page, `${key}-b`, `${tile} at (${q}, ${r}) after R`);
  if (a.ghost?.rotation === b.ghost?.rotation)
    console.log(`${tile}: one distinct orientation at (${q}, ${r}); R keeps rotation ${a.ghost?.rotation}`);
  if (a.ghost_meshes !== true) fail(`${tile}: ghost meshes not drawn`);
}

{
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  const errors = await open(page);
  // A small world: a forest west of the start meadow, a road east, a river south-west.
  await page.evaluate(() => {
    const h = window.hexWorld;
    h.selectTile("forest"); h.placeTile(-1, 0);
    h.selectTile("road_straight"); h.placeTile(1, 0);
    h.selectTile("river_straight"); h.placeTile(-1, 1);
  });
  await until(page, () => window.hexWorld.state().tiles === 4 && (window.hexWorld.state().pack_waiting ?? []).length === 0, "world set up with every pack in");
  await page.waitForTimeout(2500);
  await ghostPair(page, "meadow", 0, -1, "ghost-meadow");
  await ghostPair(page, "forest", -1, -1, "ghost-forest");
  await ghostPair(page, "house_cluster", 1, -1, "ghost-house");
  await ghostPair(page, "river_straight", -2, 1, "ghost-river");
  // Fit: a road straight that continues the road at (1, 0). Clash: ocean against the meadow's grass.
  await page.evaluate(() => { const h = window.hexWorld; h.selectTile("road_straight"); h.hover(2, 0); });
  await frame(page, 2, 0);
  const fit = await capture(page, "edge-fit", "road straight continuing the road: compatible edges");
  if (!fit.ghost?.edges?.includes("Compatible")) fail("fit shot has no compatible edge");
  await page.evaluate(() => { const h = window.hexWorld; h.selectTile("ocean"); h.hover(0, 1); });
  await frame(page, 0, 1);
  const clash = await capture(page, "edge-clash", "ocean against grass: incompatible edges");
  if (!clash.ghost?.edges?.includes("Incompatible")) fail("clash shot has no incompatible edge");
  // Pinned: the ghost stays on (2, 0) while the pointer is elsewhere.
  await page.evaluate(() => { const h = window.hexWorld; h.selectTile("road_straight"); h.cmd({ cmd: "pin", q: 2, r: 0 }); h.hover(0, -1); });
  await frame(page, 1, 0);
  const pinned = await capture(page, "ghost-pinned", "road straight pinned at (2, 0) with the pointer over (0, -1)");
  if (pinned.pinned?.q !== 2 || pinned.ghost?.q !== 2) fail(`pinned ghost not at (2, 0): ${JSON.stringify(pinned)}`);
  if (errors.length) fail(`console errors: ${errors.join(" | ")}`);
  await page.close();
}

{
  // Fallback: the village pack is held, so the house ghost has no mesh yet: outline, edge colours, lanes.
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  let release;
  const held = new Promise((r) => { release = r; });
  const manifest = JSON.parse(readFileSync(join(dist, "assets/kit/manifest.json"), "utf8"));
  const village = new Set(manifest.pieces.filter((p) => p.pack === "village").map((p) => p.id));
  await page.route((url) => [...village].some((id) => url.pathname.includes(`/${id}`)), async (route) => { await held; await route.continue(); });
  const errors = await open(page);
  await page.evaluate(() => { const h = window.hexWorld; h.selectTile("house_cluster"); h.hover(1, 0); });
  await frame(page, 1, 0);
  await page.waitForTimeout(1200);
  const s = await capture(page, "ghost-fallback", "house cluster while the village pack is held: outline and edge colours only");
  if (s.ghost_meshes !== false) fail(`fallback ghost drew meshes: ${JSON.stringify(s.ghost_meshes)}`);
  release();
  await until(page, () => window.hexWorld.state().ghost_meshes === true, "house ghost swaps to its meshes when the pack lands");
  await capture(page, "ghost-fallback-landed", "the same ghost once the village pack arrived");
  if (errors.length) fail(`console errors: ${errors.join(" | ")}`);
  await page.close();
}

writeFileSync(`${out}/ghost.json`, JSON.stringify(record, null, 1) + "\n");
await browser.close();
server.close();
if (failures) { console.error(`${failures} failure(s)`); process.exit(1); }
console.log(`wrote ${out}/`);
