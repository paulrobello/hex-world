// Local-only persistence e2e (make ui-e2e): GPU Chromium on the built site. Autosave to IndexedDB
// and resume after a reload, rename, export, new world, import by picker (through its preview), a
// refused file by drag-and-drop and by picker (its checks panel), load and delete a slot, and the storage-blocked bar. Asserts through
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
    // A quality change holds the input under its layer until the engine reports the new tier drawn.
    const drawn = (what) => until(async () => !(await st()).tier.rebuilding
      && (await page.locator(".rebuild:not([hidden])").count()) === 0, `${what}: drawn and the layer clears`, 30_000);
    await until(async () => (await text(".bar__world")).includes("Saved"), "settled before the settings change");
    await page.locator(".tool[data-action=settings]").click();
    await until(async () => page.locator(".sheet--settings:not(.is-closed)").count(), "Settings opens");
    const detected = (await st()).tier;
    await page.locator(".tier[data-tier=low]").click();
    await until(async () => { const t = (await st()).tier; return t.tier === "low" && t.source === "runtime"; }, "Low picked");
    await drawn("Low");
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
    await drawn("the resumed save's Low");
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
    await drawn("Auto");
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

  // Import by the picker: the preview first (S10.6), then the tiles match the export.
  await page.locator(".sheet--worlds input[type=file]").setInputFiles(exported);
  await until(async () => page.locator(".import[data-import=preview]").count(), "the import shows its preview first");
  await page.locator(".import [data-act=import-new]").click();
  await until(async () => (await play()).tiles.length === exportedWorld.tiles.length, "import loads the exported world");
  check(tileSet(await play()) === tileSet(exportedWorld), "imported tiles match the exported world");
  await until(async () => (await text(".bar__world")).includes("Riverside 2"), "the import opens as a new world, Riverside 2");
  await until(async () => (await text(".toast.is-shown")).includes("Imported Riverside 2"), "import toast");
  await until(async () => (await text(".bar__world")).includes("Saved"), "the imported world is saved at once");
  await page.waitForTimeout(250);
  await shot("import-done");

  // A refused file by drag-and-drop: the checks panel says why (S10.8), and the world is unchanged.
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
  await until(async () => (await text(".import[data-import=refused] h2")).includes("This isn't a Hex World file"), "a corrupt drop is refused with the checks saying why");
  check(tileSet(await play()) === held, "the refused file leaves the world unchanged");
  await page.waitForTimeout(250);
  await shot("import-refused");

  // A truncated file by the picker.
  const cut = `${work}/cut-short.hexworld`;
  writeFileSync(cut, bytes.subarray(0, bytes.length - 9));
  await page.locator(".sheet--worlds input[type=file]").setInputFiles(cut);
  await until(async () => (await text(".import[data-import=refused] h2")).includes("This file is cut short"), "a truncated file is refused with the checks");
  check(tileSet(await play()) === held, "the truncated file leaves the world unchanged");
  await page.locator(".import [data-act=import-cancel]").click();

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

  // Start over (Settings, World tab; owner request 2026-10-14): Cancel and Esc leave the world
  // alone; confirming clears it in place to the start tile with no history, under the same name
  // and slot, and the cleared world is what a reload resumes.
  {
    const slotsInDb = () => page.evaluate(() => new Promise((resolve, reject) => {
      const req = indexedDB.open("hex-world");
      req.onerror = () => reject(req.error);
      req.onsuccess = () => {
        const all = req.result.transaction("slots").objectStore("slots").getAll();
        all.onerror = () => reject(all.error);
        all.onsuccess = () => {
          resolve(all.result.map((m) => ({ id: m.id, name: m.name, tiles: m.tiles })));
          req.result.close();
        };
      };
    }));
    const confirmShown = () => page.locator("#settings-world .confirm:not([hidden])").count();
    const focused = () => page.evaluate(() => document.activeElement?.dataset?.act ?? null);
    const n0 = (await play()).tiles.length;
    await page.evaluate(() => {
      const h = window.hexWorld;
      h.selectTile("meadow"); h.placeTile(4, 0);
      h.selectTile("meadow"); h.placeTile(5, 0);
      h.selectTile("forest"); h.placeTile(-2, 0);
    });
    await until(async () => (await play()).tiles.length === n0 + 3, "three more tiles placed before Start over");
    const held = await play();
    await until(async () => (await slotsInDb()).find((m) => m.name === "Riverside")?.tiles === held.tiles.length, `Riverside's slot is autosaved with ${held.tiles.length} tiles`);
    await until(async () => (await text(".bar__world")).includes("Saved"), "settled before Start over");
    const slots0 = await slotsInDb();
    const slot = slots0.find((m) => m.name === "Riverside");
    check(!!slot && held.can_undo, `Riverside's slot holds ${held.tiles.length} tiles, with history`);

    await page.locator(".tool[data-action=settings]").click();
    await until(async () => page.locator(".sheet--settings:not(.is-closed)").count(), "Settings opens");
    await page.locator("#settings-tab-world").click();
    await until(async () => page.locator("#settings-world:not([hidden])").count(), "the World tab shows");
    check((await text("#settings-world .field:has([data-act=start-over]) .field__label")) === "Riverside", "the World tab names the current world");
    await page.mouse.move(700, 450);
    await page.evaluate(() => document.activeElement?.blur());
    await page.waitForTimeout(250);
    await shot("settings-world");

    await page.locator("#settings-world [data-act=start-over]").click();
    await until(confirmShown, "Start over asks first");
    const ask = (await text("#settings-world .confirm")).replace(/\s+/g, " ");
    check(ask.includes("Start over?") && ask.includes("This clears every tile in Riverside. You can't undo it."), `the confirm names the world ("${ask}")`);
    check((await focused()) === "start-over-cancel", "focus moves to Cancel");
    await page.mouse.move(700, 450);
    await page.waitForTimeout(250);
    await shot("settings-world-confirm");
    await page.locator("#settings-world [data-act=start-over-cancel]").click();
    await until(async () => !(await confirmShown()), "Cancel closes the confirm");
    check((await focused()) === "start-over", "focus returns to Start over");
    await page.waitForTimeout(300);
    const kept = await play();
    check(tileSet(kept) === tileSet(held) && kept.can_undo === held.can_undo && kept.can_redo === held.can_redo, "Cancel leaves the world and its history unchanged");

    // Keyboard: Enter on Start over asks, Esc closes the confirm first and leaves Settings open.
    await page.keyboard.press("Enter");
    await until(confirmShown, "Enter on Start over asks");
    await page.keyboard.press("Escape");
    await until(async () => !(await confirmShown()), "Esc closes the confirm first");
    check(await page.locator(".sheet--settings:not(.is-closed)").count() === 1, "and leaves Settings open");
    check((await focused()) === "start-over", "focus is back on Start over");
    check(tileSet(await play()) === tileSet(held), "Esc leaves the world unchanged");

    await page.locator("#settings-world [data-act=start-over]").click();
    await until(confirmShown, "Start over asks again");
    await page.locator("#settings-world [data-act=start-over-confirm]").click();
    await until(async () => (await play()).tiles.length === 1, "confirming clears the world to the start tile");
    const cleared = await play();
    check(cleared.tiles[0]?.q === 0 && cleared.tiles[0]?.r === 0, "the one tile left is the start tile at the origin");
    check(!cleared.can_undo && !cleared.can_redo, "undo and redo history are empty");
    check(!(await confirmShown()), "the confirm closes");
    await until(async () => (await text(".toast.is-shown")).includes("Started Riverside over"), "a toast says the world started over");
    await until(async () => { const b = (await text(".bar__world")).replace(/\s+/g, " "); return b.includes("Riverside") && b.includes("1 tile"); }, "the bar keeps the name, now 1 tile");
    await until(async () => (await text(".bar__world")).includes("Saved"), "the cleared world is autosaved");
    await until(async () => (await slotsInDb()).find((m) => m.id === slot?.id)?.tiles === 1, "the same slot now holds the cleared world");
    const slots1 = await slotsInDb();
    check(slots1.length === slots0.length, `no new slot (${slots0.length} before, ${slots1.length} after)`);
    check(slots1.find((m) => m.id === slot?.id)?.name === "Riverside", "the slot keeps its name");

    await page.reload();
    await page.waitForFunction(() => window.hexWorld?.ready() && window.hexWorld.state().play, null, { timeout: 90_000 });
    await until(async () => (await text(".bar__world")).includes("Riverside"), "the reload resumes Riverside");
    await until(async () => (await play()).tiles.length === 1, "the reload keeps the cleared world (1 tile)");
    const back = await play();
    check(!back.can_undo && !back.can_redo, "and its empty history");

    // Narrow desktop: the tab and its confirm fit inside the sheet and the window at 1280 × 720.
    await page.setViewportSize({ width: 1280, height: 720 });
    await page.locator(".tool[data-action=settings]").click();
    await until(async () => page.locator(".sheet--settings:not(.is-closed)").count(), "Settings opens at 1280 × 720");
    check((await page.evaluate(() => document.activeElement?.id)) === "settings-tab-graphics", "focus lands on the Graphics tab after the reload");
    // End selects the last tab (Debug); one step left is World.
    await page.keyboard.press("End");
    await until(async () => page.locator("#settings-tab-debug[aria-selected=true]").count(), "End on the tab strip selects the last tab, Debug");
    await page.keyboard.press("ArrowLeft");
    await until(async () => page.locator("#settings-tab-world[aria-selected=true]").count(), "ArrowLeft from Debug selects the World tab");
    check((await page.evaluate(() => document.activeElement?.id)) === "settings-tab-world", "focus follows to the World tab");
    await page.keyboard.press("Tab");
    check((await page.evaluate(() => document.activeElement?.getAttribute("aria-label"))) === "Walk speed", "Tab reaches the Walk speed slider first");
    await page.keyboard.press("Tab");
    check((await focused()) === "start-over", "Tab reaches Start over");
    await page.keyboard.press("Enter");
    await until(confirmShown, "the confirm opens at 1280 × 720");
    await page.waitForTimeout(300);
    const sheetBox = await page.locator(".sheet--settings").boundingBox();
    for (const act of ["start-over-confirm", "start-over-cancel"]) {
      const b = await page.locator(`#settings-world [data-act=${act}]`).boundingBox();
      check(!!b && b.x >= sheetBox.x && b.x + b.width <= sheetBox.x + sheetBox.width && b.y + b.height <= Math.min(720, sheetBox.y + sheetBox.height),
        `${act} sits inside the sheet and the window at 1280 × 720`);
    }
    await page.mouse.move(400, 450);
    await shot("settings-world-confirm-1280x720");
    await page.keyboard.press("Escape");
    await until(async () => !(await confirmShown()), "Esc closes the confirm at 1280 × 720");
    await page.keyboard.press("Escape");
    await until(async () => page.locator(".sheet--settings.is-closed").count(), "a second Esc closes Settings");
    check((await play()).tiles.length === 1, "the cleared world is untouched by the cancelled confirm");
    await page.setViewportSize({ width: 1440, height: 900 });
  }
  await ctx.close();

  // ---------- The viewer's camera per world (owner request 2026-10-14) ----------
  // A camera move with no edit, walk mode, and two worlds each keep their own camera across a reload
  // or a switch, from localStorage (hexWorld.camera) rather than the world save; camera moves add no
  // history and no autosave. A fresh context, so IndexedDB and localStorage start empty.
  {
    const cctx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
    const cp = await open(cctx);
    const c = helpers(cp);
    const st = () => cp.evaluate(() => window.hexWorld.state());
    const orbit = async () => (await c.play()).orbit;
    const poses = () => cp.evaluate(() => JSON.parse(localStorage.getItem("hexWorld.camera") ?? "{}"));
    const slotStamps = () => cp.evaluate(() => new Promise((resolve, reject) => {
      const req = indexedDB.open("hex-world");
      req.onerror = () => reject(req.error);
      req.onsuccess = () => {
        const all = req.result.transaction("slots").objectStore("slots").getAll();
        all.onerror = () => reject(all.error);
        all.onsuccess = () => {
          resolve(Object.fromEntries(all.result.map((m) => [m.name, m.updatedAt])));
          req.result.close();
        };
      };
    }));
    const wrap = (a, b) => { const d = Math.abs(a - b) % (2 * Math.PI); return Math.min(d, 2 * Math.PI - d); };
    const sameOrbit = (a, b) => !!a && !!b && a.focus.every((v, i) => Math.abs(v - b.focus[i]) < 0.02)
      && wrap(a.yaw, b.yaw) < 1e-3 && Math.abs(a.pitch - b.pitch) < 1e-3 && Math.abs(a.distance - b.distance) < 0.05;
    const fmt = (o) => o ? `focus ${o.focus.map((v) => v.toFixed(2))} yaw ${o.yaw.toFixed(4)} pitch ${o.pitch.toFixed(4)} zoom ${o.distance.toFixed(2)}` : "none";
    const reload = async () => {
      await cp.reload();
      await cp.waitForFunction(() => window.hexWorld?.ready() && window.hexWorld.state().play, null, { timeout: 90_000 });
    };
    // Sets the orbit camera through the bridge (pos on the orbit, looking at its focus).
    const fly = async (focus, yaw, pitch, distance) => {
      await cp.evaluate(([f, y, p, d]) => window.hexWorld.cmd({
        cmd: "camera",
        pos: [f[0] + Math.cos(p) * Math.sin(y) * d, f[1] + Math.sin(p) * d, f[2] + Math.cos(p) * Math.cos(y) * d],
        look_at: f,
      }), [focus, yaw, pitch, distance]);
      const want = { focus, yaw, pitch, distance };
      await c.until(async () => sameOrbit(await orbit(), want), `the camera reaches ${fmt(want)}`);
      return orbit();
    };

    await c.boot();
    await cp.evaluate(() => { const h = window.hexWorld; h.selectTile("meadow"); h.placeTile(1, 0); });
    await c.until(async () => (await c.play()).tiles.length === 2, "camera: a tile placed so World 1 has a slot");
    await c.until(async () => (await slotStamps())["World 1"], "camera: World 1's slot is written");
    await c.until(async () => (await c.text(".bar__world")).includes("Saved"), "camera: World 1 is autosaved");
    const hist = await c.play();
    const stamps = await slotStamps();

    // An orbit move with no edit survives a reload.
    const a = await fly([18, 0, -6], -2.2, 0.62, 95);
    await cp.waitForTimeout(3000);
    const after = await c.play();
    check(after.can_undo === hist.can_undo && after.can_redo === hist.can_redo, "a camera move adds no undo or redo entry");
    check((await c.text(".bar__world")).includes("Saved"), "a camera move starts no autosave");
    check(JSON.stringify(await slotStamps()) === JSON.stringify(stamps), "a camera move writes no world save");
    const stored = Object.values(await poses());
    check(stored.length === 1 && sameOrbit(stored[0].orbit, a) && stored[0].walk === null, `the pose is stored per viewer within 3 s (${fmt(stored[0]?.orbit)})`);
    await reload();
    await c.until(async () => sameOrbit(await orbit(), a), `after a reload the orbit camera is back (${fmt(a)})`, 20_000);
    const restored = (want) => c.until(async () => (await cp.evaluate(() => window.hexWorld.cameraRestore())).outcome === want, `hexWorld.cameraRestore() reports ${want}`, 20_000);
    await restored("orbit");
    console.log(`  orbit after reload: ${fmt(await orbit())}`);
    await cp.waitForTimeout(500);
    await c.shot("camera-orbit-after-reload");

    // Walk mode: the walker's feet, heading and pitch survive a reload, behind the click-to-look hint.
    await cp.evaluate(() => window.hexWorld.cmd({ cmd: "walk", on: true, pointer_lock: null }));
    await c.until(async () => (await st()).walk?.on, "walk mode on");
    await cp.evaluate(() => {
      const h = window.hexWorld;
      h.cmd({ cmd: "walk_to", x: 30, z: 4, heading: 250, pitch: -7 });
      h.cmd({ cmd: "walk_step", forward: 1, right: 0.3, seconds: 1.2, jog: false });
    });
    await c.until(async () => { const w = (await st()).walk; return w?.on && w.distance > 0.5 && w.step_left === null; }, "the walker walks somewhere");
    await cp.evaluate(() => window.hexWorld.cmd({ cmd: "walk_look", heading: 237.5, pitch: 4.25 }));
    await c.until(async () => Math.abs((await st()).walk.heading - 237.5) < 1e-2, "and looks around");
    const walked = (await st()).walk;
    await cp.waitForTimeout(3000);
    const wpose = Object.values(await poses())[0];
    check(!!wpose?.walk && sameOrbit(wpose.orbit, a), `the stored pose holds the walker over the orbit camera (${JSON.stringify(wpose?.walk)})`);
    await reload();
    const near = (w) => !!w?.on && w.feet && Math.hypot(w.feet[0] - walked.feet[0], w.feet[2] - walked.feet[2]) < 0.05;
    await c.until(async () => near((await st()).walk), `after a reload walk mode resumes at the feet [${walked.feet}]`, 25_000);
    await restored("walk");
    const back = (await st()).walk;
    check(Math.abs(back.heading - walked.heading) < 0.05 && Math.abs(back.pitch - walked.pitch) < 0.05,
      `heading and pitch restored (${back.heading}, ${back.pitch}; were ${walked.heading}, ${walked.pitch})`);
    check(!back.pointer_lock, "no pointer lock without a gesture");
    await c.until(async () => (await cp.locator(".walk-ui:not(.is-off)").count()) === 1, "the walk hint shows");
    check((await c.text(".walk-hint")).includes("Click to look around"), "the hint says Click to look around");
    await cp.waitForTimeout(500);
    await c.shot("camera-walk-after-reload");
    await cp.evaluate(() => window.hexWorld.cmd({ cmd: "walk", on: false, pointer_lock: null }));
    await c.until(async () => !(await st()).walk?.on, "walk mode off");
    await c.until(async () => sameOrbit(await orbit(), a), "leaving walk mode returns to the saved orbit camera");

    // Two worlds: each keeps its own camera across a switch.
    const one = await fly([-12, 0, 20], 0.9, 1.1, 210);
    await cp.locator(".bar__world").click();
    await c.until(async () => worldsOpen(cp), "camera: the Worlds sheet opens");
    await cp.locator(".sheet--worlds [data-act=new]").click();
    await c.until(async () => (await c.play()).tiles.length === 1 && (await c.text(".bar__world")).includes("World 2"), "camera: New world opens World 2");
    await cp.keyboard.press("Escape");
    await c.until(async () => !(await worldsOpen(cp)), "camera: Esc closes the Worlds sheet");
    await cp.evaluate(() => { const h = window.hexWorld; h.selectTile("forest"); h.placeTile(0, 1); });
    await c.until(async () => (await c.text(".bar__world")).replace(/\s+/g, " ").includes("World 2 2 tiles · Saved"), "camera: World 2 is autosaved");
    const two = await fly([25, 0, 30], -0.4, 0.35, 60);
    await cp.waitForTimeout(2500);
    check(Object.keys(await poses()).length === 2, "one stored pose per world");
    const loadWorld = async (name, other) => {
      await cp.locator(".bar__world").click();
      await c.until(async () => worldsOpen(cp), `the Worlds sheet opens to load ${name}`);
      await c.until(async () => cp.locator(".slot-row", { hasText: name }).count(), `${name} is listed`);
      await cp.locator(".slot-row", { hasText: name }).locator("[data-act=load]").click();
      await c.until(async () => { const b = await c.text(".bar__world"); return b.includes(name) && !b.includes(other); }, `Load switches to ${name}`);
      await cp.keyboard.press("Escape");
    };
    await loadWorld("World 1", "World 2");
    await c.until(async () => sameOrbit(await orbit(), one), `World 1 comes back with its own camera (${fmt(one)})`, 20_000);
    await loadWorld("World 2", "World 1");
    await c.until(async () => sameOrbit(await orbit(), two), `World 2 comes back with its own camera (${fmt(two)})`, 20_000);
    await cp.waitForTimeout(400);
    await c.shot("camera-second-world");

    // A stored walker on a tile that is no longer there (bulldozed, off the map) is not restored:
    // the camera stays in orbit at the saved focus. World 2 is the most recent, so a reload resumes it.
    const twoId = Object.entries(await poses()).find(([, p]) => sameOrbit(p.orbit, two))?.[0];
    check(!!twoId, "World 2's pose is stored");
    await cp.evaluate((id) => {
      const all = JSON.parse(localStorage.getItem("hexWorld.camera") ?? "{}");
      all[id].walk = { feet: [400, 0, 400], heading: 90, pitch: 0, cell: { q: 12, r: 6 } };
      localStorage.setItem("hexWorld.camera", JSON.stringify(all));
    }, twoId);
    await reload();
    await c.until(async () => (await c.text(".bar__world")).includes("World 2"), "the reload resumes World 2");
    await c.until(async () => sameOrbit(await orbit(), two), "a walker on a missing tile falls back to the saved orbit", 20_000);
    await restored("fallback");
    await cp.waitForTimeout(3000);
    check(!(await st()).walk?.on, "walk mode stays off");
    check((await poses())[twoId]?.walk === null, "and the orbit pose replaces the stale walker");
    await cctx.close();
  }

  // A blocked localStorage: the game loads and resumes its world with no stored camera.
  {
    const lctx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
    await lctx.addInitScript(() => {
      Object.defineProperty(window, "localStorage", {
        configurable: true,
        get() { throw new DOMException("The operation is insecure.", "SecurityError"); },
      });
    });
    const lp = await open(lctx);
    const l = helpers(lp);
    await l.boot();
    await lp.evaluate(() => { const h = window.hexWorld; h.selectTile("meadow"); h.placeTile(1, 0); });
    await l.until(async () => (await l.text(".bar__world")).includes("Saved"), "localStorage blocked: the world autosaves");
    await lp.evaluate(() => window.hexWorld.cmd({ cmd: "camera", pos: [60, 80, 60], look_at: [10, 0, 0] }));
    await lp.waitForTimeout(2500);
    await lp.reload();
    await lp.waitForFunction(() => window.hexWorld?.ready() && window.hexWorld.state().play, null, { timeout: 90_000 });
    await l.until(async () => (await l.play()).tiles.length === 2, "localStorage blocked: the game loads and resumes its world");
    check((await l.text(".bar__world")).includes("World 1"), "localStorage blocked: the HUD runs");
    await lctx.close();
  }

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
