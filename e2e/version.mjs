// Local-only new-version e2e (make ui-e2e; owner request 2026-10-15): GPU Chromium on the built site,
// served from memory so the "deploy" never touches dist/. The server can swap version.json and the
// build id inlined in index.html to a new id (a new deploy), or answer version.json with a 503.
// Checks the poll (no-store fetch, a fresh ?t= per request, version_poll), silence offline and on a
// failed request, the update note's place (clear of the bar and the sheet, hidden in photo mode),
// that Reload writes a just-placed tile and loads the new build, that a visibility change checks at
// the default interval, and the plain unsaved wording with storage blocked. Captures go to
// e2e/test-results/version/ (VERSION_OUT=docs/checkpoints/version-check refreshes the committed ones).
import { chromium } from "playwright";
import { createServer } from "node:http";
import { mkdirSync, readFileSync } from "node:fs";
import { extname, join } from "node:path";
import { BAD_TEXT } from "./console-errors.mjs";
import { assertCsp } from "../scripts/web/csp-check.mjs";
import { hashInlineScripts } from "../scripts/web/version-dist.mjs";

const dist = process.argv[2] ?? "dist";
const out = process.env.VERSION_OUT ?? "e2e/test-results/version";
mkdirSync(out, { recursive: true });
const types = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm", ".css": "text/css",
  ".woff2": "font/woff2", ".ktx2": "image/ktx2", ".glb": "model/gltf-binary", ".png": "image/png",
  ".webp": "image/webp", ".json": "application/json" };

const built = JSON.parse(readFileSync(join(dist, "version.json"), "utf8")).id;
const page0 = readFileSync(join(dist, "index.html"), "utf8");
if (!page0.includes(`window.__HEX_BUILD="${built}"`)) throw new Error(`index.html does not inline build ${built}`);
assertCsp(page0, join(dist, "index.html"));
/** index.html as build `id`. The build id lives in the hex-hashes inline script, so a new id changes that
 * script's bytes and its CSP hash must be recomputed, as version-dist does for a real deploy. */
const pages = new Map();
function pageAs(id) {
  if (!pages.has(id)) {
    const page = id === built ? page0 : hashInlineScripts(page0.replace(`window.__HEX_BUILD="${built}"`, `window.__HEX_BUILD="${id}"`));
    assertCsp(page, `index.html as build ${id}`);
    pages.set(id, page);
  }
  return pages.get(id);
}
/** The deployed build: null serves dist as built; an id serves it as that id; "fail" answers version.json 503. */
let live = null;
const polls = [];
const server = createServer((req, res) => {
  const url = new URL(req.url, "http://x");
  let path = decodeURIComponent(url.pathname);
  if (path.endsWith("/")) path += "index.html";
  const id = live && live !== "fail" ? live : built;
  if (path === "/version.json") {
    polls.push({ at: Date.now(), search: url.search, cache: req.headers["cache-control"] ?? "" });
    if (live === "fail") { res.writeHead(503); res.end("down"); return; }
    res.writeHead(200, { "content-type": "application/json", "cache-control": "max-age=600" });
    res.end(`${JSON.stringify({ id })}\n`);
    return;
  }
  if (path === "/index.html") {
    res.writeHead(200, { "content-type": "text/html" });
    res.end(pageAs(id));
    return;
  }
  try {
    const body = readFileSync(join(dist, path));
    res.writeHead(200, { "content-type": types[extname(path)] ?? "application/octet-stream" });
    res.end(body);
  } catch { res.writeHead(404); res.end(); }
}).listen(0);
const origin = `http://127.0.0.1:${server.address().port}/`;
const query = "?scenario=play&tilt=off&fps=off";

const gpuArgs = process.env.PW_GPU === "swiftshader"
  ? ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--enable-unsafe-webgpu"]
  : ["--use-angle=metal", "--enable-gpu", "--ignore-gpu-blocklist", "--enable-unsafe-webgpu"];
let failures = 0;
const fail = (m) => { console.error("FAIL:", m); failures++; };
const check = (ok, m) => { if (!ok) fail(m); else console.log("ok:", m); };

const browser = await chromium.launch({ channel: "chromium", headless: true, args: gpuArgs });
const errors = [];
/** Chromium's own network log line for a failed version.json request (not page code). */
const netLog = [];

async function open(context) {
  const page = await context.newPage();
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    const t = m.text();
    if (/Failed to load resource/.test(t) && /version\.json|503/.test(`${t} ${m.location().url}`)) netLog.push(t);
    else if (m.type() === "error" || BAD_TEXT.test(t)) errors.push(t);
  });
  const until = async (fn, what, ms = 15_000) => {
    const deadline = Date.now() + ms;
    for (;;) {
      const v = await fn();
      if (v) return v;
      if (Date.now() > deadline) { fail(`timed out: ${what}`); return v; }
      await page.waitForTimeout(50);
    }
  };
  const boot = async (q) => {
    await page.goto(origin + q);
    await page.waitForFunction(() => window.hexWorld?.ready() && window.hexWorld.state().play, null, { timeout: 90_000 });
    await until(async () => !(await page.locator(".load").count()), "loading layer gone");
  };
  const note = page.locator(".hud > .toast--update");
  const shown = async () => (await note.count()) > 0 && (await note.evaluate((el) => el.classList.contains("is-shown") && getComputedStyle(el).opacity === "1"));
  return { page, until, boot, note, shown };
}

