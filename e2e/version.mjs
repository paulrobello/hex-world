// Local-only new-version e2e (make ui-e2e; owner request 2026-10-15): GPU Chromium on the built site,
// served from memory so the "deploy" never touches dist/. The server can swap version.json and the
// build id inlined in index.html to a new id (a new deploy), or answer version.json with a 503.
// Checks the poll (no-store fetch, a fresh ?t= per request, version_poll), silence offline and on a
// failed request, the update note's place (clear of the bar and the sheet, hidden in photo mode),
// that Reload writes a just-placed tile and loads the new build, that a visibility change checks at
// the default interval, and the plain unsaved wording with storage blocked. Captures go to
// e2e/test-results/version/ (VERSION_OUT=docs/checkpoints/version-check refreshes the committed ones).
// Every phase waits on the page's own check outcomes (the `hexversioncheck` event, version-check.ts),
// never on a wall-clock window, and the deploy changes only while no check is in flight, so a loaded
// host (late or bunched interval ticks) cannot let a poll land in the wrong phase.
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
    polls.push({ t: Number(url.searchParams.get("t")), search: url.search, cache: req.headers["cache-control"] ?? "" });
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

/** Big enough for a host at load 400+; a pass never waits for them. */
const BOOT_MS = 240_000;
const WAIT_MS = 90_000;

async function open(context) {
  // Every version check's outcome, per document (reset by a reload).
  await context.addInitScript(() => {
    window.__versionChecks = [];
    addEventListener("hexversioncheck", (e) => window.__versionChecks.push(e.detail));
  });
  const page = await context.newPage();
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    const t = m.text();
    if (/Failed to load resource/.test(t) && /version\.json|503/.test(`${t} ${m.location().url}`)) netLog.push(t);
    else if (m.type() === "error" || BAD_TEXT.test(t)) errors.push(t);
  });
  const until = async (fn, what, ms = WAIT_MS) => {
    const deadline = Date.now() + ms;
    for (;;) {
      const v = await fn();
      if (v) return v;
      if (Date.now() > deadline) { fail(`timed out: ${what}`); return v; }
      await page.waitForTimeout(50);
    }
  };
  const boot = async (q) => {
    await page.goto(origin + q, { timeout: BOOT_MS });
    await page.waitForFunction(() => window.hexWorld?.ready() && window.hexWorld.state().play, null, { timeout: BOOT_MS });
    await until(async () => !(await page.locator(".load").count()), "loading layer gone");
  };
  const note = page.locator(".hud > .toast--update");
  const shown = async () => (await note.count()) > 0 && (await note.evaluate((el) => el.classList.contains("is-shown") && getComputedStyle(el).opacity === "1"));
  /** How many checks this document has finished. */
  const checks = () => page.evaluate(() => window.__versionChecks.length);
  /** Waits until `n` checks after the first `from` ended with `outcome`; returns the outcomes since `from`. */
  const outcomes = async (from, outcome, n, what) => {
    try {
      await page.waitForFunction(([f, o, k]) => window.__versionChecks.slice(f).filter((x) => x === o).length >= k,
        [from, outcome, n], { timeout: WAIT_MS, polling: 50 });
    } catch { fail(`timed out: ${what} (${n} "${outcome}" checks)`); }
    return page.evaluate((f) => window.__versionChecks.slice(f), from);
  };
  return { page, until, boot, note, shown, checks, outcomes };
}

const overlap = (a, b) => a && b && a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;

