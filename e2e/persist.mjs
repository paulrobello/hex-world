// Local-only persistence e2e (make ui-e2e): GPU Chromium on the built site. Autosave to IndexedDB
// and resume after a reload, rename, export, new world, import by picker, a refused file by
// drag-and-drop and by picker, load and delete a slot, and the storage-blocked bar. Asserts through
// hexWorld.state() and the HUD's text, and saves captures to e2e/test-results/persistence/ (set
// PERSIST_OUT=docs/checkpoints/persistence to refresh the owner's committed captures).
import { chromium } from "playwright";
import { createServer } from "node:http";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { extname, join } from "node:path";
import { BAD_TEXT } from "./console-errors.mjs";

const dist = process.argv[2] ?? "dist";
const out = process.env.PERSIST_OUT ?? "e2e/test-results/persistence";
const work = "e2e/test-results/persist";
mkdirSync(out, { recursive: true });
mkdirSync(work, { recursive: true });
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
const base = `http://127.0.0.1:${server.address().port}/?scenario=play&tilt=off&fps=off`;

const gpuArgs = process.env.PW_GPU === "swiftshader"
  ? ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--enable-unsafe-webgpu"]
  : ["--use-angle=metal", "--enable-gpu", "--ignore-gpu-blocklist", "--enable-unsafe-webgpu"];
let failures = 0;
const fail = (m) => { console.error("FAIL:", m); failures++; };
const check = (ok, m) => { if (!ok) fail(m); else console.log("ok:", m); };

const browser = await chromium.launch({ channel: "chromium", headless: true, args: gpuArgs });
const errors = [];

async function open(context) {
  const page = await context.newPage();
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => { if (m.type() === "error" || BAD_TEXT.test(m.text())) errors.push(m.text()); });
  return page;
}

function helpers(page) {
  const shot = (name) => page.screenshot({ path: `${out}/${name}.jpg`, type: "jpeg", quality: 88 });
  const play = () => page.evaluate(() => window.hexWorld.state().play);
  const text = (sel) => page.locator(sel).first().innerText().catch(() => "");
  async function until(fn, what, ms = 15_000) {
    const deadline = Date.now() + ms;
    for (;;) {
      const v = await fn();
      if (v) return v;
      if (Date.now() > deadline) { fail(`timed out: ${what}`); return v; }
      await page.waitForTimeout(50);
    }
  }
  async function boot() {
    await page.goto(base);
    await page.waitForFunction(() => window.hexWorld?.ready() && window.hexWorld.state().play, null, { timeout: 90_000 });
    await until(async () => !(await page.locator(".load").count()), "loading layer gone");
  }
  const tileSet = (p) => p.tiles.map((t) => `${t.q},${t.r},${t.tile},${t.rotation}`).sort().join(" ");
  return { shot, play, text, until, boot, tileSet };
}

const worldsOpen = (page) => page.locator(".sheet--worlds:not(.is-closed)").count();

