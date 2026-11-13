// Massif web smoke (P4 mega tiles) on the built site in GPU Chromium, driven through
// `window.hexWorld`: selects massif_granite, hovers a frontier slot, places it, turns the ghost
// (R, the `rotate` command), then undoes, redoes and bulldozes it. After every step it asserts from
// `state().play` that
//   1. the ghost at a fitting slot covers seven cells (`ghost.footprint`) with 18 outer edges,
//      standing on the centre `megaFit` picks from `mega_slots`,
//   2. `mega_slots` updates with the world (undo and bulldoze restore the opening list, redo the
//      placed one),
//   3. `tiles` holds the massif's anchor alone: no row for any of its six ring cells.
// Bulldozing a ring cell names the anchor (`bulldoze` target and `tileBulldozed`). On the opening
// world, with the massif placed, and on a pocket world (a frontier slot no footprint covers) it also
// asserts that the HUD Tiles sheet lists the massif once, under "Fits here" exactly at the slots
// where `megaFit` (web/ui/src/rules.ts) finds a centre and under "Doesn't fit here" elsewhere.
// Exits 0 only when every check holds and the page logged no error. Not a timing run, so it takes
// no load guard. Run after `make build-web`: node e2e/massif.mjs dist
import { chromium } from "playwright";
import { createServer } from "node:http";
import { readFileSync } from "node:fs";
import { extname, join } from "node:path";
import { BAD_TEXT } from "./console-errors.mjs";
import { megaFit } from "../web/ui/src/rules.ts";

const dist = process.argv[2] ?? "dist";
const MASSIF = "massif_granite";
const STEP_MS = 15_000;
const SHEET_MS = 5_000;
// Frontier slots the sheet check hovers per world.
const SHEET_SLOTS = 12;
const DIRS = [[1, 0], [0, 1], [-1, 1], [-1, 0], [0, -1], [1, -1]];
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
const key = (c) => `${c.q},${c.r}`;
const same = (a, b) => a != null && b != null && a.q === b.q && a.r === b.r;
// hex-core `footprint::members`: the anchor, then its six neighbours in edge order.
const members = (a) => [a, ...DIRS.map(([dq, dr]) => ({ q: a.q + dq, r: a.r + dr }))];
const slotsKey = (s) => JSON.stringify(s.mega_slots);

const browser = await chromium.launch({ channel: "chromium", headless: true, args: gpuArgs });
const page = await browser.newPage({ viewport: { width: 1600, height: 1000 } });
const errors = [];
page.on("pageerror", (e) => errors.push(String(e)));
page.on("console", (m) => { if (m.type() === "error" || BAD_TEXT.test(m.text())) errors.push(m.text()); });

const play = () => page.evaluate(() => window.hexWorld.state().play);
const cmd = (c) => page.evaluate((c) => window.hexWorld.cmd(c), c);
const events = () => page.evaluate(() => window.hexWorld.events());
// Sends `c`, then waits until the command queue drains and `ok(play)` holds; returns that state.
async function step(name, c, ok) {
  for (const one of [c].flat()) await cmd(one);
  let s;
  for (const d = Date.now() + STEP_MS; Date.now() < d; await page.waitForTimeout(100)) {
    const queued = await page.evaluate(() => window.hexWorld.state().queued);
    s = await play();
    if (queued === 0 && s && ok(s)) return s;
  }
  fail(`${name}: state did not settle within ${STEP_MS / 1000} s`);
  return s;
}

// The massif's rows in `tiles`: exactly one at `anchor` and none on a ring cell (`anchor` null:
// no massif at all).
function checkTiles(name, s, anchor) {
  const rows = s.tiles.filter((t) => t.tile === MASSIF);
  if (anchor == null) {
    if (rows.length !== 0) fail(`${name}: tiles still list the massif ${JSON.stringify(rows)}`);
    return;
  }
  if (rows.length !== 1 || !same(rows[0], anchor)) fail(`${name}: massif rows ${JSON.stringify(rows)}, want one at ${key(anchor)}`);
  const ring = members(anchor).slice(1);
  const onRing = s.tiles.filter((t) => ring.some((m) => same(m, t)));
  if (onRing.length) fail(`${name}: tiles list ring cells ${JSON.stringify(onRing)}`);
}

// A footprint ghost at `centre`: its seven cells in member order, 18 outer edges, and valid.
function checkGhostShape(name, g, centre) {
  if ((g.footprint ?? []).length !== 7) fail(`${name}: ghost footprint ${JSON.stringify(g.footprint)}, want 7 cells`);
  else if (members(centre).some((m, i) => !same(m, g.footprint[i]))) fail(`${name}: ghost footprint ${JSON.stringify(g.footprint)} is not the cells of ${key(centre)}`);
  if ((g.outer_edges ?? []).length !== 18) fail(`${name}: ghost outer_edges has ${(g.outer_edges ?? []).length} entries, want 18`);
  if (!g.valid) fail(`${name}: the ghost at ${key(centre)} is not valid`);
}

