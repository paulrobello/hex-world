// Local-only HUD motion e2e (make ui-e2e), UI card criterion 3: every state change has short eased
// feedback that never delays input, and reduced motion is respected.
// 1. Input never waits on feedback: five R presses in one task send five rotate commands at once and
//    land as five ghostRotated events; Tab twice mid-slide flips the sheet twice; B twice toggles twice.
// 2. Reduced motion, from the OS media query and from the engine flag alone (set_reduced_motion):
//    the root carries is-reduced-motion and no HUD animation or transition runs; with it off they do.
// 3. Frame strips of the key transitions (animations paused and stepped through 0..100%), written to
//    MOTION_OUT (default e2e/test-results/ui-motion; docs/checkpoints/ui-feedback for the owner).
import { chromium } from "playwright";
import { createServer } from "node:http";
import { mkdirSync, readFileSync } from "node:fs";
import { extname, join } from "node:path";
import { BAD_TEXT } from "./console-errors.mjs";

const dist = process.argv[2] ?? "dist";
const out = process.env.MOTION_OUT ?? "e2e/test-results/ui-motion";
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

const play = () => page.evaluate(() => window.hexWorld.state().play);
const text = (sel) => page.locator(sel).first().innerText().catch(() => "");
const events = () => page.evaluate(() => window.hexWorld.events());
async function until(fn, what, ms = 10_000) {
  const deadline = Date.now() + ms;
  for (;;) {
    const v = await fn();
    if (v) return v;
    if (Date.now() > deadline) { fail(`timed out: ${what}`); return v; }
    await page.waitForTimeout(50);
  }
}
async function pointAt(q, r) {
  const b = await page.evaluate(([q, r]) => window.hexWorld.cellRect(q, r), [q, r]);
  await page.mouse.move((b[0] + b[2]) / 2, (b[1] + b[3]) / 2, { steps: 4 });
  await until(async () => { const h = (await play()).hovered; return h && h.q === q && h.r === r; }, `pointer hovers (${q}, ${r})`);
}
const frames = (n = 3) => page.evaluate((n) => new Promise((done) => {
  const step = (i) => (i ? requestAnimationFrame(() => step(i - 1)) : done());
  step(n);
}), n);

/** Presses keys in one task (no awaits between them) and reports the commands each sent, synchronously. */
const burst = (keys) => page.evaluate((keys) => {
  const hw = window.hexWorld;
  const orig = hw.cmd;
  const sent = [];
  const sheetClosed = [];
  hw.cmd = function (c) {
    for (const x of Array.isArray(c) ? c : [c]) sent.push(x.cmd);
    return orig.call(this, c);
  };
  const t0 = performance.now();
  try {
    for (const [key, code] of keys) {
      document.body.dispatchEvent(new KeyboardEvent("keydown", { key, code, bubbles: true, cancelable: true }));
      document.body.dispatchEvent(new KeyboardEvent("keyup", { key, code, bubbles: true, cancelable: true }));
      sheetClosed.push(document.querySelector(".sheet--tiles").classList.contains("is-closed"));
    }
  } finally {
    hw.cmd = orig;
  }
  return { sent, sheetClosed, ms: performance.now() - t0 };
}, keys);

/** HUD animations and transitions running now (CSS and Web Animations), by target. */
const running = (scope = "", action = "") => page.evaluate(([scope, action]) => {
  // The action runs in the same task, so a transition it starts is counted before any frame can end it.
  if (action) (0, eval)(action);
  return document.getAnimations()
    .filter((a) => a.playState === "running" && (!scope || a.effect?.target?.closest?.(scope)))
    .map((a) => `${a.effect.target.className?.baseVal ?? a.effect.target.className} ${a.transitionProperty ?? a.animationName ?? "waapi"}`);
}, [scope, action]);

