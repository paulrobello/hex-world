// Local-only Worlds sheet e2e (make ui-e2e): GPU Chromium on the built site, for the three save
// screens of design/ui/legend/saves.html. S10.3: Load over unsaved changes asks first, holds the
// autosave while it asks (for at most LOAD_HOLD_MS, then saves and asks only Load or Cancel), and
// each answer does what it says. S10.6: an imported file shows its preview, then opens as a new
// world or replaces the one on screen, and Undo on the Replace toast puts the replaced world's exact
// bytes back in its slot. S10.8: a zip, a damaged file
// and a file from a newer version each show the decoder's checks, and change nothing. Captures go to
// e2e/test-results/saves-ui/ (SAVES_OUT=docs/checkpoints/saves-ui refreshes the committed ones).
import { chromium } from "playwright";
import { createServer } from "node:http";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { extname, join } from "node:path";
import { BAD_TEXT } from "./console-errors.mjs";

const dist = process.argv[2] ?? "dist";
const out = process.env.SAVES_OUT ?? "e2e/test-results/saves-ui";
const work = "e2e/test-results/saves-ui-work";
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
try {
  const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 }, acceptDownloads: true });
  const page = await ctx.newPage();
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => { if (m.type() === "error" || BAD_TEXT.test(m.text())) errors.push(m.text()); });
  const shot = (name) => page.screenshot({ path: `${out}/${name}.jpg`, type: "jpeg", quality: 88 });
  const play = () => page.evaluate(() => window.hexWorld.state().play);
  const text = (sel) => page.locator(sel).first().innerText().catch(() => "");
  const tileSet = (p) => p.tiles.map((t) => `${t.q},${t.r},${t.tile},${t.rotation}`).sort().join(" ");
  async function until(fn, what, ms = 15_000) {
    const deadline = Date.now() + ms;
    for (;;) {
      const v = await fn();
      if (v) return v;
      if (Date.now() > deadline) { fail(`timed out: ${what}`); return v; }
      await page.waitForTimeout(50);
    }
  }
  const settleShot = async (name) => { await page.waitForTimeout(300); await shot(name); };
  const place = (pairs) => page.evaluate((pairs) => {
    const h = window.hexWorld;
    for (const [id, q, r] of pairs) { h.selectTile(id); h.placeTile(q, r); }
  }, pairs);
  const saved = () => until(async () => (await text(".bar__world")).includes("Saved"), "the world is saved");
  const pick = (file) => page.locator(".sheet--worlds input[type=file]").setInputFiles(file);
  // A slot as IndexedDB holds it, found by name: its id, summary tile count and saved bytes.
  const slot = (name) => page.evaluate(async (name) => {
    const db = await new Promise((res, rej) => {
      const r = indexedDB.open("hex-world");
      r.onsuccess = () => res(r.result);
      r.onerror = () => rej(r.error);
    });
    const get = (store, key) => new Promise((res, rej) => {
      const os = db.transaction(store).objectStore(store);
      const q = key === undefined ? os.getAll() : os.get(key);
      q.onsuccess = () => res(q.result);
      q.onerror = () => rej(q.error);
    });
    const metas = await get("slots");
    const m = metas.find((x) => x.name === name);
    const data = m ? await get("saves", m.id) : null;
    db.close();
    const bytes = data instanceof Uint8Array || data instanceof ArrayBuffer ? Array.from(new Uint8Array(data)) : null;
    return { id: m?.id, tiles: m?.tiles, bytes, why: `${metas.map((x) => x.name).join(",")} / ${Object.prototype.toString.call(data)}` };
  }, name);
  const checkStates = () => page.locator(".import [data-check]").evaluateAll((els) => els.map((e) => e.dataset.check).join(" "));

  await page.goto(base);
  await page.waitForFunction(() => window.hexWorld?.ready() && window.hexWorld.state().play, null, { timeout: 90_000 });
  await until(async () => !(await page.locator(".load").count()), "loading layer gone");
  await until(async () => (await text(".bar__world")).includes("World 1"), "start world named World 1");
  await page.evaluate(() => window.hexWorld.cmd({ cmd: "camera", pos: [40, 70, 90], look_at: [10, 0, 0] }));
  await place([["meadow", 1, 0], ["forest", -1, 0], ["road_straight", 0, 1]]);
  await until(async () => (await play()).tiles.length === 4, "three tiles placed");
  await saved();

  // Name it Riverside and export it: the file every import below starts from.
  await page.locator(".bar__world").click();
  await until(async () => page.locator(".sheet--worlds:not(.is-closed)").count(), "Worlds sheet opens");
  await page.locator(".current [data-act=rename]").click();
  await page.locator(".current .rename").fill("Riverside");
  await page.keyboard.press("Enter");
  await until(async () => (await text(".bar__world")).includes("Riverside"), "renamed Riverside");
  const [dl] = await Promise.all([page.waitForEvent("download"), page.locator(".current [data-act=export]").click()]);
  const exported = `${work}/Riverside.hexworld`;
  await dl.saveAs(exported);
  const good = readFileSync(exported);
  const riverside = await play();

  // ---------- S10.6 import preview ----------
  await page.evaluate(() => {
    const dt = new DataTransfer();
    dt.items.add(new File([new Uint8Array([1])], "x.hexworld"));
    window.dispatchEvent(new DragEvent("dragenter", { dataTransfer: dt, bubbles: true, cancelable: true }));
  });
  await until(async () => page.locator(".sheet--worlds.is-drop").count(), "a dragged file shows the drop target");
  check((await text(".drop small")).includes("You choose whether it replaces Riverside or opens as a new world."), "drop copy names the world on screen");
  await settleShot("s10-6-import-drop");
  await page.evaluate(() => window.dispatchEvent(new DragEvent("dragleave", { bubbles: true })));

  await pick(exported);
  await until(async () => page.locator(".import[data-import=preview]").count(), "a good file shows its preview");
  check((await play()).tiles.length === riverside.tiles.length, "the preview loads nothing");
  check((await text(".import h2")) === "Riverside 2", `the preview names the new world (${await text(".import h2")})`);
  const fileLine = await text(".import .import__file");
  check(/^Riverside\.hexworld · \d+ (B|KB) · format v3$/.test(fileLine), `file line: name, size, format version (${fileLine})`);
  const rows = (await text(".import .legend-rows")).replace(/\s+/g, " ");
  check(rows.includes("Grass") && rows.includes("Forest") && rows.includes("Road"), `legend rows key the file's kinds (${rows})`);
  check(rows.includes(`Tiles ${riverside.tiles.length}`), `legend counts the file's tiles (${rows})`);
  check(/Saved \d{1,2} \w{3} \d{4}/.test(rows), "legend dates the file");
  check(await page.locator(".import .mix i").count() >= 3, "the preview carries the file's mix strip");
  check(await page.evaluate(() => document.activeElement?.dataset?.focus) === "import-new", "focus moves to Open as a new world");
  check((await text(".import [data-act=import-replace]")) === "Replace Riverside", "Replace names the world on screen");
  await settleShot("s10-6-import-preview");
  await page.keyboard.press("Escape");
  await until(async () => !(await page.locator(".import").count()), "Esc cancels the preview");
  check(await page.locator(".sheet--worlds:not(.is-closed)").count() === 1, "the sheet stays open after Esc on the preview");

  await pick(exported);
  await until(async () => page.locator(".import[data-import=preview]").count(), "preview again");
  await page.locator(".import [data-act=import-new]").click();
  await until(async () => (await text(".bar__world")).includes("Riverside 2"), "Open as a new world: Riverside 2");
  check(tileSet(await play()) === tileSet(riverside), "the new world holds the file's tiles");
  await saved();

  // Replace: change Riverside 2, then replace it with the file again: same name, the file's tiles.
  await place([["meadow", 2, 0]]);
  await until(async () => (await play()).tiles.length === riverside.tiles.length + 1, "Riverside 2 changed");
  await saved();

  // Replace, then Undo on its toast: the slot gets back the exact bytes it held, same id and name.
  const changed = await play();
  // The bar can read Saved before the change's autosave is asked for, so wait on the slot itself.
  const before = await until(async () => {
    const s = await slot("Riverside 2");
    return s.tiles === changed.tiles.length && s.bytes?.length && s;
  }, "Riverside 2's change is in its slot");
  check(!!before?.bytes?.length, `Riverside 2's slot bytes read before Replace (${before?.bytes?.length} B)`);
  await pick(exported);
  await until(async () => page.locator(".import[data-import=preview]").count(), "preview for replace and undo");
  await page.locator(".import [data-act=import-replace]").click();
  await until(async () => (await play()).tiles.length === riverside.tiles.length, "Replace loads the file's tiles (to undo)");
  await until(async () => (await text(".toast.is-shown")).includes("Replaced Riverside 2"), "replace toast (to undo)");
  check((await text(".toast.is-shown .toast__action")) === "Undo", "the Replace toast offers Undo");
  const toastMotion = await page.locator("[data-region=toast]").evaluate((el) => getComputedStyle(el).transitionDuration);
  check(/[1-9]/.test(toastMotion), `the toast moves on the motion tokens (${toastMotion})`);
  await page.locator("[data-region=toast]").hover();
  const imported = await until(async () => {
    const s = await slot("Riverside 2");
    return s?.bytes?.join() !== before?.bytes?.join() && s;
  }, "the import's autosave writes over the slot");
  check(imported?.id === before?.id, "the import is written over the same slot");
  await saved();
  await settleShot("s10-6-replace-undo-toast");
  check(await page.locator(".toast.is-shown").count() === 1, "hovered, the toast stays up");
  await page.locator("[data-region=toast] .toast__action").click();
  await until(async () => tileSet(await play()) === tileSet(changed), "Undo brings back the replaced tiles");
  await until(async () => (await text(".toast.is-shown")).includes("Restored Riverside 2"), "the restored toast");
  check((await text(".bar__world")).includes("Riverside 2"), "Undo keeps the name");
  await saved();
  await page.waitForTimeout(1500);
  const restored = await slot("Riverside 2");
  check(restored?.id === before?.id, "Undo keeps the slot id");
  check(restored?.bytes?.join() === before?.bytes?.join(), `the slot's bytes are byte-identical to before Replace (${restored?.bytes?.length} B)`);
  check(restored?.tiles === changed.tiles.length, `the slot's summary is the replaced world's (${restored?.tiles} tiles)`);
  await page.mouse.move(720, 450);

  await pick(exported);
  await until(async () => page.locator(".import[data-import=preview]").count(), "preview for replace");
  await page.locator(".import [data-act=import-replace]").click();
  await until(async () => (await play()).tiles.length === riverside.tiles.length, "Replace loads the file's tiles");
  check((await text(".bar__world")).includes("Riverside 2"), "Replace keeps the world's name");
  await until(async () => (await text(".toast.is-shown")).includes("Replaced Riverside 2"), "replace toast");
  await saved();

  // ---------- S10.8 refusal checks ----------
  const refused = async (name, bytes, title, states, shotName, extra) => {
    const file = `${work}/${name}`;
    writeFileSync(file, bytes);
    const held = tileSet(await play());
    await pick(file);
    await until(async () => page.locator(".import[data-import=refused]").count(), `${name}: the checks panel`);
    check((await text(".import h2")) === title, `${name}: "${title}" (${await text(".import h2")})`);
    check((await checkStates()) === states, `${name}: checks ${states} (${await checkStates()})`);
    if (extra) await extra();
    check(tileSet(await play()) === held, `${name}: the world is unchanged`);
    check(await page.evaluate(() => document.activeElement?.dataset?.focus) === "import-again", `${name}: focus on Choose another file`);
    await settleShot(shotName);
  };
  const zip = Buffer.from([0x50, 0x4b, 3, 4, 20, 0, 0, 0, 8, 0, ...Array(40).fill(7)]);
  await refused("harbour-town.zip", zip, "This isn't a Hex World file", "ok bad skip skip skip", "s10-8-import-not-hexworld", async () => {
    check((await text(".import .checks")).includes("Header: expected HEXWORLD, found PK, which is a .zip archive"), "the zip is named");
    check((await text(".import .import__todo")) === "Unzip it first, then import the .hexworld file inside.", "the zip says to unzip");
  });
  const damaged = Buffer.from(good);
  damaged[Math.floor(damaged.length / 2)] ^= 0x10;
  await refused("damaged.hexworld", damaged, "This file is damaged", "ok ok ok bad skip", "s10-8-import-damaged", async () => {
    check((await text(".import .checks")).includes("Checksum: doesn't match the contents"), "the corrupted file fails its checksum");
  });
  const newer = Buffer.from(good);
  newer.writeUInt16LE(4, 8);
  await refused("harbour-town.hexworld", newer, "Made by a newer Hex World", "ok ok bad skip skip", "s10-8-import-newer-version", async () => {
    check((await text(".import .checks")).includes("Format v4. This version reads up to v3."), "the newer file names both versions");
    check((await text(".import .import__todo")).startsWith("Reload the page to get the newest Hex World"), "the newer file says to reload");
  });
  const [chooser] = await Promise.all([page.waitForEvent("filechooser"), page.locator(".import [data-act=import-again]").click()]);
  check(!!chooser, "Choose another file opens the picker");
  await chooser.setFiles(exported);
  await until(async () => page.locator(".import[data-import=preview]").count(), "the chosen file replaces the checks with its preview");
  await page.locator(".import [data-act=import-cancel]").click();
  await until(async () => !(await page.locator(".import").count()), "Cancel closes the panel");

  // Reduced motion: the panels cut in with no animation.
  await page.emulateMedia({ reducedMotion: "reduce" });
  await pick(`${work}/harbour-town.zip`);
  await until(async () => page.locator(".import[data-import=refused]").count(), "checks panel under reduced motion");
  const anim = await page.locator(".import").evaluate((el) => getComputedStyle(el).animationName);
  check(anim === "none", `reduced motion: the panel has no animation (${anim})`);
  const toastCut = await page.locator("[data-region=toast]").evaluate((el) => getComputedStyle(el).transitionDuration);
  check(/^0s(, 0s)*$/.test(toastCut), `reduced motion: the toast (Undo included) cuts in (${toastCut})`);
  await page.keyboard.press("Escape");
  await page.emulateMedia({ reducedMotion: "no-preference" });

  // ---------- S10.3 load confirm ----------
  // On Riverside 2 (the file's tiles). Make a change and click Load on Riverside while its autosave
  // still waits: all in the page, so no round trip eats the 600 ms debounce.
  const clickLoadWhileUnsaved = (q) => page.evaluate(async (q) => {
    const h = window.hexWorld;
    h.selectTile("meadow");
    h.placeTile(q, 0);
    const until = performance.now() + 3000;
    while (performance.now() < until) {
      await new Promise((r) => requestAnimationFrame(r));
      if (document.querySelector(".save-state[data-state=saving]")) break;
    }
    const rows = [...document.querySelectorAll(".slot-row")];
    const row = rows.find((r) => r.querySelector(".slot-row__text b")?.textContent === "Riverside");
    row?.querySelector("[data-act=load]")?.click();
    return !!row;
  }, q);
  check(await clickLoadWhileUnsaved(2), "Load clicked while the autosave waits");
  await until(async () => (await text(".slot-row.is-open .confirm")).includes("Load Riverside?"), "Load asks first over unsaved changes");
  const confirm = (await text(".slot-row.is-open .confirm")).replace(/\s+/g, " ");
  check(confirm.includes("Riverside 2 has changes from the last few seconds that aren't saved yet."), `confirm names what would be lost (${confirm})`);
  check(confirm.includes("Save Riverside 2, then load") && confirm.includes("Load without saving") && confirm.includes("Cancel"), "confirm offers the three answers");
  check(await page.evaluate(() => document.activeElement?.dataset?.focus) === "load-save", "focus on Save, then load");
  await page.keyboard.press("Tab");
  check(await page.evaluate(() => document.activeElement?.dataset?.focus) === "load-discard", "Tab walks to Load without saving");
  check(await page.locator(".sheet--worlds:not(.is-closed)").count() === 1, "Tab inside the sheet leaves it open");
  await page.keyboard.press("Shift+Tab");
  await page.waitForTimeout(1200);
  check((await text(".bar__world")).includes("Saving…"), "the autosave is held while the confirm is open");
  check(await page.locator(".slot-row.is-open .confirm").count() === 1, "the confirm stays while held");
  await settleShot("s10-3-load-confirm");
  // Esc cancels: nothing loads and the held autosave runs.
  await page.keyboard.press("Escape");
  await until(async () => !(await page.locator(".slot-row.is-open").count()), "Esc cancels the confirm");
  await saved();
  check((await text(".bar__world")).includes("Riverside 2"), "Cancel keeps the world on screen");
  const afterCancel = (await play()).tiles.length;

  // Load without saving: the change is dropped from Riverside 2's slot.
  check(await clickLoadWhileUnsaved(3), "Load clicked again while unsaved");
  await until(async () => page.locator(".slot-row.is-open [data-act=load-discard]").count(), "confirm again");
  await page.locator(".slot-row.is-open [data-act=load-discard]").click();
  await until(async () => (await text(".bar__world")).includes("Riverside") && !(await text(".bar__world")).includes("Riverside 2"), "Load without saving opens Riverside");
  check(tileSet(await play()) === tileSet(riverside), "Riverside's own tiles");
  await saved();
  await page.locator(".slot-row", { hasText: "Riverside 2" }).locator("[data-act=load]").click();
  await until(async () => (await text(".bar__world")).includes("Riverside 2"), "back to Riverside 2 (nothing unsaved, no confirm)");
  await until(async () => (await play()).tiles.length === afterCancel, `the dropped change is not in Riverside 2 (${afterCancel} tiles)`);
  await saved();

  // Save, then load: the change is kept.
  check(await clickLoadWhileUnsaved(3), "Load clicked a third time while unsaved");
  await until(async () => page.locator(".slot-row.is-open [data-act=load-save]").count(), "confirm a third time");
  await page.locator(".slot-row.is-open [data-act=load-save]").click();
  await until(async () => !(await text(".bar__world")).includes("Riverside 2"), "Save, then load opens Riverside");
  await saved();
  await page.locator(".slot-row", { hasText: "Riverside 2" }).locator("[data-act=load]").click();
  await until(async () => (await text(".bar__world")).includes("Riverside 2"), "back to Riverside 2 again");
  await until(async () => (await play()).tiles.length === afterCancel + 1, `the saved change is in Riverside 2 (${afterCancel + 1} tiles)`);
  await saved();

  // The hold has a limit (LOAD_HOLD_MS, 20 s): left open, the confirm's held autosave runs and the
  // confirm drops to a plain Load or Cancel.
  check(await clickLoadWhileUnsaved(4), "Load clicked and left open");
  await until(async () => page.locator(".slot-row.is-open [data-act=load-discard]").count(), "the three-answer confirm");
  const heldAt = Date.now();
  await until(async () => page.locator(".slot-row.is-open .confirm[data-confirm=load-saved]").count(), "the hold runs out", 40_000);
  const heldFor = Date.now() - heldAt;
  check(heldFor > 15_000 && heldFor < 30_000, `the hold lasted about 20 s (${heldFor} ms)`);
  check(await page.locator(".save-state").getAttribute("data-state") === "saved", "the held autosave ran: the bar's state is saved");
  const plain = (await text(".slot-row.is-open .confirm")).replace(/\s+/g, " ");
  check(plain.includes("Load Riverside?") && plain.includes("Your changes to Riverside 2 are saved."), `the confirm says the world is saved (${plain})`);
  const answers = await page.locator(".slot-row.is-open .confirm button").allInnerTexts();
  check(answers.join("|") === "Load|Cancel", `the confirm offers Load and Cancel only (${answers.join("|")})`);
  check(await page.evaluate(() => document.activeElement?.dataset?.focus) === "load-save", "focus stays on the first answer");
  await settleShot("s10-3-load-confirm-saved");
  await page.locator(".slot-row.is-open [data-act=load-save]").click();
  await until(async () => !(await text(".bar__world")).includes("Riverside 2"), "Load opens Riverside");
  await saved();
  await page.locator(".slot-row", { hasText: "Riverside 2" }).locator("[data-act=load]").click();
  await until(async () => (await text(".bar__world")).includes("Riverside 2"), "back to Riverside 2 once more");
  await until(async () => (await play()).tiles.length === afterCancel + 2, `the change the hold saved is in Riverside 2 (${afterCancel + 2} tiles)`);
  await ctx.close();
} catch (e) {
  fail(String(e?.stack ?? e));
} finally {
  if (errors.length) fail("console errors: " + errors.join(" | "));
  await browser.close();
  server.close();
}
console.log(failures ? `saves-ui e2e FAILED (${failures})` : `saves-ui e2e OK: shots in ${out}/`);
process.exitCode = failures ? 1 : 0;
