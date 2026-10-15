// Local-only photo-effects e2e (make ui-e2e; owner, 2026-10-14): GPU Chromium on the built site.
// Sets each effect from the photo panel and checks state().photo follows, exports PNGs through the
// offscreen target and measures them in the page (black and white has no saturation, the vignette
// darkens the corners against the centre), leaves photo mode (the pass is off the game camera, the
// canvas has colour again) and re-enters (the settings and the panel are kept). Captures go to
// e2e/test-results/photo-fx/.
import { chromium } from "playwright";
import { createServer } from "node:http";
import { mkdirSync, readFileSync } from "node:fs";
import { extname, join } from "node:path";
import { BAD_TEXT } from "./console-errors.mjs";

const dist = process.argv[2] ?? "dist";
const out = "e2e/test-results/photo-fx";
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
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
const errors = [];
page.on("pageerror", (e) => errors.push(String(e)));
page.on("console", (m) => { if (m.type() === "error" || BAD_TEXT.test(m.text())) errors.push(m.text()); });

const st = () => page.evaluate(() => window.hexWorld.state());
const cmd = (c) => page.evaluate((c) => window.hexWorld.cmd(c), c);
async function until(fn, what, ms = 10_000) {
  const deadline = Date.now() + ms;
  for (;;) {
    const v = await fn();
    if (v) return v;
    if (Date.now() > deadline) { fail(`timed out: ${what}`); return v; }
    await page.waitForTimeout(50);
  }
}
const panel = ".sheet--photo .photo-fx";
async function setSlider(label, pct) {
  await page.locator(`${panel} input[aria-label='${label}']`).evaluate((el, v) => {
    el.value = String(v);
    el.dispatchEvent(new Event("input", { bubbles: true }));
  }, pct);
}

/** Mean saturation (HSL), and the mean of a corner box over the centre box, of a PNG data URL. */
const stats = (url) => page.evaluate(async (url) => {
  const img = new Image();
  img.src = url;
  await img.decode();
  const c = document.createElement("canvas");
  c.width = img.width;
  c.height = img.height;
  const g = c.getContext("2d", { willReadFrequently: true });
  g.drawImage(img, 0, 0);
  const box = (x, y) => {
    const w = Math.round(img.width * 0.12), h = Math.round(img.height * 0.12);
    const d = g.getImageData(x, y, w, h).data;
    let s = 0;
    for (let i = 0; i < d.length; i += 4) s += (d[i] + d[i + 1] + d[i + 2]) / 3;
    return s / (d.length / 4);
  };
  const d = g.getImageData(0, 0, img.width, img.height).data;
  let sat = 0;
  for (let i = 0; i < d.length; i += 4) {
    const mx = Math.max(d[i], d[i + 1], d[i + 2]) / 255, mn = Math.min(d[i], d[i + 1], d[i + 2]) / 255;
    const l = (mx + mn) / 2;
    sat += mx === mn ? 0 : (mx - mn) / (1 - Math.abs(2 * l - 1));
  }
  const cx = Math.round(img.width * 0.44), cy = Math.round(img.height * 0.44);
  return { width: img.width, height: img.height, sat: sat / (d.length / 4), cornerOverCentre: box(0, 0) / box(cx, cy) };
}, url);

/** Exports through the engine (the panel ignores names it did not start) and measures the PNG. */
async function exportStats(name, scale = 1) {
  await cmd({ cmd: "photo_export", scale, name });
  const url = await until(() => page.evaluate((n) => window.hexWorld.takeShot(n), name), `export ${name} arrives`, 30_000);
  if (!url) return null;
  const s = await stats(url);
  console.log(`export ${name}: ${JSON.stringify(s)}`);
  return s;
}
/** A box of the canvas clear of the HUD and the photo panel (both have colour of their own). */
const canvasStats = async () =>
  stats(`data:image/png;base64,${(await page.screenshot({ clip: { x: 240, y: 240, width: 480, height: 300 } })).toString("base64")}`);

