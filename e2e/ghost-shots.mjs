// Ghost evidence (owner feedback 2026-10-14, U1): the selected tile's see-through meshes at the
// hovered slot with its own socket colours on each edge, fit and clash edges, and the outline-only
// fallback while the tile's pack is held. The pine, oak and meadow ghosts draw their seeded scatter,
// and match-forest-ghost / match-forest-placed show a pine ghost and the same cell once placed: the
// ghost's scatter layout fingerprint must equal the placed tile's. The beach, headland and bay
// ghosts (two rotations each), the river ghosts and an ocean ghost tint their ground per material
// slot and draw a flat water surface where the water goes. The windmill and farm ghosts (and every
// tile with hero props) must reach a second rotation with R, since their heroes turn with the tile
// even where every edge is grass (owner, 2026-10-20). GPU Chromium on the built site; writes
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

/**
 * Tiles whose look turns with them (the bridge schema's catalog, from data/tiles/*.ron): an
 * orientation period over 1, hex-core `orientation_period` (hero props 6, the wheat field 3).
 */
const HERO_TILES = new Set(JSON.parse(readFileSync(new URL("../web/ui/src/bridge-schema.json", import.meta.url), "utf8"))
  .catalog.tiles.filter((t) => t.period > 1).map((t) => t.id));

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
    return { ghost: st.play?.ghost ?? null, pinned: st.play?.pinned ?? null, ghost_meshes: st.ghost_meshes, ghost_report: st.ghost ?? null, packs: st.packs };
  });
  await page.screenshot({ path: `${out}/${name}.jpg`, type: "jpeg", quality: 88 });
  record.push({ name, note, ghost: s.ghost, ghost_meshes: s.ghost_meshes, ghost_report: s.ghost_report });
  console.log(`${name}: ${note} ${JSON.stringify({ ghost: s.ghost && { tile: s.ghost.tile, rotation: s.ghost.rotation, valid: s.ghost.valid, legal: s.ghost.legal_rotations, edges: s.ghost.edges }, meshes: s.ghost_meshes, report: s.ghost_report })}`);
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
  // A tile with hero props turns them with it, so R always reaches another rotation (the windmill's
  // sails, the farm's barn: owner, 2026-10-20).
  if (a.ghost?.rotation === b.ghost?.rotation) {
    if (HERO_TILES.has(tile)) fail(`${tile}: R keeps rotation ${a.ghost?.rotation} at (${q}, ${r}), but its look turns with it`);
    else console.log(`${tile}: one distinct orientation at (${q}, ${r}); R keeps rotation ${a.ghost?.rotation}`);
  }
  if (HERO_TILES.has(tile) && !(a.ghost_report?.entities > 0 && b.ghost_report?.entities > 0))
    fail(`${tile}: ghost meshes missing at a rotation: ${JSON.stringify([a.ghost_report, b.ghost_report])}`);
  if (a.ghost_meshes !== true) fail(`${tile}: ghost meshes not drawn`);
  return { ...a, b };
}

function expectWater(shot, tile) {
  if (shot.ghost_report?.water_surface !== true) fail(`${tile} ghost draws no water surface: ${JSON.stringify(shot.ghost_report)}`);
}

/**
 * `tile` at the first two free slots (frontier order) where it fits at different rotations, captured
 * as `${key}-a` and `${key}-b` (a headland fits a lone meadow at one rotation per slot).
 */