try {
  // ---------- Storage on: autosave, resume, rename, export, new world, import, load, delete ----------
  const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 }, acceptDownloads: true });
  const page = await open(ctx);
  const { shot, play, text, until, boot, tileSet } = helpers(page);
  await boot();
  await until(async () => (await text(".bar__world")).includes("World 1"), "start world named World 1");
  check((await text(".bar__world")).includes("1 tile"), "bar counts 1 tile");

  // Place through the bridge (the HUD e2e drives the mouse), from a moved camera.
  await page.evaluate(() => {
    const h = window.hexWorld;
    h.cmd({ cmd: "camera", pos: [40, 70, 90], look_at: [10, 0, 0] });
    h.selectTile("meadow"); h.placeTile(1, 0);
    h.selectTile("forest"); h.placeTile(-1, 0);
    h.selectTile("road_straight"); h.placeTile(0, 1);
    h.selectTile("meadow"); h.placeTile(2, 0);
  });
  await until(async () => (await play()).tiles.length === 5, "four tiles placed");
  await until(async () => (await text(".bar__world")).includes("Saving…"), "bar shows Saving… while the autosave is pending", 3_000);
  await shot("autosave-saving");
  await page.evaluate(() => window.hexWorld.undo());
  await until(async () => (await play()).tiles.length === 4, "undo");
  await until(async () => (await text(".bar__world")).includes("Saved"), "bar shows Saved after the debounced autosave");
  const title = (await text(".bar__world")).replace(/\s+/g, " ");
  check(/World 1 4 tiles · Saved/.test(title), `bar title: name, tile count, Saved ("${title}")`);
  await page.waitForTimeout(300);
  await shot("autosave-saved");
  const before = await play();
  const beforeCam = (await page.evaluate(() => window.hexWorld.state().camera)).pos;

  // Reload: the most recent slot resumes, with its history and camera.
  await page.reload();
  await page.waitForFunction(() => window.hexWorld?.ready() && window.hexWorld.state().play, null, { timeout: 90_000 });
  await until(async () => (await play()).tiles.length === 4, "after reload the world returns (4 tiles)");
  const after = await play();
  // A resumed world is drawn settled (water filled, agents ready): no tile rises, and no rise
  // stage other than settled is reported, for a second of frames after the world returns.
  const rising = [];
  for (let i = 0; i < 20; i++) {
    for (const t of (await play()).rising) rising.push(`${t.q},${t.r}:${t.stage}`);
    await page.waitForTimeout(50);
  }
  check(!rising.length, `after the reload no tile rises (${rising.slice(0, 4).join(" ")})`);
  const cues = (await page.evaluate(() => window.hexWorld.events())).filter((e) => e.type === "riseStage" && e.stage !== "settled");
  check(!cues.length, `after the reload no rise stage other than settled (${cues.map((e) => e.stage).slice(0, 4)})`);
  check(tileSet(after) === tileSet(before), "every tile, rotation and cell matches after the reload");
  check(after.can_undo && after.can_redo, "undo and redo stacks survive the reload");
  const cam = (await page.evaluate(() => window.hexWorld.state().camera)).pos;
  check(cam.every((v, i) => Math.abs(v - beforeCam[i]) < 0.5), `camera restored (${cam.map((v) => v.toFixed(1))})`);
  await until(async () => (await text(".bar__world")).includes("World 1"), "bar keeps the world's name after the reload");
  await page.evaluate(() => window.hexWorld.redo());
  await until(async () => (await play()).tiles.length === 5, "redo after the reload restores the undone tile");
  await until(async () => (await text(".bar__world")).includes("Saved"), "redo autosaves");
  await page.waitForTimeout(1200);
  await shot("resumed-after-reload");

  // A reload inside the autosave debounce keeps the edit: page hide writes it at once.
  await page.evaluate(() => { const h = window.hexWorld; h.selectTile("meadow"); h.placeTile(3, 0); });
  await until(async () => (await play()).tiles.length === 6, "a sixth tile placed");
  await until(async () => (await text(".bar__world")).includes("Saving…"), "its autosave is still pending", 2_000);
  await page.reload();
  await page.waitForFunction(() => window.hexWorld?.ready() && window.hexWorld.state().play, null, { timeout: 90_000 });
  await until(async () => (await play()).tiles.length === 6, "the edit made inside the debounce survives the reload (page hide flush)");
  await until(async () => (await text(".bar__world")).includes("World 1"), "same world after the quick reload");

  // Settings round trip (S9): quality and tilt-shift travel in the save and apply when it loads;
  // audio levels and mute are this viewer's own (hexWorld.audio, localStorage).
  {
    const st = () => page.evaluate(() => window.hexWorld.state());
    const audio = () => page.evaluate(() => window.hexWorld.audio.state());
    const setRange = (label, v) =>
      page.locator(`.sheet--settings input[aria-label='${label}']`).evaluate((el, v) => {
        el.value = String(v);
        el.dispatchEvent(new Event("input", { bubbles: true }));
        el.dispatchEvent(new Event("change", { bubbles: true }));
      }, v);
    await until(async () => (await text(".bar__world")).includes("Saved"), "settled before the settings change");
    await page.locator(".tool[data-action=settings]").click();
    await until(async () => page.locator(".sheet--settings:not(.is-closed)").count(), "Settings opens");
    const detected = (await st()).tier;
    await page.locator(".tier[data-tier=low]").click();
    await until(async () => { const t = (await st()).tier; return t.tier === "low" && t.source === "runtime"; }, "Low picked");
    await page.locator(".sheet--settings .switch[aria-label='Tilt-shift']").click();
    await until(async () => (await st()).tilt.enabled, "tilt-shift on (the page runs tilt=off)");
    await setRange("Tilt-shift strength", 35);
    await until(async () => Math.abs((await st()).tilt.strength - 0.35) < 1e-6, "strength 0.35");
    await page.locator("#settings-tab-audio").click();
    await setRange("Master volume", 30);
    await page.locator(".sheet--settings .switch[aria-label='Mute everything']").click();
    await until(async () => { const a = await audio(); return a.muted && a.volumes.master === 0.3; }, "master 0.3, muted");
    await until(async () => (await text(".bar__world")).includes("Saved"), "the settings change is autosaved");
    await page.reload();
    await page.waitForFunction(() => window.hexWorld?.ready() && window.hexWorld.state().play, null, { timeout: 90_000 });
    await until(async () => (await play()).tiles.length === 6, "the world resumes after the reload");
    await until(async () => { const t = (await st()).tier; return t.tier === "low" && t.reason === "loaded save"; }, "the loaded save applies its quality (Low)");
    const s = await st();
    check(s.tier.detected?.tier === detected.detected?.tier, `detection still reports ${s.tier.detected?.tier}`);
    check(s.tilt.enabled === true && Math.abs(s.tilt.strength - 0.35) < 1e-6, `the save's tilt-shift holds through the tier rebuild (${s.tilt.enabled}, ${s.tilt.strength})`);
    const a = await audio();
    check(a.muted === true && a.volumes.master === 0.3, `this viewer's mute and master level persist (${a.muted}, ${a.volumes.master})`);
    await page.locator(".tool[data-action=settings]").click();
    await until(async () => page.locator(".sheet--settings:not(.is-closed)").count(), "Settings opens after the reload");
    // The sheet repaints its controls from state after it opens, so wait for the DOM, not the first frame.
    if (await until(async () => (await page.locator(".tier[data-tier=low][aria-checked=true]").count()) === 1, "Settings shows Low checked"))
      console.log("ok: Settings shows Low checked");
    if (await until(async () => (await page.locator(".sheet--settings .switch[aria-label='Tilt-shift'][aria-checked=true]").count()) === 1, "and tilt-shift on"))
      console.log("ok: and tilt-shift on");
    await page.locator("#settings-tab-audio").click();
    check((await page.locator(".sheet--settings input[aria-label='Master volume']").inputValue()) === "30", "the Master slider reads 30%");
    check(await page.locator(".sheet--settings .switch[aria-label='Mute everything'][aria-checked=true]").count() === 1, "Mute everything reads on");
    await shot("settings-after-reload");
    // Back to Auto, untilted and unmuted at the default level for the steps below.
    await page.locator(".sheet--settings .switch[aria-label='Mute everything']").click();
    await setRange("Master volume", 80);
    await page.locator("#settings-tab-graphics").click();
    await page.locator(".tier[data-tier=auto]").click();
    await until(async () => { const t = (await st()).tier; return t.tier === detected.tier && t.source === detected.source; }, "Auto returns to the detected tier");
    await page.locator(".sheet--settings .switch[aria-label='Tilt-shift']").click();
    await until(async () => !(await st()).tilt.enabled, "tilt-shift off");
    await page.keyboard.press("Escape");
    await until(async () => page.locator(".sheet--settings.is-closed").count(), "Esc closes Settings");
    await until(async () => (await text(".bar__world")).includes("Saved"), "the restored settings are saved");
  }

  // Worlds sheet: rename the current world.
  await page.locator(".bar__world").click();
  await until(async () => worldsOpen(page), "the bar title opens the Worlds sheet");
  await page.locator(".current [data-act=rename]").click();
  await page.locator(".current .rename").fill("Riverside");
  await page.keyboard.press("Enter");
  await until(async () => (await text(".bar__world")).includes("Riverside"), "rename shows in the bar title");
  await page.waitForTimeout(300);
  await shot("worlds-sheet");

  // Export downloads <world name>.hexworld.
  const [dl] = await Promise.all([page.waitForEvent("download"), page.locator(".current [data-act=export]").click()]);
  check(dl.suggestedFilename() === "Riverside.hexworld", `export is named Riverside.hexworld (${dl.suggestedFilename()})`);
  const exported = `${work}/Riverside.hexworld`;
  await dl.saveAs(exported);
  const bytes = readFileSync(exported);
  check(bytes.subarray(0, 8).toString("latin1") === "HEXWORLD", "the export starts with the HEXWORLD magic");
  await until(async () => (await text(".toast.is-shown")).includes("Exported Riverside.hexworld"), "export toast");
  await page.waitForTimeout(250);
  await shot("export-done");
  const exportedWorld = await play();

  // New world: the start tile under the next free name.
  await page.locator(".sheet--worlds [data-act=new]").click();
  await until(async () => (await play()).tiles.length === 1, "New world resets to the start tile");
  await until(async () => (await text(".bar__world")).includes("World 1"), "the new world takes the next free name");
  check(!(await play()).can_undo, "a new world has no history");

  // Import by the picker: the tiles match the export.
  await page.locator(".sheet--worlds input[type=file]").setInputFiles(exported);
  await until(async () => (await play()).tiles.length === exportedWorld.tiles.length, "import loads the exported world");
  check(tileSet(await play()) === tileSet(exportedWorld), "imported tiles match the exported world");
  await until(async () => (await text(".bar__world")).includes("Riverside 2"), "the import opens as a new world, Riverside 2");
  await until(async () => (await text(".toast.is-shown")).includes("Imported Riverside 2"), "import toast");
  await until(async () => (await text(".bar__world")).includes("Saved"), "the imported world is saved at once");
  await page.waitForTimeout(250);
  await shot("import-done");

  // A refused file by drag-and-drop: a plain toast says why, and the world is unchanged.
  const held = tileSet(await play());
  await page.evaluate(() => {
    const dt = new DataTransfer();
    dt.items.add(new File([new Uint8Array([80, 75, 3, 4, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14])], "holiday.hexworld"));
    const fire = (type) => window.dispatchEvent(new DragEvent(type, { dataTransfer: dt, bubbles: true, cancelable: true }));
    fire("dragenter");
    fire("dragover");
  });
  await until(async () => page.locator(".sheet--worlds.is-drop").count(), "dragging a file shows the drop target");
  await shot("import-drop");
  await page.evaluate(() => {
    const dt = new DataTransfer();
    dt.items.add(new File([new Uint8Array([80, 75, 3, 4, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14])], "holiday.hexworld"));
    window.dispatchEvent(new DragEvent("drop", { dataTransfer: dt, bubbles: true, cancelable: true }));
  });
  await until(async () => (await text(".toast.is-shown")).includes("This isn't a Hex World file"), "a corrupt drop is refused with a toast saying why");
  check(tileSet(await play()) === held, "the refused file leaves the world unchanged");
  await page.waitForTimeout(250);
  await shot("import-refused");

  // A truncated file by the picker.
  const cut = `${work}/cut-short.hexworld`;
  writeFileSync(cut, bytes.subarray(0, bytes.length - 9));
  await page.locator(".sheet--worlds input[type=file]").setInputFiles(cut);
  await until(async () => (await text(".toast.is-shown")).includes("This file is cut short"), "a truncated file is refused with a toast");
  check(tileSet(await play()) === held, "the truncated file leaves the world unchanged");

  // Load the other slot, then delete the one left behind.
  await until(async () => page.locator(".slot-row[data-slot]").count(), "saved worlds listed");
  const riverside = page.locator(".slot-row", { hasText: "Riverside" }).filter({ hasNotText: "Riverside 2" });
  await riverside.locator("[data-act=load]").click();
  await until(async () => (await text(".bar__world")).includes("Riverside") && !(await text(".bar__world")).includes("Riverside 2"), "Load switches to Riverside");
  await until(async () => (await text(".toast.is-shown")).includes("Loaded Riverside"), "load toast");
  check(tileSet(await play()) === tileSet(exportedWorld), "the loaded slot's tiles match");
  const other = page.locator(".slot-row", { hasText: "Riverside 2" });
  await other.locator("[data-more]").click();
  await until(async () => page.locator(".menu.is-open").count(), "row menu opens");
  await page.waitForTimeout(150);
  await shot("row-menu");
  await page.locator(".menu [data-menu-act=delete]").click();
  await until(async () => (await text(".slot-row.is-open .confirm")).includes("Delete Riverside 2?"), "delete asks first");
  await page.waitForTimeout(250);
  await shot("delete-confirm");
  await page.locator(".slot-row.is-open [data-act=delete]").click();
  await until(async () => !(await page.locator(".slot-row", { hasText: "Riverside 2" }).count()), "the slot is deleted");
  // Narrow desktop (DESIGN.md, 1400 px rule): the bar and the wider Worlds sheet never meet.
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.waitForTimeout(400);
  const bar = await page.locator(".bar[data-region=bar]").boundingBox();
  const worlds = await page.locator(".sheet--worlds").boundingBox();
  check(bar.x + bar.width < worlds.x, `bar (right ${Math.round(bar.x + bar.width)}) clears the Worlds sheet (left ${Math.round(worlds.x)}) at 1280 px`);
  await shot("worlds-narrow-1280x720");
  // A long name stays one line and never pushes the bar into the sheet.
  await page.evaluate(() => {
    const b = document.querySelector(".bar__world b");
    if (b) b.textContent = "A very long world name that keeps on going";
  });
  const longBar = await page.locator(".bar[data-region=bar]").boundingBox();
  check(longBar.x + longBar.width < worlds.x, `a 40-character name still clears the sheet (bar right ${Math.round(longBar.x + longBar.width)})`);
  await page.evaluate(() => {
    const b = document.querySelector(".bar__world b");
    if (b) b.textContent = "Riverside";
  });
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.keyboard.press("Escape");
  await until(async () => !(await worldsOpen(page)), "Esc closes the Worlds sheet");
  await ctx.close();

  // ---------- Storage blocked: the game plays on and the bar says autosave is off ----------
  const blocked = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  await blocked.addInitScript(() => {
    Object.defineProperty(window, "indexedDB", {
      configurable: true,
      get() { throw new DOMException("The operation is insecure.", "SecurityError"); },
    });
  });
  const p2 = await open(blocked);
  const b = helpers(p2);
  await b.boot();
  await b.until(async () => (await b.text(".bar__world")).includes("Autosave off"), "storage blocked: the bar shows Autosave off");
  check(await p2.locator(".save-state[data-state=off]").count() === 1, "the off state carries the caution token");
  const caution = await p2.locator(".save-state[data-state=off]").evaluate((el) => getComputedStyle(el).color);
  check(caution === "rgb(244, 196, 92)", `caution colour on the bar (${caution})`);
  await b.until(async () => (await b.text(".toast.is-shown")).includes("Autosave is off in this window"), "storage-off toast offers Export");
  await p2.evaluate(() => { const h = window.hexWorld; h.selectTile("meadow"); h.placeTile(1, 0); });
  await b.until(async () => (await b.play()).tiles.length === 2, "play goes on with storage blocked");
  await p2.waitForTimeout(800);
  check((await b.text(".bar__world")).includes("Autosave off"), "still Autosave off after a placement");
  await b.shot("storage-off-play");
  await p2.locator(".bar__world").click();
  await b.until(async () => worldsOpen(p2), "Worlds sheet opens with storage blocked");
  await p2.waitForTimeout(300);
  const sheetText = await b.text(".sheet--worlds");
  check(sheetText.includes("· Autosave off") && !sheetText.includes("Not saved"), "the Worlds sheet reads Autosave off, as the bar does");
  await b.shot("storage-off-worlds");
  await blocked.close();
} catch (e) {
  fail(String(e?.stack ?? e));
} finally {
  if (errors.length) fail("console errors: " + errors.join(" | "));
  await browser.close();
  server.close();
}
console.log(failures ? `persist e2e FAILED (${failures})` : `persist e2e OK: shots in ${out}/`);
process.exitCode = failures ? 1 : 0;