const overlap = (a, b) => a && b && a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;

try {
  // ---------- Storage on: poll, silence, the note, photo mode, Reload ----------
  const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const a = await open(ctx);
  const { page } = a;
  await a.boot(`${query}&version_poll=1`);
  check(await page.evaluate(() => window.__HEX_BUILD) === built, `the page carries build ${built}`);
  const n0 = polls.length;
  await page.waitForTimeout(3_500);
  const first = polls.slice(n0);
  check(first.length >= 2, `version_poll=1 polls about once a second (${first.length} in 3.5 s)`);
  check(new Set(first.map((p) => p.search)).size === first.length && first.every((p) => /^\?t=\d+$/.test(p.search)),
    `each poll has its own ?t= query (${first.map((p) => p.search).slice(0, 3).join(" ")})`);
  check(first.every((p) => /no-cache|no-store/.test(p.cache)), `polls bypass the HTTP cache (cache-control "${first[0]?.cache}")`);
  check(!(await a.note.count()), "the same build shows no update note");

  await ctx.setOffline(true);
  check(await page.evaluate(() => navigator.onLine) === false, "offline emulated");
  const n1 = polls.length;
  live = "beefbeef01";
  await page.waitForTimeout(3_000);
  await ctx.setOffline(false);
  live = null;
  check(polls.length === n1, `offline: no poll requests (${polls.length - n1})`);
  check(!(await a.note.count()), "offline: no update note");

  live = "fail";
  const n2 = polls.length;
  const e2 = errors.length;
  await page.waitForTimeout(3_000);
  check(polls.length > n2, `a 503 is retried (${polls.length - n2} requests)`);
  check(errors.length === e2, `a 503 raises no page error or console error from page code (${errors.slice(e2).join(" | ")})`);
  check(!(await a.note.count()), "a 503 shows no update note");
  console.log(`503 phase: Chromium's own network log lines: ${netLog.length ? netLog.slice(0, 2).join(" | ") : "none"}`);

  // A tile placed just before the deploy, and another just before Reload (its autosave still pending).
  await page.evaluate(() => { const h = window.hexWorld; h.selectTile("meadow"); h.placeTile(1, 0); });
  await a.until(async () => (await page.evaluate(() => window.hexWorld.state().play.tiles.length)) === 2, "a tile placed");
  const NEW = "0123abcdef";
  live = NEW;
  await a.until(a.shown, "a newer build shows the update note", 6_000);
  const text = (await a.note.innerText()).replace(/\s+/g, " ");
  check(/New version available/.test(text) && /saved before the page reloads/.test(text) && /Reload/.test(text), `note text: "${text}"`);
  check(!(await a.note.evaluate((el) => el.classList.contains("toast--caution"))), "storage on: no caution tone");
  const box = await a.note.boundingBox();
  const bar = await page.locator(".hud > .bar").first().boundingBox();
  const sheet = await page.locator(".hud > .sheet:not(.is-closed)").first().boundingBox().catch(() => null);
  check(box && !overlap(box, bar), `the note is clear of the tool bar (${JSON.stringify(box)} vs ${JSON.stringify(bar)})`);
  check(!sheet || !overlap(box, sheet), "the note is clear of the open sheet");
  check(Math.round(900 - box.y - box.height) === 84, `the note sits 84 px from the bottom (${900 - box.y - box.height})`);
  await page.waitForTimeout(400);
  await page.screenshot({ path: `${out}/update-note.jpg`, type: "jpeg", quality: 88 });

  // Photo mode owns the screen: the note hides with the HUD, and comes back after.
  await page.evaluate(() => window.hexWorld.cmd({ cmd: "toggle", what: "photo" }));
  await a.until(async () => (await page.evaluate(() => window.hexWorld.state().play.photo_mode)), "photo mode on");
  await a.until(async () => (await a.note.evaluate((el) => getComputedStyle(el).opacity)) === "0", "the note hides in photo mode");
  const pbar = await page.locator(".photo-ui .pbar").first().boundingBox();
  const nb = await a.note.boundingBox();
  check(await a.note.evaluate((el) => getComputedStyle(el).pointerEvents) === "none", "the hidden note takes no clicks in photo mode");
  check(!overlap(nb, pbar), "the note does not sit on the photo bar");
  await page.screenshot({ path: `${out}/photo-mode.jpg`, type: "jpeg", quality: 88 });
  await page.evaluate(() => window.hexWorld.cmd({ cmd: "toggle", what: "photo" }));
  await a.until(async () => !(await page.evaluate(() => window.hexWorld.state().play.photo_mode)), "photo mode off");
  await a.until(a.shown, "the note returns after photo mode");

  // The save state as the page hides: "saved" means Reload's own write finished before it navigated
  // (the page-hide flush alone would leave it "saving").
  await page.evaluate(() => addEventListener("pagehide", () =>
    sessionStorage.setItem("saveAtHide", document.querySelector(".save-state")?.dataset.state ?? "none")));
  await page.evaluate(() => { const h = window.hexWorld; h.selectTile("forest"); h.placeTile(-1, 0); });
  await a.until(async () => (await page.evaluate(() => window.hexWorld.state().play.tiles.length)) === 3, "a third tile placed");
  const atClick = await page.locator(".save-state").evaluate((el) => el.dataset.state);
  check(atClick === "saving", `Reload is clicked while the third tile's autosave is pending (${atClick})`);
  const tilesBefore = await page.evaluate(() => window.hexWorld.state().play.tiles.map((t) => `${t.q},${t.r},${t.tile}`).sort().join(" "));
  const loaded = page.waitForEvent("load", { timeout: 30_000 });
  await a.note.locator(".toast__action").click();
  await loaded;
  const atHide = await page.evaluate(() => sessionStorage.getItem("saveAtHide"));
  check(atHide === "saved", `Reload wrote the world before the page went away (save state at page hide: ${atHide})`);
  await page.waitForFunction(() => window.hexWorld?.ready() && window.hexWorld.state().play, null, { timeout: 90_000 });
  check(await page.evaluate(() => window.__HEX_BUILD) === NEW, `Reload loads the new build (${await page.evaluate(() => window.__HEX_BUILD)})`);
  await a.until(async () => (await page.evaluate(() => window.hexWorld.state().play.tiles.length)) === 3, "the world returns after Reload with every tile");
  const tilesAfter = await page.evaluate(() => window.hexWorld.state().play.tiles.map((t) => `${t.q},${t.r},${t.tile}`).sort().join(" "));
  check(tilesAfter === tilesBefore, `Reload kept the world, including the tile placed just before it (${tilesAfter})`);
  await page.waitForTimeout(2_500);
  check(!(await a.note.count()), "the new build, now live and loaded, shows no note");
  await ctx.close();
  live = null;

  // ---------- The default interval: a visibility change checks at once ----------
  const ctxB = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const b = await open(ctxB);
  await b.boot(query);
  const lastPoll = polls.at(-1)?.at ?? 0;
  const gap = 15_500 - (Date.now() - lastPoll);
  if (gap > 0) await b.page.waitForTimeout(gap);
  const nb0 = polls.length;
  live = "feedface01";
  await b.page.evaluate(() => document.dispatchEvent(new Event("visibilitychange")));
  await b.until(b.shown, "with the default interval, a visibility change finds the newer build", 5_000);
  check(polls.length - nb0 === 1, `one poll for the visibility change (${polls.length - nb0})`);
  await ctxB.close();
  live = null;

  // ---------- Storage blocked: the note says plainly the world is lost ----------
  const ctxC = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  await ctxC.addInitScript(() => {
    Object.defineProperty(window, "indexedDB", {
      configurable: true,
      get() { throw new DOMException("The operation is insecure.", "SecurityError"); },
    });
  });
  const c = await open(ctxC);
  await c.boot(`${query}&version_poll=1`);
  await c.until(async () => (await c.page.locator(".bar__world").innerText()).includes("Autosave off"), "storage blocked: Autosave off");
  live = "c0ffee0001";
  await c.until(c.shown, "storage blocked: the update note shows", 6_000);
  const ctext = (await c.note.innerText()).replace(/\s+/g, " ");
  check(/Autosave is off, so reloading loses this world/.test(ctext), `storage blocked: the note says the world is unsaved ("${ctext}")`);
  check(await c.note.evaluate((el) => el.classList.contains("toast--caution")), "storage blocked: caution tone");
  await c.page.screenshot({ path: `${out}/update-note-unsaved.jpg`, type: "jpeg", quality: 88 });
  const reloaded = c.page.waitForEvent("load", { timeout: 30_000 });
  await c.note.locator(".toast__action").click();
  await reloaded;
  await c.page.waitForFunction(() => window.hexWorld?.ready(), null, { timeout: 90_000 });
  check(await c.page.evaluate(() => window.__HEX_BUILD) === "c0ffee0001", "storage blocked: Reload loads the new build");
  await ctxC.close();
} catch (e) {
  fail(e?.stack ?? String(e));
} finally {
  await browser.close();
  server.close();
}

check(!errors.length, `no page or console errors (${errors.slice(0, 4).join(" | ")})`);
console.log(failures ? `version e2e FAILED (${failures})` : "version e2e OK");
process.exit(failures ? 1 : 0);