async function rotationPair(page, tile, key) {
  const frontier = await page.evaluate(() => window.hexWorld.state().play.frontier);
  const fits = [];
  for (const c of frontier) {
    await page.evaluate(([tile, q, r]) => { const h = window.hexWorld; h.selectTile(tile); h.hover(q, r); }, [tile, c.q, c.r]);
    await page.waitForTimeout(150);
    const rot = await rotationAt(page);
    if (rot !== null) fits.push({ ...c, rot });
  }
  const second = fits.find((f) => f.rot !== fits[0]?.rot);
  if (!second) { fail(`${tile}: no two free slots fit it at different rotations: ${JSON.stringify(fits)}`); return []; }
  const shots = [];
  for (const [f, s] of [[fits[0], "a"], [second, "b"]]) {
    await page.evaluate(([tile, q, r]) => { const h = window.hexWorld; h.selectTile(tile); h.hover(q, r); }, [tile, f.q, f.r]);
    await until(page, () => window.hexWorld.state().ghost_meshes === true, `${tile} ghost draws its meshes`);
    await frame(page, f.q, f.r);
    const shot = await capture(page, `${key}-${s}`, `${tile} at (${f.q}, ${f.r}), rotation ${f.rot}`);
    if (shot.ghost?.rotation !== f.rot || !shot.ghost?.valid) fail(`${tile}: ${key}-${s} is not at fitting rotation ${f.rot}: ${JSON.stringify(shot.ghost)}`);
    shots.push(shot);
  }
  return shots;
}

/** The first free slot (frontier order) where `tile` fits at some rotation, hovered, or null. */
async function firstFit(page, tile) {
  const frontier = await page.evaluate(() => window.hexWorld.state().play.frontier);
  for (const c of frontier) {
    await page.evaluate(([tile, q, r]) => { const h = window.hexWorld; h.selectTile(tile); h.hover(q, r); }, [tile, c.q, c.r]);
    await page.waitForTimeout(150);
    if (await page.evaluate(() => window.hexWorld.state().play?.ghost?.valid === true)) return c;
  }
  return null;
}

/** The ghost's scatter count per piece must show every rule's piece at least once where expected. */
function expectScatter(shot, tile, pieces) {
  const sc = shot.ghost_report?.scatter ?? {};
  for (const p of pieces) if (!(sc[p] > 0)) fail(`${tile} ghost draws no ${p}: ${JSON.stringify(shot.ghost_report)}`);
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
  expectScatter(await ghostPair(page, "meadow", 0, -1, "ghost-meadow"), "meadow", ["wildflower_clump"]);
  expectScatter(await ghostPair(page, "forest", -1, -1, "ghost-forest"), "forest", ["tree_forest"]);
  expectScatter(await ghostPair(page, "oak", -1, -1, "ghost-oak"), "oak", ["rock_small"]);
  await ghostPair(page, "house_cluster", 1, -1, "ghost-house");
  await ghostPair(page, "windmill", 0, -1, "ghost-windmill");
  await ghostPair(page, "farm", 0, -1, "ghost-farm");
  expectWater(await ghostPair(page, "river_straight", -2, 1, "ghost-river"), "river_straight");
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
  // Ghost, then placed: a pine forest at (-2, 0) (it touches only the forest at (-1, 0)), from the
  // other ghost shots' camera (a steep one stacks the translucent crowns into one flat tint). The
  // placed tile keeps the ghost's rotation and its scatter layout fingerprint equals the ghost's.
  await page.evaluate(() => { const h = window.hexWorld; h.unpin(); h.selectTile("forest"); h.hover(-2, 0); });
  await until(page, () => { const st = window.hexWorld.state(); return st.ghost_meshes === true && st.ghost?.q === -2 && st.ghost?.r === 0 && st.ghost?.tile === "forest"; }, "pine ghost at (-2, 0)");
  await page.evaluate(() => window.hexWorld.rotateGhost("cw"));
  await page.waitForTimeout(300);
  await frame(page, -2, 0);
  const g = await capture(page, "match-forest-ghost", "pine forest ghost at (-2, 0)");
  expectScatter(g, "forest", ["tree_forest"]);
  await page.evaluate(() => window.hexWorld.placeTile(-2, 0));
  await until(page, () => (window.hexWorld.state().scatter?.tiles ?? []).some((t) => t.q === -2 && t.r === 0 && t.drawn.tree_forest > 0), "pine forest placed at (-2, 0) with its scatter drawn");
  await page.waitForTimeout(1500);
  const placed = await capture(page, "match-forest-placed", "the same cell after placing it");
  const after = await page.evaluate(() => {
    const st = window.hexWorld.state();
    return { tile: st.play.tiles.find((t) => t.q === -2 && t.r === 0), row: st.scatter.tiles.find((t) => t.q === -2 && t.r === 0) };
  });
  const gr = g.ghost_report;
  record.push({ name: "match-forest", note: "ghost vs placed at (-2, 0)", ghost: { rotation: g.ghost?.rotation, layout: gr?.layout, scatter: gr?.scatter },
    placed: { rotation: after.tile?.rotation, layout: after.row?.layout, drawn: after.row?.drawn } });
  console.log(`match-forest: ghost ${JSON.stringify({ rotation: g.ghost?.rotation, layout: gr?.layout, scatter: gr?.scatter })} placed ${JSON.stringify({ rotation: after.tile?.rotation, layout: after.row?.layout, drawn: after.row?.drawn })}`);
  if (after.tile?.rotation !== g.ghost?.rotation) fail(`placed rotation ${after.tile?.rotation} != ghost rotation ${g.ghost?.rotation}`);
  if (!gr?.layout || after.row?.layout !== gr.layout) fail(`placed layout ${after.row?.layout} != ghost layout ${gr?.layout}`);
  const sorted = (o) => JSON.stringify(Object.entries(o ?? {}).sort(([a], [b]) => a.localeCompare(b)));
  if (sorted(after.row?.drawn) !== sorted(gr?.scatter)) fail(`placed scatter ${JSON.stringify(after.row?.drawn)} != ghost scatter ${JSON.stringify(gr?.scatter)}`);
  if (placed.ghost_meshes !== false) fail(`a ghost still draws after placing: ${JSON.stringify(placed.ghost_report)}`);
  if (errors.length) fail(`console errors: ${errors.join(" | ")}`);
  await page.close();
}

