// Local-only Debug tab e2e (make ui-e2e): GPU Chromium on the built site. Builds a small world with
// every socket kind, then drives each developer overlay through the Settings Debug tab and through
// the `debug` command, reads state().debug back, checks the LOD census follows Force LOD, checks a
// reload starts with everything off and that `?debug=` turns overlays on from the start. Every step
// saves a capture to DEBUG_OUT (default e2e/test-results/debug; DEBUG_OUT=docs/checkpoints/debug
// refreshes the committed checkpoint shots).
import { chromium } from "playwright";
import { createServer } from "node:http";
import { mkdirSync, readFileSync } from "node:fs";
import { extname, join } from "node:path";
import { BAD_TEXT } from "./console-errors.mjs";

const dist = process.argv[2] ?? "dist";
const out = process.env.DEBUG_OUT ?? "e2e/test-results/debug";
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
// The first-run callouts are done, so they stay out of the captures.
await page.addInitScript(() => { try { localStorage.setItem("hexWorld.hints", "99"); } catch {} });
const errors = [];
page.on("pageerror", (e) => errors.push(String(e)));
page.on("console", (m) => { if (m.type() === "error" || BAD_TEXT.test(m.text())) errors.push(m.text()); });

const shot = (name) => page.screenshot({ path: `${out}/${name}.jpg`, type: "jpeg", quality: 88 });
const state = () => page.evaluate(() => window.hexWorld.state());
const dbg = async () => (await state()).debug;
const cmd = (c) => page.evaluate((c) => window.hexWorld.cmd(c), c);
async function until(fn, what, ms = 15_000) {
  const deadline = Date.now() + ms;
  for (;;) {
    const v = await fn();
    if (v) return v;
    if (Date.now() > deadline) { fail(`timed out: ${what}`); return v; }
    await page.waitForTimeout(50);
  }
}
const OFF = { lod_tint: false, force_lod: "auto", perf: false, grid: false, sockets: false, colliders: false, scatter: false, networks: false };
const SWITCHES = ["lod_tint", "perf", "grid", "sockets", "colliders", "scatter", "networks"];
const allOff = (d) => d && Object.entries(OFF).every(([k, v]) => d[k] === v);
const sum = (a) => a.reduce((x, y) => x + y, 0);

// The village, forest, river, beach and ocean the first-person checkpoint walks (docs/checkpoints/first-person).
const WORLD = [
  ["road_straight", [1, 0], [1, 1]],
  ["house_cluster", [1, 2], [1, -1]],
  ["forest", [-1, 0], [-1, 1]],
  ["river_straight", [-2, 1], [-2, 2]],
  ["beach", [-1, -1]],
  ["ocean", [-1, -2], [-2, -1]],
].flatMap(([id, ...cells]) => [{ cmd: "select_tile", id }, ...cells.map(([q, r]) => ({ cmd: "place", q, r }))]);
const CAMERA = { cmd: "camera", pos: [70, 95, 125], look_at: [-12, 0, -4] };

async function boot(query) {
  await page.goto(base + `?scenario=play&tilt=off&fps=off${query}`, { waitUntil: "commit" });
  await page.waitForFunction(() => window.hexWorld?.ready(), null, { timeout: 90_000 });
  await until(async () => !(await page.locator(".load").count()), "loading layer removed after ready", 10_000);
}

async function buildWorld() {
  await cmd(WORLD);
  await until(async () => {
    const s = await state();
    return s.play.tiles.length >= 12 && !(s.pack_waiting ?? []).length && !s.play.rising.length && s.queued === 0;
  }, "the world is placed, its packs loaded and every tile settled", 60_000);
  await cmd([CAMERA, { cmd: "wait", frames: 2 }]);
  // The Tiles sheet closes so the captures show the whole world.
  await page.focus("canvas#hex-world");
  await page.keyboard.press("Tab");
  await until(async () => page.locator(".sheet--tiles.is-closed").count(), "the Tiles sheet closes");
  await page.waitForTimeout(600);
}
/** The pointer leaves the tools (no tooltip) for a corner with no slot under it (no ghost). */
const park = () => page.mouse.move(2, page.viewportSize().height - 2);
/** The Settings tool, also when a phone-width bar has scrolled it out of view. */
const settingsTool = () => page.locator(".tool[data-action=settings]").dispatchEvent("click");