// Hovers the first frontier slot a centre covers and checks the ghost there; returns {slot, centre}.
async function checkGhost(name) {
  const s0 = await play();
  const slot = s0.frontier.find((c) => megaFit(c, s0.mega_slots, MASSIF));
  if (!slot) { fail(`${name}: no frontier slot fits the massif`); return null; }
  const centre = megaFit(slot, s0.mega_slots, MASSIF).centre;
  const s = await step(`${name} hover`, { cmd: "hover", q: slot.q, r: slot.r },
    (s) => s.ghost != null && s.ghost.tile === MASSIF && same(s.ghost, centre));
  const g = s?.ghost;
  if (!g) return { slot, centre };
  checkGhostShape(name, g, centre);
  console.log(`${name}: ghost at ${key(centre)} for slot ${key(slot)}, rotation ${g.rotation}, ${g.footprint?.length} cells`);
  return { slot, centre, ghost: g };
}

// The HUD sheet lists the massif exactly once, under "Fits here" exactly where megaFit finds a
// centre and under "Doesn't fit here" everywhere else. `first` slots are checked before the
// frontier's. Returns how many slots of each kind were checked.
async function checkSheet(name, first = []) {
  const s0 = await play();
  let fits = 0, off = 0;
  const slots = [...first, ...s0.frontier.filter((c) => !first.some((f) => same(f, c))).slice(0, SHEET_SLOTS)];
  for (const slot of slots) {
    const want = megaFit(slot, s0.mega_slots, MASSIF) != null;
    await cmd({ cmd: "hover", q: slot.q, r: slot.r });
    const fitRow = page.locator(`.list--fit .row[data-tile=${MASSIF}]`);
    const offRow = page.locator(`.list--off .row[data-tile=${MASSIF}]`);
    let seen = null;
    for (const d = Date.now() + SHEET_MS; Date.now() < d; await page.waitForTimeout(100)) {
      const s = await play();
      if (!same(s.hovered, slot)) continue;
      seen = { fit: await fitRow.count(), off: await offRow.count() };
      if (seen.fit + seen.off === 1 && (seen.fit === 1) === want) break;
    }
    const ok = seen != null && seen.fit + seen.off === 1 && (seen.fit === 1) === want;
    if (!ok) {
      const got = seen == null ? "never showed the hover" : `holds the massif row ${seen.fit} time(s) in Fits here and ${seen.off} in Doesn't fit here`;
      fail(`${name}: sheet ${got} at ${key(slot)}, megaFit says ${want ? "it fits" : "it does not"}`);
    }
    if (want) fits++; else off++;
  }
  console.log(`${name}: sheet checked against megaFit at ${fits} fitting and ${off} non-fitting slots`);
  return { fits, off };
}