{
  // Coast and ocean ghosts on a fresh world, the start meadow alone, so every slot beside it fits a
  // coast tile (a headland's one grass edge needs grass there and nothing beside its beach edges).
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  const errors = await open(page);
  // Two rotations each: grass, dry and damp sand and seabed tinted apart, and the sea surface over
  // the seaward side.
  for (const [tile, key] of [["beach", "ghost-beach"], ["beach_outer", "ghost-headland"], ["beach_inner", "ghost-bay"]])
    for (const shot of await rotationPair(page, tile, key)) expectWater(shot, tile);
  // Ocean: a beach placed where it first fits, then the ocean ghost where it first fits beside it.
  const shore = await firstFit(page, "beach");
  if (!shore) fail("no free slot fits a beach");
  else {
    await page.evaluate(([q, r]) => window.hexWorld.placeTile(q, r), [shore.q, shore.r]);
    await until(page, () => window.hexWorld.state().tiles === 2 && (window.hexWorld.state().pack_waiting ?? []).length === 0, `beach placed at (${shore.q}, ${shore.r})`);
    const sea = await firstFit(page, "ocean");
    if (!sea) fail(`no free slot fits the ocean beside the beach at (${shore.q}, ${shore.r})`);
    else {
      await until(page, () => window.hexWorld.state().ghost_meshes === true, "ocean ghost draws its meshes");
      await frame(page, sea.q, sea.r);
      expectWater(await capture(page, "ghost-ocean", `ocean at (${sea.q}, ${sea.r}) beside the beach at (${shore.q}, ${shore.r})`), "ocean");
    }
  }
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
  // The rail pieces are village pieces too: a rail straight's ghost strokes its rail lanes meanwhile.
  await page.evaluate(() => { const h = window.hexWorld; h.selectTile("rail_straight"); h.hover(1, 0); });
  await page.waitForTimeout(600);
  const rail = await capture(page, "ghost-fallback-rail", "rail straight while the village pack is held: rail lane strokes stand in for its meshes");
  if (rail.ghost_meshes !== false) fail(`fallback rail ghost drew meshes: ${JSON.stringify(rail.ghost_meshes)}`);
  await page.evaluate(() => { const h = window.hexWorld; h.selectTile("house_cluster"); h.hover(1, 0); });
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
