// Local-only HUD e2e (make ui-e2e): GPU Chromium on the built site, the play scenario driven through
// the Legend overlay with a real keyboard and mouse. Every step asserts through hexWorld.state() or
// the event queue, then saves e2e/test-results/ui/<mockup state>.jpg (the names in
// design/ui/legend/shots/), so each capture sits beside the approved mockup state it implements.
import { chromium } from "playwright";
import { createServer } from "node:http";
import { mkdirSync, readFileSync } from "node:fs";
import { extname, join } from "node:path";
import { BAD_TEXT } from "./console-errors.mjs";

const dist = process.argv[2] ?? "dist";
const out = "e2e/test-results/ui";
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
const check = (ok, m) => { if (!ok) fail(m); else console.log("ok:", m); };

const browser = await chromium.launch({ channel: "chromium", headless: true, args: gpuArgs });
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
const errors = [];
page.on("pageerror", (e) => errors.push(String(e)));
page.on("console", (m) => { if (m.type() === "error" || BAD_TEXT.test(m.text())) errors.push(m.text()); });

const shot = (name) => page.screenshot({ path: `${out}/${name}.jpg`, type: "jpeg", quality: 88 });
// Placement modifiers (owner feedback 2026-10-14); PLACEMENT_OUT=docs/checkpoints/placement refreshes the committed captures.
const placementOut = process.env.PLACEMENT_OUT ?? "e2e/test-results/placement";
mkdirSync(placementOut, { recursive: true });
const placementShot = (name, clip) =>
  page.screenshot({ path: `${placementOut}/${name}.jpg`, type: "jpeg", quality: 88, ...(clip ? { clip } : {}) });
const play = () => page.evaluate(() => window.hexWorld.state().play);
/** A screenshot cropped to the cells given as [q, r], with a margin of half a cell. */
async function shotCells(name, ...cells) {
  const rects = await Promise.all(cells.map(([q, r]) => page.evaluate(([q, r]) => window.hexWorld.cellRect(q, r), [q, r])));
  const [x0, y0] = [Math.min(...rects.map((b) => b[0])), Math.min(...rects.map((b) => b[1]))];
  const [x1, y1] = [Math.max(...rects.map((b) => b[2])), Math.max(...rects.map((b) => b[3]))];
  const m = 0.5 * Math.max(...rects.map((b) => b[2] - b[0]));
  const vp = page.viewportSize();
  const x = Math.max(0, x0 - m), y = Math.max(0, y0 - m);
  await page.screenshot({ path: `${out}/${name}.jpg`, type: "jpeg", quality: 92,
    clip: { x, y, width: Math.min(vp.width, x1 + m) - x, height: Math.min(vp.height, y1 + m) - y } });
}
const text = (sel) => page.locator(sel).first().innerText().catch(() => "");
async function until(fn, what, ms = 10_000) {
  const deadline = Date.now() + ms;
  for (;;) {
    const v = await fn();
    if (v) return v;
    if (Date.now() > deadline) { fail(`timed out: ${what}`); return v; }
    await page.waitForTimeout(50);
  }
}
const same = (c, q, r) => c && c.q === q && c.r === r;
/** Screen centre of a cell (CSS px), from the engine camera through hexWorld.cellRect. */
async function centre(q, r) {
  const b = await page.evaluate(([q, r]) => window.hexWorld.cellRect(q, r), [q, r]);
  return [(b[0] + b[2]) / 2, (b[1] + b[3]) / 2];
}
async function pointAt(q, r) {
  const [x, y] = await centre(q, r);
  await page.mouse.move(x, y, { steps: 4 });
  await until(async () => same((await play()).hovered, q, r), `pointer hovers (${q}, ${r})`);
}
/** The first click on the slot under the pointer pins it (skipped when it is pinned already); the next one places. */
async function pinHere(q, r) {
  if (same((await play()).pinned, q, r)) return;
  await page.mouse.down();
  await page.mouse.up();
  await until(async () => same((await play()).pinned, q, r), `the first click pins (${q}, ${r})`);
}
/** Events since the last call (the overlay watches without draining them from us). */
const events = () => page.evaluate(() => window.hexWorld.events());
const key = (k) => page.keyboard.press(k);
/** A click with a modifier held across frames, as a player holds it (one frame would collapse its press and release). */
async function modClick(mod) {
  await page.keyboard.down(mod);
  await page.waitForTimeout(80);
  await page.mouse.down();
  await page.mouse.up();
  await page.waitForTimeout(80);
  await page.keyboard.up(mod);
}
const rowNumber = async (id) => (await text(`.row[data-tile=${id}] .row__num`)).trim();