let exit = 1;
try {
  await page.goto(base + "?scenario=play&tier=high");
  await page.waitForFunction(() => window.hexWorld?.ready() === true, null, { timeout: 120_000 });
  await cmd({ cmd: "set_reduced_motion", on: true });
  await cmd({ cmd: "tilt", on: false, strength: 0 });
  const open = await play();
  if (open.tiles.length !== 1) fail(`the play world opens with ${open.tiles.length} tiles, want the start tile alone`);
  // The HUD opens the Tiles sheet itself on its first sync (e2e/smoke.mjs asserts it), so wait for
  // that rather than toggling, which could cancel the HUD's own queued toggle.
  const sheetOpen = await step("sheet open", [], (s) => s.inventory_open === true);
  if (!sheetOpen?.inventory_open) fail("the Tiles sheet never opened");
  await events();

  // Select.
  const selected = await step("select", { cmd: "select_tile", id: MASSIF }, (s) => s.selected === MASSIF);
  const opening = slotsKey(selected);
  if (!selected.mega_slots.some((m) => m.tile === MASSIF)) fail("mega_slots lists no centre for the massif on the opening world");
  checkTiles("select", selected, null);
  await checkSheet("opening world");

  // Hover, then place at the hovered slot: the massif lands on megaFit's centre.
  const at = await checkGhost("hover");
  if (!at) throw new Error("no slot to place the massif on");
  const anchor = at.centre;
  const placed = await step("place", { cmd: "place", q: at.slot.q, r: at.slot.r },
    (s) => s.tiles.some((t) => t.tile === MASSIF));
  checkTiles("place", placed, anchor);
  const placedSlots = slotsKey(placed);
  if (placedSlots === opening) fail("place: mega_slots did not change");
  const covered = new Set(members(anchor).map(key));
  const overlap = placed.mega_slots.filter((m) => members(m).some((c) => covered.has(key(c))));
  if (overlap.length) fail(`place: mega_slots overlap the placed footprint ${JSON.stringify(overlap)}`);
  const ev = (await events()).filter((e) => e.type === "tilePlaced" && e.tile === MASSIF);
  if (ev.length !== 1 || !same(ev[0], anchor)) fail(`place: tilePlaced ${JSON.stringify(ev)}, want one at ${key(anchor)}`);
  await checkSheet("massif placed");

  // R: the ghost at a new slot turns and still covers seven cells.
  const turnAt = await checkGhost("before R");
  if (turnAt?.ghost) {
    const from = turnAt.ghost.rotation;
    const turned = await step("R", { cmd: "rotate", dir: "cw" }, (s) => s.ghost != null && s.ghost.rotation !== from);
    const g = turned?.ghost;
    if (g && !same(g, turnAt.centre)) fail(`R: the ghost moved from ${key(turnAt.centre)} to ${key(g)}`);
    if (g) checkGhostShape("R", g, turnAt.centre);
    if (g && !g.legal_rotations.includes(g.rotation)) fail(`R: rotation ${g.rotation} is not among ${g.legal_rotations}`);
    if (g) console.log(`R: ghost turned ${from} -> ${g.rotation}`);
    checkTiles("R", turned, anchor);
    if (turned && slotsKey(turned) !== placedSlots) fail("R: mega_slots changed without a world change");
  }

  // Undo: the massif goes and the opening centres come back.
  const undone = await step("undo", { cmd: "undo" }, (s) => !s.tiles.some((t) => t.tile === MASSIF));
  checkTiles("undo", undone, null);
  if (slotsKey(undone) !== opening) fail("undo: mega_slots differ from the opening world");
  await checkGhost("after undo");

  // Redo: the massif is back at its anchor with the placed world's centres.
  const redone = await step("redo", { cmd: "redo" }, (s) => s.tiles.some((t) => t.tile === MASSIF));
  checkTiles("redo", redone, anchor);
  if (slotsKey(redone) !== placedSlots) fail("redo: mega_slots differ from the placed world");
  await checkGhost("after redo");

  // Bulldoze a ring cell: the target and the event name the anchor, and the whole massif goes.
  const ring = members(anchor)[1];
  await events();
  const aimed = await step("bulldoze aim", [{ cmd: "set_tool", mode: "bulldoze" }, { cmd: "hover", q: ring.q, r: ring.r }],
    (s) => s.tool === "bulldoze" && s.bulldoze != null);
  if (!same(aimed?.bulldoze, anchor) || !aimed.bulldoze.allowed)
    fail(`bulldoze: target ${JSON.stringify(aimed?.bulldoze)} over ring cell ${key(ring)}, want the anchor ${key(anchor)}, allowed`);
  const dozed = await step("bulldoze", { cmd: "bulldoze", q: ring.q, r: ring.r }, (s) => !s.tiles.some((t) => t.tile === MASSIF));
  checkTiles("bulldoze", dozed, null);
  if (slotsKey(dozed) !== opening) fail("bulldoze: mega_slots differ from the opening world");
  const gone = (await events()).filter((e) => e.type === "tileBulldozed");
  if (gone.length !== 1 || !same(gone[0], anchor) || gone[0].tile !== MASSIF)
    fail(`bulldoze: tileBulldozed ${JSON.stringify(gone)}, want one for ${MASSIF} at ${key(anchor)}`);
  await step("place tool", { cmd: "set_tool", mode: "place" }, (s) => s.tool === "place");
  await checkGhost("after bulldoze");

  // A pocket (hex-core's mega_centre_for test): six meadows round (2,0) leave it a frontier slot
  // no footprint covers, so the sheet must list the massif under "Doesn't fit here" there.
  const pocket = { q: 2, r: 0 };
  const ring6 = [[1, 0], [1, 1], [2, 1], [3, 0], [3, -1], [2, -1]];
  await step("pocket", ring6.flatMap(([q, r]) => [{ cmd: "select_tile", id: "meadow" }, { cmd: "place", q, r }]),
    (s) => s.tiles.length === 1 + ring6.length);
  const pocketed = await step("pocket select", { cmd: "select_tile", id: MASSIF }, (s) => s.selected === MASSIF);
  if (!pocketed.frontier.some((c) => same(c, pocket))) fail(`pocket: ${key(pocket)} is not a frontier slot`);
  if (megaFit(pocket, pocketed.mega_slots, MASSIF) != null) fail(`pocket: megaFit finds a centre for ${key(pocket)}`);
  const { off } = await checkSheet("pocket world", [pocket]);
  if (off === 0) fail("pocket world: no non-fitting slot was checked");

  if (errors.length > 0) fail(`page errors: ${errors.slice(0, 5).join(" | ")}`);
  exit = failures === 0 ? 0 : 1;
} catch (e) {
  fail(String(e?.stack ?? e));
} finally {
  await browser.close();
  server.close();
}
console.log(exit === 0 ? "massif smoke OK" : "massif smoke FAILED");
process.exit(exit);