const settingsOpen = () => page.locator(".sheet--settings:not(.is-closed)").count();
async function openDebugTab() {
  if (!(await settingsOpen())) await settingsTool();
  await until(settingsOpen, "Settings opens");
  await page.locator("#settings-tab-debug").click();
  await page.locator("#settings-tab-debug").focus();
  await until(async () => page.locator("#settings-debug:not([hidden])").count(), "the Debug tab shows");
}
async function closeSettings() {
  if (await settingsOpen()) await settingsTool();
  await until(async () => !(await settingsOpen()), "Settings closes");
  // Focus leaves the Settings tool, so its tooltip and focus ring stay out of the captures.
  await page.focus("canvas#hex-world");
  await park();
}

try {
  // A plain load: every overlay off, and no overlay system has run.
  await boot("");
  const first = await dbg();
  check(allOff(first), `state().debug starts with every overlay off: ${JSON.stringify(first)}`);
  check(first?.work === 0, `no overlay system ran while every overlay was off (work ${first?.work})`);
  console.log(`tier: ${JSON.stringify((await state()).tier?.active ?? (await state()).tier)}`);
  await buildWorld();
  check((await dbg()).work === 0, "building the world ran no overlay system");

  // The tab: every switch through the UI, then off through the bridge.
  await openDebugTab();
  await park();
  await page.waitForTimeout(300);
  await shot("debug-tab-1440x900");
  check(await page.locator(".lod-legend__row").count() === 3, "the LOD key lists three levels");
  for (const id of SWITCHES) {
    await page.locator(`.switch[data-debug=${id}]`).click();
    await until(async () => (await dbg())[id] === true, `the ${id} switch turns its overlay on`);
    await cmd({ cmd: "debug", [id]: false });
    await until(async () => (await dbg())[id] === false, `the debug command turns ${id} off`);
    await until(async () => (await page.locator(`.switch[data-debug=${id}]`).getAttribute("aria-checked")) === "false",
      `the ${id} switch follows the bridge`);
  }

  // LOD census: tint on through the UI, counts non-zero, Force LOD 2 then 0 through the UI and the bridge.
  await page.locator(".switch[data-debug=lod_tint]").click();
  const auto = await until(async () => { const d = await dbg(); return d.lod_tint && sum(d.lod.by_lod) > 0 && d; },
    "LOD tint on counts drawn parts");
  console.log(`auto by_lod ${JSON.stringify(auto?.lod)}`);
  await page.locator(".field--force .seg button[data-value='2']").click();
  const two = await until(async () => { const d = await dbg(); const b = d.lod.by_lod;
    return d.force_lod === "2" && b[2] > 0 && b[0] === 0 && b[1] === 0 && d; }, "Force LOD 2 draws LOD2 parts only");
  console.log(`forced 2 by_lod ${JSON.stringify(two?.lod)}`);
  check(two && two.lod.tris_by_lod[2] > 0, "LOD2 parts carry triangles");
  await page.waitForTimeout(300);
  await shot("debug-tab-lod-forced-1440x900");
  await cmd({ cmd: "debug", force_lod: "0" });
  const zero = await until(async () => { const d = await dbg(); const b = d.lod.by_lod;
    return d.force_lod === "0" && b[0] > 0 && b[1] === 0 && b[2] === 0 && d; }, "Force LOD 0 draws LOD0 parts only");
  check(zero && two && zero.lod.tris_by_lod[0] > two.lod.tris_by_lod[2], "LOD0 draws more triangles than LOD2");
  await until(async () => (await page.locator(".field--force .seg button[data-value='0']").getAttribute("aria-checked")) === "true",
    "the Force LOD control follows the bridge");
  await page.locator(".field--force .seg button[data-value='auto']").click();
  await until(async () => (await dbg()).force_lod === "auto", "Auto restores the distance bands");

  // One capture per overlay, the sheet closed.
  await closeSettings();
  const only = async (name, c) => {
    await cmd({ cmd: "debug", ...OFF, ...c });
    await page.waitForTimeout(900);
    await shot(`debug-${name}`);
  };
  await only("lod-tint", { lod_tint: true });
  await only("lod-force-2", { lod_tint: true, force_lod: "2" });
  await only("lod-force-0", { lod_tint: true, force_lod: "0" });
  await cmd({ cmd: "debug", ...OFF, perf: true });
  await until(async () => (await dbg()).frame.fps > 0, "the perf sampler reports a frame rate");
  await until(async () => page.locator(".debug-perf:not([hidden]) .debug-perf__row").count(), "the perf corner shows with Settings closed");
  await page.waitForTimeout(700);
  const perfText = await page.locator(".debug-perf").innerText();
  check(/fps/.test(perfText) && /Grass blades/.test(perfText), `the perf corner reads out: ${perfText.replace(/\n/g, " | ")}`);
  // Closer in, so the meadow's grass is inside its build radius and the readout counts blades.
  await cmd({ cmd: "camera", pos: [18, 34, 52], look_at: [-4, 0, 4] });
  await until(async () => ((await state()).grass?.blades ?? 0) > 0, "grass draws near the camera", 20_000);
  await page.waitForTimeout(900);
  const nearText = await page.locator(".debug-perf").innerText();
  check(!/Grass blades\s+0\b/.test(nearText), `the perf corner counts drawn grass blades: ${nearText.replace(/\n/g, " | ")}`);
  await shot("debug-perf");
  await cmd(CAMERA);
  await cmd({ cmd: "debug", ...OFF, grid: true });
  await until(async () => page.locator(".debug-labels:not([hidden]) .debug-label:not([hidden])").count(), "the grid labels show");
  check((await page.locator(".debug-label").allInnerTexts()).includes("0, 0"), "the origin tile is labelled 0, 0");
  await page.waitForTimeout(400);
  await shot("debug-grid");
  await only("sockets", { sockets: true });
  await only("colliders", { colliders: true });
  await only("scatter", { scatter: true });
  await only("networks", { networks: true });
  await only("all", { lod_tint: true, perf: true, grid: true, sockets: true, colliders: true, scatter: true, networks: true });
  await cmd({ cmd: "debug", ...OFF });
  await until(async () => allOff(await dbg()), "the bridge turns everything off");
  check(await page.locator(".debug-perf[hidden]").count() === 1, "the perf corner hides when perf is off");

  // The tab at phone size.
  await page.setViewportSize({ width: 375, height: 667 });
  await page.waitForTimeout(400);
  await openDebugTab();
  await page.waitForTimeout(300);
  await shot("debug-tab-375x667");
  await closeSettings();
  await page.setViewportSize({ width: 1440, height: 900 });

  // A reload starts with everything off; `?debug=` turns overlays on from the start.
  await cmd({ cmd: "debug", lod_tint: true, perf: true, grid: true, force_lod: "1" });
  await until(async () => (await dbg()).grid, "overlays on before the reload");
  await boot("");
  check(allOff(await dbg()), "a reload starts with every overlay off");
  await boot("&debug=grid,lod,lod2");
  const flagged = await dbg();
  check(flagged.grid && flagged.lod_tint && flagged.force_lod === "2" && !flagged.perf, `?debug=grid,lod,lod2 starts them on: ${JSON.stringify(flagged)}`);
  await openDebugTab();
  await until(async () => (await page.locator(".switch[data-debug=grid]").getAttribute("aria-checked")) === "true",
    "the tab shows a flag-started overlay");

  check(errors.length === 0, `no console errors (${errors.length})`);
  for (const e of errors.slice(0, 5)) console.error("  console:", e.slice(0, 300));
} catch (e) {
  fail(`threw: ${e?.stack ?? e}`);
} finally {
  await browser.close();
  server.close();
}
console.log(failures ? `debug e2e: ${failures} failure(s)` : `debug e2e: all checks passed (${out})`);
process.exit(failures ? 1 : 0);