try {
  // ---------- Storage on: poll, silence, the note, photo mode, Reload ----------
  const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const a = await open(ctx);
  const { page } = a;
  await a.boot(`${query}&version_poll=1`);
  check(await page.evaluate(() => window.__HEX_BUILD) === built, `the page carries build ${built}`);
  // No wall clock decides "about once a second" (pollMs is unit-tested); three timed checks must fetch.
  const n0 = polls.length;
  const k0 = await a.checks();
  const o0 = await a.outcomes(k0, "same", 3, "version_poll=1 polls on its interval");
  const first = polls.slice(n0);
  check(first.length >= 3, `version_poll=1 polls on its interval (${first.length} requests; outcomes ${o0.join(" ")})`);
  check(new Set(first.map((p) => p.search)).size === first.length && first.every((p) => /^\?t=\d+$/.test(p.search)),
    `each poll has its own ?t= query (${first.map((p) => p.search).slice(0, 3).join(" ")})`);
  check(first.every((p) => /no-cache|no-store/.test(p.cache)), `polls bypass the HTTP cache (cache-control "${first[0]?.cache}")`);
  check(!(await a.note.count()), "the same build shows no update note");

  // Offline. An "offline" outcome means no check is in flight (a check in flight answers "busy"), so the
  // deploy changes only after one, and changes back before the page goes online again.
  await ctx.setOffline(true);
  check(await page.evaluate(() => navigator.onLine) === false, "offline emulated");
  const offAt = await page.evaluate(() => Date.now());
  await a.outcomes(await a.checks(), "offline", 1, "an offline check");
  const n1 = polls.length;
  live = "beefbeef01";
  const o1 = await a.outcomes(await a.checks(), "offline", 2, "two more offline checks");
  live = null;
  const offPolls = polls.slice(n1).filter((p) => p.t >= offAt);
  check(!offPolls.length, `offline: no poll requests (${offPolls.length}; outcomes ${o1.join(" ")})`);
  check(!(await a.note.count()), "offline: no update note");
  await ctx.setOffline(false);
  check(await page.evaluate(() => navigator.onLine) === true, "back online");

  live = "fail";
  const n2 = polls.length;
  const e2 = errors.length;
  const o2 = await a.outcomes(await a.checks(), "bad-status", 2, "two checks answered 503");
  check(polls.length - n2 >= 2, `a 503 is retried (${polls.length - n2} requests; outcomes ${o2.join(" ")})`);
  check(errors.length === e2, `a 503 raises no page error or console error from page code (${errors.slice(e2).join(" | ")})`);
  check(!(await a.note.count()), "a 503 shows no update note");
  console.log(`503 phase: Chromium's own network log lines: ${netLog.length ? netLog.slice(0, 2).join(" | ") : "none"}`);

  // A tile placed just before the deploy, and another just before Reload (its autosave still pending).
  await page.evaluate(() => { const h = window.hexWorld; h.selectTile("meadow"); h.placeTile(1, 0); });
  await a.until(async () => (await page.evaluate(() => window.hexWorld.state().play.tiles.length)) === 2, "a tile placed");
  const NEW = "0123abcdef";
  const k3 = await a.checks();
  live = NEW;
  await a.outcomes(k3, "newer", 1, "a check finds the newer build");
  await a.until(a.shown, "a newer build shows the update note");
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
  // The click happens in the page the moment the third tile's autosave turns pending (a MutationObserver
  // runs before the 600 ms debounce timer can), so a slow host cannot let the autosave finish first.
  await a.until(async () => (await page.locator(".save-state").evaluate((el) => el.dataset.state)) === "saved", "the earlier autosaves written");
  const loaded = page.waitForEvent("load", { timeout: BOOT_MS });
  await page.evaluate(() => {
    const state = document.querySelector(".save-state");
    const reload = document.querySelector(".hud > .toast--update .toast__action");
    const seen = new MutationObserver(() => {
      if (state.dataset.state !== "saving") return;
      seen.disconnect();
      sessionStorage.setItem("saveAtClick", state.dataset.state);
      sessionStorage.setItem("tilesAtClick",
        window.hexWorld.state().play.tiles.map((t) => `${t.q},${t.r},${t.tile}`).sort().join(" "));
      reload.click();
    });
    seen.observe(state, { attributes: true, attributeFilter: ["data-state"] });
    const h = window.hexWorld;
    h.selectTile("forest");
    h.placeTile(-1, 0);
  });
  await loaded;
  const atClick = await page.evaluate(() => sessionStorage.getItem("saveAtClick"));
  check(atClick === "saving", `Reload is clicked while the third tile's autosave is pending (${atClick})`);
  const tilesBefore = await page.evaluate(() => sessionStorage.getItem("tilesAtClick"));
  check(tilesBefore?.split(" ").length === 3, `three tiles when Reload was clicked (${tilesBefore})`);
  const atHide = await page.evaluate(() => sessionStorage.getItem("saveAtHide"));
  check(atHide === "saved", `Reload wrote the world before the page went away (save state at page hide: ${atHide})`);
  await page.waitForFunction(() => window.hexWorld?.ready() && window.hexWorld.state().play, null, { timeout: BOOT_MS });
  check(await page.evaluate(() => window.__HEX_BUILD) === NEW, `Reload loads the new build (${await page.evaluate(() => window.__HEX_BUILD)})`);
  await a.until(async () => (await page.evaluate(() => window.hexWorld.state().play.tiles.length)) === 3, "the world returns after Reload with every tile");
  const tilesAfter = await page.evaluate(() => window.hexWorld.state().play.tiles.map((t) => `${t.q},${t.r},${t.tile}`).sort().join(" "));
  check(tilesAfter === tilesBefore, `Reload kept the world, including the tile placed just before it (${tilesAfter})`);
  await a.outcomes(0, "same", 2, "the reloaded page checks the live build");
  check(!(await a.note.count()), "the new build, now live and loaded, shows no note");
  await ctx.close();
  live = null;

  // ---------- The default interval: a visibility change checks at once ----------
  // A fresh page has made no check, so its first visibility check is never throttled; the 180 s
  // interval cannot fire in this phase's time on any host the boot deadline allows.
  const ctxB = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const b = await open(ctxB);
  await b.boot(query);
  const before = await b.page.evaluate(() => window.__versionChecks.filter((o) => o !== "busy" && o !== "throttled"));
  check(!before.length, `the default interval makes no check during boot (${before.join(" ")})`);
  const nb0 = polls.length;
  const kb = await b.checks();
  live = "feedface01";
  await b.page.evaluate(() => document.dispatchEvent(new Event("visibilitychange")));
  const ob = await b.outcomes(kb, "newer", 1, "the visibility check");
  await b.until(b.shown, "with the default interval, a visibility change finds the newer build");
  check(polls.length - nb0 === 1, `one poll for the visibility change (${polls.length - nb0}; outcomes ${ob.join(" ")})`);
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
  const kc = await c.checks();
  live = "c0ffee0001";
  await c.outcomes(kc, "newer", 1, "storage blocked: a check finds the newer build");
  await c.until(c.shown, "storage blocked: the update note shows");
  const ctext = (await c.note.innerText()).replace(/\s+/g, " ");
  check(/Autosave is off, so reloading loses this world/.test(ctext), `storage blocked: the note says the world is unsaved ("${ctext}")`);
  check(await c.note.evaluate((el) => el.classList.contains("toast--caution")), "storage blocked: caution tone");
  await c.page.screenshot({ path: `${out}/update-note-unsaved.jpg`, type: "jpeg", quality: 88 });
  const reloaded = c.page.waitForEvent("load", { timeout: BOOT_MS });
  await c.note.locator(".toast__action").click();
  await reloaded;
  await c.page.waitForFunction(() => window.hexWorld?.ready(), null, { timeout: BOOT_MS });
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