async function inputNeverWaits(label) {
  // Road straight on the grass slot east of the meadow: two distinct orientations, so R alternates.
  await pointAt(1, 0);
  await page.evaluate(() => window.hexWorld.cmd({ cmd: "select_tile", id: "road_straight" }));
  await until(async () => /Rotation \d of 2/.test(await text(".ghost-tag")), `${label}: road straight shows two rotations`);
  await events();
  const rot0 = (await play()).rotation;
  const r = await burst(Array(5).fill(["r", "KeyR"]));
  check(r.sent.filter((c) => c === "rotate").length === 5, `${label}: five R presses in one task send five rotate commands at once (${r.sent.join(",")}; ${r.ms.toFixed(1)} ms)`);
  let rotated = [];
  await until(async () => (rotated.push(...(await events()).filter((e) => e.type === "ghostRotated")), rotated.length >= 5), `${label}: five ghostRotated events`, 5_000);
  check(rotated.length === 5, `${label}: every press lands, none queued behind an animation (${rotated.length} ghostRotated)`);
  check((await play()).rotation !== rot0, `${label}: an odd number of presses leaves the other orientation`);

  // Tab twice inside the 240 ms slide: the sheet closes and reopens on the presses, not after the slide.
  const t = await burst([["Tab", "Tab"], ["Tab", "Tab"]]);
  check(t.sheetClosed.join() === "true,false", `${label}: Tab, Tab mid-slide flips the sheet on each press (${t.sheetClosed.join()})`);
  await until(async () => (await play()).inventory_open === true, `${label}: the engine ends with the sheet open`);

  // B twice: two toggles sent at once, back on the place tool.
  const b = await burst([["b", "KeyB"], ["b", "KeyB"]]);
  check(b.sent.filter((c) => c === "toggle").length === 2, `${label}: B, B sends two bulldoze toggles at once (${b.sent.join(",")})`);
  await until(async () => (await play()).tool === "place", `${label}: B, B ends on the place tool`);
}

/** Frame times as fractions of each animation's duration; the one curve front-loads the motion. */
const STEPS = [0, 0.1, 0.25, 0.5, 1];

/** Pauses every animation the action starts and shoots it at each of STEPS, side by side. */
async function strip(name, action, clip, caption) {
  await page.evaluate(action);
  await frames(2);
  const n = await page.evaluate(() => {
    const all = document.getAnimations().filter((a) => a.effect?.getComputedTiming().iterations !== Infinity);
    window.__strip = all;
    for (const a of all) a.pause();
    return all.length;
  });
  const shots = [];
  for (const f of STEPS) {
    await page.evaluate((f) => {
      for (const a of window.__strip) {
        const end = a.effect.getComputedTiming().endTime;
        a.currentTime = Math.min(end - 0.01, end * f);
      }
    }, f);
    await frames(1);
    shots.push((await page.screenshot({ clip, type: "jpeg", quality: 88 })).toString("base64"));
  }
  await page.evaluate(() => { for (const a of window.__strip ?? []) a.finish(); window.__strip = null; });
  const sheet = await browser.newPage({ viewport: { width: 1600, height: 200 } });
  const w = Math.floor(1600 / 5);
  const h = Math.round((clip.height * w) / clip.width);
  await sheet.setViewportSize({ width: 1600, height: h + 40 });
  await sheet.setContent(`<body style="margin:0;background:#0e0f10;color:#eef3f6;font:600 13px system-ui">
    <div style="display:flex">${shots.map((s, i) => `<figure style="margin:0;width:${w}px"><img src="data:image/jpeg;base64,${s}" style="display:block;width:${w}px;height:${h}px"><figcaption style="padding:4px 8px">${Math.round(STEPS[i] * 100)}% of the time</figcaption></figure>`).join("")}</div>
    <div style="position:absolute;right:8px;bottom:4px">${caption}</div></body>`);
  await sheet.screenshot({ path: `${out}/${name}.jpg`, type: "jpeg", quality: 88 });
  await sheet.close();
  console.log(`strip: ${out}/${name}.jpg (${n} animations)`);
  return n;
}

const keydown = (key, code) => `document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "${key}", code: "${code}", bubbles: true, cancelable: true }))`;