try {
  await page.goto(base + "?scenario=play&tilt=off&fps=off");
  await page.waitForFunction(() => window.hexWorld?.ready() && window.hexWorld.state().play, null, { timeout: 90_000 });
  await page.waitForTimeout(3000);
  // A close view of the start tiles.
  await cmd({ cmd: "camera", pos: [0, 9, 16], look_at: [0, 0, 0] });
  await page.waitForTimeout(500);
  await page.focus("canvas#hex-world");
  await page.keyboard.press("p");
  await until(async () => { const s = await st(); return s.play.photo_mode && s.photo.on; }, "P enters photo mode");
  await cmd({ cmd: "photo", dof: "off" });

  let s = await st();
  check(s.photo.filter === "normal" && s.photo.vignette === 0 && s.photo.fx_active === false,
    "effects start off and the pass is off");
  check(await page.locator(`${panel} .fx-reset`).isDisabled(), "Reset effects is off at the defaults");
  check(await page.locator(`${panel} input[aria-label='Filter strength']`).isDisabled(), "strength is inert without a filter");

  // Each control from the panel reaches the engine.
  await page.locator(`${panel} .seg[aria-label='Colour filter'] button[data-value=mono]`).click();
  await until(async () => { const p = (await st()).photo; return p.filter === "mono" && p.fx_active; }, "B&W reaches the engine and the pass runs");
  check(!(await page.locator(`${panel} input[aria-label='Filter strength']`).isDisabled()), "strength comes alive with a filter");
  await setSlider("Filter strength", 70);
  await until(async () => (await st()).photo.filter_strength === 0.7, "filter strength 70% reaches the engine");
  await page.locator(`${panel} .seg[aria-label='Colour filter'] button[data-value=sepia]`).click();
  await until(async () => (await st()).photo.filter === "sepia", "sepia reaches the engine");
  await setSlider("Vignette", 80);
  await until(async () => (await st()).photo.vignette === 0.8, "vignette 80% reaches the engine");
  await setSlider("Vignette softness", 30);
  await until(async () => (await st()).photo.vignette_softness === 0.3, "softness 30% reaches the engine");
  await setSlider("Film grain", 40);
  await until(async () => (await st()).photo.grain === 0.4, "grain 40% reaches the engine");
  await setSlider("Chromatic aberration", 50);
  await until(async () => (await st()).photo.aberration === 0.5, "aberration 50% reaches the engine");
  await page.locator(panel).scrollIntoViewIfNeeded();
  await page.waitForTimeout(300);
  await page.screenshot({ path: `${out}/photo-fx-panel.jpg`, type: "jpeg", quality: 88 });

  await page.locator(`${panel} .fx-reset`).click();
  await until(async () => {
    const p = (await st()).photo;
    return p.filter === "normal" && p.filter_strength === 1 && p.vignette === 0 && p.vignette_softness === 0.5 &&
      p.grain === 0 && p.aberration === 0 && p.fx_active === false;
  }, "Reset effects puts every effect back and the pass stops");
  check(await page.locator(`${panel} .fx-reset`).isDisabled(), "Reset effects is off again");
  check((await page.locator(`${panel} input[aria-label='Vignette']`).inputValue()) === "0", "the vignette slider is back at 0");

  // Exports carry the effects.
  await page.waitForTimeout(300);
  const plain = await exportStats("fx-plain");
  await cmd({ cmd: "photo", filter: "mono", filter_strength: 1 });
  await until(async () => (await st()).photo.fx_active, "mono pass on");
  const mono = await exportStats("fx-mono");
  check(plain && mono && plain.sat > 0.08 && mono.sat < 0.01,
    `a black and white export has no colour (saturation ${mono?.sat.toFixed(4)}, ${plain?.sat.toFixed(4)} without)`);
  await cmd({ cmd: "photo", filter: "normal", vignette: 0.9, vignette_softness: 0.5 });
  await until(async () => (await st()).photo.vignette === 0.9, "vignette on");
  const vig = await exportStats("fx-vignette");
  check(plain && vig && vig.cornerOverCentre < 0.6 * plain.cornerOverCentre,
    `a vignette export darkens the corner against the centre (${vig?.cornerOverCentre.toFixed(3)} vs ${plain?.cornerOverCentre.toFixed(3)} without)`);
  const vig2 = await exportStats("fx-vignette-2x", 2);
  check(vig && vig2 && vig2.width === 2 * vig.width && Math.abs(vig2.cornerOverCentre - vig.cornerOverCentre) < 0.25 * vig.cornerOverCentre + 0.02,
    `a 2x export keeps the vignette (${vig2?.cornerOverCentre.toFixed(3)} at ${vig2?.width} px vs ${vig?.cornerOverCentre.toFixed(3)} at ${vig?.width} px)`);

  // Leaving photo mode takes the pass off the game camera; the settings stay.
  await cmd({ cmd: "photo", filter: "mono" });
  await until(async () => (await st()).photo.filter === "mono", "mono again before leaving");
  await page.waitForTimeout(300);
  const inPhoto = await canvasStats();
  await page.focus("canvas#hex-world");
  await page.keyboard.press("p");
  await until(async () => { const p = (await st()).photo; return !p.on && p.fx_active === false; }, "leaving photo mode turns the pass off");
  await page.waitForTimeout(500);
  const game = await canvasStats();
  check(inPhoto.sat < 0.02 && game.sat > 0.08,
    `the game camera has colour again (saturation ${game.sat.toFixed(4)}; ${inPhoto.sat.toFixed(4)} in photo mode with B&W)`);
  s = await st();
  check(s.photo.filter === "mono" && s.photo.vignette === 0.9, "the settings survive leaving photo mode");

  // Re-entering keeps them, and the panel shows them.
  await page.keyboard.press("p");
  await until(async () => { const p = (await st()).photo; return p.on && p.fx_active; }, "re-entering turns the pass back on");
  check((await page.locator(`${panel} .seg[aria-label='Colour filter'] button[data-value=mono]`).getAttribute("aria-checked")) === "true",
    "the panel shows B&W on re-entering");
  check((await page.locator(`${panel} input[aria-label='Vignette']`).inputValue()) === "90", "the panel shows vignette 90% on re-entering");
  await page.waitForTimeout(300);
  await page.screenshot({ path: `${out}/photo-fx-reenter.jpg`, type: "jpeg", quality: 88 });
  await page.keyboard.press("p");
  await until(async () => !(await st()).photo.on, "P leaves again");

  check(errors.length === 0, `no page errors (${errors.slice(0, 3).join(" | ")})`);
} catch (e) {
  fail(String(e?.stack ?? e));
} finally {
  await browser.close();
  server.close();
}
console.log(failures ? `photo-fx e2e: ${failures} failure(s)` : "photo-fx e2e: all ok");
process.exit(failures ? 1 : 0);
