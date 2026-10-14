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
const play = () => page.evaluate(() => window.hexWorld.state().play);
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
/** Events since the last call (the overlay watches without draining them from us). */
const events = () => page.evaluate(() => window.hexWorld.events());
const key = (k) => page.keyboard.press(k);
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
  check((await text(".key__list")).includes("Grass, forest or beach on one edge"), "key lists the grass edge");
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

  // Q5: the pointer over the HUD keeps the last slot's filter, and the engine keeps hovering it.
  const [rx, ry] = await page.locator(".row[data-tile=river_bend]").boundingBox().then((b) => [b.x + b.width / 2, b.y + b.height / 2]);
  await page.mouse.move(rx, ry, { steps: 4 });
  await page.waitForTimeout(400);
  check(same((await play()).hovered, 1, 0), "engine still hovers (1, 0) while the pointer is on the sheet");
  check((await text(".key__title")) === "This slot needs", "sheet keeps the slot's filter over the HUD");
  await shot("s3-5-tile-hover");

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
  await until(async () => (await play()).last_result?.ok === false, "the click is refused");
  check((await play()).tiles.length === 1, "refused placement leaves 1 tile");
  check((await events()).some((e) => e.type === "placementRefused"), "placementRefused event");
  await shot("s5-2-place-rejected");

  // S5.1 place: Road straight at (1, 0).
  await page.locator(".row[data-tile=road_straight]").click();
  await until(async () => (await play()).selected === "road_straight", "click selects Road straight");
  await pointAt(1, 0);
  await page.mouse.down();
  await page.mouse.up();
  await until(async () => (await play()).tiles.length === 2, "road placed");
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
  await page.mouse.down();
  await page.mouse.up();
  await until(async () => (await play()).tiles.length === 3, "meadow placed at (2, 0)");

  // S7 bulldoze: B, a refused removal that would split the world, then a removable leaf.
  await key("b");
  await until(async () => (await play()).tool === "bulldoze", "B turns on bulldoze");
  check(await page.locator(".tool[data-action=bulldoze].is-active").count() === 1, "bulldoze tool active");
  check((await text(".key__title")) === "Bulldoze", "sheet key turns into the bulldoze legend");
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

  // S3.4 nothing fits: a river straight west of the start meadow and a beach turned so its ocean
  // edge faces (-1, 1) leave that slot needing a river and the sea at once, which no tile carries.
  // Set up through the bridge (auto-rotate picks those rotations), then driven by mouse.
  await page.evaluate(() => {
    const h = window.hexWorld;
    h.selectTile("river_straight"); h.placeTile(-1, 0);
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

  // Engine motion follows the OS reduced-motion setting, at start and on change.
  check((await play()).reduced_motion === false, "reduced_motion starts off with the OS setting");
  await page.emulateMedia({ reducedMotion: "reduce" });
  await until(async () => (await play()).reduced_motion === true, "prefers-reduced-motion turns engine reduced motion on");
  await page.emulateMedia({ reducedMotion: "no-preference" });
  await until(async () => (await play()).reduced_motion === false, "and back off");

  // S12 narrow desktop: the bar and sheet never meet.
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.waitForTimeout(400);
  const bar = await page.locator(".bar").boundingBox();
  const sheet = await page.locator(".sheet").boundingBox();
  check(bar.x + bar.width < sheet.x, `bar (right ${Math.round(bar.x + bar.width)}) clears the sheet (left ${Math.round(sheet.x)}) at 1280 px`);
  await shot("s12-narrow-1280x720");
} catch (e) {
  fail(String(e?.stack ?? e));
} finally {
  if (errors.length) fail("console errors: " + errors.join(" | "));
  await browser.close();
  server.close();
}
console.log(failures ? `ui e2e FAILED (${failures})` : `ui e2e OK: shots in ${out}/`);
process.exitCode = failures ? 1 : 0;