try {
  await page.goto(base + "?scenario=play&tilt=off&fps=off", { waitUntil: "commit" });
  await page.waitForFunction(() => window.hexWorld?.ready(), null, { timeout: 90_000 });
  await until(async () => !(await page.locator(".load").count()), "loading layer removed after ready", 5_000);
  await until(async () => (await play())?.inventory_open === true, "Tiles sheet open");
  await page.focus("canvas#hex-world");

  // ---------- 1. Input never waits on feedback ----------
  check(!(await page.evaluate(() => document.documentElement.classList.contains("is-reduced-motion"))), "full motion: the root has no is-reduced-motion");
  await inputNeverWaits("full motion");

  // ---------- 3. Frame strips (full motion) ----------
  const right = { x: 1040, y: 0, width: 400, height: 900 };
  const barClip = { x: 0, y: 0, width: 640, height: 84 };
  check(await strip("sheet-close", keydown("Tab", "Tab"), right, "Tab: the sheet slides off the right edge, 240 ms") > 0, "sheet close animates under full motion");
  check(await strip("sheet-open", keydown("Tab", "Tab"), right, "Tab: the sheet slides back, 240 ms") > 0, "sheet open animates under full motion");
  check(await strip("rotate-tool", keydown("r", "KeyR"), barClip, "R: the Rotate tool nudges -14° and rings, 180 ms") > 0, "rotate fires the tool");
  const row = await page.locator(".row[aria-pressed=true]").boundingBox();
  const rowClip = { x: Math.round(row.x) - 8, y: Math.round(row.y) - 8, width: Math.round(row.width) + 16, height: Math.round(row.height) + 16 };
  check(await strip("rotate-row", keydown("r", "KeyR"), rowClip, "R: the selected row's socket code turns 60°, 180 ms") > 0, "rotate turns the selected row's socket code");
  // A placement, then undo: the margin note rises 8 px and fades in.
  const ghost = (await play()).ghost;
  await page.evaluate((g) => window.hexWorld.cmd({ cmd: "place", q: g.q, r: g.r }), ghost);
  await until(async () => (await play()).can_undo, "a tile placed for the undo note");
  await page.waitForTimeout(400);
  check(await strip("undo-note", `${keydown("z", "KeyZ").replace("bubbles", "ctrlKey: true, metaKey: true, bubbles")}`, { x: 0, y: 790, width: 600, height: 110 }, "⌘Z: the undo note rises 8 px and fades in, 180 ms") > 0, "the undo note animates in");
  check(/Undid/.test(await text(".toast.is-shown")), "the undo note reads Undid");
  check(await strip("bulldoze-on", keydown("b", "KeyB"), barClip, "B: bulldoze fills red, the tool nudges and rings") > 0, "bulldoze toggles with feedback");
  await page.evaluate(keydown("b", "KeyB"));
  await until(async () => (await play()).tool === "place", "back to place");
  check(await strip("walk-hint", keydown("f", "KeyF"), { x: 370, y: 790, width: 700, height: 110 }, "F: the walk hint rises 8 px and fades in, 180 ms (the toast's fade)") > 0, "the walk hint fades in");
  await until(async () => (await play()).walk_mode, "walk mode on");
  await page.keyboard.press("Escape");
  await until(async () => !(await play()).walk_mode, "Esc leaves walk mode");
  await page.waitForTimeout(400);
  await page.click(".tool[data-action=settings]");
  await page.waitForTimeout(400);
  check(await strip("settings-tab", `document.querySelector("#settings-tab-audio").click()`, { x: 1000, y: 0, width: 440, height: 600 }, "Settings tab: the panel rises 4 px and fades in, 120 ms") > 0, "a settings tab switch animates its panel");
  await page.evaluate(keydown("Escape", "Escape"));
  await until(async () => page.locator(".sheet--settings.is-closed").count(), "Settings closed");
  await page.mouse.move(200, 450);
  await page.waitForTimeout(300);

  // ---------- 2. Reduced motion: the OS media query ----------
  await page.emulateMedia({ reducedMotion: "reduce" });
  await until(async () => page.evaluate(() => document.documentElement.classList.contains("is-reduced-motion")), "OS reduce: the root carries is-reduced-motion");
  await page.waitForTimeout(300);
  await inputNeverWaits("OS reduce");
  for (const [what, action] of [["Tab", keydown("Tab", "Tab")], ["Tab again", keydown("Tab", "Tab")], ["R", keydown("r", "KeyR")], ["B", keydown("b", "KeyB")], ["B again", keydown("b", "KeyB")]]) {
    const now = await running("", action);
    await frames(2);
    const a = [...now, ...(await running())];
    check(a.length === 0, `OS reduce: ${what} runs no HUD animation, at the press or after it (${a.join("; ")})`);
  }
  check(await strip("sheet-close-reduced", keydown("Tab", "Tab"), right, "Reduced motion: Tab cuts the sheet away at once") === 0, "OS reduce: the sheet close is an instant cut");
  await page.evaluate(keydown("Tab", "Tab"));
  // The photo focus ring shows only through its animation: under reduced motion it keeps a still, fading ring.
  const mark = await page.evaluate(async () => {
    const m = document.createElement("div");
    m.className = "focus-mark is-shown";
    const host = document.createElement("div");
    host.className = "hud";
    host.append(m);
    document.body.append(host);
    await new Promise((r) => setTimeout(r, 200));
    const cs = getComputedStyle(m);
    const res = { name: cs.animationName, opacity: Number(cs.opacity) };
    host.remove();
    return res;
  });
  check(mark.name === "focus-mark-still" && mark.opacity > 0.9, `OS reduce: the photo focus ring still shows, as a still ring (${mark.name}, opacity ${mark.opacity})`);

  // ---------- 2b. Reduced motion: the engine flag alone, OS at no-preference ----------
  await page.emulateMedia({ reducedMotion: "no-preference" });
  await until(async () => (await play()).reduced_motion === false, "OS back to no-preference");
  await until(async () => !(await page.evaluate(() => document.documentElement.classList.contains("is-reduced-motion"))), "the root drops is-reduced-motion");
  await page.evaluate(() => window.hexWorld.cmd({ cmd: "set_reduced_motion", on: true }));
  await until(async () => (await play()).reduced_motion === true, "engine flag on");
  await until(async () => page.evaluate(() => document.documentElement.classList.contains("is-reduced-motion")), "engine flag: the root carries is-reduced-motion");
  check(!(await page.evaluate(() => matchMedia("(prefers-reduced-motion: reduce)").matches)), "engine flag: the OS query is not set");
  await inputNeverWaits("engine flag");
  for (const [what, action] of [["Tab", keydown("Tab", "Tab")], ["Tab again", keydown("Tab", "Tab")], ["R", keydown("r", "KeyR")]]) {
    const now = await running("", action);
    await frames(2);
    const a = [...now, ...(await running())];
    check(a.length === 0, `engine flag: ${what} runs no HUD animation, at the press or after it (${a.join("; ")})`);
  }
  // Re-ranking the sheet (a new slot) uses Web Animations, which the CSS cut does not reach.
  await pointAt(0, 1);
  await frames(2);
  check((await running(".sheet--tiles")).length === 0, "engine flag: a new slot re-ranks the sheet without motion");

  // And off again: the positive control, so the zero counts above are not vacuous.
  await page.evaluate(() => window.hexWorld.cmd({ cmd: "set_reduced_motion", on: false }));
  await until(async () => !(await page.evaluate(() => document.documentElement.classList.contains("is-reduced-motion"))), "engine flag off: the root drops is-reduced-motion");
  const back = await running(".sheet--tiles", keydown("Tab", "Tab"));
  check(back.length > 0, `full motion again: Tab slides the sheet (${back.length} running)`);
  await page.evaluate(keydown("Tab", "Tab"));
} catch (e) {
  fail(String(e?.stack ?? e));
} finally {
  if (errors.length) fail("console errors: " + errors.join(" | "));
  await browser.close();
  server.close();
}
console.log(failures ? `ui motion e2e FAILED (${failures})` : `ui motion e2e OK: strips in ${out}/`);
process.exitCode = failures ? 1 : 0;