try {
  // S1 loading: the layer shows before the engine is up, then fades out in every scenario.
  await page.goto(base + "?scenario=play&tilt=off&fps=off", { waitUntil: "commit" });
  await page.waitForSelector(".load", { timeout: 15_000 });
  await shot("s1-1-first-load");
  await page.waitForFunction(() => window.hexWorld?.ready(), null, { timeout: 90_000 });
  await until(async () => !(await page.locator(".load").count()), "loading layer removed after ready", 5_000);

  // S2.1 first run: one meadow, the sheet open by default, the place callout.
  await until(async () => (await play())?.inventory_open === true, "engine inventory_open synced to the open sheet");
  check(await page.locator(".sheet--tiles:not(.is-closed)").count() === 1, "Tiles sheet open by default");
  check((await text(".bar__world")).includes("1 tile"), "bar title counts 1 tile");
  await until(async () => page.locator(".callout.is-shown").count(), "first-run place callout", 6_000);
  await page.waitForTimeout(300);
  await shot("s2-1-first-run-place-hint");
  check((await text(".key__title")) === "Point at a dashed slot", "no slot: key asks for a slot");
  await shot("s3-1-sheet-no-slot");
  await page.focus("canvas#hex-world");
  await events();

  // S7.5 the last tile: bulldoze refuses to empty the world.
  await key("b");
  await until(async () => (await play()).tool === "bulldoze", "B turns on bulldoze");
  await pointAt(0, 0);
  await until(async () => (await text(".ghost-tag")).includes("The world needs at least one tile"), "last-tile tag");
  await page.waitForTimeout(200);
  await shot("s7-5-bulldoze-last-tile");
  await key("b");
  await until(async () => (await play()).tool === "place", "B again returns to place");

  // S3.3 slot hovered: the filter and the slot key.
  await pointAt(1, 0);
  await until(async () => (await text(".key__title")) === "This slot needs", "slot key prints what the slot needs");
  check((await text(".key__list")).includes("Grass or forest on one edge"), "key lists the grass edge");
  check((await text(".group--off")).includes("Doesn't fit here"), "Doesn't fit here group present");
  check(await page.locator(".list--off .row[data-tile=ocean]").count() === 1, "ocean listed under Doesn't fit here");
  check((await text(".list--off .row[data-tile=ocean] .row__meta")).includes("grass"), "ocean row states its reason");
  await until(async () => (await text(".ghost-tag")).includes("Meadow"), "ghost tag names the selected tile");
  check((await text(".ghost-tag")).includes("Fits"), "meadow: a single orientation reads Fits (Q6)");
  await shot("s3-3-slot-grass");
  await shot("s2-2-play-sheet-open");

  // S3.8 number keys pick from Fits here; S4.2 R turns once per press.
  const n = await rowNumber("road_straight");
  await key(n);
  await until(async () => (await play()).selected === "road_straight", `key ${n} selects Road straight`);
  await until(async () => (await text(".ghost-tag")).includes("Rotation 1 of 2"), "tag reads Rotation 1 of 2 (distinct orientations)");
  await until(async () => page.locator(".callout.is-shown").count(), "rotate callout", 6_000);
  await page.waitForTimeout(300);
  await shot("s2-1-first-run-rotate-hint");
  await events();
  const before = (await play()).rotation;
  await key("r");
  await until(async () => (await play()).rotation !== before, "R rotates the ghost");
  await page.waitForTimeout(200);
  const rotated = (await events()).filter((e) => e.type === "ghostRotated");
  check(rotated.length === 1, `one R press is one ghostRotated (got ${rotated.length})`);
  await until(async () => (await text(".ghost-tag")).includes("Rotation 2 of 2"), "tag reads Rotation 2 of 2");
  await page.waitForTimeout(250);
  await shot("s4-2-rotate");
  // Moving off the slot (through (1, -1), where this rotation is illegal) and back keeps the
  // chosen rotation instead of resetting to the first legal one.
  const chosen = (await play()).rotation;
  await pointAt(1, -1);
  await until(async () => (await play()).rotation !== chosen, "(1, -1) snaps to a rotation legal there");
  await pointAt(1, 0);
  await until(async () => (await play()).rotation === chosen, "back on (1, 0): the chosen rotation, not a reset");
  await until(async () => (await text(".ghost-tag")).includes("Rotation 2 of 2"), "tag still reads Rotation 2 of 2");

  // Q5: the pointer over the HUD keeps the last slot's filter, and the engine keeps hovering it.
  const [rx, ry] = await page.locator(".row[data-tile=river_bend]").boundingBox().then((b) => [b.x + b.width / 2, b.y + b.height / 2]);
  await page.mouse.move(rx, ry, { steps: 4 });
  await page.waitForTimeout(400);
  check(same((await play()).hovered, 1, 0), "engine still hovers (1, 0) while the pointer is on the sheet");
  check((await text(".key__title")) === "This slot needs", "sheet keeps the slot's filter over the HUD");
  await shot("s3-5-tile-hover");

  // Pinning: one click on a slot pins it. The ghost, its rotation and the sheet stay on the pin while
  // the pointer roams, R turns it from the sheet, and Esc only unpins.
  await pointAt(1, 0);
  await page.waitForTimeout(200);
  const pinKey = await page.locator(".key").innerHTML();
  await events();
  await page.mouse.down();
  await page.mouse.up();
  await until(async () => same((await play()).pinned, 1, 0), "one click pins (1, 0)");
  check((await play()).tiles.length === 1, "the pinning click places nothing");
  check((await events()).some((e) => e.type === "pinChanged" && same(e.cell, 1, 0)), "pinChanged names (1, 0)");
  await until(async () => (await text(".ghost-tag")).includes("Pinned"), "the tag says the slot is pinned");
  await pointAt(0, 0);
  await pointAt(1, -1);
  let pin = await play();
  check(same(pin.pinned, 1, 0) && same(pin.ghost, 1, 0), "over (1, -1) the pin and the ghost stay on (1, 0)");
  check(pin.ghost?.rotation === chosen, "and the ghost keeps the rotation chosen there (no snap to (1, -1))");
  check((await page.locator(".key").innerHTML()) === pinKey, "the slot key stays on (1, 0) while (1, -1) is hovered");
  await page.mouse.move(rx, ry, { steps: 12 });
  await page.waitForTimeout(400);
  pin = await play();
  check(!same(pin.hovered, 1, 0), `over the sheet hovered follows the pointer (${JSON.stringify(pin.hovered)}), not the pin`);
  check(same(pin.pinned, 1, 0) && same(pin.ghost, 1, 0), "over the sheet the pin and the ghost stay on (1, 0)");
  check((await page.locator(".key").innerHTML()) === pinKey, "the sheet's slot key stays on (1, 0)");
  await shot("pin-1-pinned-sheet");

  // R from the sheet turns the pinned ghost, and the selected row's socket hex turns with it.
  const rowTurn = () =>
    page.locator(".list--fit .row[aria-pressed=true] .row__hex .g-rot").evaluate((g) => g.style.transform);
  const turnA = await rowTurn();
  check(turnA === `rotate(${chosen * 60}deg)`, `the selected row's hex shows the ghost's rotation (${turnA})`);
  await shot("rows-rot-a");
  await key("r");
  await until(async () => (await play()).ghost?.rotation !== chosen, "R from the sheet turns the pinned ghost");
  pin = await play();
  check(same(pin.ghost, 1, 0) && same(pin.pinned, 1, 0), "the turned ghost stays on the pin (1, 0)");
  await until(async () => (await rowTurn()) === `rotate(${pin.ghost.rotation * 60}deg)`, "the selected row's hex turns with the ghost");
  check((await rowTurn()) !== turnA, "the row hex's transform changed");
  await page.waitForTimeout(400);
  await shot("rows-rot-b");

  // Esc with a pin only unpins: the tool, the sheet and its slot stay.
  await key("Escape");
  await until(async () => !(await play()).pinned, "Esc unpins");
  await page.waitForTimeout(200);
  pin = await play();
  check(pin.tool === "place" && pin.inventory_open === true && pin.tiles.length === 1, "that Esc does nothing else: still placing, the sheet open");
  check((await text(".key__title")) === "This slot needs", "the sheet keeps its slot (that Esc did not clear it)");

  // S3.7 a tile that doesn't fit, selected by mouse; S5.2 the refused click.
  await page.locator(".list--off .row[data-tile=ocean]").click();
  await until(async () => (await play()).selected === "ocean", "click selects Ocean");
  await pointAt(1, 0);
  await until(async () => (await text(".ghost-tag")).includes("to meet the grass"), "tag states why Ocean doesn't fit");
  check(await page.locator(".ghost-tag.ghost-tag--bad").count() === 1, "refusal tag carries the bad ring");
  await shot("s3-7-selected-does-not-fit");
  await events();
  await page.mouse.down();
  await page.mouse.up();
  await until(async () => same((await play()).pinned, 1, 0), "the first click pins (1, 0) again");
  check((await play()).tiles.length === 1, "pinning a slot the tile doesn't fit places nothing");
  await page.mouse.down();
  await page.mouse.up();
  await until(async () => (await play()).last_result?.ok === false, "the click is refused");
  check((await play()).tiles.length === 1, "refused placement leaves 1 tile");
  check((await events()).some((e) => e.type === "placementRefused"), "placementRefused event");
  check(same((await play()).pinned, 1, 0), "a refusal keeps the pin");
  await shot("s5-2-place-rejected");

  // S5.1 place: Road straight at (1, 0).
  await page.locator(".row[data-tile=road_straight]").click();
  await until(async () => (await play()).selected === "road_straight", "click selects Road straight");
  await pointAt(1, 0);
  await pinHere(1, 0);
  await page.mouse.down();
  await page.mouse.up();
  await until(async () => (await play()).tiles.length === 2, "road placed");
  check(!(await play()).pinned, "placing clears the pin");
  await until(async () => (await text(".ghost-tag")).includes("Placed Road straight"), "tag acknowledges the placement");
  check((await text(".bar__world")).includes("2 tiles"), "bar title counts 2 tiles");
  await shot("s5-1-placed-rise");

  // S6 undo and redo from the keyboard, with the toast naming what changed.
  await page.waitForTimeout(1200);
  await key("ControlOrMeta+z");
  await until(async () => (await play()).tiles.length === 1, "undo removes the road");
  await until(async () => (await text(".toast.is-shown")).includes("Undid: placed Road straight at (1, 0)"), "undo toast names the action");
  await page.waitForTimeout(300);
  await shot("s6-1-undo-toast");
  await key("ControlOrMeta+Shift+z");
  await until(async () => (await play()).tiles.length === 2, "redo restores the road");
  await until(async () => (await text(".toast.is-shown")).includes("Redid: placed Road straight at (1, 0)"), "redo toast names the action");
  await page.waitForTimeout(300);
  await shot("s6-2-redo-toast");

  // A meadow east of the road, so removing the road would split the world.
  await key(await rowNumber("meadow"));
  await until(async () => (await play()).selected === "meadow", "number key selects Meadow");
  await pointAt(2, 0);
  await pinHere(2, 0);
  await page.mouse.down();
  await page.mouse.up();
  await until(async () => (await play()).tiles.length === 3, "meadow placed at (2, 0)");

  // Owner 2026-10-14: in place mode the placed tile under the pointer gets a faint rim (in-world.md,
  // Hovered tile), and the state names it; the pointer on the HUD hovers no placed tile.
  await until(async () => (await play()).rising.length === 0, "the meadow at (2, 0) settles", 20_000);
  await pointAt(0, 0);
  const over = await play();
  check(over.tool === "place" && over.hovered_kind === "tile", `place mode: the pointer over (0, 0) reports a placed tile (${over.hovered_kind})`);
  check(!over.ghost && !over.bulldoze, "no ghost and no bulldoze target over a placed tile");
  await page.waitForTimeout(300);
  await shotCells("hover-tile-placed", [0, 0], [0, 1]);
  await pointAt(0, 1);
  check((await play()).hovered_kind === "slot", "the empty slot at (0, 1) reports a slot");
  await page.waitForTimeout(400);
  await shotCells("hover-tile-slot", [0, 0], [0, 1]);
  await pointAt(0, 0);
  const title = await page.locator(".bar__world").boundingBox();
  await page.mouse.move(title.x + title.width / 2, title.y + title.height / 2, { steps: 12 });
  await until(async () => (await play()).hovered_kind !== "tile", "the pointer on the HUD bar hovers no placed tile");

  // S7 bulldoze: B, a refused removal that would split the world, then a removable leaf.
  await key("b");
  await until(async () => (await play()).tool === "bulldoze", "B turns on bulldoze");
  // The HUD repaints from the next state publish, which lags the engine under host load.
  await until(async () => (await page.locator(".tool[data-action=bulldoze].is-active").count()) === 1, "bulldoze tool active");
  await until(async () => (await text(".key__title")) === "Bulldoze", "sheet key turns into the bulldoze legend");
  await pointAt(1, 0);
  await until(async () => (await play()).bulldoze?.allowed === false, "removing the road is refused by the engine");
  await until(async () => (await text(".ghost-tag")).includes("Removing this would split your world"), "split tag");
  check((await text(".ghost-tag")).includes("1 meadow tile would be cut off"), "split tag counts the cut-off tile");
  await shot("s7-4-bulldoze-split-refused");
  await events();
  await page.mouse.down();
  await page.mouse.up();
  await until(async () => (await play()).last_result?.action === "bulldoze" && (await play()).last_result?.ok === false, "the refused bulldoze");
  check((await events()).some((e) => e.type === "bulldozeRefused"), "bulldozeRefused event");
  check((await play()).tiles.length === 3, "refused bulldoze keeps 3 tiles");
  await pointAt(2, 0);
  await until(async () => (await text(".ghost-tag")).includes("Remove Meadow"), "removable tag");
  await shot("s7-2-bulldoze-hover");
  await page.mouse.down();
  await page.mouse.up();
  await until(async () => (await play()).tiles.length === 2, "meadow bulldozed");
  await until(async () => (await text(".toast.is-shown")).includes("Removed Meadow"), "bulldoze toast offers Undo");
  await page.waitForTimeout(300);
  await shot("s7-3-bulldoze-done");
  await key("Escape");
  await until(async () => (await play()).tool === "place", "Esc leaves bulldoze");

  // S3.9 Tab closes and reopens the sheet; the engine flag follows.
  await key("Tab");
  await until(async () => (await play()).inventory_open === false, "Tab closes the sheet (engine flag)");
  await until(async () => page.locator(".sheet--tiles.is-closed").count(), "sheet slides closed");
  await page.waitForTimeout(300);
  await shot("s3-9-sheet-closed");
  await key("Tab");
  await until(async () => (await play()).inventory_open === true, "Tab reopens the sheet");

  // S2.3 H hides the HUD.
  await key("h");
  await until(async () => page.locator(".hud.is-hidden").count(), "H hides the HUD");
  await page.waitForTimeout(300);
  await shot("s2-3-hud-hidden");
  await key("h");

  // S3.4 nothing fits: a road straight west of the start meadow and a beach turned so a beach
  // edge faces (-1, 1) leave that slot needing a road and a beach at once, which no tile carries
  // (a river and a beach fit the river mouth).
  // Set up through the bridge (auto-rotate picks those rotations), then driven by mouse.
  await page.evaluate(() => {
    const h = window.hexWorld;
    h.selectTile("road_straight"); h.placeTile(-1, 0);
    h.selectTile("beach"); h.placeTile(-2, 1);
    h.selectTile("meadow");
  });
  await until(async () => (await play()).tiles.length === 4, "nothing-fits world set up");
  await pointAt(-1, 1);
  await until(async () => (await text(".key__title")) === "Nothing fits this slot", "key: Nothing fits this slot");
  check(await page.locator(".list--fit .row").count() === 0, "every tile drops to Doesn't fit here");
  await page.waitForTimeout(200);
  await shot("s3-4-nothing-fits");
  await page.locator(".key button", { hasText: "Show the nearest slot" }).click();
  await until(async () => { const h = (await play()).hovered; return h && !same(h, -1, 1); }, "Show the nearest slot hovers another slot");
  await until(async () => (await text(".key__title")) === "This slot needs", "the key moves to the nearest fitting slot");
  await until(async () => {
    const p = await play();
    return p.pinned && !same(p.pinned, -1, 1) && same(p.ghost, p.pinned.q, p.pinned.r);
  }, "Show the nearest slot pins it, with the ghost there");
  await key("Escape");
  await until(async () => !(await play()).pinned, "Esc unpins the nearest slot");
  for (let i = 0; i < 2; i++) await key("ControlOrMeta+z");
  await until(async () => (await play()).tiles.length === 2, "the setup undone");
  await page.waitForTimeout(4200);

  // S6.3 repeated undo is one toast with a count; S6.4 the empty stack disables Undo.
  await page.mouse.move(700, 880);
  for (let i = 0; i < 3; i++) {
    await key("ControlOrMeta+z");
    await page.waitForTimeout(120);
  }
  await until(async () => (await play()).can_undo === false, "undo stack emptied");
  await until(async () => /Undid 3 actions/.test(await text(".toast.is-shown")), "repeated undo counts in one toast");
  await shot("s6-3-undo-repeated");
  check(await page.locator(".tool[data-action=undo][aria-disabled=true]").count() === 1, "Undo disabled when empty");
  await page.locator(".tool[data-action=undo]").hover();
  await until(async () => (await text(".tip.is-shown")).includes("Nothing to undo"), "Undo tooltip says Nothing to undo");
  await shot("s6-4-undo-empty");

  // Placement modifiers: Shift-click places a run of the selected tile with no pin step, Ctrl or
  // Cmd-click swaps a placed tile (one undo step), a refused swap changes nothing, and the canvas
  // never opens the browser's context menu.
  const tileAt = (p, q, r) => p.tiles.find((t) => same(t, q, r));
  await page.evaluate(() => {
    window.__menus = [];
    window.__downs = [];
    addEventListener("contextmenu", (e) => window.__menus.push({ button: e.button, ctrl: e.ctrlKey, prevented: e.defaultPrevented }));
    addEventListener("pointerdown", (e) => window.__downs.push({ button: e.button, ctrl: e.ctrlKey, meta: e.metaKey, target: e.target?.tagName + "." + e.target?.className }), true);
  });
  const modsText = (await text(".sheet__mods")).replace(/\s+/g, " ");
  check(/Shift-click place .*-click replace/.test(modsText), `the sheet footer names the modifier clicks (${modsText})`);
  const mods = await page.locator(".sheet__mods").boundingBox();
  check(mods && mods.height < 30, `the modifier hint is one line (${Math.round(mods?.height ?? 0)} px)`);
  await page.locator(".sheet--tiles").screenshot({ path: `${placementOut}/footer-hint.jpg`, type: "jpeg", quality: 88 });
  await page.evaluate(() => window.hexWorld.selectTile("meadow"));
  await until(async () => (await play()).selected === "meadow", "Meadow selected for the run");
  await page.focus("canvas#hex-world");
  await events();
  const run = [[-1, 0], [-2, 0], [-3, 0]];
  await page.keyboard.down("Shift");
  for (const [q, r] of run) {
    await pointAt(q, r);
    await until(async () => same((await play()).ghost, q, r), `the ghost follows the pointer to (${q}, ${r})`);
    const n = (await play()).tiles.length;
    await page.mouse.down();
    await page.mouse.up();
    await until(async () => (await play()).tiles.length === n + 1, `Shift-click places at (${q}, ${r})`);
    check(!(await play()).pinned, `no pin after the Shift-click at (${q}, ${r})`);
  }
  await page.keyboard.up("Shift");
  const runEvents = await events();
  check(!runEvents.some((e) => e.type === "pinChanged"), "the Shift run never pins");
  check(runEvents.filter((e) => e.type === "tilePlaced" && e.tile === "meadow").length === 3, "three meadows placed in a row");
  await page.waitForTimeout(2300);
  await placementShot("shift-run-three-meadows");
  // A plain click still pins after the run (no stuck Shift).
  await pointAt(-1, 1);
  await page.mouse.down();
  await page.mouse.up();
  await until(async () => same((await play()).pinned, -1, 1), "a plain click after the Shift run pins (-1, 1)");
  await key("Escape");
  await until(async () => !(await play()).pinned, "Esc unpins");

  // Ctrl-click (Cmd on macOS also works) swaps the meadow at (-2, 0) for a pine forest.
  await page.evaluate(() => window.hexWorld.selectTile("forest"));
  await until(async () => (await play()).selected === "forest", "Pine forest selected");
  await pointAt(-2, 0);
  await events();
  let p = await play();
  const countBefore = p.tiles.length;
  await modClick("Control");
  await until(async () => tileAt(await play(), -2, 0)?.tile === "forest", "Ctrl-click replaces the meadow with a pine forest");
  p = await play();
  check(p.tiles.length === countBefore, "a replace keeps the tile count");
  check(!p.pinned, "a replace pins nothing");
  const swap = await events();
  check(swap.some((e) => e.type === "tileReplaced" && same(e, -2, 0) && e.tile === "forest" && e.replaced === "meadow"), "tileReplaced names the swap");
  await until(async () => (await text(".ghost-tag")).includes("Replaced Meadow with Pine forest"), "the tag names the swap");
  await page.waitForTimeout(700);
  await placementShot("ctrl-replace-mid-swap");
  await page.waitForTimeout(2200);
  await placementShot("ctrl-replace-forest");
  const downs = await page.evaluate(() => window.__downs.slice(-1)[0]);
  console.log(`ctrl-click reached the page as button ${downs?.button} (ctrl ${downs?.ctrl})`);

  // Undo restores the meadow in one step; redo swaps the forest back; undo again for the next cases.
  await key("ControlOrMeta+z");
  await until(async () => tileAt(await play(), -2, 0)?.tile === "meadow", "one undo restores the meadow");
  await until(async () => (await text(".toast.is-shown")).includes("Undid: replaced Meadow with Pine forest"), "undo toast names the replace");
  check((await play()).tiles.length === countBefore, "undo keeps the tile count");
  await page.waitForTimeout(2600);
  await placementShot("ctrl-replace-undone");
  await key("ControlOrMeta+Shift+z");
  await until(async () => tileAt(await play(), -2, 0)?.tile === "forest", "redo swaps the forest back");
  await key("ControlOrMeta+z");
  await until(async () => tileAt(await play(), -2, 0)?.tile === "meadow", "undo again");

  // Cmd-click (Meta) replaces too.
  await modClick("Meta");
  await until(async () => tileAt(await play(), -2, 0)?.tile === "forest", "Cmd-click replaces as well");
  await key("ControlOrMeta+z");
  await until(async () => tileAt(await play(), -2, 0)?.tile === "meadow", "and undo restores it");

  // A refused replace: ocean cannot meet the meadows around (-2, 0). The world stays as it was.
  await page.evaluate(() => window.hexWorld.selectTile("ocean"));
  await until(async () => (await play()).selected === "ocean", "Ocean selected");
  await pointAt(-2, 0);
  await events();
  const tilesBefore = JSON.stringify((await play()).tiles);
  const undoBefore = (await play()).can_undo;
  await modClick("Control");
  await until(async () => (await play()).last_result?.action === "replace" && (await play()).last_result?.ok === false, "the replace is refused");
  check(JSON.stringify((await play()).tiles) === tilesBefore, "a refused replace leaves the world unchanged");
  check((await play()).can_undo === undoBefore, "and records nothing to undo");
  check((await events()).some((e) => e.type === "placementRefused" && same(e, -2, 0) && e.tile === "ocean"), "placementRefused at (-2, 0)");
  await until(async () => (await text(".ghost-tag")).includes("doesn't fit") || (await page.locator(".ghost-tag--bad").count()) > 0, "the tag shows the refusal");
  await placementShot("ctrl-replace-refused");

  // The context menu never opens over the canvas: Ctrl-clicks and a right-click are all prevented.
  const [cx, cy] = await centre(-2, 0);
  await page.mouse.click(cx, cy, { button: "right" });
  await page.waitForTimeout(150);
  const menus = await page.evaluate(() => window.__menus);
  check(menus.length > 0, `context menu events seen over the canvas (${menus.length})`);
  check(menus.every((m) => m.prevented), `every context menu over the canvas is prevented (${JSON.stringify(menus)})`);
  const synthetic = await page.evaluate(() =>
    document.querySelector("canvas#hex-world").dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, ctrlKey: true })),
  );
  check(synthetic === false, "a Ctrl contextmenu dispatched at the canvas is cancelled");
  // A plain click still pins after the Ctrl and Cmd clicks (no stuck modifier).
  await page.evaluate(() => window.hexWorld.selectTile("meadow"));
  await pointAt(-1, 1);
  await page.mouse.down();
  await page.mouse.up();
  await until(async () => same((await play()).pinned, -1, 1), "a plain click after the Ctrl run pins (-1, 1)");
  await key("Escape");
  await until(async () => !(await play()).pinned, "Esc unpins");

  // S8 photo mode (PRD §4.6): P enters (the HUD hides, agents pause), WASD and E fly the free camera,
  // the panel changes DOF, a 2× PNG exports through the offscreen target and downloads, then P leaves
  // (HUD back, agents running, the orbit camera exactly where it was).
  const st = () => page.evaluate(() => window.hexWorld.state());
  await page.mouse.move(700, 880);
  await page.waitForTimeout(4200);
  const orbit = (await st()).camera;
  await page.focus("canvas#hex-world");
  await key("p");
  await until(async () => { const s = await st(); return s.play.photo_mode && s.photo.on; }, "P enters photo mode");
  await until(async () => page.locator(".hud.is-hidden.is-photo").count(), "the play HUD hides in photo mode");
  check(await page.locator(".photo-ui:not(.is-off) .sheet--photo").count() === 1, "photo panel shown");
  check(await page.locator(".hud.is-photo > .show-ui").isHidden(), "the play HUD's Show UI stays away");
  await until(async () => (await st()).photo.agents_paused === true, "entering pauses agents");
  const t0 = (await st()).photo.sim_ticks;
  await page.waitForTimeout(800);
  check((await st()).photo.sim_ticks === t0, `the car sim holds at tick ${t0} while paused`);
  await page.waitForTimeout(250);
  await shot("s8-2-photo-panel");

  await page.focus("canvas#hex-world");
  await page.keyboard.down("w");
  await page.keyboard.down("e");
  await page.waitForTimeout(700);
  await page.keyboard.up("e");
  await page.keyboard.up("w");
  const flown = (await st()).camera;
  const moved = Math.hypot(...flown.pos.map((v, i) => v - orbit.pos[i]));
  check(moved > 1, `WASD/E flew the free camera ${moved.toFixed(1)} m`);
  check(flown.pos[1] > orbit.pos[1], "E rises");

  await page.locator(".sheet--photo .seg[aria-label='Depth of field'] button[data-value=manual]").click();
  await until(async () => (await st()).photo.dof === "manual", "DOF switches to manual");
  await page.locator(".sheet--photo input[aria-label='Focus distance']").evaluate((el) => {
    el.value = "30";
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await until(async () => (await st()).photo.focus === 30, "focus distance reaches the engine");
  await page.locator(".sheet--photo input[aria-label='Aperture']").evaluate((el) => {
    el.value = "0";
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await until(async () => (await st()).photo.aperture === 1.4, "aperture f/1.4 reaches the engine");
  await until(async () => page.locator(".focus-band.is-shown").count(), "the focus band shows while focusing");
  await shot("s8-2-photo-focus-drag");

  await page.focus("canvas#hex-world");
  await key("h");
  await until(async () => page.locator(".photo-ui.is-hidden").count(), "H hides the photo UI");
  await page.waitForTimeout(300);
  await shot("s8-3-photo-ui-hidden");
  await key("h");
  await until(async () => !(await page.locator(".photo-ui.is-hidden").count()), "H shows it again");

  await page.locator(".sheet--photo .seg[aria-label='Resolution'] button[data-value='2']").click();
  check((await text(".sheet--photo .seg[aria-label='Resolution'] button[data-value='2'] small")) === "2880 × 1800", "2× shows 2880 × 1800");
  // The web caps exports at 2× until 4× is tested for GPU memory: 4× is disabled, struck through and explained.
  const four = page.locator(".sheet--photo .seg[aria-label='Resolution'] button[data-value='4']");
  check(await four.isDisabled(), "4× is disabled on the web");
  check(await four.locator("small.is-over").count() === 1, "4× is struck through on the web");
  check(/4× is off on the web/.test(await text(".sheet--photo .export-cap")), "the export box says why 4× is off");
  // Owner decision A14: no backdrop flash while the export renders offscreen. The window has no
  // camera then, so the canvas must keep showing the last frame; each capture taken while
  // `photo.exporting` is set is compared with one taken just before.
  const canvasMean = async (png) => {
    const b64 = (png ?? (await page.locator("canvas#hex-world").screenshot())).toString("base64");
    return page.evaluate(async (s) => {
      const bytes = Uint8Array.from(atob(s), (c) => c.charCodeAt(0));
      const bmp = await createImageBitmap(new Blob([bytes], { type: "image/png" }));
      const c = new OffscreenCanvas(64, 40);
      const g = c.getContext("2d");
      g.drawImage(bmp, 0, 0, 64, 40);
      const d = g.getImageData(0, 0, 64, 40).data;
      let sum = 0;
      for (let i = 0; i < d.length; i += 4) sum += (d[i] + d[i + 1] + d[i + 2]) / 765;
      return sum / (d.length / 4);
    }, b64);
  };
  const beforeMean = await canvasMean();
  const downloadP = page.waitForEvent("download", { timeout: 90_000 });
  await page.locator(".sheet--photo .export-go").click();
  // A capture counts only when `photo.exporting` is set both before and after it, so it was taken
  // while the window had no camera; the others are logged as unbracketed.
  const during = [];
  const unbracketed = [];
  let seen = false;
  const flashDeadline = Date.now() + 20_000;
  while (Date.now() < flashDeadline && during.length < 6) {
    const s0 = await st();
    if (!s0.photo.exporting) { if (seen || Date.now() > flashDeadline - 15_000) break; continue; }
    seen = true;
    const png = await page.locator("canvas#hex-world").screenshot();
    const s1 = await st();
    const m = Number((await canvasMean(png)).toFixed(4));
    (s1.photo.exporting ? during : unbracketed).push(m);
  }
  console.log(`photo export: canvas mean before ${beforeMean.toFixed(4)}; ${during.length} capture(s) bracketed by ` +
    `photo.exporting ${JSON.stringify(during)}, unbracketed ${JSON.stringify(unbracketed)}` +
    (during.length ? "" : " (inconclusive: the export ended before a bracketed capture)"));
  check(during.every((m) => Math.abs(m - beforeMean) < 0.25 * beforeMean + 0.01),
    `no backdrop flash while exporting (canvas mean before ${beforeMean.toFixed(4)}, bracketed ${JSON.stringify(during)})`);
  const download = await downloadP;
  const png = `${out}/photo-export-2x.png`;
  await download.saveAs(png);
  const head = readFileSync(png);
  const pngW = head.readUInt32BE(16);
  const pngH = head.readUInt32BE(20);
  check(head.toString("latin1", 1, 4) === "PNG" && pngW === 2880 && pngH === 1800, `2× export is a ${pngW} × ${pngH} PNG (want 2880 × 1800)`);
  check(/^hex-world-\d{4}-\d\d-\d\d\.png$/.test(download.suggestedFilename()), `download named ${download.suggestedFilename()}`);
  await until(async () => /Saved hex-world-/.test(await text(".photo-ui .toast.is-shown")), "Saved toast (S8.6)");
  await shot("s8-6-photo-saved");

  await page.focus("canvas#hex-world");
  await key("p");
  await until(async () => { const s = await st(); return !s.play.photo_mode && !s.photo.on; }, "P leaves photo mode");
  await until(async () => !(await page.locator(".hud.is-hidden").count()), "the play HUD is back");
  const back = (await st()).camera;
  check(back.pos.every((v, i) => Math.abs(v - orbit.pos[i]) < 1e-3), `orbit camera restored at ${back.pos.map((v) => v.toFixed(2))}`);
  check(back.forward.every((v, i) => Math.abs(v - orbit.forward[i]) < 1e-3), "orbit camera faces where it did");
  await until(async () => (await st()).photo.agents_paused === false, "exiting resumes agents");
  const t1 = (await st()).photo.sim_ticks;
  await until(async () => (await st()).photo.sim_ticks > t1, "the car sim ticks again", 3000);

  // Esc leaves photo mode too.
  await key("p");
  await until(async () => (await st()).play.photo_mode, "P enters again");
  // The HUD repaints a frame after the engine; Esc before it would reach the play HUD instead.
  await until(async () => page.locator(".photo-ui:not(.is-off)").count(), "the photo UI is back");
  await key("Escape");
  await until(async () => !(await st()).play.photo_mode, "Esc leaves photo mode");

  // S9 Settings (design/ui/legend/settings.html): Graphics (Quality S9.1, Tilt-shift S9.4) and Audio
  // (S9.5) tabs. The page runs tilt=off, so tilt-shift starts off whatever the tier.
  const tiltSwitch = page.locator(".sheet--settings .switch[aria-label='Tilt-shift']");
  const setRange = (label, v) =>
    page.locator(`.sheet--settings input[aria-label='${label}']`).evaluate((el, v) => {
      el.value = String(v);
      el.dispatchEvent(new Event("input", { bubbles: true }));
      el.dispatchEvent(new Event("change", { bubbles: true }));
    }, v);
  const audio = () => page.evaluate(() => window.hexWorld.audio.state());
  await page.locator(".tool[data-action=settings]").click();
  await until(async () => page.locator(".sheet--settings:not(.is-closed)").count(), "Settings opens");
  check(await page.evaluate(() => document.activeElement?.id === "settings-tab-graphics"), "focus lands on the Graphics tab");
  const tier0 = (await st()).tier;
  check(["detected", "probe"].includes(tier0?.source), `the tier starts from detection (${tier0?.tier}, ${tier0?.source})`);
  const autoUses = tier0.probe?.tier ?? tier0.detected?.tier;
  await until(async () => (await page.locator(".tier[data-tier=auto][aria-checked=true]").count()) === 1, "Auto is checked");
  check((await text(".tier[data-tier=auto] .tag")) === `Uses ${autoUses[0].toUpperCase()}${autoUses.slice(1)} here`, "Auto names the tier it uses");
  check(await page.locator(`.tier[data-tier=${autoUses}] .tag--detected`).count() === 1, "the detected tier's row is tagged");
  const reason = await text(".tier-reason");
  check(reason.startsWith("Detected ") && reason.includes(tier0.detected.reason), `the reason line: "${reason}"`);
  check((await st()).tilt.enabled === false, "tilt-shift starts off (tilt=off)");
  await tiltSwitch.click();
  await until(async () => (await st()).tilt.enabled === true, "the switch turns tilt-shift on");
  await setRange("Tilt-shift strength", 60);
  await until(async () => Math.abs((await st()).tilt.strength - 0.6) < 1e-6, "the slider sets strength 0.6");
  await page.mouse.move(700, 450);
  await page.waitForTimeout(300);
  await page.evaluate(() => document.activeElement?.blur()); // the mockup states carry no focus ring
  await shot("s9-1-settings-graphics");

  // Picking Low: state().tier follows, the scene rebuilds around the same camera, world and HUD
  // state, and the player's tilt-shift (on, 0.6) outlives the tier's default (off on Low).
  const beforeTier = await st();
  await page.locator(".tier[data-tier=low]").click();
  await until(async () => { const t = (await st()).tier; return t.tier === "low" && t.source === "runtime"; }, "picking Low sets state().tier");
  check(await page.locator(".tier[data-tier=low][aria-checked=true]").count() === 1, "the Low row is checked");
  await page.waitForTimeout(500);
  const afterTier = await st();
  check(afterTier.camera.pos.every((v, i) => Math.abs(v - beforeTier.camera.pos[i]) < 1e-3), "the camera keeps its position");
  check(afterTier.camera.forward.every((v, i) => Math.abs(v - beforeTier.camera.forward[i]) < 1e-3), "and its heading");
  const tilesOf = (p) => p.tiles.map((t) => `${t.q},${t.r},${t.tile},${t.rotation}`).sort().join(" ");
  check(tilesOf(afterTier.play) === tilesOf(beforeTier.play), "the world is unchanged");
  check(afterTier.play.selected === beforeTier.play.selected && afterTier.play.tool === beforeTier.play.tool, "the selected tile and tool hold");
  check(afterTier.play.inventory_open === beforeTier.play.inventory_open, "the engine's inventory flag holds");
  check(afterTier.tilt.enabled === true && Math.abs(afterTier.tilt.strength - 0.6) < 1e-6, "the player's tilt-shift survives the tier change");
  check(afterTier.tilt.taps === 8, `tilt-shift takes Low's tap count (${afterTier.tilt.taps})`);

  // Ultra, where detection lands on High: the caution note (S9.2).
  if (await page.locator(".tier[data-tier=ultra]").count()) {
    await page.locator(".tier[data-tier=ultra]").click();
    await until(async () => (await st()).tier.tier === "ultra", "picking Ultra sets the tier");
    await until(async () => page.locator(".sheet--settings .note--caution:not([hidden])").count(), "the Ultra note shows");
    await page.waitForTimeout(300);
    await shot("s9-2-settings-tier-up");
  } else console.log(`ok: Ultra is not offered (Auto uses ${autoUses})`);

  // Keyboard: Home on the radio list returns to Auto, which hands the tier back to detection.
  await page.locator(".tier[data-tier][aria-checked=true]").focus();
  await key("Home");
  await until(async () => { const t = (await st()).tier; return t.source === tier0.source && t.tier === tier0.tier; }, "Home picks Auto: the detected tier is back");
  check(await page.evaluate(() => document.activeElement?.dataset.tier === "auto"), "focus follows to the Auto row");
  await tiltSwitch.click();
  await until(async () => (await st()).tilt.enabled === false, "and tilt-shift off again");
  await page.waitForTimeout(300);
  await page.evaluate(() => document.activeElement?.blur());
  await shot("s9-4-settings-tilt-off");

  // Audio (S9.5): ArrowRight on the tab strip, the mockup's channels at this viewer's levels.
  await page.locator("#settings-tab-graphics").focus();
  await key("ArrowRight");
  await until(async () => page.locator("#settings-tab-audio[aria-selected=true]").count(), "ArrowRight selects the Audio tab");
  const a0 = await audio();
  for (const [label, bus] of [["Master", "master"], ["Music", "music"], ["Ambience", "ambience"], ["Effects", "effects"], ["Interface", "interface"]]) {
    const v = await page.locator(`.sheet--settings input[aria-label='${label} volume']`).inputValue();
    check(Number(v) === Math.round(a0.volumes[bus] * 100), `${label} shows ${v}% (audio ${a0.volumes[bus]})`);
  }
  await page.waitForTimeout(200);
  await page.evaluate(() => document.activeElement?.blur());
  await shot("s9-5-settings-audio");
  await setRange("Master volume", 40);
  await setRange("Music volume", 30);
  await until(async () => { const a = await audio(); return a.volumes.master === 0.4 && a.volumes.music === 0.3; }, "the sliders set audio.state().volumes");
  await page.locator(".sheet--settings .switch[aria-label='Mute everything']").click();
  await until(async () => (await audio()).muted === true, "the switch mutes audio.state()");
  await page.locator(".sheet--settings .switch[aria-label='Mute everything']").click();
  await until(async () => (await audio()).muted === false, "and unmutes");

  // Tab walks the sheet's controls and wraps; it does not toggle Tiles while focus is in Settings.
  const invOpen = (await play()).inventory_open;
  await page.locator(".sheet--settings .switch[aria-label='Mute everything']").focus();
  await key("Tab");
  check(await page.evaluate(() => document.activeElement?.closest(".sheet--settings") != null), "Tab from the last control wraps inside Settings");
  await page.waitForTimeout(100);
  check((await play()).inventory_open === invOpen, "Tab inside Settings leaves the Tiles sheet alone");

  // Esc closes from a focused slider, and focus returns to the Settings tool.
  await page.locator(".sheet--settings input[aria-label='Master volume']").focus();
  await key("Escape");
  await until(async () => page.locator(".sheet--settings.is-closed").count(), "Esc on a slider closes Settings");
  check(await page.evaluate(() => document.activeElement?.closest(".tool[data-action=settings]") != null), "focus returns to the Settings tool");

  // Engine motion follows the OS reduced-motion setting, at start and on change.
  check((await play()).reduced_motion === false, "reduced_motion starts off with the OS setting");
  await page.emulateMedia({ reducedMotion: "reduce" });
  await until(async () => (await play()).reduced_motion === true, "prefers-reduced-motion turns engine reduced motion on");
  await page.emulateMedia({ reducedMotion: "no-preference" });
  await until(async () => (await play()).reduced_motion === false, "and back off");

  // S12 narrow desktop: the bar and sheet never meet.
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.waitForTimeout(400);
  const bar = await page.locator(".bar[data-region=bar]").boundingBox();
  const sheet = await page.locator(".sheet--tiles").boundingBox();
  check(bar.x + bar.width < sheet.x, `bar (right ${Math.round(bar.x + bar.width)}) clears the sheet (left ${Math.round(sheet.x)}) at 1280 px`);
  await shot("s12-narrow-1280x720");

  // Colour grade (Settings → Graphics): a row per engine preset; a pick switches the look live and is
  // kept per viewer in localStorage (hexWorld.grade), outside the world save, so it holds over a reload.
  await page.setViewportSize({ width: 1440, height: 900 });
  const grade = async () => (await st()).grade;
  check((await grade())?.name === "rich-3", `the grade starts at the default rich-3 (${(await grade())?.name})`);
  check((await grade())?.default === "rich-3", `state().grade.default is rich-3 (${(await grade())?.default})`);
  await page.locator(".tool[data-action=settings]").click();
  await until(async () => page.locator(".sheet--settings:not(.is-closed)").count(), "Settings opens for the colour grade");
  await page.locator("#settings-tab-graphics").click();
  await until(async () => page.locator("#settings-tab-graphics[aria-selected=true]").count(), "the Graphics tab is back");
  const presets = (await grade()).presets.map((p) => p.name);
  const rowIds = await page.locator(".tiers[aria-label='Colour grade'] .tier").evaluateAll((els) => els.map((e) => e.dataset.grade));
  check(JSON.stringify(rowIds) === JSON.stringify(presets), `one Colour grade row per engine preset (${rowIds.join(", ")})`);
  check((await text(".tier[data-grade=rich-2] small")) === (await grade()).presets.find((p) => p.name === "rich-2").detail, "the row shows the engine's description");
  check(await page.locator(".tier[data-grade=rich-3][aria-checked=true]").count() === 1, "the Rich 3 row is checked");
  const tags = await page.locator(".tiers[aria-label='Colour grade'] .tier").evaluateAll((els) =>
    els.map((e) => [e.dataset.grade, e.querySelector(".tag")?.textContent ?? null]).filter(([, t]) => t));
  check(JSON.stringify(tags) === JSON.stringify([["rich-3", "Default"]]), `only the Rich 3 row is labelled Default (${JSON.stringify(tags)})`);
  await page.locator(".tier[data-grade=rich-2]").scrollIntoViewIfNeeded();
  await page.locator(".tier[data-grade=rich-2]").click();
  await until(async () => (await grade()).name === "rich-2", "picking Rich 2 sets state().grade");
  check((await grade()).name === "rich-2", "a click on Rich 2 sets state().grade to rich-2");
  check(await page.locator(".tier[data-grade=rich-2][aria-checked=true]").count() === 1, "the Rich 2 row is checked");
  check(await page.evaluate(() => localStorage.getItem("hexWorld.grade")) === "rich-2", "the pick is stored for this viewer");
  await page.mouse.move(700, 450);
  await page.waitForTimeout(300);
  await page.evaluate(() => document.activeElement?.blur());
  await page.locator(".tiers[aria-label='Colour grade']").scrollIntoViewIfNeeded();
  await shot("s9-6-settings-colour-grade");
  // Keyboard: ArrowUp from Rich 2 moves to Rich 1 and picks it.
  await page.locator(".tier[data-grade=rich-2]").focus();
  await key("ArrowUp");
  await until(async () => (await grade()).name === "rich-1", "ArrowUp picks Rich 1");
  check(await page.evaluate(() => document.activeElement?.dataset.grade === "rich-1"), "focus follows to the Rich 1 row");
  await key("ArrowDown");
  await until(async () => (await grade()).name === "rich-2", "ArrowDown picks Rich 2 again");

  // Reload: the stored grade is sent once the overlay starts, and the row shows it.
  await page.reload({ waitUntil: "commit" });
  await page.waitForFunction(() => window.hexWorld?.ready(), null, { timeout: 90_000 });
  await until(async () => (await play())?.tiles != null, "play is back after the reload", 30_000);
  await until(async () => (await grade())?.name === "rich-2", "the stored grade holds over a reload", 15_000);
  check((await grade())?.name === "rich-2", `state().grade reports ${(await grade())?.name} after the reload`);
  await page.locator(".tool[data-action=settings]").click();
  await until(async () => page.locator(".sheet--settings:not(.is-closed)").count(), "Settings opens after the reload");
  await until(async () => page.locator(".tier[data-grade=rich-2][aria-checked=true]").count(), "the Rich 2 row is checked after the reload");
  check(await page.locator(".tier[data-grade=rich-2][aria-checked=true]").count() === 1, "the Rich 2 row is checked after the reload");

  // ?grade= in the address wins over the stored choice and leaves it stored.
  await page.goto(base + "?scenario=play&tilt=off&fps=off&grade=aces", { waitUntil: "commit" });
  await page.waitForFunction(() => window.hexWorld?.ready(), null, { timeout: 90_000 });
  await until(async () => (await play())?.tiles != null, "play is back with ?grade=aces", 30_000);
  await page.waitForTimeout(1000);
  check((await grade()).name === "aces", `?grade=aces wins over the stored grade (${(await grade()).name})`);
  check(await page.evaluate(() => localStorage.getItem("hexWorld.grade")) === "rich-2", "and the stored choice stays rich-2");

  // The grade reaches the WebGPU frame: engine captures (canvas only, no HUD) at one camera under
  // tony, aces, rich-2 and tony again. Agents and water move between frames, so tony against tony is
  // the noise floor. aces changes the tone mapper; rich-2 keeps tony's and differs only by its
  // ColorGrading (sections and post-saturation), so its difference proves the grading runs on WebGPU.
  const capture = async (g, name) => {
    await page.evaluate((g) => window.hexWorld.cmd([{ cmd: "set_grade", grade: g }, { cmd: "wait", frames: 20 }]), g);
    await until(async () => (await grade()).name === g, `the frame grade is ${g}`);
    return page.evaluate((n) => window.hexWorld.screenshot(n, 60_000), name);
  };
  await page.evaluate(() => window.hexWorld.cmd({ cmd: "camera", pos: [18, 22, 26], look_at: [0, 0, 0] }));
  const frames = [
    await capture("tony", "grade-tony-a"),
    await capture("aces", "grade-aces"),
    await capture("rich-2", "grade-rich-2"),
    await capture("tony", "grade-tony-b"),
  ];
  const stats = await page.evaluate(async (urls) => {
    const W = 480;
    const pixels = await Promise.all(urls.map(async (u) => {
      // The page CSP refuses fetch() of data: URLs (connect-src 'self') but lets images load them.
      const bmp = new Image();
      bmp.src = u;
      await bmp.decode();
      const c = new OffscreenCanvas(W, Math.round((W * bmp.height) / bmp.width));
      const g = c.getContext("2d");
      g.drawImage(bmp, 0, 0, c.width, c.height);
      return g.getImageData(0, 0, c.width, c.height).data;
    }));
    const sat = (d) => {
      let s = 0, l = 0;
      for (let i = 0; i < d.length; i += 4) {
        const mx = Math.max(d[i], d[i + 1], d[i + 2]), mn = Math.min(d[i], d[i + 1], d[i + 2]);
        s += mx ? (mx - mn) / mx : 0;
        l += 0.2126 * d[i] + 0.7152 * d[i + 1] + 0.0722 * d[i + 2];
      }
      return { sat: s / (d.length / 4), luma: l / (d.length / 4) };
    };
    const mad = (a, b) => {
      let t = 0;
      for (let i = 0; i < a.length; i += 4) t += Math.abs(a[i] - b[i]) + Math.abs(a[i + 1] - b[i + 1]) + Math.abs(a[i + 2] - b[i + 2]);
      return t / ((a.length / 4) * 3);
    };
    return {
      tony: sat(pixels[0]),
      aces: sat(pixels[1]),
      rich2: sat(pixels[2]),
      tony2: sat(pixels[3]),
      gradeDiff: mad(pixels[0], pixels[1]),
      gradingDiff: mad(pixels[0], pixels[2]),
      noise: mad(pixels[0], pixels[3]),
    };
  }, frames);
  const f = (v) => v.toFixed(3);
  console.log(`grade frames: tony sat ${f(stats.tony.sat)} luma ${f(stats.tony.luma)} | aces sat ${f(stats.aces.sat)} luma ${f(stats.aces.luma)} | tony again sat ${f(stats.tony2.sat)} luma ${f(stats.tony2.luma)}`);
  console.log(`grade frames: mean |tony - aces| ${f(stats.gradeDiff)} / 255, mean |tony - tony again| ${f(stats.noise)} / 255`);
  check(stats.gradeDiff > 2 && stats.gradeDiff > 3 * stats.noise, `the grade changes the WebGPU frame (${f(stats.gradeDiff)} against noise ${f(stats.noise)})`);
  console.log(`grade frames: rich-2 sat ${f(stats.rich2.sat)} luma ${f(stats.rich2.luma)}, mean |tony - rich-2| ${f(stats.gradingDiff)} / 255`);
  check(
    stats.gradingDiff > 1 && stats.gradingDiff > 3 * stats.noise,
    `ColorGrading changes the WebGPU frame (rich-2 ${f(stats.gradingDiff)} against noise ${f(stats.noise)})`,
  );
  check(
    stats.rich2.sat > Math.max(stats.tony.sat, stats.tony2.sat) + 0.01,
    `rich-2 raises saturation on WebGPU (${f(stats.rich2.sat)} against tony ${f(stats.tony.sat)})`,
  );
} catch (e) {
  fail(String(e?.stack ?? e));
} finally {
  if (errors.length) fail("console errors: " + errors.join(" | "));
  await browser.close();
  server.close();
}
console.log(failures ? `ui e2e FAILED (${failures})` : `ui e2e OK: shots in ${out}/`);
process.exitCode = failures ? 1 : 0;
